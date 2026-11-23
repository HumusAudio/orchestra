// SPDX-License-Identifier: GPL-3.0-or-later

//! The settings of the session (ADR-0050 DG6, SCALETTA F2.12a2): read once at start-up, changed
//! by the Preferences (F2.12a4), written half a second after the last change and at exit, **on a
//! thread of its own** (choice S5).
//!
//! # Why a writer thread
//! A save ends with `fsync` on the file and its directory (ADR-0029 S13): on a slow disk that can
//! take tens of milliseconds, more than a frame's budget (CLAUDE.md §5: input to frame ≤ 50 ms).
//! So the interface thread never writes: [`SettingsStore::advance`] hands a copy of the settings
//! (a few kilobytes) to a writer thread through a channel and returns. The writer keeps only the
//! **latest** copy waiting in the channel (older ones are outdated), writes it, and reports a
//! failure back through a second channel, which [`SettingsStore::take_failures`] empties.
//!
//! # Why half a second
//! Dragging a slider in the Preferences changes a setting at every pointer move; writing after
//! each would be dozens of writes per second for one gesture. The delay restarts at every change,
//! so one gesture is one write, soon after it ends (DG6).
//!
//! # The timer
//! The store does not keep a clock of its own: the window asks [`SettingsStore::next_wakeup`] when
//! to wake (`WindowApp::next_wakeup`, ADR-0040, the same timed wake-up as the caret's blink) and
//! calls [`SettingsStore::advance`] then. The demonstration window is hooked to them when the
//! Preferences make the first change (F2.12a4); until then nothing changes during a session.
//!
//! # A file that must not be written
//! When the file was written by a newer Orchestra, could not be read, or was broken and could not
//! be moved aside, [`orchestra_io::settings::Loaded::writable`] is `false`: changes still apply to
//! the session, but nothing is ever written (DG6). Without a settings directory at all (no
//! `HOME`/`APPDATA`) the store has no path and never writes either.
//!
//! # At exit
//! [`SettingsStore::close`] writes what is still waiting and **waits** for the writer to finish,
//! so a change made just before quitting is not lost.

pub(crate) mod convert;

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use orchestra_io::settings::{self, SaveError, Settings, Warning};

/// How long after the last change the settings are written (DG6).
pub const WRITE_DELAY: Duration = Duration::from_millis(500);

/// The settings of the session and their writing (see the module documentation).
#[derive(Debug)]
pub struct SettingsStore {
    /// The settings in use.
    settings: Settings,
    /// Where they are written; `None` when they must not be (see the module documentation).
    path: Option<PathBuf>,
    /// When the next write is due: `Some` after a change not yet handed to the writer.
    due: Option<Instant>,
    /// The writer thread, started at the first write.
    writer: Option<Writer>,
    /// Failures of the writes made without a thread (when one could not be started).
    failures: Vec<SaveError>,
}

/// The writer thread and its two channels.
#[derive(Debug)]
struct Writer {
    /// Settings to write, to the thread.
    to_write: Sender<Settings>,
    /// Failures, from the thread.
    failed: Receiver<SaveError>,
    /// The thread, joined by [`SettingsStore::close`].
    thread: JoinHandle<()>,
}

impl SettingsStore {
    /// Reads the settings file at `path` (see `orchestra_io::settings::load`) for the session;
    /// `None` (no settings directory) gives the defaults and a store that never writes. Returns
    /// the store and what was not used as written, for the log.
    ///
    /// O(size of the file).
    #[must_use]
    pub fn open(path: Option<PathBuf>) -> (Self, Vec<Warning>) {
        let Some(path) = path else {
            return (Self::with(Settings::default(), None), Vec::new());
        };
        let loaded = settings::load(&path);
        let path = loaded.writable.then_some(path);
        (Self::with(loaded.settings, path), loaded.warnings)
    }

