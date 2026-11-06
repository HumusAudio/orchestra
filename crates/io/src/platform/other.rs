// SPDX-License-Identifier: GPL-3.0-or-later

//! Windows and every other non-Unix system.

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
}
