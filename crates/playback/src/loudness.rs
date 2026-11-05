// SPDX-License-Identifier: GPL-3.0-or-later

//! How loud each note is played: the written dynamics and hairpins along the path, the dynamic
//! accents (*sf*, *fp*…), the articulation accents and the **attack profile** of every note
//! (ADR-0027 §4 and §14, H1–H10 and H14–H17; SCALETTA F1.13c).
//!
//! # The scale the rules work on
//! Every rule is written on the familiar **0–127** scale of MIDI 1.0 (the table of ADR-0027 §4:
//! *pppp* = 10, … *mf* = 75, … *ffff* = 127, one level every 13), and computed there **exactly**,
//! as a [`Rational`]: a note halfway through a crescendo from *p* (49) to *f* (88) is at 68.5, not
//! 68 or 69. Only at the very end is the value up-scaled to the 16-bit scale of MIDI 2.0 that the
//! played notes carry (H8): an integer by the bit replication `v << 9 | v << 2 | v >> 5` of
//! ADR-0027 §4 (so the table's levels land exactly where §4 says), a fraction by linear
//! interpolation between the up-scaled values of its two neighbouring integers, rounded to the
//! nearest step with halves going up. That keeps the result monotone (a louder rule never gives a
//! softer velocity) and identical to §4 on every level of the table.
//!
//! # The timeline of a voice (H1, H2, H7, H9)
//! The dynamic in force is followed **along the path**, the way a player remembers what they last
//! played: after a repeat back to a bar with no dynamic, the dynamic of the end of the repeat
//! stays. So the loudness of a voice is a [`Timeline`] over **path points** — (played bar, offset
//! inside its written bar) — built once by walking the path, and then asked for any note in
//! O(log n). It holds:
//! - **states**, sorted by path point: from each point on, the loudness is either a constant
//!   level or a hairpin's ramp (a [`Plan`]);
//! - **point accents**, sorted by path point: the *sf*-like and *fp*-like marks, which act only on
//!   the notes starting exactly there (H3, H4).
//!
//! Only the dynamics and hairpins that apply to the voice are read: those of the whole instrument
//! and those of this voice. At one point the instrument's come first and the voice's after, so the
//! voice's win (G5, H9).
//!
//! # Hairpins (H5–H9)
//! A hairpin becomes active when the walk reaches its start, and stops when the walk reaches its
//! end or leaves its written range in any other way (a repeat back, a jump): from then on its
//! **target** stays in force (H7). While active, the level is interpolated linearly in the
//! **written position** (H8) along its anchors: the level in force at its start, every dynamic
//! inside it (H7: they split it into stretches), and the target at its end (H6: the first
//! dynamic at or after the end, within the bar holding the end, if it goes the right way;
//! otherwise the next level of the table up or down).
//!
//! # Why not a single pass while the notes are made?
//! The notes of a voice are made in path order too, so the levels could be read with a cursor.
//! But the incremental update of SCALETTA F1.13c (H12) recomputes the notes of a few cells only,
//! and then needs the level at an arbitrary point: a timeline answers that in O(log n) without
//! walking the path again, and a change of the dynamics rebuilds only the timelines (H11).

use orchestra_core_model::{
    ArticulationSet, Document, DocumentError, DynamicLevel, DynamicMark, DynamicScope, HairpinKind,
    Instrument, VoiceId,
};
use orchestra_core_time::{BarIndex, Duration, Position, Rational, RationalError, Seconds};

use crate::perform::PerformError;
use crate::unroll::{UnrollError, Unrolled};

// ------------------------------------------------------------------------------------------------
// Levels on the 0–127 scale (ADR-0027 §4)
// ------------------------------------------------------------------------------------------------

/// The softest playable level, *pppp*, on the 0–127 scale (G4).
const PPPP: i64 = 10;

/// The distance between two neighbouring levels of the table, on the 0–127 scale (G4).
const LEVEL_STEP: i64 = 13;

/// The loudest playable level, *ffff*: the top of the 0–127 scale (G4).
const FFFF: i64 = 127;

