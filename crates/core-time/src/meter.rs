// SPDX-License-Identifier: GPL-3.0-or-later

//! Written meters (time signatures) and beat groupings (ADR-0018 §2, ADR-0019).
//!
//! # Two different questions about a bar
//! - **How is it written?** That is the [`Meter`]: the time signature printed in the score, such
//!   as `4/4`, `6/8`, `9/8` or the additive `3+2+2/8`.
//! - **How is it counted?** That is the beat grouping: how many beats the bar has and how long
//!   each one is. By default it is derived from the meter (see below); a [`BeatGrouping`] can
//!   override it without changing what is printed: `9/8` counted `3+2+2+2` (ADR-0019 §1), or a
//!   `3/4` counted as two dotted quarters `[3, 3]/8` (hemiola, ADR-0019 §2).
//!
//! The default counting rule (ADR-0018 §2, unchanged by ADR-0019 §6) is applied by the meter map,
//! not here, because it needs the arithmetic of positions: additive meters have one beat per
//! group; a single numerator that is a multiple of 3 greater than 3 is compound (beats of three
//! units); any other single numerator is simple (beats of one unit), so `7/8` counts seven
//! eighths unless the user chooses a grouping.
//!
//! # Fixed-size storage
//! Both types keep their groups in a fixed array plus a length, so they are `Copy` and never
//! allocate: a `Vec` would allocate for every meter in the map and gain nothing, since no real
//! meter has more than 8 written groups (owner's choice) and no real counting more than 16
//! (ADR-0019 §5). The unused slots of the array are always **zero**: that invariant is what makes
//! the derived `PartialEq`, `Eq` and `Hash` correct (two equal meters have identical arrays).
//!
//! # Denominators
//! Any denominator from 1 to 65535 is accepted, not only powers of two (decision D04): `4/3` and
//! `3/10` are valid meters. How to engrave them is F2B's problem, how to export them to MIDI
//! files (whose time signature message only encodes powers of two) is F1.18's.

use std::error::Error;
use std::fmt;

use crate::position::Duration;
use crate::rational::{Rational, RationalError};

/// The maximum number of groups of a written [`Meter`] (owner's choice, ADR-0018 §2).
pub const MAX_METER_GROUPS: usize = 8;

/// The maximum number of groups of a [`BeatGrouping`] (owner's choice, ADR-0019 §5): enough to
/// count a 12/8 "in twelve".
pub const MAX_GROUPING_GROUPS: usize = 16;

/// Why a [`Meter`] or a [`BeatGrouping`] could not be built.
///
/// The `Display` texts are English and meant for developers; the interface maps each variant to
/// a Fluent key (CLAUDE.md §7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MeterError {
    /// The list of groups was empty: a meter needs at least one group.
    NoGroups,
    /// More groups than allowed: [`MAX_METER_GROUPS`] for a meter, [`MAX_GROUPING_GROUPS`] for a
    /// grouping.
    TooManyGroups,
    /// One of the groups was zero: every group lasts at least one unit.
    ZeroGroup,
    /// The denominator (or the unit of a grouping) was zero.
    ZeroDenominator,
}

impl fmt::Display for MeterError {
    /// Writes a short English description of the error, for developers.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NoGroups => "a meter or a beat grouping needs at least one group",
            Self::TooManyGroups => "too many groups in a meter or a beat grouping",
            Self::ZeroGroup => "a group of a meter or a beat grouping cannot be zero",
            Self::ZeroDenominator => "the denominator of a meter or a beat grouping cannot be zero",
        })
    }
}

impl Error for MeterError {}

/// A written meter (time signature): 1 to [`MAX_METER_GROUPS`] numerator groups over one
/// denominator (ADR-0018 §2).
///
/// `4/4` is `[4]/4`, `6/8` is `[6]/8`, the additive `3+2+2/8` is `[3, 2, 2]/8`. The nominal bar
/// length is `(sum of groups) / denominator` whole notes.
///
/// # Examples
/// ```
/// use orchestra_core_time::{Duration, Meter, Rational};
///
/// let additive = Meter::new(&[3, 2, 2], 8)?;
/// assert_eq!(additive.numerator(), 7);
/// assert!(additive.is_additive());
/// assert_eq!(
///     additive.nominal_length()?,
///     Duration::from_rational(Rational::new(7, 8)?)
/// );
/// // Any denominator is valid (D04): a bar of 4/3 lasts four triplet half notes.
/// assert_eq!(
///     Meter::new(&[4], 3)?.nominal_length()?,
///     Duration::from_rational(Rational::new(4, 3)?)
/// );
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Meter {
    /// The numerator groups; only the first `len` are meaningful, the others are zero (see the
    /// module documentation for why that matters).
    groups: [u8; MAX_METER_GROUPS],
    /// How many groups are in use: `1..=MAX_METER_GROUPS`.
    len: u8,
    /// The denominator: `1..=u16::MAX`, any value (D04).
    denominator: u16,
}

