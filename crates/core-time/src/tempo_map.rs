// SPDX-License-Identifier: GPL-3.0-or-later

//! The tempo map: written positions ⇄ seconds (ADR-0020, amended by ADR-0021).
//!
//! # What it answers
//! - at what time a written position sounds ([`TempoMap::time_at`]): **exact** inside
//!   constant-tempo segments and at the start of every segment, an approximate `f64` inside a
//!   gradual change, where the true value is irrational (ADR-0020 §4); the answer says which of
//!   the two it is ([`TimeAt`]);
//! - which written position sounds at a given time ([`TempoMap::position_at`]): exact inside
//!   constant-tempo segments, rounded to the [`TIME_GRID`] of whole notes inside a gradual change
//!   (ADR-0021 §4).
//!
//! - at which audio sample and in which video frame a position sounds ([`TempoMap::sample_at`],
//!   [`TempoMap::frame_at`]): **exact** everywhere in practice, also inside gradual changes, where
//!   an `f64` estimate is checked exactly (ADR-0020 §5, ADR-0021 §5); the answer says whether the
//!   check could be completed ([`IndexAt`]); and back ([`TempoMap::position_of_sample`],
//!   [`TempoMap::position_of_frame`]).
//!
//! Like the meter map, the tempo map describes the **written** score: repeats are unrolled by
//! `orchestra-playback` (ADR-0020 §3).
//!
//! # Exact indices inside gradual changes (ADR-0021 §5)
//! The time of a point inside a gradual change is irrational, but its index is not: index `n`
//! covers a time interval whose ends are rational, and the **position** reached at a rational time
//! is exact (the polynomial). Since position grows strictly with time, "the point sounds during
//! index `n`" is the same as "the point lies between the positions reached when index `n` begins
//! and ends", which is two exact comparisons. The `f64` time gives a first guess, almost always
//! right; the comparisons confirm it or move it by one, at most four times (a fixed bound, so the
//! lookup's cost is bounded on the audio thread). If the `i128` arithmetic overflows, the guess is returned as [`IndexAt::Estimated`]. A long test score
//! (2 h 16 min, 500 gradual changes, marks with large prime factors) needed no estimate at any
//! common sample or frame rate, nor at 384 kHz (`tests/tempo_map_fallback.rs`).
//!
//! # Source data and derived segments
//! The map holds the **source data** that the document stores and the commands edit: a list of
//! tempo changes sorted by position, at most one per position, the first an immediate change at
//! position 0 (the initial tempo). A change is either immediate ("from here, ♩ = 96") or gradual
//! ("from here to there, move linearly in time to ♩ = 60", optionally jumping first to a starting
//! tempo: "Meno mosso, poco a poco accel.", ADR-0021 §6).
//!
//! From them it derives a sorted vector of **segments**, each constant or gradual, with its start
//! position and its start time. After a gradual change the target tempo holds until the next
//! change, so a gradual change that is not immediately followed by another change is followed by
//! an implicit constant segment:
//!
//! ```text
//! changes:   0: ♩=120        8: rit. to ♩=60 until 12        16: ♩=96
//! segments: [ constant 120 ][ gradual 120 → 60 ][ constant 60 ][ constant 96 … forever
//! positions: 0               8                   12             16
//! ```
//!
//! The first segment is always constant and also covers negative positions and times (pre-roll,
//! notes played early: the first tempo extended backwards, ADR-0020 §3). The last segment is
//! always constant and continues forever.
//!
//! # Start times on a grid (ADR-0021 §1)
//! Each segment's start time is the previous start plus the **exact** duration of the previous
//! segment, rounded to the nearest point of a grid of `1/`[`TIME_GRID`] seconds (ties to the later
//! point). Exact cumulated times would overflow any fixed-width fraction after a few dozen gradual
//! changes; on the grid the error is at most half a tick (≈ 0.7 ns) per boundary, deterministic,
//! and the same for every track. Because the previous start is already on the grid, rounding the
//! sum is the same as adding the rounded duration, so start times are kept as an integer number
//! of ticks and never need fractions with large denominators.
//!
//! A segment therefore spans the times `[t_k, t_{k+1})`, which may differ from its exact duration
//! by up to half a tick. Both conversions **clamp** to the segment (ADR-0021 §2), so that they stay
//! non-decreasing across boundaries: a position just before the next segment never maps to a time
//! after the next segment's start, and a time just before the next segment's start never maps to
//! a position after the next segment's start.
//!
//! # Gradual changes (ADR-0020 §1)
//! Inside a gradual change of length `L` whole notes, from speed `s₀` to speed `s₁` (whole notes
//! per second), the speed grows linearly **in time**: constant acceleration `a`. With `τ` the time
//! since the start of the change and `x` the position since its start:
//!
//! - duration `D = 2L / (s₀ + s₁)` (rational);
//! - `x(τ) = s₀·τ + h·τ²` with `h = a/2 = (s₁² − s₀²) / (4L)` (rational, exact for a rational `τ`);
//! - `τ(x) = 2x / (s₀ + √(s₀² + 4h·x))` (irrational in general; computed in `f64`, in this form
//!   to avoid the cancellation of `(−s₀ + √…)/2h` when `h` is small).
//!
//! # Editing: all or nothing
//! As in the meter map (ADR-0018 §4), every edit rebuilds the segments from a modified copy of the
//! source data and validates the whole result; only if it is valid does it replace the map, and
//! each edit returns the previous value, which is what an undo needs. Rebuilding is O(n) after the
//! O(log n) search for the edited entry (O(n) to shift the list), in the number `n` of tempo
//! changes, and allocates: it runs on the editing path, never on the audio thread.
//!
//! # Lookups: no allocation
//! Every lookup takes `&self`, allocates nothing and runs in O(log n): a binary search on the
//! segment starts (positions or times), then O(1) arithmetic. They return a [`TempoMapError`]
//! instead of panicking, also on arithmetic overflow.

use std::cmp::Ordering;
use std::error::Error;
use std::fmt;

