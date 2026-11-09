// SPDX-License-Identifier: GPL-3.0-or-later

//! What differs between operating systems for files (CLAUDE.md §2, ADR-0006: platform code only
//! here, behind a trait).
//!
//! # Syncing a directory after a rename
//! A safe save (ADR-0029 S13) renames a synced temporary file over the target. On Unix file
//! systems the rename itself lives in the directory, so the directory must be synced too, or a
//! crash right after the save can bring back the old directory entry. Windows has no way (and no
//! need) to sync a directory through the standard library: NTFS journals the rename itself.
//!
//! # Keeping the permissions of the file replaced
//! The temporary file is created with the default permissions (on Unix: `0o666` minus the umask),
//! so after the rename a project the user had made private (`0o600`) or shared with a group
//! (`0o664`) would silently get the default mode. On Unix the target's mode is therefore copied
//! onto the temporary file before the rename. Windows needs nothing: a new file inherits the
//! directory's access rules, and its only standard permission, the read-only flag, would make
//! the next save fail if copied (a read-only target refuses the replacing rename anyway, which
//! reports the problem instead of hiding it).

#[cfg(not(unix))]
mod other;
#[cfg(unix)]
mod unix;

use std::fs::{self, File};
use std::io;
use std::path::Path;

/// The file operations whose implementation depends on the platform.
pub(crate) trait FileSystem {
    /// Makes a completed rename in `directory` durable. O(1) system calls.
    ///
    /// # Errors
    /// The operating system's error.
    fn sync_directory(directory: &Path) -> io::Result<()>;

    /// Gives `temp` the permissions of `target`, which it is about to replace (see the module
    /// documentation). O(1) system calls.
    ///
    /// # Errors
    /// The operating system's error.
    fn copy_permissions(target: &fs::Metadata, temp: &File) -> io::Result<()>;
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

/// Gives `temp` the permissions of the file it replaces, on this platform (see [`FileSystem`]).
///
/// # Errors
/// The operating system's error.
pub(crate) fn copy_permissions(target: &fs::Metadata, temp: &File) -> io::Result<()> {
    Native::copy_permissions(target, temp)
}
