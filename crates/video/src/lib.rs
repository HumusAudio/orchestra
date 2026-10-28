// SPDX-License-Identifier: GPL-3.0-or-later

//! # orchestra-video — video tracks (comes last)
//!
//! This crate is **intentionally empty**: video is the last phase of the roadmap (Phase 5). It
//! exists now only so that the workspace structure, the dependency rules and the place of video in
//! the architecture are fixed from the start.
//!
//! ## Purpose (when it arrives)
//! Video is a third kind of track on the **same timeline** as notation and audio (ADR-0003):
//! clips positioned in rational musical time or in seconds, decoded through low-resolution
//! **proxy files** for smooth editing on modest hardware (ADR-0007), with frame numbers derived
//! from a rational frame rate (ADR-0005).
//!
//! ## What this crate must NOT do
//! - Keep its own clock: all timing goes through `orchestra-core-time`.
//!
//! ## Allowed dependencies
//! `orchestra-core-time` (and, later, the decoding libraries chosen in a dedicated ADR).

#![forbid(unsafe_code)]
