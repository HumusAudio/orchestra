// SPDX-License-Identifier: GPL-3.0-or-later

//! Hand-written cases of the saved-document description (`crate::parts`, ADR-0029 S4 and S8;
//! owner's choices ADR-0029 L4–L8 of 2026-11-06).
//!
//! The property tests of the commands check the round trip after every command
//! (`editor_tests::assert_round_trip`); these cases add what random commands never make (every
//! ornament, fermatas on rests, arpeggios, nested tuplets, extra voices, an exhausted counter) and
//! one case per refusal of [`Document::from_parts`]: duplicate and unissued IDs, an unknown bar,
//! lists out of order, an invalid document, a meter change past the last bar, too many tracks.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test helpers: a failed unwrap or index fails the test"
)]
#![allow(
    clippy::too_many_lines,
    reason = "the markings document is one long list of small, independent parts"
)]

use std::num::NonZeroU64;

use orchestra_core_time::{BarIndex, Duration, Meter, MeterChange, MeterMap, Rational, Tempo};

use crate::document::{Document, DocumentBuilder, DocumentError, MAX_TRACKS, Placement};
use crate::editor_tests::{assert_round_trip, rich};
use crate::event::{BaseValue, Event, EventBody, NoteValue, PlacedEvent, RestValue, Tie, Tuplet};
use crate::form::BarNumbering;
use crate::id::{BarId, EventId};
use crate::marking::{
    Arpeggio, Articulation, ArticulationSet, Fermata, Ornament, PerformanceOffsets, TremoloStrokes,
};
use crate::marks::{Clef, KeyOverride, KeySignature};
use crate::pitch::{Alteration, Interval, Key, Mode, Pitch, Step};
use crate::tempo_mark::{Metronome, TempoMark};
use crate::tolerance::Tolerance;
use crate::validate::Violation;

/// Shorthand: the duration `n/den` of a whole note.
fn d(n: i64, den: i64) -> Duration {
    Duration::from_rational(Rational::new(n, den).unwrap())
}

