// SPDX-License-Identifier: GPL-3.0-or-later

//! The documents of the golden files of version 1 (ADR-0029 S12, ADR-0010 §3; SCALETTA F1.15c):
//! one small document per kind of saved object, plus the "everything" document of
//! `everything.rs`. Each golden file `tests/golden/v1/<name>.orchestra` was written once from the
//! document built here and is **never regenerated**: the test loads it and compares the result
//! with the document built by this code, IDs included. If the model, the builder or the writer
//! ever change what these documents are or how version 1 is read, the golden test fails: that is
//! the compatibility contract.
//!
//! The documents are kept small and readable, so that a reviewer can check each file by eye
//! against the code that builds it.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::too_many_lines,
    reason = "test fixtures: lists of small, independent parts; a failed unwrap fails the test"
)]
#![allow(
    dead_code,
    reason = "shared by several test files, each using its part"
)]

use std::num::NonZeroU64;

use orchestra_core_model::{
    Alteration, Arpeggio, Articulation, ArticulationSet, BarId, BarNumbering, BarRepeatSpan,
    BaseValue, Clef, ClefSign, Document, DocumentBuilder, DynamicLevel, DynamicMark,
    DynamicPlacement, DynamicScope, Fermata, Grace, GraceAttach, GraceKind, GradualMark,
    HairpinKind, HairpinShape, HairpinStyle, InstrumentId, Interval, JumpFires, JumpSpec,
    JumpTarget, Key, KeyOverride, KeySignature, MarkerKind, Metronome, Mode, NoteValue, Ornament,
    OtherDynamic, PassSet, PerformanceOffsets, Pitch, Placement, RepeatsAfterJump, RestValue,
    ScorePoint, SlurPlacement, StaffId, Step, TempoMark, TempoMarkChange, Tie, Tolerance, TrackId,
    TremoloStrokes, Tuplet, TupletBracket, TupletDisplay, TupletNumber, VoiceId,
};
use orchestra_core_time::{
    BarIndex, BarKind, BeatGrouping, Duration, IrregularBar, Meter, MeterChange, MeterMap,
    Rational, Tempo,
};

use super::everything::everything;

