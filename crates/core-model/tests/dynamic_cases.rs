// SPDX-License-Identifier: GPL-3.0-or-later

//! Hand-written cases for dynamics and hairpins (ADR-0024 §8, SCALETTA F1.09c, second part): the
//! order of the levels, the sorted lists per instrument, the scope and staff checks, the rule of
//! one dynamic per scope at a point, the hairpin rules of `validate` (rule 6), and the case
//! "Dynamics" of ADR-0024 §13 (a dynamic never touches a note's stored offsets).

// Helper functions (not `#[test]` functions) are outside the scope of clippy.toml's
// `allow-unwrap-in-tests`, so the exemption is stated here, for this test file only.
#![allow(
    clippy::unwrap_used,
    reason = "test helpers: a failed unwrap fails the test"
)]

use orchestra_core_model::{
    Alteration, BarId, BaseValue, Clef, DocumentBuilder, DocumentError, Dynamic, DynamicLevel,
    DynamicMark, DynamicPlacement, DynamicScope, HairpinKind, HairpinShape, HairpinStyle,
    InstrumentId, Interval, Key, Metronome, Mode, NoteValue, OtherDynamic, PerformanceOffsets,
    Pitch, Placement, ScorePoint, StaffId, Step, TempoMark, Violation, VoiceId,
};
use orchestra_core_time::{Duration, Meter, MeterChange, MeterMap, Rational, Tempo};

#[path = "common/duration.rs"]
mod duration;
use duration::d;

/// The point `n/den` after the start of `bar`.
fn at(bar: BarId, n: i64, den: i64) -> ScorePoint {
    ScorePoint::new(bar, d(n, den))
}

/// A dynamic of the scale.
fn level(l: DynamicLevel) -> DynamicMark {
    DynamicMark::Level(l)
}

/// A piano in 4/4 at ♩ = 60, in C major, with `bars` bars: (builder, bars, the piano's
/// instrument, its two staves, its two voices).
struct Piano {
    /// The builder.
    b: DocumentBuilder,
    /// The bars, in order.
    bars: Vec<BarId>,
    /// The piano.
    instrument: InstrumentId,
    /// Upper and lower staff.
    staves: [StaffId; 2],
    /// Right-hand and left-hand voice.
    voices: [VoiceId; 2],
}

/// The piano described on [`Piano`], with a flute after it.
fn piano(bars: usize) -> (Piano, InstrumentId, StaffId, VoiceId) {
    let meters = MeterMap::new(MeterChange::new(Meter::new(&[4], 4).unwrap())).unwrap();
    let tempo = Tempo::new(d(1, 4), Rational::from_integer(60).unwrap()).unwrap();
    let mut b = DocumentBuilder::new(
        meters,
        TempoMark::immediate(None, Metronome::shown(tempo)).unwrap(),
        Key::traditional(0, Mode::Major).unwrap(),
    )
    .unwrap();
    let bars = (0..bars).map(|_| b.add_bar().unwrap()).collect();
    let unison = Interval::new(0, Rational::ZERO);
    let track = b
        .add_instrument_track("Piano", unison, &[Clef::TREBLE, Clef::BASS])
        .unwrap();
    let flute_track = b
        .add_instrument_track("Flute", unison, &[Clef::TREBLE])
        .unwrap();
    let doc = b.document();
    let instrument = doc.track(track).unwrap().instrument();
    let (piano, staves, voices) = (
        instrument.id(),
        [instrument.staves()[0].id(), instrument.staves()[1].id()],
        [instrument.voices()[0].id(), instrument.voices()[1].id()],
    );
    let flute = doc.track(flute_track).unwrap().instrument();
    let (flute, flute_staff, flute_voice) =
        (flute.id(), flute.staves()[0].id(), flute.voices()[0].id());
    (
        Piano {
            b,
            bars,
            instrument: piano,
            staves,
            voices,
        },
        flute,
        flute_staff,
        flute_voice,
    )
}

/// The levels go from soft to loud, so `Ord` answers "is this louder?".
#[test]
fn levels_are_ordered_soft_to_loud() {
    use DynamicLevel::{F, Ffffff, Mf, Mp, P, Pp, Pppppp};
    let ladder = [Pppppp, Pp, P, Mp, Mf, F, Ffffff];
    assert!(ladder.windows(2).all(|w| w[0] < w[1]));
}

