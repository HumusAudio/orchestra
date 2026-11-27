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
//! # Timed wake-ups (F2.10b2, a precision of W2)
//! Frames on demand stay the rule: the loop waits for the next event. When the app wants to be
//! woken at a time without an event (a text field's blinking caret, ADR-0047 TF8), it says so
//! with [`WindowApp::next_wakeup`], asked after every batch of events; the loop then sleeps until
//! that time (`ControlFlow::WaitUntil`) or until an event, whichever comes first. At the start of
//! each batch, timed or not, the app receives the time ([`WindowApp::advance`]), so its widgets
//! never read the clock (and are tested with made-up times). Without a wake-up the loop goes back
//! to waiting: an idle window, caret included once it stopped blinking, uses no processor time.
//!
//! # Files
//! - `appearance`: following the system's preference, our proxy event, the theme in use (W3);
//! - `input`: our input events and their translation from `winit` (W5);
//! - `present`: from our RGBA frame to `softbuffer`'s buffer (W4);
//! - `order`: invisible, set up, shown (W7);
//! - `pointer`: the pointer's shape and its translation to `winit` (F2.09e);
//! - `shell`: the event loop's handler.

pub(crate) mod appearance;
pub(crate) mod input;
pub(crate) mod order;
pub(crate) mod pointer;
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

    /// The shape the pointer should have now (F2.09e). The window asks after every input event,
    /// layout and assistive-technology action, and changes the system's pointer only when the
    /// answer differs from the last one, so answering is the whole job of the app: an app built
    /// on [`Ui`](crate::Ui) answers [`Ui::pointer_shape`](crate::Ui). The default is the arrow.
    ///
    /// # Cost
    /// Called once per event, so it must be cheap and must not allocate: `Ui`'s answer walks from
    /// the hovered widget to the root, O(depth of the tree), asking each widget a rectangle test.
    fn pointer_shape(&self) -> pointer::PointerShape {
        pointer::PointerShape::Default
    }

    /// Tells the app the time, at the start of every batch of events and when its wake-up came
    /// (see the module documentation), re-recording what that changes (a caret that blinked). An
    /// app built on [`Ui`](crate::Ui) calls [`Ui::advance`](crate::Ui::advance). The default
    /// ignores it.
    ///
    /// # Cost
    /// Called before every batch of events, so it must be cheap when nothing is due: `Ui`'s
    /// answer scans its few pending wake-ups and runs a frame only if one came.
    fn advance(&mut self, now: std::time::Instant, list: &mut DisplayList) {
        let _ = (now, list);
    }

    /// Receives the system's clipboard (ADR-0047 TF12, F2.10c1): called once, before the first
    /// layout, when the window could connect to it. The window creates one clipboard for the
    /// whole session and the application keeps it; an app built on [`Ui`](crate::Ui) passes it
    /// on with [`Ui::set_clipboard`](crate::Ui::set_clipboard). Not called when there is no
    /// system clipboard: the interface then keeps its own, in memory. The default drops it.
    fn attach_clipboard(&mut self, clipboard: Box<dyn crate::ClipboardSource>) {
        let _ = clipboard;
    }

    /// Receives the window's [`WindowWaker`] (ADR-0054 AR1, a precision of ADR-0040 W3 written in
    /// F2.14a): called once, before the first layout. An app whose work finishes on another
    /// thread (a project read or written by its worker) keeps it there and calls
    /// [`WindowWaker::wake`] when an answer is ready; the window then starts a batch, so the app
    /// finds the answer in its next [`advance`](Self::advance). The default drops it.
    fn attach_waker(&mut self, waker: WindowWaker) {
        let _ = waker;
    }

    /// When the app wants to be woken without an event (an [`advance`](Self::advance) at that
    /// time), or `None` to wait for the next event (the default). Asked after every batch of
    /// events; an app built on [`Ui`](crate::Ui) answers [`Ui::next_wakeup`](crate::Ui).
    fn next_wakeup(&self) -> Option<std::time::Instant> {
        None
    }

    /// What the window must do with the system's input method now (ADR-0047 TF5, TF6; F2.10c2):
    /// `Some` while a widget that takes text has the focus, with its caret's area, and `None`
    /// otherwise (the default). An app built on [`Ui`](crate::Ui) answers
    /// [`Ui::text_input`](crate::Ui::text_input).
    ///
    /// The window asks after every input event, layout and assistive-technology action, and
    /// calls the system only when the answer changes: it turns the input method on or off, moves
    /// the candidate window to the new area, or ends the composition when
    /// [`TextInput::reset`](crate::TextInput::reset) changed (precision of ADR-0040 W5 and of
    /// ADR-0047 TF5, written in F2.10c2).
    ///
    /// # Cost
    /// Called once per event, so it must be cheap and must not allocate.
    fn text_input(&self) -> Option<crate::TextInput> {
        None
    }

    /// What the app asks of the window itself (a precision of ADR-0040 written in F2.11d1): to
    /// close, or to enter or leave full screen, after an action asked for it (Quit, Full screen:
    /// ADR-0048 AM6). The window asks after every window event and keeps asking until `None`
    /// (the default), so the app hands its requests out one by one.
    ///
    /// # Cost
    /// Called once per event: answer from a stored queue, without allocating.
    fn take_window_request(&mut self) -> Option<WindowRequest> {
        None
    }

    /// The program's menus for the system's menu bar (macOS, F2.11e1, ADR-0048 AM9), usually
    /// [`Ui::system_menu`](crate::Ui::system_menu). Asked once, when the window opens; `None` (the
    /// default) installs nothing, and on Linux and Windows nothing is installed anyway (the app
    /// draws its bar, ADR-0036 B5).
    fn system_menu(&mut self) -> Option<crate::SystemMenu> {
        None
    }

    /// Runs `action`, which the user chose in the system's menu bar (F2.11e1), as a shortcut or a
    /// drawn menu would ([`Ui::run_action`](crate::Ui::run_action)), re-recording what changed.
    /// Nothing by default.
    fn run_action(&mut self, action: crate::ActionId, list: &mut DisplayList) {
        let _ = (action, list);
    }

    /// Hands to `out` the actions of the system's menu whose state changed since the last call
    /// ([`Ui::take_menu_updates`](crate::Ui::take_menu_updates)), so the window shows them there
    /// (AM2). Asked after every batch of events while a system menu is installed; nothing by
    /// default.
    fn take_menu_updates(&mut self, out: &mut Vec<(crate::ActionId, crate::ActionState)>) {
        let _ = out;
    }

    /// A file chooser the app wants opened (ADR-0050 DG4, F2.12a5a), usually
    /// [`Ui::take_file_request`](crate::Ui::take_file_request). Asked after every batch of events;
    /// `None` (the default) when there is none. The window starts one chooser at a time: a
    /// request while one is open is dropped (the interface's blocking layer keeps it from
    /// happening).
    ///
    /// # Cost
    /// Called once per batch: answer from a stored slot, without allocating.
    fn take_file_request(&mut self) -> Option<crate::FileRequest> {
        None
    }

    /// The answer of the chooser started for the last request (F2.12a5a), re-recording what
    /// changed: an app built on [`Ui`](crate::Ui) calls [`Ui::file_chosen`](crate::Ui::file_chosen)
    /// to remove its blocking layer, then acts on the answer. Ignored by default.
    fn file_chosen(&mut self, choice: crate::FileChoice, list: &mut DisplayList) {
        let _ = (choice, list);
    }

    /// The user asked to close the window (a precision of ADR-0040 written in F2.12a5b): the
    /// system's close button, Alt+F4, the window menu's *Close*, or our title bar's close button
    /// ([`WindowRequest::RequestClose`]). `true` closes it now and ends the event loop (the
    /// default); `false` keeps it open, re-recording what the app showed instead: a question
    /// about unsaved changes (ADR-0050 DG5), whose answer later closes it with
    /// [`WindowRequest::Close`]. Never asked while a file chooser is open: the window is then
    /// kept open anyway (choice 4 of F2.12a5a).
    ///
    /// # Why the app decides, not the window
    /// Only the app knows whether there is something to lose. The window cannot wait for an
    /// answer itself (nothing may block the event loop), so a refusal now and a `Close` later is
    /// the only shape that keeps the loop running while the question is open.
    fn close_requested(&mut self, list: &mut DisplayList) -> bool {
        let _ = list;
        true
    }

    /// The window's title now (F2.12a5b: the project's name, with a mark while it has unsaved
    /// changes, ADR-0050 DG5), usually [`Ui::title`](crate::Ui::title). Asked after every batch
    /// of events; the window gives it to the system only when it differs from the last one it
    /// gave. `None` (the default) keeps [`WindowOptions::title`](crate::WindowOptions), the one it
    /// opened with.
    ///
    /// # Cost
    /// Called once per batch: answer with a borrowed string, without allocating. The window
    /// compares it with its own copy, O(title length).
    fn title(&self) -> Option<&str> {
        None
    }
}

