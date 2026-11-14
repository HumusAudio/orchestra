// SPDX-License-Identifier: GPL-3.0-or-later

//! The scroll area and the splitter (F2.09d, choices WF1–WF7): sizes and bars, the wheel (lines,
//! pixels, Shift, bubbling at the edges), the bars (thumb drag, page on the track), the keyboard
//! (own focus and keys bubbling from inside), the focus brought into view, clipping of drawing
//! and of the pointer, the accessibility node and its actions; the splitter's shares, minimums,
//! drag (plain and fine), double click, wheel, keyboard and accessibility.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::float_cmp,
    reason = "test helpers: a failed unwrap or index fails the test; offsets compared are sums \
              of whole pixels, exact in f32"
)]
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "pixel coordinates of small, positive test rectangles"
)]

use accesskit::{Action, NodeId, Role};
use orchestra_text::Faces;
use orchestra_ui::{
    AccessAction, AccessActionKind, Appearance, Axis, BuiltInTheme, ButtonState, ColorToken,
    Constraints, CpuRenderer, DisplayList, EventCx, Flex, Handled, InputEvent, Key, LayoutCx,
    LayoutProps, Length, Modifiers, NamedKey, Point, PointerButton, RecordCx, Recorder, Rect,
    ScrollArea, ScrollDirection, Size, Slider, Space, Splitter, Theme, Ui, WheelDelta, Widget,
    WidgetEvent, WidgetId,
};

/// The messages of these tests: a pad pressed, a slider's value, a splitter's share.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Msg {
    /// A pad was pressed.
    Pressed(WidgetId),
    /// A slider moved.
    Value(f64),
    /// A splitter moved.
    Share(f64),
}

/// A focusable leaf of a fixed size that draws itself in `accent` and reports presses.
#[derive(Debug)]
struct Pad {
    /// Its size.
    size: Size,
}

impl Pad {
    /// A pad of `width` × `height`.
    const fn new(width: f32, height: f32) -> Self {
        Self {
            size: Size::new(width, height),
        }
    }
}

impl Widget<Msg> for Pad {
    fn measure(&mut self, _cx: &mut LayoutCx<'_, Msg>, constraints: Constraints) -> Size {
        constraints.constrain(self.size)
    }
    fn record(&self, cx: &RecordCx, recorder: &mut Recorder<'_>) {
        recorder.fill_rect(cx.rect, ColorToken::Accent);
    }
    fn event(&mut self, cx: &mut EventCx<'_, Msg>, event: &WidgetEvent<'_>) -> Handled {
        match event {
            WidgetEvent::PointerButton {
                state: ButtonState::Pressed,
                ..
            } => {
                cx.emit(Msg::Pressed(cx.id()));
                Handled::Yes
            }
            WidgetEvent::PointerButton { .. } => Handled::Yes,
            _ => Handled::No,
        }
    }
    fn focusable(&self) -> bool {
        true
    }
}

/// A leaf that draws wider than the rectangle it is given, like a label narrowed below its
/// text's width: it asks for 40 × 40 and fills `reach` pixels to the right of its left edge.
#[derive(Debug)]
struct Overflow {
    /// How far right of its rectangle's left edge it draws, logical pixels.
    reach: f32,
}

impl Widget<Msg> for Overflow {
    fn measure(&mut self, _cx: &mut LayoutCx<'_, Msg>, constraints: Constraints) -> Size {
        constraints.constrain(Size::new(40.0, 40.0))
    }
    fn record(&self, cx: &RecordCx, recorder: &mut Recorder<'_>) {
        let r = cx.rect;
        recorder.fill_rect(
            Rect::new(r.x0, r.y0, r.x0 + self.reach, r.y1),
            ColorToken::Accent,
        );
    }
}

/// Studio light.
fn theme() -> Theme {
    *BuiltInTheme::Studio.pair().instance(Appearance::Light)
}

/// The window: 300 × 200.
const BOUNDS: Rect = Rect::new(0.0, 0.0, 300.0, 200.0);

/// Comfortable density: a line (control height) is 28 px, the bars are 12 px thick.
const LINE: f32 = 28.0;
/// See [`LINE`].
const BAR: f32 = 12.0;

/// An interface whose root is `area` filling the window, with a column of `rows` pads of
/// `width` × 40 (no spacing, no padding: row `i` starts at `40 i` in the content). Returns the
/// interface, the area, the pads and the display list, laid out.
fn area(
    area: ScrollArea,
    rows: usize,
    width: f32,
) -> (Ui<Msg>, WidgetId, Vec<WidgetId>, DisplayList) {
    let mut ui = Ui::new(
        Flex::column(Space::None, Space::None),
        &theme(),
        Faces::bundled().unwrap(),
    );
    ui.set_title("Scrolling");
    let root = ui.root();
    let scroll = ui
        .add(
            root,
            area,
            LayoutProps::default()
                .width(Length::Fill(1.0))
                .height(Length::Fill(1.0)),
        )
        .unwrap();
    let column = ui
        .add(
            scroll,
            Flex::column(Space::None, Space::None),
            LayoutProps::default(),
        )
        .unwrap();
    let pads = (0..rows)
        .map(|_| {
            ui.add(column, Pad::new(width, 40.0), LayoutProps::default())
                .unwrap()
        })
        .collect();
    let mut list = DisplayList::new();
    ui.layout(BOUNDS, &mut list);
    (ui, scroll, pads, list)
}

/// The area's offset.
fn offset(ui: &Ui<Msg>, id: WidgetId) -> Point {
    ui.widget::<ScrollArea>(id).unwrap().offset()
}

/// The area's viewport.
fn viewport(ui: &Ui<Msg>, id: WidgetId) -> Rect {
    ui.widget::<ScrollArea>(id).unwrap().viewport()
}

/// Moves the pointer to `at`.
fn move_to(ui: &mut Ui<Msg>, list: &mut DisplayList, at: Point) {
    ui.input(&InputEvent::PointerMoved { position: at }, list);
}

/// A wheel event at the pointer.
fn wheel(ui: &mut Ui<Msg>, list: &mut DisplayList, delta: WheelDelta) {
    ui.input(&InputEvent::Wheel { delta }, list);
}

