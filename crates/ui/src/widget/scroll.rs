// SPDX-License-Identifier: GPL-3.0-or-later

//! The scroll area (F2.09d, choices WF1–WF5, WF7): a viewport on one child larger than itself,
//! with scroll bars, the wheel, the keyboard and assistive technologies.
//!
//! # How it scrolls (WF1: "A today, B only if measured")
//! Widget rectangles are in logical pixels of the window (WB5), so scrolling means **placing the
//! content again** at the new offset: the area asks to be arranged again
//! ([`EventCx::request_arrange`], no measuring), the content and its descendants get their new
//! rectangles and are recorded again, each inside the viewport (the area answers it as its
//! [`Widget::child_clip`]; the interface clips drawing and hit testing to it). The cost of a
//! scroll step is therefore proportional to the widgets inside, plus redrawing the viewport,
//! which any design pays.
//!
//! # Sizes
//! The content is measured by its own size rules (`LayoutProps`, as any parent does): `Content`
//! with no limit in the directions that scroll and up to the viewport in the others (so a
//! vertical area wraps its content to its width); `Fill` at least as large as the viewport;
//! `Fixed` exactly. A scroll bar appears
//! in a direction only when the content is larger than the viewport there; it takes room from
//! the viewport (classic bars, always visible when needed, WF4), which may make the content
//! larger than the viewport in the other direction too: the fitting loop runs until the answer
//! no longer changes (at most three rounds, because bars are only ever added). The area asks for
//! its content's size plus its bars, at most the space given; a parent that gives it more (a
//! `Fill` rule) gets a larger viewport, and the content is then placed in a rectangle at least as
//! large as the viewport.
//!
//! # Padding for focus rings
//! The viewport clips everything inside it, focus rings included, and a ring is drawn just
//! outside its control. Content whose controls touch its edges must therefore leave a padding of
//! at least the ring's reach (one pixel plus the `focus-width` token; the density's small gap is
//! enough), or the ring of a control at the edge is cut.
//!
//! # Input
//! - **Wheel** (WF3): a notch scrolls three lines (a line is the density's control height); a
//!   touchpad's pixels scroll as they come. Shift turns a vertical wheel into a horizontal one
//!   in an area that scrolls horizontally; an area that only scrolls vertically keeps it
//!   vertical, so Shift+wheel over a control always scrolls the panel around it (ADR-0046 V5). At an edge, the part of the movement that cannot be used bubbles: the area
//!   says it did not handle the event when it could not move at all, so an area around it
//!   scrolls instead.
//! - **Scroll bars** (WF4): dragging the thumb (with capture) moves the content proportionally;
//!   a press on the track outside the thumb moves one page towards the press.
//! - **Keyboard** (WF5): arrows (a line), Page Up and Page Down (a viewport less a line, so one
//!   line stays in sight), Home and End (the top and the bottom, or the left and right edges for
//!   an area that only scrolls horizontally). They work when the area has the focus (only if made
//!   [`focusable`](ScrollArea::focusable)) and when a key pressed on a widget inside it is not
//!   handled there and bubbles up (WB7). A key that would not move the area bubbles further.
//! - **Focus** (WF5): when the keyboard focus moves to a widget hidden in the area, the interface
//!   asks the area to bring it into view ([`Widget::scroll_into_view`]).
//! - **Assistive technologies** (WF7): a `ScrollView` node with the offsets and their limits, and
//!   the actions to scroll by a line or a page.
//!
//! # Looks
//! The bars sit along the right and the bottom edges, as thick as the density's medium gap (12
//! logical pixels comfortable, 8 compact, at least 8): a `surface-sunken` track and a rounded
//! `control-border` thumb, `text-muted` while hovered or dragged. The corner where two bars meet
//! is `surface-sunken` too.

use crate::draw::geometry::{Point, Rect, RoundedRect};
use crate::draw::list::Recorder;
use crate::theme::density::Spacing;
use crate::theme::token::ColorToken;
use crate::window::input::{ButtonState, Key, NamedKey, PointerButton, WheelDelta};

use super::access::{AccessInfo, AccessRole, ScrollInfo};
use super::layout::{Constraints, Length, Size};
use super::tree::{EventCx, Handled, LayoutCx, RecordCx, ScrollDirection, Widget, WidgetEvent};

