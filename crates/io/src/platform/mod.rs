// SPDX-License-Identifier: GPL-3.0-or-later

//! What differs between operating systems for files (CLAUDE.md §2, ADR-0006: platform code only
//! here, behind a trait).
//!
//! # Syncing a directory after a rename
//! A safe save (ADR-0029 S13) renames a synced temporary file over the target. On Unix file
//! systems the rename itself lives in the directory, so the directory must be synced too, or a
//! crash right after the save can bring back the old directory entry. Windows has no way (and no
//! need) to sync a directory through the standard library: NTFS journals the rename itself.

#[cfg(not(unix))]
mod other;
#[cfg(unix)]
mod unix;

use std::io;
use std::path::Path;

/// The file operations whose implementation depends on the platform.
pub(crate) trait FileSystem {
    /// Makes a completed rename in `directory` durable. O(1) system calls.
    ///
    /// # Errors
    /// The operating system's error.
    fn sync_directory(directory: &Path) -> io::Result<()>;
}

/// The implementation for the platform being built.
#[cfg(unix)]
type Native = unix::Unix;
/// The implementation for the platform being built.
#[cfg(not(unix))]
type Native = other::Other;

/// Makes a completed rename in `directory` durable on this platform (see [`FileSystem`]).
///
/// # Errors
/// The operating system's error.
pub(crate) fn sync_directory(directory: &Path) -> io::Result<()> {
    Native::sync_directory(directory)
}