/// A left button event at `at`, the `clicks`-th of a series.
fn button(ui: &mut Ui<Msg>, list: &mut DisplayList, state: ButtonState, at: Point, clicks: u8) {
    ui.input(
        &InputEvent::PointerButton {
            button: PointerButton::Left,
            state,
            position: at,
            clicks,
        },
        list,
    );
}

/// A press and release at `at`.
fn click(ui: &mut Ui<Msg>, list: &mut DisplayList, at: Point) {
    button(ui, list, ButtonState::Pressed, at, 1);
    button(ui, list, ButtonState::Released, at, 1);
}

/// A key press with `modifiers`.
fn key(ui: &mut Ui<Msg>, list: &mut DisplayList, named: NamedKey, modifiers: Modifiers) {
    ui.input(
        &InputEvent::Key {
            key: Key::Named(named),
            state: ButtonState::Pressed,
            repeat: false,
            modifiers,
        },
        list,
    );
}

/// Holds `modifiers` (a modifiers event, as the window sends).
fn hold(ui: &mut Ui<Msg>, list: &mut DisplayList, modifiers: Modifiers) {
    ui.input(&InputEvent::ModifiersChanged { modifiers }, list);
}

/// Shift alone.
fn shift() -> Modifiers {
    Modifiers {
        shift: true,
        ..Modifiers::default()
    }
}

/// The primary modifier alone.
fn primary() -> Modifiers {
    Modifiers {
        primary: true,
        ..Modifiers::default()
    }
}

/// The messages so far.
fn messages(ui: &mut Ui<Msg>) -> Vec<Msg> {
    ui.take_messages().collect()
}

/// The pixel at (`x`, `y`) of the whole frame.
fn pixel(list: &DisplayList, x: u32, y: u32) -> [u8; 4] {
    let image = CpuRenderer::new(Faces::bundled().unwrap())
        .render_to_image(list, &theme(), 300, 200, 1.0)
        .unwrap();
    let at = ((y * 300 + x) * 4) as usize;
    let data = image.data();
    [data[at], data[at + 1], data[at + 2], data[at + 3]]
}

/// The opaque pixel of a token.
fn rgba(token: ColorToken) -> [u8; 4] {
    let c = theme().color(token);
    [c.r, c.g, c.b, 255]
}

/// The node of `id` in an accessibility update.
fn node(update: &orchestra_ui::AccessUpdate, id: WidgetId) -> accesskit::Node {
    update
        .tree_update()
        .nodes
        .iter()
        .find(|(node, _)| *node == NodeId(id.get()))
        .map(|(_, node)| node.clone())
        .unwrap()
}

#[test]
fn a_vertical_bar_appears_only_when_the_content_is_taller() {
    // Ten rows of 40 = 400 px of content in a 200 px window: a bar, the content as wide as the
    // viewport (the window less the bar).
    let (ui, scroll, pads, _) = area(ScrollArea::vertical(), 10, 100.0);
    let view = viewport(&ui, scroll);
    assert_eq!(view, Rect::new(0.0, 0.0, 300.0 - BAR, 200.0));
    assert_eq!(
        ui.widget::<ScrollArea>(scroll).unwrap().max_offset(),
        Point::new(0.0, 200.0)
    );
    assert_eq!(ui.rect(pads[0]).unwrap(), Rect::new(0.0, 0.0, 100.0, 40.0));
    // Three rows fit: no bar, nothing to scroll, the viewport is the whole area.
    let (ui, scroll, _, _) = area(ScrollArea::vertical(), 3, 100.0);
    assert_eq!(viewport(&ui, scroll), BOUNDS);
    assert_eq!(
        ui.widget::<ScrollArea>(scroll).unwrap().max_offset(),
        Point::default()
    );
}

#[test]
fn one_bar_can_bring_the_other() {
    // Four rows (160 high) and 295 wide fit 300 × 200: no bars.
    let (ui, scroll, _, _) = area(ScrollArea::both(), 4, 295.0);
    assert_eq!(viewport(&ui, scroll), BOUNDS);
    // Six rows (240) need a vertical bar, which leaves 288 for a content 295 wide: a horizontal
    // bar too.
    let (ui, scroll, _, _) = area(ScrollArea::both(), 6, 295.0);
    assert_eq!(
        viewport(&ui, scroll),
        Rect::new(0.0, 0.0, 300.0 - BAR, 200.0 - BAR)
    );
    let max = ui.widget::<ScrollArea>(scroll).unwrap().max_offset();
    assert_eq!(
        max,
        Point::new(295.0 - (300.0 - BAR), 240.0 - (200.0 - BAR))
    );
}

#[test]
fn the_wheel_scrolls_three_lines_and_stops_at_the_ends() {
    let (mut ui, scroll, pads, mut list) = area(ScrollArea::vertical(), 10, 100.0);
    move_to(&mut ui, &mut list, Point::new(50.0, 100.0));
    // A notch towards the user (negative) shows what is below.
    wheel(&mut ui, &mut list, WheelDelta::Lines { x: 0.0, y: -1.0 });
    assert_eq!(offset(&ui, scroll), Point::new(0.0, 3.0 * LINE));
    // The content moved by as much.
    assert_eq!(ui.rect(pads[0]).unwrap().y0, -3.0 * LINE);
    // Pixels are used as they come.
    wheel(&mut ui, &mut list, WheelDelta::Pixels { x: 0.0, y: 10.0 });
    assert_eq!(offset(&ui, scroll), Point::new(0.0, 3.0 * LINE - 10.0));
    // Far past the end: clamped at the largest offset.
    wheel(&mut ui, &mut list, WheelDelta::Lines { x: 0.0, y: -20.0 });
    assert_eq!(offset(&ui, scroll), Point::new(0.0, 200.0));
    assert_eq!(ui.rect(pads[9]).unwrap().y1, 200.0);
    // And back past the start.
    wheel(&mut ui, &mut list, WheelDelta::Lines { x: 0.0, y: 20.0 });
    assert_eq!(offset(&ui, scroll), Point::default());
}