/// *p*, *mf*, *f* and *ff* on the 0–127 scale: the levels *fp*, *sfp*, *pf* go to and from (H16),
/// and *mf*, the level in force before any dynamic (H1, N9).
const P: i64 = 49;
/// *pp* on the 0–127 scale (the second level of *sfpp*, H16).
const PP: i64 = 36;
/// *mf* on the 0–127 scale: in force before the first dynamic of a voice (H1).
const MF: i64 = 75;
/// *f* on the 0–127 scale (the attack of *fp*, the second level of *pf*, H16).
const F: i64 = 88;
/// *ff* on the 0–127 scale (the attack of *sfp* and *sfpp*, H16).
const FF: i64 = 101;

/// The value on the 0–127 scale of a written level (G4): the ten levels from *pppp* to *ffff*
/// spread evenly, one every 13 from 10; the softer and louder levels the model can hold
/// (*ppppp*, *pppppp*, *fffff*, *ffffff*) are written but play as the extremes. O(1).
const fn level_value(level: DynamicLevel) -> i64 {
    // The index of the level among the ten playable ones, 0 for *pppp* to 9 for *ffff*.
    let index = match level {
        DynamicLevel::Pppppp | DynamicLevel::Ppppp | DynamicLevel::Pppp => 0,
        DynamicLevel::Ppp => 1,
        DynamicLevel::Pp => 2,
        DynamicLevel::P => 3,
        DynamicLevel::Mp => 4,
        DynamicLevel::Mf => 5,
        DynamicLevel::F => 6,
        DynamicLevel::Ff => 7,
        DynamicLevel::Fff => 8,
        DynamicLevel::Ffff | DynamicLevel::Fffff | DynamicLevel::Ffffff => 9,
    };
    // At most 10 + 13 × 9 = 127: no overflow is possible.
    PPPP + LEVEL_STEP * index
}

/// The next level of the table strictly above `level`, or *ffff* if there is none: the target of
/// a crescendo without a fitting dynamic (H6). From *niente* (0) it is *pppp*. O(1).
fn level_above(level: Rational) -> Result<Rational, RationalError> {
    // The levels are 10 + 13 i; the first one strictly above `level` has i = ⌊(level − 10) / 13⌋
    // + 1, at least 0 (for levels under *pppp*) and at most 9.
    let i = level
        .checked_sub(Rational::from_integer(PPPP)?)?
        .checked_div(Rational::from_integer(LEVEL_STEP)?)?
        .floor()
        .saturating_add(1)
        .clamp(0, 9);
    Rational::from_integer(PPPP.saturating_add(LEVEL_STEP.saturating_mul(i)))
}

/// The next level of the table strictly below `level`, or *pppp* if there is none: the target of
/// a diminuendo without a fitting dynamic (H6). O(1).
fn level_below(level: Rational) -> Result<Rational, RationalError> {
    // The first level strictly below has i = ⌈(level − 10) / 13⌉ − 1, between 0 and 9.
    let i = level
        .checked_sub(Rational::from_integer(PPPP)?)?
        .checked_div(Rational::from_integer(LEVEL_STEP)?)?
        .ceil()
        .saturating_sub(1)
        .clamp(0, 9);
    Rational::from_integer(PPPP.saturating_add(LEVEL_STEP.saturating_mul(i)))
}

/// `n/d` seconds, for the constant times of H15 and H16. O(1).
fn seconds(n: i64, d: i64) -> Result<Seconds, RationalError> {
    Ok(Seconds::from_rational(Rational::new(n, d)?))
}

// ------------------------------------------------------------------------------------------------
// What a dynamic mark does
// ------------------------------------------------------------------------------------------------

/// A dynamic accent: what a mark does to the notes that start exactly at its point (H3, H4, H16).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PointAccent {
    /// *rf*, *rfz*, *sf*, *sfz*, *fz*, *sffz*: the attack `levels` levels above the level in
    /// force (never above *ffff*), settling back to it in `settle_in` (H3, H16).
    Up {
        /// How many levels of the table the attack is louder: 1, 2 or 3.
        levels: i64,
        /// How long the sound takes to settle back to the level in force.
        settle_in: Seconds,
    },
    /// *fp*, *sfp*, *sfpp*, *pf*: a fixed attack, held for `hold`, then going to `then` in
    /// `settle_in` (H16); `then` is also the level in force afterwards (H4).
    Shift {
        /// The level of the attack, on the 0–127 scale.
        attack: i64,
        /// How long the attack level is held.
        hold: Seconds,
        /// How long the sound then takes to reach `then`.
        settle_in: Seconds,
        /// The level the sound goes to, and the level in force after the mark.
        then: i64,
    },
}