use crate::clock::{FrameRate, SampleRate, Seconds};
use crate::position::{Duration, Position};
use crate::rational::{Rational, RationalError, gcd_u128};
use crate::tempo::Tempo;

/// The resolution of the tempo map's grids (ADR-0021 §1, §4): **705 600 000** ticks per second for
/// the start times of segments, and the same number of ticks per whole note for the positions
/// rounded inside gradual changes.
///
/// `705 600 000 = 2⁹ · 3² · 5⁵ · 7²` (the "flick"): it divides every common sample rate (8 kHz to
/// 192 kHz, including 44.1 kHz) and every common integer frame rate (24 to 240 fps). A tick lasts
/// about 1.4 ns; an `i64` number of ticks spans about 3.6 million hours.
pub const TIME_GRID: i64 = 705_600_000;

/// [`TIME_GRID`] widened to `i128`, for the intermediate products of the rounding helpers.
const GRID_WIDE: i128 = TIME_GRID as i128;

/// A gradual tempo change (accelerando, ritardando), linear in time (ADR-0020 §1, ADR-0021 §6).
///
/// The change starts at the position where it is stored in the map and ends at `end`, where `to`
/// is reached; `to` then holds until the next change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GradualChange {
    /// The tempo to jump to at the start, before moving gradually ("Meno mosso, poco a poco
    /// accel."); `None` starts from the tempo in force there.
    pub from: Option<Tempo>,
    /// The tempo reached at `end`.
    pub to: Tempo,
    /// Where the change ends; must be after its start.
    pub end: Position,
}

/// A tempo change: the source data of the tempo map.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TempoChange {
    /// From its position on, the tempo is this one.
    Immediate(Tempo),
    /// From its position to `end`, the speed moves linearly in time to a new tempo.
    Gradual(GradualChange),
}

/// The time of a position, exact or approximate (ADR-0021 §3).
///
/// The two cases are kept apart in the type so that a caller can never mistake an approximation
/// for an exact value: an approximate time is for display only and must never flow back into the
/// model (ADR-0015 §8).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TimeAt {
    /// The exact time: constant-tempo segments, and the start of every segment.
    Exact(Seconds),
    /// An approximation in seconds: inside a gradual change, where the time is irrational.
    Approximate(f64),
}

impl TimeAt {
    /// The time in seconds as an `f64`, exact or not: for display only. O(1).
    #[must_use]
    pub fn to_f64(self) -> f64 {
        match self {
            Self::Exact(seconds) => seconds.to_f64(),
            Self::Approximate(seconds) => seconds,
        }
    }

    /// The exact time, if this is one. O(1).
    #[must_use]
    pub const fn exact(self) -> Option<Seconds> {
        match self {
            Self::Exact(seconds) => Some(seconds),
            Self::Approximate(_) => None,
        }
    }
}

/// A sample or frame index, exact or estimated (ADR-0020 §5, ADR-0021 §5).
///
/// Indices are exact everywhere in practice, also inside gradual changes, where they are found
/// with an `f64` estimate followed by an exact check. Only when the exact check overflows (or the
/// estimate is more than a few indices off, which would take an absurd tempo) is the estimate
/// returned unchecked, as `Estimated`: like [`TimeAt`], the type never hides an approximation.
/// The audio thread simply uses [`value`](Self::value); the tests count the estimates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IndexAt {
    /// The exact index under the rounding rule (nearest sample, or the frame containing the time).
    Exact(i64),
    /// An `f64` estimate whose exact check could not be completed; off by at most one index for
    /// any realistic score.
    Estimated(i64),
}

impl IndexAt {
    /// The index, exact or not. O(1).
    #[must_use]
    pub const fn value(self) -> i64 {
        match self {
            Self::Exact(index) | Self::Estimated(index) => index,
        }
    }

    /// `true` if the index was checked exactly. O(1).
    #[must_use]
    pub const fn is_exact(self) -> bool {
        matches!(self, Self::Exact(_))
    }
}

/// Why a tempo map edit or lookup failed.
///
/// The `Display` texts are English and meant for developers; the interface maps each variant to
/// a Fluent key (CLAUDE.md §7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TempoMapError {
    /// The exact arithmetic overflowed (ADR-0015): a value too large to represent.
    Arithmetic(RationalError),
    /// A tempo change at a negative position: the score starts at 0 (earlier times use the first
    /// tempo extended backwards).
    BeforeStart,
    /// The change at position 0 cannot be removed: some tempo must always be in force.
    NoTempoAtStart,
    /// A gradual change at position 0: the first change must be the initial tempo (ADR-0021 §6).
    GradualAtStart,
    /// A gradual change whose end is not after its start.
    NonPositiveGradual {
        /// The start of the offending change.
        at: Position,
    },
    /// A change strictly inside a gradual change (ADR-0020 §3).
    ChangeInsideGradual {
        /// The position of the change that falls inside.
        at: Position,
        /// The start of the gradual change it falls into.
        gradual: Position,
    },
}

impl From<RationalError> for TempoMapError {
    /// Wraps an arithmetic error, so that `?` works on rational operations inside the map.
    fn from(error: RationalError) -> Self {
        Self::Arithmetic(error)
    }
}

impl fmt::Display for TempoMapError {
    /// Writes a short English description of the error, for developers.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Arithmetic(error) => write!(f, "tempo map arithmetic failed: {error}"),
            Self::BeforeStart => f.write_str("tempo change before the start of the score"),
            Self::NoTempoAtStart => f.write_str("the tempo change at position 0 cannot be removed"),
            Self::GradualAtStart => {
                f.write_str("the first tempo change must be an immediate one at position 0")
            }
            Self::NonPositiveGradual { at } => {
                write!(
                    f,
                    "the gradual tempo change at {at:?} must end after it starts"
                )
            }
            Self::ChangeInsideGradual { at, gradual } => write!(
                f,
                "the tempo change at {at:?} falls inside the gradual change starting at {gradual:?}"
            ),
        }
    }
}

impl Error for TempoMapError {
    /// The arithmetic error behind an `Arithmetic` failure, if any.
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Arithmetic(error) => Some(error),
            _ => None,
        }
    }
}