/// How many lines one wheel notch scrolls (WF3): the usual desktop default.
const LINES_PER_NOTCH: f32 = 3.0;

/// The shortest a thumb gets, logical pixels: long content must still leave something to grab.
const MIN_THUMB: f32 = 24.0;

/// The gap between a thumb and the edges of its track, logical pixels on each side.
const THUMB_INSET: f32 = 2.0;

/// A size difference below this is not a reason for a scroll bar, logical pixels: content that
/// is as large as the viewport up to rounding must not get a bar for a hundredth of a pixel.
const FIT_EPSILON: f32 = 0.5;

/// One of the two scroll bars.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Bar {
    /// Along the right edge: it scrolls vertically.
    Vertical,
    /// Along the bottom edge: it scrolls horizontally.
    Horizontal,
}

/// A thumb being dragged: where the pointer and the offset were at the press.
#[derive(Debug, Clone, Copy, PartialEq)]
struct ThumbDrag {
    /// Which bar.
    bar: Bar,
    /// The pointer's coordinate along the bar at the press, logical pixels of the window.
    pointer: f32,
    /// The offset along the bar at the press.
    offset: f32,
}

/// A viewport on one child larger than itself (F2.09d). Add the content as its only child (more
/// children are ignored).
#[derive(Debug)]
pub struct ScrollArea {
    /// Scrolls horizontally.
    scroll_x: bool,
    /// Scrolls vertically.
    scroll_y: bool,
    /// Takes the keyboard focus itself (for content with nothing focusable in it).
    focusable: bool,
    /// How far the content is scrolled: from its top left corner to the viewport's, logical
    /// pixels, between zero and [`ScrollArea::max_offset`]. Kept exact; the content is placed on
    /// whole logical pixels (text stays crisp).
    offset: Point,
    /// The content's size at the last arrange, at least the viewport's.
    content: Size,
    /// The viewport at the last arrange, logical pixels of the window: the area less its bars.
    viewport: Rect,
    /// The area's own rectangle at the last arrange.
    rect: Rect,
    /// Which bars were shown at the last arrange: (vertical, horizontal).
    bars: (bool, bool),
    /// The bars' thickness at the last arrange (from the density).
    thickness: f32,
    /// One line, logical pixels (the density's control height) at the last arrange.
    line: f32,
    /// The thumb under the pointer, if any.
    hover: Option<Bar>,
    /// The thumb being dragged, if any.
    drag: Option<ThumbDrag>,
}

impl ScrollArea {
    /// An area that scrolls vertically only: its content is as wide as the viewport.
    #[must_use]
    pub const fn vertical() -> Self {
        Self::new(false, true)
    }

    /// An area that scrolls horizontally only: its content is as high as the viewport.
    #[must_use]
    pub const fn horizontal() -> Self {
        Self::new(true, false)
    }

    /// An area that scrolls in both directions.
    #[must_use]
    pub const fn both() -> Self {
        Self::new(true, true)
    }

    /// An area scrolling in the directions given.
    const fn new(scroll_x: bool, scroll_y: bool) -> Self {
        Self {
            scroll_x,
            scroll_y,
            focusable: false,
            offset: Point::new(0.0, 0.0),
            content: Size::ZERO,
            viewport: Rect::EMPTY,
            rect: Rect::EMPTY,
            bars: (false, false),
            thickness: 0.0,
            line: 0.0,
            hover: None,
            drag: None,
        }
    }

    /// The same area, taking the keyboard focus itself: for content with nothing focusable in it
    /// (a long text), which could otherwise not be scrolled from the keyboard.
    #[must_use]
    pub const fn focusable(mut self) -> Self {
        self.focusable = true;
        self
    }

    /// How far the content is scrolled, logical pixels (zero at the top left).
    #[must_use]
    pub const fn offset(&self) -> Point {
        self.offset
    }

    /// The largest offset in each direction at the last layout (zero where nothing scrolls).
    #[must_use]
    pub fn max_offset(&self) -> Point {
        Point::new(
            if self.scroll_x {
                (self.content.width - self.viewport.width()).max(0.0)
            } else {
                0.0
            },
            if self.scroll_y {
                (self.content.height - self.viewport.height()).max(0.0)
            } else {
                0.0
            },
        )
    }

