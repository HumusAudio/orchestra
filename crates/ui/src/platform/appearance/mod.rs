// SPDX-License-Identifier: GPL-3.0-or-later

//! The operating system's light/dark preference, read once and then observed (ADR-0014 §5,
//! ADR-0038).
//!
//! # What this module gives the rest of the interface
//!
//! The theme system already knows how to pick a half of a pair from the user's mode and the
//! system's answer: [`ThemeMode::resolve`](crate::ThemeMode::resolve) takes an
//! `Option<Appearance>`, where `None` means "the system said nothing" and gives dark (ADR-0037
//! T9). This module produces that `Option`, and every later change of it:
//!
//! - [`SystemAppearanceSource`] is the one trait (ADR-0038 S1). [`system_appearance_source`]
//!   returns the implementation of the platform the program was built for.
//! - [`SystemAppearanceSource::observe`] returns an [`AppearanceWatch`], whose
//!   [`current`](AppearanceWatch::current) is the preference known at that moment, and calls a
//!   callback with each **real transition** afterwards, until the watch is dropped.
//!
//! # Per-platform files
//!
//! - `portal` (Linux): the XDG desktop portal over D-Bus, with our own code on `zbus`
//!   (ADR-0038 S2).
//! - `no_answer` (Windows, macOS): always `None` for now. ADR-0038 S3 implements both with
//!   `winit` in step F2.04, when `winit` enters the workspace; until then there is no window to
//!   theme, so the dark fallback is harmless.
//!
//! # The thread-based machinery, shared and tested on every platform
//!
//! A source that has to *wait* for the system (the portal is a separate process) runs on a
//! thread of its own and reports what it learns through a [`Reporter`]. The rules that matter
//! for the user are all in this file, not in the platform code, so that they are tested on every
//! CI runner with fake sources (see `tests`):
//!
//! 1. **Startup never waits more than [`FIRST_READ_TIMEOUT`]** (250 ms, ADR-0038 S5). If the first
//!    answer is late, `observe` returns with `None` (dark) and the late answer arrives through the
//!    callback like any change.
//! 2. **Only real transitions reach the callback.** The portal was seen sending each change three
//!    times (ADR-0038, context); a value equal to the last one delivered is dropped.
//! 3. **Errors are never failures for the user.** Any error means "no answer" (`None`), and the
//!    last one is kept for diagnostics in [`AppearanceWatch::last_error`] (ADR-0038 S2).
//! 4. **After the watch is dropped, the callback is never called again**, not even once: the
//!    callback lives behind the same lock the reporter takes to call it.

// On Windows and macOS (until F2.04, ADR-0038 S3) no source uses the thread-based machinery
// below, so outside the tests the compiler would call it dead. It is kept compiled there on
// purpose: the tests exercise it on every platform with fake sources, and F2.04 may use it. The
// attribute is limited to exactly that case, so on Linux, and in every test build, real dead code
// is still reported.
#![cfg_attr(
    not(any(target_os = "linux", test)),
    allow(
        dead_code,
        reason = "thread-based sources exist only on Linux until F2.04 (ADR-0038 S3)"
    )
)]

#[cfg(not(target_os = "linux"))]
mod no_answer;
#[cfg(target_os = "linux")]
mod portal;
#[cfg(test)]
mod tests;

use std::fmt;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use crate::Appearance;

/// The longest time [`SystemAppearanceSource::observe`] waits for the system's first answer
/// (ADR-0038 S5, owner's choice of 2026-11-10).
///
/// # Why 250 ms
/// Startup must reach an interactive window within 3 s on the reference laptop (CLAUDE.md §5).
/// When the desktop portal is not running, D-Bus may try to start it and the call can wait up to
/// D-Bus's own timeout (25 s by default), so an unbounded wait is out of the question. A portal
/// that is running answers in a local round trip of a few milliseconds, so 250 ms is generous for
/// the normal case and still a small slice of the startup budget. The cost of a late answer is a
/// theme switch shortly after the window appears, in that rare case only.
pub(crate) const FIRST_READ_TIMEOUT: Duration = Duration::from_millis(250);

