// SPDX-License-Identifier: GPL-3.0-or-later

//! The project file (ADR-0010, ADR-0029; SCALETTA F1.15b part 2): saving a [`Document`] to a
//! `.orchestra` file and loading it back.
//!
//! # The format, in short
//! Version 1 is one plain UTF-8 JSON file (ADR-0029 S1, ADR-0029 S2), written deterministically
//! with one record per line (ADR-0029 S3, ADR-0029 I9). Its first member is `"orchestra_format": 1`
//! (ADR-0029 S5); bars, events and marks are named by their saved IDs (ADR-0029 S7, ADR-0029 I6);
//! exact values are canonical strings (ADR-0029 S6, ADR-0029 I4, ADR-0029 I5). The schema is
//! `schema`, separate from the model's memory layout (ADR-0029 S4); `convert` maps it to and from
//! the model's `DocumentParts`, and `Document::from_parts` checks the result as the builder checks
//! what it creates.
//!
//! # Loading, step by step (`from_bytes`)
//! 1. The size: more than [`MAX_FILE_BYTES`] is refused before anything is read (ADR-0029 S9).
//! 2. The version, from the first bytes (`header`): newer → [`LoadError::NewerFormat`], a
//!    compressed container too; not a project → [`LoadError::NotAProject`].
//! 3. The JSON into the version-1 schema, strictly: unknown fields, wrong types, non-canonical
//!    values are refused with their line and column (ADR-0029 S8).
//! 4. The schema into `DocumentParts` (`convert`), then `Document::from_parts`.
//!
//! Nothing is ever partially loaded: every step either succeeds or the whole load fails. A file
//! of an older version takes a slower path at step 3: the generic JSON tree, the chain of
//! migrations (`migrate`), then the current schema (ADR-0029 S11). Version 1 is the first, so the
//! chain is empty for now.
//!
//! # Saving (`save`)
//! The bytes are written to a temporary file in the target's directory, synced, then renamed over
//! the target (ADR-0029 S13): a failed save leaves the previous file intact.

mod codec;
mod convert;
mod header;
mod migrate;
mod schema;
mod writer;

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use orchestra_core_model::{Document, DocumentError};
use serde::Serialize;

pub use migrate::MigrationError;

/// The format version this program writes and the newest it reads (ADR-0010 §1).
pub const FORMAT_VERSION: u32 = 1;

/// The largest file the reader accepts, in bytes (1 GiB, ADR-0029 S9): a protection against
/// corrupt or hostile files, about fifty times the heavy test score (1 000 bars × 30 staves).
pub const MAX_FILE_BYTES: u64 = 1 << 30;

/// Why a project file could not be loaded (ADR-0029 S8).
///
/// The `Display` texts are English and meant for developers; the interface maps each variant to
/// a Fluent key, like every other user-visible text (CLAUDE.md §7).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadError {
    /// The file could not be read.
    Io {
        /// What went wrong.
        kind: io::ErrorKind,
        /// The operating system's error code, if the error came from it (`errno` on Unix,
        /// `GetLastError` on Windows): `io::Error::from_raw_os_error` turns it back into the
        /// system's own message, which the kind alone loses.
        os_code: Option<i32>,
        /// The file.
        path: PathBuf,
    },
    /// The file is larger than [`MAX_FILE_BYTES`].
    TooLarge {
        /// Its size in bytes.
        bytes: u64,
    },
    /// The file does not start like a project file.
    NotAProject,
    /// The file was made with a newer Orchestra.
    NewerFormat {
        /// The version it declares; `None` for a compressed container, which only later versions
        /// write.
        found: Option<u64>,
        /// The newest version this program reads.
        supported: u32,
    },
    /// The file is not valid JSON at this place (1-based).
    Syntax {
        /// The line.
        line: usize,
        /// The column.
        column: usize,
    },
    /// The JSON does not follow the format: an unknown or missing field, a wrong type, a value
    /// not in canonical form, an event both chord and rest… With its place when the JSON reader
    /// knows it; otherwise the message names the object by its ID.
    Schema {
        /// The line, if known.
        line: Option<usize>,
        /// The column, if known.
        column: Option<usize>,
        /// What is wrong.
        message: String,
    },
    /// A file of an older version could not be migrated to the current one (ADR-0029 S11).
    Migration {
        /// The version of the step that failed.
        from: u64,
        /// What went wrong.
        reason: MigrationError,
    },
    /// The document described is not valid (the builder's and `validate`'s errors).
    Document(DocumentError),
}

