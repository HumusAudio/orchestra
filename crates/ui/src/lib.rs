// SPDX-License-Identifier: GPL-3.0-or-later

//! # orchestra-ui — the user interface
//!
//! ## Purpose
//! Present the single model through its views — **notation**, **DAW** (always the unrolled
//! timeline, linked to the score) and **video** — and turn user input into model commands.
//!
//! ## Responsibilities (planned; toolkit chosen in ADR-0012)
//! - Draw the geometry produced by `orchestra-notation-layout` and the DAW timeline, incrementally,
//!   with a software rendering fallback when no GPU is available.
//! - Our own drawn widgets and bundled fonts: same look and behaviour on every platform, while
//!   respecting system conventions (primary modifier Ctrl/Cmd, global menu bar on macOS, native
//!   file dialogs) through `platform/` modules behind traits.
//! - Headless rendering to PNG, used by `cargo xtask screenshots` for the user guides.
//! - Every visible string comes from `orchestra-i18n`; none is written in the code.
//!
//! ## What this crate must NOT do
//! - Keep its own copy of musical data: view state (zoom, scroll, selection) only.
//! - Modify the model other than through commands.
//!
//! ## Allowed dependencies
//! `orchestra-core-time`, `orchestra-core-model`, `orchestra-playback`, `orchestra-notation-layout`,
//! `orchestra-i18n`, `orchestra-video`.

#![forbid(unsafe_code)]