#[test]
fn shift_turns_the_wheel_horizontal() {
    let (mut ui, scroll, _, mut list) = area(ScrollArea::both(), 10, 600.0);
    move_to(&mut ui, &mut list, Point::new(50.0, 100.0));
    hold(&mut ui, &mut list, shift());
    wheel(&mut ui, &mut list, WheelDelta::Lines { x: 0.0, y: -1.0 });
    assert_eq!(offset(&ui, scroll), Point::new(3.0 * LINE, 0.0));
    // A touchpad's own horizontal movement is used as it is, Shift or not.
    hold(&mut ui, &mut list, Modifiers::default());
    wheel(&mut ui, &mut list, WheelDelta::Pixels { x: -5.0, y: 0.0 });
    assert_eq!(offset(&ui, scroll), Point::new(3.0 * LINE + 5.0, 0.0));
}

#[test]
fn at_its_end_an_inner_area_lets_the_outer_one_scroll() {
    // An outer vertical area whose column holds a 100 px high inner area (with ten rows) and
    // then ten more rows.
    let mut ui: Ui<Msg> = Ui::new(
        Flex::column(Space::None, Space::None),
        &theme(),
        Faces::bundled().unwrap(),
    );
    let root = ui.root();
    let fill = LayoutProps::default()
        .width(Length::Fill(1.0))
        .height(Length::Fill(1.0));
    let outer = ui.add(root, ScrollArea::vertical(), fill).unwrap();
    let column = ui
        .add(
            outer,
            Flex::column(Space::None, Space::None),
            LayoutProps::default().width(Length::Fill(1.0)),
        )
        .unwrap();
    let inner = ui
        .add(
            column,
            ScrollArea::vertical(),
            LayoutProps::default()
                .width(Length::Fixed(200.0))
                .height(Length::Fixed(100.0)),
        )
        .unwrap();
    let inner_column = ui
        .add(
            inner,
            Flex::column(Space::None, Space::None),
            LayoutProps::default(),
        )
        .unwrap();
    for _ in 0..10 {
        ui.add(inner_column, Pad::new(100.0, 40.0), LayoutProps::default())
            .unwrap();
    }
    for _ in 0..10 {
        ui.add(column, Pad::new(100.0, 40.0), LayoutProps::default())
            .unwrap();
    }
    let mut list = DisplayList::new();
    ui.layout(BOUNDS, &mut list);
    move_to(&mut ui, &mut list, Point::new(50.0, 50.0));
    // The inner area scrolls first: 400 − 100 = 300 to go.
    wheel(&mut ui, &mut list, WheelDelta::Pixels { x: 0.0, y: -250.0 });
    assert_eq!(offset(&ui, inner), Point::new(0.0, 250.0));
    assert_eq!(offset(&ui, outer), Point::default());
    // Then it reaches its end (moving 50 of the 100 asked: it still handles that event).
    wheel(&mut ui, &mut list, WheelDelta::Pixels { x: 0.0, y: -100.0 });
    assert_eq!(offset(&ui, inner), Point::new(0.0, 300.0));
    assert_eq!(offset(&ui, outer), Point::default());
    // At its end it cannot move: the wheel bubbles to the outer area.
    wheel(&mut ui, &mut list, WheelDelta::Pixels { x: 0.0, y: -30.0 });
    assert_eq!(offset(&ui, inner), Point::new(0.0, 300.0));
    assert_eq!(offset(&ui, outer), Point::new(0.0, 30.0));
    // The inner area moved with the outer content: its viewport is 30 px higher.
    assert_eq!(viewport(&ui, inner).y0, -30.0);
}

#[test]
fn the_content_is_clipped_to_the_viewport_for_drawing_and_pointing() {
    // Content 600 wide and 400 high in an area scrolling both ways: both bars. Row 4 (160–200)
    // passes under the horizontal bar (188–200), which must cover it for drawing and pointing.
    let (mut ui, scroll, pads, mut list) = area(ScrollArea::both(), 10, 600.0);
    let view = viewport(&ui, scroll);
    assert_eq!(view, Rect::new(0.0, 0.0, 300.0 - BAR, 200.0 - BAR));
    assert_eq!(ui.rect(pads[4]).unwrap().y1, 200.0);
    // The pad is drawn down to the viewport, the bar's track below it.
    assert_eq!(pixel(&list, 50, 185), rgba(ColorToken::Accent));
    assert_ne!(pixel(&list, 50, 194), rgba(ColorToken::Accent));
    // A press on the bar reaches the area (it pages right), not the pad under it.
    click(&mut ui, &mut list, Point::new(250.0, 194.0));
    assert_eq!(messages(&mut ui), []);
    assert!(offset(&ui, scroll).x > 0.0);
    // Scrolled down by 100: pad 0 (0–40) is out of sight, pad 2 is at the top of the viewport.
    move_to(&mut ui, &mut list, Point::new(50.0, 100.0));
    wheel(&mut ui, &mut list, WheelDelta::Pixels { x: 0.0, y: -100.0 });
    assert_eq!(ui.rect(pads[2]).unwrap().y0, -20.0);
    click(&mut ui, &mut list, Point::new(50.0, 5.0));
    assert_eq!(messages(&mut ui), [Msg::Pressed(pads[2])]);
}

#[test]
fn a_widget_taller_than_the_viewport_is_shown_from_its_top() {
    // Pad 1 is 300 high in a 200 viewport: focusing it shows its top (with its ring's margin).
    let mut ui: Ui<Msg> = Ui::new(
        Flex::column(Space::None, Space::None),
        &theme(),
        Faces::bundled().unwrap(),
    );
    let root = ui.root();
    let fill = LayoutProps::default()
        .width(Length::Fill(1.0))
        .height(Length::Fill(1.0));
    let scroll = ui.add(root, ScrollArea::vertical(), fill).unwrap();
    let column = ui
        .add(
            scroll,
            Flex::column(Space::None, Space::None),
            LayoutProps::default(),
        )
        .unwrap();
    let pads = [
        ui.add(column, Pad::new(100.0, 250.0), LayoutProps::default())
            .unwrap(),
        ui.add(column, Pad::new(100.0, 300.0), LayoutProps::default())
            .unwrap(),
        ui.add(column, Pad::new(100.0, 40.0), LayoutProps::default())
            .unwrap(),
    ];
    let mut list = DisplayList::new();
    ui.layout(BOUNDS, &mut list);
    // From the top: the pad starts inside the viewport but does not fit; its top is shown.
    ui.set_focus(Some(pads[1]), &mut list).unwrap();
    assert_eq!(offset(&ui, scroll), Point::new(0.0, 244.0));
    // Scrolled to the end first: the pad's top is above the viewport, its bottom inside.
    ui.set_focus(Some(pads[2]), &mut list).unwrap();
    assert_eq!(offset(&ui, scroll), Point::new(0.0, 390.0));
    ui.set_focus(Some(pads[1]), &mut list).unwrap();
    assert_eq!(offset(&ui, scroll), Point::new(0.0, 244.0));
}

