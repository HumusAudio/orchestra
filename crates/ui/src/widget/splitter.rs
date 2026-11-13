// SPDX-License-Identifier: GPL-3.0-or-later

//! The splitter (F2.09d, choice WF6): two panes side by side (or one above the other) with a
//! divider between them that the user moves.
//!
//! # The value
//! The splitter's value is the **share of the space that goes to the first pane**, a fraction
//! from 0 to 1 (announced as a percentage to assistive technologies). A fraction rather than a
//! width in pixels, so that both panes grow and shrink together when the window is resized. Each
//! pane keeps a minimum size (48 logical pixels by default); when the space cannot give both their
//! minimum, it is shared in proportion to the minimums.
//!
//! # Behaviour (ADR-0046, the value-control conventions, V1)
//! The divider is a value control, so it follows V3–V6, with one exception to V2 (below):
//! - **Drag** (with capture): the divider follows the pointer; with the primary modifier held
//!   (Ctrl, ⌘ on macOS) it moves a tenth as fast (V3), re-anchored when the modifier changes so
//!   it never jumps. The press does not move the divider (the pointer is already on it).
//!   **Exception to the grid of V1**: a drag is not snapped to the step, so the divider stays
//!   under the pointer pixel by pixel; the step applies to the wheel and the keyboard.
//! - **Double click**: back to the default share (V4), the initial one unless the application
//!   gives another with [`Splitter::default_fraction`].
//! - **Wheel: exception to V2.** The wheel does not move the divider; it goes on to the area
//!   around (WB7). Why: the divider's band is a few pixels wide and the first notch would move it
//!   out from under the pointer, so the next notch would land on a pane: turning the wheel would
//!   move the divider once and then scroll whatever is below. The keyboard, the drag and the
//!   double click cover what the wheel would do. (Found by this step's tests, F2.09d.)
//! - **Tab order**: between the panes (after the first pane's widgets, before the second's), as
//!   the reader sees the divider.
//! - **Keyboard**, when the divider has the focus (V6): the arrows move the coarse step (one step
//!   with the primary modifier; right and down give more space to the first pane), Page Up and
//!   Page Down ten steps, Home and End to the smallest and the largest share allowed. Keys
//!   pressed inside the panes are not the splitter's: it reacts only while focused itself.
//! - **Assistive technologies**: a `Splitter` node with the share in percent, its limits and
//!   step, the orientation of the divider; increase, decrease and set the value.
//!
//! Each change tells the application, if it asked ([`Splitter::on_change`]), with the new share:
//! saving it with the settings comes with F2.12.
//!
//! # Looks
//! The divider is a band as wide as the density's small gap (at least 6 logical pixels, so it can
//! be grabbed) with a one-pixel `border` line along its middle. Under the pointer the line turns
//! `control-border`, a neutral shade a little stronger, and stays one pixel; while dragged it
//! becomes two pixels of `accent`. The focus ring surrounds the band.
//!
//! Why so quiet under the pointer (owner's choice, F2.09e): the double-arrow pointer already says
//! "this can be dragged", and three pixels of accent on a mere hover were too loud; the accent is
//! kept for the action in progress. Until F2.09e the hover showed the same three pixels of accent
//! as the drag.
//!
//! # The pointer (F2.09e)
//! Over the band, and during a drag wherever the pointer goes, the pointer takes the resize
//! shape of the divider's direction: [`PointerShape::ResizeColumn`] for panes side by side,
//! [`PointerShape::ResizeRow`] for panes one above the other. Over the panes the splitter says
//! nothing, so their own widgets (or the arrow) decide.

use crate::draw::geometry::{Point, Rect};
use crate::draw::list::Recorder;
use crate::theme::token::ColorToken;
use crate::window::input::{ButtonState, Key, NamedKey, PointerButton};
use crate::window::pointer::PointerShape;

use super::access::{AccessInfo, AccessRole};
use super::layout::{Axis, Constraints, Size};
use super::tree::{EventCx, Handled, LayoutCx, RecordCx, Widget, WidgetEvent};

