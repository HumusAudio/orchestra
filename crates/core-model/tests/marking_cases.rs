// SPDX-License-Identifier: GPL-3.0-or-later

//! Hand-written cases for what is written on the events (ADR-0024 §5–§7 and §10, SCALETTA
//! F1.09c, first part): grace chords and their order at one offset, articulations, ornaments,
//! fermatas, arpeggios, the performance offsets of the noteheads, slurs with their derived index,
//! and the score order that ties and slurs follow. The cases of ADR-0024 §13 that belong to this
//! part ("Articulations", "Grace notes") are named after it.

// Helper functions (not `#[test]` functions) are outside the scope of clippy.toml's
// `allow-unwrap-in-tests`, so the exemption is stated here, for this test file only.
#![allow(
    clippy::unwrap_used,
    reason = "test helpers: a failed unwrap fails the test"
)]

use orchestra_core_model::{
    Alteration, Arpeggio, Articulation, ArticulationSet, BarId, BaseValue, ChordIds, Clef,
    DocumentBuilder, DocumentError, EventId, Fermata, Grace, GraceAttach, GraceKind, Interval, Key,
    Metronome, Mode, NoteId, NoteValue, Ornament, OrnamentError, PerformanceOffsets, Pitch,
    Placement, RestValue, Slur, SlurPlacement, Step, TempoMark, Tie, TremoloStrokes, Tuplet,
    Violation, VoiceId,
};
use orchestra_core_time::{Duration, Meter, MeterChange, MeterMap, Rational, Tempo};

#[path = "common/duration.rs"]
mod duration;
use duration::d;

/// An undotted value.
fn v(base: BaseValue) -> NoteValue {
    NoteValue::plain(base)
}

/// A natural pitch.
fn nat(step: Step, octave: i8) -> Pitch {
    Pitch::new(step, Alteration::NATURAL, octave)
}

/// The unison: the transposition of an instrument at concert pitch.
fn unison() -> Interval {
    Interval::new(0, Rational::ZERO)
}

/// An empty builder in 4/4 at ♩ = 60, in C major, with `bars` bars.
fn builder(bars: usize) -> (DocumentBuilder, Vec<BarId>) {
    let meters = MeterMap::new(MeterChange::new(Meter::new(&[4], 4).unwrap())).unwrap();
    let tempo = Tempo::new(d(1, 4), Rational::from_integer(60).unwrap()).unwrap();
    let key = Key::traditional(0, Mode::Major).unwrap();
    let mut b = DocumentBuilder::new(
        meters,
        TempoMark::immediate(None, Metronome::shown(tempo)).unwrap(),
        key,
    )
    .unwrap();
    let ids = (0..bars).map(|_| b.add_bar().unwrap()).collect();
    (b, ids)
}

/// A builder with `bars` bars and one flute: (builder, bars, the flute's voice).
fn flute(bars: usize) -> (DocumentBuilder, Vec<BarId>, VoiceId) {
    let (mut b, ids) = builder(bars);
    let track = b
        .add_instrument_track("Flute", unison(), &[Clef::TREBLE])
        .unwrap();
    let voice = b.document().track(track).unwrap().instrument().voices()[0].id();
    (b, ids, voice)
}

/// A builder with one bar and a piano: (builder, the bar, right-hand voice, left-hand voice).
fn piano() -> (DocumentBuilder, BarId, VoiceId, VoiceId) {
    let (mut b, bars) = builder(1);
    let track = b
        .add_instrument_track("Piano", unison(), &[Clef::TREBLE, Clef::BASS])
        .unwrap();
    let voices = b.document().track(track).unwrap().instrument().voices();
    let (rh, lh) = (voices[0].id(), voices[1].id());
    (b, bars[0], rh, lh)
}

/// A quarter note C5 at `offset` of `bar` in `voice`.
fn quarter(b: &mut DocumentBuilder, voice: VoiceId, bar: BarId, offset: Duration) -> ChordIds {
    b.add_chord(
        Placement::new(voice, bar, offset),
        v(BaseValue::Quarter),
        &[nat(Step::C, 5)],
    )
    .unwrap()
}

