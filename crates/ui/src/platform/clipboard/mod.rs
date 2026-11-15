// SPDX-License-Identifier: GPL-3.0-or-later

//! The clipboard, text only (ADR-0047 TF12, step F2.10c1).
//!
//! # What this module gives the rest of the interface
//!
//! - [`ClipboardSource`] is the one trait: read the text, write a text. Widgets reach it through
//!   their event context ([`EventCx::clipboard`](crate::EventCx::clipboard)) and never know what
//!   is behind it.
//! - [`MemoryClipboard`] keeps the text in the program's own memory. It is what an interface has
//!   until it is given another one, so no test ever needs a display (CLAUDE.md §3), and copying
//!   and pasting still work inside the program when the system's clipboard cannot be reached.
//! - `SystemClipboard` (in `system`) is the system's clipboard, through `arboard`. The window
//!   creates **one** for the whole session and hands it to the application
//!   ([`WindowApp::attach_clipboard`](crate::WindowApp::attach_clipboard)): on Linux the program
//!   that copied serves every later paste while it lives, so a short-lived object would lose
//!   what was copied.
//!
//! # Why a trait, when `arboard` already hides the platforms
//!
//! To keep `arboard` in one file (as AccessKit and `winit` are): if GNOME users meet problems
//! with the X11 fallback, another implementation on `smithay-clipboard` can replace it without
//! touching a widget (ADR-0047 TF12, variant C); and the primary selection of Linux, postponed
//! by the owner, will be added here.
//!
//! # Not on the audio thread
//!
//! Reading can block for a moment (an X11 round trip to the program that owns the clipboard).
//! It happens only when the user pastes, on the interface thread.

pub(crate) mod system;

use std::fmt;

/// Why the clipboard could not be read or written. Exhaustive (ADR-0034 D2). The details are the
/// library's English messages, for developers: nothing here is shown to the user (a paste that
/// fails pastes nothing, ADR-0047's precision of F2.10c1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClipboardError {
    /// There is no clipboard to talk to: no display server, or one without a clipboard this
    /// program can use.
    Unavailable {
        /// The library's description.
        detail: String,
    },
    /// The clipboard is there but the request failed (held by another program, a conversion
    /// that did not work, a protocol error).
    Failed {
        /// The library's description.
        detail: String,
    },
}

impl fmt::Display for ClipboardError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable { detail } => write!(f, "no clipboard is available: {detail}"),
            Self::Failed { detail } => write!(f, "the clipboard request failed: {detail}"),
        }
    }
}

impl std::error::Error for ClipboardError {}

/// A clipboard holding text (ADR-0047 TF12).
///
/// ```
/// use orchestra_ui::{ClipboardSource, MemoryClipboard};
///
/// let mut clipboard = MemoryClipboard::default();
/// assert_eq!(clipboard.read_text(), Ok(None));
/// clipboard.write_text("Violin").unwrap();
/// assert_eq!(clipboard.read_text(), Ok(Some("Violin".to_owned())));
/// ```
pub trait ClipboardSource: fmt::Debug {
    /// The text on the clipboard, exactly as it is there (a field cleans what it pastes);
    /// `None` when the clipboard is empty or holds something that is not text (an image).
    ///
    /// # Errors
    /// [`ClipboardError`] when the clipboard could not be asked.
    fn read_text(&mut self) -> Result<Option<String>, ClipboardError>;

    /// Puts `text` on the clipboard, replacing what was there.
    ///
    /// # Errors
    /// [`ClipboardError`] when the clipboard could not be written; what it held is then unknown
    /// (usually unchanged).
    fn write_text(&mut self, text: &str) -> Result<(), ClipboardError>;
}

/// A clipboard in the program's own memory: nothing leaves the program and nothing comes in.
/// The clipboard of an interface that was given no other, and of every test.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MemoryClipboard {
    /// The text held, if any.
    text: Option<String>,
}

impl ClipboardSource for MemoryClipboard {
    fn read_text(&mut self) -> Result<Option<String>, ClipboardError> {
        Ok(self.text.clone())
    }

    fn write_text(&mut self, text: &str) -> Result<(), ClipboardError> {
        // The old text's buffer is reused when there is one.
        match &mut self.text {
            Some(held) => text.clone_into(held),
            None => self.text = Some(text.to_owned()),
        }
        Ok(())
    }
}