/// The step of the share, as a fraction of the space (V1's grid for the wheel and the keyboard):
/// half a percent, about five logical pixels in a 1000-pixel window.
const STEP: f64 = 0.005;

/// The coarse step (ADR-0046 V2, used by the arrows, V6): 2% of the range, which is the whole
/// space.
const COARSE: f64 = 0.02;

/// How much slower a fine drag is (ADR-0046 V3).
const FINE_DRAG: f64 = 0.1;

/// The default minimum size of each pane, logical pixels.
const DEFAULT_MIN: f32 = 48.0;

/// The narrowest the divider's band gets, logical pixels: thinner is hard to grab.
const MIN_HANDLE: f32 = 6.0;

/// A drag of the divider in progress: the share follows the pointer relatively to an anchor, set
/// at the press and again whenever the fine modifier changes (as the slider's, ADR-0046 V3).
#[derive(Debug, Clone, Copy)]
struct Drag {
    /// The pointer's coordinate along the axis at the anchor, logical pixels of the window.
    pointer: f32,
    /// The share at the anchor.
    fraction: f64,
    /// Whether this stretch is fine.
    fine: bool,
    /// The pointer's coordinate at the last move, the next anchor's.
    last: f32,
}

/// Two panes and a divider (F2.09d, WF6). Add the two panes as its children (the first is the
/// left or top one; more children are ignored).
#[derive(Debug)]
pub struct Splitter<M> {
    /// `Horizontal`: panes side by side; `Vertical`: one above the other.
    axis: Axis,
    /// The first pane's share of the space, 0–1, as last set (before the minimums apply).
    fraction: f64,
    /// The share a double click goes back to (V4).
    default: f64,
    /// The smallest size of the first and of the second pane, logical pixels.
    minimum: (f32, f32),
    /// The message built from the new share at each change, if the application wants one.
    message: Option<fn(f64) -> M>,
    /// The name assistive technologies announce, if any.
    name: Option<String>,
    /// The divider's band at the last arrange, logical pixels of the window.
    handle: Rect,
    /// The space shared by the panes at the last arrange (the length along the axis less the
    /// band), logical pixels.
    space: f32,
    /// The pointer is over the band.
    hover: bool,
    /// The splitter has the keyboard focus (keys bubbling from the panes are not its own).
    focused: bool,
    /// The drag in progress, if any.
    drag: Option<Drag>,
}

impl<M> Splitter<M> {
    /// Two panes along `axis` (`Horizontal`: side by side), the first taking `fraction` of the
    /// space (clamped to 0–1; NaN becomes one half).
    #[must_use]
    pub fn new(axis: Axis, fraction: f64) -> Self {
        let fraction = if fraction.is_nan() {
            0.5
        } else {
            fraction.clamp(0.0, 1.0)
        };
        Self {
            axis,
            fraction,
            default: fraction,
            minimum: (DEFAULT_MIN, DEFAULT_MIN),
            message: None,
            name: None,
            handle: Rect::EMPTY,
            space: 0.0,
            hover: false,
            focused: false,
            drag: None,
        }
    }

    /// The same splitter, each pane at least `first` and `second` logical pixels.
    #[must_use]
    pub const fn minimum_sizes(mut self, first: f32, second: f32) -> Self {
        self.minimum = (first.max(0.0), second.max(0.0));
        self
    }

    /// The same splitter, going back to `fraction` on a double click (V4).
    #[must_use]
    pub fn default_fraction(mut self, fraction: f64) -> Self {
        if !fraction.is_nan() {
            self.default = fraction.clamp(0.0, 1.0);
        }
        self
    }

    /// The same splitter, telling the application each new share with `message`.
    #[must_use]
    pub fn on_change(mut self, message: fn(f64) -> M) -> Self {
        self.message = Some(message);
        self
    }