/// What a dynamic mark does (H3–H5, H16): the accent it gives the notes at its point, if any, and
/// the level in force after it, if it sets one. O(1).
///
/// - a level: no accent, that level;
/// - *rf*/*rfz* (+1 level, 0.4 s), *sf*/*sfz*/*fz* (+2, 0.6 s), *sffz* (+3, 0.8 s): an accent,
///   the level unchanged;
/// - *fp* (*f* held 0.3 s, then *p* in 0.1 s), *sfp*/*sfpp* (*ff*, then *p*/*pp* in 0.3 s), *pf*
///   (*p* held 0.3 s, then *f* in 0.1 s): an accent, and the second level afterwards;
/// - *n*: level 0 (velocity 1 once bounded, H5);
/// - free text: its level if it has one, else nothing.
///
/// # Errors
/// Only the arithmetic of the constants (never on these values).
fn mark_effect(mark: &DynamicMark) -> Result<(Option<PointAccent>, Option<i64>), RationalError> {
    let up = |levels: i64, tenths: i64| -> Result<_, RationalError> {
        Ok(Some(PointAccent::Up {
            levels,
            settle_in: seconds(tenths, 10)?,
        }))
    };
    let shift = |attack: i64, hold: i64, settle: i64, then: i64| -> Result<_, RationalError> {
        Ok(Some(PointAccent::Shift {
            attack,
            hold: seconds(hold, 10)?,
            settle_in: seconds(settle, 10)?,
            then,
        }))
    };
    Ok(match mark {
        DynamicMark::Level(level) => (None, Some(level_value(*level))),
        DynamicMark::Rf | DynamicMark::Rfz => (up(1, 4)?, None),
        DynamicMark::Sf | DynamicMark::Sfz | DynamicMark::Fz => (up(2, 6)?, None),
        DynamicMark::Sffz => (up(3, 8)?, None),
        DynamicMark::Fp => (shift(F, 3, 1, P)?, Some(P)),
        DynamicMark::Sfp => (shift(FF, 0, 3, P)?, Some(P)),
        DynamicMark::Sfpp => (shift(FF, 0, 3, PP)?, Some(PP)),
        DynamicMark::Pf => (shift(P, 3, 1, F)?, Some(F)),
        DynamicMark::Niente => (None, Some(0)),
        DynamicMark::Other(other) => (None, other.level().map(level_value)),
    })
}

// ------------------------------------------------------------------------------------------------
// The timeline of a voice
// ------------------------------------------------------------------------------------------------

/// A point of the path: the played bar (its index on the path) and the offset inside the written
/// bar it plays. Tuples order lexicographically, which is exactly playing order.
pub(crate) type PathPoint = (u32, Duration);

/// The loudness from a path point on (see the module documentation).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    /// A constant level on the 0–127 scale.
    Const(Rational),
    /// The ramp of the hairpin plan at this index of [`Timeline::plans`].
    Ramp(usize),
}

/// One play of a hairpin: its anchors, (written position, level), ascending in position, at
/// least two (its start and its end). Between two anchors the level is linear in the written
/// position (H7, H8).
#[derive(Debug, Clone, PartialEq, Eq)]
struct Plan {
    /// The anchors: the start with the level in force there, every dynamic inside the hairpin,
    /// the end with the target.
    anchors: Vec<(Position, Rational)>,
}

impl Plan {
    /// The level at written position `w`: the first anchor's before it, the last one's after it,
    /// linear between two anchors. O(log a) for a anchors.
    ///
    /// # Errors
    /// The arithmetic errors.
    fn level_at(&self, w: Position) -> Result<Rational, RationalError> {
        // The first anchor strictly after `w`: the stretch holding `w` ends there.
        let i = self.anchors.partition_point(|&(p, _)| p <= w);
        let (Some(&(p0, l0)), Some(&(p1, l1))) = (
            i.checked_sub(1).and_then(|j| self.anchors.get(j)),
            self.anchors.get(i),
        ) else {
            // Before the first anchor or after the last one: the nearest anchor's level.
            return Ok(if i == 0 {
                self.anchors.first().map_or(Rational::ZERO, |a| a.1)
            } else {
                self.anchors.last().map_or(Rational::ZERO, |a| a.1)
            });
        };
        // p0 <= w < p1, so p1 > p0 and the division is safe.
        let t = w
            .checked_duration_since(p0)?
            .checked_ratio(p1.checked_duration_since(p0)?)?;
        l0.checked_add(l1.checked_sub(l0)?.checked_mul(t)?)
    }

