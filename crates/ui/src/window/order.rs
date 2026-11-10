// SPDX-License-Identifier: GPL-3.0-or-later

//! The order of window creation (ADR-0036 B9, ADR-0040 W7): created invisible, then set up, then
//! shown.
//!
//! AccessKit's adapter panics when it is created after the window was first shown, and drawing
//! the first frame before showing avoids a flash of an empty window. Both need a moment between
//! "the window exists" and "the user sees it". This file is that moment, generic over the window
//! type so a test can check the order with a fake window, without a display.

/// A window that can be made visible.
pub(crate) trait Showable {
    /// Makes the window visible.
    fn show(&self);
}

impl Showable for std::rc::Rc<winit::window::Window> {
    /// `winit`'s `set_visible(true)`.
    fn show(&self) {
        self.set_visible(true);
    }
}

/// Creates a window with `create_hidden` (which must create it invisible), runs `setup` on it,
/// and only then shows it. If creating fails, nothing else runs.
///
/// # Errors
/// Whatever `create_hidden` returns.
pub(crate) fn open_in_order<W, E, S>(
    create_hidden: impl FnOnce() -> Result<W, E>,
    setup: impl FnOnce(&W) -> S,
) -> Result<(W, S), E>
where
    W: Showable,
{
    let window = create_hidden()?;
    let prepared = setup(&window);
    window.show();
    Ok((window, prepared))
}
