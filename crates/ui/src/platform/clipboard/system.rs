// SPDX-License-Identifier: GPL-3.0-or-later

//! The system's clipboard through `arboard` (ADR-0047 TF12, variant B): the only file that knows
//! that crate.
//!
//! `arboard` picks the platform itself: the pasteboard on macOS, the Win32 clipboard on Windows;
//! on Linux the `data-control` protocol where the Wayland compositor offers it (KDE Plasma, Sway,
//! Hyprland, COSMIC, niri) and X11 otherwise, which on GNOME means through XWayland. So this file
//! has no `cfg`: it only translates two calls and their errors.
//!
//! # Not tested in CI
//! Creating it needs a display and another program to exchange text with; no test may need a
//! display (CLAUDE.md §3). Tests use [`MemoryClipboard`](super::MemoryClipboard) through the same
//! trait, and this file is checked by hand (ROADMAP, F2.10c1).

use std::fmt;

use super::{ClipboardError, ClipboardSource};

/// The system's clipboard. One object for the whole session, owned by the window and then by the
/// application it is handed to (see the parent module).
pub(crate) struct SystemClipboard {
    /// `arboard`'s handle. On Linux it keeps a thread that serves what this program copied to
    /// the programs that paste it, for as long as it lives.
    inner: arboard::Clipboard,
}

impl SystemClipboard {
    /// Connects to the system's clipboard.
    ///
    /// # Errors
    /// [`ClipboardError::Unavailable`] when there is none to connect to (no display server); the
    /// caller then keeps the in-memory clipboard.
    pub(crate) fn new() -> Result<Self, ClipboardError> {
        arboard::Clipboard::new()
            .map(|inner| Self { inner })
            .map_err(|error| ClipboardError::Unavailable {
                detail: error.to_string(),
            })
    }
}

// `arboard::Clipboard` has no `Debug`; its state is the system's anyway.
impl fmt::Debug for SystemClipboard {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SystemClipboard").finish_non_exhaustive()
    }
}

/// Our error for one of `arboard`'s, for a request on a clipboard that exists.
fn failed(error: &arboard::Error) -> ClipboardError {
    match error {
        arboard::Error::ClipboardNotSupported => ClipboardError::Unavailable {
            detail: error.to_string(),
        },
        // `arboard::Error` is non-exhaustive: everything else, known or new, is a failed request.
        _ => ClipboardError::Failed {
            detail: error.to_string(),
        },
    }
}

impl ClipboardSource for SystemClipboard {
    fn read_text(&mut self) -> Result<Option<String>, ClipboardError> {
        match self.inner.get_text() {
            Ok(text) => Ok(Some(text)),
            // Empty, or holding something that is not text: nothing to paste, not an error.
            Err(arboard::Error::ContentNotAvailable) => Ok(None),
            Err(error) => Err(failed(&error)),
        }
    }

    fn write_text(&mut self, text: &str) -> Result<(), ClipboardError> {
        self.inner.set_text(text).map_err(|error| failed(&error))
    }
}
