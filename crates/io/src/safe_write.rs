// SPDX-License-Identifier: GPL-3.0-or-later

//! Writing a file **safely**: either the new bytes are all there, or the previous file is intact
//! (ADR-0029 S13). Shared by the project file ([`crate::project::save`]) and the MIDI export
//! ([`crate::midi::save`], ADR-0032 E11), so that both get the very same guarantees.
//!
//! # The steps
//! 1. The bytes go to a temporary file **in the same directory** as the target, named
//!    `.<name>.<process id>.<serial>.tmp` (hidden on Unix). See "Unique temporary names" below.
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
//!
//! # Unique temporary names
//! The temporary file is opened with `create_new`, so we never write into a file that someone
//! else is writing. Its name must therefore be unique among the writers that may run at once:
//! - **Other processes**: the process id is in the name.
//! - **Other threads of this process** (the automatic save of F2.14 next to a manual save, or a
//!   MIDI export of the same file): a process-wide serial number ([`SERIAL`]), taken atomically.
//! - **Leftovers of a crash**: a process that died between creating and renaming its temporary
//!   file leaves it behind, and the operating system may later give its process id to us. The
//!   serial number of a fresh process starts at 0 again, so the same name may come back. When a
//!   name is taken (`AlreadyExists`), we simply try the next serial number, at most
//!   [`MAX_ATTEMPTS`] times; the leftover is not ours to delete (it might be another program's
//!   save in progress), so it is left alone.
//!
//! An earlier version used only the process id: two saves of one process collided, and one
//! leftover blocked every later save of that file with the same process id.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Why a safe write failed: the kind of the operating system's error and the path it concerns
/// (the temporary file, the target or the directory). Each caller wraps it into its own error
/// type, which is why this is a plain pair rather than an error type of its own.
pub(crate) type WriteFailure = (io::ErrorKind, PathBuf);

/// The process-wide serial number of the next temporary file (see "Unique temporary names" in
/// the module documentation). `Relaxed` ordering is enough: we only need every `fetch_add` to
/// return a different value, which atomicity alone guarantees; no other memory is synchronised
/// through it. A `u64` never wraps in practice (one save per nanosecond would take 584 years).
static SERIAL: AtomicU64 = AtomicU64::new(0);

/// How many temporary names a write tries before giving up with `AlreadyExists`. Each attempt
/// that fails this way means a file with that exact name already exists in the directory, which
/// takes a crash leftover per attempt: 64 is far more than any real directory holds, and still
/// bounds the loop (no unbounded waits, CLAUDE.md §5).
const MAX_ATTEMPTS: u32 = 64;

/// Writes `bytes` to `path` safely (see the module documentation).
///
/// # Errors
/// The kind of the first operating-system error and the path it concerns; `InvalidInput` with
/// the target itself when `path` has no file name (for example `..` or a root);
/// `AlreadyExists` with the last temporary path tried if [`MAX_ATTEMPTS`] names were all taken.
///
/// # Complexity
/// O(bytes) to write, plus a constant number of system calls (at most [`MAX_ATTEMPTS`] opens).
pub(crate) fn write(path: &Path, bytes: &[u8]) -> Result<(), WriteFailure> {
    write_with(path, bytes, || SERIAL.fetch_add(1, Ordering::Relaxed))
}

/// [`write`] with the source of serial numbers passed in, so that the tests can make it collide
/// with a leftover on purpose. `next_serial` is called once per attempt.
///
/// # Errors
/// As [`write`].
///
/// # Complexity
/// As [`write`].
fn write_with(
    path: &Path,
    bytes: &[u8],
    mut next_serial: impl FnMut() -> u64,
) -> Result<(), WriteFailure> {
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
    let (temp, mut file) = create_temp(&directory, name, &mut next_serial)?;
    // The remaining steps, in a closure so that a failure at any of them reaches the clean-up
    // below. The file was created by us (`create_new`), so removing it on failure never touches
    // anybody else's file.
    let written = (|| -> Result<(), WriteFailure> {
        file.write_all(bytes).map_err(failure(&temp))?;
        file.sync_all().map_err(failure(&temp))?;
        // Close the file before renaming it: Windows refuses to rename an open file.
        drop(file);
        fs::rename(&temp, path).map_err(failure(path))?;
        crate::platform::sync_directory(&directory).map_err(failure(&directory))
    })();
    if written.is_err() {
        // Best effort: the temporary file may already be the target (the failure was syncing the
        // directory, after the rename); either way there is nothing more to do, and the original
        // error is the one to report.
        let _ = fs::remove_file(&temp);
    }
    written
}