/// An eighth-note grace D5 of this kind and side at `offset` of `bar` in `voice`.
fn grace(
    b: &mut DocumentBuilder,
    voice: VoiceId,
    bar: BarId,
    offset: Duration,
    attach: GraceAttach,
) -> ChordIds {
    b.add_grace_chord(
        Placement::new(voice, bar, offset),
        Grace {
            kind: GraceKind::Acciaccatura,
            attach,
        },
        v(BaseValue::Eighth),
        &[nat(Step::D, 5)],
    )
    .unwrap()
}

/// The IDs of the events of `voice` in `bar`, in chunk order.
fn order(b: &DocumentBuilder, voice: VoiceId, bar: BarId) -> Vec<EventId> {
    b.document()
        .events(voice, bar)
        .unwrap()
        .iter()
        .map(|e| e.event().id())
        .collect()
}

/// ADR-0024 §13, articulations: a set survives set/clear round trips on a chord, and portato is
/// its own mark, not staccato + tenuto.
#[test]
fn articulations_round_trip() {
    let (mut b, bars, fl) = flute(1);
    let note = quarter(&mut b, fl, bars[0], Duration::ZERO);
    let accented_staccato = ArticulationSet::EMPTY
        .with(Articulation::Accent)
        .with(Articulation::Staccato);
    b.set_articulations(note.event, accented_staccato).unwrap();
    let document = b.clone().build().unwrap();
    let chord = document.event(note.event).unwrap().event().chord().unwrap();
    assert_eq!(chord.articulations(), accented_staccato);
    assert_eq!(
        chord.articulations().iter().collect::<Vec<_>>(),
        [Articulation::Accent, Articulation::Staccato]
    );

    // Removing one and adding it again gives the same set; removing an absent one changes nothing.
    let set = accented_staccato.without(Articulation::Accent);
    assert_eq!(set.with(Articulation::Accent), accented_staccato);
    assert_eq!(set.without(Articulation::Tenuto), set);
    // Clearing the chord's articulations.
    b.set_articulations(note.event, ArticulationSet::EMPTY)
        .unwrap();
    let document = b.build().unwrap();
    let chord = document.event(note.event).unwrap().event().chord().unwrap();
    assert!(chord.articulations().is_empty());

    // Portato is neither staccato nor tenuto, and staccato + tenuto is not portato.
    let portato = ArticulationSet::EMPTY.with(Articulation::Portato);
    let staccato_tenuto = ArticulationSet::EMPTY
        .with(Articulation::Staccato)
        .with(Articulation::Tenuto);
    assert_ne!(portato, staccato_tenuto);
    assert!(!portato.contains(Articulation::Staccato));
    assert!(!portato.contains(Articulation::Tenuto));
    assert!(!staccato_tenuto.contains(Articulation::Portato));
    // Every articulation of the palette fits in the set at once.
    let all: ArticulationSet = Articulation::ALL.into_iter().collect();
    assert_eq!(all.len(), Articulation::ALL.len());
}

/// ADR-0024 §13, grace notes: `Before` and `After` graces have zero metric duration and do not
/// move the following events; at one offset the chunk holds After < Before < main, whatever order
/// they were entered in.
#[test]
fn grace_notes_take_no_time_and_sort_by_slot() {
    let (mut b, bars, fl) = flute(1);
    let bar = bars[0];
    // Entered in the "wrong" order on purpose: main event first, then its Before grace, then the
    // After grace of the previous event.
    let first = quarter(&mut b, fl, bar, Duration::ZERO);
    let second = quarter(&mut b, fl, bar, d(1, 4));
    let before = grace(&mut b, fl, bar, d(1, 4), GraceAttach::Before);
    let after = grace(&mut b, fl, bar, d(1, 4), GraceAttach::After);
    let leading = grace(&mut b, fl, bar, Duration::ZERO, GraceAttach::Before);
    assert_eq!(
        order(&b, fl, bar),
        [
            leading.event,
            first.event,
            after.event,
            before.event,
            second.event
        ]
    );
    let document = b.build().unwrap();
    for g in [&before, &after, &leading] {
        assert_eq!(document.metric_duration(g.event).unwrap(), Duration::ZERO);
        // The written value is kept: an eighth is drawn.
        let chord = document.event(g.event).unwrap().event().chord().unwrap();
        assert_eq!(chord.value(), v(BaseValue::Eighth));
        assert!(chord.grace().is_some());
    }
    // The main events keep their place and their duration.
    assert_eq!(document.event(second.event).unwrap().offset(), d(1, 4));
    assert_eq!(document.metric_duration(first.event).unwrap(), d(1, 4));
    // Several graces in a row before one main event are fine too.
    let (mut b, bars, fl) = flute(1);
    let main = quarter(&mut b, fl, bars[0], d(1, 2));
    let g1 = grace(&mut b, fl, bars[0], d(1, 2), GraceAttach::Before);
    let g2 = grace(&mut b, fl, bars[0], d(1, 2), GraceAttach::Before);
    assert_eq!(order(&b, fl, bars[0]), [g1.event, g2.event, main.event]);
    assert!(b.build().is_ok());
}

