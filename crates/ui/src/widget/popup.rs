// SPDX-License-Identifier: GPL-3.0-or-later

//! Popups: what is drawn above the main interface (F2.11c, ADR-0048 AM11), and where it goes.
//!
//! # What a popup is
//! A popup is a widget subtree with a root of its own, in the same arena as the main interface
//! (owner's choice in F2.11c: focus, accessibility and wake-ups stay in one place), but not a child
//! of the main root: it is laid out on its own, at its natural size, next to an **anchor**, and
//! drawn in a **layer** of the display list above the main interface (a precision of ADR-0039 D1,
//! see [`DisplayList`](crate::DisplayList)). Two kinds exist:
//! - a **menu** ([`Ui::open_menu`](crate::Ui::open_menu)): it takes the pointer and the keyboard
//!   while it is open, and a press outside it closes every open menu and is consumed (WB9, as
//!   ADR-0048 AM11 decides). The menus themselves are written in F2.11d; this step gives them the
//!   layer, the placement and the routing;
//! - the **tooltip** (AM14), owned by the interface itself ([`super::tooltip`]), which takes no
//!   input.
//!
//! # Staying inside the window
//! `winit` 0.30 cannot open popup windows on Wayland (no `xdg_popup`), so popups are drawn inside
//! the main window and must fit in it: [`place`] puts a popup on its preferred side of the anchor,
//! on the other side when there is no room, and then pushes it inside the window. A popup larger
//! than the window is aligned with the window's top left corner and cut by the window's edge (a
//! limit recorded in ADR-0048 AM11).

use crate::draw::geometry::{Point, Rect};

use super::layout::Size;

/// Which side of its anchor a popup prefers (ADR-0048 AM11).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PopupSide {
    /// Below the anchor, left edges aligned: a menu under its title in the menu bar, a context
    /// menu at the pointer. Above the anchor when there is no room below.
    Below,
    /// To the right of the anchor, top edges aligned: a submenu beside its item. To the left
    /// when there is no room on the right.
    Right,
}

/// Where a menu opens (ADR-0048 AM11): next to `anchor`, on `side` if it fits.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Popup {
    /// The rectangle the popup opens next to, logical pixels of the window: a menu title, a
    /// menu item, or an empty rectangle at the pointer for a context menu.
    pub anchor: Rect,
    /// The side it prefers.
    pub side: PopupSide,
    /// The widget the popup belongs to, if any: a menu bar's title, the widget whose context menu
    /// it is. While the popup is open the pointer still reaches this widget (so moving over the
    /// next title of a menu bar can switch menus, F2.11d), a press on it does not close the menus,
    /// and it is told when the interface closes the popup by itself
    /// ([`WidgetEvent::MenuClosed`](crate::WidgetEvent::MenuClosed)).
    pub owner: Option<crate::WidgetId>,
}

/// Places a popup of `size` next to `anchor` on `side`, inside `bounds` (the window), and
/// answers its rectangle (ADR-0048 AM11).
///
/// # The rule
/// 1. On the preferred side if the popup fits there between the anchor and the window's edge.
/// 2. Otherwise on the opposite side if it fits there.
/// 3. Otherwise on whichever of the two sides has more room, and then pushed inside the window.
/// 4. Along the other axis it is aligned with the anchor (left edges for [`PopupSide::Below`],
///    top edges for [`PopupSide::Right`]) and pushed inside the window if it would cross its far
///    edge.
/// 5. A popup larger than the window on an axis starts at the window's edge on that axis and is
///    cut by the other edge: its start is always visible, as a menu's first items must be.
///
/// # Units and complexity
/// Logical pixels of the window throughout; O(1), no allocation.
#[must_use]
pub(crate) fn place(anchor: Rect, size: Size, side: PopupSide, bounds: Rect) -> Rect {
    match side {
        PopupSide::Below => {
            let y = across(anchor.y0, anchor.y1, size.height, bounds.y0, bounds.y1);
            let x = along(anchor.x0, size.width, bounds.x0, bounds.x1);
            Rect::new(x, y, x + size.width, y + size.height)
        }
        PopupSide::Right => {
            let x = across(anchor.x0, anchor.x1, size.width, bounds.x0, bounds.x1);
            let y = along(anchor.y0, size.height, bounds.y0, bounds.y1);
            Rect::new(x, y, x + size.width, y + size.height)
        }
    }
}

/// The start of a popup of `length` on the axis that crosses its anchor (rules 1–3 of
/// [`place`]): after the anchor's `end` if it fits before `max`, otherwise before the anchor's
/// `start` if it fits after `min`, otherwise on the roomier side, kept inside `min..max`.
fn across(start: f32, end: f32, length: f32, min: f32, max: f32) -> f32 {
    let after = max - end;
    let before = start - min;
    if length <= after {
        end
    } else if length <= before {
        start - length
    } else if after >= before {
        // Neither fits: the roomier side, pushed back inside the window as far as it can go.
        clamp_start(end, length, min, max)
    } else {
        clamp_start(start - length, length, min, max)
    }
}

/// The start of a popup of `length` on the axis along its anchor (rule 4 of [`place`]): aligned
/// with the anchor's `start`, kept inside `min..max`.
fn along(start: f32, length: f32, min: f32, max: f32) -> f32 {
    clamp_start(start, length, min, max)
}

/// `start` moved so that `start..start + length` lies inside `min..max` when it can, and so that
/// it begins at `min` when it is too long for that (rule 5 of [`place`]).
fn clamp_start(start: f32, length: f32, min: f32, max: f32) -> f32 {
    // First back from the far edge, then forward from the near one: when the popup is longer
    // than the window, the second step wins and its start stays visible.
    let start = if start + length > max {
        max - length
    } else {
        start
    };
    start.max(min)
}

