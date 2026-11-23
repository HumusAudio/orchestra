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

use serde_json::Value;

use super::LoadError;

use crate::migration::Migration;
pub use crate::migration::MigrationError;

/// The chain: `MIGRATIONS[k]` migrates version `k + 1` to `k + 2`. Empty while version 1 is the
/// only one; its length is always `FORMAT_VERSION - 1` (checked by a unit test).
pub(crate) const MIGRATIONS: [Migration; 0] = [];

/// The key of the format version, which every step updates (ADR-0029 S5).
const VERSION_KEY: &str = "orchestra_format";

/// Migrates the tree of a project file of version `from` to version `to` with `chain`, through
/// the engine shared with the settings file ([`crate::migration::run`]). O(steps × size of the
/// tree).
///
/// # Errors
/// [`LoadError::Migration`] if a step is missing from the chain, if the tree is not an object, or
/// if a step fails.
pub(crate) fn migrate_with(
    tree: Value,
    from: u64,
    to: u64,
    chain: &[Migration],
) -> Result<Value, LoadError> {
    crate::migration::run(tree, from, to, chain, VERSION_KEY)
        .map_err(|(from, reason)| LoadError::Migration { from, reason })
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
