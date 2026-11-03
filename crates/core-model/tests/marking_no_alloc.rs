// SPDX-License-Identifier: GPL-3.0-or-later

//! Every lookup of the markings, the performance offsets, the slurs and the score order is
//! allocation-free (ADR-0007 rule 1, ADR-0024 §5–§7 and §10): layout calls them once per event it
//! draws, and playback once per note it compiles. Validating a document with graces and slurs
//! still allocates only its voice → instrument map.
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
    Alteration, Articulation, ArticulationSet, BaseValue, Clef, Document, DocumentBuilder, EventId,
    Fermata, Grace, GraceAttach, GraceKind, Interval, Key, Metronome, Mode, NoteId, NoteValue,
    Ornament, PerformanceOffsets, Pitch, Placement, SlurId, SlurPlacement, Step, TempoMark,
};
use orchestra_core_time::{Duration, Meter, MeterChange, MeterMap, Rational, Tempo};
use orchestra_test_support::assert_no_allocations;

orchestra_test_support::install_counting_allocator!();

/// The duration `n/d` of a whole note.
fn d(n: i64, den: i64) -> Duration {
    Duration::from_rational(Rational::new(n, den).unwrap())
}

/// Eight bars of a flute in quarters, each bar opening with an acciaccatura before its first note
/// and closing with an After grace; articulations, an ornament, a fermata and offsets on every
/// note; a slur over each bar, one from each grace to its note, and a phrase over the whole
/// passage (so the first event is touched by three slurs: the "many" case of the index).
fn document() -> (Document, Vec<EventId>, Vec<NoteId>, Vec<SlurId>) {
    let meters = MeterMap::new(MeterChange::new(Meter::new(&[4], 4).unwrap())).unwrap();
    let tempo = Tempo::new(d(1, 4), Rational::from_integer(72).unwrap()).unwrap();
    let key = Key::traditional(0, Mode::Major).unwrap();
    let mut b = DocumentBuilder::new(
        meters,
        TempoMark::immediate(None, Metronome::shown(tempo)).unwrap(),
        key,
    )
    .unwrap();
    let bars: Vec<_> = (0..8).map(|_| b.add_bar().unwrap()).collect();
    let track = b
        .add_instrument_track("Flute", Interval::new(0, Rational::ZERO), &[Clef::TREBLE])
        .unwrap();
    let voice = b.document().track(track).unwrap().instrument().voices()[0].id();
    let pitch = Pitch::new(Step::E, Alteration::NATURAL, 5);
    let quarter = NoteValue::plain(BaseValue::Quarter);
    let eighth = NoteValue::plain(BaseValue::Eighth);
    let accent = ArticulationSet::EMPTY.with(Articulation::Accent);

    let (mut events, mut notes, mut slurs) = (Vec::new(), Vec::new(), Vec::new());
    for &bar in &bars {
        let mut in_bar = Vec::new();
        let grace = |attach| Grace {
            kind: GraceKind::Acciaccatura,
            attach,
        };
        let before = b
            .add_grace_chord(
                Placement::new(voice, bar, Duration::ZERO),
                grace(GraceAttach::Before),
                eighth,
                &[pitch],
            )
            .unwrap();
        for beat in 0..4 {
            let ids = b
                .add_chord(Placement::new(voice, bar, d(beat, 4)), quarter, &[pitch])
                .unwrap();
            b.set_articulations(ids.event, accent).unwrap();
            b.set_ornament(ids.event, Some(Ornament::Mordent)).unwrap();
            b.set_offsets(
                ids.notes[0],
                PerformanceOffsets {
                    onset: d(-1, 128),
                    length: Duration::ZERO,
                    velocity: -512,
                },
            )
            .unwrap();
            in_bar.push(ids.event);
            notes.extend(ids.notes);
        }
        let after = b
            .add_grace_chord(
                Placement::new(voice, bar, d(1, 1)),
                grace(GraceAttach::After),
                eighth,
                &[pitch],
            )
            .unwrap();
        b.set_fermata(in_bar[3], Some(Fermata::Short)).unwrap();
        slurs.push(
            b.add_slur(before.event, in_bar[0], SlurPlacement::Auto)
                .unwrap(),
        );
        slurs.push(
            b.add_slur(in_bar[0], in_bar[3], SlurPlacement::Above)
                .unwrap(),
        );
        events.push(before.event);
        events.extend(in_bar);
        events.push(after.event);
    }
    let first = events[1];
    let last = events[events.len() - 2];
    slurs.push(b.add_slur(first, last, SlurPlacement::Above).unwrap());
    (b.build().unwrap(), events, notes, slurs)
}

/// Every lookup, on hits.
#[test]
fn lookups_do_not_allocate() {
    let (document, events, notes, slurs) = black_box(document());
    // The "many" entry of the index exists, so its slice is looked up too.
    assert_eq!(document.slurs_at(events[1]).len(), 3);
    assert_no_allocations(|| {
        for &event in &events {
            let found = black_box(document.event(black_box(event))).unwrap();
            let e = found.event();
            black_box(e.grace());
            black_box(e.fermata());
            if let Some(chord) = e.chord() {
                black_box(chord.articulations().iter().count());
                black_box(chord.ornament());
                black_box(chord.arpeggio());
            }
            black_box(document.slurs_at(black_box(event)));
            black_box(document.metric_duration(event)).unwrap();
        }
        for pair in events.windows(2) {
            black_box(document.comes_before(black_box(pair[0]), black_box(pair[1]))).unwrap();
        }
        for &note in &notes {
            black_box(document.note(black_box(note)).unwrap().1.offsets());
        }
        for &slur in &slurs {
            black_box(document.slur(black_box(slur))).unwrap();
        }
    });
}

/// Validating a document with graces and slurs allocates only its voice → instrument map, as
/// before F1.09c: the grace and slur rules add no allocation.
#[test]
fn validate_allocates_a_bounded_amount() {
    let (document, _, _, _) = document();
    let ((), stats) = orchestra_test_support::count_allocations(|| {
        black_box(document.validate()).unwrap();
    })
    .unwrap();
    assert!(stats.allocations <= 1, "validate allocated: {stats:?}");
    assert_eq!(stats.reallocations, 0, "validate reallocated: {stats:?}");
}