    /// The viewport at the last layout, logical pixels of the window.
    #[must_use]
    pub const fn viewport(&self) -> Rect {
        self.viewport
    }

    /// Scrolls to `offset` (clamped at the next layout, when the content's size is known); the
    /// caller asks the interface to lay the area out again ([`Ui::request_layout`]).
    ///
    /// [`Ui::request_layout`]: crate::Ui::request_layout
    pub const fn set_offset(&mut self, offset: Point) {
        self.offset = offset;
    }

    /// The bars' thickness for the theme's spacing: the medium gap, at least 8 logical pixels
    /// (thinner is hard to grab).
    fn thickness_for(spacing: Spacing) -> f32 {
        spacing.gap_m.max(8.0)
    }

    /// Measures the content for an area of `space` and decides which bars it needs: returns the
    /// content's size and the bars (vertical, horizontal). See "Sizes" in the module
    /// documentation.
    ///
    /// # Complexity
    /// At most three measures of the content (bars are only ever added, two of them), each one
    /// answered from the content's cache when its constraints repeat.
    fn fit<M: 'static>(&self, cx: &mut LayoutCx<'_, M>, space: Size) -> (Size, bool, bool) {
        let mut bars = (false, false);
        let mut content = Size::ZERO;
        for _ in 0..3 {
            let viewport = self.viewport_size(space, bars);
            content = if cx.child_count() == 0 {
                Size::ZERO
            } else {
                let props = cx.child_props(0);
                let (min_w, max_w) = rule(props.width, viewport.width, self.scroll_x);
                let (min_h, max_h) = rule(props.height, viewport.height, self.scroll_y);
                cx.measure_child(
                    0,
                    Constraints {
                        min: Size::new(min_w, min_h),
                        max: Size::new(max_w, max_h),
                    },
                )
            };
            let need = (
                bars.0 || (self.scroll_y && content.height > viewport.height + FIT_EPSILON),
                bars.1 || (self.scroll_x && content.width > viewport.width + FIT_EPSILON),
            );
            if need == bars {
                break;
            }
            bars = need;
        }
        (content, bars.0, bars.1)
    }

    /// The viewport's size in an area of `space` with `bars` (vertical, horizontal) shown.
    fn viewport_size(&self, space: Size, bars: (bool, bool)) -> Size {
        let less = |shown: bool| if shown { self.thickness } else { 0.0 };
        Size::new(
            (space.width - less(bars.0)).max(0.0),
            (space.height - less(bars.1)).max(0.0),
        )
    }

    /// Moves the offset by (`dx`, `dy`), clamped; `true` if it changed.
    fn scroll_by(&mut self, dx: f32, dy: f32) -> bool {
        let max = self.max_offset();
        let next = Point::new(
            (self.offset.x + dx).clamp(0.0, max.x),
            (self.offset.y + dy).clamp(0.0, max.y),
        );
        if next == self.offset {
            return false;
        }
        self.offset = next;
        true
    }

    /// Scrolls by (`dx`, `dy`) on behalf of an event: asks to be arranged again if it moved, and
    /// answers whether it did (an area that cannot move lets the event bubble).
    fn scroll_event<M>(&mut self, cx: &mut EventCx<'_, M>, dx: f32, dy: f32) -> Handled {
        if self.scroll_by(dx, dy) {
            cx.request_arrange();
            Handled::Yes
        } else {
            Handled::No
        }
    }

    /// A page in each direction: the viewport less a line, so one line of context stays in view
    /// (at least one line).
    fn page(&self) -> Size {
        Size::new(
            (self.viewport.width() - self.line).max(self.line),
            (self.viewport.height() - self.line).max(self.line),
        )
    }

    /// The wheel (WF3): lines become three lines each, pixels are used as they are; Shift turns
    /// a vertical movement into a horizontal one where the area scrolls horizontally (ADR-0046
    /// V5). Away from the user (positive)
    /// shows what is above, so the offset decreases.
    fn wheel<M>(&mut self, cx: &mut EventCx<'_, M>, delta: WheelDelta) -> Handled {
        let (mut dx, mut dy) = match delta {
            WheelDelta::Lines { x, y } => (
                x * LINES_PER_NOTCH * self.line,
                y * LINES_PER_NOTCH * self.line,
            ),
            WheelDelta::Pixels { x, y } => (x, y),
        };
        if cx.modifiers().shift && dx == 0.0 && self.scroll_x {
            // Only where there is somewhere to go horizontally: an area that scrolls vertically
            // only keeps Shift+wheel vertical, because a slider inside has let the wheel go
            // precisely so that the area can still scroll (ADR-0046 V5).
            (dx, dy) = (dy, 0.0);
        }
        self.scroll_event(cx, -dx, -dy)
    }

    /// The track of `bar` at the last layout (along the right or the bottom edge, as long as the
    /// viewport), or `None` if that bar is not shown.
    fn track(&self, bar: Bar) -> Option<Rect> {
        match bar {
            Bar::Vertical if self.bars.0 => Some(Rect::new(
                self.viewport.x1,
                self.rect.y0,
                self.rect.x1,
                self.viewport.y1,
            )),
            Bar::Horizontal if self.bars.1 => Some(Rect::new(
                self.rect.x0,
                self.viewport.y1,
                self.viewport.x1,
                self.rect.y1,
            )),
            _ => None,
        }
    }

    /// The along-the-bar quantities of `bar`: (the track's length, the thumb's length, the
    /// offset, the largest offset).
    fn along(&self, bar: Bar, track: Rect) -> (f32, f32, f32, f32) {
        let max = self.max_offset();
        let (length, visible, content, offset, max) = match bar {
            Bar::Vertical => (
                track.height(),
                self.viewport.height(),
                self.content.height,
                self.offset.y,
                max.y,
            ),
            Bar::Horizontal => (
                track.width(),
                self.viewport.width(),
                self.content.width,
                self.offset.x,
                max.x,
            ),
        };
        // The thumb is to the track what the viewport is to the content, never shorter than
        // MIN_THUMB (nor longer than the track).
        let thumb = if content > 0.0 {
            (length * visible / content).max(MIN_THUMB).min(length)
        } else {
            length
        };
        (length, thumb, offset, max)
    }

    /// The thumb of `bar` at the last layout, if that bar is shown.
    fn thumb(&self, bar: Bar) -> Option<Rect> {
        let track = self.track(bar)?;
        let (length, thumb, offset, max) = self.along(bar, track);
        let start = if max > 0.0 {
            (length - thumb) * offset / max
        } else {
            0.0
        };
        Some(match bar {
            Bar::Vertical => Rect::new(
                track.x0 + THUMB_INSET,
                track.y0 + start,
                track.x1 - THUMB_INSET,
                track.y0 + start + thumb,
            )
            .inflate_y(-THUMB_INSET),
            Bar::Horizontal => Rect::new(
                track.x0 + start,
                track.y0 + THUMB_INSET,
                track.x0 + start + thumb,
                track.y1 - THUMB_INSET,
            )
            .inflate_x(-THUMB_INSET),
        })
    }

    /// The bar whose track contains `point`, if any.
    fn bar_at(&self, point: Point) -> Option<Bar> {
        [Bar::Vertical, Bar::Horizontal]
            .into_iter()
            .find(|&bar| self.track(bar).is_some_and(|track| inside(track, point)))
    }

    /// A left press at `point`: on a thumb it starts a drag; elsewhere on a track it moves one
    /// page towards the press (WF4). Not on a bar: not handled (a press on the content bubbled
    /// up to the area).
    fn press<M>(&mut self, cx: &mut EventCx<'_, M>, point: Point) -> Handled {
        let Some(bar) = self.bar_at(point) else {
            return Handled::No;
        };
        let Some(thumb) = self.thumb(bar) else {
            return Handled::No;
        };
        let (pointer, offset, before, after) = match bar {
            Bar::Vertical => (
                point.y,
                self.offset.y,
                point.y < thumb.y0,
                point.y >= thumb.y1,
            ),
            Bar::Horizontal => (
                point.x,
                self.offset.x,
                point.x < thumb.x0,
                point.x >= thumb.x1,
            ),
        };
        if before || after {
            let page = self.page();
            let sign = if before { -1.0 } else { 1.0 };
            let (dx, dy) = match bar {
                Bar::Vertical => (0.0, sign * page.height),
                Bar::Horizontal => (sign * page.width, 0.0),
            };
            // Handled (and the pointer captured) even when already at the end: the press was on
            // the bar, nothing else should take it.
            let _ = self.scroll_event(cx, dx, dy);
        } else {
            self.drag = Some(ThumbDrag {
                bar,
                pointer,
                offset,
            });
            cx.request_record();
        }
        Handled::Yes
    }

    /// The pointer moved to `point` during a thumb drag: the offset follows proportionally (the
    /// thumb's free travel maps onto the offset's range).
    fn drag_to<M>(&mut self, cx: &mut EventCx<'_, M>, drag: ThumbDrag, point: Point) {
        let Some(track) = self.track(drag.bar) else {
            return;
        };
        let (length, thumb, _, max) = self.along(drag.bar, track);
        let travel = (length - thumb).max(1.0);
        let (pointer, current) = match drag.bar {
            Bar::Vertical => (point.y, self.offset.y),
            Bar::Horizontal => (point.x, self.offset.x),
        };
        let target = (drag.offset + (pointer - drag.pointer) * max / travel).clamp(0.0, max);
        let moved = target - current;
        let (dx, dy) = match drag.bar {
            Bar::Vertical => (0.0, moved),
            Bar::Horizontal => (moved, 0.0),
        };
        let _ = self.scroll_event(cx, dx, dy);
    }

    /// A key (focused, or bubbled from inside, WF5): returns whether the area moved.
    fn key<M>(&mut self, cx: &mut EventCx<'_, M>, key: NamedKey) -> Handled {
        let page = self.page();
        let max = self.max_offset();
        // Home and End go along the direction that scrolls: vertically, unless the area only
        // scrolls horizontally.
        let vertical = self.scroll_y;
        let (dx, dy) = match key {
            NamedKey::ArrowUp => (0.0, -self.line),
            NamedKey::ArrowDown => (0.0, self.line),
            NamedKey::ArrowLeft => (-self.line, 0.0),
            NamedKey::ArrowRight => (self.line, 0.0),
            NamedKey::PageUp if vertical => (0.0, -page.height),
            NamedKey::PageDown if vertical => (0.0, page.height),
            NamedKey::PageUp => (-page.width, 0.0),
            NamedKey::PageDown => (page.width, 0.0),
            NamedKey::Home if vertical => (0.0, -self.offset.y),
            NamedKey::End if vertical => (0.0, max.y - self.offset.y),
            NamedKey::Home => (-self.offset.x, 0.0),
            NamedKey::End => (max.x - self.offset.x, 0.0),
            _ => return Handled::No,
        };
        self.scroll_event(cx, dx, dy)
    }

    /// An assistive technology's scroll request (WF7): a line or a page in `direction`.
    fn scroll_request<M>(
        &mut self,
        cx: &mut EventCx<'_, M>,
        direction: ScrollDirection,
        page: bool,
    ) -> Handled {
        let step = if page {
            self.page()
        } else {
            Size::new(self.line, self.line)
        };
        let (dx, dy) = match direction {
            ScrollDirection::Up => (0.0, -step.height),
            ScrollDirection::Down => (0.0, step.height),
            ScrollDirection::Left => (-step.width, 0.0),
            ScrollDirection::Right => (step.width, 0.0),
        };
        self.scroll_event(cx, dx, dy)
    }

    /// Updates the thumb under the pointer; asks for a redraw if it changed.
    fn set_hover<M>(&mut self, cx: &mut EventCx<'_, M>, hover: Option<Bar>) {
        if hover != self.hover {
            self.hover = hover;
            cx.request_record();
        }
    }
}

