// SPDX-License-Identifier: GPL-3.0-or-later

//! The **notes** of the full preset: how each voice fills its bars (rhythm cells, pitches in the
//! key, ties, tuplets, grace notes, ornaments, articulations, arpeggios, fermatas, offsets, slurs).
//!
//! # How a voice is written
//! Bar by bar, in written order ([`write_voice`]):
//! 1. a few bars are special: the pickup, the free cadenza bar, the bar-repeat bars (no events),
//!    the final bar (a long note with a fermata for everybody);
//! 2. at every phrase start the voice decides whether it rests for the whole phrase (a **tacet**,
//!    as often as its [`Family`] says), and then writes whole-bar rests;
//! 3. otherwise it picks a **rhythm cell** for its role and bar shape ([`cell`]), and writes one
//!    event per element of the cell, choosing each pitch from the bar's harmony ([`Harmony`]).
//!
//! # Units
//! Cells are written in **sixteenths** (a 4/4 bar is 16); offsets inside the bar are placed in
//! **ticks of 1/96 of a whole note**, so that sixteenths (6 ticks), triplet eighths (8), dotted
//! duplets (18) and the cadenza's nested tuplets (12, 16) are all whole numbers.
//!
//! # Coverage by rotation
//! Some markings must appear at least once (every articulation, every ornament: the coverage test
//! checks it). Rather than hope that random choices reach them, they **rotate**: the n-th accent
//! takes the n-th articulation of the list, every 40th melody note the next ornament. That is both
//! plausible (they are spread through the score) and certain.
//!
//! # Complexity
//! Linear in the number of events written: each one costs one builder call plus O(1) choices.

use orchestra_core_model::{
    Alteration, Arpeggio, Articulation, ArticulationSet, BarId, BaseValue, DocumentBuilder,
    EventId, Fermata, Grace, GraceAttach, GraceKind, Mode, NoteId, NoteValue, Ornament,
    PerformanceOffsets, Pitch, Placement, RestValue, SlurPlacement, Step, Tie, TremoloStrokes,
    Tuplet, VoiceId,
};
use orchestra_core_time::{Duration, Rational};

use super::ensemble::{Family, InstrumentSpec, Register, Role, VoiceSpec};
use super::plan::{
    self, BarShape, Section, chord_root, is_cadence, movement_shift, phrase_position,
};

/// A small deterministic generator (64-bit xorshift), the same as the plain preset's and for the
/// same reasons (see `plain::Rng`): reproducible forever, on every platform, with no dependency.
/// The seed is a non-zero constant of [`super::score`].
#[derive(Debug)]
pub(super) struct Rng(pub(super) u64);

impl Rng {
    /// The next value in `0..n` (`n > 0`): one xorshift step (13/7/17), then a modulo whose bias
    /// is negligible for the small `n` used here.
    pub(super) fn below(&mut self, n: u64) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0 % n
    }

    /// True with probability `percent`/100.
    fn chance(&mut self, percent: u64) -> bool {
        self.below(100) < percent
    }
}

/// Counters shared by every voice of the score, for the markings that rotate (see the module
/// documentation).
#[derive(Debug, Default)]
pub(super) struct Counters {
    /// Accents written so far: the next one takes `ARTICULATIONS[accents % 10]`.
    accents: usize,
    /// Melody notes written so far: every 40th takes the next ornament.
    melody_notes: u64,
    /// Ornaments written by rotation so far.
    ornaments: usize,
    /// Melody cadences written so far: every second one ends its trill with an after-grace.
    cadences: u64,
    /// Arpeggios written on piano chords so far.
    arpeggios: usize,
}

/// Every articulation of the model, in the order the accents rotate through them.
const ARTICULATIONS: [Articulation; 10] = [
    Articulation::Accent,
    Articulation::Tenuto,
    Articulation::Marcato,
    Articulation::Staccato,
    Articulation::Portato,
    Articulation::Staccatissimo,
    Articulation::Spiccato,
    Articulation::Stress,
    Articulation::Unstress,
    Articulation::SoftAccent,
];

/// The steps, by their index (0 = C … 6 = B): the step of diatonic number `n` is
/// `STEPS[n mod 7]`.
const STEPS: [Step; 7] = [
    Step::C,
    Step::D,
    Step::E,
    Step::F,
    Step::G,
    Step::A,
    Step::B,
];

/// The steps in the order sharps are added to a key signature (F C G D A E B), by index.
const SHARP_ORDER: [i64; 7] = [3, 0, 4, 1, 5, 2, 6];

/// The steps in the order flats are added (B E A D G C F), by index.
const FLAT_ORDER: [i64; 7] = [6, 2, 5, 1, 4, 0, 3];

/// The alteration, in semitones, that a key of `fifths` gives to `step` (index 0–6).
///
/// It counts how many times the step appears among the first `|fifths|` entries of the order of
/// sharps (or flats), cycling through the order: a key of 9 sharps sharpens F and C twice (double
/// sharps), which the written keys of transposing instruments can reach (see
/// `plan::movement_shift`). O(|fifths|), at most 9.
pub(super) fn alteration(step: i64, fifths: i64) -> i64 {
    let order = if fifths >= 0 {
        &SHARP_ORDER
    } else {
        &FLAT_ORDER
    };
    let count = (0..fifths.unsigned_abs())
        .filter(|&k| order[usize::try_from(k % 7).unwrap()] == step)
        .count();
    let count = i64::try_from(count).unwrap();
    if fifths >= 0 { count } else { -count }
}

