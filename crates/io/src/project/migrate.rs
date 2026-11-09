// SPDX-License-Identifier: GPL-3.0-or-later

//! Migrations of older project files (ADR-0010 §2, ADR-0029 S11; SCALETTA F1.15c).
//!
//! # How a migration works
//! Only one typed schema exists, the current one (`crate::project::schema`). A file of an older
//! version is read into a generic JSON tree (`serde_json::Value`), passed through the chain of
//! migrations, one per version step (`v1 → v2`, `v2 → v3`, …), each a pure function on the tree,
//! and only then read into the current schema. Old versions live on as their migrations and their
//! golden files (`tests/golden/vN/`), never as old Rust types.
//!
//! The machinery sets `orchestra_format` to the new version after each step, so a migration only
//! changes what its version changed; it checks that the chain covers every step between the file's
//! version and the current one.
//!
//! # Today
//! Version 1 is the first, so the chain is empty and no file takes this path yet. The machinery is
//! tested with a test-only chain (the unit tests below), as ADR-0029 S11 asks, so that the first
//! real format change only has to add its function to [`MIGRATIONS`].
//!
//! # Cost
//! The tree path measured about 3× slower than reading the typed schema directly (the spike of
//! `spikes/fileformat/`), paid once per old file: the next save writes the current version.

use std::fmt;

use serde_json::Value;

use super::LoadError;

/// Why a file could not be migrated from one version to the next (ADR-0029 S11): typed, so that
/// the interface can map each case to a Fluent key (ADR-0008) and the tests can match it, instead
/// of a free English text.
///
/// The fields name places in the file as `&'static str` paths written by the migration itself
/// (for example `"written_by"` or `"tracks[].voices"`): a migration knows statically what the
/// old version guarantees, so it never needs to build a path at run time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MigrationError {
    /// The chain has no migration from this version (a bug of the program, never of the file:
    /// a unit test checks that the chain covers every step).
    MissingStep,
    /// The file, or the tree a step produced, is not a JSON object.
    NotAnObject,
    /// A field that the old version guarantees is missing.
    MissingField(&'static str),
    /// A field has a value of a type the old version does not allow.
    WrongType(&'static str),
}

impl fmt::Display for MigrationError {
    /// A short English description, for developers (the interface maps the variants to Fluent
    /// keys).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingStep => f.write_str("no migration from this version"),
            Self::NotAnObject => f.write_str("the file is not a JSON object"),
            Self::MissingField(path) => write!(f, "the field `{path}` is missing"),
            Self::WrongType(path) => write!(f, "the field `{path}` has the wrong type"),
        }
    }
}

impl std::error::Error for MigrationError {}

/// One migration: the tree of a file of version `n` → the tree of version `n + 1`. A failure
/// means the file does not have what version `n` guarantees.
pub(crate) type Migration = fn(Value) -> Result<Value, MigrationError>;

/// The chain: `MIGRATIONS[k]` migrates version `k + 1` to `k + 2`. Empty while version 1 is the
/// only one; its length is always `FORMAT_VERSION - 1` (checked by a unit test).
pub(crate) const MIGRATIONS: [Migration; 0] = [];

/// Migrates the tree of a file of version `from` to version `to` with `chain` (see the module
/// documentation). O(steps × size of the tree).
///
/// # Errors
/// [`LoadError::Migration`] if a step is missing from the chain, if the tree is not an object, or
/// if a step fails.
pub(crate) fn migrate_with(
    mut tree: Value,
    from: u64,
    to: u64,
    chain: &[Migration],
) -> Result<Value, LoadError> {
    let mut version = from;
    while version < to {
        // The version of this step, copied: the loop variable moves on after the step.
        let at = version;
        let fail = move |reason: MigrationError| LoadError::Migration { from: at, reason };
        let step = usize::try_from(version.saturating_sub(1))
            .ok()
            .and_then(|k| chain.get(k))
            .ok_or(fail(MigrationError::MissingStep))?;
        tree = step(tree).map_err(fail)?;
        version = version.saturating_add(1);
        match &mut tree {
            Value::Object(map) => {
                map.insert("orchestra_format".to_owned(), Value::from(version));
            }
            _ => return Err(fail(MigrationError::NotAnObject)),
        }
    }
    Ok(tree)
}

#[cfg(test)]
mod tests {
    //! The machinery, with a test-only chain of two steps (ADR-0029 S11).
    use serde_json::json;

    use super::*;

    /// A test-only `v1 → v2`: renames a field, as a real format change might.
    fn rename_writer(mut tree: Value) -> Result<Value, MigrationError> {
        let map = tree.as_object_mut().ok_or(MigrationError::NotAnObject)?;
        let writer = map
            .remove("written_by")
            .ok_or(MigrationError::MissingField("written_by"))?;
        map.insert("writer".to_owned(), writer);
        Ok(tree)
    }

    /// A test-only `v2 → v3`: adds a field with a default.
    fn add_title(mut tree: Value) -> Result<Value, MigrationError> {
        let map = tree.as_object_mut().ok_or(MigrationError::NotAnObject)?;
        map.insert("title".to_owned(), json!(""));
        Ok(tree)
    }

    /// The chain of the tests.
    const CHAIN: [Migration; 2] = [rename_writer, add_title];

    /// The real chain covers every version step up to the current one.
    #[test]
    fn the_chain_covers_every_step() {
        assert_eq!(
            u64::try_from(MIGRATIONS.len()).ok(),
            u64::from(super::super::FORMAT_VERSION).checked_sub(1)
        );
    }

    /// Steps run in order, from the file's version only, and the version follows.
    #[test]
    fn steps_run_in_order() {
        let v1 = json!({"orchestra_format": 1, "written_by": "Orchestra 0.0.0"});
        let v3 = migrate_with(v1.clone(), 1, 3, &CHAIN).ok();
        assert_eq!(
            v3,
            Some(json!({"orchestra_format": 3, "writer": "Orchestra 0.0.0", "title": ""}))
        );
        let v2 = json!({"orchestra_format": 2, "writer": "x"});
        assert_eq!(
            migrate_with(v2, 2, 3, &CHAIN).ok(),
            Some(json!({"orchestra_format": 3, "writer": "x", "title": ""}))
        );
        // Nothing to do for a file already current.
        assert_eq!(migrate_with(v1.clone(), 1, 1, &CHAIN).ok(), Some(v1));
    }

    /// A failing step and a missing step are typed errors naming the version.
    #[test]
    fn failures_name_the_version() {
        let broken = json!({"orchestra_format": 1});
        assert!(matches!(
            migrate_with(broken, 1, 3, &CHAIN),
            Err(LoadError::Migration {
                from: 1,
                reason: MigrationError::MissingField("written_by")
            })
        ));
        let v1 = json!({"orchestra_format": 1, "written_by": "x"});
        assert!(matches!(
            migrate_with(v1, 1, 4, &CHAIN),
            Err(LoadError::Migration {
                from: 3,
                reason: MigrationError::MissingStep
            })
        ));
    }
}