/// A borrowed app is an app ([`run`] borrows it): every method is forwarded unchanged.
impl<A: WindowApp + ?Sized> WindowApp for &mut A {
    fn layout(&mut self, bounds: Rect, list: &mut DisplayList) {
        (**self).layout(bounds, list);
    }

    fn input(&mut self, event: &input::InputEvent, list: &mut DisplayList) {
        (**self).input(event, list);
    }

    fn access_update(&mut self, full: bool) -> Option<crate::AccessUpdate> {
        (**self).access_update(full)
    }

    fn access_action(&mut self, action: crate::AccessAction, list: &mut DisplayList) {
        (**self).access_action(action, list);
    }

    fn pointer_shape(&self) -> pointer::PointerShape {
        (**self).pointer_shape()
    }

    fn advance(&mut self, now: std::time::Instant, list: &mut DisplayList) {
        (**self).advance(now, list);
    }

    fn attach_clipboard(&mut self, clipboard: Box<dyn crate::ClipboardSource>) {
        (**self).attach_clipboard(clipboard);
    }

    fn attach_waker(&mut self, waker: WindowWaker) {
        (**self).attach_waker(waker);
    }

    fn next_wakeup(&self) -> Option<std::time::Instant> {
        (**self).next_wakeup()
    }

    fn text_input(&self) -> Option<crate::TextInput> {
        (**self).text_input()
    }