impl fmt::Display for LoadError {
    /// A short English description, for developers (see the type's docs).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io {
                kind,
                os_code,
                path,
            } => {
                write!(f, "cannot read {}: ", path.display())?;
                crate::safe_write::describe(f, *kind, *os_code)
            }
            Self::TooLarge { bytes } => write!(f, "the file has {bytes} bytes, more than allowed"),
            Self::NotAProject => f.write_str("not an Orchestra project file"),
            Self::NewerFormat {
                found: Some(found),
                supported,
            } => write!(
                f,
                "made with a newer Orchestra (format {found}; this one reads up to {supported})"
            ),
            Self::NewerFormat {
                found: None,
                supported,
            } => write!(
                f,
                "made with a newer Orchestra (a compressed project; this one reads up to format \
                 {supported})"
            ),
            Self::Syntax { line, column } => {
                write!(f, "invalid JSON at line {line}, column {column}")
            }
            Self::Schema {
                line: Some(line),
                column: Some(column),
                message,
            } => write!(f, "{message} (line {line}, column {column})"),
            Self::Schema { message, .. } => f.write_str(message),
            Self::Migration { from, reason } => {
                write!(f, "cannot migrate from format {from}: {reason}")
            }
            Self::Document(e) => write!(f, "invalid document: {e}"),
        }
    }
}

impl std::error::Error for LoadError {
    /// The document's error, when the file was read but described an invalid document, or the
    /// migration's; the other variants carry no underlying error (the I/O error is kept as plain data, see
    /// `os_code`).
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Document(e) => Some(e),
            Self::Migration { reason, .. } => Some(reason),
            _ => None,
        }
    }
}

/// Why a project file could not be saved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveError {
    /// The file could not be written; the previous file, if any, is intact.
    Io {
        /// What went wrong.
        kind: io::ErrorKind,
        /// The operating system's error code, if the error came from it (`errno` on Unix,
        /// `GetLastError` on Windows): `io::Error::from_raw_os_error` turns it back into the
        /// system's own message, which the kind alone loses.
        os_code: Option<i32>,
        /// The file.
        path: PathBuf,
    },
    /// The document could not be encoded. A valid document always can; this reports, instead of
    /// a panic, the impossible case of a broken model invariant.
    Encode(String),
}

impl fmt::Display for SaveError {
    /// A short English description, for developers.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io {
                kind,
                os_code,
                path,
            } => {
                write!(f, "cannot write {}: ", path.display())?;
                crate::safe_write::describe(f, *kind, *os_code)
            }
            Self::Encode(message) => write!(f, "cannot encode the document: {message}"),
        }
    }
}

impl std::error::Error for SaveError {}

/// The bytes of a document's project file (ADR-0029 S3): deterministic, LF line endings, no BOM,
/// a final newline.
///
/// # Errors
/// [`SaveError::Encode`] only if a model invariant is broken (see the variant).
///
/// # Complexity
/// O(size of the document): one copy of its saved data (`to_parts`), one conversion, one write.
pub fn to_bytes(doc: &Document) -> Result<Vec<u8>, SaveError> {
    let file = convert::to_file(&doc.to_parts())?;
    let mut bytes = Vec::new();
    let mut serializer =
        serde_json::Serializer::with_formatter(&mut bytes, writer::RecordFormatter::default());
    file.serialize(&mut serializer)
        .map_err(|e| SaveError::Encode(e.to_string()))?;
    bytes.push(b'\n');
    Ok(bytes)
}