/// The overflow error of this module's own `i128` arithmetic, which has no `RationalError` of
/// its own to forward.
const OVERFLOW: TempoMapError = TempoMapError::Arithmetic(RationalError::Overflow);

/// How the speed behaves inside a segment, with the values its conversions need precomputed.
///
/// The written tempi (`tempo`, `from`, `to`) are not needed by the conversions, which only use
/// speeds; they are kept so that the segment describes the music as written, for the tempo
/// queries of the interface and the metronome (later steps), without a second lookup in the
/// source data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Shape {
    /// A constant tempo.
    Constant {
        /// The tempo as written.
        tempo: Tempo,
        /// Its speed in whole notes per second; > 0.
        speed: Rational,
    },
    /// A gradual change, linear in time (see the module documentation for the formulas).
    Gradual {
        /// The starting tempo (explicit, or the one in force).
        from: Tempo,
        /// The tempo reached at the end.
        to: Tempo,
        /// The length `L` in whole notes; > 0.
        length: Duration,
        /// The starting speed `s₀`; > 0.
        start_speed: Rational,
        /// The exact duration `D = 2L / (s₀ + s₁)`; > 0.
        duration: Rational,
        /// Half the acceleration, `h = (s₁² − s₀²) / (4L)`; may be negative or zero.
        half_acceleration: Rational,
    },
}

impl Shape {
    /// A constant segment at `tempo`.
    fn constant(tempo: Tempo) -> Result<Self, TempoMapError> {
        Ok(Self::Constant {
            tempo,
            speed: tempo.speed()?,
        })
    }

    /// A gradual segment from `from` to `to` over `length` (> 0) whole notes.
    ///
    /// The duration and half the acceleration are computed once here, on the editing path, so
    /// that the lookups only multiply and add.
    fn gradual(from: Tempo, to: Tempo, length: Duration) -> Result<Self, TempoMapError> {
        let s0 = from.speed()?;
        let s1 = to.speed()?;
        let l = length.as_rational();
        let sum = s0.checked_add(s1)?;
        // D = 2L / (s₀ + s₁): positive because L > 0 and both speeds are positive.
        let duration = l.checked_add(l)?.checked_div(sum)?;
        // h = a/2 with a = (s₁ − s₀)/D, i.e. h = (s₁ − s₀)(s₀ + s₁) / (4L) = (s₁² − s₀²) / (4L).
        let four_l = l.checked_mul(Rational::from_integer(4)?)?;
        let half_acceleration = s1.checked_sub(s0)?.checked_mul(sum)?.checked_div(four_l)?;
        Ok(Self::Gradual {
            from,
            to,
            length,
            start_speed: s0,
            duration,
            half_acceleration,
        })
    }
}

/// A run of the score with one tempo behaviour, and where it starts in both time domains.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct Segment {
    /// The written position where the segment starts.
    start: Position,
    /// The time where it starts, in ticks of `1/TIME_GRID` s (ADR-0021 §1).
    start_ticks: i64,
    /// The same time as `Seconds`, for the binary search and the exact arithmetic: always
    /// `start_ticks / TIME_GRID`, kept so that the lookups do not rebuild it.
    start_time: Seconds,
    /// How the speed behaves.
    shape: Shape,
}

impl Segment {
    /// The exact time of the point `x` after the start of this **constant** segment at `speed`:
    /// `t_k + x/s`, clamped to the start of `next` (ADR-0021 §2: the rounded next start may come
    /// up to half a tick before the exact end of this segment).
    fn constant_time(
        &self,
        next: Option<&Self>,
        x: Duration,
        speed: Rational,
    ) -> Result<Seconds, TempoMapError> {
        let time = self
            .start_time
            .checked_add(Seconds::from_rational(x.as_rational().checked_div(speed)?))?;
        Ok(match next {
            Some(next) if time > next.start_time => next.start_time,
            _ => time,
        })
    }

    /// The approximate time of the point `x` after the start of this **gradual** segment, clamped
    /// to the start of `next` like [`constant_time`](Self::constant_time).
    ///
    /// `τ(x) = 2x / (s₀ + √(s₀² + 4h·x))`. Inside the segment the radicand is the square of the
    /// speed reached at `x`, so it lies between `s₀²` and `s₁²`, both positive; the `max(0)` only
    /// guards against rounding.
    fn gradual_time_f64(&self, next: Option<&Self>, x: Duration, s0: Rational, h: Rational) -> f64 {
        let x = x.as_rational().to_f64();
        let s0 = s0.to_f64();
        let h = h.to_f64();
        let radicand = (s0 * s0 + 4.0 * h * x).max(0.0);
        let tau = 2.0 * x / (s0 + radicand.sqrt());
        let time = self.start_time.to_f64() + tau;
        // A gradual segment is never the last one, so `next` always exists.
        next.map_or(time, |next| time.min(next.start_time.to_f64()))
    }

    /// Where the point `x` (with `0 < x < L`) of this gradual segment lies with respect to the
    /// times of index `n` of `grid`: `Less` if it sounds before index `n` begins, `Greater` if at
    /// or after index `n + 1` begins, `Equal` if index `n` is its index. `None` if the exact
    /// arithmetic overflows.
    ///
    /// Since the position grows strictly with time inside the segment, "the time of `x` is before
    /// `T`" is the same as "`x` is before the position reached at `T`", and the latter is exact
    /// (ADR-0020 §5). Times outside the segment are clamped: before its start the position is 0,
    /// after its exact end it is `L` (the polynomial must not be used there, because a slowing
    /// change would turn back).
    fn locate_in_index(
        &self,
        x: Duration,
        n: i64,
        grid: IndexGrid,
        values: (Duration, Rational, Rational, Rational),
    ) -> Option<Ordering> {
        let (length, s0, duration, h) = values;
        let x = x.as_rational();
        // The position reached at time `t`, compared with `x`.
        let compare_at = |t: Rational| -> Option<Ordering> {
            let tau = t.checked_sub(self.start_time.as_rational()).ok()?;
            if !tau.is_positive() {
                // Position 0 ≤ x.
                return Some(Ordering::Less);
            }
            if tau >= duration {
                // Position L > x.
                return Some(length.as_rational().cmp(&x));
            }
            let (numerator, denominator) = gradual_offset_exact(tau, s0, h).ok()?;
            compare_exact(numerator, denominator, x)
        };
        let start = grid.start_of(n).ok()?;
        if compare_at(start)? == Ordering::Greater {
            // The index begins at a position after x: x sounds earlier.
            return Some(Ordering::Less);
        }
        let end = grid.start_of(n.checked_add(1)?).ok()?;
        if compare_at(end)? == Ordering::Greater {
            Some(Ordering::Equal)
        } else {
            // The next index begins at or before x.
            Some(Ordering::Greater)
        }
    }
}

