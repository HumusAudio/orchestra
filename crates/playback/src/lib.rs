// SPDX-License-Identifier: GPL-3.0-or-later

//! # orchestra-playback — from written intent to performance
//!
//! ## Purpose
//! Compute **what is actually played** from what is written (ADR-0004). The result is the
//! *unrolled* timeline shown by the DAW view: repeats and jumps expanded, ties merged into single
//! sounding notes, grace notes and ornaments given concrete timing, dynamics turned into velocities
//! and controller curves, transposing instruments sounding at concert pitch.
//!
//! ## Responsibilities (planned)
//! - Unroll the form of the piece into a linear sequence of passes, each keeping a link back to
//!   the written position it came from (so a click in the DAW view selects the right written note).
//! - Apply articulation/dynamic interpretation rules, then the explicit performance offsets stored
//!   in the model.
//! - Implement the **tolerance threshold** logic: decide whether a drag in the DAW view is a
//!   performance offset (within tolerance) or a change of written position (beyond it), and emit
//!   the corresponding model command.
//! - Do all of this incrementally: a change to one note re-derives only the affected passes.
//!
//! ## What this crate must NOT do
//! - Produce audio or talk to devices/plugins (that is `orchestra-audio-engine`).
//! - Store its output as a second editable copy of the music: the performance is *derived* data,
//!   a cache that can always be thrown away and recomputed from the model.
//!
//! ## Allowed dependencies
//! `orchestra-core-time`, `orchestra-core-model`.

#![forbid(unsafe_code)]