/// The written pitch of diatonic number `n` in a key of `fifths`: its step, the key's alteration
/// of that step, its octave. O(1).
fn pitch(n: i64, fifths: i64) -> Pitch {
    let step = n.rem_euclid(7);
    let octave = i8::try_from(n.div_euclid(7)).unwrap();
    Pitch::new(
        STEPS[usize::try_from(step).unwrap()],
        Alteration::chromatic(alteration(step, fifths)).unwrap(),
        octave,
    )
}

/// The harmony of one bar for one voice, in its *written* key.
#[derive(Debug, Clone, Copy)]
struct Harmony {
    /// The written key, in fifths (concert key plus the instrument's shift).
    fifths: i64,
    /// The step index (0–6) of the chord's root.
    root: i64,
    /// The chord's scale degree above the tonic (0 = I, 4 = V, …), from `plan::chord_root`.
    degree: i64,
}

impl Harmony {
    /// The harmony of local bar `local` of movement `m` for an instrument whose written key is
    /// `shift` fifths above the concert key.
    ///
    /// The tonic of a major key of `f` fifths is the step `4 f mod 7` (each fifth adds four steps:
    /// C, G, D, …); a minor key's tonic is a sixth above its relative major's (the step + 5). The
    /// root is the tonic plus the phrase's scale degree ([`chord_root`]). O(1).
    fn of(local: u32, m: u32, shift: i8) -> Self {
        let section = Section::of(local);
        let (fifths, mode) = section.key();
        let fifths = i64::from(fifths) + i64::from(movement_shift(m)) + i64::from(shift);
        let tonic = (4 * fifths).rem_euclid(7) + if mode == Mode::Minor { 5 } else { 0 };
        let degree = chord_root(phrase_position(local));
        let root = (tonic + degree).rem_euclid(7);
        Self {
            fifths,
            root,
            degree,
        }
    }

    /// Whether diatonic number `n` is a tone of the chord (its root, third or fifth). O(1).
    fn is_chord_tone(self, n: i64) -> bool {
        matches!((n - self.root).rem_euclid(7), 0 | 2 | 4)
    }

    /// The chord tone nearest to `n` inside `register` (the lower one on a tie), or `n` clamped
    /// into the register if the register holds none (it always does: every register spans more
    /// than an octave or a fifth). O(1): at most seven candidates on each side.
    fn nearest_chord_tone(self, n: i64, register: Register) -> i64 {
        Self::nearest(n, register, |x| self.is_chord_tone(x))
    }

    /// The nearest diatonic number to `n` inside `register` satisfying `wanted`, searching outwards.
    fn nearest(n: i64, register: Register, wanted: impl Fn(i64) -> bool) -> i64 {
        let n = n.clamp(register.0, register.1);
        for distance in 0..=7 {
            for candidate in [n - distance, n + distance] {
                if (register.0..=register.1).contains(&candidate) && wanted(candidate) {
                    return candidate;
                }
            }
        }
        n
    }

    /// The nearest note of pitch class `class` (a step index above the root: 0 root, 4 fifth) to
    /// `n` inside `register`. O(1).
    fn nearest_degree(self, n: i64, class: i64, register: Register) -> i64 {
        Self::nearest(n, register, |x| (x - self.root).rem_euclid(7) == class)
    }

    /// The written pitch of diatonic number `n` in this key.
    fn pitch(self, n: i64) -> Pitch {
        pitch(n, self.fifths)
    }
}

/// The rhythm cell code of a triplet of eighths in the time of a quarter (4 sixteenths).
const TRIPLET: i8 = 100;
/// The rhythm cell code of a duplet of eighths in the time of a dotted quarter (6 sixteenths).
const DUPLET: i8 = 101;

