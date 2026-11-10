// SPDX-License-Identifier: GPL-3.0-or-later

//! # orchestra — the executable
//!
//! ## Purpose
//! The thin top of the dependency graph: parse the command line, set up logging, create the model,
//! start the audio engine and the plugin host, and hand control to the UI. All real work lives in
//! the library crates, so that it can be tested without starting the program.
//!
//! ## Today
//! It opens the demonstration window of ADR-0040 W8 (`demo`), which checks presentation, damage,
//! input and scale by eye until the real views exist.
//!
//! ## What this crate must NOT do
//! - Contain business logic: if something here grows beyond wiring, it belongs in a library crate.
//!
//! ## Allowed dependencies
//! Every workspace crate (it is the only crate allowed to see all of them).

#![forbid(unsafe_code)]
#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "the executable is where output to the terminal belongs (command-line errors, --help); \
              the library crates must not print (workspace lints)"
)]

mod demo;

use std::process::ExitCode;

use orchestra_i18n::{Locale, Localizer};
use orchestra_ui::{
    BuiltInTheme, SystemAppearanceSource, ThemeMode, WindowOptions, system_appearance_source,
};

/// Program entry point: opens the demonstration window of ADR-0040 W8 until the real interface
/// exists.
///
/// The title is the localised application name (CLAUDE.md §7; the language is fixed to en-US
/// until F2.13 detects the system's). The theme is Studio (ADR-0037 T1) in the system's mode, read
/// once at start (ADR-0038); following its changes comes in F2.04b.
fn main() -> ExitCode {
    let title = match Localizer::new(Locale::EnUs).and_then(|l| l.format("app-name", None)) {
        Ok(title) => title,
        Err(error) => {
            eprintln!("orchestra: the interface messages could not be loaded: {error}");
            return ExitCode::FAILURE;
        }
    };
    // Read the system's preference once; the watch is dropped right after (no changes followed).
    let system = system_appearance_source()
        .observe(Box::new(|_| {}))
        .current();
    let theme = *BuiltInTheme::Studio
        .pair()
        .instance(ThemeMode::System.resolve(system));
    let options = WindowOptions {
        title,
        theme,
        width: 960.0,
        height: 600.0,
    };
    match orchestra_ui::run(demo::Demo::default(), options) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("orchestra: {error}");
            ExitCode::FAILURE
        }
    }
}