/// A document with what the property tests' rich document lacks: every ornament, fermatas on a
/// chord and on a rest, an arpeggio, articulations, a let-ring tie and a forward tie, performance
/// offsets, a nested tuplet, a second voice on the lower staff, a key override, a per-track
/// tolerance, an uncounted bar and a numbering restart, and a non-default numbering style.
fn markings() -> Document {
    let mut meters = MeterMap::new(MeterChange::new(Meter::new(&[4], 4).unwrap())).unwrap();
    meters.set_numbering(
        BarIndex::new(0),
        orchestra_core_time::BarCounting::Uncounted,
    );
    meters.set_numbering_restart(BarIndex::new(2), Some(10));
    let tempo = Tempo::new(d(1, 4), Rational::from_integer(90).unwrap()).unwrap();
    let initial = TempoMark::immediate(None, Metronome::shown(tempo)).unwrap();
    let mut b =
        DocumentBuilder::new(meters, initial, Key::traditional(-2, Mode::Minor).unwrap()).unwrap();
    let bars: Vec<BarId> = (0..4).map(|_| b.add_bar().unwrap()).collect();
    let unison = Interval::new(0, Rational::ZERO);
    let horn = b
        .add_instrument_track(
            "Horn in F",
            Interval::new(-4, Rational::from_integer(-7).unwrap()),
            &[Clef::TREBLE, Clef::BASS],
        )
        .unwrap();
    let other = b
        .add_instrument_track("Viola", unison, &[Clef::ALTO])
        .unwrap();
    let (horn_id, upper, lower_staff) = {
        let i = b.document().track(horn).unwrap().instrument();
        (i.id(), i.voices()[0].id(), i.staves()[1].id())
    };
    // A second voice whose home is the lower staff: voices need not be one per staff.
    let extra = b.add_voice(lower_staff).unwrap();
    let quarter = NoteValue::plain(BaseValue::Quarter);
    let eighth = NoteValue::plain(BaseValue::Eighth);
    let p = |step, octave| Pitch::new(step, Alteration::NATURAL, octave);

    // Bar 0: one chord per ornament kind, with articulations, an arpeggio and a fermata.
    let ornaments = [
        Ornament::Trill {
            auxiliary: Some(Alteration::chromatic(1).unwrap()),
        },
        Ornament::Turn,
        Ornament::InvertedTurn,
        Ornament::Mordent,
    ];
    let mut chords = Vec::new();
    for (i, ornament) in ornaments.into_iter().enumerate() {
        let ids = b
            .add_chord(
                Placement::new(upper, bars[0], d(i64::try_from(i).unwrap(), 4)),
                quarter,
                &[p(Step::C, 4), p(Step::E, 4)],
            )
            .unwrap();
        b.set_ornament(ids.event, Some(ornament)).unwrap();
        chords.push(ids);
    }
    b.set_articulations(
        chords[0].event,
        ArticulationSet::EMPTY
            .with(Articulation::Staccato)
            .with(Articulation::Accent),
    )
    .unwrap();
    b.set_arpeggio(chords[1].event, Some(Arpeggio::Up)).unwrap();
    b.set_fermata(chords[3].event, Some(Fermata::Long)).unwrap();
    // A forward tie into bar 1, a let-ring tie, offsets.
    let target = b
        .add_chord(
            Placement::new(upper, bars[1], Duration::ZERO),
            quarter,
            &[p(Step::C, 4)],
        )
        .unwrap();
    b.set_tie(chords[3].notes[0], Tie::To(target.notes[0]))
        .unwrap();
    b.set_tie(chords[2].notes[1], Tie::LetRing).unwrap();
    b.set_offsets(
        chords[1].notes[0],
        PerformanceOffsets {
            onset: d(-1, 64),
            length: d(1, 32),
            velocity: -500,
        },
    )
    .unwrap();
    // Bar 1: a tremolo, then a triplet with a nested duplet.
    let tremolo = b
        .add_chord(
            Placement::new(upper, bars[1], d(1, 4)),
            quarter,
            &[p(Step::G, 4)],
        )
        .unwrap();
    b.set_ornament(
        tremolo.event,
        Some(Ornament::Tremolo(TremoloStrokes::new(3).unwrap())),
    )
    .unwrap();
    let triplet = b
        .add_tuplet(upper, Tuplet::new(3, 2, quarter).unwrap(), None)
        .unwrap();
    let inner = b
        .add_tuplet(upper, Tuplet::new(2, 3, eighth).unwrap(), Some(triplet))
        .unwrap();
    b.add_chord(
        Placement::new(upper, bars[1], d(1, 2)).in_tuplet(triplet),
        quarter,
        &[p(Step::A, 4)],
    )
    .unwrap();
    b.add_chord(
        Placement::new(upper, bars[1], d(2, 3)).in_tuplet(inner),
        eighth,
        &[p(Step::B, 4)],
    )
    .unwrap();
    // The extra voice: a rest with a fermata (a general pause) in bar 2.
    let pause = b
        .add_rest(
            Placement::new(extra, bars[2], Duration::ZERO),
            RestValue::WholeBar,
        )
        .unwrap();
    b.set_fermata(pause, Some(Fermata::VeryLong)).unwrap();
    // A key override for the horn (no signature, as horn parts often are), a per-track tolerance,
    // a numbering style.
    b.add_key_override(
        horn_id,
        bars[0],
        Duration::ZERO,
        KeyOverride::Signature(KeySignature::NoSignature),
    )
    .unwrap();
    b.set_tolerance(Some(other), Some(Tolerance::Millis(25)))
        .unwrap();
    b.set_bar_numbering(BarNumbering::SameAsFirst);
    b.build().unwrap()
}

/// The two test documents come back identical: content, IDs, counter, description.
#[test]
fn documents_round_trip() {
    assert_round_trip(&rich());
    assert_round_trip(&markings());
}

/// A document whose counter is exhausted loads, and stays exhausted.
#[test]
fn an_exhausted_counter_round_trips() {
    let mut parts = markings().to_parts();
    parts.next_id = None;
    let doc = Document::from_parts(&parts).unwrap();
    assert_eq!(doc.to_parts().next_id, None);
}

/// The same raw ID on two objects, of the same kind or not, is refused.
#[test]
fn duplicate_ids_are_refused() {
    let doc = markings();
    // Two bars with one ID.
    let mut parts = doc.to_parts();
    parts.bars[1] = parts.bars[0];
    assert_eq!(
        Document::from_parts(&parts).err(),
        Some(DocumentError::DuplicateId(parts.bars[0].get()))
    );
    // An event given a bar's ID: IDs are unique across kinds.
    let mut parts = doc.to_parts();
    let bar = parts.bars[3].get();
    let cell = &mut parts.tracks[0].voices[0].cells[0];
    let placed = &cell.events[0];
    let event = placed.event();
    cell.events[0] = PlacedEvent::new(
        placed.offset(),
        Event::new(
            EventId::from_raw(bar).unwrap(),
            event.staff(),
            event.body().clone(),
        ),
    );
    assert_eq!(
        Document::from_parts(&parts).err(),
        Some(DocumentError::DuplicateId(bar))
    );
}

