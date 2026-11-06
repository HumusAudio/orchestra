// SPDX-License-Identifier: GPL-3.0-or-later

//! A small document that uses **every kind of saved object** (ADR-0029 S7, S12), for the tests of
//! the project file (and, in F1.15c, the source of the "everything" golden file).
//!
//! 12 bars: a pickup of a quarter (uncounted), 4/4, then 6/8 grouped 3+3 from bar 7 with a hemiola
//! (2+2+2) in bar 8, and an irregular last bar of 3/8; a numbering restart. Three instruments:
//! a piano (two staves, three voices: a voice whose home is the lower staff besides the two of the
//! builder), a flute and a horn in F (transposing, with key overrides). Every event kind (chords,
//! rests, a whole-bar rest, graces before and after, a nested tuplet), every ornament, ties to a
//! note and "let ring", offsets, articulations, an arpeggio, fermatas on a chord and a rest, a
//! microtone; clef, key and tempo changes (immediate with a hidden metronome, gradual with a
//! starting tempo, text only); dynamics of every kind (a level, an accent mark, a free text with a
//! level, one per voice), hairpins (wedge and text with niente, one per voice); a slur; a repeat
//! with two voltas, a pass rule, a segno, a Fine and a D.S. al Fine, a bar repeat; a pass-specific
//! offset, a per-track tolerance and a non-default numbering style.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::too_many_lines,
    reason = "test fixture: a long list of small, independent parts; a failed unwrap fails the test"
)]
#![allow(
    dead_code,
    reason = "shared by several test files, each using its part"
)]

use orchestra_core_model::{
    Alteration, Arpeggio, Articulation, ArticulationSet, BarNumbering, BarRepeatSpan, BaseValue,
    Clef, Document, DocumentBuilder, DynamicLevel, DynamicMark, DynamicPlacement, DynamicScope,
    Fermata, Grace, GraceAttach, GraceKind, GradualMark, HairpinKind, HairpinShape, HairpinStyle,
    Interval, JumpFires, JumpSpec, JumpTarget, Key, KeyOverride, KeySignature, MarkerKind,
    Metronome, Mode, NoteValue, Ornament, OtherDynamic, PassSet, PerformanceOffsets, Pitch,
    Placement, RepeatsAfterJump, RestValue, ScorePoint, SlurPlacement, Step, TempoMark,
    TempoMarkChange, Tie, Tolerance, TremoloStrokes, Tuplet,
};
use orchestra_core_time::{
    BarIndex, BarKind, BeatGrouping, Duration, IrregularBar, Meter, MeterChange, MeterMap,
    Rational, Tempo,
};