/// Creates a new temporary file `.<name>.<pid>.<serial>.tmp` in `directory`, trying the next
/// serial number while the name is taken (see "Unique temporary names" in the module
/// documentation). Returns its path and the open file.
///
/// # Errors
/// The first error other than `AlreadyExists`, with the temporary path; `AlreadyExists` with the
/// last path tried after [`MAX_ATTEMPTS`] taken names.
///
/// # Complexity
/// O(attempts) system calls, at most [`MAX_ATTEMPTS`].
fn create_temp(
    directory: &Path,
    name: &std::ffi::OsStr,
    next_serial: &mut impl FnMut() -> u64,
) -> Result<(PathBuf, fs::File), WriteFailure> {
    let pid = std::process::id();
    // `MAX_ATTEMPTS ≥ 1`, so the loop runs at least once and `last` is always replaced; it starts
    // as the directory only to have a value of the right type without an `Option`.
    let mut last = directory.to_path_buf();
    for _ in 0..MAX_ATTEMPTS {
        // Built as an `OsString` so that a non-UTF-8 name stays intact.
        let mut temp_name = std::ffi::OsString::from(".");
        temp_name.push(name);
        temp_name.push(format!(".{pid}.{}.tmp", next_serial()));
        let temp = directory.join(temp_name);
        // `create_new`: never write into a file that already exists under the temporary name.
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
        {
            Ok(file) => return Ok((temp, file)),
            // Taken by a leftover or by another writer: try the next name.
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => last = temp,
            Err(e) => return Err((e.kind(), temp)),
        }
    }
    Err((io::ErrorKind::AlreadyExists, last))
}

#[cfg(test)]
mod tests {
    //! The unique temporary names: a leftover with the next name is skipped, too many leftovers
    //! give a typed error, and saves from several threads at once all succeed.

    use super::*;

    /// A fresh, empty directory for one test, under the system's temporary directory.
    fn test_dir(test: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "orchestra-safe-write-{test}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// The names of the files in `dir`, sorted.
    fn names(dir: &Path) -> Vec<std::ffi::OsString> {
        let mut names: Vec<_> = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        names.sort();
        names
    }

    /// The temporary name for serial `n` of the target `name`.
    fn temp_name(name: &str, n: u64) -> String {
        format!(".{name}.{}.{n}.tmp", std::process::id())
    }

    /// A crash leftover under the first name tried is skipped and left alone; the save succeeds.
    #[test]
    fn a_leftover_is_skipped() {
        let dir = test_dir("leftover");
        let leftover = dir.join(temp_name("a.mid", 0));
        fs::write(&leftover, b"leftover").unwrap();
        let mut serials = 0..;
        write_with(&dir.join("a.mid"), b"new", || serials.next().unwrap()).unwrap();
        assert_eq!(fs::read(dir.join("a.mid")).unwrap(), b"new");
        assert_eq!(
            fs::read(&leftover).unwrap(),
            b"leftover",
            "not ours to delete"
        );
        assert_eq!(names(&dir).len(), 2, "{:?}", names(&dir));
        fs::remove_dir_all(&dir).unwrap();
    }

    /// When every name tried is taken, the error is `AlreadyExists` and the target is untouched.
    #[test]
    fn too_many_leftovers_are_an_error() {
        let dir = test_dir("too-many");
        let leftover = dir.join(temp_name("a.mid", 7));
        fs::write(&leftover, b"leftover").unwrap();
        // Always the same serial: every attempt meets the leftover.
        let error = write_with(&dir.join("a.mid"), b"new", || 7).unwrap_err();
        assert_eq!(error, (io::ErrorKind::AlreadyExists, leftover));
        assert!(!dir.join("a.mid").exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    /// Eight threads saving the same file 25 times each: no two saves share a temporary file, the
    /// file holds one of the contents in full, and no temporary file is left.
    ///
    /// On Unix every save succeeds. On Windows a replacing rename may be refused with
    /// `PermissionDenied` while another thread is replacing the same target at that very moment
    /// (the target is briefly held open by the other rename); that refusal concerns the target,
    /// never a temporary name, and is the only failure accepted there.
    #[test]
    fn concurrent_saves_of_one_file_succeed() {
        let dir = test_dir("concurrent");
        let path = dir.join("song.orchestra");
        std::thread::scope(|s| {
            for t in 0..8_u8 {
                let path = &path;
                s.spawn(move || {
                    for _ in 0..25 {
                        match write(path, &[t; 4096]) {
                            Ok(()) => {}
                            Err((io::ErrorKind::PermissionDenied, p))
                                if cfg!(windows) && p == *path => {}
                            Err(e) => panic!("save failed: {e:?}"),
                        }
                    }
                });
            }
        });
        let content = fs::read(&path).unwrap();
        assert_eq!(content.len(), 4096);
        assert!(content.iter().all(|b| *b == content[0]), "a mixed file");
        assert_eq!(
            names(&dir),
            vec![std::ffi::OsString::from("song.orchestra")]
        );
        fs::remove_dir_all(&dir).unwrap();
    }
}
