// SPDX-License-Identifier: GPL-3.0-or-later

//! The pointer's shape (F2.09e, a precision of ADR-0045 WA3): the splitter's resize shapes over
//! its divider only, kept for the whole drag; the arrow while another widget captures the
//! pointer, outside the window and over what a modal layer blocks; the bubbling from the hovered
//! widget to its ancestors, where the innermost answer wins and a modal layer's root is the top.
//!
//! The shape is what [`Ui::pointer_shape`] answers: the window passes it to the system, which no
//! test can see (no display, CLAUDE.md §3). The translation to `winit` has its own unit test.

#![allow(
    clippy::unwrap_used,
    reason = "test helpers: a failed unwrap fails the test"
)]

use orchestra_text::Faces;
use orchestra_ui::{
    Appearance, Axis, BuiltInTheme, ButtonState, ColorToken, Constraints, DisplayList, EventCx,
    Flex, Handled, InputEvent, LayoutCx, LayoutProps, Length, Point, PointerButton, PointerShape,
    RecordCx, Recorder, Rect, Size, Space, Splitter, Theme, Ui, Widget, WidgetEvent, WidgetId,
};

/// A leaf that fills what it is given, asks for `shape` everywhere over itself (`None`: lets the
/// parent decide), and captures the pointer on a press, as any clickable widget does.
#[derive(Debug)]
struct Pad {
    /// The shape it answers over its whole rectangle.
    shape: Option<PointerShape>,
    /// Its rectangle at the last arrange, to answer only over itself.
    rect: Rect,
}

impl Pad {
    /// A pad answering `shape`.
    const fn new(shape: Option<PointerShape>) -> Self {
        Self {
            shape,
            rect: Rect::EMPTY,
        }
    }
}

impl Widget<()> for Pad {
    fn measure(&mut self, _cx: &mut LayoutCx<'_, ()>, constraints: Constraints) -> Size {
        constraints.constrain(Size::new(10.0, 10.0))
    }
    fn arrange(&mut self, _cx: &mut LayoutCx<'_, ()>, rect: Rect) {
        self.rect = rect;
    }
    fn record(&self, cx: &RecordCx, recorder: &mut Recorder<'_>) {
        recorder.fill_rect(cx.rect, ColorToken::Surface);
    }
    fn event(&mut self, _cx: &mut EventCx<'_, ()>, event: &WidgetEvent<'_>) -> Handled {
        match event {
            WidgetEvent::PointerButton { .. } => Handled::Yes,
            _ => Handled::No,
        }
    }
    fn pointer_shape(&self, point: Point) -> Option<PointerShape> {
        let inside = point.x >= self.rect.x0
            && point.x < self.rect.x1
            && point.y >= self.rect.y0
            && point.y < self.rect.y1;
        self.shape.filter(|_| inside)
    }
}

/// A container with one child filling it, asking for `shape` everywhere over itself: the
/// ancestor whose answer bubbling reaches when the hovered widget says nothing.
#[derive(Debug)]
struct Frame {
    /// The shape it answers.
    shape: PointerShape,
}