    /// The level at the end: the target (H6). O(1).
    fn target(&self) -> Rational {
        self.anchors.last().map_or(Rational::ZERO, |a| a.1)
    }
}

/// The loudness of one voice along the path (see the module documentation).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Timeline {
    /// From each path point on, the loudness; sorted by point, one entry per point.
    states: Vec<(PathPoint, State)>,
    /// The dynamic accents, sorted by point, one per point (the voice's wins).
    accents: Vec<(PathPoint, PointAccent)>,
    /// The plays of the hairpins.
    plans: Vec<Plan>,
}

/// A dynamic that applies to the voice, as the walk needs it.
#[derive(Debug, Clone, Copy)]
struct Dyn {
    /// Its written bar.
    bar: u32,
    /// Its offset in that bar.
    offset: Duration,
    /// Its written position (bar start + offset).
    pos: Position,
    /// Whether it is the voice's (else the instrument's).
    voice: bool,
    /// Its accent, if any.
    accent: Option<PointAccent>,
    /// The level after it, if it sets one.
    after: Option<i64>,
}

/// A hairpin that applies to the voice, as the walk needs it.
#[derive(Debug, Clone, Copy)]
struct Pin {
    /// The written bar of its start.
    bar: u32,
    /// The offset of its start in that bar.
    offset: Duration,
    /// Its written start (included).
    start: Position,
    /// Its written end (excluded).
    end: Position,
    /// The written bar of its end, and the offset there.
    end_at: (u32, Duration),
    /// The start of the bar after the bar holding its end: the window of its target ends there
    /// (H6).
    window: Position,
    /// Louder or softer.
    kind: HairpinKind,
    /// *Al niente* / *dal niente* (H5).
    niente: bool,
    /// Whether it is the voice's (else the instrument's).
    voice: bool,
}

/// What happens at a point of a written bar, in the order the walk handles them at one offset:
/// hairpin ends first ("cresc. up to here"), then dynamics, then hairpin starts ("dim. from
/// here", from the level just set); within each kind the instrument's before the voice's, so the
/// voice's win.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Mark {
    /// The end of the hairpin at this index (only ends inside a bar: an end on a barline is the
    /// start of the next bar, handled there).
    End(usize),
    /// The dynamic at this index.
    Dyn(usize),
    /// The start of the hairpin at this index.
    Start(usize),
}

/// A hairpin being played.
#[derive(Debug, Clone, Copy)]
struct Active {
    /// The hairpin (index in the walk's list).
    pin: usize,
    /// Its plan (index in [`Timeline::plans`]).
    plan: usize,
}