/// ADR-0024 §5: graces are never tuplet members.
#[test]
fn grace_in_a_tuplet_is_refused() {
    let (mut b, bars, fl) = flute(1);
    let triplet = b
        .add_tuplet(fl, Tuplet::new(3, 2, v(BaseValue::Eighth)).unwrap(), None)
        .unwrap();
    let before = b.document().clone();
    let at = Placement::new(fl, bars[0], Duration::ZERO).in_tuplet(triplet);
    let g = Grace {
        kind: GraceKind::Appoggiatura,
        attach: GraceAttach::Before,
    };
    assert_eq!(
        b.add_grace_chord(at, g, v(BaseValue::Sixteenth), &[nat(Step::E, 5)]),
        Err(DocumentError::GraceInTuplet)
    );
    assert_eq!(b.document(), &before);
}

/// ADR-0024 §5, rule 4 of `validate`: a grace without its main event is refused.
#[test]
fn graces_without_their_main_event() {
    // A Before grace with nothing at its offset (the chunk ends).
    let (mut b, bars, fl) = flute(1);
    quarter(&mut b, fl, bars[0], Duration::ZERO);
    let lonely = grace(&mut b, fl, bars[0], d(1, 2), GraceAttach::Before);
    assert_eq!(
        b.build(),
        Err(DocumentError::Invalid(Violation::BeforeGraceWithoutEvent(
            lonely.event
        )))
    );

    // A Before grace followed by a main event at a later offset.
    let (mut b, bars, fl) = flute(1);
    let early = grace(&mut b, fl, bars[0], d(1, 4), GraceAttach::Before);
    quarter(&mut b, fl, bars[0], d(1, 2));
    assert_eq!(
        b.build(),
        Err(DocumentError::Invalid(Violation::BeforeGraceWithoutEvent(
            early.event
        )))
    );

    // A Before grace followed by an After grace (necessarily at a later offset). A valid pair
    // comes first, so that only the stray grace is wrong.
    let (mut b, bars, fl) = flute(1);
    quarter(&mut b, fl, bars[0], Duration::ZERO);
    grace(&mut b, fl, bars[0], Duration::ZERO, GraceAttach::Before);
    assert!(b.clone().build().is_ok());
    let stray = grace(&mut b, fl, bars[0], d(1, 2), GraceAttach::Before);
    grace(&mut b, fl, bars[0], d(3, 4), GraceAttach::After);
    assert_eq!(
        b.build(),
        Err(DocumentError::Invalid(Violation::BeforeGraceWithoutEvent(
            stray.event
        )))
    );

    // An After grace that is not where the previous main event ends.
    let (mut b, bars, fl) = flute(1);
    quarter(&mut b, fl, bars[0], Duration::ZERO);
    let late = grace(&mut b, fl, bars[0], d(1, 2), GraceAttach::After);
    assert_eq!(
        b.build(),
        Err(DocumentError::Invalid(Violation::AfterGraceWithoutEvent(
            late.event
        )))
    );

    // An After grace with no main event before it at all.
    let (mut b, bars, fl) = flute(1);
    let first = grace(&mut b, fl, bars[0], Duration::ZERO, GraceAttach::After);
    assert_eq!(
        b.build(),
        Err(DocumentError::Invalid(Violation::AfterGraceWithoutEvent(
            first.event
        )))
    );

    // A grace inside a main event is an overlap (rule 1), found before rule 4.
    let (mut b, bars, fl) = flute(1);
    quarter(&mut b, fl, bars[0], Duration::ZERO);
    let inside = grace(&mut b, fl, bars[0], d(1, 8), GraceAttach::Before);
    assert_eq!(
        b.build(),
        Err(DocumentError::Invalid(Violation::Overlap(inside.event)))
    );

    // An After grace at the end of a full bar, after its last note: valid (the barline is where
    // the note ends).
    let (mut b, bars, fl) = flute(1);
    b.add_chord(
        Placement::new(fl, bars[0], Duration::ZERO),
        v(BaseValue::Whole),
        &[nat(Step::G, 4)],
    )
    .unwrap();
    grace(&mut b, fl, bars[0], d(1, 1), GraceAttach::After);
    assert!(b.build().is_ok());
}

