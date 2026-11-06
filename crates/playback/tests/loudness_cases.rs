// SPDX-License-Identifier: GPL-3.0-or-later

//! The loudness of the played notes, rule by rule (ADR-0027 §4 and §14, H1–H17; SCALETTA F1.13c):
//! small scores built with the real builder, each checked against the numbers of the ADR worked
//! out by hand on the 0–127 scale (*pppp* 10, *ppp* 23, *pp* 36, *p* 49, *mp* 62, *mf* 75, *f* 88,
//! *ff* 101, *fff* 114, *ffff* 127).

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test helpers: a failed unwrap fails the test"
)]

use orchestra_core_model::{
    Alteration, Articulation, ArticulationSet, BarId, BaseValue, Clef, Document, DocumentBuilder,
    DynamicLevel, DynamicMark, DynamicPlacement, EventId, Grace, GraceAttach, GraceKind,
    HairpinShape, InstrumentId, Interval, Key, Metronome, Mode, NoteValue, PerformanceOffsets,
    Pitch, Placement, ScorePoint, StaffId, Step, TempoMark, VoiceId,
};
use orchestra_core_time::{Duration, Meter, MeterChange, MeterMap, Rational, Seconds, Tempo};
use orchestra_playback::{INITIAL_VELOCITY, Performance, PlayedNote, Unrolled};

/// A score being built: the builder, its bars, and its one instrument with one voice.
struct Score {
    /// The builder.
    b: DocumentBuilder,
    /// The bars.
    bars: Vec<BarId>,
    /// The instrument's first voice.
    voice: VoiceId,
    /// Its staff.
    staff: StaffId,
    /// The instrument.
    instrument: InstrumentId,
}

/// `n` bars of 4/4, one one-staff instrument.
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
        .add_instrument_track("Violin", Interval::new(0, Rational::ZERO), &[Clef::TREBLE])
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

/// `n/den` seconds.
fn secs(n: i64, den: i64) -> Seconds {
    Seconds::from_rational(Rational::new(n, den).unwrap())
}

/// The 16-bit velocity of an integer level of the 0–127 scale (ADR-0027 §4: bit replication).
fn up(v: u16) -> u16 {
    (v << 9) | (v << 2) | (v >> 5)
}

/// The 16-bit velocity of the level `num/den` of the 0–127 scale (H8), computed here with plain
/// floating point as an independent check: between the two neighbouring integers' values, rounded
/// to the nearest. The levels of these tests are quarters and halves, exact in `f64`.
fn upf(num: i64, den: i64) -> u16 {
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "small exact test values"
    )]
    {
        let x = num as f64 / den as f64;
        let low = x.floor();
        let (lo, hi) = (f64::from(up(low as u16)), f64::from(up(low as u16 + 1)));
        (lo + (x - low) * (hi - lo) + 0.5).floor() as u16
    }
}

impl Score {
    /// A quarter note C4 in a bar at an offset, in a voice; returns its event.
    fn note_in(&mut self, voice: VoiceId, bar: usize, offset: Duration) -> EventId {
        self.b
            .add_chord(
                Placement::new(voice, self.bars[bar], offset),
                NoteValue::plain(BaseValue::Quarter),
                &[Pitch::new(Step::C, Alteration::NATURAL, 4)],
            )
            .unwrap()
            .event
    }

    /// Four quarters in each of the first `bars` bars of the first voice.
    fn quarters(&mut self, bars: usize) {
        for bar in 0..bars {
            for k in 0..4 {
                self.note_in(self.voice, bar, d(k, 4));
            }
        }
    }

    /// A dynamic of the whole instrument.
    fn dynamic(&mut self, bar: usize, offset: Duration, mark: DynamicMark) {
        self.b
            .add_dynamic(
                self.instrument,
                ScorePoint::new(self.bars[bar], offset),
                DynamicPlacement::instrument(self.staff),
                mark,
            )
            .unwrap();
    }

    /// A level of the whole instrument.
    fn level(&mut self, bar: usize, offset: Duration, level: DynamicLevel) {
        self.dynamic(bar, offset, DynamicMark::Level(level));
    }