/// The tempo map of a project (ADR-0005 §2, ADR-0020, ADR-0021).
///
/// See the module documentation for how it is organised and what it costs.
///
/// # Examples
/// ```
/// use orchestra_core_time::{
///     Duration, GradualChange, Position, Rational, Seconds, Tempo, TempoChange, TempoMap, TimeAt,
/// };
///
/// type Res<T> = Result<T, Box<dyn std::error::Error>>;
/// // ♩ = `bpm`, and the position `n/d` whole notes.
/// fn tempo(bpm: i64) -> Res<Tempo> {
///     let quarter = Duration::from_rational(Rational::new(1, 4)?);
///     Ok(Tempo::new(quarter, Rational::from_integer(bpm)?)?)
/// }
/// fn at(n: i64, d: i64) -> Res<Position> {
///     Ok(Position::from_rational(Rational::new(n, d)?))
/// }
/// fn secs(n: i64, d: i64) -> Res<Seconds> {
///     Ok(Seconds::from_rational(Rational::new(n, d)?))
/// }
///
/// // ♩ = 120, then from position 2 a ritardando to ♩ = 60 over 4 whole notes.
/// let mut map = TempoMap::new(tempo(120)?)?;
/// let rit = GradualChange { from: None, to: tempo(60)?, end: at(6, 1)? };
/// map.set_change(at(2, 1)?, TempoChange::Gradual(rit))?;
/// // Two whole notes at ♩ = 120 last 4 s exactly; the ritardando lasts 2L/(s₀+s₁) = 32/3 s.
/// assert_eq!(map.time_at(at(2, 1)?)?, TimeAt::Exact(secs(4, 1)?));
/// assert_eq!(map.time_at(at(6, 1)?)?, TimeAt::Exact(secs(44, 3)?));
/// // Inside the change the time is irrational: only an approximation is offered.
/// assert!(matches!(map.time_at(at(4, 1)?)?, TimeAt::Approximate(_)));
/// // Seconds → position is exact at constant tempo.
/// assert_eq!(map.position_at(secs(3, 1)?)?, at(3, 2)?);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TempoMap {
    /// The source data: sorted by position, at most one change per position, the first an
    /// immediate change at position 0.
    changes: Vec<(Position, TempoChange)>,
    /// The derived segments: sorted by `start` and by `start_ticks`, never empty; the first starts
    /// at position 0 and time 0 and is constant, the last is constant.
    segments: Vec<Segment>,
}

/// `⌊a / b + 1/2⌋` for `b > 0`: `a / b` rounded to the nearest integer, ties to the larger one.
///
/// Computed as quotient and remainder (Euclidean, so `0 ≤ r < b` also for negative `a`) instead of
/// `⌊(2a + b) / 2b⌋`, so that no intermediate is larger than the inputs.
///
/// # Errors
/// `None` only if `b ≤ 0` (the callers never pass one) or on overflow of the final `+ 1`.
fn nearest(a: i128, b: i128) -> Option<i128> {
    let q = a.checked_div_euclid(b)?;
    let r = a.checked_rem_euclid(b)?;
    // Round up when r / b ≥ 1/2, i.e. r ≥ b − r (no doubling, so no overflow).
    if r >= b.checked_sub(r)? {
        q.checked_add(1)
    } else {
        Some(q)
    }
}

/// `numerator / denominator` (`= a/b`, with `b > 0`) rounded to the nearest multiple of
/// `1/TIME_GRID`, as a number of ticks: `⌊a·G / b + 1/2⌋`, ties to the later tick (ADR-0021 §1, §4).
///
/// The integer part is split off first (`a = q·b + r`, `0 ≤ r < b`), so the product with the grid
/// is `r·G < b·G`: it fits in `i128` for any `b` below about 2.4·10²⁹. That covers every input whose
/// times have a denominator dividing the grid (sample instants, frame starts, boundaries), for
/// which `b` stays below about 10²⁶.
///
/// # The slow path: reducing first
/// A time with a denominator foreign to the grid (say `n/3001` s) makes `b` much larger, and
/// `r·G` can overflow. The fraction `r/b` is then reduced by its greatest common divisor
/// (`gcd(r, b) = gcd(a, b)`, so this is the reduced form of `a/b` too) and the product retried
/// once. The unreduced products of the gradual formula share large common factors, so this rescues
/// most such inputs; what is left is reported as an overflow (ADR-0021 §4). The reduction costs a
/// 128-bit gcd, which is why it is not done on the common path.
///
/// # Errors
/// [`TempoMapError::Arithmetic`] if an intermediate (even after the reduction) or the result does
/// not fit in `i64`.
fn grid_ticks(numerator: i128, denominator: i128) -> Result<i64, TempoMapError> {
    let whole = numerator.checked_div_euclid(denominator).ok_or(OVERFLOW)?;
    let rest = numerator.checked_rem_euclid(denominator).ok_or(OVERFLOW)?;
    let fraction = if let Some(scaled) = rest.checked_mul(GRID_WIDE) {
        nearest(scaled, denominator)
    } else {
        // `0 ≤ rest < denominator`, so both conversions to `u128` are exact, and the gcd is at
        // least 1 (it divides `denominator ≠ 0`), so the divisions below cannot fail.
        let common = gcd_u128(rest.unsigned_abs(), denominator.unsigned_abs());
        let common = i128::try_from(common).map_err(|_| OVERFLOW)?;
        let rest = rest.checked_div(common).ok_or(OVERFLOW)?;
        let denominator = denominator.checked_div(common).ok_or(OVERFLOW)?;
        nearest(rest.checked_mul(GRID_WIDE).ok_or(OVERFLOW)?, denominator)
    }
    .ok_or(OVERFLOW)?;
    let ticks = whole
        .checked_mul(GRID_WIDE)
        .and_then(|whole| whole.checked_add(fraction))
        .ok_or(OVERFLOW)?;
    i64::try_from(ticks).map_err(|_| OVERFLOW)
}

