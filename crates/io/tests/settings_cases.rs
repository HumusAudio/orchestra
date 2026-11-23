// SPDX-License-Identifier: GPL-3.0-or-later

//! The settings file (ADR-0050 DG6–DG7, SCALETTA F2.12a2), case by case:
//! - **Golden files** of version 1 (`tests/golden/settings-v1/`): read to the expected settings,
//!   and written back byte for byte (CLAUDE.md §9: every version's files stay readable).
//! - **Round trip**: a property test over generated settings, `to_bytes` then `from_bytes` gives
//!   the same settings with no warning.
//! - **Leniency**: every field wrong on its own keeps the others; unknown fields; a newer version;
//!   files that are not settings.
//! - **Files**: a missing file, a broken file moved aside (replacing an earlier one), a newer file
//!   marked read-only, saving into a directory that does not exist yet.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic,
    reason = "test code: a failed unwrap, index or panic fails the test"
)]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use orchestra_io::settings::{
    self, BROKEN_FILE_NAME, Broken, CustomTheme, FILE_NAME, FineModifier, Language, Mode, Settings,
    ShortcutEntry, ThemeChoice, ThemeOverride, ThemeSettings, Warning,
};
use proptest::prelude::*;

/// The directory of the version-1 golden files.
fn golden(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
        .join("settings-v1")
        .join(name)
}

/// The settings `everything.json` holds.
fn everything() -> Settings {
    Settings {
        theme: ThemeSettings {
            choice: ThemeChoice::Custom("Night desk".to_owned()),
            mode: Mode::Dark,
            custom: vec![
                CustomTheme {
                    name: "Night desk".to_owned(),
                    base: "slate".to_owned(),
                    light: vec![
                        ThemeOverride::Color {
                            token: "accent".to_owned(),
                            rgba: 0x3366_ccff,
                        },
                        ThemeOverride::Shape {
                            token: "corner-radius".to_owned(),
                            value: 4,
                        },
                    ],
                    dark: vec![
                        ThemeOverride::Family {
                            token: "family-ui".to_owned(),
                            family: "inter".to_owned(),
                        },
                        ThemeOverride::TextSize {
                            token: "size-body".to_owned(),
                            size: 14,
                        },
                        ThemeOverride::Density("compact".to_owned()),
                    ],
                },
                CustomTheme {
                    name: "Paper".to_owned(),
                    base: "parchment".to_owned(),
                    light: Vec::new(),
                    dark: Vec::new(),
                },
            ],
        },
        language: Language::Tag("it".to_owned()),
        caret_blink: false,
        fine_modifier: FineModifier::Shift,
        shortcuts: vec![
            ShortcutEntry {
                action: "file.save".to_owned(),
                keys: vec!["Primary+s".to_owned(), "F2".to_owned()],
            },
            ShortcutEntry {
                action: "edit.redo".to_owned(),
                keys: Vec::new(),
            },
        ],
        playback: BTreeMap::from([
            ("accent-step".to_owned(), 2),
            ("staccato-percent".to_owned(), 50),
        ]),
    }
}