    /// A hairpin of the whole instrument, from (bar, offset) to (bar, offset).
    fn hairpin(&mut self, from: (usize, Duration), to: (usize, Duration), shape: HairpinShape) {
        self.b
            .add_hairpin(
                self.instrument,
                ScorePoint::new(self.bars[from.0], from.1),
                ScorePoint::new(self.bars[to.0], to.1),
                DynamicPlacement::instrument(self.staff),
                shape,
            )
            .unwrap();
    }

    /// The played notes of the instrument.
    fn played(self) -> Vec<PlayedNote> {
        let instrument = self.instrument;
        let doc: Document = self.b.build().unwrap();
        let path = Unrolled::new(&doc).unwrap();
        Performance::new(&doc, &path)
            .unwrap()
            .notes(instrument)
            .to_vec()
    }
}

/// The attack velocities of the notes.
fn attacks(notes: &[PlayedNote]) -> Vec<u16> {
    notes.iter().map(|n| n.velocity).collect()
}

/// Whether a note has no attack profile (H14): settling level equal to the attack, no times.
fn flat(note: &PlayedNote) -> bool {
    note.settle_to == note.velocity && note.hold == Seconds::ZERO && note.settle_in == Seconds::ZERO
}

/// G4: the ten levels of the table; more *p*s or *f*s play as the extremes; before any dynamic
/// *mf* (H1); a note without an accent has no profile.
#[test]
fn levels_play_the_table() {
    let mut s = score(1);
    s.quarters(1);
    s.level(0, d(1, 4), DynamicLevel::Pppppp);
    s.level(0, d(2, 4), DynamicLevel::P);
    s.level(0, d(3, 4), DynamicLevel::Fffff);
    let notes = s.played();
    assert_eq!(attacks(&notes), [INITIAL_VELOCITY, up(10), up(49), up(127)]);
    assert_eq!(INITIAL_VELOCITY, up(75));
    assert!(notes.iter().all(flat));
}

/// H1: the dynamic in force is followed along the path: after the repeat back to bar 0, which has
/// no dynamic, the *f* written in bar 1 stays.
#[test]
fn the_dynamic_in_force_follows_the_path() {
    let mut s = score(2);
    s.quarters(2);
    s.level(1, d(0, 1), DynamicLevel::F);
    s.b.add_repeat_end(s.bars[1], 2).unwrap();
    let notes = s.played();
    // Bar 0 (mf), bar 1 (f), bar 0 again (f), bar 1 (f).
    let mut expected = vec![up(75); 4];
    expected.extend([up(88); 12]);
    assert_eq!(attacks(&notes), expected);
}

/// H3, H16: an *sf* in *p* attacks two levels up (*f*) and settles back to *p* in 0.6 s, only on
/// the note starting at its point; an *sf* where no note starts does nothing; in *fff* it stops at
/// *ffff*; *rf* is one level, *sffz* three.
#[test]
fn sforzato_and_the_like() {
    let mut s = score(2);
    s.quarters(2);
    s.level(0, d(0, 1), DynamicLevel::P);
    s.dynamic(0, d(1, 4), DynamicMark::Sf);
    s.dynamic(0, d(5, 8), DynamicMark::Sfz);
    s.dynamic(0, d(3, 4), DynamicMark::Rf);
    s.level(1, d(0, 1), DynamicLevel::Fff);
    s.dynamic(1, d(1, 4), DynamicMark::Sffz);
    let notes = s.played();
    assert_eq!(
        attacks(&notes),
        [
            up(49),
            up(75),
            up(49),
            up(62),
            up(114),
            up(127),
            up(114),
            up(114)
        ]
    );
    assert_eq!(notes[1].settle_to, up(49));
    assert_eq!(notes[1].settle_in, secs(3, 5));
    assert_eq!(notes[1].hold, Seconds::ZERO);
    assert_eq!(notes[3].settle_in, secs(2, 5));
    assert_eq!(notes[5].settle_to, up(114));
    assert_eq!(notes[5].settle_in, secs(4, 5));
    assert!(flat(&notes[2]) && flat(&notes[4]));
}

