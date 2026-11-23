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

use orchestra_app::{Demo, DemoTexts, SettingsStore, theme_from_settings};
use orchestra_i18n::{Locale, Localizer};
use orchestra_ui::WindowOptions;

/// Program entry point: opens the demonstration window of ADR-0040 W8 until the real interface
/// exists.
///
/// The title is the localised application name (CLAUDE.md §7; the language is fixed to en-US
/// until F2.13 detects the system's). The theme comes from the **settings file** (ADR-0050 DG6,
/// F2.12a2): the pair (or custom theme) and the mode saved there, Studio in the *System* mode by
/// default; in *System* the window reads the system's light/dark preference and follows its
/// changes (ADR-0038). What the settings could not use is written to the standard error (choice
/// S4: no log yet), and the program starts anyway.
fn main() -> ExitCode {
    // The settings first, before the window: a few kilobytes read synchronously (choice S7).
    let (store, warnings) = SettingsStore::open(orchestra_io::settings::default_path());
    for warning in &warnings {
        eprintln!("orchestra: settings: {warning}");
    }
    let mut problems = Vec::new();
    let (pair, mode) = theme_from_settings(&store.settings().theme, &mut problems);
    for problem in &problems {
        eprintln!("orchestra: settings: {problem}");
    }
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
        pair,
        mode,
        width: 960.0,
        height: 600.0,
        faces,
        decorations,
    };
    // The density, text and shape tokens of the chosen pair's light half: the same in both halves
    // of every built-in pair, so only a custom theme that changes them in one half differs (the
    // layout following the mode for those comes with the Preferences, F2.12a4); the colours
    // follow the mode through the window.
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
    let result = orchestra_ui::run(demo, options);
    // What is still waiting is written, and the writer thread is waited for (DG6).
    for failure in store.close() {
        eprintln!("orchestra: settings: {failure}");
    }
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("orchestra: {error}");
            ExitCode::FAILURE
        }
    }
}