/// ADR-0024 §6: ornaments, tremolo strokes, fermatas and arpeggios are stored as set; markings
/// that only a chord can carry are refused on a rest.
#[test]
fn ornaments_fermatas_and_arpeggios() {
    assert_eq!(
        TremoloStrokes::new(0),
        Err(OrnamentError::TremoloStrokesOutOfRange)
    );
    assert_eq!(
        TremoloStrokes::new(5),
        Err(OrnamentError::TremoloStrokesOutOfRange)
    );
    for n in 1..=TremoloStrokes::MAX {
        assert_eq!(TremoloStrokes::new(n).unwrap().get(), n);
    }

    let (mut b, bars, fl) = flute(2);
    let note = quarter(&mut b, fl, bars[0], Duration::ZERO);
    let rest = b
        .add_rest(
            Placement::new(fl, bars[1], Duration::ZERO),
            RestValue::WholeBar,
        )
        .unwrap();
    let trill = Ornament::Trill {
        auxiliary: Some(Alteration::FLAT),
    };
    b.set_ornament(note.event, Some(trill)).unwrap();
    b.set_arpeggio(note.event, Some(Arpeggio::Up)).unwrap();
    b.set_fermata(note.event, Some(Fermata::Long)).unwrap();
    // A general pause: a whole-bar rest with a fermata.
    b.set_fermata(rest, Some(Fermata::Normal)).unwrap();

    // Only a chord carries articulations, ornaments and arpeggios; the builder stays unchanged.
    let before = b.document().clone();
    assert_eq!(
        b.set_articulations(rest, ArticulationSet::EMPTY.with(Articulation::Accent)),
        Err(DocumentError::NotAChord(rest))
    );
    assert_eq!(
        b.set_ornament(rest, Some(Ornament::Turn)),
        Err(DocumentError::NotAChord(rest))
    );
    assert_eq!(
        b.set_arpeggio(rest, Some(Arpeggio::Plain)),
        Err(DocumentError::NotAChord(rest))
    );
    assert_eq!(b.document(), &before);

    let document = b.clone().build().unwrap();
    let chord = document.event(note.event).unwrap().event().chord().unwrap();
    assert_eq!(chord.ornament(), Some(trill));
    assert_eq!(chord.arpeggio(), Some(Arpeggio::Up));
    assert_eq!(chord.fermata(), Some(Fermata::Long));
    assert_eq!(
        document.event(rest).unwrap().event().fermata(),
        Some(Fermata::Normal)
    );

    // `None` removes them again.
    b.set_ornament(note.event, None).unwrap();
    b.set_fermata(rest, None).unwrap();
    let document = b.build().unwrap();
    let chord = document.event(note.event).unwrap().event().chord().unwrap();
    assert_eq!(chord.ornament(), None);
    assert_eq!(document.event(rest).unwrap().event().fermata(), None);
}

/// ADR-0024 §10: performance offsets are per notehead, zero by default, stored as given, and
/// never change the written position or the metric duration.
#[test]
fn performance_offsets_are_per_notehead() {
    let (mut b, bars, fl) = flute(1);
    let chord = b
        .add_chord(
            Placement::new(fl, bars[0], d(1, 4)),
            v(BaseValue::Quarter),
            &[nat(Step::C, 5), nat(Step::E, 5)],
        )
        .unwrap();
    let early_and_loud = PerformanceOffsets {
        onset: d(-1, 64),
        length: d(1, 8),
        velocity: 4096,
    };
    b.set_offsets(chord.notes[1], early_and_loud).unwrap();
    let missing = missing_note(&b);
    assert_eq!(
        b.set_offsets(missing, early_and_loud),
        Err(DocumentError::UnknownNote(missing))
    );
    let document = b.build().unwrap();
    assert!(document.note(chord.notes[0]).unwrap().1.offsets().is_none());
    assert_eq!(
        document.note(chord.notes[1]).unwrap().1.offsets(),
        early_and_loud
    );
    // The written layer is untouched.
    assert_eq!(document.event(chord.event).unwrap().offset(), d(1, 4));
    assert_eq!(document.metric_duration(chord.event).unwrap(), d(1, 4));
}