    fn take_window_request(&mut self) -> Option<WindowRequest> {
        (**self).take_window_request()
    }

    fn system_menu(&mut self) -> Option<crate::SystemMenu> {
        (**self).system_menu()
    }

    fn run_action(&mut self, action: crate::ActionId, list: &mut DisplayList) {
        (**self).run_action(action, list);
    }

    fn take_menu_updates(&mut self, out: &mut Vec<(crate::ActionId, crate::ActionState)>) {
        (**self).take_menu_updates(out);
    }

    fn take_file_request(&mut self) -> Option<crate::FileRequest> {
        (**self).take_file_request()
    }

    fn file_chosen(&mut self, choice: crate::FileChoice, list: &mut DisplayList) {
        (**self).file_chosen(choice, list);
    }

    fn close_requested(&mut self, list: &mut DisplayList) -> bool {
        (**self).close_requested(list)
    }

    fn title(&self) -> Option<&str> {
        (**self).title()
    }
}

/// Wakes the window's event loop from another thread (ADR-0054 AR1, a precision of ADR-0040 W3
/// written in F2.14a), handed to the app with [`WindowApp::attach_waker`].
///
/// # How it works
/// [`wake`](Self::wake) sends an empty event through the loop's proxy, the same channel the file
/// chooser's and the system appearance's answers already take. The loop then starts a batch:
/// [`WindowApp::advance`] runs first (where the app looks at what its threads answered), and
/// after the event the window takes the app's requests, its pointer shape and its damage, as
/// after any event of the app. Waking a loop that is already awake costs one more, empty, batch.
///
/// # Why an empty event and not the answer itself
/// The answers belong to the app (a project's document, its errors): carrying them in the window's
/// own event type would make `orchestra-ui` know the app's types, against the crates' order
/// (CLAUDE.md §2). The app keeps its own channel; the window only needs to be told to look.
///
/// # Threads
/// `Clone` and `Send`: every thread that may finish work keeps its own copy. Once the loop has
/// ended, [`wake`](Self::wake) does nothing.
#[derive(Clone)]
pub struct WindowWaker {
    /// The event loop's proxy (ADR-0040 W3).
    proxy: winit::event_loop::EventLoopProxy<appearance::ShellEvent>,
}

impl WindowWaker {
    /// The waker of the loop `proxy` belongs to.
    pub(crate) const fn new(
        proxy: winit::event_loop::EventLoopProxy<appearance::ShellEvent>,
    ) -> Self {
        Self { proxy }
    }

    /// Asks the event loop for a batch (see the type's documentation). Never blocks: `winit`
    /// queues the event and wakes the loop. O(1).
    pub fn wake(&self) {
        // An error only says the loop has ended: there is nobody left to wake.
        let _closed = self.proxy.send_event(appearance::ShellEvent::Wake);
    }
}

impl fmt::Debug for WindowWaker {
    /// The proxy has nothing worth printing.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("WindowWaker")
    }
}