/// The constraints (min, max) of the content in one direction from its size rule (its
/// [`LayoutProps`](super::layout::LayoutProps), as any parent honours them): `Fixed` is exact;
/// `Fill` fills the viewport, and in a direction that scrolls may grow beyond it; `Content` is
/// free, up to the viewport where the area does not scroll.
fn rule(length: Length, viewport: f32, scrolls: bool) -> (f32, f32) {
    let open = if scrolls { f32::INFINITY } else { viewport };
    match length {
        Length::Fixed(value) => (value, value),
        Length::Fill(_) => (viewport, open),
        Length::Content => (0.0, open),
    }
}

/// How far `target` must move along one direction to fit between `start` and `end` (a span of
/// the viewport), positive towards the end: nothing if it already fits; its start edge when it
/// is longer than the viewport (the beginning of a widget matters more than its end).
fn reveal_shift(target_start: f32, target_end: f32, start: f32, end: f32) -> f32 {
    if target_start < start || target_end - target_start > end - start {
        start - target_start
    } else if target_end > end {
        end - target_end
    } else {
        0.0
    }
}

/// `true` when `point` is inside `rect` (left and top edges in, right and bottom out).
fn inside(rect: Rect, point: Point) -> bool {
    point.x >= rect.x0 && point.x < rect.x1 && point.y >= rect.y0 && point.y < rect.y1
}