/// Dynamics are kept per instrument in score order, at one point in creation order; one per scope
/// at a point, so an instrument dynamic and a voice dynamic may share it.
#[test]
fn dynamics_are_sorted_and_one_per_scope_at_a_point() {
    let (mut p, _, _, _) = piano(2);
    let below = DynamicPlacement::instrument(p.staves[0]);
    let f =
        p.b.add_dynamic(
            p.instrument,
            ScorePoint::start_of(p.bars[1]),
            below,
            level(DynamicLevel::F),
        )
        .unwrap();
    let soft =
        p.b.add_dynamic(
            p.instrument,
            at(p.bars[0], 1, 2),
            DynamicPlacement::instrument(p.staves[1]),
            level(DynamicLevel::P),
        )
        .unwrap();
    // The left hand alone at the same point as the f: another scope, so allowed.
    let left =
        p.b.add_dynamic(
            p.instrument,
            ScorePoint::start_of(p.bars[1]),
            DynamicPlacement::voice(p.voices[1], p.staves[1]),
            level(DynamicLevel::Mp),
        )
        .unwrap();
    // A second instrument dynamic there contradicts the f, even on another staff.
    let before = p.b.document().clone();
    assert_eq!(
        p.b.add_dynamic(
            p.instrument,
            ScorePoint::start_of(p.bars[1]),
            DynamicPlacement::instrument(p.staves[1]),
            level(DynamicLevel::Pp),
        ),
        Err(DocumentError::DuplicateMark)
    );
    assert_eq!(p.b.document(), &before);

    let document = p.b.build().unwrap();
    let instrument = document.instrument(p.instrument).unwrap();
    let ids: Vec<_> = instrument.dynamics().iter().map(Dynamic::id).collect();
    assert_eq!(ids, [soft, f, left]);
    let first = &instrument.dynamics()[0];
    assert_eq!(first.at(), at(p.bars[0], 1, 2));
    assert_eq!(first.placement().staff, p.staves[1]);
    assert_eq!(first.placement().scope, DynamicScope::Instrument);
    assert_eq!(first.mark(), &level(DynamicLevel::P));
}

/// The staff and the voice of a placement must belong to the instrument; the point must be in the
/// document. A refusal leaves the builder unchanged and consumes no ID.
#[test]
fn placements_are_checked() {
    let (mut p, flute, flute_staff, flute_voice) = piano(1);
    let bar = p.bars[0];
    let before = p.b.document().clone();
    let mark = level(DynamicLevel::Mf);
    assert_eq!(
        p.b.add_dynamic(
            p.instrument,
            ScorePoint::start_of(bar),
            DynamicPlacement::instrument(flute_staff),
            mark.clone(),
        ),
        Err(DocumentError::StaffOfAnotherInstrument(flute_staff))
    );
    assert_eq!(
        p.b.add_dynamic(
            p.instrument,
            ScorePoint::start_of(bar),
            DynamicPlacement::voice(flute_voice, p.staves[0]),
            mark.clone(),
        ),
        Err(DocumentError::VoiceOfAnotherInstrument(flute_voice))
    );
    assert_eq!(
        p.b.add_dynamic(
            p.instrument,
            at(bar, -1, 4),
            DynamicPlacement::instrument(p.staves[0]),
            mark.clone(),
        ),
        Err(DocumentError::NegativeOffset)
    );
    assert_eq!(
        p.b.add_hairpin(
            flute,
            ScorePoint::start_of(bar),
            at(bar, 1, 2),
            DynamicPlacement::instrument(p.staves[0]),
            HairpinShape::CRESCENDO,
        ),
        Err(DocumentError::StaffOfAnotherInstrument(p.staves[0]))
    );
    // An instrument of another document. Built with more bars, so that its IDs are shifted: the
    // same construction would hand out the very same IDs, and the piano would be found here.
    let (other, _, _, _) = piano(5);
    assert_eq!(
        p.b.add_dynamic(
            other.instrument,
            ScorePoint::start_of(bar),
            DynamicPlacement::instrument(p.staves[0]),
            mark,
        ),
        Err(DocumentError::UnknownInstrument(other.instrument))
    );
    assert_eq!(p.b.document(), &before);
    // No ID was consumed: the next bar takes the ID right after the last one handed out.
    let last = p.voices[1].get().max(flute_voice.get());
    assert_eq!(p.b.add_bar().unwrap().get(), last + 1);
}

/// A free-text dynamic keeps its text and its playback level; an empty text is refused.
#[test]
fn free_text_dynamics() {
    let subito = OtherDynamic::new("subito p", Some(DynamicLevel::P)).unwrap();
    assert_eq!(subito.text(), "subito p");
    assert_eq!(subito.level(), Some(DynamicLevel::P));
    assert_eq!(
        OtherDynamic::new("   ", None),
        Err(DocumentError::EmptyText)
    );
    let (mut p, _, _, _) = piano(1);
    p.b.add_dynamic(
        p.instrument,
        ScorePoint::start_of(p.bars[0]),
        DynamicPlacement::instrument(p.staves[0]),
        DynamicMark::Other(subito.clone()),
    )
    .unwrap();
    let document = p.b.build().unwrap();
    assert_eq!(
        document.instrument(p.instrument).unwrap().dynamics()[0].mark(),
        &DynamicMark::Other(subito)
    );
}

