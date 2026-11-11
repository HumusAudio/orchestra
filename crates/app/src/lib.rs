// SPDX-License-Identifier: GPL-3.0-or-later

//! # orchestra-app — the wiring shared by the executables
//!
//! ## Purpose
//! The crate builds two programs from the same code (ADR-0041 S1):
//! - `orchestra`, the program users run (`src/main.rs`);
//! - `orchestra-screenshots`, a developer tool that renders the guides' screenshots headless
//!   (`src/bin/orchestra-screenshots.rs`, run by `cargo xtask screenshots`). It is never
//!   distributed to users.
//!
//! Both need the same views: the screenshots must show exactly what the program shows, drawn by
//! the same code with the same renderer. A library target is how two binaries of one crate share
//! modules without copying them, so the views live here and each binary is a thin `main`.
//!
//! ## Modules (private, ADR-0034 D1: the root re-exports the API)
//! - `demo`: the demonstration window of ADR-0040 W8, until the real views exist;
//! - `screenshots`: the catalogue of screenshot scenes and how one is rendered (ADR-0041).
//!
//! ## What this crate must NOT do
//! - Contain business logic: if something here grows beyond wiring, it belongs in a library crate.
//!   The views themselves will move to `orchestra-ui` as soon as they are real widgets (F2.08
//!   onwards); only the wiring stays.
//!
//! ## Allowed dependencies
//! Every workspace crate (it is the only crate allowed to see all of them).

#![forbid(unsafe_code)]

mod demo;
mod screenshots;

pub use demo::Demo;
pub use screenshots::{SCALE, SCENES, Scene, ScreenshotError, render, theme};
