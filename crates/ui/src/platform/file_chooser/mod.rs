// SPDX-License-Identifier: GPL-3.0-or-later

//! The system's file choosers (ADR-0050 DG4, SCALETTA F2.12a5a): *Open* and *Save as* ask the
//! system for a path, and the answer comes back to the event loop later.
//!
//! # What this module gives the rest of the interface
//! - [`FileRequest`]: what the application asks: open or save, the chooser's title, the filter
//!   (its name and the extensions it shows), the name proposed when saving, the folder to start
//!   in. Every text is already localised by the application (CLAUDE.md §7: no text here).
//! - [`FileChoice`]: the answer: a path, *cancelled*, or *unavailable* (no chooser could open).
//! - `FileChooser`: the trait the window starts a chooser through. Its one implementation,
//!   `system::SystemChooser`, is `rfd`'s; the rest of the crate never names `rfd`.
//!
//! # The flow (DG4)
//! 1. The application asks the interface ([`Ui::choose_file`](crate::Ui::choose_file)), which puts
//!    an empty modal layer over the window, so nothing under it can be clicked or typed into
//!    while the chooser is open (where the system makes the chooser modal this changes nothing;
//!    where it does not, as with some Linux portals, our own input stays blocked anyway).
//! 2. The window takes the request
//!    ([`WindowApp::take_file_request`](crate::WindowApp::take_file_request)) after the batch of
//!    events and starts the chooser with **our window as its parent**, so the system keeps it
//!    above our window (and modal, where it can).
//! 3. A **worker thread** waits for the answer: the interface thread keeps redrawing and answering
//!    the compositor while the chooser is open (a frozen window would be marked "not responding").
//! 4. The answer travels to the event loop through its proxy (ADR-0040 W3) and reaches the
//!    application ([`WindowApp::file_chosen`](crate::WindowApp::file_chosen)), which closes the
//!    layer ([`Ui::file_chosen`](crate::Ui::file_chosen)) and acts on the path.
//!
//! While a chooser is open the window ignores the requests to close it (the system's close
//! button, Alt+F4, Quit): Windows and macOS disable the parent of a modal chooser anyway, so this
//! makes the three systems behave alike (choice 4 of F2.12a5a).
//!
//! # Telling "cancelled" from "failed" (ADR-0036 B4, choice 2 of F2.12a5a)
//! `rfd` answers `None` both when the user cancels and when no chooser could open (on Linux:
//! libdbus-1 missing, or no `FileChooser` portal, and then no `zenity` to fall back on). A silent
//! failure is not allowed, so on Linux the worker first asks the portal itself, with the `zbus`
//! connection the program already uses for the appearance (ADR-0038), whether its `FileChooser`
//! interface answers (`portal::file_chooser_answers`), and only when it does not, whether
//! `zenity` is in the `PATH` ([`program_in_path`]):
//! - the portal answers: `rfd` is called, and its `None` means *cancelled*;
//! - the portal does not answer, but `zenity` is in the `PATH`: `rfd` is called, falls back on
//!   `zenity` by itself, and its `None` means *cancelled*;
//! - neither: the answer is [`FileChoice::Unavailable`] at once, and the application shows its
//!   error dialog, which says what to install.
//!
//! One case stays silent, and is recorded as a limit (ADR-0050, `docs/native-deps.md`): a portal
//! that answers on a system **without libdbus-1 and without `zenity`**. The portal is a D-Bus
//! service, and every desktop that runs one ships libdbus-1, so it is not expected in practice.
//!
//! On Windows and macOS the system's own dialogs are always there: no probe, `None` is
//! *cancelled*.
//!
//! # Threads
//! One short-lived thread per chooser (`orchestra-file-chooser`), which ends with the answer. If
//! the program closes while a chooser is open (it cannot through the window, see above, but a
//! signal can end it) the thread is simply dropped with the process.

#[cfg(target_os = "linux")]
pub(crate) mod portal;
pub(crate) mod system;
#[cfg(test)]
mod tests;

use std::ffi::OsStr;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::pin;
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};

/// What a chooser is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FileRequestKind {
    /// Choose an existing file to open.
    Open,
    /// Choose where to save, and under what name.
    Save,
}

/// What the application asks a file chooser for (see the module documentation). Every text is
/// already localised by the application.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileRequest {
    /// Open or save.
    pub kind: FileRequestKind,
    /// The chooser's title ("Open project"). Some systems do not show it (macOS's sheets).
    pub title: String,
    /// The filter's name ("Orchestra project"), shown in the chooser's list of file types.
    pub filter_name: String,
    /// The extensions the filter shows, without the dot (`orchestra`, ADR-0029 S14).
    pub extensions: Vec<String>,
    /// The name proposed when saving ("Untitled.orchestra"); ignored when opening.
    pub file_name: Option<String>,
    /// The folder to start in; `None` lets the system choose (usually the last one used).
    pub directory: Option<PathBuf>,
}

impl FileRequest {
    /// A request to open a file whose extension is `extension` (without the dot), with the
    /// chooser titled `title` and its filter named `filter_name`.
    #[must_use]
    pub fn open(
        title: impl Into<String>,
        filter_name: impl Into<String>,
        extension: impl Into<String>,
    ) -> Self {
        Self {
            kind: FileRequestKind::Open,
            title: title.into(),
            filter_name: filter_name.into(),
            extensions: vec![extension.into()],
            file_name: None,
            directory: None,
        }
    }

