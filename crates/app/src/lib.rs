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
//! - `about`: the About Orchestra dialog, its texts, the version and the repository's address
//!   (ADR-0050 DG10, F2.12a3b);
//! - `actions`: the program's actions with their default shortcuts per platform (ADR-0048 AM6);
//! - `demo`: the demonstration window of ADR-0040 W8, until the real views exist;
//! - `preferences`: the Preferences' texts, content and changes (ADR-0050 DG8–DG9, F2.12a4a);
//! - `screenshots`: the catalogue of screenshot scenes and how one is rendered (ADR-0041);
//! - `settings`: the settings of the session, their writing on a thread of their own, and their
//!   translation between the settings file and the interface's types (ADR-0050 DG6, F2.12a2).
//!
//! ## What this crate must NOT do
//! - Contain business logic: if something here grows beyond wiring, it belongs in a library crate.
//!   The views themselves will move to `orchestra-ui` as soon as they are real widgets (F2.08
//!   onwards); only the wiring stays.
//!
//! ## Allowed dependencies
//! Every workspace crate (it is the only crate allowed to see all of them).

#![forbid(unsafe_code)]

mod about;
mod actions;
mod demo;
mod files;
mod preferences;
mod project;
mod screenshots;
mod settings;
mod template;

pub use about::{AboutTexts, REPOSITORY, VERSION, about_dialog};
pub use actions::{
    ActionIds, ActionTexts, ActionsError, TITLES, action_texts, field_menu, menu_bar, registry,
    system_texts,
};
pub use demo::{Demo, DemoMessage, DemoTexts, StaffPreview};
pub use files::{
    FileTexts, NameTemplate, ProjectTexts, open_request, project_error_dialog, replace_dialog,
    save_changes_dialog, save_request, unavailable_dialog, window_title,
};
pub use preferences::{
    Effect, FINE_MODIFIERS, MODES, PlaybackTexts, PreferencesMessage, PreferencesTexts,
    PreferencesView, SECTIONS, Section, apply as apply_preference, open_preferences,
    playback_value_text, show_section,
};
pub use preferences::{
    ThemeEditor, ThemeMessage, ThemeOutcome, ThemeTexts, theme_values,
};
pub use preferences::{
    KeyboardMessage, KeyboardOutcome, KeyboardTexts, SectionWidgets, ShortcutEditor,
    shortcut_matches, shortcuts_text, shown_commands,
};
pub use project::{
    NewDocumentError, OpenProject, ProjectEffect, ProjectError, ProjectFlow, SaveAnswer, Step,
    Then, file_name, new_document, project_name, with_extension_added, with_project_extension,
};
pub use screenshots::{
    SCALE, SCENES, Scene, SceneContext, ScreenshotError, TOOLTIP_SCENE_WIDTH, render, theme,
};
pub use settings::convert::{
    SettingsProblem, apply_settings_shortcuts, custom_theme_from_settings,
    custom_theme_to_settings, fine_modifier_from_settings, fine_modifier_to_settings,
    language_from_settings, language_to_settings, layout_to_entries, mode_to_settings,
    shortcuts_to_settings, theme_from_settings, tuning_from_settings, tuning_to_settings,
};
pub use settings::{SettingsStore, WRITE_DELAY};
pub use template::Template;
