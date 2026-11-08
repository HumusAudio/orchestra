// SPDX-License-Identifier: GPL-3.0-or-later

//! Writing a file **safely**: either the new bytes are all there, or the previous file is intact
//! (ADR-0029 S13). Shared by the project file ([`crate::project::save`]) and the MIDI export
//! ([`crate::midi::save`], ADR-0032 E11), so that both get the very same guarantees.
//!
//! # The steps
//! 1. The bytes go to a temporary file **in the same directory** as the target, named
//!    `.<name>.<process id>.tmp` (hidden on Unix, unique per process, so two programs saving the
//!    same file at once cannot write into each other's temporary file).
//! 2. The temporary file is synced to disk (`sync_all`), so its content is durable before it
//!    becomes visible under the target's name.
//! 3. It is renamed over the target: an atomic replacement within one file system.
//! 4. The directory is synced where the platform needs it (`crate::platform`), so the rename
//!    itself survives a crash.
//!
//! A failure at any step removes the temporary file (best effort) and leaves the previous file
//! as it was.
//!
//! # Why the same directory
//! A rename is atomic only within one file system; a temporary file elsewhere (say, `/tmp`) could
//! be on another one, and the "rename" would become a copy that a crash can cut in half.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Why a safe write failed: the kind of the operating system's error and the path it concerns
/// (the temporary file, the target or the directory). Each caller wraps it into its own error
/// type, which is why this is a plain pair rather than an error type of its own.
pub(crate) type WriteFailure = (io::ErrorKind, PathBuf);

/// Writes `bytes` to `path` safely (see the module documentation).
///
/// # Errors
/// The kind of the first operating-system error and the path it concerns; `InvalidInput` with
/// the target itself when `path` has no file name (for example `..` or a root).
///
/// # Complexity
/// O(bytes) to write, plus a constant number of system calls.
pub(crate) fn write(path: &Path, bytes: &[u8]) -> Result<(), WriteFailure> {
    // Turns an `io::Error` into the failure for the path `p`. A closure factory, because each step
    // reports a different path and `map_err` takes a closure by value.
    let failure = |p: &Path| {
        let p = p.to_path_buf();
        move |e: io::Error| (e.kind(), p)
    };
    let name = path
        .file_name()
        .ok_or_else(|| (io::ErrorKind::InvalidInput, path.to_path_buf()))?;
    // A bare file name has an empty parent: the current directory is meant.
    let directory = match path.parent() {
        Some(d) if !d.as_os_str().is_empty() => d.to_path_buf(),
        _ => PathBuf::from("."),
    };
    // `.<name>.<pid>.tmp`, built as an `OsString` so that a non-UTF-8 name stays intact.
    let mut temp_name = std::ffi::OsString::from(".");
    temp_name.push(name);
    temp_name.push(format!(".{}.tmp", std::process::id()));
    let temp = directory.join(temp_name);
    // The steps, in a closure so that a failure at any of them reaches the clean-up below.
    let written = (|| -> Result<(), WriteFailure> {
        // `create_new`: never write into a file that already exists under the temporary name
        // (a leftover of a crashed save of this very process id is reported, not overwritten).
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(failure(&temp))?;
        file.write_all(bytes).map_err(failure(&temp))?;
        file.sync_all().map_err(failure(&temp))?;
        // Close the file before renaming it: Windows refuses to rename an open file.
        drop(file);
        fs::rename(&temp, path).map_err(failure(path))?;
        crate::platform::sync_directory(&directory).map_err(failure(&directory))
    })();
    if written.is_err() {
        // Best effort: the temporary file may not exist (the failure was opening it) or may
        // already be the target (the failure was syncing the directory); either way there is
        // nothing more to do, and the original error is the one to report.
        let _ = fs::remove_file(&temp);
    }
    written
}