/// The rhythm cells a role chooses from in a bar shape, in sixteenths: a positive number is a
/// note of that length, a negative one a rest. Each list sums to the bar's length (checked by the
/// tests of the whole preset: an overfull bar would not be flagged by `validate`, ADR-0022 §4, but
/// would change the unrolled timeline the tests check).
fn cells(role: Role, shape: BarShape) -> &'static [&'static [i8]] {
    use BarShape::{Four, SixEight, ThreeFour};
    match (role, shape) {
        (Role::Melody, Four) => &[
            &[4, 4, 4, 4],
            &[2, 2, 4, 4, 4],
            &[4, 2, 2, 8],
            &[1, 1, 1, 1, 4, 4, 4],
            &[6, 2, 4, 4],
            &[8, 4, 4],
            &[4, 4, 8],
            &[2, 2, 2, 2, 4, 4],
        ],
        (Role::Melody, SixEight) => &[
            &[6, 6],
            &[4, 2, 6],
            &[2, 2, 2, 6],
            &[4, 2, 4, 2],
            &[2, 2, 2, 2, 2, 2],
            &[3, 1, 2, 6],
        ],
        (Role::Melody, _) => &[
            &[4, 4, 4],
            &[8, 4],
            &[2, 2, 4, 4],
            &[6, 2, 4],
            &[4, 2, 2, 4],
        ],
        (Role::Inner, Four) => &[
            &[2, 2, 2, 2, 2, 2, 2, 2],
            &[-2, 2, -2, 2, -2, 2, -2, 2],
            &[4, 4, 4, 4],
            &[8, 8],
        ],
        (Role::Inner, SixEight) => &[&[2, 2, 2, 2, 2, 2], &[-2, 2, 2, -2, 2, 2], &[6, 6]],
        (Role::Inner, _) => &[&[2, 2, 2, 2, 2, 2], &[-4, 4, 4], &[4, 4, 4]],
        (Role::Sustain | Role::PianoInner, Four) => &[&[16], &[8, 8], &[12, 4]],
        (Role::Sustain | Role::PianoInner | Role::PianoPedal, SixEight | ThreeFour) => {
            &[&[12], &[6, 6]]
        }
        (Role::Bass, Four) => &[&[8, 8], &[4, 4, 4, 4], &[12, 4], &[4, -4, 4, -4]],
        (Role::Bass, SixEight) => &[&[6, 6], &[4, 2, 4, 2], &[12]],
        (Role::Bass, _) => &[&[4, 4, 4], &[12], &[4, -8]],
        (Role::PianoRight, Four) => &[&[2, 2, 2, 2, 2, 2, 2, 2], &[4, 4, 4, 4], &[4, 2, 2, 4, 4]],
        (Role::PianoRight, SixEight) => &[&[2, 2, 2, 2, 2, 2], &[6, 6], &[4, 2, 4, 2]],
        (Role::PianoRight, _) => &[&[-4, 4, 4], &[4, 4, 4], &[2, 2, 2, 2, 2, 2]],
        (Role::PianoLeft, Four) => &[&[8, 8], &[4, 4, 4, 4]],
        (Role::PianoLeft, SixEight) => &[&[6, 6], &[12]],
        (Role::PianoLeft, _) => &[&[4, 4, 4], &[12]],
        (Role::Timpani, Four) => &[&[4, -4, 4, -4], &[16], &[-8, 8]],
        (Role::Timpani, SixEight) => &[&[6, -6], &[12]],
        (Role::Timpani, _) => &[&[4, -8], &[12]],
        (Role::HarpRight, Four) => &[&[4, 4, 4, 4]],
        (Role::HarpRight, SixEight) => &[&[6, 6]],
        // The harp in 3/4, and the second voices in shapes they never play (the pickup, the free
        // bar and the final bar are handled before cells are chosen).
        _ => &[&[4, 4, 4]],
    }
}

/// The note value of a length in sixteenths (1, 2, 3, 4, 6, 8, 12 or 16). O(1).
fn value(sixteenths: i8) -> NoteValue {
    let dotted = |base| NoteValue::new(base, 1).unwrap();
    match sixteenths {
        1 => NoteValue::plain(BaseValue::Sixteenth),
        2 => NoteValue::plain(BaseValue::Eighth),
        3 => dotted(BaseValue::Eighth),
        4 => NoteValue::plain(BaseValue::Quarter),
        6 => dotted(BaseValue::Quarter),
        8 => NoteValue::plain(BaseValue::Half),
        12 => dotted(BaseValue::Half),
        _ => NoteValue::plain(BaseValue::Whole),
    }
}

/// An offset of `ticks` (1/96 of a whole note) from the start of the bar.
fn at(ticks: i64) -> Duration {
    Duration::from_rational(Rational::new(ticks, 96).unwrap())
}

/// The rhythm cell of one bar of one voice, as a fresh vector (it may be modified: a tuplet
/// replaces a quarter).
///
/// Special bars first: the final bar is one long note; the soloist's last bar of section D ends
/// with a rest under a fermata (the pause before the D.S.); melody cadences are two long notes
/// (the first trilled); the hemiola bar is three quarters for everyone; the inner strings of
/// section D play held notes under a tremolo. Otherwise a random cell of the role, and a melody
/// may turn a quarter into a triplet (5% of its bars in 4/4 and 3/4) or a dotted quarter into a
/// duplet (3% in 6/8).
fn cell(rng: &mut Rng, part: &Part<'_>, local: u32) -> Vec<i8> {
    let shape = BarShape::of(local);
    let role = part.voice.role;
    if local == 99 {
        return vec![16];
    }
    if local == 79 && part.spec.soloist {
        return vec![8, 4, -4];
    }
    if shape == BarShape::Hemiola {
        return vec![4, 4, 4];
    }
    if role == Role::Melody && is_cadence(local) {
        return match shape {
            BarShape::Four => vec![8, 8],
            BarShape::SixEight => vec![6, 6],
            _ => vec![8, 4],
        };
    }
    if part.is_tremolo(local) {
        return vec![12];
    }
    let list = cells(role, shape);
    let mut chosen =
        list[usize::try_from(rng.below(u64::try_from(list.len()).unwrap())).unwrap()].to_vec();
    if role == Role::Melody {
        let (from, to, percent) = if shape == BarShape::SixEight {
            (6, DUPLET, 3)
        } else {
            (4, TRIPLET, 5)
        };
        if rng.chance(percent)
            && let Some(slot) = chosen.iter_mut().find(|x| **x == from)
        {
            *slot = to;
        }
    }
    chosen
}

