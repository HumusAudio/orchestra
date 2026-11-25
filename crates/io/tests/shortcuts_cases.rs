// SPDX-License-Identifier: GPL-3.0-or-later

//! The shortcut file (ADR-0051, SCALETTA F2.12c2), case by case:
//! - the **golden file** of version 1 (`tests/golden/shortcuts-v1/`): read to the expected entries
//!   and written back byte for byte (CLAUDE.md §9);
//! - **leniency**: misshapen entries dropped alone and counted, unknown fields ignored;
//! - **whole-file failures**: not a shortcut file, not JSON, a newer version, too large;
//! - **files**: saved and loaded back, a missing file, saving into a missing folder.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic,
    reason = "test code: a failed unwrap, index or panic fails the test"
)]

use std::fs;
use std::path::{Path, PathBuf};

use orchestra_io::settings::ShortcutEntry;
use orchestra_io::shortcuts::{
    self, EXTENSION, FORMAT_VERSION, MAX_FILE_BYTES, ShortcutFile, ShortcutFileError,
};

/// The golden file of version 1.
fn golden() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
        .join("shortcuts-v1")
        .join(format!("keyboard.{EXTENSION}"))
}

/// One entry.
fn entry(action: &str, keys: &[&str]) -> ShortcutEntry {
    ShortcutEntry {
        action: action.to_owned(),
        keys: keys.iter().map(|key| (*key).to_owned()).collect(),
    }
}

/// What the golden file holds: one shortcut, two, none, and an action this version does not know.
fn keyboard() -> Vec<ShortcutEntry> {
    vec![
        entry("file.new", &["Primary+n"]),
        entry("edit.redo", &["Primary+Shift+z", "Primary+y"]),
        entry("help.contents", &[]),
        entry("future.action", &["F5"]),
    ]
}

/// A fresh folder for the test `name`.
fn scratch(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("orchestra-shortcuts-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn the_golden_file_of_version_1_reads_and_writes_back() {
    assert_eq!(FORMAT_VERSION, 1);
    let bytes = fs::read(golden()).unwrap();
    assert_eq!(
        shortcuts::from_bytes(&bytes).unwrap(),
        ShortcutFile {
            entries: keyboard(),
            dropped: 0
        }
    );
    assert_eq!(shortcuts::to_bytes(&keyboard()).unwrap(), bytes);
}

#[test]
fn misshapen_entries_are_dropped_alone_and_counted() {
    let text = br#"{ "orchestra_shortcuts": 1, "note": "unknown fields are ignored",
        "shortcuts": [
          { "action": "file.new", "keys": ["Primary+n", 7] },
          { "keys": ["F2"] },
          "not an entry",
          { "action": "file.open", "keys": [] }
        ] }"#;
    let file = shortcuts::from_bytes(text).unwrap();
    assert_eq!(
        file.entries,
        [entry("file.new", &["Primary+n"]), entry("file.open", &[])]
    );
    assert_eq!(file.dropped, 3, "a key, an entry without action, a string");
    // No list at all: an empty keyboard.
    let empty = shortcuts::from_bytes(br#"{ "orchestra_shortcuts": 1 }"#).unwrap();
    assert_eq!(empty, ShortcutFile::default());
}

#[test]
fn files_that_are_not_shortcut_files_are_refused() {
    assert_eq!(
        shortcuts::from_bytes(br#"{ "orchestra_settings": 1 }"#),
        Err(ShortcutFileError::NotShortcuts)
    );
    assert_eq!(
        shortcuts::from_bytes(b"plain text"),
        Err(ShortcutFileError::NotShortcuts)
    );
    assert!(matches!(
        shortcuts::from_bytes(br#"{ "orchestra_shortcuts": 1, "shortcuts": [ "#),
        Err(ShortcutFileError::Syntax { .. })
    ));
    assert_eq!(
        shortcuts::from_bytes(br#"{ "orchestra_shortcuts": 2, "shortcuts": [] }"#),
        Err(ShortcutFileError::NewerFormat {
            found: 2,
            supported: 1
        })
    );
    let large = vec![b' '; usize::try_from(MAX_FILE_BYTES).unwrap() + 1];
    assert!(matches!(
        shortcuts::from_bytes(&large),
        Err(ShortcutFileError::TooLarge { .. })
    ));
}

#[test]
fn saved_files_load_back_and_failures_name_the_path() {
    let dir = scratch("files");
    let path = dir.join(format!("Mine.{EXTENSION}"));
    shortcuts::save(&keyboard(), &path).unwrap();
    assert_eq!(shortcuts::load(&path).unwrap().entries, keyboard());
    // Missing: the operating system's error, with the path.
    let missing = dir.join("missing");
    match shortcuts::load(&missing) {
        Err(ShortcutFileError::Io(failure)) => {
            assert_eq!(failure.kind(), std::io::ErrorKind::NotFound);
            assert_eq!(failure.path(), missing);
        }
        other => panic!("expected an I/O error, got {other:?}"),
    }
    // Into a folder that does not exist: an I/O error, nothing created.
    let nowhere = dir.join("no folder").join("Mine.orchestra-shortcuts");
    assert!(matches!(
        shortcuts::save(&keyboard(), &nowhere),
        Err(ShortcutFileError::Io(_))
    ));
    assert!(!nowhere.exists());
    let _ = fs::remove_dir_all(&dir);
}