impl Widget<()> for Frame {
    fn measure(&mut self, cx: &mut LayoutCx<'_, ()>, constraints: Constraints) -> Size {
        if cx.child_count() > 0 {
            let _ = cx.measure_child(0, constraints);
        }
        constraints.constrain(constraints.max)
    }
    fn arrange(&mut self, cx: &mut LayoutCx<'_, ()>, rect: Rect) {
        if cx.child_count() > 0 {
            let _ = cx.measure_child(
                0,
                Constraints::tight(Size::new(rect.width(), rect.height())),
            );
            cx.place_child(0, rect);
        }
    }
    fn record(&self, _cx: &RecordCx, _recorder: &mut Recorder<'_>) {}
    fn pointer_shape(&self, _point: Point) -> Option<PointerShape> {
        Some(self.shape)
    }
}

/// The window: 300 × 200.
const BOUNDS: Rect = Rect::new(0.0, 0.0, 300.0, 200.0);

/// The theme of these tests (comfortable density: the divider's band is 8 px).
fn theme() -> Theme {
    *BuiltInTheme::Studio.pair().instance(Appearance::Light)
}

/// Both directions filled.
fn fill() -> LayoutProps {
    LayoutProps::default()
        .width(Length::Fill(1.0))
        .height(Length::Fill(1.0))
}

/// A window filled by a splitter along `axis` at half, with two pads saying nothing as panes.
/// Side by side the band is at x 146–154; one above the other at y 96–104.
fn split(axis: Axis) -> (Ui<()>, DisplayList) {
    let mut ui = Ui::new(
        Flex::column(Space::None, Space::None),
        &theme(),
        Faces::bundled().unwrap(),
    );
    let root = ui.root();
    let splitter = ui.add(root, Splitter::new(axis, 0.5), fill()).unwrap();
    ui.add(splitter, Pad::new(None), fill()).unwrap();
    ui.add(splitter, Pad::new(None), fill()).unwrap();
    let mut list = DisplayList::new();
    ui.layout(BOUNDS, &mut list);
    (ui, list)
}

/// A window whose root is a frame asking for `ResizeRow`, around a row of two pads: the left one
/// says nothing, the right one asks for `right`. Returns the right pad's ID.
fn framed(right: Option<PointerShape>) -> (Ui<()>, WidgetId, DisplayList) {
    let mut ui = Ui::new(
        Frame {
            shape: PointerShape::ResizeRow,
        },
        &theme(),
        Faces::bundled().unwrap(),
    );
    let root = ui.root();
    let row = ui
        .add(root, Flex::row(Space::None, Space::None), fill())
        .unwrap();
    ui.add(row, Pad::new(None), fill()).unwrap();
    let pad = ui.add(row, Pad::new(right), fill()).unwrap();
    let mut list = DisplayList::new();
    ui.layout(BOUNDS, &mut list);
    (ui, pad, list)
}

/// Moves the pointer to (`x`, `y`) and answers the shape there.
fn shape_at(ui: &mut Ui<()>, list: &mut DisplayList, x: f32, y: f32) -> PointerShape {
    ui.input(
        &InputEvent::PointerMoved {
            position: Point::new(x, y),
        },
        list,
    );
    ui.pointer_shape()
}

/// A left button event at (`x`, `y`).
fn button(ui: &mut Ui<()>, list: &mut DisplayList, state: ButtonState, x: f32, y: f32) {
    ui.input(
        &InputEvent::PointerButton {
            button: PointerButton::Left,
            state,
            position: Point::new(x, y),
            clicks: 1,
        },
        list,
    );
}

#[test]
fn a_new_interface_shows_the_arrow() {
    let (ui, _) = split(Axis::Horizontal);
    assert_eq!(ui.pointer_shape(), PointerShape::Default);
}

#[test]
fn panes_side_by_side_resize_columns_over_the_band_only() {
    let (mut ui, mut list) = split(Axis::Horizontal);
    assert_eq!(
        shape_at(&mut ui, &mut list, 150.0, 100.0),
        PointerShape::ResizeColumn
    );
    // The band's edges: its left edge is in, its right edge belongs to the second pane.
    assert_eq!(
        shape_at(&mut ui, &mut list, 146.0, 100.0),
        PointerShape::ResizeColumn
    );
    assert_eq!(
        shape_at(&mut ui, &mut list, 154.0, 100.0),
        PointerShape::Default
    );
    assert_eq!(
        shape_at(&mut ui, &mut list, 50.0, 100.0),
        PointerShape::Default
    );
    // Back on the band, then out of the window: nothing is hovered, the arrow.
    assert_eq!(
        shape_at(&mut ui, &mut list, 150.0, 100.0),
        PointerShape::ResizeColumn
    );
    ui.input(&InputEvent::PointerLeft, &mut list);
    assert_eq!(ui.pointer_shape(), PointerShape::Default);
}

#[test]
fn panes_one_above_the_other_resize_rows() {
    let (mut ui, mut list) = split(Axis::Vertical);
    assert_eq!(
        shape_at(&mut ui, &mut list, 150.0, 100.0),
        PointerShape::ResizeRow
    );
    assert_eq!(
        shape_at(&mut ui, &mut list, 150.0, 50.0),
        PointerShape::Default
    );
}

#[test]
fn a_drag_keeps_the_resize_shape_until_the_release() {
    let (mut ui, mut list) = split(Axis::Horizontal);
    button(&mut ui, &mut list, ButtonState::Pressed, 150.0, 100.0);
    // Far past the window: the divider stops at the second pane's minimum, the pointer goes on.
    assert_eq!(
        shape_at(&mut ui, &mut list, 900.0, 100.0),
        PointerShape::ResizeColumn
    );
    // Back over the first pane, past the first pane's minimum (the band stops at 48–56).
    assert_eq!(
        shape_at(&mut ui, &mut list, 20.0, 100.0),
        PointerShape::ResizeColumn
    );
    // Released over the pane: the drag is over, the pane says nothing, the arrow.
    button(&mut ui, &mut list, ButtonState::Released, 20.0, 100.0);
    assert_eq!(ui.pointer_shape(), PointerShape::Default);
}

#[test]
fn while_another_widget_captures_the_divider_is_not_asked() {
    let (mut ui, mut list) = split(Axis::Horizontal);
    // A press on the first pane: it captures the pointer.
    button(&mut ui, &mut list, ButtonState::Pressed, 50.0, 100.0);
    // Dragged over the band: the band will not get the release, so it shows no resize shape.
    assert_eq!(
        shape_at(&mut ui, &mut list, 150.0, 100.0),
        PointerShape::Default
    );
    // Released there: the band is hovered now.
    button(&mut ui, &mut list, ButtonState::Released, 150.0, 100.0);
    assert_eq!(ui.pointer_shape(), PointerShape::ResizeColumn);
}

#[test]
fn the_hovered_widget_answers_first_then_its_ancestors() {
    let (mut ui, _, mut list) = framed(Some(PointerShape::ResizeColumn));
    // The left pad says nothing, nor does the row: the root frame's shape.
    assert_eq!(
        shape_at(&mut ui, &mut list, 50.0, 100.0),
        PointerShape::ResizeRow
    );
    // The right pad answers: its shape wins over the frame's.
    assert_eq!(
        shape_at(&mut ui, &mut list, 250.0, 100.0),
        PointerShape::ResizeColumn
    );
}

#[test]
fn a_modal_layer_is_the_top_of_the_bubbling_and_blocks_the_rest() {
    let (mut ui, pad, mut list) = framed(None);
    ui.open_modal(pad, &mut list).unwrap();
    // Inside the layer: the pad says nothing and the walk stops at the layer's root, so the
    // frame behind the dialog has no say.
    assert_eq!(
        shape_at(&mut ui, &mut list, 250.0, 100.0),
        PointerShape::Default
    );
    // Outside the layer: blocked, nothing hovered.
    assert_eq!(
        shape_at(&mut ui, &mut list, 50.0, 100.0),
        PointerShape::Default
    );
    // Closed: the frame's shape again, on both sides.
    ui.close_modal(pad, &mut list).unwrap();
    assert_eq!(
        shape_at(&mut ui, &mut list, 250.0, 100.0),
        PointerShape::ResizeRow
    );
    assert_eq!(
        shape_at(&mut ui, &mut list, 50.0, 100.0),
        PointerShape::ResizeRow
    );
}