    /// A request to choose where to save, proposing `file_name` (with its extension, so the
    /// chooser shows the whole name), with the chooser titled `title` and its filter named
    /// `filter_name` for `extension` (without the dot).
    #[must_use]
    pub fn save(
        title: impl Into<String>,
        filter_name: impl Into<String>,
        extension: impl Into<String>,
        file_name: impl Into<String>,
    ) -> Self {
        Self {
            kind: FileRequestKind::Save,
            title: title.into(),
            filter_name: filter_name.into(),
            extensions: vec![extension.into()],
            file_name: Some(file_name.into()),
            directory: None,
        }
    }

    /// The same request, starting in `directory`.
    #[must_use]
    pub fn in_directory(mut self, directory: impl Into<PathBuf>) -> Self {
        self.directory = Some(directory.into());
        self
    }
}

/// A file chooser's answer ([`WindowApp::file_chosen`](crate::WindowApp::file_chosen)).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileChoice {
    /// The user chose this path. When saving, it is the name the user typed: the extension is
    /// not added by every system, so the application checks it.
    Chosen(PathBuf),
    /// The user closed the chooser without choosing.
    Cancelled,
    /// No chooser could open (Linux without a `FileChooser` portal and without `zenity`): the
    /// application tells the user, never silence (ADR-0036 B4).
    Unavailable,
}

/// What the window calls when a chooser has answered: once, from the chooser's worker thread.
pub(crate) type ChoiceCallback = Box<dyn FnOnce(FileChoice) + Send>;

/// The window's way to start a chooser (see the module documentation). One implementation in the
/// program, `system::SystemChooser`; behind a trait so `rfd` stays in one file, as `arboard`
/// does for the clipboard.
pub(crate) trait FileChooser {
    /// Starts a chooser for `request` with `parent` as its parent window, and returns at once;
    /// `done` is called once, from another thread, with the answer.
    fn start(&self, request: FileRequest, parent: &winit::window::Window, done: ChoiceCallback);
}

/// The worker's whole decision (see the module documentation), with the system's parts passed
/// in so the tests can play every case: `available` says whether a chooser can appear at all,
/// `show` shows it and waits, answering `rfd`'s `Option`. `show` is not called when nothing can
/// appear. O(1) besides the two calls.
pub(crate) fn decide(
    available: impl FnOnce() -> bool,
    show: impl FnOnce() -> Option<PathBuf>,
) -> FileChoice {
    if !available() {
        return FileChoice::Unavailable;
    }
    show().map_or(FileChoice::Cancelled, FileChoice::Chosen)
}

/// Whether a program called `name` is in one of the folders of `path` (the `PATH` variable's
/// value, `None` when it is not set), as the shell would find it: the first folder holding a file
/// of that name. `is_file` is the file system's test (`Path::is_file` in the program), passed in
/// so the tests need no real files.
///
/// # Why this is enough
/// `rfd` starts `zenity` by name, so the shell's own search through `PATH` (`std::process::Command`
/// does the same) is what decides whether it will be found. Whether the file is executable is not
/// checked: a `zenity` that is there but cannot run is a broken system, and `rfd` then reports
/// its failure as `None`, the limit the module documentation records.
///
/// # Cost
/// O(folders in `path`), one `stat` per folder: once per chooser, on the worker thread.
#[cfg_attr(
    not(target_os = "linux"),
    allow(
        dead_code,
        reason = "only Linux looks for `zenity`; kept on every system so its tests run everywhere"
    )
)]
pub(crate) fn program_in_path(
    path: Option<&OsStr>,
    name: &str,
    is_file: impl Fn(&Path) -> bool,
) -> bool {
    path.is_some_and(|path| std::env::split_paths(path).any(|folder| is_file(&folder.join(name))))
}

/// Runs `future` to its end on the calling thread (the chooser's worker): polls it, and sleeps
/// between polls until it says it can make progress.
///
/// # Why our own, in a dozen lines
/// `rfd`'s choosers are futures. `zbus::block_on` and `futures-lite` are Linux-only dependencies
/// here (ADR-0038), and `pollster` comes only with `rfd`'s Linux feature; a new crate for
/// Windows and macOS would add a dependency for what the standard library already allows
/// (`std::task::Wake`): the waker unparks the worker, which parks between polls.
///
/// # Correctness
/// A wake that comes before the worker parks is not lost: `unpark` makes the next `park` return
/// at once (the standard library's token). `park` may also return without a wake (it is allowed
/// to); the loop then just polls again, which a future must accept.
pub(crate) fn block_on<F: Future>(future: F) -> F::Output {
    /// Wakes the thread that waits for the future.
    struct Unpark(std::thread::Thread);

    impl Wake for Unpark {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }

        fn wake_by_ref(self: &Arc<Self>) {
            self.0.unpark();
        }
    }

    let mut future = pin!(future);
    let waker = Waker::from(Arc::new(Unpark(std::thread::current())));
    let mut context = Context::from_waker(&waker);
    loop {
        if let Poll::Ready(output) = future.as_mut().poll(&mut context) {
            return output;
        }
        std::thread::park();
    }
}