/// One voice of one instrument, with everything [`write_voice`] needs to know about it.
#[derive(Debug, Clone, Copy)]
pub(super) struct Part<'a> {
    /// The instrument's specification.
    pub(super) spec: &'a InstrumentSpec,
    /// The voice's ID.
    pub(super) id: VoiceId,
    /// The voice's specification.
    pub(super) voice: VoiceSpec,
    /// Whether it is the first voice of the instrument (the one bar repeats and the cadenza use).
    pub(super) first: bool,
}

impl Part<'_> {
    /// Whether local bar `local` is one of the bars under a bar-repeat sign on this voice's staff
    /// (21, repeating 20; 30–31, repeating 28–29: see `marks::add_bar_repeats`).
    fn is_repeated_bar(&self, local: u32) -> bool {
        self.spec.bar_repeats && self.voice.staff == 0 && matches!(local, 21 | 30 | 31)
    }

    /// Whether this voice plays local bar `local` at all. The two second voices of the piano only
    /// play in some sections (the inner voice in A and C, the pedal in D), never in the pickup nor
    /// in the free bar; every other voice plays every bar (resting is still writing rests).
    fn plays(&self, local: u32) -> bool {
        let section = Section::of(local);
        let shape = BarShape::of(local);
        match self.voice.role {
            Role::PianoInner => {
                matches!(section, Section::A | Section::C) && shape != BarShape::Pickup
            }
            Role::PianoPedal => section == Section::D && shape != BarShape::Free,
            _ => !self.is_repeated_bar(local),
        }
    }

    /// Whether local bar `local` is one of the tremolo bars of the inner strings (section D up to
    /// the cadenza).
    fn is_tremolo(&self, local: u32) -> bool {
        self.spec.family == Family::Strings
            && self.voice.role == Role::Inner
            && (60..=67).contains(&local)
    }

    /// The voice's register in local bar `local`: an octave up during a tenor-clef passage
    /// (section D), and the piano's left hand moves up to the treble clef in 42–49 (the clef
    /// changes themselves are written by `marks::add_clef_changes`).
    fn register(&self, local: u32) -> Register {
        let (low, high) = self.voice.register;
        if self.spec.tenor_passage && Section::of(local) == Section::D {
            return (low + 7, high + 7);
        }
        if self.voice.role == Role::PianoLeft && (42..=49).contains(&local) {
            return (low + 14, high + 7);
        }
        (low, high)
    }
}

/// The state a voice carries from one event to the next.
#[derive(Debug)]
struct State {
    /// The diatonic number of its last note (or the middle of its register at the start).
    last: i64,
    /// The last note, if the previous event was a single note: a tie can start from it. Reset by
    /// rests, by tacets and at section starts (a tie into a volta or a coda reads badly).
    tie_from: Option<(NoteId, Pitch)>,
    /// Whether the voice is resting through the current phrase.
    tacet: bool,
    /// The notes written in the current movement, in order, for the slurs: (event, local bar).
    notes: Vec<(EventId, u32)>,
}

/// Writes one voice of movement `m` (whose 100 bars are `bars`) into the builder.
pub(super) fn write_voice(
    b: &mut DocumentBuilder,
    rng: &mut Rng,
    counters: &mut Counters,
    part: &Part<'_>,
    bars: &[BarId],
    m: u32,
) {
    let (low, high) = part.voice.register;
    let mut state = State {
        last: i64::midpoint(low, high),
        tie_from: None,
        tacet: false,
        notes: Vec::new(),
    };
    for local in 0..plan::BARS {
        let bar = bars[usize::try_from(local).unwrap()];
        let section = Section::of(local);
        if local == section.start() || local == 17 {
            state.tie_from = None;
        }
        if !part.plays(local) {
            state.tie_from = None;
            continue;
        }
        let shape = BarShape::of(local);
        if shape == BarShape::Pickup {
            write_pickup(b, part, bar, local, m, &mut state);
            continue;
        }
        if shape == BarShape::Free {
            if part.spec.soloist && part.first {
                write_cadenza(
                    b,
                    part.id,
                    bar,
                    Harmony::of(local, m, part.spec.written_shift),
                );
            } else {
                b.add_rest(
                    Placement::new(part.id, bar, Duration::ZERO),
                    RestValue::WholeBar,
                )
                .unwrap();
            }
            state.tie_from = None;
            continue;
        }
        if local == section.start() || phrase_position(local) == 0 {
            state.tacet = rng.chance(part.spec.family.tacet_percent());
        }
        if state.tacet && local != 99 {
            // The final bar is excluded above: everybody plays the last chord.
            b.add_rest(
                Placement::new(part.id, bar, Duration::ZERO),
                RestValue::WholeBar,
            )
            .unwrap();
            state.tie_from = None;
            continue;
        }
        let elements = cell(rng, part, local);
        write_bar(b, rng, counters, part, bar, local, m, &elements, &mut state);
    }
    if matches!(part.voice.role, Role::Melody | Role::Inner) {
        add_slurs(b, rng, &state.notes);
    }
}