/// A beat grouping: how a bar is **counted**, independently of how its meter is written
/// (ADR-0019 §1).
///
/// 1 to [`MAX_GROUPING_GROUPS`] beats, each of `group / unit` whole notes. The unit is its own,
/// not necessarily the meter's denominator, so that `[3, 3]/8` can count a 3/4 bar in two dotted
/// quarters (ADR-0019 §2). The meter map checks that the total equals the nominal length of the
/// bar's meter.
///
/// # Examples
/// ```
/// use orchestra_core_time::{BeatGrouping, Duration, Rational};
///
/// // 9/8 counted 3+2+2+2: four beats, the first one longer.
/// let grouping = BeatGrouping::new(&[3, 2, 2, 2], 8)?;
/// assert_eq!(grouping.groups(), &[3, 2, 2, 2]);
/// assert_eq!(grouping.total()?, Duration::from_rational(Rational::new(9, 8)?));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct BeatGrouping {
    /// The beat lengths in units; only the first `len` are meaningful, the others are zero.
    groups: [u8; MAX_GROUPING_GROUPS],
    /// How many beats are in use: `1..=MAX_GROUPING_GROUPS`.
    len: u8,
    /// The unit of the groups, as a fraction `1/unit` of a whole note: `1..=u16::MAX`.
    unit: u16,
}

/// Checks a list of groups and copies it into a zero-padded array of `N` slots.
///
/// Shared by [`Meter::new`] and [`BeatGrouping::new`], which differ only in their capacity.
///
/// # Errors
/// [`MeterError::ZeroDenominator`] if `denominator` is zero, [`MeterError::NoGroups`] if the list
/// is empty, [`MeterError::TooManyGroups`] if it has more than `N` entries,
/// [`MeterError::ZeroGroup`] if an entry is zero. The checks run in this order, so the error
/// reported for an input with several problems is predictable.
///
/// # Complexity
/// O(N), with N ≤ 16.
fn checked_groups<const N: usize>(
    groups: &[u8],
    denominator: u16,
) -> Result<([u8; N], u8), MeterError> {
    if denominator == 0 {
        return Err(MeterError::ZeroDenominator);
    }
    if groups.is_empty() {
        return Err(MeterError::NoGroups);
    }
    if groups.len() > N {
        return Err(MeterError::TooManyGroups);
    }
    if groups.contains(&0) {
        return Err(MeterError::ZeroGroup);
    }
    // Copy into the zero-initialised array: `zip` stops at the shorter of the two, which is the
    // input list (checked above to be at most N long), so the remaining slots stay zero.
    let mut array = [0_u8; N];
    for (slot, &group) in array.iter_mut().zip(groups) {
        *slot = group;
    }
    // At most N ≤ 16 entries, so the length always fits in a `u8`; the error is unreachable but
    // mapped rather than unwrapped (no panics in library code).
    let len = u8::try_from(groups.len()).map_err(|_| MeterError::TooManyGroups)?;
    Ok((array, len))
}

/// The groups actually in use: the first `len` slots of the array.
///
/// `get` instead of slicing because the crate denies `clippy::indexing_slicing`; `len` never
/// exceeds the array's size (constructor invariant), so the fallback empty slice is never used.
fn used(groups: &[u8], len: u8) -> &[u8] {
    groups.get(..usize::from(len)).unwrap_or(&[])
}

/// The sum of the groups in use.
///
/// It cannot saturate: at most 16 groups of at most 255 give 4080, far below `u16::MAX`;
/// `saturating_add` only keeps the arithmetic lint satisfied without a panic path.
fn sum(groups: &[u8]) -> u16 {
    groups
        .iter()
        .fold(0_u16, |acc, &g| acc.saturating_add(u16::from(g)))
}

/// `count / unit` whole notes, as a [`Duration`].
///
/// # Errors
/// [`RationalError::DivisionByZero`] if `unit` is zero (never the case for values coming from a
/// validated meter or grouping).
fn units(count: u16, unit: u16) -> Result<Duration, RationalError> {
    Ok(Duration::from_rational(Rational::new(
        i64::from(count),
        i64::from(unit),
    )?))
}

/// Writes `3+2+2/8`: the groups joined by `+`, then the denominator.
fn write_groups(f: &mut fmt::Formatter<'_>, groups: &[u8], denominator: u16) -> fmt::Result {
    for (i, group) in groups.iter().enumerate() {
        if i > 0 {
            f.write_str("+")?;
        }
        write!(f, "{group}")?;
    }
    write!(f, "/{denominator}")
}