    /// A store holding `settings`, writing to `path` (or never, with `None`).
    #[must_use]
    pub const fn with(settings: Settings, path: Option<PathBuf>) -> Self {
        Self {
            settings,
            path,
            due: None,
            writer: None,
            failures: Vec::new(),
        }
    }

    /// The settings in use.
    #[must_use]
    pub const fn settings(&self) -> &Settings {
        &self.settings
    }

    /// `true` when the settings are written to a file (see the module documentation).
    #[must_use]
    pub const fn is_saved(&self) -> bool {
        self.path.is_some()
    }

    /// Changes the settings with `change`; if they really changed (and are saved), the write is due
    /// [`WRITE_DELAY`] after `now`, restarting the delay of an earlier change. O(size of the
    /// settings) for the comparison.
    pub fn update(&mut self, now: Instant, change: impl FnOnce(&mut Settings)) {
        let before = self.settings.clone();
        change(&mut self.settings);
        if self.settings != before && self.path.is_some() {
            self.due = Some(now + WRITE_DELAY);
        }
    }

    /// When the window should wake to write, or `None` when nothing is waiting.
    #[must_use]
    pub const fn next_wakeup(&self) -> Option<Instant> {
        self.due
    }

    /// Hands the settings to the writer if their write is due at `now`. O(size of the settings)
    /// for the copy; never touches the disk on this thread (unless no thread could be started).
    pub fn advance(&mut self, now: Instant) {
        if self.due.is_some_and(|due| due <= now) {
            self.due = None;
            self.write_now();
        }
    }

    /// The failures reported since the last call, in order (for the log). Never blocks.
    pub fn take_failures(&mut self) -> Vec<SaveError> {
        let mut failures = std::mem::take(&mut self.failures);
        if let Some(writer) = &self.writer {
            failures.extend(writer.failed.try_iter());
        }
        failures
    }

    /// Writes what is waiting, waits for the writer to finish, and returns every failure not yet
    /// taken (see the module documentation).
    #[must_use]
    pub fn close(mut self) -> Vec<SaveError> {
        if self.due.take().is_some() {
            self.write_now();
        }
        let mut failures = std::mem::take(&mut self.failures);
        if let Some(writer) = self.writer.take() {
            // Dropping the sender ends the thread's loop once it has written what it holds.
            drop(writer.to_write);
            // A writer that panicked has nothing more to report than what it already sent.
            let _ = writer.thread.join();
            failures.extend(writer.failed.try_iter());
        }
        failures
    }

    /// Sends a copy of the settings to the writer, starting it at the first write. If the thread
    /// cannot be started (the system refuses a thread), the write happens here instead: slower,
    /// but nothing is lost.
    fn write_now(&mut self) {
        let Some(path) = self.path.clone() else {
            return;
        };
        if self.writer.is_none() {
            self.writer = Writer::start(path.clone());
        }
        let copy = self.settings.clone();
        let sent = match &self.writer {
            Some(writer) => writer.to_write.send(copy).map_err(|error| error.0),
            None => Err(copy),
        };
        // No thread, or a thread that has gone: write on this thread.
        if let Err(copy) = sent
            && let Err(failure) = settings::save(&path, &copy)
        {
            self.failures.push(failure);
        }
    }
}

impl Writer {
    /// Starts the writer thread for `path`, or `None` if the system refuses a thread.
    fn start(path: PathBuf) -> Option<Self> {
        let (to_write, pending) = mpsc::channel::<Settings>();
        let (report, failed) = mpsc::channel::<SaveError>();
        let thread = thread::Builder::new()
            .name("orchestra-settings".to_owned())
            .spawn(move || {
                // One write per batch of waiting copies: the latest one only, the others are
                // outdated. The loop ends when the store drops its sender (at `close`).
                while let Ok(first) = pending.recv() {
                    let latest = pending.try_iter().last().unwrap_or(first);
                    if let Err(failure) = settings::save(&path, &latest) {
                        // The store may be gone already at exit: nothing left to tell then.
                        let _ = report.send(failure);
                    }
                }
            })
            .ok()?;
        Some(Self {
            to_write,
            failed,
            thread,
        })
    }
}

