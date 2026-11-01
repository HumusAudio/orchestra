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
//!   according to ADR-0015: see the [`rational`] module.
//! - [`Position`] and [`Duration`], the domain types over `Rational` (ADR-0018 §1): a position
//!   minus a position is a duration, and adding two positions does not compile. See the
//!   [`position`] module.
//! - The tempo map (ADR-0020, ADR-0021): immediate and gradual tempo changes ([`TempoChange`]),
//!   and [`TempoMap`], which turns written positions into seconds and back with O(log n)
//!   lookups; segment start times lie on a fixed grid ([`TIME_GRID`]). See the [`tempo_map`]
//!   module.
//! - The meter map (ADR-0018, ADR-0019): written meters ([`Meter`]), beat groupings
//!   ([`BeatGrouping`]), irregular and free bars, and [`MeterMap`], which turns positions into
//!   bars and beats and back with O(log n) lookups. See the [`meter`] and [`meter_map`] modules.
//! - Conversions rational time ⇄ seconds ⇄ sample index (at a given sample rate) ⇄ video frame
//!   (with a rational frame rate, e.g. 30000/1001 for NTSC).
//!
//! ## What this crate must NOT do
//! - Know about notes, tracks, staves or any other model concept (that is `orchestra-core-model`).
//! - Store floating-point positions anywhere: floats only appear as *outputs* of conversions.
//! - Allocate in the conversion functions used by the audio thread: those must be callable from a
//!   real-time context (CLAUDE.md §5).
//!
//! ## Still to come in Phase 1
//! Exact sample and frame indices inside gradual tempo changes (second part of F1.05b, ADR-0020
//! §5, ADR-0021 §5). The units are already here: [`Tempo`], [`Seconds`], [`SampleRate`] and
//! [`FrameRate`] with the rounding rules of ADR-0020 §5, see the [`tempo`] and [`clock`] modules.
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

pub mod clock;
pub mod meter;
pub mod meter_map;
pub mod position;
pub mod rational;
pub mod tempo;
pub mod tempo_map;

pub use clock::{ClockError, FrameRate, SampleRate, Seconds};
pub use meter::{BeatGrouping, MAX_GROUPING_GROUPS, MAX_METER_GROUPS, Meter, MeterError};
pub use meter_map::{
    BarIndex, BarKind, BarPosition, BeatPosition, IrregularBar, MeterChange, MeterMap,
    MeterMapError,
};
pub use position::{Duration, Position};
pub use rational::{Rational, RationalError};
pub use tempo::{Tempo, TempoError};
pub use tempo_map::{GradualChange, TIME_GRID, TempoChange, TempoMap, TempoMapError, TimeAt};
