// SPDX-License-Identifier: GPL-3.0-or-later

//! The meter map: bars and beats ⇄ positions (ADR-0018 §3–§6, ADR-0019).
//!
//! # What it answers
//! - where bar *k* starts, and how long it is ([`MeterMap::bar_start`], [`MeterMap::bar_length`]);
//! - which bar contains a position, and where inside it ([`MeterMap::locate`]);
//! - which beat of its bar a point falls on, and back ([`MeterMap::beat_of`],
//!   [`MeterMap::beat_position`], [`MeterMap::beat_range`]); in a **free** bar the answer is
//!   explicitly "no beat";
//! - the bar number shown to the user ([`MeterMap::display_number`]).
//!
//! It describes the **written** score bar after bar; repeats and jumps are unrolled elsewhere
//! (`orchestra-playback`, ADR-0004). It knows nothing about notes.
//!
//! # Source data and derived segments
//! The map holds the **source data** that the document stores and commands edit (ADR-0003):
//! meter changes (with an optional beat grouping for their section, ADR-0019 §3), irregular bars
//! (pickups, incomplete bars, free cadenzas), per-bar beat groupings, and the numbering rules. All
//! lists are sorted by bar and have at most one entry per bar.
//!
//! From the source data it derives a sorted vector of **segments**: runs of consecutive bars that
//! share meter, length and beat counting. Each segment knows its first bar and its start position
//! (cumulated), so every lookup is a binary search followed by O(1) arithmetic:
//!
//! ```text
//! bars:      0      1    2    3    4    5   …      (bar 0 a 1/4 pickup, 3/4 from bar 3)
//! segments: [pickup][ 4/4 × 2 ][ 3/4 forever  …
//! starts:    0      1/4        9/4
//! ```
//!
//! The last segment continues **forever**: where the score ends is a model fact, not a meter one.
//!
//! # Editing: all or nothing
//! Every edit rebuilds the segments from a modified copy of the source data and validates the
//! whole result; only if it is valid does it replace the map. An edit that would leave any bar
//! inconsistent (a pickup longer than a new meter, a per-bar grouping whose total no longer
//! matches) fails and leaves the map unchanged, as ADR-0003 commands require. Each edit returns
//! the previous value, which is what an undo needs. Rebuilding is O(n log n) in the number `n` of
//! source entries — tens, rarely hundreds — and allocates: it runs on the editing path, never on
//! the audio thread (ADR-0018 §4).
//!
//! # Lookups: no allocation
//! Every lookup takes `&self`, allocates nothing and runs in O(log n): the audio thread will use
//! them for the metronome (F3). They return a [`MeterMapError`] instead of panicking, including on
//! overflow of the bar index (more than about 4·10⁹ bars) or of the rational arithmetic.

use std::error::Error;
use std::fmt;
use std::ops::Range;

use crate::meter::{BeatGrouping, Meter};
use crate::position::{Duration, Position};
use crate::rational::{Rational, RationalError};

/// The internal, contiguous index of a written bar: 0, 1, 2… (ADR-0018 §3).
///
/// Never shown to the user: the number printed in the score comes from
/// [`MeterMap::display_number`]. Bar 0 is the pickup bar when there is one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BarIndex(u32);

impl BarIndex {
    /// The first bar of the score.
    pub const FIRST: Self = Self(0);

    /// Wraps a 0-based bar index. O(1).
    #[must_use]
    pub const fn new(index: u32) -> Self {
        Self(index)
    }

    /// The 0-based bar index. O(1).
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// How the beats of an irregular bar are counted (ADR-0018 §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum BarKind {
    /// The bar is the *beginning* of a nominal bar: beat 0 at its start. If it is longer than its
    /// meter, the beat pattern starts again and the beat numbers continue (ADR-0019 §4).
    #[default]
    AlignStart,
    /// The bar is the *end* of a nominal bar, as a pickup: a quarter-note pickup in 4/4 is beat 3
    /// (the musician's "beat 4"). Its length may not exceed the nominal one.
    AlignEnd,
    /// The meter is **suspended** (free cadenza, *senza misura*): the bar has no beats at all, and
    /// the meter resumes at the next regular bar without a new meter change.
    Free,
}

/// A bar whose actual length differs from its meter's, or whose meter is suspended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct IrregularBar {
    /// The actual length of the bar; must be strictly positive. For a free bar it is explicit
    /// source data, not derived from the notes (owner's choice, ADR-0018 §3).
    pub length: Duration,
    /// How its beats are counted.
    pub kind: BarKind,
}

/// A meter change: the meter in force from its bar on, with an optional beat grouping for the
/// whole section (ADR-0019 §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MeterChange {
    /// The written meter.
    pub meter: Meter,
    /// How the bars of the section are counted; `None` means the default rule.
    pub grouping: Option<BeatGrouping>,
}

impl MeterChange {
    /// A meter change counted with the default rule. O(1).
    #[must_use]
    pub const fn new(meter: Meter) -> Self {
        Self {
            meter,
            grouping: None,
        }
    }

    /// A meter change counted with an explicit grouping. O(1).
    #[must_use]
    pub const fn with_grouping(meter: Meter, grouping: BeatGrouping) -> Self {
        Self {
            meter,
            grouping: Some(grouping),
        }
    }
}

