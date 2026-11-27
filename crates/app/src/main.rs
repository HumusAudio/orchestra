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

use orchestra_app::{Demo, DemoTexts, SettingsStore, language_from_settings, theme_from_settings};
use orchestra_i18n::Localizer;
use orchestra_ui::{LanguageSource, SystemLanguages, WindowOptions};

/// Program entry point: opens the demonstration window of ADR-0040 W8 until the real interface
/// exists.
///
/// The title is the localised application name (CLAUDE.md §7), in the language of the settings
/// file (F2.12a4b1; *System language* is en-US until F2.13 detects the system's). The theme comes from the **settings file** (ADR-0050 DG6,
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
    // The language of the settings (F2.12a4b1), applied from the start; *System language* (the
    // default) is the first of the system's languages that one of ours serves, en-US when none
    // does (ADR-0053 LR2–LR4, F2.13).
    let system = SystemLanguages.locale();
    let locale =
        language_from_settings(&store.settings().language, &mut problems).unwrap_or(system);
    for problem in &problems {
        eprintln!("orchestra: settings: {problem}");
    }
    let texts = Localizer::new(locale).and_then(|localizer| {
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
    let mut demo = Demo::with_decorations(
        &options.faces,
        &theme,
        &options.title,
        &texts,
        decorations,
        button_layout.as_deref(),
    );
    // The dialogs' buttons in this system's order (ADR-0050 DG2): affirmative first on Windows
    // and KDE Plasma, last on macOS and GNOME (`XDG_CURRENT_DESKTOP`), read once here.
    demo.ui_mut()
        .set_button_order(orchestra_ui::system_button_order());
    // The settings go into the demonstration: the Preferences change them and the window wakes to
    // write them (F2.12a4a); they come back at exit to be closed.
    // The system's languages, read again whenever *System language* is chosen (ADR-0053 LR4).
    demo.set_language_source(Box::new(SystemLanguages));
    for problem in demo.attach_settings(store) {
        eprintln!("orchestra: settings: {problem}");
    }
    let result = orchestra_ui::run(&mut demo, options);
    let store = demo.take_settings();
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