/// A rational rounded to the nearest tick of the grid. See [`grid_ticks`].
fn rational_ticks(value: Rational) -> Result<i64, TempoMapError> {
    grid_ticks(
        i128::from(value.numerator()),
        i128::from(value.denominator()),
    )
}

/// A number of ticks as an exact rational (seconds or whole notes): `ticks / TIME_GRID`.
fn ticks_rational(ticks: i64) -> Result<Rational, TempoMapError> {
    Ok(Rational::new(ticks, TIME_GRID)?)
}

/// The exact position reached `tau` seconds after the start of a gradual change, `s₀·τ + h·τ²`,
/// as an unreduced `i128` fraction `(numerator, denominator)` with a positive denominator.
///
/// # Why `i128` and not `Rational`?
/// The exact value usually does not fit in an `i64` fraction: its denominator grows like the
/// square of the denominator of `τ` (ADR-0021, Context). Writing `τ = n/d`, `s₀ = p/q`,
/// `h = u/v`, the value is
///
/// `x = (p·n·v·d + u·n²·q) / (q·v·d²)`
///
/// and for realistic inputs (d up to the grid, about 7·10⁸; q and v up to a few millions) every
/// term stays below about 10³⁰, well inside `i128` (1.7·10³⁸). Larger inputs are reported as an
/// overflow, never a panic.
///
/// # Complexity
/// O(1): a fixed number of `i128` multiplications.
#[allow(
    clippy::many_single_char_names,
    reason = "the names are those of the formula in the doc comment, which is easier to check"
)]
fn gradual_offset_exact(
    tau: Rational,
    s0: Rational,
    h: Rational,
) -> Result<(i128, i128), TempoMapError> {
    let (n, d) = (i128::from(tau.numerator()), i128::from(tau.denominator()));
    let (p, q) = (i128::from(s0.numerator()), i128::from(s0.denominator()));
    let (u, v) = (i128::from(h.numerator()), i128::from(h.denominator()));
    // The two numerator terms, then their sum, then the common denominator; each step checked.
    let linear = p
        .checked_mul(n)
        .and_then(|x| x.checked_mul(v))
        .and_then(|x| x.checked_mul(d))
        .ok_or(OVERFLOW)?;
    let quadratic = u
        .checked_mul(n)
        .and_then(|x| x.checked_mul(n))
        .and_then(|x| x.checked_mul(q))
        .ok_or(OVERFLOW)?;
    let numerator = linear.checked_add(quadratic).ok_or(OVERFLOW)?;
    let denominator = q
        .checked_mul(v)
        .and_then(|x| x.checked_mul(d))
        .and_then(|x| x.checked_mul(d))
        .ok_or(OVERFLOW)?;
    Ok((numerator, denominator))
}

/// The position reached `tau` seconds after the start of a gradual change, rounded to the nearest
/// tick of the position grid (ADR-0021 §4). See [`gradual_offset_exact`] and [`grid_ticks`].
fn gradual_offset_ticks(tau: Rational, s0: Rational, h: Rational) -> Result<i64, TempoMapError> {
    let (numerator, denominator) = gradual_offset_exact(tau, s0, h)?;
    grid_ticks(numerator, denominator)
}

/// Reduces `numerator / denominator` (`denominator > 0`) to lowest terms. O(log) for the gcd.
fn reduce(numerator: i128, denominator: i128) -> Option<(i128, i128)> {
    let common = gcd_u128(numerator.unsigned_abs(), denominator.unsigned_abs());
    // The gcd divides `denominator ≠ 0`, so it is at least 1 and at most `denominator`: the
    // conversion back to `i128` and the divisions cannot fail for valid inputs.
    let common = i128::try_from(common).ok()?;
    Some((
        numerator.checked_div(common)?,
        denominator.checked_div(common)?,
    ))
}

/// Compares the exact position `numerator / denominator` (unreduced, `denominator > 0`) with the
/// reduced rational `x`, by cross products: `num · x_den` against `x_num · den`.
///
/// The unreduced fraction from [`gradual_offset_exact`] can be large enough for the cross products
/// to overflow; in that case it is reduced by its gcd and the products are retried once (the slow
/// path, as in [`grid_ticks`]). `None` if they still overflow: the caller falls back to the `f64`
/// estimate (ADR-0021 §5).
///
/// # Complexity
/// O(1) on the fast path; one 128-bit gcd on the slow path.
fn compare_exact(numerator: i128, denominator: i128, x: Rational) -> Option<Ordering> {
    let (x_num, x_den) = (i128::from(x.numerator()), i128::from(x.denominator()));
    let cross = |numerator: i128, denominator: i128| {
        Some(
            numerator
                .checked_mul(x_den)?
                .cmp(&x_num.checked_mul(denominator)?),
        )
    };
    cross(numerator, denominator).or_else(|| {
        let (numerator, denominator) = reduce(numerator, denominator)?;
        cross(numerator, denominator)
    })
}

/// An evenly spaced series of indexed instants: samples or video frames (ADR-0020 §5).
///
/// Index `n` covers the times `[(n − offset)/rate, (n + 1 − offset)/rate)`, so the index of a time
/// `t` is `⌊t·rate + offset⌋`. Samples have `offset = 1/2` (the nearest sample, ties to the later
/// one), frames `offset = 0` (the frame that contains the time). One description for both keeps
/// the exact check of gradual changes in a single place.
#[derive(Debug, Clone, Copy)]
struct IndexGrid {
    /// Indices per second; > 0.
    rate: Rational,
    /// `1/2` for samples, `0` for frames.
    offset: Rational,
}