/// Shrinking or growing a rectangle along one direction only (the thumbs' insets).
trait InflateAxis {
    /// Grown by `amount` at the left and the right (shrunk if negative).
    fn inflate_x(self, amount: f32) -> Self;
    /// Grown by `amount` at the top and the bottom (shrunk if negative).
    fn inflate_y(self, amount: f32) -> Self;
}

impl InflateAxis for Rect {
    fn inflate_x(self, amount: f32) -> Self {
        Self::new(self.x0 - amount, self.y0, self.x1 + amount, self.y1)
    }

    fn inflate_y(self, amount: f32) -> Self {
        Self::new(self.x0, self.y0 - amount, self.x1, self.y1 + amount)
    }
}

impl<M: 'static> Widget<M> for ScrollArea {
    fn measure(&mut self, cx: &mut LayoutCx<'_, M>, constraints: Constraints) -> Size {
        self.thickness = Self::thickness_for(cx.spacing());
        self.line = cx.spacing().control_height;
        let space = constraints.max;
        if !(space.width.is_finite() && space.height.is_finite()) {
            // Unbounded in some direction (a scroll area inside a scroll area that scrolls the
            // same way, or in a column measured without a height): the area simply wraps its
            // content there, and the arrange that follows decides the bars in the rectangle it
            // gets.
            let content = if cx.child_count() == 0 {
                Size::ZERO
            } else {
                cx.measure_child(0, Constraints::loose(space))
            };
            return constraints.constrain(content);
        }
        let (content, vertical, horizontal) = self.fit(cx, space);
        let bar = |shown: bool| if shown { self.thickness } else { 0.0 };
        constraints.constrain(Size::new(
            (content.width + bar(vertical)).min(space.width),
            (content.height + bar(horizontal)).min(space.height),
        ))
    }

    fn arrange(&mut self, cx: &mut LayoutCx<'_, M>, rect: Rect) {
        self.thickness = Self::thickness_for(cx.spacing());
        self.line = cx.spacing().control_height;
        let (content, vertical, horizontal) = self.fit(cx, Size::new(rect.width(), rect.height()));
        self.rect = rect;
        self.bars = (vertical, horizontal);
        let viewport = self.viewport_size(Size::new(rect.width(), rect.height()), self.bars);
        self.viewport = Rect::from_origin_size(rect.x0, rect.y0, viewport.width, viewport.height);
        // The content fills at least the viewport, so its background (if it draws one) covers
        // it and it can be scrolled only where it is larger.
        self.content = Size::new(
            content.width.max(viewport.width),
            content.height.max(viewport.height),
        );
        // The content may have shrunk (or the area grown): the offset is brought back in range.
        let max = self.max_offset();
        self.offset = Point::new(
            self.offset.x.clamp(0.0, max.x),
            self.offset.y.clamp(0.0, max.y),
        );
        if self.drag.is_some_and(|drag| self.track(drag.bar).is_none()) {
            // The bar being dragged went away (the content shrank under the drag).
            self.drag = None;
        }
        if cx.child_count() > 0 {
            // On whole logical pixels, so text and lines inside stay as crisp as anywhere else.
            cx.place_child(
                0,
                Rect::from_origin_size(
                    self.viewport.x0 - self.offset.x.round(),
                    self.viewport.y0 - self.offset.y.round(),
                    self.content.width,
                    self.content.height,
                ),
            );
        }
    }

    fn record(&self, cx: &RecordCx, recorder: &mut Recorder<'_>) {
        if self.bars.0 && self.bars.1 {
            // The corner where the two bars meet.
            recorder.fill_rect(
                Rect::new(self.viewport.x1, self.viewport.y1, cx.rect.x1, cx.rect.y1),
                ColorToken::SurfaceSunken,
            );
        }
        for bar in [Bar::Vertical, Bar::Horizontal] {
            let (Some(track), Some(thumb)) = (self.track(bar), self.thumb(bar)) else {
                continue;
            };
            recorder.fill_rect(track, ColorToken::SurfaceSunken);
            let active = self.hover == Some(bar) || self.drag.is_some_and(|drag| drag.bar == bar);
            let token = if active {
                ColorToken::TextMuted
            } else {
                ColorToken::ControlBorder
            };
            let radius = thumb.width().min(thumb.height()) / 2.0;
            recorder.fill_rounded_rect(RoundedRect::new(thumb, radius), token);
        }
        if cx.focused {
            // Inside the viewport (the area usually fills its parent, so outside would be cut).
            let width = f32::from(cx.shape.focus_width.max(1));
            super::look::focus_ring(recorder, self.viewport.inflate(-2.0 - width), 0.0, cx.shape);
        }
    }

    fn event(&mut self, cx: &mut EventCx<'_, M>, event: &WidgetEvent<'_>) -> Handled {
        match *event {
            WidgetEvent::Wheel { delta, .. } => self.wheel(cx, delta),
            WidgetEvent::PointerButton {
                button: PointerButton::Left,
                state: ButtonState::Pressed,
                position,
                ..
            } => self.press(cx, position),
            WidgetEvent::PointerButton {
                button: PointerButton::Left,
                state: ButtonState::Released,
                position,
                ..
            } => {
                if self.drag.take().is_some() {
                    let hover = self.bar_at(position);
                    self.hover = hover;
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
                let hover = self
                    .bar_at(position)
                    .filter(|&bar| self.thumb(bar).is_some_and(|thumb| inside(thumb, position)));
                self.set_hover(cx, hover);
                if self.bar_at(position).is_some() {
                    Handled::Yes
                } else {
                    Handled::No
                }
            }
            WidgetEvent::PointerLeft => {
                self.set_hover(cx, None);
                Handled::Yes
            }
            WidgetEvent::Key {
                key: Key::Named(key),
                state: ButtonState::Pressed,
                ..
            } => self.key(cx, *key),
            WidgetEvent::Scroll { direction, page } => self.scroll_request(cx, direction, page),
            _ => Handled::No,
        }
    }

    fn focusable(&self) -> bool {
        self.focusable
    }

    fn access_role(&self) -> AccessRole {
        AccessRole::ScrollView
    }

    fn access_info(&self, info: &mut AccessInfo) {
        let max = self.max_offset();
        info.scroll = Some(ScrollInfo {
            x: self.offset.x,
            max_x: max.x,
            y: self.offset.y,
            max_y: max.y,
        });
    }

    fn child_clip(&self) -> Option<Rect> {
        Some(self.viewport)
    }

    fn scroll_into_view(&mut self, target: Rect) -> Point {
        if self.viewport.is_empty() {
            return Point::default();
        }
        // How far the content should move (positive: right and down) to show `target`.
        let shift_x = if self.scroll_x {
            reveal_shift(target.x0, target.x1, self.viewport.x0, self.viewport.x1)
        } else {
            0.0
        };
        let shift_y = if self.scroll_y {
            reveal_shift(target.y0, target.y1, self.viewport.y0, self.viewport.y1)
        } else {
            0.0
        };
        // Moving the content right and down means a smaller offset.
        let before = self.offset;
        let _ = self.scroll_by(-shift_x, -shift_y);
        // What really moved, after clamping and the rounding of the placement.
        Point::new(
            before.x.round() - self.offset.x.round(),
            before.y.round() - self.offset.y.round(),
        )
    }
}
