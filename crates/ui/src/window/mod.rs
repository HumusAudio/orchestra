// SPDX-License-Identifier: GPL-3.0-or-later

//! The window and the event loop (ADR-0040): the only module that knows `winit` and `softbuffer`.
//!
//! [`run`] opens a window and runs the event loop until the window is closed. What the window
//! shows is a [`WindowApp`]: it records its interface into a [`DisplayList`] when the window's
//! size changes ([`WindowApp::layout`]) and re-records the segments an input event changes
//! ([`WindowApp::input`]). The loop sleeps between events and draws a frame only when the list has
//! damage (W2), redrawing and presenting only the damaged regions (W4). It draws with the theme
//! pair and mode of [`WindowOptions`] and follows the system's light/dark preference while it runs
//! (ADR-0038 S3, step F2.04b).
//!
//! # Files
//! - `appearance`: following the system's preference, our proxy event, the theme in use (W3);
//! - `input`: our input events and their translation from `winit` (W5);
//! - `present`: from our RGBA frame to `softbuffer`'s buffer (W4);
//! - `order`: invisible, set up, shown (W7);
//! - `shell`: the event loop's handler.

pub(crate) mod appearance;
pub(crate) mod input;
pub(crate) mod order;
pub(crate) mod present;
mod shell;
#[cfg(test)]
mod tests;

use std::fmt;

use winit::event_loop::{ControlFlow, EventLoop};

use crate::{DisplayList, DrawError, Rect, ThemeMode, ThemePair};

/// What a window shows (ADR-0040 W1): the interface records itself through this trait and never
/// sees `winit`.
pub trait WindowApp {
    /// Records the whole interface for a surface of `bounds` (logical pixels, origin at the top
    /// left). Called when the window first has a size and after every resize or scale change; the
    /// whole surface is redrawn afterwards.
    fn layout(&mut self, bounds: Rect, list: &mut DisplayList);

    /// Handles one input event, re-recording the segments it changes. Only their area is redrawn.
    fn input(&mut self, event: &input::InputEvent, list: &mut DisplayList);

    /// The accessibility tree's update (ADR-0045 WA5, WC6): the whole tree when `full` (an
    /// assistive technology just connected), otherwise what changed since the last call. An app
    /// built on [`Ui`](crate::Ui) answers with [`Ui::take_access_update`](crate::Ui), after
    /// [`Ui::reset_access`](crate::Ui) when `full`. The default (`None`) leaves a tree with only
    /// the window in it.
    fn access_update(&mut self, full: bool) -> Option<crate::AccessUpdate> {
        let _ = full;
        None
    }

    /// Carries out an assistive technology's request, re-recording what it changes (an app built
    /// on [`Ui`](crate::Ui) calls [`Ui::access_action`](crate::Ui)). The default ignores it.
    fn access_action(&mut self, action: crate::AccessAction, list: &mut DisplayList) {
        let _ = (action, list);
    }
}

/// How to open the window.
#[derive(Debug, Clone)]
pub struct WindowOptions {
    /// The title, already localised by the caller (no text in this crate's code, CLAUDE.md §7).
    pub title: String,
    /// The theme pair to draw with (ADR-0037 T1): its light or dark half is chosen by `mode`.
    pub pair: ThemePair,
    /// The user's mode (ADR-0014 §1). With [`ThemeMode::System`] the window reads the system's
    /// preference when it opens and follows its changes while it is open (ADR-0038); with
    /// *Light* or *Dark* the system is not consulted for drawing.
    pub mode: ThemeMode,
    /// The initial width, logical pixels.
    pub width: f32,
    /// The initial height, logical pixels.
    pub height: f32,
    /// The bundled faces, parsed once by the application and lent to the window's renderer
    /// for the glyphs it draws (ADR-0044 TB2, TC1). The application keeps its own copy (a
    /// reference-count increment) to lay its texts out with the same faces.
    pub faces: orchestra_text::Faces,
}

/// Why the window could not run. Exhaustive (ADR-0034 D2). The details are the libraries'
/// English messages, for developers.
#[derive(Debug, Clone, PartialEq)]
pub enum WindowError {
    /// The event loop could not start (no display server reachable, for example).
    EventLoop {
        /// The library's description.
        detail: String,
    },
    /// The window could not be created.
    CreateWindow {
        /// The library's description.
        detail: String,
    },
    /// The surface for presenting pixels failed.
    Surface {
        /// The library's description.
        detail: String,
    },
    /// Drawing failed.
    Draw(DrawError),
}

impl fmt::Display for WindowError {
    /// One English line for developers.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EventLoop { detail } => write!(f, "the event loop could not start: {detail}"),
            Self::CreateWindow { detail } => write!(f, "the window could not be created: {detail}"),
            Self::Surface { detail } => write!(f, "presenting pixels failed: {detail}"),
            Self::Draw(error) => write!(f, "drawing failed: {error}"),
        }
    }
}

impl std::error::Error for WindowError {}

/// Opens a window showing `app` and runs the event loop until the window is closed (ADR-0040).
///
/// Blocks the calling thread, which must be the program's main thread (macOS requires it).
///
/// # Errors
/// [`WindowError`] when the loop, the window or the surface cannot be created, or drawing fails.
pub fn run<A: WindowApp>(app: A, options: WindowOptions) -> Result<(), WindowError> {
    // An event loop that also carries our own event (W3), so other threads (the portal's) can wake
    // it through a proxy.
    let event_loop = EventLoop::<appearance::ShellEvent>::with_user_event()
        .build()
        .map_err(|error| WindowError::EventLoop {
            detail: error.to_string(),
        })?;
    // Frames on demand (W2): sleep until the next event.
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut shell = shell::Shell::new(app, options, event_loop.create_proxy());
    event_loop
        .run_app(&mut shell)
        .map_err(|error| WindowError::EventLoop {
            detail: error.to_string(),
        })?;
    shell.error.map_or(Ok(()), Err)
}
