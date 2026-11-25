// SPDX-License-Identifier: GPL-3.0-or-later

//! The **shortcut file** (ADR-0051, SCALETTA F2.12c2): a whole keyboard layout in a file of its
//! own, to carry to another computer, share, or keep.
//!
//! # The file (SF1)
//! UTF-8 JSON, indented with two spaces, its first field the format's version:
//!
//! ```json
//! {
//!   "orchestra_shortcuts": 1,
//!   "shortcuts": [
//!     { "action": "file.new", "keys": ["Primary+n"] },
//!     { "action": "help.contents", "keys": [] }
//!   ]
//! }
//! ```
//!
//! Each entry is a settings file's shortcut entry ([`ShortcutEntry`]): an action's stable
//! identifier and its **whole list** in the settings text, with the abstract modifiers
//! (`Primary`), so the file means the same thing on every system. An empty list is "no
//! shortcut". The application writes **every** action (SF2); this module does not know the
//! actions and writes the list it is given, in its order.
//!
//! # Reading (SF4)
//! Leniently entry by entry, as the settings file (ADR-0050 DG6): an entry of the wrong shape is
//! dropped alone and counted ([`ShortcutFile::dropped`]), unknown fields are ignored. The whole
//! file fails, with a [`ShortcutFileError`], when it cannot be read, is larger than
//! [`MAX_FILE_BYTES`], is not JSON, does not start with `"orchestra_shortcuts"`, or was written by
//! a newer Orchestra (version above [`FORMAT_VERSION`]). An older version goes through the
//! migration chain (empty today: version 1 is the first; the engine is the one shared with the
//! project and settings files).
//!
//! # Writing
//! Safely (ADR-0029 S13, `crate::safe_write`): a temporary file renamed over the target, so a
//! failed export never leaves a half-written file where a good one was.

use std::fmt;
use std::fs;
use std::path::Path;

use serde::Serialize;
use serde_json::Value;

use crate::migration::{BOM, Migration, MigrationError, leading_version};
use crate::safe_write::IoFailure;
use crate::settings::ShortcutEntry;

/// The extension of a shortcut file, without the dot (SF5): `Shortcuts.orchestra-shortcuts`.
pub const EXTENSION: &str = "orchestra-shortcuts";

/// The format's version this program writes and reads up to.
pub const FORMAT_VERSION: u32 = 1;

/// The largest file read (SF4): 1 MiB. A whole keyboard takes a few kilobytes; the limit, checked
/// before reading, keeps a wrong file chosen by mistake from being read whole.
pub const MAX_FILE_BYTES: u64 = 1 << 20;

/// The key of the version.
const KEY: &str = "orchestra_shortcuts";

/// [`KEY`] in quotes, as the version scanner looks for it.
const KEY_QUOTED: &str = "\"orchestra_shortcuts\"";

/// The migration chain: `MIGRATIONS[k]` migrates version `k + 1` to `k + 2`. Empty while version
/// 1 is the only one; its length is always `FORMAT_VERSION - 1` (a unit test checks it).
const MIGRATIONS: [Migration; 0] = [];

/// A shortcut file as read.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ShortcutFile {
    /// The entries, in the file's order (AM7: the later one wins a conflict).
    pub entries: Vec<ShortcutEntry>,
    /// How many entries were dropped because of their shape (SF4), plus the keys that were not
    /// strings: what the application adds to its count of things not used (SF3).
    pub dropped: usize,
}

/// Why a shortcut file could not be read or written (SF4). The `Display` texts are English and
/// for developers; the interface maps each variant to its own translated text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShortcutFileError {
    /// The file could not be read or written.
    Io(IoFailure),
    /// Larger than [`MAX_FILE_BYTES`].
    TooLarge {
        /// Its size, or what was read of it.
        bytes: u64,
    },
    /// It does not start with `{ "orchestra_shortcuts": <version>`.
    NotShortcuts,
    /// It is not valid JSON at this place (1-based).
    Syntax {
        /// The line.
        line: usize,
        /// The column.
        column: usize,
    },
    /// It was written by a newer Orchestra.
    NewerFormat {
        /// The version it declares.
        found: u64,
        /// The newest version this program reads.
        supported: u32,
    },
    /// An older version whose migration failed.
    Migration {
        /// The version at which the migration stopped.
        from: u64,
        /// Why.
        reason: MigrationError,
    },
    /// The entries could not be encoded (not expected: they are strings).
    Encode(String),
}

impl fmt::Display for ShortcutFileError {
    /// A short English description, for developers.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(failure) => write!(f, "cannot read or write {failure}"),
            Self::TooLarge { bytes } => write!(f, "the file has {bytes} bytes, more than allowed"),
            Self::NotShortcuts => f.write_str("not an Orchestra shortcut file"),
            Self::Syntax { line, column } => {
                write!(f, "invalid JSON at line {line}, column {column}")
            }
            Self::NewerFormat { found, supported } => write!(
                f,
                "made with a newer Orchestra (format {found}; this one reads up to {supported})"
            ),
            Self::Migration { from, reason } => {
                write!(f, "cannot migrate from format {from}: {reason}")
            }
            Self::Encode(message) => write!(f, "cannot encode the shortcuts: {message}"),
        }
    }
}