/// A notehead ID that `b`'s document does not have: the ID the next chord of a clone would get.
/// IDs cannot be made from raw numbers outside the crate, so this is how a test names one.
fn missing_note(b: &DocumentBuilder) -> NoteId {
    let mut other = b.clone();
    let bar = other.document().bars().ids()[0];
    let voice = other.document().tracks()[0].instrument().voices()[0].id();
    other
        .add_chord(
            Placement::new(voice, bar, Duration::ZERO),
            v(BaseValue::Whole),
            &[nat(Step::A, 4)],
        )
        .unwrap()
        .notes[0]
}

/// ADR-0024 §7: slurs nest, share ends, join two voices of one instrument; the derived index
/// lists the slurs that start or end on each event, in the order they were added.
#[test]
fn slurs_and_their_index() {
    let (mut b, bar, rh, lh) = piano();
    let n: Vec<ChordIds> = (0..4).map(|i| quarter(&mut b, rh, bar, d(i, 4))).collect();
    let bass = b
        .add_chord(
            Placement::new(lh, bar, d(3, 4)),
            v(BaseValue::Quarter),
            &[nat(Step::C, 3)],
        )
        .unwrap();
    // A phrase over the bar, two shorter slurs inside it sharing an end, one from the right hand
    // into the left hand.
    let phrase = b
        .add_slur(n[0].event, n[3].event, SlurPlacement::Above)
        .unwrap();
    let s1 = b
        .add_slur(n[0].event, n[1].event, SlurPlacement::Auto)
        .unwrap();
    let s2 = b
        .add_slur(n[1].event, n[2].event, SlurPlacement::Auto)
        .unwrap();
    let across = b
        .add_slur(n[0].event, bass.event, SlurPlacement::Below)
        .unwrap();
    let document = b.build().unwrap();

    // Three or more slurs at one event (the "many" case of the index), in the order added.
    assert_eq!(document.slurs_at(n[0].event), [phrase, s1, across]);
    assert_eq!(document.slurs_at(n[1].event), [s1, s2]);
    assert_eq!(document.slurs_at(n[2].event), [s2]);
    assert_eq!(document.slurs_at(n[3].event), [phrase]);
    assert_eq!(document.slurs_at(bass.event), [across]);
    // The slurs of the instrument, in creation order, and the lookup by ID.
    let (instrument, slur) = document.slur(across).unwrap();
    assert_eq!(slur.start(), n[0].event);
    assert_eq!(slur.end(), bass.event);
    assert_eq!(slur.placement(), SlurPlacement::Below);
    let ids: Vec<_> = instrument.slurs().iter().map(Slur::id).collect();
    assert_eq!(ids, [phrase, s1, s2, across]);
}

