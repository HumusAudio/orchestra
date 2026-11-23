// SPDX-License-Identifier: GPL-3.0-or-later

//! # orchestra — the executable
//!
//! ## Purpose
//! The thin top of the dependency graph: parse the command line, set up logging, create the model,
//! start the audio engine and the plugin host, and hand control to the UI. All real work lives in
//! the library crates, so that it can be tested without starting the program.
//!
//! ## Today
//! It opens the demonstration window of ADR-0040 W8 ([`orchestra_app::Demo`], in the crate's
//! library so that `orchestra-screenshots` draws the very same view, ADR-0041), which checks
//! presentation, damage, input and scale by eye until the real views exist.
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

use std::process::ExitCode;

use orchestra_app::{Demo, DemoTexts};
use orchestra_i18n::{Locale, Localizer};
use orchestra_ui::{BuiltInTheme, ThemeMode, WindowOptions};

/// Program entry point: opens the demonstration window of ADR-0040 W8 until the real interface
/// exists.
///
/// The title is the localised application name (CLAUDE.md §7; the language is fixed to en-US
/// until F2.13 detects the system's). The theme is Studio (ADR-0037 T1) in the *System* mode: the
/// window reads the system's light/dark preference and follows its changes (ADR-0038), until the
/// Preferences of F2.12 let the user choose both.
fn main() -> ExitCode {
    let texts = Localizer::new(Locale::EnUs).and_then(|localizer| {
        Ok((
            localizer.format("app-name", None)?,
            DemoTexts::new(&localizer)?,
        ))
    });
    let (title, texts) = match texts {
        Ok(texts) => texts,
        Err(error) => {
            eprintln!("orchestra: the interface messages could not be loaded: {error}");
            return ExitCode::FAILURE;
        }
    };
    // The bundled faces are parsed once here and lent to the window (ADR-0044 TB2).
    let faces = match orchestra_text::Faces::bundled() {
        Ok(faces) => faces,
        Err(error) => {
            eprintln!("orchestra: the bundled fonts could not be read: {error}");
            return ExitCode::FAILURE;
        }
    };
    // Who draws the window's frame, asked once before the interface is built (ADR-0049 TB2): on
    // Wayland without the compositor's decorations (GNOME) the demonstration draws its own title
    // bar, and the window is opened without decorations to match.
    let decorations = orchestra_ui::decorations();
    let options = WindowOptions {
        title,
        pair: *BuiltInTheme::Studio.pair(),
        mode: ThemeMode::System,
        width: 960.0,
        height: 600.0,
        faces,
        decorations,
    };
    // The density, text and shape tokens of Studio (the same in both halves); the colours follow
    // the system through the window.
    let theme = *options.pair.instance(orchestra_ui::Appearance::Light);
    // GNOME's choice of title bar buttons, read once, only where our title bar is drawn (F2.11b3).
    let button_layout = (decorations == orchestra_ui::Decorations::Own)
        .then(orchestra_ui::button_layout)
        .flatten();
    let demo = Demo::with_decorations(
        &options.faces,
        &theme,
        &options.title,
        &texts,
        decorations,
        button_layout.as_deref(),
    );
    match orchestra_ui::run(demo, options) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("orchestra: {error}");
            ExitCode::FAILURE
        }
    }
}
