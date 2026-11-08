// SPDX-License-Identifier: GPL-3.0-or-later

//! The played notes, rule by rule (ADR-0027 §2–§8; SCALETTA F1.13b, N1–N10): small scores built
//! with the real builder, each checked against the notes a musician would play.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test helpers: a failed unwrap fails the test"
)]

use orchestra_core_model::{
    Alteration, Arpeggio, Articulation, ArticulationSet, BarId, BarRepeatSpan, BaseValue, Clef,
    Document, DocumentBuilder, DynamicScope, Grace, GraceAttach, GraceKind, InstrumentId, Interval,
    Key, Metronome, Mode, NoteValue, PassSet, PerformanceOffsets, Pitch, Placement, RestValue,
    ScorePoint, StaffId, Step, TempoMark, Tie, VoiceId,
};
use orchestra_core_time::{
    Duration, Meter, MeterChange, MeterMap, Position, Rational, Seconds, Tempo,
};
use orchestra_playback::{INITIAL_VELOCITY, Performance, PlayedNote, Unrolled};

/// A score being built: the builder, its bars, and its one instrument.
struct Score {
    /// The builder.
    b: DocumentBuilder,
    /// The bars.
    bars: Vec<BarId>,
    /// The piano's voice.
    voice: VoiceId,
    /// The piano's staff.
    staff: StaffId,
    /// The piano.
    instrument: InstrumentId,
}

/// `n` bars of 4/4, one one-staff piano.
fn score(n: usize) -> Score {
    let meters = MeterMap::new(MeterChange::new(Meter::new(&[4], 4).unwrap())).unwrap();
    let tempo = Tempo::new(d(1, 4), Rational::from_integer(120).unwrap()).unwrap();
    let mut b = DocumentBuilder::new(
        meters,
        TempoMark::immediate(None, Metronome::shown(tempo)).unwrap(),
        Key::traditional(0, Mode::Major).unwrap(),
    )
    .unwrap();
    let bars = (0..n).map(|_| b.add_bar().unwrap()).collect();
    let track = b
        .add_instrument_track("Piano", Interval::new(0, Rational::ZERO), &[Clef::TREBLE])
        .unwrap();
    let i = b.document().track(track).unwrap().instrument();
    let (voice, staff, instrument) = (i.voices()[0].id(), i.staves()[0].id(), i.id());
    Score {
        b,
        bars,
        voice,
        staff,
        instrument,
    }
}

/// `n/den` of a whole note.
fn d(n: i64, den: i64) -> Duration {
    Duration::from_rational(Rational::new(n, den).unwrap())
}

/// The position `n/den` whole notes from the start.
fn p(n: i64, den: i64) -> Position {
    Position::from_rational(Rational::new(n, den).unwrap())
}

/// A plain value.
fn v(base: BaseValue) -> NoteValue {
    NoteValue::plain(base)
}

/// C4, E4, G4.
fn pitch(step: Step) -> Pitch {
    Pitch::new(step, Alteration::NATURAL, 4)
}

impl Score {
    /// A single note in a bar at an offset; returns its event and notehead.
    fn note(
        &mut self,
        bar: usize,
        offset: Duration,
        value: NoteValue,
    ) -> (orchestra_core_model::EventId, orchestra_core_model::NoteId) {
        let ids = self
            .b
            .add_chord(
                Placement::new(self.voice, self.bars[bar], offset),
                value,
                &[pitch(Step::C)],
            )
            .unwrap();
        (ids.event, ids.notes[0])
    }

    /// A grace note of this kind before (or after) a point.
    fn grace(&mut self, bar: usize, offset: Duration, kind: GraceKind, attach: GraceAttach) {
        self.b
            .add_grace_chord(
                Placement::new(self.voice, self.bars[bar], offset),
                Grace { kind, attach },
                v(BaseValue::Eighth),
                &[pitch(Step::D)],
            )
            .unwrap();
    }
}