#[cfg(test)]
mod tests {
    //! The store's timing and writing, on real files in a scratch directory.
    #![allow(clippy::unwrap_used, reason = "a failed unwrap fails the test")]

    use std::fs;

    use orchestra_io::settings::FILE_NAME;

    use super::*;

    /// A fresh directory for one test.
    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "orchestra-app-settings-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A change is due after the delay, restarted by a second change; nothing is written before.
    #[test]
    fn the_write_waits_for_the_last_change() {
        let dir = scratch("delay");
        let path = dir.join(FILE_NAME);
        let (mut store, warnings) = SettingsStore::open(Some(path.clone()));
        assert_eq!(warnings, Vec::new());
        let start = Instant::now();
        store.update(start, |s| s.caret_blink = false);
        assert_eq!(store.next_wakeup(), Some(start + WRITE_DELAY));
        let later = start + Duration::from_millis(300);
        store.update(later, |s| {
            s.language = settings::Language::Tag("it".to_owned());
        });
        assert_eq!(store.next_wakeup(), Some(later + WRITE_DELAY));
        store.advance(start + WRITE_DELAY);
        assert!(store.next_wakeup().is_some(), "not due yet");
        assert!(!path.exists());
        store.advance(later + WRITE_DELAY);
        assert_eq!(store.next_wakeup(), None);
        let failures = store.close();
        assert_eq!(failures, Vec::new());
        let loaded = settings::load(&path);
        assert!(!loaded.settings.caret_blink);
        assert_eq!(
            loaded.settings.language,
            settings::Language::Tag("it".to_owned())
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    /// A change that changes nothing schedules nothing.
    #[test]
    fn no_change_no_write() {
        let mut store = SettingsStore::with(Settings::default(), Some(PathBuf::from("unused")));
        store.update(Instant::now(), |s| s.caret_blink = true);
        assert_eq!(store.next_wakeup(), None);
    }

    /// Closing writes what is still waiting.
    #[test]
    fn close_writes_what_waits() {
        let dir = scratch("close");
        let path = dir.join("sub").join(FILE_NAME);
        let mut store = SettingsStore::with(Settings::default(), Some(path.clone()));
        store.update(Instant::now(), |s| s.caret_blink = false);
        assert_eq!(store.close(), Vec::new());
        assert!(!settings::load(&path).settings.caret_blink);
        fs::remove_dir_all(&dir).unwrap();
    }

    /// A newer file is never written over: changes apply, the file stays.
    #[test]
    fn a_newer_file_is_kept() {
        let dir = scratch("newer");
        let path = dir.join(FILE_NAME);
        let text = b"{\"orchestra_settings\": 9}";
        fs::write(&path, text).unwrap();
        let (mut store, _) = SettingsStore::open(Some(path.clone()));
        assert!(!store.is_saved());
        store.update(Instant::now(), |s| s.caret_blink = false);
        assert!(!store.settings().caret_blink);
        assert_eq!(store.next_wakeup(), None);
        assert_eq!(store.close(), Vec::new());
        assert_eq!(fs::read(&path).unwrap(), text);
        fs::remove_dir_all(&dir).unwrap();
    }

    /// A write that fails is reported, not lost in the thread.
    #[test]
    fn a_failed_write_is_reported() {
        let dir = scratch("fail");
        // A directory where the file should be: the rename over it fails.
        let path = dir.join(FILE_NAME);
        fs::create_dir_all(path.join("occupied")).unwrap();
        let mut store = SettingsStore::with(Settings::default(), Some(path));
        store.update(Instant::now(), |s| s.caret_blink = false);
        let failures = store.close();
        assert_eq!(failures.len(), 1, "{failures:?}");
        fs::remove_dir_all(&dir).unwrap();
    }
}