impl IndexGrid {
    /// The grid of the samples at `rate`.
    fn samples(rate: SampleRate) -> Result<Self, TempoMapError> {
        Ok(Self {
            rate: Rational::from_integer(i64::from(rate.hz()))?,
            offset: Rational::new(1, 2)?,
        })
    }

    /// The grid of the frames at `fps`.
    const fn frames(fps: FrameRate) -> Self {
        Self {
            rate: fps.fps(),
            offset: Rational::ZERO,
        }
    }

    /// The index of an exact time, `⌊t·rate + offset⌋`. O(log) for the rational arithmetic.
    fn index_of(self, t: Seconds) -> Result<i64, TempoMapError> {
        Ok(t.as_rational()
            .checked_mul(self.rate)?
            .checked_add(self.offset)?
            .floor())
    }

    /// The exact time where index `n` begins, `(n − offset)/rate`.
    fn start_of(self, n: i64) -> Result<Rational, TempoMapError> {
        Ok(Rational::from_integer(n)?
            .checked_sub(self.offset)?
            .checked_div(self.rate)?)
    }

    /// The index of an approximate time, `⌊t·rate + offset⌋` in `f64`, if it is finite and fits.
    #[allow(
        clippy::cast_possible_truncation,
        reason = "the value is an integer (floor) checked to lie well inside the i64 range"
    )]
    fn estimate(self, t: f64) -> Option<i64> {
        /// A bound safely inside the `i64` range (2⁶² ≈ 4.6·10¹⁸) for the `f64` → `i64` cast.
        const LIMIT: f64 = 4.0e18;
        let index = (t * self.rate.to_f64() + self.offset.to_f64()).floor();
        (index.is_finite() && index.abs() < LIMIT).then_some(index as i64)
    }
}

/// How many times the exact check of a gradual change may move the `f64` estimate before giving
/// up (ADR-0021 §5). The estimate is off by at most one index in practice; the bound keeps the
/// lookup's cost bounded on the audio thread (CLAUDE.md §5: no unbounded loops).
const MAX_CORRECTIONS: u32 = 4;

/// Validates the source data and derives the segments.
///
/// # Algorithm
/// 1. Check the first change (immediate, at position 0).
/// 2. Walk the changes in order, keeping the tempo in force. An immediate change starts a constant
///    segment. A gradual change is checked (it must end after it starts, and the next change must
///    not start before its end), starts a gradual segment from its explicit starting tempo or the
///    one in force, and, unless the next change starts exactly at its end, is followed by an
///    implicit constant segment at its target tempo.
/// 3. Give each segment its start time: the previous start plus the previous segment's exact
///    duration rounded to the grid (ADR-0021 §1).
///
/// # Complexity
/// O(n) in the number of changes; allocates the segment vector.
fn build_segments(changes: &[(Position, TempoChange)]) -> Result<Vec<Segment>, TempoMapError> {
    // Step 1: the initial tempo.
    let mut in_force = match changes.first() {
        Some((at, TempoChange::Immediate(tempo))) if *at == Position::ZERO => *tempo,
        Some((at, TempoChange::Gradual(_))) if *at == Position::ZERO => {
            return Err(TempoMapError::GradualAtStart);
        }
        _ => return Err(TempoMapError::NoTempoAtStart),
    };

    // Step 2: the shapes and their start positions. At most two segments per change.
    let mut shapes: Vec<(Position, Shape)> = Vec::with_capacity(changes.len().saturating_mul(2));
    for (i, &(at, change)) in changes.iter().enumerate() {
        let next = changes.get(i.saturating_add(1)).map(|&(next, _)| next);
        match change {
            TempoChange::Immediate(tempo) => {
                shapes.push((at, Shape::constant(tempo)?));
                in_force = tempo;
            }
            TempoChange::Gradual(gradual) => {
                if gradual.end <= at {
                    return Err(TempoMapError::NonPositiveGradual { at });
                }
                if let Some(next) = next
                    && next < gradual.end
                {
                    return Err(TempoMapError::ChangeInsideGradual {
                        at: next,
                        gradual: at,
                    });
                }
                let from = gradual.from.unwrap_or(in_force);
                let length = gradual.end.checked_duration_since(at)?;
                shapes.push((at, Shape::gradual(from, gradual.to, length)?));
                // The target tempo holds from the end on; when the next change starts exactly at
                // the end, it takes over at once and no constant segment is needed.
                if next != Some(gradual.end) {
                    shapes.push((gradual.end, Shape::constant(gradual.to)?));
                }
                in_force = gradual.to;
            }
        }
    }

    // Step 3: the start times, cumulated on the grid.
    let mut segments: Vec<Segment> = Vec::with_capacity(shapes.len());
    let mut ticks: i64 = 0;
    for (i, &(start, shape)) in shapes.iter().enumerate() {
        segments.push(Segment {
            start,
            start_ticks: ticks,
            start_time: Seconds::from_rational(ticks_rational(ticks)?),
            shape,
        });
        // The next segment starts after the exact duration of this one, rounded to the grid.
        // `t_k` is on the grid, so `round(t_k + D) = t_k + round(D)`: integers only.
        if let Some(&(next_start, _)) = shapes.get(i.saturating_add(1)) {
            let duration = match shape {
                Shape::Constant { speed, .. } => next_start
                    .checked_duration_since(start)?
                    .as_rational()
                    .checked_div(speed)?,
                Shape::Gradual { duration, .. } => duration,
            };
            ticks = ticks
                .checked_add(rational_ticks(duration)?)
                .ok_or(OVERFLOW)?;
        }
    }
    Ok(segments)
}

impl TempoMap {
    /// A map with one tempo for the whole score.
    ///
    /// # Errors
    /// Arithmetic errors only, for absurd tempo values.
    pub fn new(initial: Tempo) -> Result<Self, TempoMapError> {
        let changes = vec![(Position::ZERO, TempoChange::Immediate(initial))];
        let segments = build_segments(&changes)?;
        Ok(Self { changes, segments })
    }