/// The callback that receives each change of the system's preference (ADR-0038 S1).
///
/// The argument is the new answer: `Some` light or dark, or `None` when the system stops
/// answering (an error, the portal going away). It is called on the source's own thread, never on
/// the caller's, hence `Send`; in F2.04 it will forward the value to the event loop of the window.
///
/// # Contract
/// It runs while the watch's internal lock is held (rule 4 of the module documentation), so it
/// must be short and must not call methods of the [`AppearanceWatch`] it belongs to: that would
/// wait for the lock it is holding. Forwarding the value to another thread (a channel, an event
/// loop proxy) is the intended use.
pub type AppearanceCallback = Box<dyn FnMut(Option<Appearance>) + Send + 'static>;

/// A source of the operating system's light/dark preference: the one trait of ADR-0038 S1.
///
/// The interface asks [`system_appearance_source`] for the platform's implementation and never
/// learns which one it got. Tests and, later, other front ends can supply their own.
pub trait SystemAppearanceSource {
    /// Starts observing the system's preference.
    ///
    /// Returns once the first answer is known, or after [`FIRST_READ_TIMEOUT`] at most (then with
    /// `None`). From then on `on_change` receives every real transition, including a first answer
    /// that came too late, until the returned [`AppearanceWatch`] is dropped.
    ///
    /// Never fails: a system that cannot be asked simply answers `None` (ADR-0038 S2), and the
    /// reason is in [`AppearanceWatch::last_error`].
    fn observe(&self, on_change: AppearanceCallback) -> AppearanceWatch;
}

/// The implementation of [`SystemAppearanceSource`] for the platform this program was built for.
///
/// - Linux: the XDG desktop portal (ADR-0038 S2).
/// - Windows and macOS: no answer until step F2.04 brings `winit` (ADR-0038 S3).
///
/// Cheap: it builds a value without touching the system; the work starts in
/// [`observe`](SystemAppearanceSource::observe).
#[must_use]
pub fn system_appearance_source() -> impl SystemAppearanceSource {
    #[cfg(target_os = "linux")]
    {
        portal::PortalSource
    }
    #[cfg(not(target_os = "linux"))]
    {
        no_answer::NoAnswerSource
    }
}

/// Why the system's preference could not be read, kept for diagnostics (ADR-0038 S2).
///
/// None of these is shown to the user as a failure: each one means "no answer" and the theme
/// falls back to dark (ADR-0037 T9). The `detail` strings come from the D-Bus library and are
/// for developers (bug reports, a future log), in English like every message of the libraries.
/// The enum is exhaustive (ADR-0034 D2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SystemAppearanceError {
    /// The session bus could not be reached: no bus at all, a sandbox that forbids the socket, a
    /// session started without D-Bus.
    Unreachable {
        /// The library's description of the failure.
        detail: String,
    },
    /// The bus answered, but reading the setting failed: no portal installed, a portal without
    /// the `org.freedesktop.portal.Settings` interface, or the key missing.
    Read {
        /// The library's description of the failure.
        detail: String,
    },
    /// The setting had a type other than the documented `u32` (after unwrapping the extra variant
    /// layer of the old `Read` method).
    UnexpectedType {
        /// The D-Bus signature of the value received, for example `s` for a string.
        signature: String,
    },
    /// The setting was a number the portal documentation does not define (only 0, 1 and 2 are).
    UnknownValue {
        /// The number received.
        value: u32,
    },
    /// Observing the changes failed: subscribing to the signal was refused, or the stream of
    /// messages broke or ended (the bus went away).
    Observe {
        /// The library's description of the failure.
        detail: String,
    },
    /// No first answer within [`FIRST_READ_TIMEOUT`]. Not final: a late answer is still
    /// delivered to the callback.
    Timeout,
    /// The thread that talks to the system could not be started (the operating system refused
    /// to create it, which in practice means it is out of resources).
    Thread {
        /// The operating system's description of the failure.
        detail: String,
    },
}

impl fmt::Display for SystemAppearanceError {
    /// A one-line English description for developers (the interface never shows these).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unreachable { detail } => write!(f, "session bus unreachable: {detail}"),
            Self::Read { detail } => write!(f, "reading the colour scheme failed: {detail}"),
            Self::UnexpectedType { signature } => {
                write!(
                    f,
                    "colour scheme of unexpected type `{signature}`, expected `u32`"
                )
            }
            Self::UnknownValue { value } => write!(f, "unknown colour scheme value {value}"),
            Self::Observe { detail } => write!(f, "observing the colour scheme failed: {detail}"),
            Self::Timeout => write!(f, "no answer within {} ms", FIRST_READ_TIMEOUT.as_millis()),
            Self::Thread { detail } => write!(f, "could not start the observer thread: {detail}"),
        }
    }
}