/// H4, H16: *fp* is *f* held 0.3 s then *p* in 0.1 s, and *p* afterwards; *pf* the reverse;
/// *sfpp* is *ff* then *pp* in 0.3 s; without a note at its point the second level holds at once.
#[test]
fn forte_piano_and_piano_forte() {
    let mut s = score(2);
    s.quarters(2);
    s.dynamic(0, d(0, 1), DynamicMark::Fp);
    s.dynamic(0, d(2, 4), DynamicMark::Pf);
    s.dynamic(1, d(0, 1), DynamicMark::Sfpp);
    // An fp between two notes: p from there.
    s.dynamic(1, d(3, 8), DynamicMark::Fp);
    let notes = s.played();
    let fp = &notes[0];
    assert_eq!(
        (fp.velocity, fp.hold, fp.settle_in, fp.settle_to),
        (up(88), secs(3, 10), secs(1, 10), up(49))
    );
    assert_eq!(notes[1].velocity, up(49));
    let pf = &notes[2];
    assert_eq!(
        (pf.velocity, pf.hold, pf.settle_in, pf.settle_to),
        (up(49), secs(3, 10), secs(1, 10), up(88))
    );
    assert_eq!(notes[3].velocity, up(88));
    let sfpp = &notes[4];
    assert_eq!(
        (sfpp.velocity, sfpp.hold, sfpp.settle_in, sfpp.settle_to),
        (up(101), Seconds::ZERO, secs(3, 10), up(36))
    );
    assert_eq!(notes[5].velocity, up(36));
    assert_eq!(attacks(&notes[6..]), [up(49), up(49)]);
}

/// H5: *niente* is velocity 1.
#[test]
fn niente_is_velocity_one() {
    let mut s = score(1);
    s.quarters(1);
    s.dynamic(0, d(0, 1), DynamicMark::Niente);
    let notes = s.played();
    assert!(notes.iter().all(|n| n.velocity == 1 && flat(n)));
}

/// H6, H7, H8: a crescendo from *p* to the *f* at its end is linear in the written position; the
/// *f* holds after it.
#[test]
fn a_crescendo_to_its_written_target() {
    let mut s = score(2);
    s.quarters(2);
    s.level(0, d(0, 1), DynamicLevel::P);
    s.hairpin((0, d(0, 1)), (1, d(0, 1)), HairpinShape::CRESCENDO);
    s.level(1, d(0, 1), DynamicLevel::F);
    let notes = s.played();
    // 49 + 39 × k / 4: 49, 58.75, 68.5, 78.25; then 88.
    assert_eq!(
        attacks(&notes),
        [
            up(49),
            upf(235, 4),
            upf(137, 2),
            upf(313, 4),
            up(88),
            up(88),
            up(88),
            up(88)
        ]
    );
}

/// H6, H7: without a dynamic at its end (the *pp* two bars later is outside the window, and a
/// written *p* at the end of a crescendo from *p* goes the wrong way), a crescendo goes one level
/// up and that level holds after it.
#[test]
fn a_crescendo_without_a_fitting_target() {
    let mut s = score(4);
    s.quarters(4);
    s.level(0, d(0, 1), DynamicLevel::P);
    // Bar 0: crescendo to nothing written: p → mp.
    s.hairpin((0, d(0, 1)), (0, d(1, 2)), HairpinShape::CRESCENDO);
    // Bar 2: crescendo from mp, a written p at its end goes the wrong way: mp → mf; then the p.
    s.hairpin((2, d(0, 1)), (2, d(1, 2)), HairpinShape::CRESCENDO);
    s.level(2, d(1, 2), DynamicLevel::P);
    s.level(3, d(3, 4), DynamicLevel::Pp);
    let notes = s.played();
    assert_eq!(
        attacks(&notes),
        [
            up(49),
            upf(111, 2),
            up(62),
            up(62),
            up(62),
            up(62),
            up(62),
            up(62),
            up(62),
            upf(137, 2),
            up(49),
            up(49),
            up(49),
            up(49),
            up(49),
            up(36)
        ]
    );
}

/// H7: a dynamic inside a hairpin splits it: p → mf at bar 1, then mf → f at bar 2.
#[test]
fn a_dynamic_inside_a_hairpin_splits_it() {
    let mut s = score(3);
    s.quarters(3);
    s.level(0, d(0, 1), DynamicLevel::P);
    s.hairpin((0, d(0, 1)), (2, d(0, 1)), HairpinShape::CRESCENDO);
    s.level(1, d(0, 1), DynamicLevel::Mf);
    s.level(2, d(0, 1), DynamicLevel::F);
    let notes = s.played();
    // Bar 0: 49 + 26 k / 4; bar 1: 75 + 13 k / 4; bar 2: 88.
    assert_eq!(notes[2].velocity, up(62));
    assert_eq!(notes[4].velocity, up(75));
    assert_eq!(notes[6].velocity, upf(163, 2));
    assert_eq!(notes[8].velocity, up(88));
}