#[test]
fn content_outside_a_smaller_area_is_neither_drawn_nor_hit() {
    // The area is the top 100 px of the window; below it, nothing else.
    let mut ui: Ui<Msg> = Ui::new(
        Flex::column(Space::None, Space::None),
        &theme(),
        Faces::bundled().unwrap(),
    );
    let root = ui.root();
    let scroll = ui
        .add(
            root,
            ScrollArea::vertical(),
            LayoutProps::default()
                .width(Length::Fill(1.0))
                .height(Length::Fixed(100.0)),
        )
        .unwrap();
    let column = ui
        .add(
            scroll,
            Flex::column(Space::None, Space::None),
            LayoutProps::default(),
        )
        .unwrap();
    let pads: Vec<_> = (0..5)
        .map(|_| {
            ui.add(column, Pad::new(100.0, 40.0), LayoutProps::default())
                .unwrap()
        })
        .collect();
    let mut list = DisplayList::new();
    ui.layout(BOUNDS, &mut list);
    // Pad 3 is at 120–160, below the area: not drawn, not hit.
    assert_eq!(ui.rect(pads[3]).unwrap().y0, 120.0);
    assert_eq!(pixel(&list, 50, 130), rgba(ColorToken::Surface));
    click(&mut ui, &mut list, Point::new(50.0, 130.0));
    assert_eq!(messages(&mut ui), []);
    // Pad 2 is cut at the area's bottom edge (80–120 → 80–100).
    assert_eq!(pixel(&list, 50, 95), rgba(ColorToken::Accent));
    assert_eq!(pixel(&list, 50, 105), rgba(ColorToken::Surface));
}

#[test]
fn drawing_wider_than_its_rectangle_stays_inside_its_pane() {
    // Found on screen after F2.10b2: with the divider moved left, the labels of the left pane
    // (narrowed below their text's width) drew across the divider into the right pane. Their
    // rectangles were inside the pane, so the clip was skipped; what they draw now decides.
    let mut ui: Ui<Msg> = Ui::new(
        Flex::column(Space::None, Space::None),
        &theme(),
        Faces::bundled().unwrap(),
    );
    let root = ui.root();
    let fill = LayoutProps::default()
        .width(Length::Fill(1.0))
        .height(Length::Fill(1.0));
    let splitter = ui
        .add(
            root,
            Splitter::new(Axis::Horizontal, 0.3).minimum_sizes(0.0, 0.0),
            fill,
        )
        .unwrap();
    let left = ui.add(splitter, ScrollArea::vertical(), fill).unwrap();
    ui.add(splitter, ScrollArea::vertical(), fill).unwrap();
    // In a padded column, as the demonstration's options are: the widget's rectangle (with the
    // room kept for a focus ring) is then well inside the pane.
    let column = ui
        .add(
            left,
            Flex::column(Space::None, Space::M),
            LayoutProps::default().width(Length::Fill(1.0)),
        )
        .unwrap();
    let wide = ui
        .add(column, Overflow { reach: 250.0 }, LayoutProps::default())
        .unwrap();
    let mut list = DisplayList::new();
    ui.layout(BOUNDS, &mut list);
    let pane = viewport(&ui, left);
    let rect = ui.rect(wide).unwrap();
    // The premise: the widget's rectangle lies inside its pane, its drawing does not.
    assert!(rect.x1 <= pane.x1 && rect.x0 + 250.0 > pane.x1);
    // Drawn up to the pane's edge, not past it (the right pane shows its background).
    let y = (rect.y0 + 20.0) as u32;
    assert_eq!(
        pixel(&list, (pane.x1 - 2.0) as u32, y),
        rgba(ColorToken::Accent)
    );
    assert_eq!(
        pixel(&list, (pane.x1 + 20.0) as u32, y),
        rgba(ColorToken::Surface)
    );
    assert_eq!(pixel(&list, 250, y), rgba(ColorToken::Surface));
    // Recording it again damages only inside the pane: the unclipped attempt leaves no trace.
    let _ = list.take_damage();
    ui.request_record(wide);
    ui.layout(BOUNDS, &mut list);
    let damage = list.take_damage();
    assert_ne!(damage.rects(), &[] as &[Rect]);
    for r in damage.rects() {
        assert!(r.x1 <= pane.x1, "damage {r:?} leaves the pane {pane:?}");
    }
}

#[test]
fn growing_the_area_shows_what_was_clipped() {
    // The same rows; the window grows from 200 to 400 high: the rows below 200 were recorded
    // empty and must be recorded again now that their clip changed, though they did not move.
    // It was scrolled to its end (120): with nothing left to scroll, the offset returns to 0.
    let (mut ui, scroll, pads, mut list) = area(ScrollArea::vertical(), 8, 100.0);
    assert_eq!(pixel(&list, 50, 190), rgba(ColorToken::Accent));
    move_to(&mut ui, &mut list, Point::new(50.0, 100.0));
    wheel(&mut ui, &mut list, WheelDelta::Lines { x: 0.0, y: -10.0 });
    assert_eq!(offset(&ui, scroll), Point::new(0.0, 120.0));
    ui.layout(Rect::new(0.0, 0.0, 300.0, 400.0), &mut list);
    assert_eq!(offset(&ui, scroll), Point::default());
    assert_eq!(ui.rect(pads[0]).unwrap().y0, 0.0);
    let image = CpuRenderer::new(Faces::bundled().unwrap())
        .render_to_image(&list, &theme(), 300, 400, 1.0)
        .unwrap();
    // Row 7 is at 280–320.
    let at = ((300 * 300 + 50) * 4) as usize;
    assert_eq!(&image.data()[at..at + 4], &rgba(ColorToken::Accent));
}