/// A point inside a bar: the bar and the offset from its start, `0 ≤ offset < bar length`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BarPosition {
    /// The bar.
    pub bar: BarIndex,
    /// The distance from the start of the bar.
    pub offset: Duration,
}

/// A point expressed in beats: the 0-based beat number within its bar and the offset from the
/// start of that beat.
///
/// Beat numbers are counted from the virtual start of the nominal bar, so in a quarter-note pickup
/// in 4/4 the first point is beat 3; the "beat 1" a musician says is a presentation matter of the
/// interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BeatPosition {
    /// The 0-based beat number.
    pub beat: u32,
    /// The distance from the start of the beat; `0 ≤ offset < beat length`.
    pub offset: Duration,
}

/// Why a meter map edit or lookup failed (ADR-0018 §6).
///
/// The `Display` texts are English and meant for developers; the interface maps each variant to
/// a Fluent key (CLAUDE.md §7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MeterMapError {
    /// The exact arithmetic overflowed (ADR-0015): a position or a length too large to represent.
    Arithmetic(RationalError),
    /// A position before the start of the score (before bar 0).
    BeforeStart,
    /// A bar or beat index beyond the representable range (about 4·10⁹).
    IndexOutOfRange,
    /// The meter change at bar 0 cannot be removed: some meter must always be in force.
    NoMeterAtStart,
    /// An irregular bar with a length that is zero or negative.
    NonPositiveLength {
        /// The offending bar.
        bar: BarIndex,
    },
    /// An `AlignEnd` (pickup) bar longer than the nominal length of its meter.
    PickupLongerThanMeter {
        /// The offending bar.
        bar: BarIndex,
    },
    /// A beat grouping whose total differs from the nominal length of the meter in force.
    GroupingLengthMismatch {
        /// The bar of the offending grouping (the meter change's bar for a section grouping).
        bar: BarIndex,
    },
    /// A per-bar beat grouping on a free bar, which has no beats.
    GroupingOnFreeBar {
        /// The offending bar.
        bar: BarIndex,
    },
    /// An offset outside its bar (negative, or not shorter than the bar).
    OffsetOutsideBar,
    /// A beat was requested in a free bar, which has none.
    NoBeats,
    /// A beat (with its offset) that does not fall inside the given bar.
    BeatOutsideBar,
}

impl From<RationalError> for MeterMapError {
    /// Wraps an arithmetic error, so that `?` works on rational operations inside the map.
    fn from(error: RationalError) -> Self {
        Self::Arithmetic(error)
    }
}

impl fmt::Display for MeterMapError {
    /// Writes a short English description of the error, for developers.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Arithmetic(error) => write!(f, "meter map arithmetic failed: {error}"),
            Self::BeforeStart => f.write_str("position before the start of the score"),
            Self::IndexOutOfRange => f.write_str("bar or beat index out of range"),
            Self::NoMeterAtStart => f.write_str("the meter change at bar 0 cannot be removed"),
            Self::NonPositiveLength { bar } => {
                write!(f, "irregular bar {} must have a positive length", bar.0)
            }
            Self::PickupLongerThanMeter { bar } => {
                write!(f, "pickup bar {} is longer than its meter", bar.0)
            }
            Self::GroupingLengthMismatch { bar } => write!(
                f,
                "the beat grouping at bar {} does not add up to the length of its meter",
                bar.0
            ),
            Self::GroupingOnFreeBar { bar } => {
                write!(f, "bar {} is free and cannot have a beat grouping", bar.0)
            }
            Self::OffsetOutsideBar => f.write_str("offset outside its bar"),
            Self::NoBeats => f.write_str("a free bar has no beats"),
            Self::BeatOutsideBar => f.write_str("beat outside its bar"),
        }
    }
}

impl Error for MeterMapError {
    /// The arithmetic error behind an `Arithmetic` failure, if any.
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Arithmetic(error) => Some(error),
            _ => None,
        }
    }
}

/// How the beats of a bar are laid out, as derived from the meter or from a grouping.
///
/// Kept apart from [`BeatGrouping`] because the default counting of simple and compound meters is
/// **uniform** (a count and one beat length): that needs no array, so a 17/16 with 17 beats does
/// not hit the 16-group limit (ADR-0019 §6), and its lookups are a single division.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum BeatPattern {
    /// `count` beats of equal `beat` length (simple and compound meters).
    Uniform {
        /// Beats in one pass of the pattern; ≥ 1.
        count: u32,
        /// Length of each beat; > 0.
        beat: Duration,
    },
    /// One beat per group (additive meters, explicit groupings).
    Grouped(BeatGrouping),
}