/// A golden file's name and the function that builds its document.
pub(crate) type Golden = (&'static str, fn() -> Document);

/// The golden documents of version 1, by file name (without `.orchestra`), in the order the files
/// were written.
pub(crate) const GOLDEN_V1: [Golden; 15] = [
    ("minimal", minimal),
    ("meter", meter),
    ("instruments", instruments),
    ("events", events),
    ("tuplets", tuplets),
    ("graces", graces),
    ("markings", markings),
    ("ties_and_offsets", ties_and_offsets),
    ("slurs", slurs),
    ("dynamics", dynamics),
    ("tempo_and_keys", tempo_and_keys),
    ("form", form),
    ("bar_repeats_and_pass_rules", bar_repeats_and_pass_rules),
    ("settings_and_counter", settings_and_counter),
    ("everything", everything),
];

/// The duration `n/den` of a whole note.
fn d(n: i64, den: i64) -> Duration {
    Duration::from_rational(Rational::new(n, den).unwrap())
}

/// A quarter-note tempo.
fn tempo(bpm: i64) -> Tempo {
    Tempo::new(d(1, 4), Rational::from_integer(bpm).unwrap()).unwrap()
}

/// A natural pitch.
fn p(step: Step, octave: i8) -> Pitch {
    Pitch::new(step, Alteration::NATURAL, octave)
}

/// A plain note value.
fn v(base: BaseValue) -> NoteValue {
    NoteValue::plain(base)
}

/// What most documents start from: 4/4, ♩ = 100, C major, `bars` bars and one flute.
struct Base {
    /// The builder.
    b: DocumentBuilder,
    /// The bars.
    bars: Vec<BarId>,
    /// The flute's track.
    track: TrackId,
    /// The flute's instrument.
    instrument: InstrumentId,
    /// Its staff.
    staff: StaffId,
    /// Its voice.
    voice: VoiceId,
}

/// The base of `bars` bars with the meter map `meters`.
fn base_with(meters: MeterMap, bars: usize) -> Base {
    let initial = TempoMark::immediate(None, Metronome::shown(tempo(100))).unwrap();
    let mut b =
        DocumentBuilder::new(meters, initial, Key::traditional(0, Mode::Major).unwrap()).unwrap();
    let bars: Vec<_> = (0..bars).map(|_| b.add_bar().unwrap()).collect();
    let track = b
        .add_instrument_track("Flute", Interval::new(0, Rational::ZERO), &[Clef::TREBLE])
        .unwrap();
    let (instrument, staff, voice) = {
        let i = b.document().track(track).unwrap().instrument();
        (i.id(), i.staves()[0].id(), i.voices()[0].id())
    };
    Base {
        b,
        bars,
        track,
        instrument,
        staff,
        voice,
    }
}

/// The base of `bars` bars of 4/4.
fn base(bars: usize) -> Base {
    base_with(
        MeterMap::new(MeterChange::new(Meter::new(&[4], 4).unwrap())).unwrap(),
        bars,
    )
}

/// The smallest document: one empty bar of 4/4 and one flute.
fn minimal() -> Document {
    base(1).b.build().unwrap()
}

/// The meter map: an additive meter, a change with a grouping, a pickup, a free bar, an aligned
/// irregular bar, a per-bar grouping (a hemiola), uncounted bars, a numbering restart.
fn meter() -> Document {
    let mut m = MeterMap::new(MeterChange::new(Meter::new(&[3, 2, 2], 8).unwrap())).unwrap();
    m.set_irregular_bar(
        BarIndex::new(0),
        IrregularBar {
            length: d(1, 8),
            kind: BarKind::AlignEnd,
        },
    )
    .unwrap();
    m.set_uncounted(BarIndex::new(0), true);
    m.set_meter_change(
        BarIndex::new(2),
        MeterChange::with_grouping(
            Meter::new(&[9], 8).unwrap(),
            BeatGrouping::new(&[3, 2, 2, 2], 8).unwrap(),
        ),
    )
    .unwrap();
    m.set_bar_grouping(BarIndex::new(3), BeatGrouping::new(&[3, 3, 3], 8).unwrap())
        .unwrap();
    m.set_irregular_bar(
        BarIndex::new(4),
        IrregularBar {
            length: d(5, 4),
            kind: BarKind::Free,
        },
    )
    .unwrap();
    m.set_uncounted(BarIndex::new(4), true);
    m.set_meter_change(
        BarIndex::new(5),
        MeterChange::new(Meter::new(&[4], 4).unwrap()),
    )
    .unwrap();
    m.set_irregular_bar(
        BarIndex::new(6),
        IrregularBar {
            length: d(1, 2),
            kind: BarKind::AlignStart,
        },
    )
    .unwrap();
    m.set_numbering_restart(BarIndex::new(5), Some(100));
    base_with(m, 7).b.build().unwrap()
}

/// Instruments: a transposing one, one with three staves and an extra voice on its middle staff,
/// clefs of every sign with an octave shift, clef changes, key overrides of every kind.
fn instruments() -> Document {
    let mut s = base(4);
    let b = &mut s.b;
    let clarinet = b
        .add_instrument_track(
            "Clarinet in B♭",
            Interval::new(-1, Rational::from_integer(-2).unwrap()),
            &[Clef::TREBLE],
        )
        .unwrap();
    let organ = b
        .add_instrument_track(
            "Organ",
            Interval::new(0, Rational::ZERO),
            &[
                Clef::TREBLE,
                Clef::BASS,
                Clef::new(ClefSign::F, 4, -1).unwrap(),
            ],
        )
        .unwrap();
    b.add_instrument_track(
        "Percussion",
        Interval::new(0, Rational::ZERO),
        &[Clef::PERCUSSION],
    )
    .unwrap();
    let (clarinet_id, organ_staves) = {
        let doc = b.document();
        (
            doc.track(clarinet).unwrap().instrument().id(),
            doc.track(organ)
                .unwrap()
                .instrument()
                .staves()
                .iter()
                .map(orchestra_core_model::Staff::id)
                .collect::<Vec<_>>(),
        )
    };
    b.add_voice(organ_staves[1]).unwrap();
    b.add_clef_change(organ_staves[0], s.bars[2], Duration::ZERO, Clef::ALTO)
        .unwrap();
    b.add_clef_change(organ_staves[1], s.bars[3], d(1, 2), Clef::TENOR)
        .unwrap();
    b.add_clef_change(
        s.staff,
        s.bars[1],
        Duration::ZERO,
        Clef::new(ClefSign::G, 2, 1).unwrap(),
    )
    .unwrap();
    b.add_key_override(
        clarinet_id,
        s.bars[0],
        Duration::ZERO,
        KeyOverride::Signature(KeySignature::Key(Key::traditional(2, Mode::Major).unwrap())),
    )
    .unwrap();
    b.add_key_override(
        clarinet_id,
        s.bars[2],
        Duration::ZERO,
        KeyOverride::Signature(KeySignature::NoSignature),
    )
    .unwrap();
    b.add_key_override(
        clarinet_id,
        s.bars[3],
        Duration::ZERO,
        KeyOverride::FollowScore,
    )
    .unwrap();
    s.b.build().unwrap()
}

/// Events: chords of several notes, rests of written values, a whole-bar rest, dotted values,
/// the longest and shortest values, every chromatic alteration, microtones, octave -1, an event
/// on another staff of its instrument.
fn events() -> Document {
    let mut s = base(4);
    let piano =
        s.b.add_instrument_track(
            "Piano",
            Interval::new(0, Rational::ZERO),
            &[Clef::TREBLE, Clef::BASS],
        )
        .unwrap();
    let (rh, lower) = {
        let i = s.b.document().track(piano).unwrap().instrument();
        (i.voices()[0].id(), i.staves()[1].id())
    };
    let b = &mut s.b;
    let alt = |semitones: i64| Alteration::chromatic(semitones).unwrap();
    b.add_chord(
        Placement::new(s.voice, s.bars[0], Duration::ZERO),
        NoteValue::new(BaseValue::Quarter, 1).unwrap(),
        &[Pitch::new(Step::F, alt(1), 5)],
    )
    .unwrap();
    b.add_chord(
        Placement::new(s.voice, s.bars[0], d(3, 8)),
        NoteValue::new(BaseValue::Eighth, 0).unwrap(),
        &[Pitch::new(Step::B, alt(-1), 4)],
    )
    .unwrap();
    b.add_rest(
        Placement::new(s.voice, s.bars[0], d(1, 2)),
        RestValue::Value(NoteValue::new(BaseValue::Quarter, 2).unwrap()),
    )
    .unwrap();
    b.add_chord(
        Placement::new(s.voice, s.bars[0], d(15, 16)),
        v(BaseValue::Sixteenth),
        &[Pitch::new(Step::C, alt(2), 5)],
    )
    .unwrap();
    b.add_rest(
        Placement::new(s.voice, s.bars[1], Duration::ZERO),
        RestValue::WholeBar,
    )
    .unwrap();
    // Every alteration of ±3 semitones and two microtones, in one chord each.
    b.add_chord(
        Placement::new(rh, s.bars[0], Duration::ZERO),
        v(BaseValue::Half),
        &[
            Pitch::new(Step::C, alt(-3), 4),
            Pitch::new(Step::D, alt(-2), 4),
            Pitch::new(Step::G, alt(3), 4),
            Pitch::new(
                Step::A,
                Alteration::new(Rational::new(1, 2).unwrap()).unwrap(),
                4,
            ),
            Pitch::new(
                Step::E,
                Alteration::new(Rational::new(-3, 2).unwrap()).unwrap(),
                5,
            ),
        ],
    )
    .unwrap();
    // A note of the right hand's voice drawn on the lower staff.
    b.add_chord(
        Placement::new(rh, s.bars[0], d(1, 2)).on_staff(lower),
        v(BaseValue::Half),
        &[p(Step::C, -1)],
    )
    .unwrap();
    // The extremes of the note values (the longest overfills its bar, a valid state).
    b.add_chord(
        Placement::new(rh, s.bars[2], Duration::ZERO),
        v(BaseValue::OneThousandTwentyFourth),
        &[p(Step::C, 8)],
    )
    .unwrap();
    b.add_chord(
        Placement::new(rh, s.bars[3], Duration::ZERO),
        NoteValue::new(BaseValue::Breve, 1).unwrap(),
        &[p(Step::C, 2)],
    )
    .unwrap();
    s.b.build().unwrap()
}

/// Tuplets: a triplet, a quintuplet with a ratio number and a shown bracket, a tuplet nested in
/// another with a hidden number, a rest inside a tuplet.
fn tuplets() -> Document {
    let mut s = base(2);
    let b = &mut s.b;
    let e = v(BaseValue::Eighth);
    let triplet = b
        .add_tuplet(s.voice, Tuplet::new(3, 2, e).unwrap(), None)
        .unwrap();
    for (i, step) in [Step::C, Step::D, Step::E].into_iter().enumerate() {
        b.add_chord(
            Placement::new(s.voice, s.bars[0], d(i64::try_from(i).unwrap(), 12)).in_tuplet(triplet),
            e,
            &[p(step, 5)],
        )
        .unwrap();
    }
    let s16 = v(BaseValue::Sixteenth);
    let quintuplet = b
        .add_tuplet(
            s.voice,
            Tuplet::new(5, 4, s16).unwrap().with_display(TupletDisplay {
                bracket: TupletBracket::Shown,
                number: TupletNumber::Ratio,
            }),
            None,
        )
        .unwrap();
    for i in 0..4 {
        b.add_chord(
            Placement::new(s.voice, s.bars[0], d(5 + i, 20)).in_tuplet(quintuplet),
            s16,
            &[p(Step::F, 5)],
        )
        .unwrap();
    }
    b.add_rest(
        Placement::new(s.voice, s.bars[0], d(9, 20)).in_tuplet(quintuplet),
        RestValue::Value(s16),
    )
    .unwrap();
    let q = v(BaseValue::Quarter);
    let outer = b
        .add_tuplet(s.voice, Tuplet::new(3, 2, q).unwrap(), None)
        .unwrap();
    let inner = b
        .add_tuplet(
            s.voice,
            Tuplet::new(2, 3, e).unwrap().with_display(TupletDisplay {
                bracket: TupletBracket::Hidden,
                number: TupletNumber::Hidden,
            }),
            Some(outer),
        )
        .unwrap();
    b.add_chord(
        Placement::new(s.voice, s.bars[1], Duration::ZERO).in_tuplet(outer),
        q,
        &[p(Step::G, 4)],
    )
    .unwrap();
    b.add_chord(
        Placement::new(s.voice, s.bars[1], d(1, 6)).in_tuplet(inner),
        e,
        &[p(Step::A, 4)],
    )
    .unwrap();
    b.add_chord(
        Placement::new(s.voice, s.bars[1], d(7, 24)).in_tuplet(inner),
        e,
        &[p(Step::B, 4)],
    )
    .unwrap();
    s.b.build().unwrap()
}

/// Grace notes: two acciaccaturas before a note, an appoggiatura before another, a grace after a
/// note (at its end).
fn graces() -> Document {
    let mut s = base(1);
    let b = &mut s.b;
    let grace = |kind, attach| Grace { kind, attach };
    for step in [Step::A, Step::B] {
        b.add_grace_chord(
            Placement::new(s.voice, s.bars[0], Duration::ZERO),
            grace(GraceKind::Acciaccatura, GraceAttach::Before),
            v(BaseValue::Sixteenth),
            &[p(step, 4)],
        )
        .unwrap();
    }
    b.add_chord(
        Placement::new(s.voice, s.bars[0], Duration::ZERO),
        v(BaseValue::Half),
        &[p(Step::C, 5)],
    )
    .unwrap();
    b.add_grace_chord(
        Placement::new(s.voice, s.bars[0], d(1, 2)),
        grace(GraceKind::Appoggiatura, GraceAttach::After),
        v(BaseValue::Eighth),
        &[p(Step::D, 5)],
    )
    .unwrap();
    b.add_grace_chord(
        Placement::new(s.voice, s.bars[0], d(1, 2)),
        grace(GraceKind::Appoggiatura, GraceAttach::Before),
        v(BaseValue::Eighth),
        &[p(Step::F, 5)],
    )
    .unwrap();
    b.add_chord(
        Placement::new(s.voice, s.bars[0], d(1, 2)),
        v(BaseValue::Half),
        &[p(Step::E, 5)],
    )
    .unwrap();
    s.b.build().unwrap()
}

/// Markings: every articulation (spread over chords), every ornament, every fermata (on chords
/// and on a rest), every arpeggio sign.
fn markings() -> Document {
    let mut s = base(4);
    let b = &mut s.b;
    let q = v(BaseValue::Quarter);
    let mut chords = Vec::new();
    for bar in 0..3 {
        for beat in 0..4 {
            chords.push(
                b.add_chord(
                    Placement::new(s.voice, s.bars[bar], d(beat, 4)),
                    q,
                    &[p(Step::C, 5), p(Step::E, 5)],
                )
                .unwrap()
                .event,
            );
        }
    }
    for (i, articulation) in Articulation::ALL.into_iter().enumerate() {
        b.set_articulations(chords[i], ArticulationSet::EMPTY.with(articulation))
            .unwrap();
    }
    b.set_articulations(
        chords[10],
        ArticulationSet::EMPTY
            .with(Articulation::Tenuto)
            .with(Articulation::Staccato)
            .with(Articulation::Marcato),
    )
    .unwrap();
    let ornaments = [
        Ornament::Trill { auxiliary: None },
        Ornament::Trill {
            auxiliary: Some(Alteration::chromatic(-1).unwrap()),
        },
        Ornament::Trill {
            auxiliary: Some(Alteration::new(Rational::new(1, 2).unwrap()).unwrap()),
        },
        Ornament::Turn,
        Ornament::InvertedTurn,
        Ornament::Mordent,
        Ornament::UpperMordent,
        Ornament::Tremolo(TremoloStrokes::new(1).unwrap()),
        Ornament::Tremolo(TremoloStrokes::new(4).unwrap()),
    ];
    for (i, ornament) in ornaments.into_iter().enumerate() {
        b.set_ornament(chords[i], Some(ornament)).unwrap();
    }
    for (i, fermata) in [Fermata::Short, Fermata::Normal, Fermata::Long]
        .into_iter()
        .enumerate()
    {
        b.set_fermata(chords[i], Some(fermata)).unwrap();
    }
    for (i, arpeggio) in [
        Arpeggio::Plain,
        Arpeggio::Up,
        Arpeggio::Down,
        Arpeggio::NonArpeggio,
    ]
    .into_iter()
    .enumerate()
    {
        b.set_arpeggio(chords[i + 4], Some(arpeggio)).unwrap();
    }
    let pause = b
        .add_rest(
            Placement::new(s.voice, s.bars[3], Duration::ZERO),
            RestValue::WholeBar,
        )
        .unwrap();
    b.set_fermata(pause, Some(Fermata::VeryLong)).unwrap();
    s.b.build().unwrap()
}

/// Ties and offsets: a tie chain across a barline, a tie inside a chord, a let-ring tie,
/// performance offsets of every component, pass-specific offsets on a repeated note.
fn ties_and_offsets() -> Document {
    let mut s = base(3);
    let b = &mut s.b;
    let h = v(BaseValue::Half);
    let first = b
        .add_chord(
            Placement::new(s.voice, s.bars[0], d(1, 2)),
            h,
            &[p(Step::C, 5), p(Step::E, 5)],
        )
        .unwrap();
    let second = b
        .add_chord(
            Placement::new(s.voice, s.bars[1], Duration::ZERO),
            h,
            &[p(Step::C, 5), p(Step::E, 5)],
        )
        .unwrap();
    b.set_tie(first.notes[0], Tie::To(second.notes[0])).unwrap();
    b.set_tie(first.notes[1], Tie::To(second.notes[1])).unwrap();
    let ring = b
        .add_chord(
            Placement::new(s.voice, s.bars[2], Duration::ZERO),
            h,
            &[p(Step::G, 4)],
        )
        .unwrap();
    b.set_tie(ring.notes[0], Tie::LetRing).unwrap();
    b.set_offsets(
        second.notes[0],
        PerformanceOffsets {
            onset: d(-1, 128),
            length: Duration::ZERO,
            velocity: 0,
        },
    )
    .unwrap();
    b.set_offsets(
        ring.notes[0],
        PerformanceOffsets {
            onset: d(1, 96),
            length: d(-1, 16),
            velocity: 2500,
        },
    )
    .unwrap();
    b.add_repeat_start(s.bars[0]).unwrap();
    b.add_repeat_end(s.bars[2], 3).unwrap();
    for (pass, velocity) in [(1, -1000), (2, 1500)] {
        b.set_pass_offsets(
            ring.notes[0],
            pass,
            PerformanceOffsets {
                onset: Duration::ZERO,
                length: d(1, 32),
                velocity,
            },
        )
        .unwrap();
    }
    s.b.build().unwrap()
}

/// Slurs: one per placement, one across a barline, two sharing an end event.
fn slurs() -> Document {
    let mut s = base(2);
    let b = &mut s.b;
    let q = v(BaseValue::Quarter);
    let mut events = Vec::new();
    for bar in 0..2 {
        for beat in 0..4 {
            events.push(
                b.add_chord(
                    Placement::new(s.voice, s.bars[bar], d(beat, 4)),
                    q,
                    &[p(Step::D, 5)],
                )
                .unwrap()
                .event,
            );
        }
    }
    b.add_slur(events[0], events[1], SlurPlacement::Auto)
        .unwrap();
    b.add_slur(events[2], events[5], SlurPlacement::Above)
        .unwrap();
    b.add_slur(events[1], events[7], SlurPlacement::Below)
        .unwrap();
    s.b.build().unwrap()
}

/// Dynamics: every level, every accent mark, a free text with and without a level, one per
/// voice; hairpins of both kinds and styles, with niente, per voice and per instrument.
fn dynamics() -> Document {
    let levels = [
        DynamicLevel::Pppppp,
        DynamicLevel::Ppppp,
        DynamicLevel::Pppp,
        DynamicLevel::Ppp,
        DynamicLevel::Pp,
        DynamicLevel::P,
        DynamicLevel::Mp,
        DynamicLevel::Mf,
        DynamicLevel::F,
        DynamicLevel::Ff,
        DynamicLevel::Fff,
        DynamicLevel::Ffff,
        DynamicLevel::Fffff,
        DynamicLevel::Ffffff,
    ];
    let accents = [
        DynamicMark::Sf,
        DynamicMark::Sfz,
        DynamicMark::Sffz,
        DynamicMark::Fz,
        DynamicMark::Rf,
        DynamicMark::Rfz,
        DynamicMark::Fp,
        DynamicMark::Sfp,
        DynamicMark::Sfpp,
        DynamicMark::Pf,
        DynamicMark::Niente,
    ];
    let mut s = base(8);
    let b = &mut s.b;
    let at = |k: usize| ScorePoint::new(s.bars[k / 4], d(i64::try_from(k % 4).unwrap(), 4));
    let marks = levels
        .into_iter()
        .map(DynamicMark::Level)
        .chain(accents)
        .chain([
            DynamicMark::Other(OtherDynamic::new("dolce", Some(DynamicLevel::Mp)).unwrap()),
            DynamicMark::Other(OtherDynamic::new("più forte", None).unwrap()),
        ]);
    for (k, mark) in marks.enumerate() {
        b.add_dynamic(
            s.instrument,
            at(k),
            DynamicPlacement::instrument(s.staff),
            mark,
        )
        .unwrap();
    }
    b.add_dynamic(
        s.instrument,
        at(0),
        DynamicPlacement::voice(s.voice, s.staff),
        DynamicMark::Level(DynamicLevel::Pp),
    )
    .unwrap();
    b.add_hairpin(
        s.instrument,
        ScorePoint::start_of(s.bars[0]),
        ScorePoint::start_of(s.bars[1]),
        DynamicPlacement::instrument(s.staff),
        HairpinShape::CRESCENDO,
    )
    .unwrap();
    b.add_hairpin(
        s.instrument,
        ScorePoint::start_of(s.bars[1]),
        ScorePoint::new(s.bars[2], d(1, 2)),
        DynamicPlacement::instrument(s.staff),
        HairpinShape {
            kind: HairpinKind::Diminuendo,
            style: HairpinStyle::Text,
            niente: true,
        },
    )
    .unwrap();
    b.add_hairpin(
        s.instrument,
        ScorePoint::start_of(s.bars[0]),
        ScorePoint::start_of(s.bars[3]),
        DynamicPlacement::voice(s.voice, s.staff),
        HairpinShape {
            kind: HairpinKind::Crescendo,
            style: HairpinStyle::Wedge,
            niente: true,
        },
    )
    .unwrap();
    s.b.build().unwrap()
}

/// Tempo and keys: a text on the initial tempo, an immediate change with a hidden metronome, a
/// gradual change with and without a starting tempo, a text-only mark, a non-quarter beat unit
/// and a fractional tempo; key changes of every mode, at a bar's start and inside a bar.
fn tempo_and_keys() -> Document {
    let mut s = base(6);
    let b = &mut s.b;
    let dotted = Tempo::new(d(3, 8), Rational::new(201, 2).unwrap()).unwrap();
    b.add_tempo_mark(
        ScorePoint::start_of(s.bars[1]),
        TempoMark::immediate(Some("Più mosso"), Metronome::hidden(dotted)).unwrap(),
    )
    .unwrap();
    b.add_tempo_mark(
        ScorePoint::start_of(s.bars[2]),
        TempoMark::new(
            Some("accel."),
            Some(TempoMarkChange::Gradual(GradualMark {
                from: None,
                to: tempo(144),
                end: ScorePoint::new(s.bars[3], d(1, 2)),
            })),
        )
        .unwrap(),
    )
    .unwrap();
    b.add_tempo_mark(
        ScorePoint::start_of(s.bars[4]),
        TempoMark::new(
            Some("Meno mosso, poco a poco rit."),
            Some(TempoMarkChange::Gradual(GradualMark {
                from: Some(Metronome::shown(tempo(90))),
                to: tempo(60),
                end: ScorePoint::start_of(s.bars[5]),
            })),
        )
        .unwrap(),
    )
    .unwrap();
    b.add_tempo_mark(
        ScorePoint::new(s.bars[5], d(1, 4)),
        TempoMark::new(Some("cantabile"), None).unwrap(),
    )
    .unwrap();
    let modes = [
        Mode::Major,
        Mode::Dorian,
        Mode::Phrygian,
        Mode::Lydian,
        Mode::Mixolydian,
        Mode::Minor,
        Mode::Locrian,
        Mode::None,
    ];
    for (i, mode) in modes.into_iter().enumerate() {
        let fifths = i8::try_from(i).unwrap() * 2 - 7;
        b.add_key_change(
            s.bars[i / 2 + 1],
            d(i64::try_from(i % 2).unwrap(), 2),
            Key::traditional(fifths, mode).unwrap(),
        )
        .unwrap();
    }
    s.b.build().unwrap()
}

/// The form: a repeat with three voltas (the last open, with its own text), a repeat without a
/// start, segno, coda, to-coda and Fine markers, a D.S. al Fine that plays the repeats again, a
/// D.C. al Coda, and a non-default numbering style. (No jump here is taken on a given pass,
/// `"fires": {"on_pass": n}`: its text form is covered by the schema's own tests.)
fn form() -> Document {
    let mut s = base(14);
    let b = &mut s.b;
    b.set_bar_numbering(BarNumbering::SameAsFirst);
    let bars = s.bars.clone();
    // Bars 0–1 repeated from the start of the score (an end repeat without a start).
    b.add_repeat_end(bars[1], 2).unwrap();
    // Bars 2–3 with three voltas: 4 (1.), 5 (2.), 6 (3., open).
    b.add_repeat_start(bars[2]).unwrap();
    b.add_repeat_end(bars[4], 3).unwrap();
    b.add_repeat_end(bars[5], 3).unwrap();
    b.add_volta(bars[4], bars[4], PassSet::of(&[1]).unwrap(), true, None)
        .unwrap();
    b.add_volta(bars[5], bars[5], PassSet::of(&[2]).unwrap(), true, None)
        .unwrap();
    b.add_volta(
        bars[6],
        bars[6],
        PassSet::of(&[3]).unwrap(),
        false,
        Some("Last time"),
    )
    .unwrap();
    let segno = b.add_marker(bars[7], MarkerKind::Segno, Some("𝄋")).unwrap();
    let to_coda = b.add_marker(bars[8], MarkerKind::ToCoda, None).unwrap();
    let fine = b.add_marker(bars[9], MarkerKind::Fine, None).unwrap();
    b.add_jump(
        bars[10],
        JumpSpec {
            to: JumpTarget::Marker(segno),
            until: Some(fine),
            then: None,
            repeats: RepeatsAfterJump::Play,
            fires: JumpFires::AfterRepeats,
        },
        None,
    )
    .unwrap();
    let coda = b.add_marker(bars[12], MarkerKind::Coda, None).unwrap();
    b.add_jump(
        bars[11],
        JumpSpec {
            to: JumpTarget::Start,
            until: Some(to_coda),
            then: Some(coda),
            repeats: RepeatsAfterJump::Skip,
            fires: JumpFires::AfterRepeats,
        },
        Some("D.C. al Coda"),
    )
    .unwrap();
    s.b.build().unwrap()
}

/// Bar repeats of every span (one bar repeating bar 0, two bars repeating bars 2–3, four bars
/// repeating bars 8–11) and pass rules per instrument and per voice inside a section played four
/// times (bars 6–7).
fn bar_repeats_and_pass_rules() -> Document {
    let mut s = base(16);
    let b = &mut s.b;
    let q = v(BaseValue::Quarter);
    for bar in [0, 2, 3, 6, 7, 8, 9, 10, 11] {
        b.add_chord(
            Placement::new(s.voice, s.bars[bar], Duration::ZERO),
            q,
            &[p(Step::E, 5)],
        )
        .unwrap();
    }
    b.add_bar_repeat(s.staff, s.bars[1], 1, BarRepeatSpan::One)
        .unwrap();
    b.add_bar_repeat(s.staff, s.bars[4], 2, BarRepeatSpan::Two)
        .unwrap();
    b.add_bar_repeat(s.staff, s.bars[12], 4, BarRepeatSpan::Four)
        .unwrap();
    b.add_repeat_start(s.bars[6]).unwrap();
    b.add_repeat_end(s.bars[7], 4).unwrap();
    b.add_pass_rule(
        s.instrument,
        ScorePoint::start_of(s.bars[6]),
        ScorePoint::start_of(s.bars[7]),
        DynamicScope::Instrument,
        PassSet::of(&[2, 4]).unwrap(),
        None,
    )
    .unwrap();
    b.add_pass_rule(
        s.instrument,
        ScorePoint::start_of(s.bars[7]),
        ScorePoint::new(s.bars[7], d(1, 2)),
        DynamicScope::Voice(s.voice),
        PassSet::range(1, 3).unwrap(),
        Some("tacet last time"),
    )
    .unwrap();
    s.b.build().unwrap()
}

/// Settings and the counter: a project tolerance in milliseconds, a per-track override in musical
/// time, and an **exhausted** ID counter (`"next_id": null`), which the builder cannot make (it
/// would need 2⁶⁴ IDs): it goes through the saved description instead.
fn settings_and_counter() -> Document {
    let mut s = base(1);
    s.b.set_tolerance(None, Some(Tolerance::Millis(40)))
        .unwrap();
    s.b.set_tolerance(Some(s.track), Some(Tolerance::Musical(d(1, 48))))
        .unwrap();
    let mut parts = s.b.build().unwrap().to_parts();
    parts.next_id = None;
    let _: Option<NonZeroU64> = parts.next_id;
    Document::from_parts(&parts).unwrap()
}