#[test]
fn dragging_the_thumb_moves_the_content_proportionally() {
    // 400 of content in 200: the thumb is half the track (100 px) and travels 100 px for 200 of
    // offset, so each pixel of drag scrolls two.
    let (mut ui, scroll, _, mut list) = area(ScrollArea::vertical(), 10, 100.0);
    let x = 300.0 - BAR / 2.0;
    button(
        &mut ui,
        &mut list,
        ButtonState::Pressed,
        Point::new(x, 50.0),
        1,
    );
    move_to(&mut ui, &mut list, Point::new(x, 70.0));
    assert_eq!(offset(&ui, scroll), Point::new(0.0, 40.0));
    // Captured: the drag continues beyond the bar, and clamps.
    move_to(&mut ui, &mut list, Point::new(10.0, 400.0));
    assert_eq!(offset(&ui, scroll), Point::new(0.0, 200.0));
    button(
        &mut ui,
        &mut list,
        ButtonState::Released,
        Point::new(10.0, 400.0),
        1,
    );
    // After the release, moving no longer scrolls.
    move_to(&mut ui, &mut list, Point::new(x, 10.0));
    assert_eq!(offset(&ui, scroll), Point::new(0.0, 200.0));
}

#[test]
fn a_press_on_the_track_moves_one_page() {
    let (mut ui, scroll, _, mut list) = area(ScrollArea::vertical(), 20, 100.0);
    let x = 300.0 - BAR / 2.0;
    // The thumb is at the top: a press near the bottom of the track pages down by the viewport
    // less a line.
    click(&mut ui, &mut list, Point::new(x, 190.0));
    assert_eq!(offset(&ui, scroll), Point::new(0.0, 200.0 - LINE));
    // A press above the thumb pages back up.
    click(&mut ui, &mut list, Point::new(x, 2.0));
    assert_eq!(offset(&ui, scroll), Point::default());
}

#[test]
fn keys_bubbling_from_inside_scroll_and_unusable_ones_go_on() {
    let (mut ui, scroll, pads, mut list) = area(ScrollArea::vertical(), 10, 100.0);
    ui.set_focus(Some(pads[0]), &mut list).unwrap();
    let none = Modifiers::default();
    key(&mut ui, &mut list, NamedKey::ArrowDown, none);
    assert_eq!(offset(&ui, scroll), Point::new(0.0, LINE));
    key(&mut ui, &mut list, NamedKey::PageDown, none);
    assert_eq!(offset(&ui, scroll), Point::new(0.0, LINE + 200.0 - LINE));
    key(&mut ui, &mut list, NamedKey::Home, none);
    assert_eq!(offset(&ui, scroll), Point::default());
    key(&mut ui, &mut list, NamedKey::End, none);
    assert_eq!(offset(&ui, scroll), Point::new(0.0, 200.0));
    // Horizontal arrows cannot move a vertical area: they go on (nothing else takes them).
    key(&mut ui, &mut list, NamedKey::ArrowRight, none);
    assert_eq!(offset(&ui, scroll), Point::new(0.0, 200.0));
}

#[test]
fn a_focusable_area_takes_the_focus_and_draws_its_ring_inside() {
    let (mut ui, scroll, _, mut list) = area(ScrollArea::vertical().focusable(), 10, 100.0);
    // The area comes first in Tab order (before its pads).
    key(&mut ui, &mut list, NamedKey::Tab, Modifiers::default());
    assert_eq!(ui.focused(), Some(scroll));
    // The ring is inside the viewport, near its right edge (the pads are only 100 wide): its
    // line is centred 1 + width / 2 px inside the viewport's edge.
    let width = f32::from(theme().shape().focus_width.max(1));
    let centre = 300.0 - BAR - 1.0 - width / 2.0;
    let ring = pixel(&list, (centre - 0.5) as u32, 100);
    assert_eq!(ring, rgba(ColorToken::Focus));
}

#[test]
fn tab_to_a_hidden_widget_brings_it_into_view() {
    let (mut ui, scroll, pads, mut list) = area(ScrollArea::vertical(), 10, 100.0);
    // Pads 0–4 are in view (0–200); Tab six times reaches pad 5 (200–240).
    for _ in 0..6 {
        key(&mut ui, &mut list, NamedKey::Tab, Modifiers::default());
    }
    assert_eq!(ui.focused(), Some(pads[5]));
    // It is brought just into view, with room for its focus ring (6 px).
    let rect = ui.rect(pads[5]).unwrap();
    assert!(rect.y1 <= 200.0 && rect.y0 >= 0.0, "{rect:?}");
    assert_eq!(offset(&ui, scroll), Point::new(0.0, 46.0));
    // Shift+Tab back to pad 0 brings the top back.
    for _ in 0..5 {
        key(&mut ui, &mut list, NamedKey::Tab, shift());
    }
    assert_eq!(ui.focused(), Some(pads[0]));
    assert_eq!(offset(&ui, scroll), Point::default());
}

#[test]
fn a_slider_inside_keeps_the_wheel_unless_shift_is_held() {
    // ADR-0046 V2 and V5: over a slider the wheel turns the slider; with Shift it scrolls.
    let mut ui: Ui<Msg> = Ui::new(
        Flex::column(Space::None, Space::None),
        &theme(),
        Faces::bundled().unwrap(),
    );
    let root = ui.root();
    let fill = LayoutProps::default()
        .width(Length::Fill(1.0))
        .height(Length::Fill(1.0));
    let scroll = ui.add(root, ScrollArea::vertical(), fill).unwrap();
    let column = ui
        .add(
            scroll,
            Flex::column(Space::None, Space::None),
            LayoutProps::default(),
        )
        .unwrap();
    let slider = ui
        .add(
            column,
            Slider::new(0.0, 100.0, 1.0, 50.0, Msg::Value),
            LayoutProps::default(),
        )
        .unwrap();
    for _ in 0..10 {
        ui.add(column, Pad::new(100.0, 40.0), LayoutProps::default())
            .unwrap();
    }
    let mut list = DisplayList::new();
    ui.layout(BOUNDS, &mut list);
    let r = ui.rect(slider).unwrap();
    move_to(
        &mut ui,
        &mut list,
        Point::new(f32::midpoint(r.x0, r.x1), f32::midpoint(r.y0, r.y1)),
    );
    wheel(&mut ui, &mut list, WheelDelta::Lines { x: 0.0, y: -1.0 });
    assert_eq!(messages(&mut ui), [Msg::Value(48.0)]);
    assert_eq!(offset(&ui, scroll), Point::default());
    // With Shift the slider lets the wheel go to the area (V5), which scrolls vertically: it
    // cannot scroll horizontally, so Shift does not turn the movement.
    hold(&mut ui, &mut list, shift());
    wheel(&mut ui, &mut list, WheelDelta::Lines { x: 0.0, y: -1.0 });
    assert_eq!(messages(&mut ui), []);
    assert_eq!(offset(&ui, scroll), Point::new(0.0, 3.0 * LINE));
}