impl BeatPattern {
    /// The default counting of a meter (ADR-0018 §2, unchanged by ADR-0019 §6).
    ///
    /// - additive (two or more written groups): one beat per group;
    /// - single numerator `n` multiple of 3 and greater than 3: compound, `n/3` beats of three
    ///   units (6/8 → two dotted quarters);
    /// - any other single numerator: simple, `n` beats of one unit (3/4, 5/8, 7/8, 4/3).
    fn default_for(meter: &Meter) -> Result<Self, MeterMapError> {
        if meter.is_additive() {
            return Ok(Self::Grouped(BeatGrouping::from_meter(meter)));
        }
        let n = meter.numerator();
        let unit = i64::from(meter.denominator());
        let compound = n > 3 && n.checked_rem(3) == Some(0);
        let (count, units_per_beat) = if compound {
            (n.checked_div(3).unwrap_or(n), 3)
        } else {
            (n, 1)
        };
        Ok(Self::Uniform {
            count: u32::from(count),
            beat: Duration::from_rational(Rational::new(units_per_beat, unit)?),
        })
    }

    /// The beat that contains `offset` (measured from the virtual start of the nominal bar,
    /// `offset ≥ 0`), and the offset inside that beat.
    ///
    /// Offsets beyond one pass of the pattern (a bar longer than its meter) start the pattern
    /// again with the beat numbers continuing (ADR-0019 §4).
    ///
    /// # Complexity
    /// O(1) for uniform patterns; O(g) for grouped ones, g ≤ 16.
    fn locate(&self, offset: Duration) -> Result<(u32, Duration), MeterMapError> {
        match self {
            Self::Uniform { beat, .. } => {
                let (index, rest) = whole_units(offset, *beat)?;
                Ok((index, rest))
            }
            Self::Grouped(grouping) => {
                let cycle = grouping.total()?;
                let (pass, mut rest) = whole_units(offset, cycle)?;
                let first = pass
                    .checked_mul(u32::from(grouping.beat_count()))
                    .ok_or(MeterMapError::IndexOutOfRange)?;
                // Walk the at most 16 groups, subtracting each beat until the rest falls inside
                // one. `rest < cycle`, so the walk always stops at some group; the last group
                // takes whatever is left, so even a mistake in that reasoning cannot loop or
                // index out of range.
                let count = grouping.groups().len();
                for (j, &group) in grouping.groups().iter().enumerate() {
                    let length = grouping.group_length(group)?;
                    if rest < length || j.saturating_add(1) == count {
                        let j = u32::try_from(j).map_err(|_| MeterMapError::IndexOutOfRange)?;
                        let beat = first.checked_add(j).ok_or(MeterMapError::IndexOutOfRange)?;
                        return Ok((beat, rest));
                    }
                    rest = rest.checked_sub(length)?;
                }
                // Unreachable: a grouping has at least one group, so the loop returns.
                Err(MeterMapError::IndexOutOfRange)
            }
        }
    }

    /// The start of beat `index`, measured from the virtual start of the nominal bar.
    ///
    /// # Complexity
    /// O(1) for uniform patterns; O(g) for grouped ones, g ≤ 16.
    fn beat_start(&self, index: u32) -> Result<Duration, MeterMapError> {
        match self {
            Self::Uniform { beat, .. } => {
                Ok(beat.checked_mul(Rational::from_integer(i64::from(index))?)?)
            }
            Self::Grouped(grouping) => {
                let per_pass = u32::from(grouping.beat_count());
                let pass = index
                    .checked_div(per_pass)
                    .ok_or(MeterMapError::IndexOutOfRange)?;
                let within = index
                    .checked_rem(per_pass)
                    .ok_or(MeterMapError::IndexOutOfRange)?;
                let mut start = grouping
                    .total()?
                    .checked_mul(Rational::from_integer(i64::from(pass))?)?;
                // Add the lengths of the groups before `within` in this pass.
                // `within < per_pass ≤ 16`, so it always fits in `usize`.
                let within = usize::try_from(within).map_err(|_| MeterMapError::IndexOutOfRange)?;
                for &group in grouping.groups().iter().take(within) {
                    start = start.checked_add(grouping.group_length(group)?)?;
                }
                Ok(start)
            }
        }
    }
}

/// Splits `offset` into whole `unit`s and a rest: `offset = k · unit + rest`, `0 ≤ rest < unit`.
///
/// This is the "remainder" operation of ADR-0018 §4, made of `checked_ratio` + `floor` +
/// `checked_mul` instead of a dedicated `rem` on `Rational`.
///
/// # Preconditions
/// `offset ≥ 0` and `unit > 0` (the callers guarantee both).
///
/// # Errors
/// [`MeterMapError::IndexOutOfRange`] if `k` does not fit in `u32`; arithmetic overflow.
fn whole_units(offset: Duration, unit: Duration) -> Result<(u32, Duration), MeterMapError> {
    let k = offset.checked_ratio(unit)?.floor();
    let k32 = u32::try_from(k).map_err(|_| MeterMapError::IndexOutOfRange)?;
    let rest = offset.checked_sub(unit.checked_mul(Rational::from_integer(k)?)?)?;
    Ok((k32, rest))
}

/// A run of consecutive bars with the same meter, length and beat counting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct Segment {
    /// The first bar of the run.
    first_bar: u32,
    /// The position where `first_bar` starts (cumulated over all earlier segments).
    start: Position,
    /// The actual length of each bar of the run; > 0.
    bar_length: Duration,
    /// The written meter in force (also during a free bar, where it is suspended).
    meter: Meter,
    /// The beat counting; `None` for a free bar.
    beats: Option<BeatPattern>,
    /// How far the bar's start lies after the virtual start of its nominal bar: `nominal − actual`
    /// for an `AlignEnd` pickup, zero otherwise. Beat lookups add it to the offset in the bar.
    beat_shift: Duration,
}