/// A fresh, empty directory for one test (removed first if a previous run left it).
fn scratch(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("orchestra-settings-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// The golden files read to their settings with no warning, and are written back identical.
#[test]
fn golden_files_of_version_1() {
    for (name, expected) in [
        ("defaults.json", Settings::default()),
        ("everything.json", everything()),
    ] {
        let bytes = fs::read(golden(name)).unwrap();
        // A checkout on Windows may turn the newlines into CRLF (`.gitattributes` keeps them LF,
        // this is a second guard): the comparison is on the meaning of the bytes.
        let bytes: Vec<u8> = bytes.into_iter().filter(|&b| b != b'\r').collect();
        let parsed = settings::from_bytes(&bytes).unwrap();
        assert_eq!(parsed.settings, expected, "{name}");
        assert!(parsed.warnings.is_empty(), "{name}: {:?}", parsed.warnings);
        assert!(!parsed.newer);
        assert_eq!(settings::to_bytes(&expected).unwrap(), bytes, "{name}");
    }
}

/// A strategy over short texts, including the ones that need escaping in JSON.
fn text() -> impl Strategy<Value = String> {
    "[a-zA-Z0-9 .:+\"\\\\é]{1,12}"
}

/// A strategy over one override.
fn an_override() -> impl Strategy<Value = ThemeOverride> {
    prop_oneof![
        (text(), any::<u32>()).prop_map(|(token, rgba)| ThemeOverride::Color { token, rgba }),
        (text(), any::<u8>()).prop_map(|(token, value)| ThemeOverride::Shape { token, value }),
        (text(), text()).prop_map(|(token, family)| ThemeOverride::Family { token, family }),
        (text(), any::<u8>()).prop_map(|(token, size)| ThemeOverride::TextSize { token, size }),
        text().prop_map(ThemeOverride::Density),
    ]
}

/// A strategy over whole settings, with unique custom theme names and a valid `use`.
fn any_settings() -> impl Strategy<Value = Settings> {
    let custom = prop::collection::btree_map(
        text(),
        (
            text(),
            prop::collection::vec(an_override(), 0..4),
            prop::collection::vec(an_override(), 0..4),
        ),
        0..3,
    );
    (
        custom,
        any::<bool>(),
        prop_oneof![Just(Mode::System), Just(Mode::Light), Just(Mode::Dark)],
        prop_oneof![
            Just(Language::System),
            "[a-z]{2}(-[A-Z]{2})?".prop_map(Language::Tag)
        ],
        any::<bool>(),
        prop_oneof![
            Just(FineModifier::Primary),
            Just(FineModifier::Shift),
            Just(FineModifier::Alt)
        ],
        prop::collection::vec(
            (text(), prop::collection::vec(text(), 0..3))
                .prop_map(|(action, keys)| ShortcutEntry { action, keys }),
            0..4,
        ),
        prop::collection::btree_map(text(), any::<i64>(), 0..4),
    )
        .prop_map(
            |(
                custom,
                use_custom,
                mode,
                language,
                caret_blink,
                fine_modifier,
                shortcuts,
                playback,
            )| {
                let custom: Vec<CustomTheme> = custom
                    .into_iter()
                    .map(|(name, (base, light, dark))| CustomTheme {
                        name,
                        base,
                        light,
                        dark,
                    })
                    .collect();
                let choice = match custom.first() {
                    Some(first) if use_custom => ThemeChoice::Custom(first.name.clone()),
                    _ => ThemeChoice::BuiltIn("forest".to_owned()),
                };
                Settings {
                    theme: ThemeSettings {
                        choice,
                        mode,
                        custom,
                    },
                    language: if language == Language::Tag("system".to_owned()) {
                        Language::System
                    } else {
                        language
                    },
                    caret_blink,
                    fine_modifier,
                    shortcuts,
                    playback,
                }
            },
        )
}

proptest! {
    /// Writing then reading gives the same settings, with no warning.
    #[test]
    fn round_trip(settings in any_settings()) {
        let bytes = settings::to_bytes(&settings).unwrap();
        let parsed = settings::from_bytes(&bytes).unwrap();
        prop_assert_eq!(&parsed.settings, &settings);
        prop_assert!(parsed.warnings.is_empty(), "{:?}", parsed.warnings);
    }
}

/// Reads `text` (a settings file) and returns the settings and the warnings.
fn read(text: &str) -> (Settings, Vec<Warning>) {
    let parsed = settings::from_bytes(text.as_bytes()).unwrap();
    (parsed.settings, parsed.warnings)
}

/// Each wrong field gives its default and a warning naming it, and leaves the others alone.
#[test]
fn a_wrong_field_keeps_the_others() {
    let (settings, warnings) = read(
        r#"{ "orchestra_settings": 1,
             "theme": { "use": "custom:Missing", "mode": "sepia", "custom": 3 },
             "language": "",
             "caret_blink": "no",
             "fine_modifier": "ctrl",
             "shortcuts": [ { "action": "file.save", "keys": ["Primary+s", 5] },
                            { "keys": [] },
                            7 ],
             "playback": { "accent-step": 2.5, "staccato-percent": 40 },
             "colour": "red" }"#,
    );
    let expected = Settings {
        shortcuts: vec![ShortcutEntry {
            action: "file.save".to_owned(),
            keys: vec!["Primary+s".to_owned()],
        }],
        playback: BTreeMap::from([("staccato-percent".to_owned(), 40)]),
        ..Settings::default()
    };
    assert_eq!(settings, expected);
    let expected_warnings = [
        Warning::WrongType("caret_blink".to_owned()),
        Warning::UnknownField("colour".to_owned()),
        Warning::InvalidValue("fine_modifier".to_owned()),
        Warning::InvalidValue("language".to_owned()),
        Warning::WrongType("playback.accent-step".to_owned()),
        Warning::WrongType("shortcuts[0].keys[1]".to_owned()),
        Warning::InvalidValue("shortcuts[1]".to_owned()),
        Warning::WrongType("shortcuts[2]".to_owned()),
        Warning::WrongType("theme.custom".to_owned()),
        Warning::InvalidValue("theme.mode".to_owned()),
        Warning::InvalidValue("theme.use".to_owned()),
    ];
    // `serde_json` reads objects into sorted maps, so the warnings come in the keys' order.
    let mut warnings = warnings;
    warnings.sort_by_key(ToString::to_string);
    let mut expected_warnings = expected_warnings.to_vec();
    expected_warnings.sort_by_key(ToString::to_string);
    assert_eq!(warnings, expected_warnings);
}

/// The custom themes: one without a base, a repeated name, broken overrides are dropped alone.
#[test]
fn custom_themes_are_checked_one_by_one() {
    let (settings, warnings) = read(
        r##"{ "orchestra_settings": 1,
              "theme": { "use": "custom:A", "custom": [
                { "name": "A", "base": "slate", "light": [
                    { "color": "accent", "value": "#123456" },
                    { "color": "accent", "value": "blue" },
                    { "shape": "corner-radius", "value": 300 },
                    { "density": "compact", "value": 1 },
                    { "color": "accent", "value": "#00000000", "extra": true } ] },
                { "name": "B" },
                { "name": "A", "base": "forest" } ] } }"##,
    );
    assert_eq!(settings.theme.choice, ThemeChoice::Custom("A".to_owned()));
    assert_eq!(settings.theme.custom.len(), 1);
    assert_eq!(
        settings.theme.custom[0].light,
        vec![ThemeOverride::Color {
            token: "accent".to_owned(),
            rgba: 0x1234_56ff
        }]
    );
    assert_eq!(
        warnings,
        vec![
            Warning::InvalidValue("theme.custom[0].light[1]".to_owned()),
            Warning::InvalidValue("theme.custom[0].light[2]".to_owned()),
            Warning::InvalidValue("theme.custom[0].light[3]".to_owned()),
            Warning::InvalidValue("theme.custom[0].light[4]".to_owned()),
            Warning::InvalidValue("theme.custom[1]".to_owned()),
            Warning::Duplicate("theme.custom[2].name".to_owned()),
        ]
    );
}