#[test]
fn the_scroll_view_node_and_its_actions() {
    let (mut ui, scroll, pads, mut list) = area(ScrollArea::vertical(), 10, 100.0);
    let update = ui.take_access_update();
    let view = node(&update, scroll);
    assert_eq!(view.role(), Role::ScrollView);
    assert!(view.clips_children());
    assert_eq!(view.scroll_y(), Some(0.0));
    assert_eq!(view.scroll_y_max(), Some(200.0));
    // At the top: down is offered, up is not.
    assert!(view.supports_action(Action::ScrollDown));
    assert!(!view.supports_action(Action::ScrollUp));
    // A vertical area offers nothing sideways.
    assert!(!view.supports_action(Action::ScrollLeft));
    assert!(!view.supports_action(Action::ScrollRight));
    // Down by a line, then by a page.
    let scroll_by = |ui: &mut Ui<Msg>, list: &mut DisplayList, page: bool| {
        ui.access_action(
            AccessAction {
                target: scroll,
                kind: AccessActionKind::Scroll {
                    direction: ScrollDirection::Down,
                    page,
                },
            },
            list,
        )
        .unwrap();
    };
    scroll_by(&mut ui, &mut list, false);
    assert_eq!(offset(&ui, scroll), Point::new(0.0, LINE));
    scroll_by(&mut ui, &mut list, true);
    assert_eq!(offset(&ui, scroll), Point::new(0.0, 200.0));
    let update = ui.take_access_update();
    let view = node(&update, scroll);
    assert_eq!(view.scroll_y(), Some(200.0));
    assert!(view.supports_action(Action::ScrollUp));
    assert!(!view.supports_action(Action::ScrollDown));
    // The pads moved: their nodes are sent with their new bounds.
    assert_eq!(node(&update, pads[9]).bounds().unwrap().y1, 200.0);
    // Scroll a pad into view.
    ui.access_action(
        AccessAction {
            target: pads[0],
            kind: AccessActionKind::ScrollIntoView,
        },
        &mut list,
    )
    .unwrap();
    assert_eq!(offset(&ui, scroll), Point::default());
}

// The splitter.

/// A window with a horizontal splitter filling it, two pads as panes (they fill their panes).
fn split(splitter: Splitter<Msg>) -> (Ui<Msg>, WidgetId, [WidgetId; 2], DisplayList) {
    let mut ui = Ui::new(
        Flex::column(Space::None, Space::None),
        &theme(),
        Faces::bundled().unwrap(),
    );
    let root = ui.root();
    let fill = LayoutProps::default()
        .width(Length::Fill(1.0))
        .height(Length::Fill(1.0));
    let id = ui.add(root, splitter, fill).unwrap();
    let first = ui.add(id, Pad::new(10.0, 10.0), fill).unwrap();
    let second = ui.add(id, Pad::new(10.0, 10.0), fill).unwrap();
    let mut list = DisplayList::new();
    ui.layout(BOUNDS, &mut list);
    (ui, id, [first, second], list)
}

/// The divider band's thickness in comfortable density (the small gap, 8 px).
const BAND: f32 = 8.0;

#[test]
fn a_splitter_shares_the_space_and_keeps_the_minimums() {
    let (mut ui, _, [first, second], mut list) = split(Splitter::new(Axis::Horizontal, 0.25));
    // 300 − 8 = 292 shared: the first pane 73, then the band, then the rest.
    assert_eq!(ui.rect(first).unwrap(), Rect::new(0.0, 0.0, 73.0, 200.0));
    assert_eq!(
        ui.rect(second).unwrap(),
        Rect::new(73.0 + BAND, 0.0, 300.0, 200.0)
    );
    // Narrower window: 120 − 8 = 112, a quarter is 28, below the minimum of 48.
    ui.layout(Rect::new(0.0, 0.0, 120.0, 200.0), &mut list);
    assert_eq!(ui.rect(first).unwrap().width(), 48.0);
    // Too narrow for both minimums (80 − 8 = 72 < 96): shared by the minimums' proportion.
    ui.layout(Rect::new(0.0, 0.0, 80.0, 200.0), &mut list);
    assert_eq!(ui.rect(first).unwrap().width(), 36.0);
    assert_eq!(ui.rect(second).unwrap().width(), 36.0);
}

#[test]
fn dragging_the_divider_moves_it_fine_with_the_modifier() {
    let (mut ui, id, [first, _], mut list) =
        split(Splitter::new(Axis::Horizontal, 0.5).on_change(Msg::Share));
    // The band is at 146–154.
    let y = 100.0;
    button(
        &mut ui,
        &mut list,
        ButtonState::Pressed,
        Point::new(150.0, y),
        1,
    );
    // A press does not move it.
    assert_eq!(ui.rect(first).unwrap().x1, 146.0);
    move_to(&mut ui, &mut list, Point::new(180.0, y));
    assert_eq!(ui.rect(first).unwrap().x1, 176.0);
    assert_eq!(messages(&mut ui).len(), 1);
    // Fine: 30 px of pointer move it 3.
    hold(&mut ui, &mut list, primary());
    move_to(&mut ui, &mut list, Point::new(210.0, y));
    assert_eq!(ui.rect(first).unwrap().x1, 179.0);
    hold(&mut ui, &mut list, Modifiers::default());
    // Captured: past the window, it stops at the second pane's minimum (300 − 48 − 8 = 244).
    move_to(&mut ui, &mut list, Point::new(900.0, y));
    assert_eq!(ui.rect(first).unwrap().x1, 244.0);
    button(
        &mut ui,
        &mut list,
        ButtonState::Released,
        Point::new(900.0, y),
        1,
    );
    let share = ui.widget::<Splitter<Msg>>(id).unwrap().fraction();
    assert!((share - 244.0 / 292.0).abs() < 1e-6, "{share}");
}