impl Meter {
    /// Builds a meter from its numerator groups and its denominator.
    ///
    /// `new(&[4], 4)` is 4/4, `new(&[3, 2, 2], 8)` the additive 3+2+2/8.
    ///
    /// # Errors
    /// See [`MeterError`]: zero denominator, no groups, more than [`MAX_METER_GROUPS`] groups, a
    /// zero group.
    ///
    /// # Complexity
    /// O(1) (at most 8 groups).
    pub fn new(groups: &[u8], denominator: u16) -> Result<Self, MeterError> {
        let (groups, len) = checked_groups::<MAX_METER_GROUPS>(groups, denominator)?;
        Ok(Self {
            groups,
            len,
            denominator,
        })
    }

    /// The numerator groups as written: `[4]` for 4/4, `[3, 2, 2]` for 3+2+2/8. O(1).
    #[must_use]
    pub fn groups(&self) -> &[u8] {
        used(&self.groups, self.len)
    }

    /// The denominator. O(1).
    #[must_use]
    pub const fn denominator(&self) -> u16 {
        self.denominator
    }

    /// The total numerator: the sum of the groups (7 for 3+2+2/8). At most 2040. O(1).
    #[must_use]
    pub fn numerator(&self) -> u16 {
        sum(self.groups())
    }

    /// `true` if the meter is written with two or more groups (3+2+2/8). O(1).
    #[must_use]
    pub const fn is_additive(&self) -> bool {
        self.len > 1
    }

    /// The nominal bar length: `numerator / denominator` whole notes.
    ///
    /// # Errors
    /// None in practice (the denominator is never zero and the values are small); the
    /// `Result` comes from the checked rational constructor.
    ///
    /// # Complexity
    /// O(log) for the reduction.
    pub fn nominal_length(&self) -> Result<Duration, RationalError> {
        units(self.numerator(), self.denominator)
    }
}

impl BeatGrouping {
    /// Builds a grouping from its beat lengths and its unit.
    ///
    /// `new(&[3, 2, 2, 2], 8)` counts a bar of 9/8 as 3+2+2+2 eighths; `new(&[3, 3], 8)` counts a
    /// bar of 3/4 as two dotted quarters.
    ///
    /// # Errors
    /// See [`MeterError`]: zero unit, no groups, more than [`MAX_GROUPING_GROUPS`] groups, a zero
    /// group.
    ///
    /// # Complexity
    /// O(1) (at most 16 groups).
    pub fn new(groups: &[u8], unit: u16) -> Result<Self, MeterError> {
        let (groups, len) = checked_groups::<MAX_GROUPING_GROUPS>(groups, unit)?;
        Ok(Self { groups, len, unit })
    }

    /// The groups of an additive meter, as a grouping with the meter's denominator as unit.
    ///
    /// Used by the meter map for the default counting of additive meters (one beat per written
    /// group). Infallible: a meter has at most 8 groups, which fit in the 16 slots, and its
    /// invariants (non-zero groups and denominator) are those of a grouping.
    pub(crate) fn from_meter(meter: &Meter) -> Self {
        let mut groups = [0_u8; MAX_GROUPING_GROUPS];
        for (slot, &group) in groups.iter_mut().zip(meter.groups()) {
            *slot = group;
        }
        Self {
            groups,
            len: meter.len,
            unit: meter.denominator,
        }
    }

    /// The beat lengths in units. O(1).
    #[must_use]
    pub fn groups(&self) -> &[u8] {
        used(&self.groups, self.len)
    }

    /// The unit: each group counts `1/unit` whole notes. O(1).
    #[must_use]
    pub const fn unit(&self) -> u16 {
        self.unit
    }

    /// The number of beats in one pass of the grouping. O(1).
    #[must_use]
    pub const fn beat_count(&self) -> u8 {
        self.len
    }

    /// The total length: `(sum of groups) / unit` whole notes.
    ///
    /// # Errors
    /// None in practice (see [`Meter::nominal_length`]).
    ///
    /// # Complexity
    /// O(log) for the reduction.
    pub fn total(&self) -> Result<Duration, RationalError> {
        units(sum(self.groups()), self.unit)
    }

    /// The length of one group of `group` units: `group / unit` whole notes.
    ///
    /// # Errors
    /// None in practice (see [`Meter::nominal_length`]).
    pub(crate) fn group_length(&self, group: u8) -> Result<Duration, RationalError> {
        units(u16::from(group), self.unit)
    }
}

impl fmt::Debug for Meter {
    /// Writes `Meter(3+2+2/8)`, without the zero padding of the array.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Meter(")?;
        write_groups(f, self.groups(), self.denominator)?;
        f.write_str(")")
    }
}

impl fmt::Debug for BeatGrouping {
    /// Writes `BeatGrouping(3+2+2+2/8)`, without the zero padding of the array.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("BeatGrouping(")?;
        write_groups(f, self.groups(), self.unit)?;
        f.write_str(")")
    }
}