/// The pickup: a melody leads in with two eighths, every other first voice rests for the quarter
/// (second voices do not play it, see [`Part::plays`]).
fn write_pickup(
    b: &mut DocumentBuilder,
    part: &Part<'_>,
    bar: BarId,
    local: u32,
    m: u32,
    state: &mut State,
) {
    if part.voice.role == Role::Melody {
        let harmony = Harmony::of(local, m, part.spec.written_shift);
        let register = part.register(local);
        for (k, n) in [state.last - 1, state.last].into_iter().enumerate() {
            let n = n.clamp(register.0, register.1);
            let ids = b
                .add_chord(
                    Placement::new(part.id, bar, at(12 * i64::try_from(k).unwrap())),
                    value(2),
                    &[harmony.pitch(n)],
                )
                .unwrap();
            state.last = n;
            state.notes.push((ids.event, local));
        }
    } else {
        b.add_rest(
            Placement::new(part.id, bar, Duration::ZERO),
            RestValue::Value(value(4)),
        )
        .unwrap();
    }
}

/// The flute's cadenza in the free bar (5/4): a run of eight sixteenths up the scale, a triplet of
/// quarters holding a quarter, a nested duplet of eighths and a triplet eighth, then a
/// **microtone** (E a quarter-tone flat) held under a long fermata.
///
/// Offsets in ticks of 1/96: the run fills 0–48; the triplet of quarters (3:2) lasts a half note
/// (48 ticks): its quarter lasts 16, the nested duplet (2:3 of triplet eighths) two eighths of 12,
/// the last triplet eighth 8; the microtone quarter fills 96–120.
fn write_cadenza(b: &mut DocumentBuilder, voice: VoiceId, bar: BarId, harmony: Harmony) {
    let quarter = NoteValue::plain(BaseValue::Quarter);
    let eighth = NoteValue::plain(BaseValue::Eighth);
    // The run starts on the chord's root in the fifth octave.
    let start = harmony.root + 35;
    for k in 0..8_i64 {
        b.add_chord(
            Placement::new(voice, bar, at(6 * k)),
            NoteValue::plain(BaseValue::Sixteenth),
            &[harmony.pitch(start + k)],
        )
        .unwrap();
    }
    let triplet = b
        .add_tuplet(voice, Tuplet::new(3, 2, quarter).unwrap(), None)
        .unwrap();
    let duplet = b
        .add_tuplet(voice, Tuplet::new(2, 3, eighth).unwrap(), Some(triplet))
        .unwrap();
    let top = start + 8;
    b.add_chord(
        Placement::new(voice, bar, at(48)).in_tuplet(triplet),
        quarter,
        &[harmony.pitch(top)],
    )
    .unwrap();
    for (k, ticks) in [64, 76].into_iter().enumerate() {
        b.add_chord(
            Placement::new(voice, bar, at(ticks)).in_tuplet(duplet),
            eighth,
            &[harmony.pitch(top - 1 - i64::try_from(k).unwrap())],
        )
        .unwrap();
    }
    b.add_chord(
        Placement::new(voice, bar, at(88)).in_tuplet(triplet),
        eighth,
        &[harmony.pitch(top - 3)],
    )
    .unwrap();
    let micro = Pitch::new(
        Step::E,
        Alteration::new(Rational::new(-1, 2).unwrap()).unwrap(),
        5,
    );
    let last = b
        .add_chord(Placement::new(voice, bar, at(96)), quarter, &[micro])
        .unwrap();
    b.set_fermata(last.event, Some(Fermata::Long)).unwrap();
}

/// Writes one bar of `elements` (a rhythm cell) for a voice that is playing.
#[allow(
    clippy::too_many_arguments,
    reason = "the bar's context, passed explicitly rather than bundled into a one-use struct"
)]
fn write_bar(
    b: &mut DocumentBuilder,
    rng: &mut Rng,
    counters: &mut Counters,
    part: &Part<'_>,
    bar: BarId,
    local: u32,
    m: u32,
    elements: &[i8],
    state: &mut State,
) {
    let harmony = Harmony::of(local, m, part.spec.written_shift);
    let register = part.register(local);
    let bar_ticks = BarShape::of(local).sixteenths() * 6;
    let mut ticks = 0_i64;
    let mut last_event = None;
    for (index, &element) in elements.iter().enumerate() {
        let event = match element {
            TRIPLET => write_tuplet(b, part.id, bar, ticks, (3, 2, 8), harmony, register, state),
            DUPLET => write_tuplet(b, part.id, bar, ticks, (2, 3, 18), harmony, register, state),
            rest if rest < 0 => {
                let id = b
                    .add_rest(
                        Placement::new(part.id, bar, at(ticks)),
                        RestValue::Value(value(-rest)),
                    )
                    .unwrap();
                state.tie_from = None;
                id
            }
            length => {
                let strong = ticks == 0 || 2 * ticks == bar_ticks;
                let note = Note {
                    bar,
                    ticks,
                    length,
                    strong,
                    index,
                };
                write_note(
                    b, rng, counters, part, local, harmony, register, note, state,
                )
            }
        };
        let length = match element {
            TRIPLET => 4,
            DUPLET => 6,
            x => x.abs(),
        };
        ticks += i64::from(length) * 6;
        last_event = Some(event);
    }
    // The final bar and the soloist's pause before the D.S. carry their fermatas.
    if let Some(event) = last_event {
        if local == 99 {
            b.set_fermata(event, Some(Fermata::Long)).unwrap();
        } else if local == 79 && part.spec.soloist {
            b.set_fermata(event, Some(Fermata::Normal)).unwrap();
        }
    }
}

