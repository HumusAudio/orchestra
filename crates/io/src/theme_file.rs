// SPDX-License-Identifier: GPL-3.0-or-later

//! The **theme file** (ADR-0052, SCALETTA F2.12b2b): one custom theme in a file of its own, to
//! carry to another computer, give to someone, or keep aside.
//!
//! # The file (TF1)
//! UTF-8 JSON, indented with two spaces, its first field the format's version:
//!
//! ```json
//! {
//!   "orchestra_theme": 1,
//!   "theme": {
//!     "name": "Night",
//!     "base": "studio",
//!     "light": [{ "color": "accent", "value": "#3366ccff" }],
//!     "dark": [{ "color": "accent", "value": "#5b9bf0ff" }]
//!   }
//! }
//! ```
//!
//! `theme` is exactly a settings file's custom theme ([`CustomTheme`]), read and written by the
//! settings file's own code, so a theme means the same thing in both files. This module does not
//! know the built-in pairs or the tokens: making an unknown base Studio, keeping unknown tokens
//! and aligning the halves' text, lines and density are the application's (TF4).
//!
//! # Reading (TF4, TF5)
//! Leniently override by override, as the settings file (ADR-0050 DG6): an override of the wrong
//! shape is dropped alone and counted ([`ThemeFile::dropped`]), unknown fields are ignored (and
//! counted too: they are entries the file holds and this Orchestra does not use). A missing name
//! or base is left empty, for the application to fill (the file's name; Studio). The whole file
//! fails, with a [`ThemeFileError`], when it cannot be read, is larger than [`MAX_FILE_BYTES`], is
//! not JSON, does not start with `"orchestra_theme"`, holds no `theme` object, or was written by a
//! newer Orchestra (version above [`FORMAT_VERSION`]). An older version goes through the migration
//! chain (empty today: version 1 is the first; the engine is the one shared with the project,
//! settings and shortcut files).
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
use crate::settings::CustomTheme;

/// The extension of a theme file, without the dot (TF1): `Night.orchestra-theme`.
pub const EXTENSION: &str = "orchestra-theme";

/// The format's version this program writes and reads up to.
pub const FORMAT_VERSION: u32 = 1;

/// The largest file read (TF5): 1 MiB. A theme takes a few kilobytes; the limit, checked before
/// reading, keeps a wrong file chosen by mistake from being read whole.
pub const MAX_FILE_BYTES: u64 = 1 << 20;

/// The key of the version.
const KEY: &str = "orchestra_theme";

/// [`KEY`] in quotes, as the version scanner looks for it.
const KEY_QUOTED: &str = "\"orchestra_theme\"";

/// The migration chain: `MIGRATIONS[k]` migrates version `k + 1` to `k + 2`. Empty while version
/// 1 is the only one; its length is always `FORMAT_VERSION - 1` (a unit test checks it).
const MIGRATIONS: [Migration; 0] = [];

/// A theme file as read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThemeFile {
    /// The theme. Its `name` or `base` is empty when the file had none (TF3, TF4: the
    /// application gives the file's name, or Studio).
    pub theme: CustomTheme,
    /// How many entries were dropped or ignored: overrides of the wrong shape, unknown fields,
    /// a missing name or base (TF4: part of what the report counts as not used).
    pub dropped: usize,
}

/// Why a theme file could not be read or written (TF5). The `Display` texts are English and for
/// developers; the interface maps each variant to its own translated text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThemeFileError {
    /// The file could not be read or written.
    Io(IoFailure),
    /// Larger than [`MAX_FILE_BYTES`].
    TooLarge {
        /// Its size, or what was read of it.
        bytes: u64,
    },
    /// It does not start with `{ "orchestra_theme": <version>`, or holds no `theme` object.
    NotTheme,
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
    /// The theme could not be encoded (not expected: strings and small numbers).
    Encode(String),
}

impl fmt::Display for ThemeFileError {
    /// A short English description, for developers.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(failure) => write!(f, "cannot read or write {failure}"),
            Self::TooLarge { bytes } => write!(f, "the file has {bytes} bytes, more than allowed"),
            Self::NotTheme => f.write_str("not an Orchestra theme file"),
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
            Self::Encode(message) => write!(f, "cannot encode the theme: {message}"),
        }
    }
}