/// An ID at or above the saved next value of the counter is refused: a later object would get it.
#[test]
fn unissued_ids_are_refused() {
    let mut parts = markings().to_parts();
    let highest = parts.next_id.unwrap().get() - 1;
    parts.next_id = NonZeroU64::new(highest);
    assert_eq!(
        Document::from_parts(&parts).err(),
        Some(DocumentError::IdNotIssued(highest))
    );
}

/// A cell naming a bar the document does not have is refused by the builder's own check.
#[test]
fn an_unknown_bar_is_refused() {
    let mut parts = markings().to_parts();
    // A staff's ID: issued and unique, but not a bar.
    let staff = parts.tracks[0].staves[0].id.get();
    parts.tracks[0].voices[0].cells[0].bar = BarId::from_raw(staff).unwrap();
    assert_eq!(
        Document::from_parts(&parts).err(),
        Some(DocumentError::UnknownBar(BarId::from_raw(staff).unwrap()))
    );
}

/// Lists out of the document's order are refused, never silently reordered: events in a chunk,
/// noteheads in a chord.
#[test]
fn lists_out_of_order_are_refused() {
    let doc = markings();
    let mut parts = doc.to_parts();
    parts.tracks[0].voices[0].cells[0].events.swap(0, 1);
    assert_eq!(
        Document::from_parts(&parts).err(),
        Some(DocumentError::NotCanonical)
    );
    let mut parts = doc.to_parts();
    let placed = &parts.tracks[0].voices[0].cells[0].events[0];
    let EventBody::Chord(chord) = placed.event().body() else {
        panic!("the first event is a chord");
    };
    let mut notes: Vec<_> = chord.notes().copied().collect();
    notes.reverse();
    let reversed = crate::event::Chord::new(chord.value(), chord.tuplet(), chord.grace(), notes)
        .unwrap()
        .with_articulations(chord.articulations())
        .with_ornament(chord.ornament());
    let event = placed.event();
    parts.tracks[0].voices[0].cells[0].events[0] = PlacedEvent::new(
        placed.offset(),
        Event::new(event.id(), event.staff(), EventBody::Chord(reversed)),
    );
    assert_eq!(
        Document::from_parts(&parts).err(),
        Some(DocumentError::NotCanonical)
    );
}

/// A description of an invalid document (two overlapping events) is refused by `validate`.
#[test]
fn an_invalid_document_is_refused() {
    let mut parts = markings().to_parts();
    let events = &mut parts.tracks[0].voices[0].cells[0].events;
    // Move the second chord onto the first one's time.
    let second = events[1].event().clone();
    events[1] = PlacedEvent::new(d(1, 8), second.clone());
    assert_eq!(
        Document::from_parts(&parts).err(),
        Some(DocumentError::Invalid(Violation::Overlap(second.id())))
    );
}

/// Rule 13: a meter change past the last bar is refused.
#[test]
fn a_meter_change_past_the_last_bar_is_refused() {
    let mut parts = markings().to_parts();
    let past = BarIndex::new(u32::try_from(parts.bars.len()).unwrap());
    parts
        .meter_map
        .set_meter_change(past, MeterChange::new(Meter::new(&[3], 4).unwrap()))
        .unwrap();
    assert_eq!(
        Document::from_parts(&parts).err(),
        Some(DocumentError::Invalid(Violation::MeterChangePastLastBar(
            past.get()
        )))
    );
    // The last bar itself is fine.
    let mut parts = markings().to_parts();
    let last = BarIndex::new(past.get() - 1);
    parts
        .meter_map
        .set_meter_change(last, MeterChange::new(Meter::new(&[3], 4).unwrap()))
        .unwrap();
    assert!(Document::from_parts(&parts).is_ok());
}

/// The builder (and so a file) holds at most `MAX_TRACKS` tracks.
#[test]
fn too_many_tracks_are_refused() {
    let meters = MeterMap::new(MeterChange::new(Meter::new(&[4], 4).unwrap())).unwrap();
    let tempo = Tempo::new(d(1, 4), Rational::from_integer(60).unwrap()).unwrap();
    let mut b = DocumentBuilder::new(
        meters,
        TempoMark::immediate(None, Metronome::shown(tempo)).unwrap(),
        Key::traditional(0, Mode::Major).unwrap(),
    )
    .unwrap();
    let unison = Interval::new(0, Rational::ZERO);
    for _ in 0..MAX_TRACKS {
        b.add_instrument_track("T", unison, &[Clef::TREBLE])
            .unwrap();
    }
    assert_eq!(
        b.add_instrument_track("T", unison, &[Clef::TREBLE]).err(),
        Some(DocumentError::TooManyTracks)
    );
}