/// The source data of the map: what the document stores and the commands edit.
///
/// Every list is sorted by bar with at most one entry per bar; `meter_changes` always has an
/// entry for bar 0.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Sources {
    /// Meter changes, each with an optional section grouping.
    meter_changes: Vec<(BarIndex, MeterChange)>,
    /// Irregular bars (pickups, incomplete bars, free bars).
    irregular_bars: Vec<(BarIndex, IrregularBar)>,
    /// Per-bar beat groupings, which win over the section's.
    bar_groupings: Vec<(BarIndex, BeatGrouping)>,
    /// Bars that are not counted in the displayed numbering.
    uncounted: Vec<BarIndex>,
    /// Bars where the displayed numbering restarts, with their number.
    restarts: Vec<(BarIndex, u32)>,
}

/// The meter map of a project (ADR-0005 §2, ADR-0018, ADR-0019).
///
/// See the module documentation for how it is organised and what it costs.
///
/// # Examples
/// ```
/// use orchestra_core_time::{
///     BarIndex, BarKind, Duration, IrregularBar, Meter, MeterChange, MeterMap, Position, Rational,
/// };
///
/// let q = |n, d| Rational::new(n, d);
/// // 4/4 with a quarter-note pickup.
/// let mut map = MeterMap::new(MeterChange::new(Meter::new(&[4], 4)?))?;
/// map.set_irregular_bar(
///     BarIndex::FIRST,
///     IrregularBar { length: Duration::from_rational(q(1, 4)?), kind: BarKind::AlignEnd },
/// )?;
/// // Bar 1 (the first full bar, numbered 1) starts after the pickup.
/// assert_eq!(map.bar_start(BarIndex::new(1))?, Position::from_rational(q(1, 4)?));
/// assert_eq!(map.display_number(BarIndex::FIRST)?, Some(0));
/// assert_eq!(map.display_number(BarIndex::new(1))?, Some(1));
/// // The pickup is the musician's "beat 4": 0-based beat 3.
/// let at = map.locate(Position::ZERO)?;
/// assert_eq!(map.beat_of(at)?.map(|b| b.beat), Some(3));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MeterMap {
    /// The source data.
    sources: Sources,
    /// The derived segments: sorted by `first_bar` (and therefore by `start`), never empty, the
    /// first one starting at bar 0 and position 0.
    segments: Vec<Segment>,
}

/// Inserts or replaces the entry for `key` in a list sorted by key, returning the old value.
///
/// O(log n) to find the place, O(n) to shift the tail on insertion.
fn upsert<V>(list: &mut Vec<(BarIndex, V)>, key: BarIndex, value: V) -> Option<V> {
    match list.binary_search_by_key(&key, |(k, _)| *k) {
        Ok(i) => list
            .get_mut(i)
            .map(|entry| std::mem::replace(&mut entry.1, value)),
        Err(i) => {
            list.insert(i, (key, value));
            None
        }
    }
}

/// Removes the entry for `key` from a list sorted by key, returning its value. O(n).
fn remove<V>(list: &mut Vec<(BarIndex, V)>, key: BarIndex) -> Option<V> {
    let i = list.binary_search_by_key(&key, |(k, _)| *k).ok()?;
    Some(list.remove(i).1)
}

/// The value of the entry for `key` in a list sorted by key. O(log n).
fn find<V: Copy>(list: &[(BarIndex, V)], key: BarIndex) -> Option<V> {
    let i = list.binary_search_by_key(&key, |(k, _)| *k).ok()?;
    list.get(i).map(|(_, v)| *v)
}

/// The meter change in force at `bar`: the last one at or before it. O(log n).
///
/// `None` only if the list has no entry at bar 0, which the map never allows.
fn change_at(changes: &[(BarIndex, MeterChange)], bar: BarIndex) -> Option<MeterChange> {
    let after = changes.partition_point(|(k, _)| *k <= bar);
    changes
        .get(after.checked_sub(1)?)
        .map(|(_, change)| *change)
}

/// Checks that a grouping adds up to the nominal length of `meter`.
fn check_grouping(
    grouping: &BeatGrouping,
    meter: &Meter,
    bar: BarIndex,
) -> Result<(), MeterMapError> {
    if grouping.total()? == meter.nominal_length()? {
        Ok(())
    } else {
        Err(MeterMapError::GroupingLengthMismatch { bar })
    }
}