impl std::error::Error for SystemAppearanceError {}

/// What a running observation knows, shared by the [`AppearanceWatch`] (the caller's side) and the
/// [`Reporter`] (the source's thread). Always accessed under [`Shared::state`]'s lock.
struct State {
    /// `true` until the first answer arrives or [`AppearanceWatch`] stops waiting for it. While it
    /// is `true`, an answer goes to the waiting caller, not to the callback.
    waiting: bool,
    /// The last answer the caller has been given, either as the initial value or through the
    /// callback. A new answer equal to this one is a repeat and is dropped (module rule 2).
    delivered: Option<Appearance>,
    /// The caller's callback; `None` once the watch is dropped (module rule 4), or for a source
    /// that never changes.
    callback: Option<AppearanceCallback>,
    /// The most recent error, for diagnostics (module rule 3).
    last_error: Option<SystemAppearanceError>,
}

/// The state of one observation and the condition variable the caller waits on.
///
/// # Why a `Mutex` and a `Condvar`
/// This is interface code, never the audio thread, so a lock is allowed (CLAUDE.md §5 forbids
/// locks only on the audio thread). The condition variable lets the caller sleep until the first
/// answer *or* the timeout, whichever comes first, with no polling. Both sides hold the lock only
/// for a few instructions, plus the callback's own (short) work.
struct Shared {
    /// The observation's state.
    state: Mutex<State>,
    /// Signalled when `waiting` becomes `false` because the first answer arrived.
    first_answer: Condvar,
}

impl Shared {
    /// A new shared state, still waiting for its first answer, with the caller's callback.
    fn new(callback: AppearanceCallback) -> Self {
        Self {
            state: Mutex::new(State {
                waiting: true,
                delivered: None,
                callback: Some(callback),
                last_error: None,
            }),
            first_answer: Condvar::new(),
        }
    }

    /// Takes the lock, recovering the state if a previous holder panicked.
    ///
    /// # Why recovering is right here
    /// A poisoned lock means a callback panicked while it held the lock. Every field of [`State`]
    /// is still meaningful (each update is a single assignment), so continuing with it is safe,
    /// and refusing would turn a bug in a callback into a second failure of the theme.
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// The source thread's way of telling the [`AppearanceWatch`] what the system said.
///
/// Every source that waits on the system (today the Linux portal) gets one and calls it from its
/// own thread. It applies the module's rules 1–4, so the platform code only translates the
/// system's messages into [`Reporter::answer`] and [`Reporter::fail`].
pub(crate) struct Reporter {
    /// The observation this reporter feeds.
    shared: Arc<Shared>,
}

impl Reporter {
    /// The system gave an answer: `Some` light or dark, or `None` for "no preference I can use".
    ///
    /// If the caller is still waiting for its first answer, this is it and the caller wakes up.
    /// Otherwise it goes to the callback, unless it equals the last value delivered (a repeat) or
    /// the watch has been dropped.
    pub(crate) fn answer(&self, value: Option<Appearance>) {
        let mut state = self.shared.lock();
        if state.waiting {
            // The first answer, in time: it becomes the watch's initial value. The callback is
            // not called, because the caller receives it as the return value of `observe`.
            state.waiting = false;
            state.delivered = value;
            self.shared.first_answer.notify_all();
        } else if state.delivered != value {
            // A real transition (module rule 2), including a first answer that came after the
            // timeout: the caller assumed `None` then, so `delivered` is `None` and anything else
            // is news. `delivered` is updated even when the callback is gone, which costs nothing.
            state.delivered = value;
            if let Some(callback) = state.callback.as_mut() {
                callback(value);
            }
        }
    }