    /// The same splitter, named for assistive technologies (already localised).
    #[must_use]
    pub fn labelled(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// The first pane's share as set, 0–1 (the panes may show less or more while the minimums
    /// apply).
    #[must_use]
    pub const fn fraction(&self) -> f64 {
        self.fraction
    }

    /// Sets the first pane's share (clamped to 0–1); the caller asks the interface to lay the
    /// splitter out again.
    pub fn set_fraction(&mut self, fraction: f64) {
        if !fraction.is_nan() {
            self.fraction = fraction.clamp(0.0, 1.0);
        }
    }

    /// The smallest and the largest share the minimums allow in `space`: equal (the minimums'
    /// proportion) when the space cannot give both their minimum.
    fn limits(&self, space: f32) -> (f64, f64) {
        let (first, second) = self.minimum;
        if space <= 0.0 {
            return (0.0, 1.0);
        }
        if first + second >= space {
            let total = first + second;
            let share = if total > 0.0 {
                f64::from(first / total)
            } else {
                0.5
            };
            return (share, share);
        }
        (f64::from(first / space), f64::from(1.0 - second / space))
    }

    /// The share the panes show in `space`: the one set, within the limits.
    fn effective(&self, space: f32) -> f64 {
        let (low, high) = self.limits(space);
        self.fraction.clamp(low, high)
    }

    /// Moves to `fraction` (within the limits of the last layout); tells the application and asks
    /// to be arranged again only if the share changed.
    fn change(&mut self, cx: &mut EventCx<'_, M>, fraction: f64) {
        if fraction.is_nan() {
            return;
        }
        let (low, high) = self.limits(self.space);
        let fraction = fraction.clamp(low, high);
        if fraction.to_bits() != self.fraction.to_bits() {
            self.fraction = fraction;
            if let Some(message) = self.message {
                cx.emit(message(fraction));
            }
            cx.request_arrange();
        }
    }

    /// The coordinate of `point` along the axis.
    const fn along(&self, point: Point) -> f32 {
        match self.axis {
            Axis::Horizontal => point.x,
            Axis::Vertical => point.y,
        }
    }

    /// The step of an arrow key: the coarse step, or one step when fine (V3).
    const fn notch(fine: bool) -> f64 {
        if fine { STEP } else { COARSE }
    }

    /// A left press at `point`: on the band, the second press of a double click restores the
    /// default (V4) and starts no drag; another press starts a drag.
    fn press(&mut self, cx: &mut EventCx<'_, M>, point: Point, clicks: u8) -> Handled {
        if !inside(self.handle, point) {
            return Handled::No;
        }
        if clicks == 2 {
            self.change(cx, self.default);
            return Handled::Yes;
        }
        let at = self.along(point);
        self.drag = Some(Drag {
            pointer: at,
            fraction: self.effective(self.space),
            fine: cx.modifiers().primary,
            last: at,
        });
        cx.request_record();
        Handled::Yes
    }

    /// The pointer moved to `point` during a drag: the share follows, at the plain or the fine
    /// speed, re-anchored when the modifier changes (V3).
    fn drag_to(&mut self, cx: &mut EventCx<'_, M>, mut drag: Drag, point: Point) {
        let at = self.along(point);
        let fine = cx.modifiers().primary;
        if fine != drag.fine {
            drag = Drag {
                pointer: drag.last,
                fraction: self.fraction,
                fine,
                last: drag.last,
            };
        }
        let speed = if fine { FINE_DRAG } else { 1.0 };
        let space = f64::from(self.space.max(1.0));
        let target = drag.fraction + f64::from(at - drag.pointer) * speed / space;
        drag.last = at;
        self.drag = Some(drag);
        self.change(cx, target);
    }

    /// The thickness of the divider's band for the theme's small gap.
    fn handle_for(gap: f32) -> f32 {
        gap.max(MIN_HANDLE)
    }

    /// Splits `rect` into the first pane, the band and the second pane for the current share.
    fn split(&self, rect: Rect, band: f32) -> (Rect, Rect, Rect, f32) {
        let length = match self.axis {
            Axis::Horizontal => rect.width(),
            Axis::Vertical => rect.height(),
        };
        let space = (length - band).max(0.0);
        // The first pane on whole logical pixels, so the band and its line stay crisp.
        #[allow(
            clippy::cast_possible_truncation,
            reason = "a share 0–1 of a length in logical pixels: f32 is plenty"
        )]
        let first = (self.effective(space) * f64::from(space)).round() as f32;
        let first = first.clamp(0.0, space);
        let (a, h, b) = match self.axis {
            Axis::Horizontal => {
                let split = rect.x0 + first;
                (
                    Rect::new(rect.x0, rect.y0, split, rect.y1),
                    Rect::new(split, rect.y0, split + band, rect.y1),
                    Rect::new(split + band, rect.y0, rect.x1, rect.y1),
                )
            }
            Axis::Vertical => {
                let split = rect.y0 + first;
                (
                    Rect::new(rect.x0, rect.y0, rect.x1, split),
                    Rect::new(rect.x0, split, rect.x1, split + band),
                    Rect::new(rect.x0, split + band, rect.x1, rect.y1),
                )
            }
        };
        (a, h, b, space)
    }
}