/// The played notes of the piano.
fn played(s: Score) -> Vec<PlayedNote> {
    let instrument = s.instrument;
    let doc: Document = s.b.build().unwrap();
    let path = Unrolled::new(&doc).unwrap();
    Performance::new(&doc, &path)
        .unwrap()
        .notes(instrument)
        .to_vec()
}

/// The 16-bit velocity of an integer level of the 0–127 scale (ADR-0027 §4: bit replication).
fn up(v: u16) -> u16 {
    (v << 9) | (v << 2) | (v >> 5)
}

/// `n/den` seconds.
fn secs(n: i64, den: i64) -> Seconds {
    Seconds::from_rational(Rational::new(n, den).unwrap())
}

/// (start, end) of every played note.
fn spans(notes: &[PlayedNote]) -> Vec<(Position, Position)> {
    notes.iter().map(|n| (n.start, n.end)).collect()
}

/// Four quarters: four notes, at their written places, at C4 = 60, *mf* (N9).
#[test]
fn plain_notes_play_as_written() {
    let mut s = score(1);
    for k in 0..4 {
        s.note(0, d(k, 4), v(BaseValue::Quarter));
    }
    let notes = played(s);
    assert_eq!(
        spans(&notes),
        [
            (p(0, 4), p(1, 4)),
            (p(1, 4), p(2, 4)),
            (p(2, 4), p(3, 4)),
            (p(3, 4), p(4, 4))
        ]
    );
    assert!(
        notes
            .iter()
            .all(|n| n.pitch == Rational::from_integer(60).unwrap()
                && n.velocity == INITIAL_VELOCITY)
    );
}

/// A quarter tied across the barline is one note (ADR-0027 §2).
#[test]
fn a_tie_chain_is_one_note() {
    let mut s = score(2);
    let (_, first) = s.note(0, d(3, 4), v(BaseValue::Quarter));
    let (_, second) = s.note(1, Duration::ZERO, v(BaseValue::Quarter));
    s.b.set_tie(first, Tie::To(second)).unwrap();
    let notes = played(s);
    assert_eq!(spans(&notes), [(p(3, 4), p(5, 4))]);
    assert_eq!(notes[0].note, first);
}

/// N1: a tie into the bar after a repeat is followed only on the pass that plays that bar next.
/// Bars 0–1 repeated: on the first pass bar 1's last quarter is followed by bar 0 again, and ends
/// at its barline; on the second pass it goes on into bar 2.
#[test]
fn a_tie_is_followed_only_where_its_target_sounds_next() {
    let mut s = score(3);
    s.b.add_repeat_end(s.bars[1], 2).unwrap();
    let (_, last) = s.note(1, d(3, 4), v(BaseValue::Quarter));
    let (_, target) = s.note(2, Duration::ZERO, v(BaseValue::Quarter));
    s.b.set_tie(last, Tie::To(target)).unwrap();
    let notes = played(s);
    // Path 0, 1, 0, 1, 2: bar 1 starts at 1 and at 3, bar 2 at 4.
    assert_eq!(spans(&notes), [(p(7, 4), p(8, 4)), (p(15, 4), p(17, 4))]);
}

/// N3: a staccato on the last notehead of a chain shortens that notehead only (a quarter tied to a
/// staccato quarter: a quarter and an eighth).
#[test]
fn staccato_shortens_the_last_note_of_a_chain() {
    let mut s = score(1);
    let (_, a) = s.note(0, Duration::ZERO, v(BaseValue::Quarter));
    let (e, b) = s.note(0, d(1, 4), v(BaseValue::Quarter));
    s.b.set_tie(a, Tie::To(b)).unwrap();
    s.b.set_articulations(e, ArticulationSet::EMPTY.with(Articulation::Staccato))
        .unwrap();
    assert_eq!(spans(&played(s)), [(p(0, 4), p(3, 8))]);
}

