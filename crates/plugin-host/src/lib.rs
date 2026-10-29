// SPDX-License-Identifier: GPL-3.0-or-later

//! # orchestra-plugin-host — hosting external plugins
//!
//! ## Purpose
//! Provide our own **`PluginHost` interface** (ADR-0011) for external instruments and effects in
//! the CLAP, VST3 and LV2 formats (plus AU on macOS; VST2 is deliberately not supported).
//!
//! ## Responsibilities (planned for Phase 4)
//! - Scan installed plugins and keep a cache of their descriptions.
//! - Run **every plugin in a separate process**, exchanging audio through shared memory, so that a
//!   crashing or misbehaving plugin can never take the host down with it.
//! - Open plugin GUIs in **separate top-level windows** (never embedded), which avoids the
//!   XEmbed/Wayland problems of embedding foreign windows.
//! - Expose to `orchestra-audio-engine` a real-time-safe processing call per buffer.
//!
//! ## What this crate must NOT do
//! - Leak format-specific types (CLAP/VST3/LV2) into its public API: the rest of the program only
//!   sees our interface, so formats can be added or replaced independently.
//!
//! ## Allowed dependencies
//! `orchestra-core-time`, plus the low-level format bindings chosen in ADR-0011: `clap-sys`,
//! `vst3`, `lilv-sys` (links the `lilv` C library, see docs/native-deps.md) and `objc2` for AU.
//! No hosting framework: each format host is ours. Scanning also runs out of process.
//!
//! ## Unsafe code
//! `unsafe` is allowed here (FFI with plugin APIs, shared memory). Every `unsafe` block carries a
//! `// SAFETY:` comment.
