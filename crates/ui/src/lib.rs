// SPDX-License-Identifier: GPL-3.0-or-later

//! # orchestra-ui — the user interface
//!
//! ## Purpose
//! Present the single model through its views — **notation**, **DAW** (always the unrolled
//! timeline, linked to the score) and **video** — and turn user input into model commands.
//!
//! ## Responsibilities (planned; no GUI toolkit, see ADR-0012)
//! - Draw the geometry produced by `orchestra-notation-layout` and the DAW timeline, incrementally,
//!   with a software rendering fallback when no GPU is available.
//! - Our own widgets, layout, focus, text editing and menus (ADR-0012: no GUI toolkit), built on
//!   low-level blocks behind our traits: `winit` (windows, input, IME), `softbuffer`/`wgpu`
//!   (presenting), `tiny-skia`/`vello` (drawing), our text shaping, `accesskit` (every widget
//!   publishes an accessibility node from day one), `rfd` (native file dialogs).
//! - Bundled fonts: same look and behaviour on every platform, while
//!   respecting system conventions (primary modifier Ctrl/Cmd, global menu bar on macOS, native
//!   file dialogs) through `platform/` modules behind traits.
//! - Light and dark themes from the first widget (ADR-0014): widgets and renderers read only
//!   semantic colour tokens of the active theme, never literal colours; the default follows the
//!   operating system (detected in `platform/` modules), the user can force light or dark.
//! - The only logo is `assets/logo/orchestra-logo.svg`, recoloured per theme, never reshaped.
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