/// N4: tenuto with staccato is a portato (3/4); tenuto alone is never shortened; an accent makes
/// the note 20% louder.
#[test]
fn tenuto_staccato_and_accent() {
    let mut s = score(1);
    let (portato, _) = s.note(0, Duration::ZERO, v(BaseValue::Half));
    let (accent, _) = s.note(0, d(1, 2), v(BaseValue::Half));
    s.b.set_articulations(
        portato,
        ArticulationSet::EMPTY
            .with(Articulation::Tenuto)
            .with(Articulation::Staccato),
    )
    .unwrap();
    s.b.set_articulations(accent, ArticulationSet::EMPTY.with(Articulation::Accent))
        .unwrap();
    let notes = played(s);
    assert_eq!(spans(&notes), [(p(0, 1), p(3, 8)), (p(1, 2), p(1, 1))]);
    assert_eq!(notes[0].velocity, INITIAL_VELOCITY);
    // H15, §15: an accent is +22 on the 0–127 scale at the attack (mf 75 → 97), settling back to mf
    // in 0.25 s (§15).
    assert_eq!(notes[1].velocity, up(97));
    assert_eq!(notes[1].settle_to, INITIAL_VELOCITY);
    assert_eq!(notes[1].settle_in, secs(1, 4));
    assert_eq!(notes[1].hold, Seconds::ZERO);
}

/// G7, N5: an acciaccatura sounds before the beat, taking 1/32 from the previous note; the main
/// note starts on its beat.
#[test]
fn an_acciaccatura_takes_time_from_the_previous_note() {
    let mut s = score(1);
    s.note(0, Duration::ZERO, v(BaseValue::Quarter));
    s.grace(0, d(1, 4), GraceKind::Acciaccatura, GraceAttach::Before);
    s.note(0, d(1, 4), v(BaseValue::Quarter));
    assert_eq!(
        spans(&played(s)),
        [
            (p(0, 1), p(7, 32)),
            (p(7, 32), p(8, 32)),
            (p(1, 4), p(2, 4))
        ]
    );
}

/// N5: after a rest the acciaccatura sounds in the silence; before the very first note it starts
/// before time 0.
#[test]
fn an_acciaccatura_after_a_rest_and_at_the_start() {
    let mut s = score(1);
    s.grace(
        0,
        Duration::ZERO,
        GraceKind::Acciaccatura,
        GraceAttach::Before,
    );
    s.note(0, Duration::ZERO, v(BaseValue::Quarter));
    s.b.add_rest(
        Placement::new(s.voice, s.bars[0], d(1, 4)),
        RestValue::Value(v(BaseValue::Quarter)),
    )
    .unwrap();
    s.grace(0, d(2, 4), GraceKind::Acciaccatura, GraceAttach::Before);
    s.note(0, d(2, 4), v(BaseValue::Half));
    let zero = Position::ZERO;
    assert_eq!(
        spans(&played(s)),
        [
            (zero.checked_sub(d(1, 32)).unwrap(), zero),
            (p(0, 1), p(1, 4)),
            (p(15, 32), p(16, 32)),
            (p(1, 2), p(1, 1))
        ]
    );
}

/// G7: an appoggiatura sounds on the beat and takes 2/3 of a dotted main note.
#[test]
fn an_appoggiatura_takes_time_from_its_main_note() {
    let mut s = score(1);
    s.grace(
        0,
        Duration::ZERO,
        GraceKind::Appoggiatura,
        GraceAttach::Before,
    );
    s.note(
        0,
        Duration::ZERO,
        NoteValue::new(BaseValue::Quarter, 1).unwrap(),
    );
    // The dotted quarter lasts 3/8; the appoggiatura takes 2/3 of it, 1/4.
    assert_eq!(spans(&played(s)), [(p(0, 1), p(1, 4)), (p(1, 4), p(3, 8))]);
}

/// N6: an `After` grace takes 1/32 from the end of the note it follows.
#[test]
fn an_after_grace_ends_the_note_it_follows() {
    let mut s = score(1);
    s.note(0, Duration::ZERO, v(BaseValue::Half));
    s.grace(0, d(1, 2), GraceKind::Acciaccatura, GraceAttach::After);
    assert_eq!(
        spans(&played(s)),
        [(p(0, 1), p(15, 32)), (p(15, 32), p(16, 32))]
    );
}