impl Timeline {
    /// The timeline of `voice` of `instrument` along `path` (see the module documentation).
    ///
    /// # Complexity
    /// O(m log m) to sort the m dynamics and hairpins that apply, O(path) for the walk plus
    /// O(log m + inner dynamics) per hairpin played.
    ///
    /// # Errors
    /// The document's and the maps' errors (a bug of the model), the arithmetic errors.
    #[allow(
        clippy::too_many_lines,
        reason = "one walk along the path, with the three kinds of marks handled in reading order"
    )]
    pub(crate) fn new(
        doc: &Document,
        path: &Unrolled,
        instrument: &Instrument,
        voice: VoiceId,
    ) -> Result<Self, PerformError> {
        let bars = doc.bars();
        let meter = doc.meter_map();
        let applies = |scope: DynamicScope| match scope {
            DynamicScope::Instrument => Some(false),
            DynamicScope::Voice(v) if v == voice => Some(true),
            DynamicScope::Voice(_) => None,
        };
        let index_of = |bar| -> Result<u32, PerformError> {
            Ok(bars
                .index_of(bar)
                .ok_or(DocumentError::UnknownBar(bar))?
                .get())
        };

        // The dynamics that apply, sorted by written position (then the voice's after the
        // instrument's, so that a search for "the first at or after" finds the voice's last).
        let mut dyns: Vec<Dyn> = Vec::new();
        for d in instrument.dynamics() {
            let Some(is_voice) = applies(d.placement().scope) else {
                continue;
            };
            let at = d.at();
            let bar = index_of(at.bar)?;
            let (accent, after) = mark_effect(d.mark())?;
            dyns.push(Dyn {
                bar,
                offset: at.offset,
                pos: meter
                    .bar_start(BarIndex::new(bar))?
                    .checked_add(at.offset)?,
                voice: is_voice,
                accent,
                after,
            });
        }
        dyns.sort_by(|a, b| a.pos.cmp(&b.pos).then(a.voice.cmp(&b.voice)));

        // The hairpins that apply.
        let mut pins: Vec<Pin> = Vec::new();
        for h in instrument.hairpins() {
            let Some(is_voice) = applies(h.placement().scope) else {
                continue;
            };
            let (start, end) = (h.start(), h.end());
            let (sb, eb) = (index_of(start.bar)?, index_of(end.bar)?);
            pins.push(Pin {
                bar: sb,
                offset: start.offset,
                start: meter
                    .bar_start(BarIndex::new(sb))?
                    .checked_add(start.offset)?,
                end: meter
                    .bar_start(BarIndex::new(eb))?
                    .checked_add(end.offset)?,
                end_at: (eb, end.offset),
                window: meter.bar_start(BarIndex::new(eb.saturating_add(1)))?,
                kind: h.shape().kind,
                niente: h.shape().niente,
                voice: is_voice,
            });
        }

        // The marks of every written bar, sorted as the walk handles them: by offset, then ends,
        // dynamics, starts; within a kind the instrument's first.
        let mut by_bar: Vec<Vec<(Duration, Mark, bool)>> = vec![Vec::new(); bars.len()];
        let mut put = |bar: u32, entry: (Duration, Mark, bool)| {
            if let Some(list) = usize::try_from(bar).ok().and_then(|b| by_bar.get_mut(b)) {
                list.push(entry);
            }
        };
        for (k, d) in dyns.iter().enumerate() {
            put(d.bar, (d.offset, Mark::Dyn(k), d.voice));
        }
        for (k, p) in pins.iter().enumerate() {
            put(p.bar, (p.offset, Mark::Start(k), p.voice));
            if p.end_at.1.is_positive() {
                put(p.end_at.0, (p.end_at.1, Mark::End(k), p.voice));
            }
        }
        for list in &mut by_bar {
            list.sort_by(|a, b| {
                a.0.cmp(&b.0)
                    .then(kind_rank(a.1).cmp(&kind_rank(b.1)))
                    .then(a.2.cmp(&b.2))
            });
        }

        // The walk.
        let mut timeline = Self::default();
        let mut level = Rational::from_integer(MF)?;
        // The hairpin being played of the instrument, and of the voice.
        let mut active: [Option<Active>; 2] = [None, None];
        for (k, played) in path.played().iter().enumerate() {
            let k = u32::try_from(k).map_err(|_| UnrollError::Endless)?;
            let bar_start = meter.bar_start(played.index)?;
            // A hairpin whose range does not hold this bar's start stops here (a barline end, a
            // repeat back, a jump): its target stays in force (H7).
            if timeline.stop_outside(&pins, &mut active, &mut level, bar_start) {
                timeline.push_state((k, Duration::ZERO), effective(&active, level));
            }
            let marks = usize::try_from(played.index.get())
                .ok()
                .and_then(|b| by_bar.get(b))
                .map_or(&[][..], Vec::as_slice);
            for &(offset, mark, _) in marks {
                let point = (k, offset);
                let w = bar_start.checked_add(offset)?;
                timeline.stop_outside(&pins, &mut active, &mut level, w);
                match mark {
                    // Its range check above already stopped it.
                    Mark::End(_) => {}
                    Mark::Dyn(d) => {
                        let Some(d) = dyns.get(d) else { continue };
                        if let Some(accent) = d.accent {
                            timeline.push_accent(point, accent);
                        }
                        if let Some(after) = d.after {
                            level = Rational::from_integer(after)?;
                        }
                    }
                    Mark::Start(p) => {
                        let Some(pin) = pins.get(p) else { continue };
                        // The level it starts from: what is heard there now (another scope's
                        // ramp, or the level in force), or nothing for *dal niente* (H5).
                        let from = if pin.niente && pin.kind == HairpinKind::Crescendo {
                            Rational::ZERO
                        } else {
                            timeline.effective_level(&active, level, w)?
                        };
                        let plan = plan(&dyns, pin, from)?;
                        timeline.plans.push(plan);
                        let slot = usize::from(pin.voice);
                        if let Some(a) = active.get_mut(slot) {
                            *a = Some(Active {
                                pin: p,
                                plan: timeline.plans.len().saturating_sub(1),
                            });
                        }
                    }
                }
                timeline.push_state(point, effective(&active, level));
            }
        }
        Ok(timeline)
    }

    /// Stops every active hairpin whose written range does not hold `w`, setting the level in
    /// force to its target (H7). Returns whether one stopped. O(1).
    fn stop_outside(
        &self,
        pins: &[Pin],
        active: &mut [Option<Active>; 2],
        level: &mut Rational,
        w: Position,
    ) -> bool {
        let mut stopped = false;
        // The instrument's first, then the voice's: when both stop at once, the voice's target is
        // the one left in force (the more specific).
        for slot in active.iter_mut() {
            let Some(a) = *slot else { continue };
            let inside = pins.get(a.pin).is_some_and(|p| p.start <= w && w < p.end);
            if !inside {
                if let Some(plan) = self.plans.get(a.plan) {
                    *level = plan.target();
                }
                *slot = None;
                stopped = true;
            }
        }
        stopped
    }

    /// The level heard at written position `w` given the hairpins being played: the voice's ramp,
    /// else the instrument's, else the level in force (H9). O(log a).
    fn effective_level(
        &self,
        active: &[Option<Active>; 2],
        level: Rational,
        w: Position,
    ) -> Result<Rational, RationalError> {
        match effective(active, level) {
            State::Const(l) => Ok(l),
            State::Ramp(plan) => self.plans.get(plan).map_or(Ok(level), |p| p.level_at(w)),
        }
    }

    /// Records the state from `point` on, replacing one recorded at the same point (the last
    /// mark there decides). O(1) amortised.
    fn push_state(&mut self, point: PathPoint, state: State) {
        if let Some(last) = self.states.last_mut()
            && last.0 == point
        {
            last.1 = state;
            return;
        }
        self.states.push((point, state));
    }

    /// Records a dynamic accent at `point`, replacing one recorded there (the instrument's is
    /// pushed first, so the voice's wins). O(1) amortised.
    fn push_accent(&mut self, point: PathPoint, accent: PointAccent) {
        if let Some(last) = self.accents.last_mut()
            && last.0 == point
        {
            last.1 = accent;
            return;
        }
        self.accents.push((point, accent));
    }

    /// The level in force at path point `at`, whose written position is `w` (H1, H2, H8): the
    /// state of the last point at or before `at`, *mf* before the first. O(log s + log a).
    ///
    /// # Errors
    /// The arithmetic errors.
    pub(crate) fn level_at(&self, at: PathPoint, w: Position) -> Result<Rational, RationalError> {
        let i = self.states.partition_point(|&(p, _)| p <= at);
        match i.checked_sub(1).and_then(|j| self.states.get(j)) {
            None => Rational::from_integer(MF),
            Some(&(_, State::Const(l))) => Ok(l),
            Some(&(_, State::Ramp(plan))) => self
                .plans
                .get(plan)
                .map_or(Rational::from_integer(MF), |p| p.level_at(w)),
        }
    }

    /// The dynamic accent written exactly at path point `at`, if any (H3, H4). O(log n).
    pub(crate) fn accent_at(&self, at: PathPoint) -> Option<PointAccent> {
        self.accents
            .binary_search_by(|(p, _)| p.cmp(&at))
            .ok()
            .and_then(|i| self.accents.get(i))
            .map(|&(_, a)| a)
    }
}