/// ADR-0024 §7 and the score order: a grace slurred (or tied) to its main note is valid even
/// though both have the same position; backward slurs, slurs between simultaneous events of two
/// voices, slurs to another instrument and slurs to the same event are refused.
#[test]
fn slurs_and_ties_follow_score_order() {
    // Acciaccatura slurred and tied to its main note: same position, earlier slot.
    let (mut b, bars, fl) = flute(1);
    let main = quarter(&mut b, fl, bars[0], d(1, 4));
    let g = b
        .add_grace_chord(
            Placement::new(fl, bars[0], d(1, 4)),
            Grace {
                kind: GraceKind::Acciaccatura,
                attach: GraceAttach::Before,
            },
            v(BaseValue::Eighth),
            &[nat(Step::C, 5)],
        )
        .unwrap();
    b.add_slur(g.event, main.event, SlurPlacement::Auto)
        .unwrap();
    b.set_tie(g.notes[0], Tie::To(main.notes[0])).unwrap();
    let document = b.clone().build().unwrap();
    assert!(document.comes_before(g.event, main.event).unwrap());
    assert!(!document.comes_before(main.event, g.event).unwrap());
    assert!(!document.comes_before(main.event, main.event).unwrap());

    // The same slur backwards.
    let mut back = b.clone();
    let wrong = back
        .add_slur(main.event, g.event, SlurPlacement::Auto)
        .unwrap();
    assert_eq!(
        back.build(),
        Err(DocumentError::Invalid(Violation::SlurNotForward(wrong)))
    );

    // Two Before graces in a row: the chunk order decides between them.
    let mut row = b.clone();
    let g2 = row
        .add_grace_chord(
            Placement::new(fl, bars[0], d(1, 4)),
            Grace {
                kind: GraceKind::Acciaccatura,
                attach: GraceAttach::Before,
            },
            v(BaseValue::Eighth),
            &[nat(Step::B, 4)],
        )
        .unwrap();
    row.add_slur(g.event, g2.event, SlurPlacement::Auto)
        .unwrap();
    let document = row.build().unwrap();
    assert!(document.comes_before(g.event, g2.event).unwrap());
    assert!(!document.comes_before(g2.event, g.event).unwrap());

    // Simultaneous events of two voices: neither comes before the other.
    let (mut b, bar, rh, lh) = piano();
    let top = quarter(&mut b, rh, bar, Duration::ZERO);
    let bottom = b
        .add_chord(
            Placement::new(lh, bar, Duration::ZERO),
            v(BaseValue::Quarter),
            &[nat(Step::C, 3)],
        )
        .unwrap();
    let flat = b
        .add_slur(top.event, bottom.event, SlurPlacement::Auto)
        .unwrap();
    assert_eq!(
        b.build(),
        Err(DocumentError::Invalid(Violation::SlurNotForward(flat)))
    );

    // To another instrument.
    let (mut b, bars) = builder(1);
    let voice_of = |b: &mut DocumentBuilder, name: &str| {
        let t = b
            .add_instrument_track(name, unison(), &[Clef::TREBLE])
            .unwrap();
        b.document().track(t).unwrap().instrument().voices()[0].id()
    };
    let fl = voice_of(&mut b, "Flute");
    let ob = voice_of(&mut b, "Oboe");
    let a = quarter(&mut b, fl, bars[0], Duration::ZERO);
    let z = quarter(&mut b, ob, bars[0], d(1, 4));
    let foreign = b.add_slur(a.event, z.event, SlurPlacement::Auto).unwrap();
    assert_eq!(
        b.build(),
        Err(DocumentError::Invalid(Violation::SlurToAnotherInstrument(
            foreign
        )))
    );
}

/// The builder's slur errors leave it unchanged and consume no ID.
#[test]
fn slur_errors_leave_the_builder_unchanged() {
    let (mut b, bars, fl) = flute(1);
    let a = quarter(&mut b, fl, bars[0], Duration::ZERO);
    let missing = {
        let mut other = b.clone();
        quarter(&mut other, fl, bars[0], d(1, 4)).event
    };
    let before = b.document().clone();
    assert_eq!(
        b.add_slur(a.event, a.event, SlurPlacement::Auto),
        Err(DocumentError::SlurToItself)
    );
    assert_eq!(
        b.add_slur(a.event, missing, SlurPlacement::Auto),
        Err(DocumentError::UnknownEvent(missing))
    );
    assert_eq!(
        b.add_slur(missing, a.event, SlurPlacement::Auto),
        Err(DocumentError::UnknownEvent(missing))
    );
    assert_eq!(b.document(), &before);
    // The next object takes the ID the failed calls did not consume: the same as in the clone.
    let second = quarter(&mut b, fl, bars[0], d(1, 4)).event;
    assert_eq!(second, missing);
    let document = b.build().unwrap();
    assert_eq!(document.slurs_at(a.event), []);
    assert_eq!(document.comes_before(a.event, second), Ok(true));
    // An event the document does not have.
    let (mut other, other_bars, other_fl) = flute(1);
    for i in 0..3 {
        quarter(&mut other, other_fl, other_bars[0], d(i, 4));
    }
    let unknown = order(&other, other_fl, other_bars[0])[2];
    assert_eq!(
        document.comes_before(a.event, unknown),
        Err(DocumentError::UnknownEvent(unknown))
    );
}
