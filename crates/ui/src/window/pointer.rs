// SPDX-License-Identifier: GPL-3.0-or-later

//! The pointer's shape (F2.09e, a precision of ADR-0045 WA3 and ADR-0040 W1): our own type, which
//! widgets answer with, and its translation to `winit`'s, which only the window uses.
//!
//! # Why our own type
//! The same reason as our input events (ADR-0040 W5) and our accessibility roles (WC2): widgets
//! never see `winit`, so a `winit` update, or another windowing backend one day, touches this file
//! only. The translation is a pure function, tested without a display.
//!
//! # Why so few shapes
//! Only the shapes a widget uses today are here (SCALETTA §1, rule 6: no work "for later"): the
//! ordinary arrow, the two resize shapes of the splitter's divider and the text field's text beam
//! (F2.10b2). The shapes of the window's edges and corners come with our own title bar
//! (F2.11b). Buttons keep the arrow, as on Windows, macOS, GNOME and KDE: the pointing hand is
//! the convention of links, and none exist yet (owner's choice, 2026-11-13).

use winit::window::CursorIcon;

/// The shape the pointer takes over a widget (F2.09e).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum PointerShape {
    /// The system's ordinary arrow: everywhere no widget asks for something else.
    #[default]
    Default,
    /// Resizing **columns**: a boundary that moves left and right, such as the divider between
    /// two panes side by side (a horizontal splitter).
    ResizeColumn,
    /// Resizing **rows**: a boundary that moves up and down, such as the divider between two
    /// panes one above the other (a vertical splitter).
    ResizeRow,
    /// The text beam (an I-bar): over text that can be edited or selected, such as a text field
    /// (ADR-0047 TF4, F2.10b2).
    Text,
}

/// `winit`'s cursor for `shape`.
///
/// # Why `ColResize` and `RowResize`, not `EwResize` and `NsResize`
/// They are the CSS names of a *divider between columns or rows*, which is what our shapes mean.
/// `winit` maps them to the native shape on every platform: on Windows to the west-east and
/// north-south arrows (`IDC_SIZEWE`, `IDC_SIZENS`), on macOS to `resizeLeftRight` and
/// `resizeUpDown`, on X11 and Wayland to the cursor theme's `col-resize` and `row-resize`, which
/// the common themes (Adwaita, Breeze) draw as a split bar with arrows, the look desktop
/// applications use for pane dividers. Where a theme lacks them, the theme's own fallbacks apply
/// (cursor themes alias the missing names), so the pointer never disappears.
pub(crate) const fn to_winit(shape: PointerShape) -> CursorIcon {
    match shape {
        PointerShape::Default => CursorIcon::Default,
        PointerShape::ResizeColumn => CursorIcon::ColResize,
        PointerShape::ResizeRow => CursorIcon::RowResize,
        // `Text` is the I-beam everywhere: `IDC_IBEAM` on Windows, `IBeamCursor` on macOS, the
        // cursor theme's `text` (aliased `xterm`) on X11 and Wayland.
        PointerShape::Text => CursorIcon::Text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every shape has its own `winit` cursor, and the default is the arrow.
    #[test]
    fn each_shape_maps_to_its_native_cursor() {
        assert_eq!(to_winit(PointerShape::Default), CursorIcon::Default);
        assert_eq!(to_winit(PointerShape::ResizeColumn), CursorIcon::ColResize);
        assert_eq!(to_winit(PointerShape::ResizeRow), CursorIcon::RowResize);
        assert_eq!(to_winit(PointerShape::Text), CursorIcon::Text);
        assert_eq!(PointerShape::default(), PointerShape::Default);
    }
}
