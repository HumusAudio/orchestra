// SPDX-License-Identifier: GPL-3.0-or-later

//! Linux, macOS and the other Unix systems.

use std::fs::{self, File};
use std::io;
use std::os::unix::fs::PermissionsExt;
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

    /// Copies the permission bits (`st_mode & 0o7777`: read, write, execute for owner, group and
    /// others, plus setuid, setgid and sticky) with `fchmod` on the open temporary file. The owner
    /// and the group are not copied: changing them needs privileges a normal user does not have,
    /// and the new file already belongs to the user saving it.
    fn copy_permissions(target: &fs::Metadata, temp: &File) -> io::Result<()> {
        let mode = target.permissions().mode() & 0o7777;
        temp.set_permissions(fs::Permissions::from_mode(mode))
    }
}
