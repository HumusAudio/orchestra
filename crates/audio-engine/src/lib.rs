// SPDX-License-Identifier: GPL-3.0-or-later

//! # orchestra-audio-engine — real-time audio
//!
//! ## Purpose
//! Run the **audio graph** in real time: read the performance events derived by
//! `orchestra-playback`, drive internal instruments and hosted plugins, mix the tracks and deliver
//! the result to the audio device (or to an offline/null backend in tests and exports).
//!
//! ## Responsibilities (planned for Phase 3)
//! - The audio callback, under the strict rules of CLAUDE.md §5: no allocations, no locks, no I/O,
//!   no unbounded waits; communication with the rest of the program only through fixed-capacity
//!   lock-free structures. A test fails if the audio thread allocates.
//! - Device backends behind a trait, in `platform/` modules (ALSA/PipeWire/JACK on Linux, WASAPI on
//!   Windows, CoreAudio on macOS), plus a **null/offline backend** so that no test ever needs a
//!   real device.
//! - Budget: callback ≤ 50 % of the buffer period with the reference project (ADR-0007).
//!
//! ## What this crate must NOT do
//! - Read the model directly from the audio thread: it receives pre-computed, immutable event
//!   buffers.
//! - Load plugins in its own process (that is `orchestra-plugin-host`, out of process).
//!
//! ## Allowed dependencies
//! `orchestra-core-time`, `orchestra-playback`, `orchestra-plugin-host`.
//!
//! ## Unsafe code
//! This is one of the three crates where `unsafe` is allowed (FFI with audio APIs, lock-free
//! primitives). Every `unsafe` block carries a `// SAFETY:` comment proving its correctness; the
//! lint `clippy::undocumented_unsafe_blocks` is an error.