/// N7: a downward arpeggio starts from the highest note, 1/64 apart, all ending together.
#[test]
fn a_downward_arpeggio() {
    let mut s = score(1);
    let ids =
        s.b.add_chord(
            Placement::new(s.voice, s.bars[0], Duration::ZERO),
            v(BaseValue::Half),
            &[pitch(Step::C), pitch(Step::E), pitch(Step::G)],
        )
        .unwrap();
    s.b.set_arpeggio(ids.event, Some(Arpeggio::Down)).unwrap();
    let mut notes = played(s);
    notes.sort_by_key(|n| std::cmp::Reverse(n.pitch));
    assert_eq!(
        spans(&notes),
        [
            (p(0, 64), p(1, 2)),
            (p(1, 64), p(1, 2)),
            (p(2, 64), p(1, 2))
        ]
    );
}

/// N8 and §8: the offsets are added after the rules (an onset 1/64 early, a length 1/16 longer, a
/// velocity 1 000 louder); a note never lasts less than 1/1024.
#[test]
fn offsets_and_bounds() {
    let mut s = score(1);
    let (_, a) = s.note(0, d(1, 4), v(BaseValue::Quarter));
    s.b.set_offsets(
        a,
        PerformanceOffsets {
            onset: d(-1, 64),
            length: d(1, 16),
            velocity: 1000,
        },
    )
    .unwrap();
    let (_, b) = s.note(0, d(3, 4), v(BaseValue::Sixteenth));
    s.b.set_offsets(
        b,
        PerformanceOffsets {
            onset: Duration::ZERO,
            length: d(-1, 8),
            velocity: -100_000,
        },
    )
    .unwrap();
    let notes = played(s);
    assert_eq!(notes[0].start, p(15, 64));
    assert_eq!(
        notes[0].end,
        p(1, 2)
            .checked_add(d(-1, 64))
            .unwrap()
            .checked_add(d(1, 16))
            .unwrap()
    );
    assert_eq!(notes[0].velocity, INITIAL_VELOCITY + 1000);
    assert_eq!(notes[1].end, p(3, 4).checked_add(d(1, 1024)).unwrap());
    assert_eq!(notes[1].velocity, 1);
}

/// P4: a "tacet 1st time" over bar 1 of a repeated section leaves out its notes on the first pass.
#[test]
fn pass_rules_silence_notes() {
    let mut s = score(2);
    s.b.add_repeat_end(s.bars[1], 2).unwrap();
    s.b.add_pass_rule(
        s.instrument,
        ScorePoint::start_of(s.bars[1]),
        ScorePoint::new(s.bars[1], d(1, 1)),
        DynamicScope::Instrument,
        PassSet::of(&[2]).unwrap(),
        None,
    )
    .unwrap();
    s.note(1, Duration::ZERO, v(BaseValue::Whole));
    // Path 0, 1, 0, 1: only the second play of bar 1, from 3.
    assert_eq!(spans(&played(s)), [(p(3, 1), p(4, 1))]);
}

/// P3: a bar repeat plays the notes of its source bar, with the pass of those notes.
#[test]
fn bar_repeats_play_the_source_notes() {
    let mut s = score(2);
    let (_, note) = s.note(0, Duration::ZERO, v(BaseValue::Whole));
    let staff = s.staff;
    s.b.add_bar_repeat(staff, s.bars[1], 1, BarRepeatSpan::One)
        .unwrap();
    let notes = played(s);
    assert_eq!(spans(&notes), [(p(0, 1), p(1, 1)), (p(1, 1), p(2, 1))]);
    assert_eq!((notes[1].note, notes[1].pass), (note, 1));
}