/// The order of the kinds of marks at one offset (see [`Mark`]). O(1).
const fn kind_rank(mark: Mark) -> u8 {
    match mark {
        Mark::End(_) => 0,
        Mark::Dyn(_) => 1,
        Mark::Start(_) => 2,
    }
}

/// The state heard now: the voice's hairpin, else the instrument's, else the constant level
/// (H9). O(1).
fn effective(active: &[Option<Active>; 2], level: Rational) -> State {
    match active {
        [_, Some(voice)] => State::Ramp(voice.plan),
        [Some(instrument), None] => State::Ramp(instrument.plan),
        [None, None] => State::Const(level),
    }
}

/// The plan of one play of a hairpin starting from level `from` (H6, H7): its start, every
/// dynamic with a level strictly inside it, and the target at its end. O(log m + inner).
///
/// # Errors
/// The arithmetic errors.
fn plan(dyns: &[Dyn], pin: &Pin, from: Rational) -> Result<Plan, RationalError> {
    let mut anchors = vec![(pin.start, from)];
    // The dynamics strictly inside, in written order (they are sorted by position).
    let first = dyns.partition_point(|d| d.pos <= pin.start);
    for d in dyns.get(first..).unwrap_or(&[]) {
        if d.pos >= pin.end {
            break;
        }
        if let Some(after) = d.after {
            let level = Rational::from_integer(after)?;
            // At one position the later entry (the voice's) wins.
            if let Some(last) = anchors.last_mut()
                && last.0 == d.pos
            {
                last.1 = level;
            } else {
                anchors.push((d.pos, level));
            }
        }
    }
    let last = anchors.last().map_or(from, |a| a.1);
    // H6: the first dynamic with a level at or after the end, within the bar holding the end (at
    // one position the voice's, sorted after the instrument's), if it goes the right way.
    let at_end = dyns.partition_point(|d| d.pos < pin.end);
    let mut written: Option<(Position, i64)> = None;
    for d in dyns.get(at_end..).unwrap_or(&[]) {
        if d.pos >= pin.window || written.is_some_and(|(p, _)| p != d.pos) {
            break;
        }
        if let Some(after) = d.after {
            written = Some((d.pos, after));
        }
    }
    let right_way = |t: Rational| match pin.kind {
        HairpinKind::Crescendo => t > last,
        HairpinKind::Diminuendo => t < last,
    };
    let target = match written {
        Some((_, t)) if right_way(Rational::from_integer(t)?) => Rational::from_integer(t)?,
        _ => match pin.kind {
            HairpinKind::Crescendo => level_above(last)?,
            // *Al niente* goes to nothing (H5).
            HairpinKind::Diminuendo if pin.niente => Rational::ZERO,
            HairpinKind::Diminuendo => level_below(last)?,
        },
    };
    anchors.push((pin.end, target));
    Ok(Plan { anchors })
}