/// Something the app asks of its window ([`WindowApp::take_window_request`]). `PartialEq` only:
/// the window menu's position is a point of `f32`s.
#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(
    clippy::large_enum_variant,
    reason = "`SetTheme` carries a whole pair (F2.12a4a): requests are a few per user action and \
              live in a short queue, so the size costs nothing, while boxing would end `Copy`"
)]
pub enum WindowRequest {
    /// Close the window and end the event loop, now: the app has decided (Quit with nothing to
    /// save, or after the answer to "Save the changes?", ADR-0050 DG5).
    Close,
    /// Ask to close, as the system's close button does (our title bar's close button, F2.12a5b):
    /// the window asks the app with [`WindowApp::close_requested`] and closes only if it agrees.
    RequestClose,
    /// Enter full screen when windowed, leave it when in full screen (Full screen). Borderless
    /// on the current monitor: the system's video mode is never changed.
    ToggleFullScreen,
    /// Move the window with the pointer until the primary button is released (our title bar,
    /// ADR-0049 TB6). Must follow a press of that button closely: the compositor checks the
    /// press it belongs to. Ignored where the system draws the frame.
    Drag,
    /// Maximise the window when it is not, restore it when it is (the title bar's button and its
    /// double click, ADR-0049 TB6).
    ToggleMaximized,
    /// Minimise the window (the title bar's button, ADR-0049 TB6).
    Minimize,
    /// Show the system's window menu (move, resize, always on top…) at `at`, logical pixels from
    /// the window's top left (a secondary click on our title bar, ADR-0049 TB6).
    ShowWindowMenu {
        /// Where, logical pixels.
        at: crate::Point,
    },
    /// Resize the window from `edge` with the pointer until the primary button is released (the
    /// edges of our own frame, ADR-0049 TB7, F2.11b3). Must follow that press closely, like
    /// [`WindowRequest::Drag`]. Ignored where the system draws the frame.
    Resize(ResizeEdge),
    /// Draw with `pair` in `mode` from now on (the Preferences, ADR-0050 DG9, F2.12a4a): the
    /// window redraws everything when what it shows changed (another pair or another half; layout
    /// does not depend on colours, so nothing is re-recorded). With *Light* or *Dark* it also
    /// asks the system to give its own decorations that half (`winit`'s `set_theme`: the title
    /// bar of Windows and macOS); back to *System* it asks for none, because while a half is
    /// forced those systems stop reporting their changes (the note of F2.04b). The application
    /// applies the pair's density, text and shape tokens to its own interface itself
    /// ([`Ui::set_theme`](crate::Ui::set_theme)): the window draws colours only.
    SetTheme {
        /// The pair (ADR-0037 T1).
        pair: ThemePair,
        /// The mode (ADR-0014 §1).
        mode: ThemeMode,
    },
    /// Build the system's menu bar again from [`WindowApp::system_menu`] and install it in place
    /// of the one shown (macOS, ADR-0053 LR1, F2.13): the menus' names changed with the language.
    /// The states come with the new description, so nothing else is needed. Nothing happens where
    /// no system menu is installed (Linux, Windows: the app renames its drawn bar itself,
    /// [`Ui::set_menu_bar_model`](crate::Ui::set_menu_bar_model)).
    RebuildSystemMenu,
}

/// An edge or a corner of the window, to resize it from ([`WindowRequest::Resize`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResizeEdge {
    /// The top edge.
    North,
    /// The bottom edge.
    South,
    /// The right edge.
    East,
    /// The left edge.
    West,
    /// The top-right corner.
    NorthEast,
    /// The top-left corner.
    NorthWest,
    /// The bottom-right corner.
    SouthEast,
    /// The bottom-left corner.
    SouthWest,
}

impl ResizeEdge {
    /// The pointer's shape over this edge or corner.
    #[must_use]
    pub const fn pointer_shape(self) -> pointer::PointerShape {
        use pointer::PointerShape;
        match self {
            Self::East | Self::West => PointerShape::ResizeHorizontal,
            Self::North | Self::South => PointerShape::ResizeVertical,
            Self::NorthWest | Self::SouthEast => PointerShape::ResizeNorthWestSouthEast,
            Self::NorthEast | Self::SouthWest => PointerShape::ResizeNorthEastSouthWest,
        }
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
    /// Who draws the window's frame (ADR-0049 TB1): usually what [`decorations`](crate::decorations)
    /// answered before the application built its interface, so both agree. With
    /// [`Decorations::Own`](crate::Decorations::Own) the window is created without decorations
    /// and the application draws its title bar.
    pub decorations: crate::Decorations,
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
/// # Why the app is borrowed (F2.12a4a, owner's choice 2)
/// The caller keeps the app, so what it owns outlives the window: the program closes its settings
/// file after the loop (writing what is still waiting), whatever way the loop ended. A
/// `&mut A` is itself a [`WindowApp`] (the forwarding implementation below), so the window's
/// code is the same as with an owned app.
///
/// # Errors
/// [`WindowError`] when the loop, the window or the surface cannot be created, or drawing fails.
pub fn run<A: WindowApp>(app: &mut A, options: WindowOptions) -> Result<(), WindowError> {
    // An event loop that also carries our own event (W3), so other threads (the portal's) can wake
    // it through a proxy.
    #[cfg_attr(
        not(target_os = "macos"),
        allow(unused_mut, reason = "changed only on macOS, below")
    )]
    let mut builder = EventLoop::<appearance::ShellEvent>::with_user_event();
    // On macOS `winit` installs a menu bar of its own unless told not to; the program's comes
    // from the app through `muda` (F2.11e1, ADR-0048 AM9), and two would fight over the bar.
    #[cfg(target_os = "macos")]
    {
        use winit::platform::macos::EventLoopBuilderExtMacOS;
        builder.with_default_menu(false);
    }
    let event_loop = builder.build().map_err(|error| WindowError::EventLoop {
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