/// Validates the source data and derives the segments.
///
/// # Algorithm
/// 1. Validate every entry on its own and against the meter in force at its bar (groupings add
///    up, irregular lengths are positive, pickups not longer than their meter, no grouping on a
///    free bar).
/// 2. Collect the **breakpoints**: every bar with a meter change, an irregular bar or a per-bar
///    grouping, plus the bar after each irregular or grouped bar (so that those bars are runs of
///    one). Sort and deduplicate.
/// 3. Turn each breakpoint into a segment that lasts until the next breakpoint, cumulating the
///    start positions.
///
/// # Complexity
/// O(n log n) in the number of source entries (the sort, plus a binary search per breakpoint).
fn build_segments(sources: &Sources) -> Result<Vec<Segment>, MeterMapError> {
    let changes = &sources.meter_changes;
    // Step 1: validation.
    if changes.first().map(|(bar, _)| *bar) != Some(BarIndex::FIRST) {
        return Err(MeterMapError::NoMeterAtStart);
    }
    for (bar, change) in changes {
        if let Some(grouping) = &change.grouping {
            check_grouping(grouping, &change.meter, *bar)?;
        }
    }
    for (bar, grouping) in &sources.bar_groupings {
        let change = change_at(changes, *bar).ok_or(MeterMapError::NoMeterAtStart)?;
        check_grouping(grouping, &change.meter, *bar)?;
    }
    for (bar, irregular) in &sources.irregular_bars {
        if !irregular.length.is_positive() {
            return Err(MeterMapError::NonPositiveLength { bar: *bar });
        }
        let change = change_at(changes, *bar).ok_or(MeterMapError::NoMeterAtStart)?;
        match irregular.kind {
            BarKind::AlignEnd if irregular.length > change.meter.nominal_length()? => {
                return Err(MeterMapError::PickupLongerThanMeter { bar: *bar });
            }
            BarKind::Free if find(&sources.bar_groupings, *bar).is_some() => {
                return Err(MeterMapError::GroupingOnFreeBar { bar: *bar });
            }
            _ => {}
        }
    }

    // Step 2: breakpoints. `checked_add` skips the bar after `u32::MAX`, which does not exist.
    let mut breaks: Vec<u32> = changes.iter().map(|(bar, _)| bar.0).collect();
    for bar in sources
        .irregular_bars
        .iter()
        .map(|(bar, _)| *bar)
        .chain(sources.bar_groupings.iter().map(|(bar, _)| *bar))
    {
        breaks.push(bar.0);
        if let Some(next) = bar.0.checked_add(1) {
            breaks.push(next);
        }
    }
    breaks.sort_unstable();
    breaks.dedup();

    // Step 3: segments, with cumulated starts.
    let mut segments = Vec::with_capacity(breaks.len());
    let mut start = Position::ZERO;
    for (i, &first) in breaks.iter().enumerate() {
        let bar = BarIndex(first);
        let change = change_at(changes, bar).ok_or(MeterMapError::NoMeterAtStart)?;
        let nominal = change.meter.nominal_length()?;
        // The counting: per-bar grouping, else the section's, else the default rule.
        let pattern = match find(&sources.bar_groupings, bar).or(change.grouping) {
            Some(grouping) => BeatPattern::Grouped(grouping),
            None => BeatPattern::default_for(&change.meter)?,
        };
        let (bar_length, beats, beat_shift) = match find(&sources.irregular_bars, bar) {
            None => (nominal, Some(pattern), Duration::ZERO),
            Some(IrregularBar { length, kind }) => match kind {
                BarKind::AlignStart => (length, Some(pattern), Duration::ZERO),
                BarKind::AlignEnd => (length, Some(pattern), nominal.checked_sub(length)?),
                BarKind::Free => (length, None, Duration::ZERO),
            },
        };
        segments.push(Segment {
            first_bar: first,
            start,
            bar_length,
            meter: change.meter,
            beats,
            beat_shift,
        });
        // The next segment starts after `next − first` bars of this one. The last segment has no
        // next breakpoint: it continues forever, and `start` is not needed any more.
        if let Some(&next) = breaks.get(i.saturating_add(1)) {
            let bars = next
                .checked_sub(first)
                .ok_or(MeterMapError::IndexOutOfRange)?;
            let run = bar_length.checked_mul(Rational::from_integer(i64::from(bars))?)?;
            start = start.checked_add(run)?;
        }
    }
    Ok(segments)
}

impl MeterMap {
    /// A map with one meter for the whole score, from bar 0, and no irregular bars.
    ///
    /// # Errors
    /// [`MeterMapError::GroupingLengthMismatch`] if the change carries a grouping that does not add
    /// up to the meter's length; arithmetic errors (not in practice).
    pub fn new(initial: MeterChange) -> Result<Self, MeterMapError> {
        let sources = Sources {
            meter_changes: vec![(BarIndex::FIRST, initial)],
            irregular_bars: Vec::new(),
            bar_groupings: Vec::new(),
            uncounted: Vec::new(),
            restarts: Vec::new(),
        };
        let segments = build_segments(&sources)?;
        Ok(Self { sources, segments })
    }

    /// Applies `change` to a copy of the source data, rebuilds and validates it, and replaces the
    /// map only on success (all or nothing, see the module documentation).
    ///
    /// # Complexity
    /// O(n log n) and allocates: editing path only.
    fn edit<T>(&mut self, change: impl FnOnce(&mut Sources) -> T) -> Result<T, MeterMapError> {
        let mut sources = self.sources.clone();
        let result = change(&mut sources);
        let segments = build_segments(&sources)?;
        self.sources = sources;
        self.segments = segments;
        Ok(result)
    }