/// A tuplet of eighths at `ticks`: `(actual, normal, ticks per member)` is (3, 2, 8) for a triplet
/// in the time of a quarter, (2, 3, 18) for a duplet in the time of a dotted quarter. Its members
/// walk by step from the voice's last note. Returns the last member's event.
#[allow(
    clippy::too_many_arguments,
    reason = "the bar's context, passed explicitly rather than bundled into a one-use struct"
)]
fn write_tuplet(
    b: &mut DocumentBuilder,
    voice: VoiceId,
    bar: BarId,
    ticks: i64,
    (actual, normal, member): (u8, u8, i64),
    harmony: Harmony,
    register: Register,
    state: &mut State,
) -> EventId {
    let eighth = NoteValue::plain(BaseValue::Eighth);
    let tuplet = b
        .add_tuplet(voice, Tuplet::new(actual, normal, eighth).unwrap(), None)
        .unwrap();
    let mut last = None;
    for k in 0..i64::from(actual) {
        let n = (state.last + 1).clamp(register.0, register.1);
        let ids = b
            .add_chord(
                Placement::new(voice, bar, at(ticks + k * member)).in_tuplet(tuplet),
                eighth,
                &[harmony.pitch(n)],
            )
            .unwrap();
        state.last = n;
        state.tie_from = None;
        last = Some(ids.event);
    }
    last.unwrap()
}

/// Where a note goes in its bar and how long it is.
#[derive(Debug, Clone, Copy)]
struct Note {
    /// Its bar.
    bar: BarId,
    /// Its offset in the bar, in ticks.
    ticks: i64,
    /// Its length, in sixteenths.
    length: i8,
    /// Whether it falls on the first beat or the middle of the bar (a chord tone there).
    strong: bool,
    /// Its index in the bar's cell (the cadence trill is on note 0).
    index: usize,
}

/// Writes one note (or chord) of a voice with everything that may decorate it, and returns its
/// event.
#[allow(
    clippy::too_many_arguments,
    reason = "the bar's context, passed explicitly rather than bundled into a one-use struct"
)]
fn write_note(
    b: &mut DocumentBuilder,
    rng: &mut Rng,
    counters: &mut Counters,
    part: &Part<'_>,
    local: u32,
    harmony: Harmony,
    register: Register,
    note: Note,
    state: &mut State,
) -> EventId {
    let role = part.voice.role;
    let placement = Placement::new(part.id, note.bar, at(note.ticks));
    // A tie: the same pitch as the previous single note, 8% of the time, for the voices that
    // hold lines (not chords).
    let tie_from = state.tie_from.filter(|_| {
        matches!(
            role,
            Role::Melody | Role::Inner | Role::Sustain | Role::Bass
        ) && rng.chance(8)
    });
    let pitches = match tie_from {
        Some((_, pitch)) => vec![pitch],
        None => choose_pitches(rng, role, part.spec.family, harmony, register, note, state),
    };
    // An acciaccatura before 2% of the melody's notes, a step above.
    if role == Role::Melody && tie_from.is_none() && rng.chance(2) {
        b.add_grace_chord(
            placement,
            Grace {
                kind: GraceKind::Acciaccatura,
                attach: GraceAttach::Before,
            },
            value(2),
            &[harmony.pitch(state.last + 1)],
        )
        .unwrap();
    }
    let ids = b
        .add_chord(placement, value(note.length), &pitches)
        .unwrap();
    if let Some((from, _)) = tie_from {
        b.set_tie(from, Tie::To(ids.notes[0])).unwrap();
    }
    state.tie_from = (ids.notes.len() == 1).then(|| (ids.notes[0], pitches[0]));
    state.notes.push((ids.event, local));
    decorate(
        b, rng, counters, part, local, harmony, note, ids.event, &ids.notes, state,
    );
    ids.event
}