/// H5: a diminuendo *al niente* without a target goes to velocity 1.
#[test]
fn a_diminuendo_al_niente() {
    let mut s = score(2);
    s.quarters(2);
    s.level(0, d(0, 1), DynamicLevel::F);
    s.hairpin(
        (0, d(0, 1)),
        (1, d(0, 1)),
        HairpinShape {
            niente: true,
            ..HairpinShape::DIMINUENDO
        },
    );
    let notes = s.played();
    // 88 × (1 − k/4): 88, 66, 44, 22; then nothing.
    assert_eq!(attacks(&notes[..4]), [up(88), up(66), up(44), up(22)]);
    assert!(notes[4..].iter().all(|n| n.velocity == 1));
}

/// H8: a hairpin played twice starts each time from the level in force: mf → f the first time,
/// f → ff the second.
#[test]
fn a_hairpin_played_twice_starts_from_the_level_in_force() {
    let mut s = score(2);
    s.quarters(2);
    s.hairpin((0, d(0, 1)), (1, d(0, 1)), HairpinShape::CRESCENDO);
    s.b.add_repeat_end(s.bars[1], 2).unwrap();
    let notes = s.played();
    assert_eq!(notes[0].velocity, up(75));
    assert_eq!(notes[4].velocity, up(88));
    assert_eq!(notes[8].velocity, up(88));
    assert_eq!(notes[12].velocity, up(101));
}

/// G5, H9: a voice's dynamic wins over the instrument's at the same point, a voice's hairpin over
/// the instrument's on the same note; the other voice follows the instrument's.
#[test]
fn the_voice_wins_over_the_instrument() {
    let mut s = score(3);
    let second = s.b.add_voice(s.staff).unwrap();
    s.level(0, d(0, 1), DynamicLevel::F);
    s.b.add_dynamic(
        s.instrument,
        ScorePoint::new(s.bars[0], d(0, 1)),
        DynamicPlacement::voice(second, s.staff),
        DynamicMark::Level(DynamicLevel::P),
    )
    .unwrap();
    // The instrument diminuendos in bar 1 (f → mf), the second voice crescendos (p → mp).
    s.hairpin((1, d(0, 1)), (2, d(0, 1)), HairpinShape::DIMINUENDO);
    s.b.add_hairpin(
        s.instrument,
        ScorePoint::new(s.bars[1], d(0, 1)),
        ScorePoint::new(s.bars[2], d(0, 1)),
        DynamicPlacement::voice(second, s.staff),
        HairpinShape::CRESCENDO,
    )
    .unwrap();
    for bar in 0..2 {
        s.note_in(s.voice, bar, d(1, 2));
        s.b.add_chord(
            Placement::new(second, s.bars[bar], d(1, 2)),
            NoteValue::plain(BaseValue::Quarter),
            &[Pitch::new(Step::E, Alteration::NATURAL, 4)],
        )
        .unwrap();
    }
    let first = s.voice;
    let notes = s.played();
    let of = |voice: VoiceId| -> Vec<u16> {
        notes
            .iter()
            .filter(|n| n.voice == voice)
            .map(|n| n.velocity)
            .collect()
    };
    // Halfway through bar 1: f → mf at 81.5, p → mp at 55.5.
    assert_eq!(of(first), [up(88), upf(163, 2)]);
    assert_eq!(of(second), [up(49), upf(111, 2)]);
}

/// H10: a grace note takes the level in force at its main note, without the *sf* written there.
#[test]
fn grace_notes_take_the_level_without_the_accent() {
    let mut s = score(1);
    s.level(0, d(0, 1), DynamicLevel::P);
    s.dynamic(0, d(1, 2), DynamicMark::Sf);
    s.b.add_grace_chord(
        Placement::new(s.voice, s.bars[0], d(1, 2)),
        Grace {
            kind: GraceKind::Appoggiatura,
            attach: GraceAttach::Before,
        },
        NoteValue::plain(BaseValue::Eighth),
        &[Pitch::new(Step::D, Alteration::NATURAL, 4)],
    )
    .unwrap();
    s.note_in(s.voice, 0, d(1, 2));
    let notes = s.played();
    // The appoggiatura first (on the beat), then its main note.
    assert_eq!(attacks(&notes), [up(49), up(75)]);
    assert!(flat(&notes[0]));
}

