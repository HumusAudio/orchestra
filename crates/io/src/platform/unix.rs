// SPDX-License-Identifier: GPL-3.0-or-later

//! Linux, macOS and the other Unix systems.

use std::fs::File;
use std::io;
use std::path::Path;

use super::FileSystem;

/// The Unix implementation.
pub(crate) struct Unix;

impl FileSystem for Unix {
    /// Opens the directory and syncs it: on Unix a directory can be opened read-only as a file,
    /// and `fsync` on it makes its entries (here, the renamed one) durable.
    fn sync_directory(directory: &Path) -> io::Result<()> {
        File::open(directory)?.sync_all()
    }
}
