// SPDX-License-Identifier: GPL-3.0-or-later

//! The theme file (ADR-0052, SCALETTA F2.12b2b), case by case:
//! - the **golden file** of version 1 (`tests/golden/theme-v1/`): read to the expected theme and
//!   written back byte for byte (CLAUDE.md §9);
//! - **leniency**: misshapen overrides dropped alone and counted, unknown fields counted, a
//!   missing name or base left empty for the application;
//! - **whole-file failures**: not a theme file, no theme in it, not JSON, a newer version, too
//!   large;
//! - **files**: saved and loaded back, a missing file, saving into a missing folder.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic,
    reason = "test code: a failed unwrap, index or panic fails the test"
)]

use std::fs;
use std::path::{Path, PathBuf};

use orchestra_io::settings::{CustomTheme, ThemeOverride};
use orchestra_io::theme_file::{
    self, EXTENSION, FORMAT_VERSION, MAX_FILE_BYTES, ThemeFile, ThemeFileError,
};

/// The golden file of version 1.
fn golden() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
        .join("theme-v1")
        .join(format!("Night.{EXTENSION}"))
}

/// What the golden file holds: a colour in each half (one translucent), the shared text size,
/// shape, family and density written in both halves, and a token this version does not know.
fn night() -> CustomTheme {
    let shared = vec![
        ThemeOverride::TextSize {
            token: "size-body".to_owned(),
            size: 14,
        },
        ThemeOverride::Shape {
            token: "corner-radius".to_owned(),
            value: 12,
        },
        ThemeOverride::Family {
            token: "family-ui".to_owned(),
            family: "jetbrains-mono".to_owned(),
        },
        ThemeOverride::Density("compact".to_owned()),
    ];
    let mut light = vec![ThemeOverride::Color {
        token: "accent".to_owned(),
        rgba: 0x3366_ccff,
    }];
    light.extend(shared.iter().cloned());
    let mut dark = vec![
        ThemeOverride::Color {
            token: "accent".to_owned(),
            rgba: 0x5b9b_f0ff,
        },
        ThemeOverride::Color {
            token: "future-token".to_owned(),
            rgba: 0x0000_0080,
        },
    ];
    dark.extend(shared);
    CustomTheme {
        name: "Night".to_owned(),
        base: "studio".to_owned(),
        light,
        dark,
    }
}

/// A fresh folder for the test `name`.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("orchestra-theme-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn the_golden_file_of_version_1_reads_and_writes_back() {
    assert_eq!(FORMAT_VERSION, 1);
    let bytes = fs::read(golden()).unwrap();
    assert_eq!(
        theme_file::from_bytes(&bytes).unwrap(),
        ThemeFile {
            theme: night(),
            dropped: 0
        }
    );
    assert_eq!(theme_file::to_bytes(&night()).unwrap(), bytes);
}

#[test]
fn misshapen_entries_are_dropped_alone_and_counted() {
    let text = br##"{ "orchestra_theme": 1, "note": "a top-level field is ignored",
        "theme": {
          "name": "Mine", "base": "slate", "author": "someone",
          "light": [
            { "color": "accent", "value": "#123456" },
            { "color": "accent", "value": "blue" },
            { "shape": "corner-radius", "value": 999 },
            "not an override"
          ],
          "dark": []
        } }"##;
    let file = theme_file::from_bytes(text).unwrap();
    assert_eq!(file.theme.name, "Mine");
    assert_eq!(file.theme.base, "slate");
    assert_eq!(
        file.theme.light,
        [ThemeOverride::Color {
            token: "accent".to_owned(),
            rgba: 0x1234_56ff
        }]
    );
    assert_eq!(
        file.dropped, 4,
        "an unknown field, a colour, a shape out of range, a string"
    );
    // No name, no base: left empty for the application (the file's name, Studio).
    let bare = theme_file::from_bytes(br#"{ "orchestra_theme": 1, "theme": {} }"#).unwrap();
    assert_eq!(
        (bare.theme.name.as_str(), bare.theme.base.as_str()),
        ("", "")
    );
    assert_eq!(bare.dropped, 0);
}

#[test]
fn files_that_are_not_theme_files_are_refused() {
    for text in [
        &br#"{ "orchestra_shortcuts": 1, "shortcuts": [] }"#[..],
        b"plain text",
        br#"{ "orchestra_theme": 1 }"#,
        br#"{ "orchestra_theme": 1, "theme": [] }"#,
    ] {
        assert_eq!(
            theme_file::from_bytes(text),
            Err(ThemeFileError::NotTheme),
            "{}",
            String::from_utf8_lossy(text)
        );
    }
    assert!(matches!(
        theme_file::from_bytes(br#"{ "orchestra_theme": 1, "theme": { "#),
        Err(ThemeFileError::Syntax { .. })
    ));
    assert_eq!(
        theme_file::from_bytes(br#"{ "orchestra_theme": 2, "theme": {} }"#),
        Err(ThemeFileError::NewerFormat {
            found: 2,
            supported: 1
        })
    );
    let large = vec![b' '; usize::try_from(MAX_FILE_BYTES).unwrap() + 1];
    assert!(matches!(
        theme_file::from_bytes(&large),
        Err(ThemeFileError::TooLarge { .. })
    ));
}

#[test]
fn saved_files_load_back_and_failures_name_the_path() {
    let dir = scratch("files");
    let path = dir.join(format!("Night.{EXTENSION}"));
    theme_file::save(&night(), &path).unwrap();
    assert_eq!(theme_file::load(&path).unwrap().theme, night());
    let missing = dir.join("missing");
    match theme_file::load(&missing) {
        Err(ThemeFileError::Io(failure)) => {
            assert_eq!(failure.kind(), std::io::ErrorKind::NotFound);
            assert_eq!(failure.path(), missing);
        }
        other => panic!("expected an I/O error, got {other:?}"),
    }
    let nowhere = dir.join("no folder").join(format!("Night.{EXTENSION}"));
    assert!(matches!(
        theme_file::save(&night(), &nowhere),
        Err(ThemeFileError::Io(_))
    ));
    assert!(!nowhere.exists());
    let _ = fs::remove_dir_all(&dir);
}