    // --------------------------------------------------------------------------------------------
    // Editing (commands use these; each returns the previous value for undo)
    // --------------------------------------------------------------------------------------------

    /// Applies `edit` to a copy of the source data, rebuilds and validates it, and replaces the
    /// map only on success (all or nothing, see the module documentation).
    ///
    /// # Complexity
    /// O(n) and allocates: editing path only.
    fn edit<T>(
        &mut self,
        edit: impl FnOnce(&mut Vec<(Position, TempoChange)>) -> T,
    ) -> Result<T, TempoMapError> {
        let mut changes = self.changes.clone();
        let result = edit(&mut changes);
        let segments = build_segments(&changes)?;
        self.changes = changes;
        self.segments = segments;
        Ok(result)
    }

    /// Sets the tempo change at `at` (inserting or replacing it); returns the previous one.
    ///
    /// Replacing the change at position 0 with another immediate change sets the initial tempo.
    ///
    /// # Errors
    /// [`TempoMapError::BeforeStart`] for a negative position; [`TempoMapError::GradualAtStart`],
    /// [`TempoMapError::NonPositiveGradual`], [`TempoMapError::ChangeInsideGradual`] (in either
    /// direction: the new change inside an existing gradual one, or a new gradual change that
    /// would contain an existing change); arithmetic errors. The map is then unchanged.
    pub fn set_change(
        &mut self,
        at: Position,
        change: TempoChange,
    ) -> Result<Option<TempoChange>, TempoMapError> {
        if at < Position::ZERO {
            return Err(TempoMapError::BeforeStart);
        }
        self.edit(
            |changes| match changes.binary_search_by_key(&at, |&(position, _)| position) {
                Ok(i) => changes
                    .get_mut(i)
                    .map(|entry| std::mem::replace(&mut entry.1, change)),
                Err(i) => {
                    changes.insert(i, (at, change));
                    None
                }
            },
        )
    }

    /// Removes the tempo change at `at`; returns it, or `None` if there was none.
    ///
    /// # Errors
    /// [`TempoMapError::NoTempoAtStart`] for position 0; arithmetic errors. The map is then
    /// unchanged.
    pub fn remove_change(&mut self, at: Position) -> Result<Option<TempoChange>, TempoMapError> {
        if at == Position::ZERO {
            return Err(TempoMapError::NoTempoAtStart);
        }
        self.edit(|changes| {
            let i = changes
                .binary_search_by_key(&at, |&(position, _)| position)
                .ok()?;
            Some(changes.remove(i).1)
        })
    }

    /// The tempo changes, sorted by position; the first is an immediate change at position 0.
    #[must_use]
    pub fn changes(&self) -> &[(Position, TempoChange)] {
        &self.changes
    }

    // --------------------------------------------------------------------------------------------
    // Lookups: O(log n), no allocation
    // --------------------------------------------------------------------------------------------

    /// The segment at index `i` and the one after it (if any).
    ///
    /// # Errors
    /// [`TempoMapError::NoTempoAtStart`] if `i` is out of range, which the callers never cause
    /// (the vector is never empty and they pass at most its last index): a defensive fallback.
    fn segment_pair(&self, i: usize) -> Result<(&Segment, Option<&Segment>), TempoMapError> {
        let segment = self.segments.get(i).ok_or(TempoMapError::NoTempoAtStart)?;
        Ok((segment, self.segments.get(i.saturating_add(1))))
    }

    /// The time at which `position` sounds (ADR-0021 §3).
    ///
    /// - In a constant segment: exact, `t_k + x/s`, clamped to the start of the next segment.
    /// - In a gradual segment: exact at its start, otherwise an approximate `f64` from the exact
    ///   inverse formula, clamped likewise.
    /// - Before position 0: the first tempo extended backwards (negative times).
    ///
    /// # Errors
    /// [`TempoMapError::Arithmetic`] if the exact time does not fit.
    ///
    /// # Complexity
    /// O(log n): binary search on the segment starts, then O(1) arithmetic; no allocation.
    pub fn time_at(&self, position: Position) -> Result<TimeAt, TempoMapError> {
        // The last segment starting at or before `position`; for a negative position none does,
        // and the first segment, extended backwards, applies.
        let after = self.segments.partition_point(|s| s.start <= position);
        let (segment, next) = self.segment_pair(after.saturating_sub(1))?;
        let x = position.checked_duration_since(segment.start)?;
        match segment.shape {
            Shape::Constant { speed, .. } => {
                Ok(TimeAt::Exact(segment.constant_time(next, x, speed)?))
            }
            Shape::Gradual {
                start_speed,
                half_acceleration,
                ..
            } => {
                if x.is_zero() {
                    return Ok(TimeAt::Exact(segment.start_time));
                }
                Ok(TimeAt::Approximate(segment.gradual_time_f64(
                    next,
                    x,
                    start_speed,
                    half_acceleration,
                )))
            }
        }
    }

    /// The sample at which `position` sounds at `rate`: the nearest sample to its time, ties to
    /// the later one (ADR-0020 §5).
    ///
    /// Exact in constant segments and at segment starts (the rounding of [`SampleRate`] on the
    /// exact time). Inside a gradual change the time is irrational, but the index is still exact:
    /// an `f64` estimate is checked exactly against the positions where the sample's interval
    /// begins and ends, and moved at most a few times (ADR-0021 §5). The result is consistent
    /// with [`time_at`](Self::time_at), including its clamping at segment ends.
    ///
    /// # Errors
    /// [`TempoMapError::Arithmetic`] if the exact time (constant segments) or the estimate does
    /// not fit; an overflow of the exact check gives [`IndexAt::Estimated`] instead.
    ///
    /// # Complexity
    /// O(log n) for the segment, then O(1); no allocation.
    pub fn sample_at(
        &self,
        position: Position,
        rate: SampleRate,
    ) -> Result<IndexAt, TempoMapError> {
        self.index_at(position, IndexGrid::samples(rate)?)
    }