/// The duration `n/den` of a whole note.
pub(crate) fn d(n: i64, den: i64) -> Duration {
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

/// The meter map: pickup, 4/4, 6/8 (3+3) from bar 7, hemiola in bar 8, last bar of 3/8.
fn meters() -> MeterMap {
    let mut m = MeterMap::new(MeterChange::new(Meter::new(&[4], 4).unwrap())).unwrap();
    m.set_irregular_bar(
        BarIndex::new(0),
        IrregularBar {
            length: d(1, 4),
            kind: BarKind::AlignEnd,
        },
    )
    .unwrap();
    m.set_uncounted(BarIndex::new(0), true);
    m.set_numbering_restart(BarIndex::new(5), Some(20));
    m.set_meter_change(
        BarIndex::new(7),
        MeterChange::with_grouping(
            Meter::new(&[6], 8).unwrap(),
            BeatGrouping::new(&[3, 3], 8).unwrap(),
        ),
    )
    .unwrap();
    m.set_bar_grouping(BarIndex::new(8), BeatGrouping::new(&[2, 2, 2], 8).unwrap())
        .unwrap();
    m.set_irregular_bar(
        BarIndex::new(11),
        IrregularBar {
            length: d(3, 8),
            kind: BarKind::AlignStart,
        },
    )
    .unwrap();
    m
}

/// The document (see the module documentation).
pub(crate) fn everything() -> Document {
    let initial = TempoMark::immediate(Some("Andante"), Metronome::shown(tempo(72))).unwrap();
    let mut b = DocumentBuilder::new(
        meters(),
        initial,
        Key::traditional(-3, Mode::Minor).unwrap(),
    )
    .unwrap();
    b.set_bar_numbering(BarNumbering::SameAsFirstLettered);
    let bars: Vec<_> = (0..12).map(|_| b.add_bar().unwrap()).collect();
    let unison = Interval::new(0, Rational::ZERO);
    let piano = b
        .add_instrument_track("Piano", unison, &[Clef::TREBLE, Clef::BASS])
        .unwrap();
    let flute = b
        .add_instrument_track("Flauto", unison, &[Clef::TREBLE])
        .unwrap();
    let horn = b
        .add_instrument_track(
            "Horn in F",
            Interval::new(-4, Rational::from_integer(-7).unwrap()),
            &[Clef::TREBLE],
        )
        .unwrap();
    let doc = b.document();
    let (piano_id, rh, lh, upper, lower) = {
        let i = doc.track(piano).unwrap().instrument();
        (
            i.id(),
            i.voices()[0].id(),
            i.voices()[1].id(),
            i.staves()[0].id(),
            i.staves()[1].id(),
        )
    };
    let (flute_id, fl, fl_staff) = {
        let i = doc.track(flute).unwrap().instrument();
        (i.id(), i.voices()[0].id(), i.staves()[0].id())
    };
    let (horn_id, hn) = {
        let i = doc.track(horn).unwrap().instrument();
        (i.id(), i.voices()[0].id())
    };
    let extra = b.add_voice(lower).unwrap();
    let q = NoteValue::plain(BaseValue::Quarter);
    let e = NoteValue::plain(BaseValue::Eighth);
    let h = NoteValue::plain(BaseValue::Half);

    // The flute: a pickup, every ornament, a microtone, a tie, a rest with a fermata, a tremolo
    // with offsets and a fermata; bar 9 is the source of the bar repeat of bar 10.
    b.add_chord(
        Placement::new(fl, bars[0], Duration::ZERO),
        q,
        &[p(Step::D, 5)],
    )
    .unwrap();
    let ornaments = [
        Ornament::Trill {
            auxiliary: Some(Alteration::chromatic(1).unwrap()),
        },
        Ornament::Turn,
        Ornament::InvertedTurn,
        Ornament::Mordent,
    ];
    let micro = Pitch::new(
        Step::E,
        Alteration::new(Rational::new(-1, 2).unwrap()).unwrap(),
        5,
    );
    let mut flute_bar_1 = Vec::new();
    for (i, (ornament, pitch)) in ornaments
        .into_iter()
        .zip([micro, p(Step::F, 5), p(Step::G, 5), p(Step::A, 5)])
        .enumerate()
    {
        let ids = b
            .add_chord(
                Placement::new(fl, bars[1], d(i64::try_from(i).unwrap(), 4)),
                q,
                &[pitch],
            )
            .unwrap();
        b.set_ornament(ids.event, Some(ornament)).unwrap();
        flute_bar_1.push(ids);
    }
    let tied = b
        .add_chord(
            Placement::new(fl, bars[2], Duration::ZERO),
            q,
            &[p(Step::A, 5)],
        )
        .unwrap();
    b.set_ornament(tied.event, Some(Ornament::UpperMordent))
        .unwrap();
    b.set_tie(flute_bar_1[3].notes[0], Tie::To(tied.notes[0]))
        .unwrap();
    let rest = b
        .add_rest(Placement::new(fl, bars[2], d(1, 2)), RestValue::Value(h))
        .unwrap();
    b.set_fermata(rest, Some(Fermata::Normal)).unwrap();
    let long = b
        .add_chord(
            Placement::new(fl, bars[3], Duration::ZERO),
            NoteValue::plain(BaseValue::Whole),
            &[p(Step::B, 5)],
        )
        .unwrap();
    b.set_ornament(
        long.event,
        Some(Ornament::Tremolo(TremoloStrokes::new(3).unwrap())),
    )
    .unwrap();
    b.set_fermata(long.event, Some(Fermata::Long)).unwrap();
    b.set_offsets(
        long.notes[0],
        PerformanceOffsets {
            onset: d(-1, 64),
            length: d(1, 32),
            velocity: -400,
        },
    )
    .unwrap();
    b.add_chord(
        Placement::new(fl, bars[9], Duration::ZERO),
        q,
        &[p(Step::C, 6)],
    )
    .unwrap();
    b.add_bar_repeat(fl_staff, bars[10], 1, BarRepeatSpan::One)
        .unwrap();

    // The piano's right hand: a chord with articulations and an arpeggio, a grace before the
    // second beat, a slur; in bar 2 a let-ring half note with a grace after it; in bar 4 a
    // triplet with a nested duplet.
    let chord = b
        .add_chord(
            Placement::new(rh, bars[1], Duration::ZERO),
            q,
            &[p(Step::C, 4), p(Step::E, 4), p(Step::G, 4)],
        )
        .unwrap();
    b.set_articulations(
        chord.event,
        ArticulationSet::EMPTY
            .with(Articulation::Staccato)
            .with(Articulation::Accent),
    )
    .unwrap();
    b.set_arpeggio(chord.event, Some(Arpeggio::Up)).unwrap();
    b.add_grace_chord(
        Placement::new(rh, bars[1], d(1, 4)),
        Grace {
            kind: GraceKind::Acciaccatura,
            attach: GraceAttach::Before,
        },
        e,
        &[p(Step::D, 4)],
    )
    .unwrap();
    let second = b
        .add_chord(Placement::new(rh, bars[1], d(1, 4)), q, &[p(Step::E, 4)])
        .unwrap();
    b.add_slur(chord.event, second.event, SlurPlacement::Above)
        .unwrap();
    let ring = b
        .add_chord(
            Placement::new(rh, bars[2], Duration::ZERO),
            h,
            &[p(Step::G, 4)],
        )
        .unwrap();
    b.set_tie(ring.notes[0], Tie::LetRing).unwrap();
    b.add_grace_chord(
        Placement::new(rh, bars[2], d(1, 2)),
        Grace {
            kind: GraceKind::Appoggiatura,
            attach: GraceAttach::After,
        },
        e,
        &[p(Step::A, 4)],
    )
    .unwrap();
    let triplet = b
        .add_tuplet(rh, Tuplet::new(3, 2, q).unwrap(), None)
        .unwrap();
    let inner = b
        .add_tuplet(rh, Tuplet::new(2, 3, e).unwrap(), Some(triplet))
        .unwrap();
    b.add_chord(
        Placement::new(rh, bars[4], Duration::ZERO).in_tuplet(triplet),
        q,
        &[p(Step::A, 4)],
    )
    .unwrap();
    b.add_chord(
        Placement::new(rh, bars[4], d(1, 6)).in_tuplet(inner),
        e,
        &[p(Step::B, 4)],
    )
    .unwrap();
    b.set_pass_offsets(
        chord.notes[0],
        1,
        PerformanceOffsets {
            onset: Duration::ZERO,
            length: Duration::ZERO,
            velocity: 300,
        },
    )
    .unwrap();

    // The left hand: a whole-bar rest, a clef change; the extra voice under it.
    b.add_rest(
        Placement::new(lh, bars[1], Duration::ZERO),
        RestValue::WholeBar,
    )
    .unwrap();
    b.add_clef_change(lower, bars[5], Duration::ZERO, Clef::TENOR)
        .unwrap();
    b.add_chord(
        Placement::new(lh, bars[5], Duration::ZERO),
        h,
        &[p(Step::C, 3)],
    )
    .unwrap();
    b.add_chord(
        Placement::new(extra, bars[1], Duration::ZERO),
        h,
        &[p(Step::E, 3)],
    )
    .unwrap();

    // The horn: no key signature, then its own key, then back to the score's.
    b.add_chord(
        Placement::new(hn, bars[4], Duration::ZERO),
        q,
        &[p(Step::C, 5)],
    )
    .unwrap();
    b.add_key_override(
        horn_id,
        bars[0],
        Duration::ZERO,
        KeyOverride::Signature(KeySignature::NoSignature),
    )
    .unwrap();
    b.add_key_override(
        horn_id,
        bars[9],
        Duration::ZERO,
        KeyOverride::Signature(KeySignature::Key(Key::traditional(2, Mode::Major).unwrap())),
    )
    .unwrap();
    b.add_key_override(horn_id, bars[11], Duration::ZERO, KeyOverride::FollowScore)
        .unwrap();

    // Score-wide marks: a key change, tempo marks of every kind.
    b.add_key_change(
        bars[7],
        Duration::ZERO,
        Key::traditional(-1, Mode::Dorian).unwrap(),
    )
    .unwrap();
    b.add_tempo_mark(
        ScorePoint::start_of(bars[4]),
        TempoMark::immediate(None, Metronome::hidden(tempo(80))).unwrap(),
    )
    .unwrap();
    b.add_tempo_mark(
        ScorePoint::start_of(bars[5]),
        TempoMark::new(
            Some("rit."),
            Some(TempoMarkChange::Gradual(GradualMark {
                from: Some(Metronome::shown(tempo(76))),
                to: tempo(60),
                end: ScorePoint::start_of(bars[6]),
            })),
        )
        .unwrap(),
    )
    .unwrap();
    b.add_tempo_mark(
        ScorePoint::start_of(bars[8]),
        TempoMark::new(Some("con moto"), None).unwrap(),
    )
    .unwrap();

    // Dynamics and hairpins.
    b.add_dynamic(
        piano_id,
        ScorePoint::start_of(bars[1]),
        DynamicPlacement::instrument(lower),
        DynamicMark::Level(DynamicLevel::Mf),
    )
    .unwrap();
    b.add_dynamic(
        piano_id,
        ScorePoint::new(bars[1], d(1, 4)),
        DynamicPlacement::voice(rh, upper),
        DynamicMark::Sfz,
    )
    .unwrap();
    b.add_dynamic(
        flute_id,
        ScorePoint::start_of(bars[1]),
        DynamicPlacement::instrument(fl_staff),
        DynamicMark::Other(OtherDynamic::new("dolce", Some(DynamicLevel::Mp)).unwrap()),
    )
    .unwrap();
    b.add_hairpin(
        piano_id,
        ScorePoint::new(bars[1], d(1, 2)),
        ScorePoint::start_of(bars[2]),
        DynamicPlacement::instrument(lower),
        HairpinShape::CRESCENDO,
    )
    .unwrap();
    b.add_hairpin(
        flute_id,
        ScorePoint::start_of(bars[3]),
        ScorePoint::start_of(bars[4]),
        DynamicPlacement::voice(fl, fl_staff),
        HairpinShape {
            kind: HairpinKind::Diminuendo,
            style: HairpinStyle::Text,
            niente: true,
        },
    )
    .unwrap();

    // The form: bars 1–2 repeated with two voltas (2 and 3), the right hand only the second time
    // in bar 1; segno at bar 4, Fine at the end of bar 6, D.S. al Fine at the end of bar 11.
    b.add_repeat_start(bars[1]).unwrap();
    b.add_repeat_end(bars[2], 2).unwrap();
    b.add_volta(bars[2], bars[2], PassSet::of(&[1]).unwrap(), true, None)
        .unwrap();
    b.add_volta(
        bars[3],
        bars[3],
        PassSet::of(&[2]).unwrap(),
        false,
        Some("2."),
    )
    .unwrap();
    b.add_pass_rule(
        piano_id,
        ScorePoint::start_of(bars[1]),
        ScorePoint::start_of(bars[2]),
        DynamicScope::Voice(rh),
        PassSet::of(&[2]).unwrap(),
        Some("2nd time only"),
    )
    .unwrap();
    let segno = b.add_marker(bars[4], MarkerKind::Segno, None).unwrap();
    let fine = b.add_marker(bars[6], MarkerKind::Fine, None).unwrap();
    b.add_jump(
        bars[11],
        JumpSpec {
            to: JumpTarget::Marker(segno),
            until: Some(fine),
            then: None,
            repeats: RepeatsAfterJump::Skip,
            fires: JumpFires::AfterRepeats,
        },
        Some("D.S. al Fine"),
    )
    .unwrap();

    // Settings: the flute's own tolerance.
    b.set_tolerance(Some(flute), Some(Tolerance::Millis(25)))
        .unwrap();
    b.build().unwrap()
}