// ------------------------------------------------------------------------------------------------
// The attack profile of a note (H14–H17)
// ------------------------------------------------------------------------------------------------

/// The loudness of one played note: its attack, how long that is held, how long the sound then
/// takes to settle, and the level it settles to (H14), on the 16-bit scale, bounded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Profile {
    /// The attack velocity, 1..=65 535.
    pub(crate) velocity: u16,
    /// How long the attack level is held.
    pub(crate) hold: Seconds,
    /// How long the sound then takes to reach `settle_to`, linearly.
    pub(crate) settle_in: Seconds,
    /// The level the sound settles to, 1..=65 535.
    pub(crate) settle_to: u16,
}

/// The step of an articulation accent on the 0–127 scale and the time to settle back (H15):
/// soft accent and stress +7 in 0.15 s, accent +15 in 0.2 s, marcato +22 in 0.25 s. Unstress is
/// handled apart (−7 on the whole note, no profile). Steps add; the time is the longest. O(1).
///
/// # Errors
/// Only the arithmetic of the constants.
fn articulation_step(set: ArticulationSet) -> Result<(i64, Seconds), RationalError> {
    use orchestra_core_model::Articulation;
    let mut step = 0_i64;
    let mut time = Seconds::ZERO;
    for (articulation, add, hundredths) in [
        (Articulation::SoftAccent, 7, 15),
        (Articulation::Stress, 7, 15),
        (Articulation::Accent, 15, 20),
        (Articulation::Marcato, 22, 25),
    ] {
        if set.contains(articulation) {
            step = step.saturating_add(add);
            time = time.max(seconds(hundredths, 100)?);
        }
    }
    Ok((step, time))
}

/// The step of an unstress on the whole note, on the 0–127 scale (H15).
const UNSTRESS: i64 = -7;