/// N6: an `After` grace of one note and an acciaccatura of the next at the same point: the `After`
/// grace first, then the acciaccatura, both taken from the first note; the second note on time.
#[test]
fn after_graces_come_before_the_next_acciaccatura() {
    let mut s = score(1);
    s.note(0, Duration::ZERO, v(BaseValue::Half));
    s.grace(0, d(1, 2), GraceKind::Acciaccatura, GraceAttach::After);
    s.grace(0, d(1, 2), GraceKind::Acciaccatura, GraceAttach::Before);
    s.note(0, d(1, 2), v(BaseValue::Half));
    assert_eq!(
        spans(&played(s)),
        [
            (p(0, 1), p(14, 32)),
            (p(14, 32), p(15, 32)),
            (p(15, 32), p(16, 32)),
            (p(1, 2), p(1, 1))
        ]
    );
}

/// ADR-0028 §5: an offset of one pass applies to that pass only, added to the inline offsets. A
/// whole note in a bar played twice, with an inline onset of 1/32 and, on its second pass, 1/16
/// later, 1/8 shorter and 1000 steps louder.
#[test]
fn a_pass_offset_changes_only_its_pass() {
    let mut s = score(1);
    let (_, note) = s.note(0, Duration::ZERO, v(BaseValue::Whole));
    s.b.add_repeat_end(s.bars[0], 2).unwrap();
    s.b.set_offsets(
        note,
        PerformanceOffsets {
            onset: d(1, 32),
            ..PerformanceOffsets::NONE
        },
    )
    .unwrap();
    s.b.set_pass_offsets(
        note,
        1,
        PerformanceOffsets {
            onset: d(1, 16),
            length: d(-1, 8),
            velocity: 1000,
        },
    )
    .unwrap();
    let notes = played(s);
    assert_eq!(
        spans(&notes),
        [(p(1, 32), p(33, 32)), (p(35, 32), p(63, 32))]
    );
    assert_eq!(notes[1].pass, 1);
    assert_eq!(
        i32::from(notes[1].velocity) - i32::from(notes[0].velocity),
        1000
    );
}

/// T4: on a staff with a bar repeat, the copy has its own pass, so an entry of pass 1 moves the
/// copy and leaves the original alone.
#[test]
fn a_pass_offset_tells_a_bar_repeat_from_its_source() {
    let mut s = score(2);
    let (_, note) = s.note(0, Duration::ZERO, v(BaseValue::Whole));
    let staff = s.staff;
    s.b.add_bar_repeat(staff, s.bars[1], 1, BarRepeatSpan::One)
        .unwrap();
    s.b.set_pass_offsets(
        note,
        1,
        PerformanceOffsets {
            onset: d(-1, 16),
            ..PerformanceOffsets::NONE
        },
    )
    .unwrap();
    assert_eq!(
        spans(&played(s)),
        [(p(0, 1), p(1, 1)), (p(15, 16), p(31, 16))]
    );
}

/// T3: along a tie chain, the onset comes from the first notehead's entry, the lengths from every
/// notehead's entry in its own pass. A half tied to a half across the barline, pass 0: the first
/// notehead's entry starts the note 1/32 later, the second notehead's entry makes it 1/16 longer
/// and its onset is ignored.
#[test]
fn pass_offsets_along_a_tie_chain() {
    let mut s = score(2);
    let (_, first) = s.note(0, d(1, 2), v(BaseValue::Half));
    let (_, second) = s.note(1, Duration::ZERO, v(BaseValue::Half));
    s.b.set_tie(first, Tie::To(second)).unwrap();
    s.b.set_pass_offsets(
        first,
        0,
        PerformanceOffsets {
            onset: d(1, 32),
            ..PerformanceOffsets::NONE
        },
    )
    .unwrap();
    s.b.set_pass_offsets(
        second,
        0,
        PerformanceOffsets {
            onset: d(1, 4),
            length: d(1, 16),
            velocity: 0,
        },
    )
    .unwrap();
    assert_eq!(spans(&played(s)), [(p(17, 32), p(51, 32))]);
}