#[test]
fn a_double_click_on_the_divider_restores_the_default() {
    let (mut ui, id, [first, _], mut list) =
        split(Splitter::new(Axis::Horizontal, 0.5).default_fraction(0.25));
    let y = 100.0;
    button(
        &mut ui,
        &mut list,
        ButtonState::Pressed,
        Point::new(150.0, y),
        1,
    );
    button(
        &mut ui,
        &mut list,
        ButtonState::Released,
        Point::new(150.0, y),
        1,
    );
    button(
        &mut ui,
        &mut list,
        ButtonState::Pressed,
        Point::new(150.0, y),
        2,
    );
    // A small move before the release does not undo it: no drag was started.
    move_to(&mut ui, &mut list, Point::new(153.0, y));
    button(
        &mut ui,
        &mut list,
        ButtonState::Released,
        Point::new(153.0, y),
        2,
    );
    assert_eq!(ui.widget::<Splitter<Msg>>(id).unwrap().fraction(), 0.25);
    assert_eq!(ui.rect(first).unwrap().x1, 73.0);
}

#[test]
fn the_wheel_over_the_divider_goes_on_to_the_area_around() {
    // The splitter's exception to ADR-0046 V2: a divider inside a vertical scroll area, the
    // pointer on its band; the wheel scrolls the area and leaves the divider where it is.
    let mut ui: Ui<Msg> = Ui::new(
        Flex::column(Space::None, Space::None),
        &theme(),
        Faces::bundled().unwrap(),
    );
    let root = ui.root();
    let fill = LayoutProps::default()
        .width(Length::Fill(1.0))
        .height(Length::Fill(1.0));
    let scroll = ui.add(root, ScrollArea::vertical(), fill).unwrap();
    let id = ui
        .add(
            scroll,
            Splitter::new(Axis::Horizontal, 0.5),
            LayoutProps::default()
                .width(Length::Fill(1.0))
                .height(Length::Fixed(600.0)),
        )
        .unwrap();
    ui.add(id, Pad::new(10.0, 10.0), fill).unwrap();
    ui.add(id, Pad::new(10.0, 10.0), fill).unwrap();
    let mut list = DisplayList::new();
    ui.layout(BOUNDS, &mut list);
    let r = ui.rect(id).unwrap();
    let band = f32::midpoint(r.x0, r.x1);
    move_to(&mut ui, &mut list, Point::new(band, 100.0));
    wheel(&mut ui, &mut list, WheelDelta::Lines { x: 0.0, y: -1.0 });
    assert_eq!(ui.widget::<Splitter<Msg>>(id).unwrap().fraction(), 0.5);
    assert_eq!(offset(&ui, scroll), Point::new(0.0, 3.0 * LINE));
}

#[test]
fn the_divider_answers_keys_only_when_focused() {
    let (mut ui, id, [first, _], mut list) = split(Splitter::new(Axis::Horizontal, 0.5));
    let share = |ui: &Ui<Msg>| ui.widget::<Splitter<Msg>>(id).unwrap().fraction();
    // A pane (a pad) focused: arrows bubble up through the splitter, which ignores them.
    ui.set_focus(Some(first), &mut list).unwrap();
    key(
        &mut ui,
        &mut list,
        NamedKey::ArrowRight,
        Modifiers::default(),
    );
    assert_eq!(share(&ui), 0.5);
    // The splitter focused.
    ui.set_focus(Some(id), &mut list).unwrap();
    key(
        &mut ui,
        &mut list,
        NamedKey::ArrowRight,
        Modifiers::default(),
    );
    assert!((share(&ui) - 0.52).abs() < 1e-9);
    key(&mut ui, &mut list, NamedKey::ArrowLeft, primary());
    assert!((share(&ui) - 0.515).abs() < 1e-9);
    key(&mut ui, &mut list, NamedKey::PageUp, Modifiers::default());
    assert!((share(&ui) - 0.465).abs() < 1e-9);
    key(&mut ui, &mut list, NamedKey::Home, Modifiers::default());
    assert_eq!(ui.rect(first).unwrap().width(), 48.0);
    key(&mut ui, &mut list, NamedKey::End, Modifiers::default());
    assert_eq!(ui.rect(first).unwrap().width(), 300.0 - BAND - 48.0);
}

#[test]
fn the_splitter_node_and_its_actions() {
    let (mut ui, id, _, mut list) = split(Splitter::new(Axis::Horizontal, 0.5).labelled("Panes"));
    let update = ui.take_access_update();
    let n = node(&update, id);
    assert_eq!(n.role(), Role::Splitter);
    assert_eq!(n.label(), Some("Panes"));
    assert_eq!(n.numeric_value(), Some(50.0));
    assert_eq!(n.numeric_value_step(), Some(0.5));
    let low = n.min_numeric_value().unwrap();
    assert!((low - 48.0 / 292.0 * 100.0).abs() < 1e-4, "{low}");
    // Panes side by side: the divider is vertical.
    assert_eq!(n.orientation(), Some(accesskit::Orientation::Vertical));
    // One above the other: horizontal.
    let (mut stacked, id_v, _, _) = split(Splitter::new(Axis::Vertical, 0.5));
    let update = stacked.take_access_update();
    assert_eq!(
        node(&update, id_v).orientation(),
        Some(accesskit::Orientation::Horizontal)
    );
    ui.access_action(
        AccessAction {
            target: id,
            kind: AccessActionKind::SetValue(25.0),
        },
        &mut list,
    )
    .unwrap();
    assert_eq!(ui.widget::<Splitter<Msg>>(id).unwrap().fraction(), 0.25);
    ui.access_action(
        AccessAction {
            target: id,
            kind: AccessActionKind::Increment,
        },
        &mut list,
    )
    .unwrap();
    assert!((ui.widget::<Splitter<Msg>>(id).unwrap().fraction() - 0.255).abs() < 1e-9);
}