/// `true` when `point` is inside `rect` (left and top edges in, right and bottom out).
fn inside(rect: Rect, point: Point) -> bool {
    point.x >= rect.x0 && point.x < rect.x1 && point.y >= rect.y0 && point.y < rect.y1
}

impl<M: std::fmt::Debug + 'static> Widget<M> for Splitter<M> {
    fn measure(&mut self, cx: &mut LayoutCx<'_, M>, constraints: Constraints) -> Size {
        let band = Self::handle_for(cx.spacing().gap_s);
        let max = constraints.max;
        if max.width.is_finite() && max.height.is_finite() {
            // A splitter takes all the space it is given: the panes share it (they are measured
            // tight to their part when the splitter is arranged).
            return constraints.constrain(max);
        }
        // Unbounded (inside a scroll area): the panes' own sizes, plus the band.
        let mut size = Size::ZERO;
        for child in 0..cx.child_count().min(2) {
            let pane = cx.measure_child(child, Constraints::loose(max));
            size = match self.axis {
                Axis::Horizontal => {
                    Size::new(size.width + pane.width, size.height.max(pane.height))
                }
                Axis::Vertical => Size::new(size.width.max(pane.width), size.height + pane.height),
            };
        }
        let size = match self.axis {
            Axis::Horizontal => Size::new(size.width + band, size.height),
            Axis::Vertical => Size::new(size.width, size.height + band),
        };
        constraints.constrain(size)
    }

    fn arrange(&mut self, cx: &mut LayoutCx<'_, M>, rect: Rect) {
        let band = Self::handle_for(cx.spacing().gap_s);
        let (first, handle, second, space) = self.split(rect, band);
        self.handle = handle;
        self.space = space;
        for (child, pane) in [first, second].into_iter().enumerate() {
            if child < cx.child_count() {
                // Each pane exactly its part: measured tight (answered from its cache when the
                // part did not change), then placed.
                let _ = cx.measure_child(
                    child,
                    Constraints::tight(Size::new(pane.width(), pane.height())),
                );
                cx.place_child(child, pane);
            }
        }
    }

    fn record(&self, cx: &RecordCx, recorder: &mut Recorder<'_>) {
        // Dragged: the accent, two pixels (the action in progress). Hovered: one pixel of the
        // neutral control edge (the pointer's shape already tells it can be dragged). At rest:
        // the thin separator.
        let (width, token) = if self.drag.is_some() {
            (2.0, ColorToken::Accent)
        } else if self.hover {
            (1.0, ColorToken::ControlBorder)
        } else {
            (1.0, ColorToken::Border)
        };
        let band = self.handle;
        // The line along the middle of the band, on whole pixels.
        let line = match self.axis {
            Axis::Horizontal => {
                let x = ((band.x0 + band.x1 - width) / 2.0).round();
                Rect::new(x, band.y0, x + width, band.y1)
            }
            Axis::Vertical => {
                let y = ((band.y0 + band.y1 - width) / 2.0).round();
                Rect::new(band.x0, y, band.x1, y + width)
            }
        };
        recorder.fill_rect(line, token);
        if cx.focused {
            // Just inside the splitter, around the band (outside it would be cut by a parent
            // that the splitter fills).
            super::look::focus_ring(recorder, band.inflate(-1.0), 0.0, cx.shape);
        }
    }

    fn event(&mut self, cx: &mut EventCx<'_, M>, event: &WidgetEvent<'_>) -> Handled {
        match *event {
            WidgetEvent::PointerButton {
                button: PointerButton::Left,
                state: ButtonState::Pressed,
                position,
                clicks,
            } => self.press(cx, position, clicks),
            WidgetEvent::PointerButton {
                button: PointerButton::Left,
                state: ButtonState::Released,
                position,
                ..
            } => {
                if self.drag.take().is_some() {
                    self.hover = inside(self.handle, position);
                    cx.request_record();
                    Handled::Yes
                } else {
                    Handled::No
                }
            }
            WidgetEvent::PointerMoved { position } => {
                if let Some(drag) = self.drag {
                    self.drag_to(cx, drag, position);
                    return Handled::Yes;
                }
                let hover = inside(self.handle, position);
                if hover != self.hover {
                    self.hover = hover;
                    cx.request_record();
                }
                if hover { Handled::Yes } else { Handled::No }
            }
            WidgetEvent::PointerLeft => {
                if self.hover {
                    self.hover = false;
                    cx.request_record();
                }
                Handled::Yes
            }
            WidgetEvent::FocusGained => {
                self.focused = true;
                Handled::Yes
            }
            WidgetEvent::FocusLost => {
                self.focused = false;
                Handled::Yes
            }
            WidgetEvent::Key {
                key: Key::Named(key),
                state: ButtonState::Pressed,
                modifiers,
                ..
            } if self.focused => {
                let notch = Self::notch(modifiers.primary);
                let (low, high) = self.limits(self.space);
                let current = self.effective(self.space);
                let target = match key {
                    NamedKey::ArrowRight | NamedKey::ArrowDown => current + notch,
                    NamedKey::ArrowLeft | NamedKey::ArrowUp => current - notch,
                    NamedKey::PageDown => current + 10.0 * STEP,
                    NamedKey::PageUp => current - 10.0 * STEP,
                    NamedKey::Home => low,
                    NamedKey::End => high,
                    _ => return Handled::No,
                };
                self.change(cx, target);
                Handled::Yes
            }
            WidgetEvent::Increment => {
                self.change(cx, self.effective(self.space) + STEP);
                Handled::Yes
            }
            WidgetEvent::Decrement => {
                self.change(cx, self.effective(self.space) - STEP);
                Handled::Yes
            }
            WidgetEvent::SetValue(percent) => {
                self.change(cx, percent / 100.0);
                Handled::Yes
            }
            _ => Handled::No,
        }
    }

    fn focusable(&self) -> bool {
        true
    }

    fn tab_after_child(&self) -> Option<usize> {
        // Between the panes, where the divider is: first pane, divider, second pane.
        Some(0)
    }

    fn pointer_shape(&self, point: Point) -> Option<PointerShape> {
        // During a drag the shape stays whatever the pointer is over (the interface asks only the
        // capturing widget then, but the band test alone would drop the shape as soon as the
        // pointer outruns the divider at a limit). Otherwise only over the band.
        if self.drag.is_none() && !inside(self.handle, point) {
            return None;
        }
        // The divider moves along the axis the panes are laid along: left and right between
        // panes side by side (resizing columns), up and down between panes one above the other
        // (resizing rows).
        Some(match self.axis {
            Axis::Horizontal => PointerShape::ResizeColumn,
            Axis::Vertical => PointerShape::ResizeRow,
        })
    }

    fn access_role(&self) -> AccessRole {
        AccessRole::Splitter
    }

    fn access_info(&self, info: &mut AccessInfo) {
        let (low, high) = self.limits(self.space);
        info.label.clone_from(&self.name);
        info.value = Some(self.effective(self.space) * 100.0);
        info.range = Some((low * 100.0, high * 100.0, STEP * 100.0));
        info.orientation = Some(self.axis);
    }
}