/// A newer version is read for what is known and reported; unknown fields are ignored.
#[test]
fn a_newer_version_is_read_in_part() {
    let parsed = settings::from_bytes(
        br#"{ "orchestra_settings": 7, "caret_blink": false, "hologram": true }"#,
    )
    .unwrap();
    assert!(parsed.newer);
    assert!(!parsed.settings.caret_blink);
    assert_eq!(
        parsed.warnings,
        vec![
            Warning::NewerFormat {
                found: 7,
                supported: settings::FORMAT_VERSION
            },
            Warning::UnknownField("hologram".to_owned()),
        ]
    );
}

/// What is not a settings file at all.
#[test]
fn files_that_are_not_settings() {
    for bytes in [
        &b""[..],
        b"[]",
        b"{\"orchestra_format\": 1}",
        b"{\"caret_blink\": true}",
    ] {
        assert_eq!(
            settings::from_bytes(bytes),
            Err(Broken::NotSettings),
            "{bytes:?}"
        );
    }
    assert!(matches!(
        settings::from_bytes(b"{\"orchestra_settings\": 1, \"theme\": "),
        Err(Broken::Syntax { line: 1, .. })
    ));
    let huge = vec![b' '; usize::try_from(settings::MAX_FILE_BYTES).unwrap() + 1];
    assert!(matches!(
        settings::from_bytes(&huge),
        Err(Broken::TooLarge { .. })
    ));
    // A byte order mark is accepted, as editors may add one.
    assert!(settings::from_bytes(b"\xEF\xBB\xBF{\"orchestra_settings\": 1}").is_ok());
}

