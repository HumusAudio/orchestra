// SPDX-License-Identifier: GPL-3.0-or-later

//! Windows and every other non-Unix system.

use std::fs::{self, File};
use std::io;
use std::path::Path;

use super::FileSystem;

/// The implementation where a directory cannot be synced through the standard library.
pub(crate) struct Other;

impl FileSystem for Other {
    /// Nothing to do: on Windows a directory cannot be opened as a file without platform calls,
    /// and NTFS journals the rename's metadata itself.
    fn sync_directory(_directory: &Path) -> io::Result<()> {
        Ok(())
    }

    /// Nothing to do: the new file inherits the directory's access rules, and copying the
    /// read-only flag would only make the next save fail (see the module documentation).
    fn copy_permissions(_target: &fs::Metadata, _temp: &File) -> io::Result<()> {
        Ok(())
    }
}