#[test]
fn the_divider_is_quiet_under_the_pointer_and_accented_while_dragged() {
    let (mut ui, _, _, mut list) = split(Splitter::new(Axis::Horizontal, 0.5));
    // The two tokens must differ in this theme, or the hover would not be seen at all.
    assert_ne!(rgba(ColorToken::Border), rgba(ColorToken::ControlBorder));
    // The line in the middle of the band (146–154): 1 px from round(149.5) = 150.
    assert_eq!(pixel(&list, 150, 100), rgba(ColorToken::Border));
    assert_ne!(pixel(&list, 149, 100), rgba(ColorToken::Border));
    // Hovered (F2.09e, owner's choice): still 1 px, in the neutral control edge, no accent.
    move_to(&mut ui, &mut list, Point::new(150.0, 100.0));
    assert_eq!(pixel(&list, 150, 100), rgba(ColorToken::ControlBorder));
    assert_ne!(pixel(&list, 149, 100), rgba(ColorToken::ControlBorder));
    assert_ne!(pixel(&list, 151, 100), rgba(ColorToken::ControlBorder));
    // Dragged: 2 px of accent from round(149) = 149, so 149 and 150.
    button(
        &mut ui,
        &mut list,
        ButtonState::Pressed,
        Point::new(150.0, 100.0),
        1,
    );
    for x in 149..151 {
        assert_eq!(pixel(&list, x, 100), rgba(ColorToken::Accent), "x = {x}");
    }
    assert_ne!(pixel(&list, 151, 100), rgba(ColorToken::Accent));
    // Released over the band: hovered again, the quiet line.
    button(
        &mut ui,
        &mut list,
        ButtonState::Released,
        Point::new(150.0, 100.0),
        1,
    );
    assert_eq!(pixel(&list, 150, 100), rgba(ColorToken::ControlBorder));
    assert_ne!(pixel(&list, 149, 100), rgba(ColorToken::Accent));
    // Left (onto the first pane): back to the thin separator.
    move_to(&mut ui, &mut list, Point::new(50.0, 100.0));
    assert_eq!(pixel(&list, 150, 100), rgba(ColorToken::Border));
}

#[test]
fn the_renderer_reuses_a_rectangular_clip_only_for_the_same_rectangle() {
    use orchestra_ui::{RoundedRect, SegmentId};
    // Four segments, each filling the whole window inside its own clip: the same rectangle
    // twice (the second reuses the first's mask), another rectangle, then a rounded clip of the
    // first rectangle's size (it must not take the cached rectangular mask), and a nested clip.
    let mut list = DisplayList::new();
    let left = Rect::new(0.0, 0.0, 100.0, 200.0);
    let right = Rect::new(200.0, 0.0, 300.0, 100.0);
    let fill = |r: &mut Recorder<'_>, token| r.fill_rect(BOUNDS, token);
    list.record(SegmentId(1), |r| {
        r.push_clip_rect(left);
        fill(r, ColorToken::Accent);
        r.pop_clip();
    });
    list.record(SegmentId(2), |r| {
        r.push_clip_rect(left);
        r.fill_rect(Rect::new(0.0, 100.0, 300.0, 200.0), ColorToken::Error);
        r.pop_clip();
    });
    list.record(SegmentId(3), |r| {
        r.push_clip_rect(right);
        fill(r, ColorToken::Success);
        r.pop_clip();
    });
    list.record(SegmentId(4), |r| {
        r.push_clip(RoundedRect::new(
            Rect::new(110.0, 110.0, 190.0, 190.0),
            40.0,
        ));
        fill(r, ColorToken::Warning);
        r.pop_clip();
    });
    // A clip that draws nothing is dropped, and what the segment drew before it is kept.
    list.record(SegmentId(6), |r| {
        r.fill_rect(Rect::new(0.0, 190.0, 10.0, 200.0), ColorToken::Focus);
        r.push_clip_rect(right);
        r.pop_clip();
    });
    list.record(SegmentId(5), |r| {
        r.push_clip_rect(left);
        r.push_clip_rect(Rect::new(50.0, 0.0, 300.0, 20.0));
        fill(r, ColorToken::Text);
        r.pop_clip();
        r.pop_clip();
    });
    let px = |x, y| pixel(&list, x, y);
    assert_eq!(px(5, 195), rgba(ColorToken::Focus));
    assert_eq!(px(50, 50), rgba(ColorToken::Accent));
    assert_eq!(px(50, 150), rgba(ColorToken::Error));
    // Segment 2 stays inside the first rectangle, also with the same mask reused.
    assert_eq!(px(150, 150), rgba(ColorToken::Warning));
    assert_eq!(px(250, 150), rgba(ColorToken::Surface));
    assert_eq!(px(250, 50), rgba(ColorToken::Success));
    // The rounded clip (a circle: radius 40 in a square of 80): the top of the circle filled,
    // the square's corner not.
    assert_eq!(px(150, 112), rgba(ColorToken::Warning));
    assert_eq!(px(112, 112), rgba(ColorToken::Surface));
    // The nested clip: the intersection of both rectangles only.
    assert_eq!(px(60, 10), rgba(ColorToken::Text));
    assert_eq!(px(40, 10), rgba(ColorToken::Accent));
    assert_eq!(px(150, 10), rgba(ColorToken::Surface));
}

#[test]
fn tab_reaches_the_divider_between_the_panes() {
    let (mut ui, id, [first, second], mut list) = split(Splitter::new(Axis::Horizontal, 0.5));
    let mut order = Vec::new();
    for _ in 0..4 {
        key(&mut ui, &mut list, NamedKey::Tab, Modifiers::default());
        order.push(ui.focused().unwrap());
    }
    assert_eq!(order, [first, id, second, first]);
    // And backwards.
    key(&mut ui, &mut list, NamedKey::Tab, shift());
    assert_eq!(ui.focused(), Some(second));
    key(&mut ui, &mut list, NamedKey::Tab, shift());
    assert_eq!(ui.focused(), Some(id));
}
