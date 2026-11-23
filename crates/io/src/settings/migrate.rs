// SPDX-License-Identifier: GPL-3.0-or-later

//! Migrations of older settings files (ADR-0050 DG6, CLAUDE.md §9), through the engine shared with
//! the project file (`crate::migration`).
//!
//! # Today
//! Version 1 is the first, so the chain is empty and no file takes this path yet. The machinery is
//! tested with a test-only chain below, so that the first real change of the format only has to
//! add its function to [`MIGRATIONS`] and its golden files to `tests/golden/settings-v1/`'s
//! successors.
//!
//! # A failed migration
//! Unlike the project file, a settings file whose migration fails does not stop anything: the
//! caller treats it as broken (moved aside, the defaults used; `super::Broken::Migration`).

use serde_json::Value;

use super::read::KEY;
use crate::migration::{Migration, MigrationError};

/// The chain: `MIGRATIONS[k]` migrates version `k + 1` to `k + 2`. Empty while version 1 is the
/// only one; its length is always `FORMAT_VERSION - 1` (checked by a unit test).
const MIGRATIONS: [Migration; 0] = [];

/// Migrates the tree of a settings file of version `from` to `to` with the real chain.
/// O(steps × size of the tree).
///
/// # Errors
/// The version where the migration stopped, and why.
pub(crate) fn run(tree: Value, from: u64, to: u64) -> Result<Value, (u64, MigrationError)> {
    crate::migration::run(tree, from, to, &MIGRATIONS, KEY)
}

#[cfg(test)]
mod tests {
    //! The machinery with a test-only chain of two steps, and the real chain's length.
    use serde_json::json;

    use super::*;

    /// A test-only `v1 → v2`: renames `caret_blink`, as a real change might.
    fn rename_blink(mut tree: Value) -> Result<Value, MigrationError> {
        let map = tree.as_object_mut().ok_or(MigrationError::NotAnObject)?;
        let blink = map
            .remove("caret_blink")
            .ok_or(MigrationError::MissingField("caret_blink"))?;
        map.insert("blink".to_owned(), blink);
        Ok(tree)
    }

    /// A test-only `v2 → v3`: adds a field with its default.
    fn add_scale(mut tree: Value) -> Result<Value, MigrationError> {
        let map = tree.as_object_mut().ok_or(MigrationError::NotAnObject)?;
        map.insert("scale".to_owned(), json!(100));
        Ok(tree)
    }

    /// The chain of the tests.
    const CHAIN: [Migration; 2] = [rename_blink, add_scale];

    /// The real chain covers every version step up to the current one.
    #[test]
    fn the_chain_covers_every_step() {
        assert_eq!(
            u64::try_from(MIGRATIONS.len()).ok(),
            u64::from(super::super::FORMAT_VERSION).checked_sub(1)
        );
    }

    /// Steps run in order, the settings' own version key follows, and a missing field stops the
    /// chain at its version.
    #[test]
    fn steps_run_in_order() {
        let v1 = json!({"orchestra_settings": 1, "caret_blink": false});
        assert_eq!(
            crate::migration::run(v1, 1, 3, &CHAIN, KEY).ok(),
            Some(json!({"orchestra_settings": 3, "blink": false, "scale": 100}))
        );
        let broken = json!({"orchestra_settings": 1});
        assert_eq!(
            crate::migration::run(broken, 1, 3, &CHAIN, KEY).err(),
            Some((1, MigrationError::MissingField("caret_blink")))
        );
        let v1 = json!({"orchestra_settings": 1, "caret_blink": true});
        assert_eq!(
            crate::migration::run(v1, 1, 4, &CHAIN, KEY).err(),
            Some((3, MigrationError::MissingStep))
        );
    }
}