/// ADR-0024 §13, "Dynamics": a dynamic is position-anchored, so adding one where a note is never
/// touches the note's stored performance offsets (they are deltas on top of what playback derives
/// from the dynamics, ADR-0004).
#[test]
fn a_dynamic_never_touches_the_offsets_of_a_note() {
    let (mut p, _, _, _) = piano(1);
    let bar = p.bars[0];
    let chord =
        p.b.add_chord(
            Placement::new(p.voices[0], bar, Duration::ZERO),
            NoteValue::plain(BaseValue::Whole),
            &[Pitch::new(Step::C, Alteration::NATURAL, 5)],
        )
        .unwrap();
    let louder = PerformanceOffsets {
        onset: d(1, 64),
        length: Duration::ZERO,
        velocity: 2048,
    };
    p.b.set_offsets(chord.notes[0], louder).unwrap();
    let events_before = p.b.document().events(p.voices[0], bar).unwrap().to_vec();
    for (i, l) in [DynamicLevel::Pp, DynamicLevel::Ff].into_iter().enumerate() {
        let scope = if i == 0 {
            DynamicPlacement::instrument(p.staves[0])
        } else {
            DynamicPlacement::voice(p.voices[0], p.staves[0])
        };
        p.b.add_dynamic(p.instrument, ScorePoint::start_of(bar), scope, level(l))
            .unwrap();
    }
    let document = p.b.build().unwrap();
    assert_eq!(document.events(p.voices[0], bar).unwrap(), events_before);
    let (_, note) = document.note(chord.notes[0]).unwrap();
    assert_eq!(note.offsets(), louder);
}

/// A hairpin must end after it starts; the end is exclusive, so two hairpins of one scope may
/// touch ("cresc. up to here, dim. from here").
#[test]
fn hairpins_go_forwards_and_may_touch() {
    let (mut p, _, _, _) = piano(2);
    let [b0, b1] = [p.bars[0], p.bars[1]];
    let place = DynamicPlacement::instrument(p.staves[0]);
    for end in [at(b0, 1, 2), at(b0, 1, 4)] {
        assert_eq!(
            p.b.add_hairpin(
                p.instrument,
                at(b0, 1, 2),
                end,
                place,
                HairpinShape::CRESCENDO
            ),
            Err(DocumentError::HairpinNotForward)
        );
    }
    let dim_al_niente = HairpinShape {
        kind: HairpinKind::Diminuendo,
        style: HairpinStyle::Text,
        niente: true,
    };
    let dim =
        p.b.add_hairpin(
            p.instrument,
            ScorePoint::start_of(b1),
            at(b1, 1, 1),
            place,
            dim_al_niente,
        )
        .unwrap();
    let cresc =
        p.b.add_hairpin(
            p.instrument,
            at(b0, 1, 2),
            ScorePoint::start_of(b1),
            place,
            HairpinShape::CRESCENDO,
        )
        .unwrap();
    // A dynamic inside the crescendo is fine.
    p.b.add_dynamic(p.instrument, at(b0, 3, 4), place, level(DynamicLevel::Mf))
        .unwrap();
    let document = p.b.build().unwrap();
    let hairpins = document.instrument(p.instrument).unwrap().hairpins();
    // Sorted by start, whatever the creation order.
    assert_eq!(hairpins[0].id(), cresc);
    assert_eq!(hairpins[1].id(), dim);
    assert_eq!(hairpins[1].shape(), dim_al_niente);
    assert_eq!(hairpins[1].end(), at(b1, 1, 1));
}

/// Two hairpins of the same scope may not overlap (rule 6 of `validate`); of different scopes they
/// may.
#[test]
fn hairpins_of_one_scope_do_not_overlap() {
    let (mut p, _, _, _) = piano(2);
    let [b0, b1] = [p.bars[0], p.bars[1]];
    let whole = DynamicPlacement::instrument(p.staves[0]);
    let right = DynamicPlacement::voice(p.voices[0], p.staves[0]);
    p.b.add_hairpin(
        p.instrument,
        ScorePoint::start_of(b0),
        ScorePoint::start_of(b1),
        whole,
        HairpinShape::CRESCENDO,
    )
    .unwrap();
    // Another scope over the same span: fine.
    p.b.add_hairpin(
        p.instrument,
        at(b0, 1, 4),
        at(b0, 3, 4),
        right,
        HairpinShape::DIMINUENDO,
    )
    .unwrap();
    assert!(p.b.clone().build().is_ok());
    // The same scope, starting before the first one ends.
    let overlapping =
        p.b.add_hairpin(
            p.instrument,
            at(b0, 1, 2),
            at(b1, 1, 2),
            whole,
            HairpinShape::DIMINUENDO,
        )
        .unwrap();
    assert_eq!(
        p.b.build(),
        Err(DocumentError::Invalid(Violation::HairpinOverlap(
            overlapping
        )))
    );
}