    // --------------------------------------------------------------------------------------------
    // Editing (commands use these; each returns the previous value for undo)
    // --------------------------------------------------------------------------------------------

    /// Sets the meter change at `bar` (inserting or replacing it); returns the previous one.
    ///
    /// # Errors
    /// Any validation error of the resulting map (e.g. an existing pickup now longer than its
    /// meter); the map is then unchanged.
    pub fn set_meter_change(
        &mut self,
        bar: BarIndex,
        change: MeterChange,
    ) -> Result<Option<MeterChange>, MeterMapError> {
        self.edit(|s| upsert(&mut s.meter_changes, bar, change))
    }

    /// Removes the meter change at `bar`; returns it, or `None` if there was none.
    ///
    /// # Errors
    /// [`MeterMapError::NoMeterAtStart`] for bar 0; any validation error of the resulting map.
    pub fn remove_meter_change(
        &mut self,
        bar: BarIndex,
    ) -> Result<Option<MeterChange>, MeterMapError> {
        if bar == BarIndex::FIRST {
            return Err(MeterMapError::NoMeterAtStart);
        }
        self.edit(|s| remove(&mut s.meter_changes, bar))
    }

    /// Makes `bar` irregular (inserting or replacing); returns the previous setting.
    ///
    /// # Errors
    /// A non-positive length, a pickup longer than its meter, a free bar with a per-bar grouping;
    /// the map is then unchanged.
    pub fn set_irregular_bar(
        &mut self,
        bar: BarIndex,
        irregular: IrregularBar,
    ) -> Result<Option<IrregularBar>, MeterMapError> {
        self.edit(|s| upsert(&mut s.irregular_bars, bar, irregular))
    }

    /// Makes `bar` regular again; returns its previous irregular setting, if any.
    ///
    /// # Errors
    /// Arithmetic errors only (not in practice).
    pub fn remove_irregular_bar(
        &mut self,
        bar: BarIndex,
    ) -> Result<Option<IrregularBar>, MeterMapError> {
        self.edit(|s| remove(&mut s.irregular_bars, bar))
    }

    /// Gives `bar` its own beat grouping (ADR-0019 §3); returns the previous one.
    ///
    /// # Errors
    /// [`MeterMapError::GroupingLengthMismatch`], [`MeterMapError::GroupingOnFreeBar`]; the map is
    /// then unchanged.
    pub fn set_bar_grouping(
        &mut self,
        bar: BarIndex,
        grouping: BeatGrouping,
    ) -> Result<Option<BeatGrouping>, MeterMapError> {
        self.edit(|s| upsert(&mut s.bar_groupings, bar, grouping))
    }

    /// Removes the per-bar grouping of `bar` (the section's counting applies again); returns it.
    ///
    /// # Errors
    /// Arithmetic errors only (not in practice).
    pub fn remove_bar_grouping(
        &mut self,
        bar: BarIndex,
    ) -> Result<Option<BeatGrouping>, MeterMapError> {
        self.edit(|s| remove(&mut s.bar_groupings, bar))
    }

    /// Marks `bar` as counted or not in the displayed numbering; returns the previous state
    /// (`true` = it was uncounted). O(n), no rebuild: numbering does not affect positions.
    pub fn set_uncounted(&mut self, bar: BarIndex, uncounted: bool) -> bool {
        let list = &mut self.sources.uncounted;
        match (list.binary_search(&bar), uncounted) {
            (Ok(_), true) => true,
            (Ok(i), false) => {
                list.remove(i);
                true
            }
            (Err(i), true) => {
                list.insert(i, bar);
                false
            }
            (Err(_), false) => false,
        }
    }

    /// Restarts the displayed numbering at `bar` with `number` (`None` removes the restart);
    /// returns the previous restart number of that bar. O(n), no rebuild.
    pub fn set_numbering_restart(&mut self, bar: BarIndex, number: Option<u32>) -> Option<u32> {
        match number {
            Some(number) => upsert(&mut self.sources.restarts, bar, number),
            None => remove(&mut self.sources.restarts, bar),
        }
    }

    // --------------------------------------------------------------------------------------------
    // Source data, read only (for the file format and the interface)
    // --------------------------------------------------------------------------------------------

    /// The meter changes, sorted by bar; the first is at bar 0.
    #[must_use]
    pub fn meter_changes(&self) -> &[(BarIndex, MeterChange)] {
        &self.sources.meter_changes
    }

    /// The irregular bars, sorted by bar.
    #[must_use]
    pub fn irregular_bars(&self) -> &[(BarIndex, IrregularBar)] {
        &self.sources.irregular_bars
    }

    /// The per-bar beat groupings, sorted by bar.
    #[must_use]
    pub fn bar_groupings(&self) -> &[(BarIndex, BeatGrouping)] {
        &self.sources.bar_groupings
    }

    /// The uncounted bars, sorted.
    #[must_use]
    pub fn uncounted_bars(&self) -> &[BarIndex] {
        &self.sources.uncounted
    }

    /// The numbering restarts, sorted by bar.
    #[must_use]
    pub fn numbering_restarts(&self) -> &[(BarIndex, u32)] {
        &self.sources.restarts
    }

