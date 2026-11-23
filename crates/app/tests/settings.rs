// SPDX-License-Identifier: GPL-3.0-or-later

//! The settings between the file and the interface (ADR-0050 DG6, SCALETTA F2.12a2, choice S7):
//! the golden file's values are all known to the interface, the defaults agree, what the
//! interface refuses becomes a problem, and each conversion has its inverse.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test code: a failed unwrap or index fails the test"
)]

use std::path::Path;

use orchestra_app::{
    SettingsProblem, apply_settings_shortcuts, custom_theme_from_settings,
    custom_theme_to_settings, language_from_settings, language_to_settings, mode_to_settings,
    registry, shortcuts_to_settings, theme_from_settings,
};
use orchestra_i18n::Locale;
use orchestra_io::settings::{
    self, DEFAULT_PAIR, Language, Mode, Settings, ShortcutEntry, ThemeChoice, ThemeOverride,
};
use orchestra_ui::{BuiltInTheme, Platform, ThemeMode};

/// The settings of the io crate's golden file with every kind of value.
fn everything() -> Settings {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("io")
        .join("tests")
        .join("golden")
        .join("settings-v1")
        .join("everything.json");
    let parsed = settings::from_bytes(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(parsed.warnings, Vec::new());
    parsed.settings
}

/// The file's default pair is the interface's (ADR-0037 T1), and the defaults give Studio in the
/// *System* mode with no problem.
#[test]
fn the_defaults_agree() {
    assert_eq!(DEFAULT_PAIR, BuiltInTheme::default().id());
    let mut problems = Vec::new();
    let (pair, mode) = theme_from_settings(&Settings::default().theme, &mut problems);
    assert_eq!(pair, *BuiltInTheme::Studio.pair());
    assert_eq!(mode, ThemeMode::System);
    assert_eq!(
        language_from_settings(&Settings::default().language, &mut problems),
        None
    );
    assert_eq!(problems, Vec::new());
}

/// Every name in the golden file is known to the interface: the custom theme resolves, its
/// overrides all apply, and converting it back gives the file's theme.
#[test]
fn the_golden_file_is_understood() {
    let settings = everything();
    let mut problems = Vec::new();
    let (pair, mode) = theme_from_settings(&settings.theme, &mut problems);
    assert_eq!(mode, ThemeMode::Dark);
    let night = &settings.theme.custom[0];
    let converted = custom_theme_from_settings(night, &mut problems);
    assert_eq!(converted.base, BuiltInTheme::Slate);
    assert_eq!(converted.light.len(), night.light.len());
    assert_eq!(converted.dark.len(), night.dark.len());
    assert_eq!(pair, converted.resolve());
    assert_eq!(custom_theme_to_settings(&night.name, &converted), *night);
    assert_eq!(
        language_from_settings(&settings.language, &mut problems),
        Some(Locale::It)
    );
    assert_eq!(problems, Vec::new());
}

/// What the interface does not know: the default in its place, and a problem naming it.
#[test]
fn unknown_names_become_problems() {
    let mut problems = Vec::new();
    let mut theme = Settings::default().theme;
    theme.choice = ThemeChoice::BuiltIn("sepia".to_owned());
    let (pair, _) = theme_from_settings(&theme, &mut problems);
    assert_eq!(pair, *BuiltInTheme::Studio.pair());
    let mut custom = everything().theme.custom[0].clone();
    custom.base = "nowhere".to_owned();
    custom.light.push(ThemeOverride::Color {
        token: "glow".to_owned(),
        rgba: 0,
    });
    let converted = custom_theme_from_settings(&custom, &mut problems);
    assert_eq!(converted.base, BuiltInTheme::Studio);
    assert_eq!(converted.light.len(), 2);
    assert_eq!(
        language_from_settings(&Language::Tag("pt".to_owned()), &mut problems),
        None
    );
    assert_eq!(
        problems,
        vec![
            SettingsProblem::UnknownPair("sepia".to_owned()),
            SettingsProblem::UnknownPair("nowhere".to_owned()),
            SettingsProblem::UnknownOverride {
                theme: "Night desk".to_owned(),
                name: "glow".to_owned()
            },
            SettingsProblem::UnknownLanguage("pt".to_owned()),
        ]
    );
}

/// The shortcuts: applied to the registry and read back to the same entries; an unreadable text
/// and an unknown action are problems.
#[test]
fn shortcuts_round_trip_through_the_registry() {
    let (mut registry, _) = registry(Platform::Linux).unwrap();
    let mut problems = Vec::new();
    let golden = everything().shortcuts;
    apply_settings_shortcuts(&golden, &mut registry, &mut problems);
    assert_eq!(problems, Vec::new());
    assert_eq!(shortcuts_to_settings(&registry), golden);

    let odd = vec![
        ShortcutEntry {
            action: "file.open".to_owned(),
            keys: vec!["Ctrl+o".to_owned(), "F3".to_owned()],
        },
        ShortcutEntry {
            action: "gone.action".to_owned(),
            keys: vec!["F4".to_owned()],
        },
    ];
    apply_settings_shortcuts(&odd, &mut registry, &mut problems);
    assert!(matches!(
        problems.as_slice(),
        [
            SettingsProblem::Shortcut { action, .. },
            SettingsProblem::Registry(orchestra_ui::LoadWarning::UnknownAction(gone)),
        ] if action == "file.open" && gone == "gone.action"
    ));
    // The unknown action is kept and written back (ADR-0048 AM7).
    assert_eq!(shortcuts_to_settings(&registry), {
        let mut expected = vec![ShortcutEntry {
            action: "file.open".to_owned(),
            keys: vec!["F3".to_owned()],
        }];
        expected.push(odd[1].clone());
        expected
    });
}

/// The small inverses.
#[test]
fn modes_and_languages_invert() {
    for mode in [ThemeMode::System, ThemeMode::Light, ThemeMode::Dark] {
        let mut theme = Settings::default().theme;
        theme.mode = mode_to_settings(mode);
        assert_eq!(theme_from_settings(&theme, &mut Vec::new()).1, mode);
    }
    assert_eq!(mode_to_settings(ThemeMode::Light), Mode::Light);
    for locale in [None, Some(Locale::EnGb), Some(Locale::De)] {
        assert_eq!(
            language_from_settings(&language_to_settings(locale), &mut Vec::new()),
            locale
        );
    }
}
