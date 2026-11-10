// SPDX-License-Identifier: GPL-3.0-or-later

//! Windows, macOS and every non-Linux system: a source **fed by the event loop** (ADR-0038 S3).
//!
//! # Why the event loop, and why "fed"
//! On Windows and macOS the right source is `winit`: `ActiveEventLoop::system_theme()` gives the
//! initial value (the registry value `AppsUseLightTheme` on Windows, `NSApp.effectiveAppearance`
//! on macOS, exactly as ADR-0014 §5 asks), and `WindowEvent::ThemeChanged` reports each change.
//! No polling, no thread of ours, no new dependency.
//!
//! But both answers exist only **inside a running event loop**, and `winit` lives only in the
//! `window` module (ADR-0040 W1). So this source does not ask anything itself: the window's event
//! loop reads `winit` and **feeds** what it learns through the [`Reporter`] that
//! [`FedSource::new`] returns. Everything else is the same machinery as the Linux portal's: the
//! rules 1–4 of the parent module (repeats dropped, the callback stopped by dropping the watch)
//! apply unchanged, because the reporter is the same type. The window then receives every change
//! of every platform through one path: the callback, forwarded to the event loop with
//! `EventLoopProxy` (ADR-0040 W3).
//!
//! # Other systems
//! On systems that are neither Linux, Windows nor macOS (the BSDs, for example; not supported
//! targets today, but they must not fail to build) `winit` answers `None` and never sends
//! `ThemeChanged`, so this source simply says "no answer" (dark, ADR-0037 T9) for the whole
//! session, with no error: nothing failed, there is just nothing to ask.
//!
//! # Compiled where
//! Everywhere except Linux, where the portal is the source (ADR-0038 S2) and `winit` never sends
//! `ThemeChanged`; and in the tests on every platform, which check it with a hand-fed reporter.

use std::sync::{Arc, Condvar, Mutex};

use super::{AppearanceCallback, AppearanceWatch, Reporter, Shared, State, SystemAppearanceSource};
use crate::Appearance;

/// A source whose answers are fed from outside, by the window's event loop (module documentation).
pub(crate) struct FedSource {
    /// The observation's state, shared with the [`Reporter`] returned by [`FedSource::new`] and
    /// with the [`AppearanceWatch`] returned by `observe`.
    shared: Arc<Shared>,
}

impl FedSource {
    /// A source whose first answer is `initial`, and the reporter that feeds its later answers.
    ///
    /// The first answer is already known (the event loop just read it), so nothing waits: the
    /// state starts with `waiting == false` and `delivered == initial`. A later
    /// [`Reporter::answer`] equal to the last delivered value is a repeat and is dropped; a
    /// different one goes to the callback installed by `observe` (rule 2 of the parent module).
    ///
    /// The reporter may be used before `observe`: an answer then only updates the value that
    /// [`AppearanceWatch::current`] will return, because there is no callback yet to call.
    pub(crate) fn new(initial: Option<Appearance>) -> (Self, Reporter) {
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                waiting: false,
                delivered: initial,
                callback: None,
                last_error: None,
            }),
            // Never waited on: `waiting` is already false. Kept so the shared type is one.
            first_answer: Condvar::new(),
        });
        let reporter = Reporter {
            shared: Arc::clone(&shared),
        };
        (Self { shared }, reporter)
    }
}

impl SystemAppearanceSource for FedSource {
    /// Installs `on_change` and returns at once: the first answer was given to [`FedSource::new`].
    ///
    /// Meant to be called once per source. A second call replaces the first callback, and both
    /// watches then share one state (dropping either removes the callback): there is no use for
    /// that, it is only documented so the behaviour is not a surprise.
    fn observe(&self, on_change: AppearanceCallback) -> AppearanceWatch {
        self.shared.lock().callback = Some(on_change);
        AppearanceWatch {
            shared: Arc::clone(&self.shared),
        }
    }
}