    // --------------------------------------------------------------------------------------------
    // Lookups: O(log n), no allocation
    // --------------------------------------------------------------------------------------------

    /// The segment containing `bar`: the last one whose first bar is ≤ `bar`. O(log n).
    fn segment_for_bar(&self, bar: BarIndex) -> Result<&Segment, MeterMapError> {
        let after = self.segments.partition_point(|s| s.first_bar <= bar.0);
        // The first segment starts at bar 0, so `after ≥ 1`; the error is a defensive fallback.
        after
            .checked_sub(1)
            .and_then(|i| self.segments.get(i))
            .ok_or(MeterMapError::NoMeterAtStart)
    }

    /// The meter written at `bar` (also for a free bar, where it is suspended). O(log n).
    ///
    /// # Errors
    /// None in practice (the map always has a meter at bar 0).
    pub fn meter_at(&self, bar: BarIndex) -> Result<Meter, MeterMapError> {
        Ok(self.segment_for_bar(bar)?.meter)
    }

    /// The actual length of `bar`. O(log n).
    ///
    /// # Errors
    /// None in practice.
    pub fn bar_length(&self, bar: BarIndex) -> Result<Duration, MeterMapError> {
        Ok(self.segment_for_bar(bar)?.bar_length)
    }

    /// `true` if `bar` is free (its meter is suspended). O(log n).
    ///
    /// # Errors
    /// None in practice.
    pub fn is_free(&self, bar: BarIndex) -> Result<bool, MeterMapError> {
        Ok(self.segment_for_bar(bar)?.beats.is_none())
    }

    /// The position where `bar` starts: `segment start + (bar − first bar) · bar length`.
    ///
    /// # Errors
    /// [`MeterMapError::Arithmetic`] if the position is too large to represent.
    ///
    /// # Complexity
    /// O(log n).
    pub fn bar_start(&self, bar: BarIndex) -> Result<Position, MeterMapError> {
        let segment = self.segment_for_bar(bar)?;
        let bars = bar
            .0
            .checked_sub(segment.first_bar)
            .ok_or(MeterMapError::IndexOutOfRange)?;
        let into = segment
            .bar_length
            .checked_mul(Rational::from_integer(i64::from(bars))?)?;
        Ok(segment.start.checked_add(into)?)
    }

    /// The bar containing `position`, and the offset from that bar's start.
    ///
    /// # Errors
    /// [`MeterMapError::BeforeStart`] for a negative position; [`MeterMapError::IndexOutOfRange`]
    /// beyond about 4·10⁹ bars; arithmetic overflow.
    ///
    /// # Complexity
    /// O(log n): binary search on the segment starts, then one division.
    pub fn locate(&self, position: Position) -> Result<BarPosition, MeterMapError> {
        if position < Position::ZERO {
            return Err(MeterMapError::BeforeStart);
        }
        let after = self.segments.partition_point(|s| s.start <= position);
        let segment = after
            .checked_sub(1)
            .and_then(|i| self.segments.get(i))
            .ok_or(MeterMapError::BeforeStart)?;
        let into = position.checked_duration_since(segment.start)?;
        let (bars, offset) = whole_units(into, segment.bar_length)?;
        let bar = segment
            .first_bar
            .checked_add(bars)
            .ok_or(MeterMapError::IndexOutOfRange)?;
        Ok(BarPosition {
            bar: BarIndex(bar),
            offset,
        })
    }

    /// The position of a point given as bar and offset (the inverse of [`locate`](Self::locate)).
    ///
    /// # Errors
    /// [`MeterMapError::OffsetOutsideBar`] unless `0 ≤ offset < bar length`; arithmetic overflow.
    ///
    /// # Complexity
    /// O(log n).
    pub fn position_of(&self, at: BarPosition) -> Result<Position, MeterMapError> {
        let length = self.bar_length(at.bar)?;
        if at.offset.is_negative() || at.offset >= length {
            return Err(MeterMapError::OffsetOutsideBar);
        }
        Ok(self.bar_start(at.bar)?.checked_add(at.offset)?)
    }

    /// The beat of a point inside a bar; `None` in a free bar (ADR-0018 §3).
    ///
    /// Beat numbers are 0-based and counted from the virtual start of the nominal bar, so a
    /// pickup starts on a late beat; in a bar longer than its meter they continue past the last
    /// beat of the pattern (ADR-0019 §4).
    ///
    /// # Errors
    /// [`MeterMapError::OffsetOutsideBar`] unless `0 ≤ offset < bar length`; arithmetic overflow.
    ///
    /// # Complexity
    /// O(log n) for the segment, then O(1) (at most 16 groups).
    pub fn beat_of(&self, at: BarPosition) -> Result<Option<BeatPosition>, MeterMapError> {
        let segment = self.segment_for_bar(at.bar)?;
        if at.offset.is_negative() || at.offset >= segment.bar_length {
            return Err(MeterMapError::OffsetOutsideBar);
        }
        let Some(pattern) = &segment.beats else {
            return Ok(None);
        };
        let (beat, offset) = pattern.locate(at.offset.checked_add(segment.beat_shift)?)?;
        Ok(Some(BeatPosition { beat, offset }))
    }

