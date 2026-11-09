// SPDX-License-Identifier: GPL-3.0-or-later

//! # orchestra-core-time — exact musical time
//!
//! The lowest crate of the workspace: everything that has to agree on "when" depends on it.
//!
//! ## Purpose
//! Represent musical positions and durations as **exact rational numbers** (fractions of a whole
//! note) and own the **single tempo map** of a project. Every other notion of time — seconds,
//! audio sample indices, video frame numbers — is *derived* from rational time through this crate,
//! and only at the very last step (ADR-0005).
//!
//! ## Responsibilities (Phase 1)
//! - [`Rational`], the rational type for positions/durations that never silently rounds (triplets
//!   of triplets must line up exactly, even after hours of music). Implemented in step F1.03
//!   according to ADR-0015: see the `rational` module.
//! - [`Position`] and [`Duration`], the domain types over `Rational` (ADR-0018 §1): a position
//!   minus a position is a duration, and adding two positions does not compile. See the
//!   `position` module.
//! - The tempo map (ADR-0020, ADR-0021): immediate and gradual tempo changes ([`TempoChange`]),
//!   and [`TempoMap`], which turns written positions into seconds and back with O(log n)
//!   lookups, and into exact sample and frame indices; segment start times lie on a fixed grid
//!   ([`TIME_GRID`]). See the `tempo_map` module.
//! - The meter map (ADR-0018, ADR-0019): written meters ([`Meter`]), beat groupings
//!   ([`BeatGrouping`]), irregular and free bars, and [`MeterMap`], which turns positions into
//!   bars and beats and back with O(log n) lookups. See the `meter` and `meter_map` modules.
//! - Conversions rational time ⇄ seconds ⇄ sample index (at a given sample rate) ⇄ video frame
//!   (with a rational frame rate, e.g. 30000/1001 for NTSC).
//!
//! ## What this crate must NOT do
//! - Know about notes, tracks, staves or any other model concept (that is `orchestra-core-model`).
//! - Store floating-point positions anywhere: floats only appear as *outputs* of conversions.
//! - Allocate in the conversion functions used by the audio thread: those must be callable from a
//!   real-time context (CLAUDE.md §5).
//!
//! The units of real time are [`Tempo`], [`Seconds`], [`SampleRate`] and [`FrameRate`], with the
//! rounding rules of ADR-0020 §5 (see the `tempo` and `clock` modules); the tempo map adds the
//! exact sample and frame indices inside gradual changes ([`IndexAt`]).
//!
//! ## Allowed dependencies
//! None inside the workspace. External crates only if they are small, GPL-compatible and justified
//! in an ADR (e.g. a big-integer crate if 64-bit numerators ever prove insufficient).

#![forbid(unsafe_code)]
// Stricter than the workspace (owner's decision, 2026-10-30, SCALETTA F0.16). Every crate trusts
// the time values computed here, so no operation may panic or wrap silently:
// - `v[i]` panics when `i` is out of range: use `get` and handle `None`, or iterate;
// - `+ - *` on integers can overflow: use `checked_*` and return the typed overflow error of
//   ADR-0015 §6 (`RationalError::Overflow`); this lint makes sure no operation escapes it.
// Tests are exempt from the first one through clippy.toml.
#![deny(clippy::indexing_slicing, clippy::arithmetic_side_effects)]

/// Unwraps a `Result` inside a `const fn`, returning the error early.
///
/// The `?` operator relies on the `Try` trait, which cannot be called in a `const fn` on stable
/// Rust; this macro is the hand-written equivalent of `?`. The error is returned as it is (no
/// `From` conversion, unlike `?`), so the enclosing function must return the same error type.
///
/// # Why here?
/// `macro_rules!` macros are visible, in textual order, to everything declared after them in the
/// same crate: defining it once at the root, before the `mod` lines, makes it available to every
/// module (`rational`, `position`, `clock` and the others) without a copy in each.
macro_rules! const_try {
    ($result:expr) => {
        match $result {
            Ok(value) => value,
            Err(error) => return Err(error),
        }
    };
}

/// A rational constant written as a literal, checked **at compile time**: `rational!(1, 2)` is
/// one half, `rational!(60)` the integer sixty.
///
/// # Why a macro
/// `Rational::new` returns a `Result` (the denominator may be zero, the fraction may overflow),
/// so a constant needs an answer for the error case. Library code may not panic at run time
/// (CLAUDE.md §4), and a silent fallback such as `Err(_) => Rational::ZERO` would turn a typo in a
/// literal (say `1, 0`) into a wrong constant nobody notices. The expansion is an inline `const`
/// block, which the compiler always evaluates while compiling, even where the macro is used in
/// ordinary code: an invalid literal makes the `panic!` below a **compile error**, and no panic
/// can ever happen at run time.
///
/// Exported (`#[macro_export]`) because other crates write rational constants too (the MIDI
/// export's grid, for example); it names the type by its full path, `$crate::Rational`, so it
/// works without any import.
///
/// # Examples
/// ```
/// use orchestra_core_time::{Rational, rational};
/// const HALF: Rational = rational!(1, 2);
/// assert_eq!((HALF.numerator(), HALF.denominator()), (1, 2));
/// assert_eq!(rational!(60).numerator(), 60);
/// ```
///
/// A zero denominator does not compile:
/// ```compile_fail
/// const BAD: orchestra_core_time::Rational = orchestra_core_time::rational!(1, 0);
/// ```
///
/// # Complexity
/// None at run time: the value is computed by the compiler.
#[macro_export]
macro_rules! rational {
    ($numerator:expr, $denominator:expr) => {
        const {
            match $crate::Rational::new($numerator, $denominator) {
                Ok(value) => value,
                Err(_) => {
                    #[allow(
                        clippy::panic,
                        reason = "evaluated by the compiler: an invalid literal is a compile \
                                  error, never a run-time panic"
                    )]
                    {
                        panic!("an invalid rational literal")
                    }
                }
            }
        }
    };
    ($integer:expr) => {
        const {
            match $crate::Rational::from_integer($integer) {
                Ok(value) => value,
                Err(_) => {
                    #[allow(
                        clippy::panic,
                        reason = "evaluated by the compiler: an invalid literal is a compile \
                                  error, never a run-time panic"
                    )]
                    {
                        panic!("an invalid rational literal")
                    }
                }
            }
        }
    };
}

mod clock;
// The one public module (ADR-0034 D1): `invariant::convert_or` and `invariant::some_or` read as
// what they are only with their namespace; at the root, `convert_or` would say nothing about the
// rule it enforces. Every other module is private and its API is re-exported below.
pub mod invariant;
mod meter;
mod meter_map;
mod position;
mod rational;
mod tempo;
mod tempo_map;

pub use clock::{ClockError, FrameRate, SampleRate, Seconds};
pub use meter::{BeatGrouping, MAX_GROUPING_GROUPS, MAX_METER_GROUPS, Meter, MeterError};
pub use meter_map::{
    BarCounting, BarIndex, BarKind, BarPosition, BarSpan, BeatPosition, IrregularBar, MeterChange,
    MeterMap, MeterMapError,
};
pub use position::{Duration, Position};
pub use rational::{ParseRationalError, Rational, RationalError};
pub use tempo::{Tempo, TempoError};
pub use tempo_map::{
    GradualChange, IndexAt, TIME_GRID, TempoChange, TempoMap, TempoMapError, TimeAt,
};
