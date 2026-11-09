// SPDX-License-Identifier: GPL-3.0-or-later

//! The golden files: the compatibility contract of the project file (ADR-0010 §3, ADR-0029 S12;
//! SCALETTA F1.15c).
//!
//! `tests/golden/vN/` holds files of version N, written **once** and never regenerated. For every
//! version this test checks that every file still loads; for version 1 it also checks each file
//! against the document built by `common/golden_docs.rs`, IDs included, and, while version 1 is
//! the current one, that saving that document gives the file's very bytes.
//!
//! When the format changes (a version 2), the version-1 files stay as they are and must still
//! load through the migrations, giving the same documents; the new files go in `v2/`.
//!
//! # Writing the files
//! `write_missing_golden_files` (ignored, run by hand with `cargo test -p orchestra-io --test
//! golden -- --ignored`) writes the files of version 1 that do not exist yet and never touches an
//! existing one. Each new file must then be reviewed by eye before it is committed.
//!
//! # Robustness (ADR-0029 S12)
//! Every truncation of every golden file is refused; random JSON values put anywhere in the
//! "everything" file never panic.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic,
    reason = "test helpers: a failed unwrap, index or panic fails the test"
)]

#[path = "common/everything.rs"]
mod everything;
#[path = "common/golden_docs.rs"]
mod golden_docs;

use std::path::PathBuf;

use orchestra_io::{FORMAT_VERSION, from_bytes, to_bytes};
use proptest::prelude::*;
use serde_json::Value;

/// The directory of the golden files of a version.
fn dir(version: u32) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
        .join(format!("v{version}"))
}

/// The bytes of a golden file of version 1.
fn golden(name: &str) -> Vec<u8> {
    let path = dir(1).join(format!("{name}.orchestra"));
    std::fs::read(&path).unwrap_or_else(|e| {
        panic!(
            "{} is missing ({e}): write it with `cargo test -p orchestra-io --test golden -- \
             --ignored`, then review and commit it",
            path.display()
        )
    })
}

/// Every golden file of version 1 loads into the document its code builds, IDs and counter
/// included; while version 1 is current, saving that document gives the file's bytes.
#[test]
fn version_1_files_match_their_documents() {
    for (name, build) in golden_docs::GOLDEN_V1 {
        let bytes = golden(name);
        let loaded = from_bytes(&bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
        let built = build();
        assert_eq!(
            loaded.to_parts(),
            built.to_parts(),
            "{name}: the document differs"
        );
        if FORMAT_VERSION == 1 {
            assert!(
                to_bytes(&built).unwrap() == bytes,
                "{name}: saving the document no longer gives the golden file"
            );
        }
    }
}

/// Every file of every version directory loads, and version 1 has no file without its document.
#[test]
fn every_golden_file_loads() {
    let root = dir(1).parent().unwrap().to_path_buf();
    let mut versions = 0;
    for entry in std::fs::read_dir(&root).unwrap() {
        let version_dir = entry.unwrap().path();
        // Only the version directories (`v1`, `v2`…): anything else (an editor's or a tool's
        // hidden folder) is not a golden file.
        let is_version = version_dir.is_dir()
            && version_dir
                .file_name()
                .and_then(|n| n.to_str())
                .and_then(|n| n.strip_prefix('v'))
                .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()));
        if !is_version {
            continue;
        }
        versions += 1;
        for file in std::fs::read_dir(&version_dir).unwrap() {
            let path = file.unwrap().path();
            if !path.is_file() || path.extension().is_none_or(|e| e != "orchestra") {
                continue;
            }
            let bytes = std::fs::read(&path).unwrap();
            assert!(
                from_bytes(&bytes).is_ok(),
                "{} does not load",
                path.display()
            );
            if version_dir.ends_with("v1") {
                let stem = path.file_stem().unwrap().to_str().unwrap();
                assert!(
                    golden_docs::GOLDEN_V1.iter().any(|(n, _)| *n == stem),
                    "{} has no document in golden_docs.rs",
                    path.display()
                );
            }
        }
    }
    assert!(versions >= 1);
}

/// Every truncation of every golden file is refused, none panics.
#[test]
fn every_truncation_of_every_golden_file_is_refused() {
    for (name, _) in golden_docs::GOLDEN_V1 {
        let bytes = golden(name);
        for len in 0..bytes.len().saturating_sub(1) {
            assert!(
                from_bytes(&bytes[..len]).is_err(),
                "{name}: {len} bytes loaded"
            );
        }
    }
}

/// Writes the golden files of version 1 that do not exist yet; never overwrites one (ADR-0010
/// §3: they are written once). Ignored: run by hand, then review the new files.
#[test]
#[ignore = "writes the golden files; run by hand, once, then review and commit them"]
fn write_missing_golden_files() {
    let dir = dir(1);
    std::fs::create_dir_all(&dir).unwrap();
    for (name, build) in golden_docs::GOLDEN_V1 {
        let path = dir.join(format!("{name}.orchestra"));
        if !path.exists() {
            std::fs::write(&path, to_bytes(&build()).unwrap()).unwrap();
        }
    }
}

/// A random JSON value, a few levels deep.
fn json() -> impl Strategy<Value = Value> {
    let leaf = prop_oneof![
        Just(Value::Null),
        any::<bool>().prop_map(Value::from),
        any::<i64>().prop_map(Value::from),
        any::<f64>().prop_map(Value::from),
        "[a-z0-9/#\\[\\]._-]{0,8}".prop_map(Value::from),
    ];
    leaf.prop_recursive(3, 16, 4, |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 0..4).prop_map(Value::from),
            prop::collection::btree_map("[a-z_]{1,8}", inner, 0..4)
                .prop_map(|m| Value::Object(m.into_iter().collect())),
        ]
    })
}

/// Replaces the value at a random place of `tree` (chosen by `path`, one index per level) with
/// `value`; a path that ends early replaces the value where it ends.
fn replace(tree: &mut Value, path: &[usize], value: Value) {
    let child = match (tree, path.split_first()) {
        (Value::Object(map), Some((&i, _))) if !map.is_empty() => {
            let key = map.keys().nth(i % map.len()).unwrap().clone();
            map.get_mut(&key).unwrap()
        }
        (Value::Array(items), Some((&i, _))) if !items.is_empty() => {
            let n = items.len();
            &mut items[i % n]
        }
        (tree, _) => {
            *tree = value;
            return;
        }
    };
    replace(child, &path[1..], value);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// A random JSON value anywhere in the "everything" file never panics; the version stays
    /// first, so the reader goes past the header and into the schema.
    #[test]
    fn random_json_values_never_panic(
        path in prop::collection::vec(any::<usize>(), 0..8),
        value in json(),
    ) {
        let bytes = golden("everything");
        let mut tree: Value = serde_json::from_slice(&bytes).unwrap();
        let map = tree.as_object_mut().unwrap();
        map.remove("orchestra_format");
        replace(&mut tree, &path, value);
        // `serde_json`'s maps sort their keys, so the version is put back first by hand.
        let rest = serde_json::to_string(&tree).unwrap();
        let text = match rest.strip_prefix('{') {
            Some(body) if body != "}" => format!("{{\"orchestra_format\":1,{body}"),
            _ => "{\"orchestra_format\":1}".to_owned(),
        };
        let _ = from_bytes(text.as_bytes());
    }
}
