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
//! ## Responsibilities (planned for Phase 1)
//! - A rational type for positions/durations that never silently rounds (triplets of triplets must
//!   line up exactly, even after hours of music).
//! - The tempo map: tempo changes (including gradual ones), with O(log n) lookup of the segment
//!   containing a position through a cumulative table.
//! - The meter map (time signatures), used to turn positions into bars/beats for display.
//! - Conversions rational time ⇄ seconds ⇄ sample index (at a given sample rate) ⇄ video frame
//!   (with a rational frame rate, e.g. 30000/1001 for NTSC).
//!
//! ## What this crate must NOT do
//! - Know about notes, tracks, staves or any other model concept (that is `orchestra-core-model`).
//! - Store floating-point positions anywhere: floats only appear as *outputs* of conversions.
//! - Allocate in the conversion functions used by the audio thread: those must be callable from a
//!   real-time context (CLAUDE.md §5).
//!
//! ## Allowed dependencies
//! None inside the workspace. External crates only if they are small, GPL-compatible and justified
//! in an ADR (e.g. a big-integer crate if 64-bit numerators ever prove insufficient).

#![forbid(unsafe_code)]