/// The pitches of one note or chord, by role, updating the voice's last note.
fn choose_pitches(
    rng: &mut Rng,
    role: Role,
    family: Family,
    harmony: Harmony,
    register: Register,
    note: Note,
    state: &mut State,
) -> Vec<Pitch> {
    let step = |rng: &mut Rng| -> i64 {
        if rng.chance(15) {
            [-4, -3, 3, 4][usize::try_from(rng.below(4)).unwrap()]
        } else {
            [-2, -1, -1, 1, 1, 2][usize::try_from(rng.below(6)).unwrap()]
        }
    };
    let numbers: Vec<i64> = match role {
        Role::Melody => {
            let mut n = (state.last + step(rng)).clamp(register.0, register.1);
            if note.strong {
                n = harmony.nearest_chord_tone(n, register);
            }
            vec![n]
        }
        Role::Inner | Role::Sustain | Role::PianoInner => {
            let n = harmony.nearest_chord_tone(
                state.last + i64::try_from(rng.below(3)).unwrap() - 1,
                register,
            );
            // Double stops on strings (5%): a third below, a chord tone too.
            if family == Family::Strings && rng.chance(5) && n - 2 >= register.0 {
                vec![n, n - 2]
            } else {
                vec![n]
            }
        }
        Role::Bass | Role::PianoPedal => {
            // The root, and on the second note of the bar sometimes the fifth.
            let class = if note.index % 2 == 1 && rng.chance(40) {
                4
            } else {
                0
            };
            vec![harmony.nearest_degree(state.last, class, register)]
        }
        Role::Timpani => {
            // On the dominant chord the timpani play its root (the dominant), otherwise the
            // tonic, which lies `degree` steps below the chord's root.
            let class = if harmony.degree == 4 {
                0
            } else {
                (-harmony.degree).rem_euclid(7)
            };
            vec![harmony.nearest_degree(state.last, class, register)]
        }
        Role::PianoLeft => {
            let root = harmony.nearest_degree(state.last, 0, register);
            match rng.below(10) {
                0 | 1 => vec![root],
                2..=5 => vec![root, root + 4],
                _ => vec![root, root + 7],
            }
        }
        Role::PianoRight | Role::HarpRight => {
            let top = harmony.nearest_chord_tone(state.last + step(rng), register);
            let count = if role == Role::HarpRight {
                3
            } else {
                match rng.below(10) {
                    0 => 1,
                    1..=4 => 2,
                    5..=8 => 3,
                    _ => 4,
                }
            };
            // The chord tones downwards from the top: each next one is the nearest chord tone
            // strictly below the previous (a third or a fourth below).
            let mut chord = vec![top];
            while chord.len() < count {
                let below = chord[chord.len() - 1] - 1;
                let next = (below - 2..=below)
                    .rev()
                    .find(|&x| harmony.is_chord_tone(x))
                    .unwrap_or(below);
                chord.push(next);
            }
            chord
        }
    };
    state.last = numbers[0];
    numbers.into_iter().map(|n| harmony.pitch(n)).collect()
}

/// The markings of a note just written: the accent of a section start, staccato in the inner
/// parts, ornaments and the cadence trill (with its after-grace) in the melody, tremolos, arpeggios
/// and offsets on the piano, the let-ring of the final chord.
#[allow(
    clippy::too_many_arguments,
    reason = "the note's context, passed explicitly rather than bundled into a one-use struct"
)]
fn decorate(
    b: &mut DocumentBuilder,
    rng: &mut Rng,
    counters: &mut Counters,
    part: &Part<'_>,
    local: u32,
    harmony: Harmony,
    note: Note,
    event: EventId,
    notes: &[NoteId],
    state: &State,
) {
    let role = part.voice.role;
    // The first note of every section start carries the next articulation of the rotation.
    if note.index == 0 && local == Section::of(local).start() {
        let articulation = ARTICULATIONS[counters.accents % ARTICULATIONS.len()];
        counters.accents += 1;
        b.set_articulations(event, ArticulationSet::EMPTY.with(articulation))
            .unwrap();
    } else if role == Role::Inner && note.length == 2 && rng.chance(30) {
        b.set_articulations(event, ArticulationSet::EMPTY.with(Articulation::Staccato))
            .unwrap();
    }
    if role == Role::Melody {
        decorate_melody(b, counters, part, local, harmony, note, event, state);
    }
    if part.is_tremolo(local) {
        b.set_ornament(
            event,
            Some(Ornament::Tremolo(TremoloStrokes::new(3).unwrap())),
        )
        .unwrap();
    }
    if role == Role::HarpRight {
        b.set_arpeggio(event, Some(Arpeggio::Up)).unwrap();
    }
    if role == Role::PianoRight {
        decorate_piano(b, rng, counters, local, note, event, notes);
    }
    if local == 99 && matches!(role, Role::PianoRight | Role::PianoLeft) {
        for &n in notes {
            b.set_tie(n, Tie::LetRing).unwrap();
        }
    }
}

/// The melody's ornaments: a trill on the first note of each cadence, ended by an appoggiatura
/// after it every second cadence; elsewhere every 40th note takes the next ornament of the
/// rotation (turn, inverted turn, mordent, upper mordent, trill with a sharpened auxiliary).
#[allow(
    clippy::too_many_arguments,
    reason = "the note's context, passed explicitly rather than bundled into a one-use struct"
)]
fn decorate_melody(
    b: &mut DocumentBuilder,
    counters: &mut Counters,
    part: &Part<'_>,
    local: u32,
    harmony: Harmony,
    note: Note,
    event: EventId,
    state: &State,
) {
    if is_cadence(local) && note.index == 0 && local != 99 && !(local == 79 && part.spec.soloist) {
        b.set_ornament(event, Some(Ornament::Trill { auxiliary: None }))
            .unwrap();
        counters.cadences += 1;
        if counters.cadences.is_multiple_of(2) {
            // The after-grace goes exactly where the trilled note ends (ADR-0024 §5), which is
            // where the cadence's second note starts.
            let end = note.ticks + i64::from(note.length) * 6;
            b.add_grace_chord(
                Placement::new(part.id, note.bar, at(end)),
                Grace {
                    kind: GraceKind::Appoggiatura,
                    attach: GraceAttach::After,
                },
                value(2),
                &[harmony.pitch(state.last - 1)],
            )
            .unwrap();
        }
        return;
    }
    counters.melody_notes += 1;
    if counters.melody_notes.is_multiple_of(40) {
        let ornaments = [
            Ornament::Turn,
            Ornament::InvertedTurn,
            Ornament::Mordent,
            Ornament::UpperMordent,
            Ornament::Trill {
                auxiliary: Some(Alteration::chromatic(1).unwrap()),
            },
        ];
        b.set_ornament(event, Some(ornaments[counters.ornaments % ornaments.len()]))
            .unwrap();
        counters.ornaments += 1;
    }
}

