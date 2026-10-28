// SPDX-License-Identifier: GPL-3.0-or-later

//! # orchestra — the executable
//!
//! ## Purpose
//! The thin top of the dependency graph: parse the command line, set up logging, create the model,
//! start the audio engine and the plugin host, and hand control to the UI. All real work lives in
//! the library crates, so that it can be tested without starting the program.
//!
//! ## What this crate must NOT do
//! - Contain business logic: if something here grows beyond wiring, it belongs in a library crate.
//!
//! ## Allowed dependencies
//! Every workspace crate (it is the only crate allowed to see all of them).

#![forbid(unsafe_code)]

/// Program entry point.
///
/// Intentionally empty during Phase 0: the workspace skeleton only has to compile. Wiring arrives
/// once the UI toolkit (ADR-0012) and the audio engine exist.
fn main() {}