    /// The video frame during which `position` sounds at `fps`: the frame that contains its time
    /// (ADR-0020 §5). Exact everywhere in practice, as [`sample_at`](Self::sample_at).
    ///
    /// # Errors
    /// As [`sample_at`](Self::sample_at).
    ///
    /// # Complexity
    /// O(log n) for the segment, then O(1); no allocation.
    pub fn frame_at(&self, position: Position, fps: FrameRate) -> Result<IndexAt, TempoMapError> {
        self.index_at(position, IndexGrid::frames(fps))
    }

    /// The written position of sample `n` at `rate`: [`position_at`](Self::position_at) of its
    /// exact time `n/rate` (so rounded to the grid inside a gradual change, ADR-0021 §4).
    ///
    /// # Errors
    /// As [`position_at`](Self::position_at).
    pub fn position_of_sample(&self, n: i64, rate: SampleRate) -> Result<Position, TempoMapError> {
        self.position_at(rate.time_of_sample(n)?)
    }

    /// The written position where frame `k` begins at `fps`: [`position_at`](Self::position_at)
    /// of its exact start `k/fps`.
    ///
    /// # Errors
    /// As [`position_at`](Self::position_at).
    pub fn position_of_frame(&self, k: i64, fps: FrameRate) -> Result<Position, TempoMapError> {
        self.position_at(fps.frame_start(k)?)
    }

    /// The index of `position` on `grid` (samples or frames).
    ///
    /// # Algorithm (gradual segments)
    /// 1. Estimate the index from the `f64` time of [`Segment::gradual_time_f64`].
    /// 2. Check it exactly with [`Segment::locate_in_index`] and move it towards the answer, at
    ///    most [`MAX_CORRECTIONS`] times.
    /// 3. Clamp to the index of the next segment's start: the time used by `time_at` is clamped
    ///    there, and the index is monotonic in time, so `index(min(t, t_next)) =
    ///    min(index(t), index(t_next))`.
    ///
    /// If the check overflows or does not converge, the clamped estimate is returned as
    /// [`IndexAt::Estimated`].
    fn index_at(&self, position: Position, grid: IndexGrid) -> Result<IndexAt, TempoMapError> {
        let after = self.segments.partition_point(|s| s.start <= position);
        let (segment, next) = self.segment_pair(after.saturating_sub(1))?;
        let x = position.checked_duration_since(segment.start)?;
        let (length, start_speed, duration, half_acceleration) = match segment.shape {
            // Constant segment (all of them, also before position 0): the exact time, rounded.
            Shape::Constant { speed, .. } => {
                let time = segment.constant_time(next, x, speed)?;
                return Ok(IndexAt::Exact(grid.index_of(time)?));
            }
            Shape::Gradual {
                length,
                start_speed,
                duration,
                half_acceleration,
                ..
            } => (length, start_speed, duration, half_acceleration),
        };
        if x.is_zero() {
            return Ok(IndexAt::Exact(grid.index_of(segment.start_time)?));
        }
        // A gradual segment is never the last one; the error is a defensive fallback.
        let next = next.ok_or(TempoMapError::NoTempoAtStart)?;
        let cap = grid.index_of(next.start_time)?;
        let time = segment.gradual_time_f64(Some(next), x, start_speed, half_acceleration);
        let estimate = grid.estimate(time).ok_or(OVERFLOW)?;
        let values = (length, start_speed, duration, half_acceleration);
        let mut n = estimate;
        for _ in 0..=MAX_CORRECTIONS {
            match segment.locate_in_index(x, n, grid, values) {
                Some(Ordering::Equal) => return Ok(IndexAt::Exact(n.min(cap))),
                Some(Ordering::Less) => n = n.checked_sub(1).ok_or(OVERFLOW)?,
                Some(Ordering::Greater) => n = n.checked_add(1).ok_or(OVERFLOW)?,
                None => break,
            }
        }
        Ok(IndexAt::Estimated(estimate.min(cap)))
    }

    /// The written position that sounds at `time` (ADR-0021 §4).
    ///
    /// - In a constant segment: exact, `p_k + s·τ`, clamped to the start of the next segment.
    /// - In a gradual segment: `s₀·τ + h·τ²` computed exactly in `i128` and rounded to the nearest
    ///   tick of the position grid ([`TIME_GRID`] ticks per whole note), clamped to the end of the
    ///   change.
    /// - Before time 0: the first tempo extended backwards (negative positions).
    ///
    /// The result is non-decreasing in `time`.
    ///
    /// # Errors
    /// [`TempoMapError::Arithmetic`] if an intermediate or the result does not fit.
    ///
    /// # Complexity
    /// O(log n): binary search on the segment start times, then O(1) arithmetic; no allocation.
    pub fn position_at(&self, time: Seconds) -> Result<Position, TempoMapError> {
        let after = self.segments.partition_point(|s| s.start_time <= time);
        let (segment, next) = self.segment_pair(after.saturating_sub(1))?;
        let tau = time.checked_sub(segment.start_time)?.as_rational();
        match segment.shape {
            Shape::Constant { speed, .. } => {
                let position = segment
                    .start
                    .checked_add(Duration::from_rational(tau.checked_mul(speed)?))?;
                // Clamp to the next start (ADR-0021 §2): the rounded next start time may come up
                // to half a tick after the exact end of this segment.
                Ok(match next {
                    Some(next) if position > next.start => next.start,
                    _ => position,
                })
            }
            Shape::Gradual {
                length,
                start_speed,
                duration,
                half_acceleration,
                ..
            } => {
                let end = segment.start.checked_add(length)?;
                // Past the exact end, but before the rounded next start: the end of the change.
                if tau >= duration {
                    return Ok(end);
                }
                let ticks = gradual_offset_ticks(tau, start_speed, half_acceleration)?;
                let position = segment
                    .start
                    .checked_add(Duration::from_rational(ticks_rational(ticks)?))?;
                // Rounding cannot pass the end by more than half a tick, but it could reach it:
                // never beyond.
                Ok(position.min(end))
            }
        }
    }
}