/// The piano's right hand: arpeggios on 10% of its chords in section B (rotating up, plain,
/// down), microtiming on 1% of its notes, and pass-specific offsets for the second time through
/// the repeat on the first chord of local bars 2 and 5.
fn decorate_piano(
    b: &mut DocumentBuilder,
    rng: &mut Rng,
    counters: &mut Counters,
    local: u32,
    note: Note,
    event: EventId,
    notes: &[NoteId],
) {
    if notes.len() >= 2 && Section::of(local) == Section::B && rng.chance(10) {
        let arpeggio = [Arpeggio::Up, Arpeggio::Plain, Arpeggio::Down][counters.arpeggios % 3];
        counters.arpeggios += 1;
        b.set_arpeggio(event, Some(arpeggio)).unwrap();
    }
    for &n in notes {
        if rng.chance(1) {
            let sign = if rng.chance(50) { 1 } else { -1 };
            b.set_offsets(
                n,
                PerformanceOffsets {
                    // A 128th early or late: a small push, well inside ADR-0027 §8's bounds.
                    onset: plan::d(sign, 128),
                    length: Duration::ZERO,
                    velocity: -200 * i32::try_from(sign).unwrap(),
                },
            )
            .unwrap();
        }
    }
    if note.index == 0 && matches!(local, 2 | 5) {
        b.set_pass_offsets(
            notes[0],
            1,
            PerformanceOffsets {
                onset: if local == 5 {
                    plan::d(1, 64)
                } else {
                    Duration::ZERO
                },
                length: Duration::ZERO,
                velocity: 300,
            },
        )
        .unwrap();
    }
}

/// Slurs over the notes of a melody or inner voice: from each note, a 25% chance of a slur over
/// the next 1–3 notes (so groups of 2–4), never across a section start and never overlapping the
/// previous slur. Placement rotates through automatic, above and below.
fn add_slurs(b: &mut DocumentBuilder, rng: &mut Rng, notes: &[(EventId, u32)]) {
    let placements = [
        SlurPlacement::Auto,
        SlurPlacement::Above,
        SlurPlacement::Below,
    ];
    let mut slurs = 0_usize;
    let mut i = 0;
    while i < notes.len() {
        let span = usize::try_from(1 + rng.below(3)).unwrap();
        if rng.chance(25) && i + span < notes.len() {
            let (start, from) = notes[i];
            let (end, to) = notes[i + span];
            if Section::of(from) == Section::of(to) && to != 17 {
                b.add_slur(start, end, placements[slurs % placements.len()])
                    .unwrap();
                slurs += 1;
                i += span + 1;
                continue;
            }
        }
        i += 1;
    }
}

/// The rhythm cells fill their bars exactly.
///
/// An overfull bar is valid in the model (ADR-0022 §4, the score shows it as an error to fix), so
/// `validate` would not catch a cell that is one sixteenth too long: it would only move every
/// later event of the voice. This test sums every cell of every role in every shape it is used in.
#[cfg(test)]
mod tests {
    use super::{BarShape, DUPLET, Role, TRIPLET, cells};

    /// The length of one element of a cell, in sixteenths.
    fn length(element: i8) -> i64 {
        match element {
            TRIPLET => 4,
            DUPLET => 6,
            x => i64::from(x.abs()),
        }
    }

    /// Every cell of every (role, shape) pair the voices use sums to the bar's length.
    #[test]
    fn every_cell_fills_its_bar() {
        use BarShape::{Four, SixEight, ThreeFour};
        use Role::{
            Bass, HarpRight, Inner, Melody, PianoInner, PianoLeft, PianoPedal, PianoRight, Sustain,
            Timpani,
        };
        let pairs = [
            (Melody, [Four, SixEight, ThreeFour].as_slice()),
            (Inner, &[Four, SixEight, ThreeFour]),
            (Sustain, &[Four, SixEight, ThreeFour]),
            (Bass, &[Four, SixEight, ThreeFour]),
            (PianoRight, &[Four, SixEight, ThreeFour]),
            (PianoLeft, &[Four, SixEight, ThreeFour]),
            // The piano's second voices only play where `Part::plays` lets them: the inner voice in
            // sections A and C (4/4), the pedal in section D (3/4).
            (PianoInner, &[Four]),
            (PianoPedal, &[ThreeFour]),
            (HarpRight, &[Four, SixEight, ThreeFour]),
            (Timpani, &[Four, SixEight, ThreeFour]),
        ];
        for (role, shapes) in pairs {
            for &shape in shapes {
                for cell in cells(role, shape) {
                    let sum: i64 = cell.iter().map(|&e| length(e)).sum();
                    assert_eq!(sum, shape.sixteenths(), "{role:?} in {shape:?}: {cell:?}");
                }
            }
        }
    }
}