/// The attack profile of a note (H14–H17) from the level in force at its point (`base`, 0–127),
/// the dynamic accent written there (`accent`; never for grace notes, H10), the articulations of
/// its first notehead and its velocity offset (16-bit steps). O(1).
///
/// # The order
/// 1. the dynamic accent sets the attack, the hold, the settling time and level (H16): by
///    default the attack and the settling level are the level in force, with no profile;
/// 2. an articulation accent adds its step to the attack and lengthens the settling time to its
///    own if longer (H15, H17); an unstress lowers both the attack and the settling level;
/// 3. both levels are bounded to 0..=127 and up-scaled (H8), the velocity offset is added to both,
///    and both are bounded to 1..=65 535 (§8);
/// 4. if the attack and the settling level are then equal, the note has no profile (hold and
///    settling time zero), so equal sounds compare equal.
///
/// # Errors
/// The arithmetic errors.
pub(crate) fn profile(
    base: Rational,
    accent: Option<PointAccent>,
    articulations: ArticulationSet,
    offset: i32,
) -> Result<Profile, RationalError> {
    let int = Rational::from_integer;
    let (mut attack, mut settle_to) = (base, base);
    let (mut hold, mut settle_in) = (Seconds::ZERO, Seconds::ZERO);
    match accent {
        None => {}
        Some(PointAccent::Up {
            levels,
            settle_in: time,
        }) => {
            attack = base
                .checked_add(int(LEVEL_STEP.saturating_mul(levels))?)?
                .min(int(FFFF)?);
            settle_in = time;
        }
        Some(PointAccent::Shift {
            attack: a,
            hold: h,
            settle_in: s,
            then,
        }) => {
            attack = int(a)?;
            hold = h;
            settle_in = s;
            settle_to = int(then)?;
        }
    }
    let (step, time) = articulation_step(articulations)?;
    if step > 0 {
        attack = attack.checked_add(int(step)?)?;
        settle_in = settle_in.max(time);
    }
    if articulations.contains(orchestra_core_model::Articulation::Unstress) {
        attack = attack.checked_add(int(UNSTRESS)?)?;
        settle_to = settle_to.checked_add(int(UNSTRESS)?)?;
    }
    let bounded = |x: Rational| -> Result<u16, RationalError> {
        let up = upscale(x.max(Rational::ZERO).min(int(FFFF)?))?;
        let v = i64::from(up)
            .saturating_add(i64::from(offset))
            .clamp(1, 65_535);
        Ok(u16::try_from(v).unwrap_or(u16::MAX))
    };
    let (velocity, settle_to) = (bounded(attack)?, bounded(settle_to)?);
    if velocity == settle_to {
        return Ok(Profile {
            velocity,
            hold: Seconds::ZERO,
            settle_in: Seconds::ZERO,
            settle_to,
        });
    }
    Ok(Profile {
        velocity,
        hold,
        settle_in,
        settle_to,
    })
}

/// The 16-bit bit replication of a 7-bit integer, `v << 9 | v << 2 | v >> 5` (ADR-0027 §4). The
/// argument is in 0..=127, so the result fits a `u16` (127 gives 65 535). O(1).
const fn replicate(v: u16) -> u16 {
    (v << 9) | (v << 2) | (v >> 5)
}

/// Up-scales a level on the 0–127 scale (0 ≤ `x` ≤ 127) to the 16-bit scale (H8): an integer by
/// bit replication, a fraction linearly between its two neighbouring integers' values, rounded to
/// the nearest with halves up. O(1).
///
/// # Errors
/// The arithmetic errors.
pub(crate) fn upscale(x: Rational) -> Result<u16, RationalError> {
    let low = x.floor().clamp(0, FFFF);
    let high = low.saturating_add(1).min(FFFF);
    let (lo, hi) = (
        replicate(u16::try_from(low).unwrap_or(0)),
        replicate(u16::try_from(high).unwrap_or(127)),
    );
    // The fraction of the way from `low` to `high`, in [0, 1).
    let frac = x.checked_sub(Rational::from_integer(low)?)?;
    // lo + frac × (hi − lo), rounded to nearest with halves up: ⌊… + 1/2⌋.
    let value = Rational::from_integer(i64::from(lo))?
        .checked_add(frac.checked_mul(Rational::from_integer(
            i64::from(hi).saturating_sub(i64::from(lo)),
        )?)?)?
        .checked_add(Rational::new(1, 2)?)?
        .floor();
    Ok(u16::try_from(value.clamp(0, 65_535)).unwrap_or(u16::MAX))
}

/// The velocity of *mf* on the 16-bit scale: the level in force before any dynamic (H1).
pub const INITIAL_VELOCITY: u16 = replicate(75);