    /// The position of a point given as bar, beat and offset in the beat (the inverse of
    /// [`beat_of`](Self::beat_of)).
    ///
    /// # Errors
    /// [`MeterMapError::NoBeats`] in a free bar; [`MeterMapError::BeatOutsideBar`] if the point
    /// does not fall inside the bar (e.g. beat 0 of a pickup, or a negative offset); arithmetic
    /// overflow.
    ///
    /// # Complexity
    /// O(log n) for the segment, then O(1) (at most 16 groups).
    pub fn beat_position(
        &self,
        bar: BarIndex,
        beat: BeatPosition,
    ) -> Result<Position, MeterMapError> {
        let segment = self.segment_for_bar(bar)?;
        let pattern = segment.beats.as_ref().ok_or(MeterMapError::NoBeats)?;
        if beat.offset.is_negative() {
            return Err(MeterMapError::BeatOutsideBar);
        }
        let from_virtual_start = pattern.beat_start(beat.beat)?.checked_add(beat.offset)?;
        let offset = from_virtual_start.checked_sub(segment.beat_shift)?;
        if offset.is_negative() || offset >= segment.bar_length {
            return Err(MeterMapError::BeatOutsideBar);
        }
        Ok(self.bar_start(bar)?.checked_add(offset)?)
    }

    /// The beats that **start** inside `bar`, as a range of 0-based beat numbers; `None` for a
    /// free bar. For the metronome (F3) and the counters of the interface.
    ///
    /// A pickup of a quarter in 4/4 gives `3..4`; a 5/4 bar in 4/4 (aligned at the start) gives
    /// `0..5`; a pickup of an eighth in 4/4 gives an empty range, because the only beat it touches
    /// started before it.
    ///
    /// # Errors
    /// Arithmetic overflow only.
    ///
    /// # Complexity
    /// O(log n), then O(1).
    pub fn beat_range(&self, bar: BarIndex) -> Result<Option<Range<u32>>, MeterMapError> {
        let segment = self.segment_for_bar(bar)?;
        let Some(pattern) = &segment.beats else {
            return Ok(None);
        };
        // The beat containing the bar's start: if the start is exactly on it, it counts;
        // otherwise the first beat starting inside the bar is the next one.
        let (first, offset) = pattern.locate(segment.beat_shift)?;
        let first = if offset.is_zero() {
            first
        } else {
            first.checked_add(1).ok_or(MeterMapError::IndexOutOfRange)?
        };
        // Likewise at the end: the beat containing the bar's end point is excluded if the end
        // falls exactly on its start, included otherwise (it starts inside the bar).
        let end_point = segment.beat_shift.checked_add(segment.bar_length)?;
        let (last, offset) = pattern.locate(end_point)?;
        let end = if offset.is_zero() {
            last
        } else {
            last.checked_add(1).ok_or(MeterMapError::IndexOutOfRange)?
        };
        Ok(Some(first..end.max(first)))
    }

    /// The number shown for `bar`, or `None` if it is uncounted (ADR-0018 §5).
    ///
    /// By default bar 0 is numbered 0 if it is an `AlignEnd` pickup, otherwise 1; every bar adds
    /// one; uncounted bars (free cadenzas in editions that do not number them) have no number and
    /// do not advance the count; a restart sets the number of its bar (or of the first counted bar
    /// from there, if the restart bar itself is uncounted). Free bars are counted by default
    /// (owner's choice).
    ///
    /// # Errors
    /// [`MeterMapError::IndexOutOfRange`] if the number would exceed `u32::MAX`.
    ///
    /// # Complexity
    /// O(log n): one binary search for the restart, two for the uncounted bars.
    pub fn display_number(&self, bar: BarIndex) -> Result<Option<u32>, MeterMapError> {
        let uncounted = &self.sources.uncounted;
        if uncounted.binary_search(&bar).is_ok() {
            return Ok(None);
        }
        // The last restart at or before `bar`, or the default start at bar 0.
        let restarts = &self.sources.restarts;
        let after = restarts.partition_point(|(k, _)| *k <= bar);
        let (from, number) =
            if let Some(&(k, n)) = after.checked_sub(1).and_then(|i| restarts.get(i)) {
                (k, n)
            } else {
                // No restart before `bar`: bar 0 is numbered 0 if it is a pickup, else 1.
                let pickup = matches!(
                    find(&self.sources.irregular_bars, BarIndex::FIRST),
                    Some(IrregularBar {
                        kind: BarKind::AlignEnd,
                        ..
                    })
                );
                (BarIndex::FIRST, u32::from(!pickup))
            };
        // Uncounted bars in `from..bar` do not advance the count.
        let skipped = uncounted
            .partition_point(|k| *k < bar)
            .saturating_sub(uncounted.partition_point(|k| *k < from));
        let skipped = u32::try_from(skipped).map_err(|_| MeterMapError::IndexOutOfRange)?;
        let advanced = bar
            .0
            .checked_sub(from.0)
            .and_then(|d| d.checked_sub(skipped))
            .ok_or(MeterMapError::IndexOutOfRange)?;
        number
            .checked_add(advanced)
            .map(Some)
            .ok_or(MeterMapError::IndexOutOfRange)
    }
}
