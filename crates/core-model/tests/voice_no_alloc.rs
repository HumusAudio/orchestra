// SPDX-License-Identifier: GPL-3.0-or-later

//! Every lookup of the voices' content, clefs and key signatures is allocation-free (ADR-0007
//! rule 1, ADR-0024): layout, playback compile and the DAW's spelling call them once per event,
//! and none of them needs the heap. Building the document does allocate and is not tested here.
//!
//! This file is its own test binary with the counting allocator installed (ADR-0016 §5). The
//! inputs go through `black_box` so that the compiler cannot evaluate the lookups at compile time.

// Helper functions (not `#[test]` functions) are outside the scope of clippy.toml's
// `allow-unwrap-in-tests`, so the exemption is stated here, for this test file only.
#![allow(
    clippy::unwrap_used,
    reason = "test helpers: a failed unwrap fails the test"
)]

use std::hint::black_box;

use orchestra_core_model::{
    Alteration, BaseValue, Clef, Document, DocumentBuilder, EventId, Interval, Key, Metronome,
    Mode, NoteId, NoteValue, Pitch, Placement, Quality, Step, TempoMark, Tie, Tuplet,
};
use orchestra_core_time::{Duration, Meter, MeterChange, MeterMap, Rational, Tempo};
use orchestra_test_support::assert_no_allocations;

orchestra_test_support::install_counting_allocator!();

/// The duration `n/d` of a whole note.
fn d(n: i64, den: i64) -> Duration {
    Duration::from_rational(Rational::new(n, den).unwrap())
}

/// Eight bars for a B♭ clarinet and a piano: chords, triplets, ties, clef and key changes.
fn document() -> (Document, Vec<EventId>, Vec<NoteId>) {
    let meters = MeterMap::new(MeterChange::new(Meter::new(&[4], 4).unwrap())).unwrap();
    let tempo = Tempo::new(d(1, 4), Rational::from_integer(72).unwrap()).unwrap();
    let mut b = DocumentBuilder::new(
        meters,
        TempoMark::immediate(None, Metronome::shown(tempo)).unwrap(),
        Key::traditional(-2, Mode::Major).unwrap(),
    )
    .unwrap();
    let bars: Vec<_> = (0..8).map(|_| b.add_bar().unwrap()).collect();
    let down_second = Interval::from_quality(Quality::Major, 2, true).unwrap();
    b.add_instrument_track("Clarinet", down_second, &[Clef::TREBLE])
        .unwrap();
    let piano = b
        .add_instrument_track(
            "Piano",
            Interval::new(0, Rational::ZERO),
            &[Clef::TREBLE, Clef::BASS],
        )
        .unwrap();
    let staff = b.document().track(piano).unwrap().instrument().staves()[1].id();
    let voice = b.document().track(piano).unwrap().instrument().voices()[0].id();
    b.add_clef_change(staff, bars[3], d(1, 2), Clef::TREBLE)
        .unwrap();
    b.add_key_change(
        bars[4],
        Duration::ZERO,
        Key::traditional(1, Mode::Major).unwrap(),
    )
    .unwrap();
    let mut events = Vec::new();
    let mut notes = Vec::new();
    let c4 = Pitch::new(Step::C, Alteration::NATURAL, 4);
    let e4 = Pitch::new(Step::E, Alteration::FLAT, 4);
    for &bar in &bars {
        let triplet = b
            .add_tuplet(
                voice,
                Tuplet::new(3, 2, NoteValue::plain(BaseValue::Quarter)).unwrap(),
                None,
            )
            .unwrap();
        for i in 0..3 {
            let at = Placement::new(voice, bar, d(i, 6)).in_tuplet(triplet);
            let ids = b
                .add_chord(at, NoteValue::plain(BaseValue::Quarter), &[c4, e4])
                .unwrap();
            events.push(ids.event);
            notes.extend(ids.notes);
        }
        let half = NoteValue::plain(BaseValue::Half);
        let ids = b
            .add_chord(Placement::new(voice, bar, d(1, 2)), half, &[c4])
            .unwrap();
        events.push(ids.event);
        notes.extend(ids.notes);
    }
    // Tie each bar's last C4 to the next bar's first C4.
    for pair in events.chunks(4).collect::<Vec<_>>().windows(2) {
        let from = b.document().event(pair[0][3]).unwrap();
        let to = b.document().event(pair[1][0]).unwrap();
        let from_note = from.event().chord().unwrap().notes().next().unwrap().id();
        let to_note = to.event().chord().unwrap().notes().next().unwrap().id();
        b.set_tie(from_note, Tie::To(to_note)).unwrap();
    }
    (b.build().unwrap(), events, notes)
}

/// Every lookup, on hits.
#[test]
fn lookups_do_not_allocate() {
    let (document, events, notes) = black_box(document());
    let instruments: Vec<_> = document
        .tracks()
        .iter()
        .map(|t| t.instrument().id())
        .collect();
    let staves: Vec<_> = document
        .tracks()
        .iter()
        .flat_map(|t| {
            t.instrument()
                .staves()
                .iter()
                .map(orchestra_core_model::Staff::id)
        })
        .collect();
    let bars: Vec<_> = document.bars().ids().to_vec();
    let voice = document.tracks()[1].instrument().voices()[0].id();

    assert_no_allocations(|| {
        for &event in &events {
            let found = black_box(document.event(black_box(event))).unwrap();
            black_box(document.metric_duration(event)).unwrap();
            black_box(document.event_position(event)).unwrap();
            let (_, v) = document.voice(found.voice()).unwrap();
            black_box(v.metric_duration(found.event(), d(1, 1))).unwrap();
            for note in found.event().chord().unwrap().notes() {
                black_box(note.pitch());
            }
        }
        for &note in &notes {
            black_box(document.note(black_box(note))).unwrap();
        }
        for &bar in &bars {
            black_box(document.events(voice, bar)).unwrap();
            black_box(document.key_at(bar, d(1, 4))).unwrap();
            for &staff in &staves {
                black_box(document.clef_at(staff, bar, d(3, 4))).unwrap();
            }
            for &instrument in &instruments {
                black_box(document.instrument_key_at(instrument, bar, Duration::ZERO)).unwrap();
                black_box(document.written_key_at(instrument, bar, Duration::ZERO)).unwrap();
            }
        }
    });
}

/// Validating a built document allocates only its voice → instrument map, sized up front: one
/// allocation whatever the number of events; checked here so that a regression to a per-event
/// allocation is caught.
#[test]
fn validate_allocates_a_bounded_amount() {
    let (document, _, _) = document();
    let ((), stats) = orchestra_test_support::count_allocations(|| {
        black_box(document.validate()).unwrap();
    })
    .unwrap();
    assert!(stats.allocations <= 1, "validate allocated: {stats:?}");
    assert_eq!(stats.reallocations, 0, "validate reallocated: {stats:?}");
}
