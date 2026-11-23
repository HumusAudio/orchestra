// SPDX-License-Identifier: GPL-3.0-or-later

//! The engine of the migrations, shared by the two versioned files of the crate: the project file
//! (ADR-0029 S11, `crate::project::migrate`) and the settings file (ADR-0050 DG6,
//! `crate::settings`).
//!
//! # How a migration works
//! Each file has one typed reader only, for its current version. A file of an older version is read
//! into a generic JSON tree (`serde_json::Value`) and passed through a **chain** of migrations, one
//! per version step (`v1 → v2`, `v2 → v3`, …), each a pure function on the tree; only then does
//! the current reader see it. Old versions live on as their migrations and their golden files,
//! never as old Rust types (ADR-0010 §2).
//!
//! The engine writes the new version under the file's own key (`orchestra_format`,
//! `orchestra_settings`) after each step, so a migration only changes what its version changed,
//! and it checks that the chain covers every step between the file's version and the target.
//!
//! # Why shared
//! The two files follow the same policy (CLAUDE.md §9); one engine means one place where "a step is
//! missing" or "the version is not updated" can go wrong, tested once here and by each file's own
//! test chain.

use std::fmt;

use serde_json::Value;

/// The UTF-8 byte order mark, which some editors add and the readers ignore (ADR-0029 S3).
pub(crate) const BOM: &[u8] = b"\xEF\xBB\xBF";

/// The version a versioned file declares in its first bytes: an optional BOM, whitespace, `{`,
/// the quoted `key` (`b"\"orchestra_format\""`), `:`, a positive integer that fits 64 bits.
/// `None` for anything else.
///
/// # Why a scanner of its own
/// The version decides how the rest is read (straight into the current reader, through
/// migrations, or not at all), so it must be known before any parsing that could fail on a field
/// of another version. The writers always put the version first; this scanner reads exactly that
/// prefix and nothing more.
///
/// # Complexity
/// O(prefix): at most a few dozen bytes are looked at.
pub(crate) fn leading_version(bytes: &[u8], key: &[u8]) -> Option<u64> {
    let rest = bytes.strip_prefix(BOM).unwrap_or(bytes);
    let rest = after_whitespace(rest);
    let rest = rest.strip_prefix(b"{")?;
    let rest = after_whitespace(rest);
    let rest = rest.strip_prefix(key)?;
    let rest = after_whitespace(rest);
    let rest = rest.strip_prefix(b":")?;
    let rest = after_whitespace(rest);
    let digits = rest.iter().take_while(|c| c.is_ascii_digit()).count();
    let mut value: u64 = 0;
    for &d in rest.get(..digits).unwrap_or(&[]) {
        value = value
            .checked_mul(10)
            .and_then(|v| v.checked_add(u64::from(d.wrapping_sub(b'0'))))?;
    }
    (digits > 0 && value > 0).then_some(value)
}

/// The bytes after the leading ASCII whitespace (JSON's whitespace is a subset). O(whitespace).
fn after_whitespace(bytes: &[u8]) -> &[u8] {
    let n = bytes.iter().take_while(|c| c.is_ascii_whitespace()).count();
    bytes.get(n..).unwrap_or(&[])
}

/// Why a file could not be migrated from one version to the next: typed, so that the interface
/// can map each case to a Fluent key (ADR-0008) and the tests can match it, instead of a free
/// English text.
///
/// The fields name places in the file as `&'static str` paths written by the migration itself
/// (for example `"written_by"` or `"tracks[].voices"`): a migration knows statically what the
/// old version guarantees, so it never needs to build a path at run time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MigrationError {
    /// The chain has no migration from this version (a bug of the program, never of the file:
    /// each file's unit test checks that its chain covers every step).
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

/// Migrates `tree`, of version `from`, to version `to` with `chain` (`chain[k]` migrates version
/// `k + 1` to `k + 2`), writing each new version under `version_key`.
///
/// O(steps × size of the tree).
///
/// # Errors
/// The version at which the migration stopped, with the reason: a step missing from the chain,
/// a tree that is not an object, or a failing step.
pub(crate) fn run(
    mut tree: Value,
    from: u64,
    to: u64,
    chain: &[Migration],
    version_key: &str,
) -> Result<Value, (u64, MigrationError)> {
    let mut version = from;
    while version < to {
        // The version of this step, copied: the loop variable moves on after the step.
        let at = version;
        let step = usize::try_from(version.saturating_sub(1))
            .ok()
            .and_then(|k| chain.get(k))
            .ok_or((at, MigrationError::MissingStep))?;
        tree = step(tree).map_err(|reason| (at, reason))?;
        version = version.saturating_add(1);
        match &mut tree {
            Value::Object(map) => {
                map.insert(version_key.to_owned(), Value::from(version));
            }
            _ => return Err((at, MigrationError::NotAnObject)),
        }
    }
    Ok(tree)
}
