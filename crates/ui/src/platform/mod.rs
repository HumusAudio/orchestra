// SPDX-License-Identifier: GPL-3.0-or-later

//! Platform-specific code of the interface (ADR-0006 §2).
//!
//! Everything that differs between Linux, Windows and macOS lives under this module, behind
//! traits, so the rest of the crate never asks which system it runs on. Each submodule is one
//! concern, and inside it the per-platform files are chosen with `#[cfg(target_os = ...)]`:
//!
//! - `appearance`: the system's light/dark preference and its changes (ADR-0014 §5, ADR-0038).
//! - `clipboard`: the clipboard's text, the system's through `arboard` or one in memory
//!   (ADR-0047 TF12, F2.10c1).
//! - `decorations`: who draws the window's frame, the system or our own title bar; on Wayland a
//!   probe asks the compositor (ADR-0049 TB1, TB2, F2.11b2).
//! - `dialog`: the order of a dialog's buttons, from the system and on Linux from the desktop
//!   (ADR-0050 DG2, F2.12a3a).
//! - `menu`: the system's menu bar, `muda` on macOS (ADR-0036 B5, ADR-0048 AM9, F2.11e1).
//!
//! Like every module of the library crates, it is private (ADR-0034 D1): what callers need is
//! re-exported at the crate root.

pub(crate) mod appearance;
pub(crate) mod clipboard;
pub(crate) mod decorations;
pub(crate) mod dialog;
pub(crate) mod menu;