/// The document of a project file's bytes (see the module documentation for the steps).
///
/// # Errors
/// See [`LoadError`].
///
/// # Complexity
/// O(size of the file) to parse and convert, plus `Document::from_parts` (about the cost of
/// building the document with the builder).
pub fn from_bytes(bytes: &[u8]) -> Result<Document, LoadError> {
    let size = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    if size > MAX_FILE_BYTES {
        return Err(LoadError::TooLarge { bytes: size });
    }
    let found = header::version(bytes)?;
    if found > u64::from(FORMAT_VERSION) {
        return Err(LoadError::NewerFormat {
            found: Some(found),
            supported: FORMAT_VERSION,
        });
    }
    let body = bytes.strip_prefix(header::BOM).unwrap_or(bytes);
    let file: schema::FileV1 = if found == u64::from(FORMAT_VERSION) {
        // The fast path: the current version, straight into the typed schema.
        serde_json::from_slice(body).map_err(json_error)?
    } else {
        // An older version: the generic tree, its migrations, then the typed schema (ADR-0029 S11).
        // The reader's line and column are those of the old file, so the schema errors after the
        // migration have none.
        let tree: serde_json::Value = serde_json::from_slice(body).map_err(json_error)?;
        let tree =
            migrate::migrate_with(tree, found, u64::from(FORMAT_VERSION), &migrate::MIGRATIONS)?;
        serde_json::from_value(tree).map_err(|e| LoadError::Schema {
            line: None,
            column: None,
            message: e.to_string(),
        })?
    };
    let parts = convert::to_parts(file)?;
    Document::from_parts(&parts).map_err(LoadError::Document)
}

/// The error of the JSON reader: a syntax error, or a schema error with its line and column.
/// O(1).
#[allow(
    clippy::needless_pass_by_value,
    reason = "used as `map_err`'s function, which hands the error over by value"
)]
fn json_error(e: serde_json::Error) -> LoadError {
    use serde_json::error::Category;
    match e.classify() {
        Category::Data => LoadError::Schema {
            line: Some(e.line()),
            column: Some(e.column()),
            message: e.to_string(),
        },
        Category::Syntax | Category::Eof | Category::Io => LoadError::Syntax {
            line: e.line(),
            column: e.column(),
        },
    }
}

/// Loads the project file at `path` (see [`from_bytes`]). The size limit (ADR-0029 S9) holds before
/// the file is read and while it is read.
///
/// # Why one open and a bounded read
/// Checking the size with `metadata` and then calling `fs::read` looks at the file twice: if it
/// grows in between (another program writing it, a hostile file on a network share), `fs::read`
/// reads it whole, past [`MAX_FILE_BYTES`]. Here the file is opened **once**; its size, taken from
/// the open handle, refuses an oversized file without reading it and sizes the buffer, and the
/// read itself is capped at one byte more than the limit (`Read::take`), so a file that grew is
/// still refused with [`LoadError::TooLarge`] after at most `MAX_FILE_BYTES + 1` bytes.
///
/// # Errors
/// [`LoadError::Io`] if the file cannot be read; otherwise as [`from_bytes`].
///
/// # Complexity
/// O(size of the file) to read, plus [`from_bytes`].
pub fn load(path: &Path) -> Result<Document, LoadError> {
    use std::io::Read;
    let io_error = |e: io::Error| LoadError::Io {
        kind: e.kind(),
        os_code: e.raw_os_error(),
        path: path.to_path_buf(),
    };
    let file = fs::File::open(path).map_err(io_error)?;
    let size = file.metadata().map_err(io_error)?.len();
    if size > MAX_FILE_BYTES {
        return Err(LoadError::TooLarge { bytes: size });
    }
    // The size of the handle sizes the buffer (one allocation for a file that does not change);
    // it fits a `usize` because it is at most `MAX_FILE_BYTES` (1 GiB), so the fallback (no
    // reservation) is never taken on 32- or 64-bit platforms.
    let mut bytes = Vec::with_capacity(usize::try_from(size).unwrap_or(0));
    file.take(MAX_FILE_BYTES.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(io_error)?;
    // `from_bytes` checks the limit again on what was actually read: a file that grew after the
    // check above ends here as `TooLarge`.
    from_bytes(&bytes)
}

/// Saves a document to `path` **safely** (ADR-0029 S13): the bytes go to a temporary file in the
/// same directory, which is synced to disk and then renamed over the target, and the directory is
/// synced where the platform needs it. A failed save leaves the previous file as it was. The steps
/// live in `crate::safe_write`, shared with the MIDI export (ADR-0032 E11).
///
/// # Errors
/// [`SaveError::Io`] with the failing path; [`SaveError::Encode`] as [`to_bytes`].
pub fn save(doc: &Document, path: &Path) -> Result<(), SaveError> {
    let bytes = to_bytes(doc)?;
    crate::safe_write::write(path, &bytes).map_err(|w| SaveError::Io {
        kind: w.kind,
        os_code: w.os_code,
        path: w.path,
    })
}
