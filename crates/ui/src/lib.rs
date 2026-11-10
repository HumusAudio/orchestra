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
//! - Themes from the first widget (ADR-0014, ADR-0037): widgets and renderers read only semantic
//!   colour and shape tokens of the active theme, never literal values. Ten built-in light/dark
//!   pairs (Studio by default) and custom themes stored as overrides of a built-in pair; the mode
//!   follows the operating system or is forced light or dark. Implemented in the private module
//!   `theme`, re-exported at the root.
//! - The operating system's light/dark preference and its changes (ADR-0014 §5, ADR-0038), in the
//!   private module `platform` behind the trait `SystemAppearanceSource`: the XDG desktop portal
//!   on Linux; on Windows and macOS `winit` from step F2.04, no answer (dark) until then.
//! - The only logo is `assets/logo/orchestra-logo.svg`, recoloured per theme, never reshaped.
//! - Drawing through a display list of themed commands in segments, redrawn only where damaged,
//!   with a `tiny-skia` backend that also renders headless into an image (ADR-0039, private module
//!   `draw`); PNG encoding for `cargo xtask screenshots` comes in F2.05.
//! - Every visible string comes from `orchestra-i18n`; none is written in the code.
//!
//! ## What this crate must NOT do
//! - Keep its own copy of musical data: view state (zoom, scroll, selection) only.
//! - Modify the model other than through commands.
//!
//! ## Allowed dependencies
//! `orchestra-core-time`, `orchestra-core-model`, `orchestra-playback`, `orchestra-notation-layout`,
//! `orchestra-i18n`, `orchestra-video`. External: `zbus` and `futures-lite` on Linux only, for the
//! desktop portal (ADR-0038 S2).

#![forbid(unsafe_code)]

mod draw;
mod platform;
mod theme;

// The theme system (ADR-0014, ADR-0037), re-exported at the root as ADR-0034 D1 requires.
pub use theme::builtin::BuiltInTheme;
pub use theme::color::Color;
pub use theme::contrast::{
    CONTRAST_PAIRS, ContrastFailure, ContrastKind, ContrastPair, EXEMPT_TOKENS, contrast_ratio,
    relative_luminance,
};
pub use theme::custom::{CustomTheme, Override};
pub use theme::mode::{Appearance, ThemeMode};
pub use theme::pair::ThemePair;
pub use theme::token::{ColorToken, Shape, ShapeToken, Theme};

// The drawing abstraction (ADR-0039), re-exported like the theme system.
pub use draw::Renderer;
pub use draw::damage::Damage;
pub use draw::geometry::{Point, Rect, RoundedRect, Transform};
pub use draw::image::{DrawError, Image, ImageId};
pub use draw::list::{DisplayList, PathBuilder, Recorder, SegmentId};
pub use draw::paint::{FillRule, LineCap, LineJoin, Paint, StrokeStyle};
pub use draw::skia::CpuRenderer;

// The system's light/dark preference (ADR-0014 §5, ADR-0038), re-exported like the theme system.
pub use platform::appearance::{
    AppearanceCallback, AppearanceWatch, SystemAppearanceError, SystemAppearanceSource,
    system_appearance_source,
};