/// H15, H17: articulation accents are fixed steps (accent +15 in 0.2 s, marcato +22 in 0.25 s);
/// with an *sf* the attack adds both and the longer time wins; an unstress is −7 on the whole
/// note; the velocity offset moves the attack and the settling level.
#[test]
fn articulation_accents_are_fixed_steps() {
    let mut s = score(1);
    s.level(0, d(0, 1), DynamicLevel::Pp);
    s.dynamic(0, d(1, 4), DynamicMark::Sf);
    let set = |a: Articulation| ArticulationSet::EMPTY.with(a);
    let marks = [
        Articulation::Accent,
        Articulation::Marcato,
        Articulation::Unstress,
        Articulation::Accent,
    ];
    let mut last = None;
    for (k, mark) in (0_i64..).zip(marks) {
        let ids =
            s.b.add_chord(
                Placement::new(s.voice, s.bars[0], d(k, 4)),
                NoteValue::plain(BaseValue::Quarter),
                &[Pitch::new(Step::C, Alteration::NATURAL, 4)],
            )
            .unwrap();
        s.b.set_articulations(ids.event, set(mark)).unwrap();
        last = Some(ids.notes[0]);
    }
    s.b.set_offsets(
        last.unwrap(),
        PerformanceOffsets {
            velocity: 100,
            ..PerformanceOffsets::default()
        },
    )
    .unwrap();
    let notes = s.played();
    // pp (36) + 15 = 51, settling back to 36 in 0.2 s: the same step as at mf.
    assert_eq!(
        (notes[0].velocity, notes[0].settle_to, notes[0].settle_in),
        (up(51), up(36), secs(1, 5))
    );
    // sf in pp: 36 + 26 = 62, + marcato 22 = 84, back to 36 in 0.6 s (the longer).
    assert_eq!(
        (notes[1].velocity, notes[1].settle_to, notes[1].settle_in),
        (up(84), up(36), secs(3, 5))
    );
    // Unstress: 29 for the whole note.
    assert_eq!(notes[2].velocity, up(29));
    assert!(flat(&notes[2]));
    // Accent with a velocity offset of +100 on both levels.
    assert_eq!(
        (notes[3].velocity, notes[3].settle_to),
        (up(51) + 100, up(36) + 100)
    );
}

/// H8: the up-scaling of a fractional level rounds to the nearest, halves up: an eighth into a
/// crescendo from *p* to *mp* (no target written) over a whole bar is at 49 + 13/8 = 50.625, which
/// is 0.625 × 516 = 322.5 steps above 50's value, rounded up to 323.
#[test]
fn fractional_levels_round_to_the_nearest() {
    let mut s = score(2);
    s.level(0, d(0, 1), DynamicLevel::P);
    s.hairpin((0, d(0, 1)), (1, d(0, 1)), HairpinShape::CRESCENDO);
    s.b.add_chord(
        Placement::new(s.voice, s.bars[0], d(1, 8)),
        NoteValue::plain(BaseValue::Eighth),
        &[Pitch::new(Step::C, Alteration::NATURAL, 4)],
    )
    .unwrap();
    let notes = s.played();
    assert_eq!(up(51) - up(50), 516);
    assert_eq!(notes[0].velocity, up(50) + 323);
    assert_eq!(notes[0].velocity, upf(405, 8));
}

/// H6: the target is looked for only within the bar holding the end: the *f* at the start of the
/// next bar is outside the window of a crescendo ending halfway through bar 0, which therefore
/// goes one level up (*p* → *mp*), and the *f* comes only where it is written.
#[test]
fn the_target_is_looked_for_within_the_bar_of_the_end() {
    let mut s = score(2);
    s.quarters(2);
    s.level(0, d(0, 1), DynamicLevel::P);
    s.hairpin((0, d(0, 1)), (0, d(1, 2)), HairpinShape::CRESCENDO);
    s.level(1, d(0, 1), DynamicLevel::F);
    let notes = s.played();
    assert_eq!(
        attacks(&notes[..5]),
        [up(49), upf(111, 2), up(62), up(62), up(88)]
    );
}