impl std::error::Error for ThemeFileError {
    /// The I/O failure or the migration's error, when there is one.
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(failure) => Some(failure),
            Self::Migration { reason, .. } => Some(reason),
            _ => None,
        }
    }
}

/// Reads a theme file from its bytes (see the module documentation). O(size of the file).
///
/// # Errors
/// [`ThemeFileError`] for a file that is not a theme file this version can read.
pub fn from_bytes(bytes: &[u8]) -> Result<ThemeFile, ThemeFileError> {
    let size = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    if size > MAX_FILE_BYTES {
        return Err(ThemeFileError::TooLarge { bytes: size });
    }
    let found = leading_version(bytes, KEY_QUOTED.as_bytes()).ok_or(ThemeFileError::NotTheme)?;
    let current = u64::from(FORMAT_VERSION);
    if found > current {
        return Err(ThemeFileError::NewerFormat {
            found,
            supported: FORMAT_VERSION,
        });
    }
    let body = bytes.strip_prefix(BOM).unwrap_or(bytes);
    let tree: Value = serde_json::from_slice(body).map_err(|e| ThemeFileError::Syntax {
        line: e.line(),
        column: e.column(),
    })?;
    let tree = if found < current {
        crate::migration::run(tree, found, current, &MIGRATIONS, KEY)
            .map_err(|(from, reason)| ThemeFileError::Migration { from, reason })?
    } else {
        tree
    };
    // Without a theme object there is nothing to import: not a theme file (TF5).
    let map = tree
        .get("theme")
        .and_then(Value::as_object)
        .ok_or(ThemeFileError::NotTheme)?;
    let mut warnings = Vec::new();
    let (name, base, light, dark) = crate::settings::read_custom_theme(map, "theme", &mut warnings);
    Ok(ThemeFile {
        dropped: warnings.len(),
        theme: CustomTheme {
            name: name.unwrap_or_default(),
            base: base.unwrap_or_default(),
            light,
            dark,
        },
    })
}

/// Reads the theme file at `path`: opened once, its size checked from the open handle before
/// reading, the read itself capped (as the shortcut file). O(size).
///
/// # Errors
/// [`ThemeFileError::Io`] if it cannot be read; otherwise as [`from_bytes`].
pub fn load(path: &Path) -> Result<ThemeFile, ThemeFileError> {
    use std::io::Read;
    let io_error = |e: std::io::Error| ThemeFileError::Io(IoFailure::new(&e, path));
    let file = fs::File::open(path).map_err(io_error)?;
    let size = file.metadata().map_err(io_error)?.len();
    if size > MAX_FILE_BYTES {
        return Err(ThemeFileError::TooLarge { bytes: size });
    }
    // At most 1 MiB: it fits a `usize` everywhere, so the fallback (no reservation) is never taken.
    let mut bytes = Vec::with_capacity(usize::try_from(size).unwrap_or(0));
    file.take(MAX_FILE_BYTES.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(io_error)?;
    from_bytes(&bytes)
}

/// The file as written: the version first, then the theme.
#[derive(Serialize)]
struct File<T: Serialize> {
    /// The version.
    orchestra_theme: u32,
    /// The theme, as the settings file writes it.
    theme: T,
}

/// Encodes `theme` (see the module documentation), with a final newline. O(its size).
///
/// # Errors
/// [`ThemeFileError::Encode`] if `serde_json` fails (not expected for these values).
pub fn to_bytes(theme: &CustomTheme) -> Result<Vec<u8>, ThemeFileError> {
    let file = File {
        orchestra_theme: FORMAT_VERSION,
        theme: crate::settings::written_custom_theme(theme),
    };
    let mut bytes =
        serde_json::to_vec_pretty(&file).map_err(|e| ThemeFileError::Encode(e.to_string()))?;
    bytes.push(b'\n');
    Ok(bytes)
}

/// Writes `theme` to `path` safely (see the module documentation).
///
/// # Errors
/// [`ThemeFileError::Io`] with the failing path (the file there, if any, is intact), or
/// [`ThemeFileError::Encode`].
pub fn save(theme: &CustomTheme, path: &Path) -> Result<(), ThemeFileError> {
    let bytes = to_bytes(theme)?;
    crate::safe_write::write(path, &bytes).map_err(ThemeFileError::Io)
}

#[cfg(test)]
mod tests {
    //! The migration chain's length; the file's cases are in `tests/theme_file_cases.rs`.
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
