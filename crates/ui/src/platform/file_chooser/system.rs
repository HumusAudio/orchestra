// SPDX-License-Identifier: GPL-3.0-or-later

//! The system's file choosers through `rfd` (ADR-0036 B4, ADR-0050 DG4): the only file that
//! names `rfd`.
//!
//! # Where the chooser is created, and why it differs by system
//! `rfd`'s asynchronous dialog does its work when its future is **created** (`pick_file`,
//! `save_file`), not when it is first polled:
//! - on **macOS** the creation reads `AppKit`'s application object and starts a sheet on our
//!   window; `AppKit` must be called on the main thread, so the future is created here, in
//!   [`SystemChooser::start`], which the window calls from the event loop (the main thread). The
//!   sheet does not block: the future only waits for its answer, on the worker.
//! - on **Windows** the creation starts `rfd`'s own thread with the system's dialog, owned by our
//!   window (which Windows then disables until it closes); created here too.
//! - on **Linux** the creation starts `rfd`'s own thread, which calls the portal at once. The
//!   probe of the module documentation must come **first**, so there the future is created on
//!   the worker, after the probe answered.
//!
//! In every case the parent window's handles are read here, on the main thread, while the window
//! surely exists (`set_parent`).

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Mutex;

use super::{ChoiceCallback, FileChoice, FileChooser, FileRequest, FileRequestKind};

/// The name of the worker thread, visible in debuggers and `top -H`.
const THREAD_NAME: &str = "orchestra-file-chooser";

/// A chooser's answer, as a future that can move to the worker thread.
type Answer = Pin<Box<dyn Future<Output = Option<PathBuf>> + Send>>;

/// The system's choosers, through `rfd` (see the module documentation).
pub(crate) struct SystemChooser;

impl FileChooser for SystemChooser {
    fn start(&self, request: FileRequest, parent: &winit::window::Window, done: ChoiceCallback) {
        let kind = request.kind;
        let dialog = dialog(&request, parent);
        // `done` is kept where the spawning code can take it back: if the thread cannot start,
        // the closure handed to it is dropped with `done` inside, and the application would wait
        // for an answer that never comes, behind a layer that blocks everything. With the slot,
        // the answer is given here instead.
        let slot = std::sync::Arc::new(Mutex::new(Some(done)));
        let for_worker = std::sync::Arc::clone(&slot);
        let work = worker(kind, dialog);
        let spawned = std::thread::Builder::new()
            .name(THREAD_NAME.to_owned())
            .spawn(move || {
                let choice = work();
                if let Some(done) = take(&for_worker) {
                    done(choice);
                }
            });
        if spawned.is_err()
            && let Some(done) = take(&slot)
        {
            // The system refused a thread (out of resources): no chooser can be shown, and the
            // application says so, as when the portal is missing.
            done(FileChoice::Unavailable);
        }
    }
}

/// Takes the callback out of its slot, once. A poisoned lock (a panic while it was held, which
/// the code above never does) still gives the callback: the answer matters more than the lock.
fn take(slot: &Mutex<Option<ChoiceCallback>>) -> Option<ChoiceCallback> {
    match slot.lock() {
        Ok(mut guard) => guard.take(),
        Err(poisoned) => poisoned.into_inner().take(),
    }
}

/// `rfd`'s dialog for `request`, with `parent` as its parent window (DG4).
fn dialog(request: &FileRequest, parent: &winit::window::Window) -> rfd::AsyncFileDialog {
    let mut dialog = rfd::AsyncFileDialog::new()
        .set_title(request.title.as_str())
        .add_filter(request.filter_name.as_str(), &request.extensions)
        .set_parent(parent);
    if let Some(directory) = request.directory.as_deref() {
        dialog = dialog.set_directory(directory);
    }
    if request.kind == FileRequestKind::Save
        && let Some(name) = request.file_name.as_deref()
    {
        dialog = dialog.set_file_name(name);
    }
    dialog
}

/// Creates the chooser (shows it) and answers the future of its path.
fn show(kind: FileRequestKind, dialog: rfd::AsyncFileDialog) -> Answer {
    // The future is created now, outside the `async` block: creating it is what shows the
    // chooser (see the module documentation), and it must happen on this thread.
    match kind {
        FileRequestKind::Open => {
            let future = dialog.pick_file();
            Box::pin(async move { future.await.map(|file| file.path().to_path_buf()) })
        }
        FileRequestKind::Save => {
            let future = dialog.save_file();
            Box::pin(async move { future.await.map(|file| file.path().to_path_buf()) })
        }
    }
}

/// What the worker thread does, prepared on the main thread. On Linux: the probe, then the
/// chooser (created on the worker, after the probe).
#[cfg(target_os = "linux")]
fn worker(
    kind: FileRequestKind,
    dialog: rfd::AsyncFileDialog,
) -> impl FnOnce() -> FileChoice + Send {
    move || {
        super::decide(
            || {
                // The portal first; `zenity` is looked for only when the portal does not answer.
                super::portal::file_chooser_answers()
                    || super::program_in_path(
                        std::env::var_os("PATH").as_deref(),
                        "zenity",
                        std::path::Path::is_file,
                    )
            },
            || super::block_on(show(kind, dialog)),
        )
    }
}

/// What the worker thread does, prepared on the main thread. On Windows and macOS: the chooser
/// is created here, on the main thread (see the module documentation), and the worker waits for
/// its answer. The system's dialogs are always there: nothing to probe.
#[cfg(not(target_os = "linux"))]
fn worker(
    kind: FileRequestKind,
    dialog: rfd::AsyncFileDialog,
) -> impl FnOnce() -> FileChoice + Send {
    let answer = show(kind, dialog);
    move || super::decide(|| true, || super::block_on(answer))
}