/// No file: the defaults, nothing to warn about, writable.
#[test]
fn a_missing_file_gives_the_defaults() {
    let dir = scratch("missing");
    let loaded = settings::load(&dir.join(FILE_NAME));
    assert_eq!(loaded.settings, Settings::default());
    assert_eq!(loaded.warnings, Vec::<Warning>::new());
    assert!(loaded.writable);
    fs::remove_dir_all(&dir).unwrap();
}

/// A broken file is moved aside, replacing an earlier broken one, and the defaults are used.
#[test]
fn a_broken_file_is_moved_aside() {
    let dir = scratch("broken");
    let path = dir.join(FILE_NAME);
    let kept = dir.join(BROKEN_FILE_NAME);
    fs::write(&kept, b"an older broken file").unwrap();
    fs::write(&path, b"{\"orchestra_settings\": 1, oops").unwrap();
    let loaded = settings::load(&path);
    assert_eq!(loaded.settings, Settings::default());
    assert!(loaded.writable);
    assert!(matches!(
        loaded.warnings.as_slice(),
        [Warning::Broken { reason: Broken::Syntax { .. }, kept_as: Some(p) }] if *p == kept
    ));
    assert!(!path.exists());
    assert_eq!(
        fs::read(&kept).unwrap(),
        b"{\"orchestra_settings\": 1, oops"
    );
    fs::remove_dir_all(&dir).unwrap();
}

/// A newer file is read but marked read-only, and stays as it was.
#[test]
fn a_newer_file_is_never_written_over() {
    let dir = scratch("newer");
    let path = dir.join(FILE_NAME);
    let text = b"{\"orchestra_settings\": 2, \"caret_blink\": false}";
    fs::write(&path, text).unwrap();
    let loaded = settings::load(&path);
    assert!(!loaded.writable);
    assert!(!loaded.settings.caret_blink);
    assert_eq!(fs::read(&path).unwrap(), text);
    fs::remove_dir_all(&dir).unwrap();
}

/// Saving creates the missing directories, and loading gives the settings back.
#[test]
fn saving_creates_the_directory() {
    let dir = scratch("save");
    let path = dir.join("deeper").join("orchestra").join(FILE_NAME);
    settings::save(&path, &everything()).unwrap();
    let loaded = settings::load(&path);
    assert_eq!(loaded.settings, everything());
    assert_eq!(loaded.warnings, Vec::<Warning>::new());
    assert!(loaded.writable);
    fs::remove_dir_all(&dir).unwrap();
}

/// The directory on this system ends with Orchestra's own folder (the rules of each system are
/// tested inside the crate with made-up environments).
#[test]
fn the_directory_is_orchestras_own() {
    if let Some(dir) = settings::directory() {
        let last = dir.file_name().unwrap().to_string_lossy().to_lowercase();
        assert_eq!(last, "orchestra");
        assert_eq!(settings::default_path().unwrap(), dir.join(FILE_NAME));
    }
}