/// The anchor of a tooltip: an empty rectangle at the pointer's hot spot, `TOOLTIP_DROP` tall,
/// so that [`place`] puts the tooltip that far below the pointer (clear of the pointer's arrow,
/// whose size the system does not tell us) and, when there is no room below, right above the
/// hot spot (ADR-0048 AM14).
///
/// # Why the hot spot is rounded to whole pixels
/// The pointer's position is fractional (logical pixels at any scale), and a tooltip placed at a
/// fraction would put its text at a new sub-pixel phase almost every time, so its glyphs would be
/// rasterised again instead of coming from the glyph cache (ADR-0044, a quarter-pixel phase per
/// glyph). Whole logical pixels are whole device pixels at the usual scales 1 and 2, so the box's
/// edges are sharp too. Half a pixel of difference is invisible for a tooltip.
pub(crate) fn tooltip_anchor(pointer: Point) -> Rect {
    let (x, y) = (pointer.x.round(), pointer.y.round());
    Rect::new(x, y, x, y + TOOLTIP_DROP)
}

/// How far below the pointer's hot spot a tooltip starts, logical pixels (choice 3 of F2.11c):
/// enough for the usual 16–24 px arrow of the three platforms' default pointer themes not to
/// cover the tooltip's first line.
pub(crate) const TOOLTIP_DROP: f32 = 20.0;

#[cfg(test)]
mod tests {
    use super::*;

    /// The window of these tests: 400 × 300 logical pixels.
    const WINDOW: Rect = Rect::new(0.0, 0.0, 400.0, 300.0);

    #[test]
    fn below_fits_under_its_anchor() {
        let anchor = Rect::new(10.0, 0.0, 60.0, 24.0);
        let rect = place(anchor, Size::new(120.0, 80.0), PopupSide::Below, WINDOW);
        assert_eq!(rect, Rect::new(10.0, 24.0, 130.0, 104.0));
    }

    #[test]
    fn below_flips_above_without_room() {
        // The anchor near the bottom: 30 px below it, 250 above.
        let anchor = Rect::new(10.0, 250.0, 60.0, 270.0);
        let rect = place(anchor, Size::new(120.0, 80.0), PopupSide::Below, WINDOW);
        assert_eq!(rect, Rect::new(10.0, 170.0, 130.0, 250.0));
    }

    #[test]
    fn below_is_pushed_left_from_the_right_edge() {
        let anchor = Rect::new(350.0, 0.0, 380.0, 24.0);
        let rect = place(anchor, Size::new(120.0, 80.0), PopupSide::Below, WINDOW);
        assert_eq!(rect.x1, 400.0, "its right edge on the window's");
        assert_eq!(rect.x0, 280.0);
    }

    #[test]
    fn right_flips_left_without_room() {
        // A submenu beside an item near the right edge.
        let anchor = Rect::new(250.0, 40.0, 350.0, 64.0);
        let rect = place(anchor, Size::new(120.0, 80.0), PopupSide::Right, WINDOW);
        assert_eq!(rect, Rect::new(130.0, 40.0, 250.0, 120.0));
    }

    #[test]
    fn right_is_pushed_up_from_the_bottom_edge() {
        let anchor = Rect::new(10.0, 280.0, 110.0, 300.0);
        let rect = place(anchor, Size::new(120.0, 80.0), PopupSide::Right, WINDOW);
        assert_eq!(rect, Rect::new(110.0, 220.0, 230.0, 300.0));
    }

    #[test]
    fn neither_side_fits_takes_the_roomier_one_inside_the_window() {
        // 200 px tall: 100 below the anchor, 80 above. Below is roomier, pushed up to fit.
        let anchor = Rect::new(10.0, 80.0, 60.0, 200.0);
        let rect = place(anchor, Size::new(50.0, 200.0), PopupSide::Below, WINDOW);
        assert_eq!(rect.y1, 300.0);
        assert_eq!(rect.y0, 100.0);
    }

    #[test]
    fn larger_than_the_window_starts_at_its_edge() {
        let anchor = Rect::new(10.0, 10.0, 60.0, 30.0);
        let rect = place(anchor, Size::new(500.0, 400.0), PopupSide::Below, WINDOW);
        assert_eq!((rect.x0, rect.y0), (0.0, 0.0), "the start stays visible");
        assert_eq!(
            (rect.width(), rect.height()),
            (500.0, 400.0),
            "cut, not shrunk"
        );
    }

    #[test]
    fn a_window_with_an_offset_origin_is_respected() {
        let window = Rect::new(100.0, 50.0, 300.0, 250.0);
        let anchor = Rect::new(280.0, 60.0, 290.0, 70.0);
        let rect = place(anchor, Size::new(50.0, 40.0), PopupSide::Below, window);
        assert_eq!(rect, Rect::new(250.0, 70.0, 300.0, 110.0));
    }

    #[test]
    fn tooltip_sits_below_the_pointer_or_right_above_it() {
        let size = Size::new(100.0, 30.0);
        let below = place(
            tooltip_anchor(Point::new(50.0, 50.0)),
            size,
            PopupSide::Below,
            WINDOW,
        );
        assert_eq!(below.y0, 50.0 + TOOLTIP_DROP);
        let above = place(
            tooltip_anchor(Point::new(50.0, 290.0)),
            size,
            PopupSide::Below,
            WINDOW,
        );
        assert_eq!(above.y1, 290.0, "its bottom at the hot spot");
    }
}