    /// Something went wrong: keep the error for diagnostics and answer "no answer" (`None`).
    ///
    /// Equivalent to [`Reporter::answer`]`(None)` after recording `error`, so a failure while
    /// observing turns the theme to the dark fallback, as ADR-0038 S2 says, and a failure before
    /// the first answer releases the waiting caller at once instead of after the timeout.
    pub(crate) fn fail(&self, error: SystemAppearanceError) {
        self.shared.lock().last_error = Some(error);
        self.answer(None);
    }
}

/// A running observation of the system's preference, returned by
/// [`SystemAppearanceSource::observe`].
///
/// Keep it for as long as the preference matters (in practice, the whole session). Dropping it
/// stops the callback at once. A source thread blocked on the system keeps sleeping (it costs no
/// CPU) and ends at its next message or with the program; see `portal` for why it is not
/// interrupted.
pub struct AppearanceWatch {
    /// The state shared with the source's thread, if any.
    shared: Arc<Shared>,
}

impl AppearanceWatch {
    /// Starts `run` on a new thread named `thread_name` and waits up to `timeout` for its first
    /// answer (module rule 1).
    ///
    /// `run` receives the [`Reporter`] and may keep it for as long as it observes. If the thread
    /// cannot be created, the watch answers `None` at once with a [`SystemAppearanceError::Thread`]
    /// error.
    ///
    /// # Complexity
    /// One thread creation and at most one bounded wait; no polling.
    pub(crate) fn spawn<F>(
        on_change: AppearanceCallback,
        thread_name: &str,
        timeout: Duration,
        run: F,
    ) -> Self
    where
        F: FnOnce(&Reporter) + Send + 'static,
    {
        let shared = Arc::new(Shared::new(on_change));
        let reporter = Reporter {
            shared: Arc::clone(&shared),
        };
        let spawned = std::thread::Builder::new()
            .name(thread_name.to_owned())
            .spawn(move || run(&reporter));
        if let Err(error) = spawned {
            // No thread, so nobody will ever answer: stop waiting now and record why.
            let mut state = shared.lock();
            state.waiting = false;
            state.last_error = Some(SystemAppearanceError::Thread {
                detail: error.to_string(),
            });
            drop(state);
            return Self { shared };
        }
        // The `JoinHandle` is dropped on purpose: the thread is detached, see the type's
        // documentation.
        let state = shared.lock();
        // `wait_timeout_while` sleeps until `waiting` is false or `timeout` has passed in total,
        // handling spurious wake-ups itself. A poisoned lock is recovered as in `Shared::lock`.
        let (mut state, _) = shared
            .first_answer
            .wait_timeout_while(state, timeout, |state| state.waiting)
            .unwrap_or_else(PoisonError::into_inner);
        if state.waiting {
            // Too late (module rule 1): start with "no answer"; the reporter will see
            // `waiting == false` and deliver the late answer through the callback.
            state.waiting = false;
            state.delivered = None;
            state.last_error = Some(SystemAppearanceError::Timeout);
        }
        drop(state);
        Self { shared }
    }

    /// A watch that never answers and never changes: the source of a platform with no
    /// implementation yet (ADR-0038 S3) or of a front end that has no system to ask.
    ///
    /// Compiled only where it is used: on the platforms without an implementation, and in the
    /// tests (on Linux the portal source never needs it).
    #[cfg(any(not(target_os = "linux"), test))]
    pub(crate) fn no_answer() -> Self {
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                waiting: false,
                delivered: None,
                callback: None,
                last_error: None,
            }),
            first_answer: Condvar::new(),
        });
        Self { shared }
    }

    /// The system's preference as last delivered to the caller: the initial value, then the value
    /// of the latest callback. `None` means "no answer" (ADR-0037 T9 gives dark).
    ///
    /// Pass it to [`ThemeMode::resolve`](crate::ThemeMode::resolve). Must not be called from
    /// inside the callback (see [`AppearanceCallback`]).
    #[must_use]
    pub fn current(&self) -> Option<Appearance> {
        self.shared.lock().delivered
    }

    /// The most recent error, if any, for diagnostics (ADR-0038 S2). A later successful answer
    /// does not clear it: it tells what went wrong last, not whether the source works now.
    ///
    /// Must not be called from inside the callback (see [`AppearanceCallback`]).
    #[must_use]
    pub fn last_error(&self) -> Option<SystemAppearanceError> {
        self.shared.lock().last_error.clone()
    }
}

impl Drop for AppearanceWatch {
    /// Removes the callback, so it is never called again (module rule 4). The reporter needs the
    /// same lock to call it, so after this returns no call can be in progress or start.
    fn drop(&mut self) {
        self.shared.lock().callback = None;
    }
}

impl fmt::Debug for AppearanceWatch {
    /// Shows what the watch knows; the callback is a closure and has nothing to print.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let state = self.shared.lock();
        f.debug_struct("AppearanceWatch")
            .field("current", &state.delivered)
            .field("waiting", &state.waiting)
            .field("observing", &state.callback.is_some())
            .field("last_error", &state.last_error)
            .finish()
    }
}