impl std::error::Error for ShortcutFileError {
    /// The I/O failure or the migration's error, when there is one.
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(failure) => Some(failure),
            Self::Migration { reason, .. } => Some(reason),
            _ => None,
        }
    }
}

/// Reads a shortcut file from its bytes (see the module documentation). O(size of the file).
///
/// # Errors
/// [`ShortcutFileError`] for a file that is not a shortcut file this version can read.
pub fn from_bytes(bytes: &[u8]) -> Result<ShortcutFile, ShortcutFileError> {
    let size = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    if size > MAX_FILE_BYTES {
        return Err(ShortcutFileError::TooLarge { bytes: size });
    }
    let found =
        leading_version(bytes, KEY_QUOTED.as_bytes()).ok_or(ShortcutFileError::NotShortcuts)?;
    let current = u64::from(FORMAT_VERSION);
    if found > current {
        return Err(ShortcutFileError::NewerFormat {
            found,
            supported: FORMAT_VERSION,
        });
    }
    let body = bytes.strip_prefix(BOM).unwrap_or(bytes);
    let tree: Value = serde_json::from_slice(body).map_err(|e| ShortcutFileError::Syntax {
        line: e.line(),
        column: e.column(),
    })?;
    let tree = if found < current {
        crate::migration::run(tree, found, current, &MIGRATIONS, KEY)
            .map_err(|(from, reason)| ShortcutFileError::Migration { from, reason })?
    } else {
        tree
    };
    // A missing or misshapen list is an empty keyboard, counted as nothing dropped: there was no
    // entry to drop. The settings file's own entry reader does the rest (SF1: the same entries).
    let Some(list) = tree.get("shortcuts").and_then(Value::as_array) else {
        return Ok(ShortcutFile::default());
    };
    let mut warnings = Vec::new();
    let entries = crate::settings::read_shortcut_entries(list, "shortcuts", &mut warnings);
    Ok(ShortcutFile {
        dropped: warnings.len(),
        entries,
    })
}

/// Reads the shortcut file at `path`: opened once, its size checked from the open handle before
/// reading, the read itself capped (as the project file, `crate::project::load`). O(size).
///
/// # Errors
/// [`ShortcutFileError::Io`] if it cannot be read; otherwise as [`from_bytes`].
pub fn load(path: &Path) -> Result<ShortcutFile, ShortcutFileError> {
    use std::io::Read;
    let io_error = |e: std::io::Error| ShortcutFileError::Io(IoFailure::new(&e, path));
    let file = fs::File::open(path).map_err(io_error)?;
    let size = file.metadata().map_err(io_error)?.len();
    if size > MAX_FILE_BYTES {
        return Err(ShortcutFileError::TooLarge { bytes: size });
    }
    // At most 1 MiB: it fits a `usize` everywhere, so the fallback (no reservation) is never taken.
    let mut bytes = Vec::with_capacity(usize::try_from(size).unwrap_or(0));
    file.take(MAX_FILE_BYTES.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(io_error)?;
    from_bytes(&bytes)
}

/// The file as written: the version first, then the entries in the given order.
#[derive(Serialize)]
struct File<'a> {
    /// The version.
    orchestra_shortcuts: u32,
    /// The entries.
    shortcuts: Vec<Entry<'a>>,
}

/// One entry as written.
#[derive(Serialize)]
struct Entry<'a> {
    /// The action's identifier.
    action: &'a str,
    /// Its shortcuts' texts.
    keys: &'a [String],
}

/// Encodes `entries` (see the module documentation), with a final newline. O(their size).
///
/// # Errors
/// [`ShortcutFileError::Encode`] if `serde_json` fails (not expected for strings).
pub fn to_bytes(entries: &[ShortcutEntry]) -> Result<Vec<u8>, ShortcutFileError> {
    let file = File {
        orchestra_shortcuts: FORMAT_VERSION,
        shortcuts: entries
            .iter()
            .map(|entry| Entry {
                action: &entry.action,
                keys: &entry.keys,
            })
            .collect(),
    };
    let mut bytes =
        serde_json::to_vec_pretty(&file).map_err(|e| ShortcutFileError::Encode(e.to_string()))?;
    bytes.push(b'\n');
    Ok(bytes)
}

/// Writes `entries` to `path` safely (see the module documentation).
///
/// # Errors
/// [`ShortcutFileError::Io`] with the failing path (the file there, if any, is intact), or
/// [`ShortcutFileError::Encode`].
pub fn save(entries: &[ShortcutEntry], path: &Path) -> Result<(), ShortcutFileError> {
    let bytes = to_bytes(entries)?;
    crate::safe_write::write(path, &bytes).map_err(ShortcutFileError::Io)
}

#[cfg(test)]
mod tests {
    //! The migration chain's length; the file's cases are in `tests/shortcuts_cases.rs`.
    use super::*;

    /// One migration per older version.
    #[test]
    fn the_chain_has_one_step_per_older_version() {
        assert_eq!(
            MIGRATIONS.len(),
            usize::try_from(FORMAT_VERSION - 1).unwrap_or(usize::MAX)
        );
    }
}
