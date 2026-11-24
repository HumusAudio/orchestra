// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of the file choosers' decision, the `PATH` search and the small executor (F2.12a5a).
//! None opens a real chooser: the system's parts are played by closures (CLAUDE.md §3).

use std::cell::Cell;
use std::ffi::OsString;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::{Context, Poll};

use super::{FileChoice, FileRequest, FileRequestKind, block_on, decide, program_in_path};

/// A chosen path comes back as `Chosen`.
#[test]
fn a_chosen_path_is_chosen() {
    let choice = decide(|| true, || Some(PathBuf::from("song.orchestra")));
    assert_eq!(choice, FileChoice::Chosen(PathBuf::from("song.orchestra")));
}

/// `None` from a chooser that could appear means the user cancelled.
#[test]
fn none_from_an_available_chooser_is_cancelled() {
    assert_eq!(decide(|| true, || None), FileChoice::Cancelled);
}

/// When nothing can appear the answer is `Unavailable`, and the chooser is never asked (on Linux
/// that would let `rfd` fail in silence).
#[test]
fn nothing_available_is_unavailable_without_showing() {
    let shown = Cell::new(false);
    let choice = decide(
        || false,
        || {
            shown.set(true);
            Some(PathBuf::from("never"))
        },
    );
    assert_eq!(choice, FileChoice::Unavailable);
    assert!(!shown.get(), "the chooser must not be shown");
}

/// A program is found in any folder of `PATH`, in the platform's own list syntax.
#[test]
fn a_program_is_found_in_any_folder_of_the_path() {
    let folders = [PathBuf::from("first"), PathBuf::from("second")];
    let path: OsString = std::env::join_paths(&folders).unwrap_or_default();
    let in_second = |file: &Path| file == Path::new("second").join("zenity");
    assert!(program_in_path(Some(path.as_os_str()), "zenity", in_second));
    assert!(!program_in_path(
        Some(path.as_os_str()),
        "kdialog",
        in_second
    ));
}

/// Without `PATH` nothing is found. (An empty `PATH` is not "nothing": POSIX reads an empty
/// entry as the current folder, and `split_paths` gives that entry, as the shell would.)
#[test]
fn no_path_finds_nothing() {
    assert!(!program_in_path(None, "zenity", |_| true));
}

/// The two constructors fill what each kind needs; a folder can be added.
#[test]
fn requests_hold_what_their_kind_needs() {
    let open = FileRequest::open("open-title", "filter-name", "orchestra");
    assert_eq!(open.kind, FileRequestKind::Open);
    assert_eq!(open.extensions, ["orchestra"]);
    assert_eq!(open.file_name, None);
    let save = FileRequest::save(
        "save-title",
        "filter-name",
        "orchestra",
        "Untitled.orchestra",
    )
    .in_directory("music");
    assert_eq!(save.kind, FileRequestKind::Save);
    assert_eq!(save.file_name.as_deref(), Some("Untitled.orchestra"));
    assert_eq!(save.directory, Some(PathBuf::from("music")));
}

/// A ready future is returned at once.
#[test]
fn block_on_returns_a_ready_value() {
    assert_eq!(block_on(async { 7 }), 7);
}

/// A future that is woken from another thread, as `rfd`'s are, is polled again and finishes; a
/// wake that arrives before the worker parks is not lost (the second poll here may come either
/// way).
#[test]
fn block_on_waits_for_a_wake_from_another_thread() {
    /// Pending until `done` is set by the other thread, which then wakes it.
    struct Later {
        /// Set by the other thread.
        done: Arc<AtomicBool>,
        /// Whether the other thread was started.
        started: bool,
    }

    impl Future for Later {
        type Output = &'static str;

        fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            if self.done.load(Ordering::Acquire) {
                return Poll::Ready("answered");
            }
            if !self.started {
                self.started = true;
                let (done, waker) = (Arc::clone(&self.done), cx.waker().clone());
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(20));
                    done.store(true, Ordering::Release);
                    waker.wake();
                });
            }
            Poll::Pending
        }
    }

    let future = Later {
        done: Arc::new(AtomicBool::new(false)),
        started: false,
    };
    assert_eq!(block_on(future), "answered");
}
