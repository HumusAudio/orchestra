// SPDX-License-Identifier: GPL-3.0-or-later

//! Popups (F2.11c, ADR-0048 AM11 and AM14): the layers of the display list (a precision of
//! ADR-0039 D1), the menus' placement and routing (a press outside closes them and is consumed,
//! keys go to the topmost menu, the focus stays, the input method is off), and the tooltip's
//! timing, dismissal and placement, with made-up times (ADR-0045 WH2).

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test helpers: a failed unwrap or index fails the test"
)]

use std::time::Duration;

use orchestra_ui::{
    Appearance, BuiltInTheme, Button, ButtonState, ColorToken, Constraints, DisplayList, EventCx,
    Flex, Handled, InputEvent, Key, LayoutCx, LayoutProps, Length, Modifiers, NamedKey, Point,
    PointerButton, Popup, PopupSide, RecordCx, Recorder, Rect, Size, Space, Stack, Theme, Ui,
    WheelDelta, Widget, WidgetEvent, WidgetId,
};

/// The application's messages in these tests.
#[derive(Debug, Clone, PartialEq)]
enum Msg {
    /// A button was pressed.
    Clicked,
}

/// A leaf that wants a size, fills it with a colour, logs what it receives, and may handle
/// presses, keys, and take the focus and typed text.
#[derive(Debug)]
struct Probe {
    /// The size it asks for.
    size: Size,
    /// Its colour.
    color: ColorToken,
    /// Handles pointer presses and releases.
    handles_pointer: bool,
    /// Handles keys.
    handles_keys: bool,
    /// Takes the focus and typed text (like a text field).
    takes_text: bool,
    /// What it received, by name.
    log: Vec<&'static str>,
}

impl Probe {
    fn new(width: f32, height: f32, color: ColorToken) -> Self {
        Self {
            size: Size::new(width, height),
            color,
            handles_pointer: false,
            handles_keys: false,
            takes_text: false,
            log: Vec::new(),
        }
    }
    fn pointer(mut self) -> Self {
        self.handles_pointer = true;
        self
    }
    fn keys(mut self) -> Self {
        self.handles_keys = true;
        self
    }
    fn text(mut self) -> Self {
        self.takes_text = true;
        self
    }
}

impl Widget<Msg> for Probe {
    fn measure(&mut self, _cx: &mut LayoutCx<'_, Msg>, constraints: Constraints) -> Size {
        constraints.constrain(self.size)
    }

    fn record(&self, cx: &RecordCx, recorder: &mut Recorder<'_>) {
        recorder.fill_rect(cx.rect, self.color);
    }

    fn event(&mut self, _cx: &mut EventCx<'_, Msg>, event: &WidgetEvent<'_>) -> Handled {
        let name = match event {
            WidgetEvent::PointerButton {
                state: ButtonState::Pressed,
                ..
            } => "pressed",
            WidgetEvent::PointerButton { .. } => "released",
            WidgetEvent::Key { .. } => "key",
            WidgetEvent::Text(_) => "text",
            WidgetEvent::MenuClosed(_) => "menu-closed",
            WidgetEvent::FocusGained => "focus",
            WidgetEvent::FocusLost => "blur",
            _ => return Handled::No,
        };
        self.log.push(name);
        match event {
            WidgetEvent::PointerButton { .. } if self.handles_pointer => Handled::Yes,
            WidgetEvent::Key { .. } if self.handles_keys => Handled::Yes,
            WidgetEvent::Text(_) if self.takes_text => Handled::Yes,
            _ => Handled::No,
        }
    }

    fn focusable(&self) -> bool {
        self.takes_text
    }

    fn accepts_text(&self) -> bool {
        self.takes_text
    }
}

/// Studio light.
fn theme() -> Theme {
    *BuiltInTheme::Studio.pair().instance(Appearance::Light)
}

/// A window of 400 × 300.
fn bounds() -> Rect {
    Rect::from_origin_size(0.0, 0.0, 400.0, 300.0)
}

/// An interface whose root stacks its children over the whole window.
fn stacked() -> Ui<Msg> {
    Ui::new(
        Stack {
            padding: Space::None,
        },
        &theme(),
        orchestra_text::Faces::bundled().unwrap(),
    )
}

/// A press and a release at `(x, y)`.
fn click(ui: &mut Ui<Msg>, list: &mut DisplayList, x: f32, y: f32) {
    for state in [ButtonState::Pressed, ButtonState::Released] {
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
}

/// A key press of `key`.
fn press(ui: &mut Ui<Msg>, list: &mut DisplayList, key: NamedKey) {
    ui.input(
        &InputEvent::Key {
            key: Key::Named(key),
            state: ButtonState::Pressed,
            repeat: false,
            modifiers: Modifiers::default(),
        },
        list,
    );
}

/// The pointer moves to `(x, y)`.
fn move_to(ui: &mut Ui<Msg>, list: &mut DisplayList, x: f32, y: f32) {
    ui.input(
        &InputEvent::PointerMoved {
            position: Point::new(x, y),
        },
        list,
    );
}

/// The log of probe `id`.
fn log(ui: &Ui<Msg>, id: WidgetId) -> Vec<&'static str> {
    ui.widget::<Probe>(id).unwrap().log.clone()
}

/// The colour of pixel `(x, y)` of the list drawn in a 400 × 300 image.
fn pixel(list: &DisplayList, x: u32, y: u32) -> [u8; 4] {
    orchestra_ui::CpuRenderer::new(orchestra_text::Faces::bundled().unwrap())
        .render_to_image(list, &theme(), 400, 300, 1.0)
        .unwrap()
        .pixel(x, y)
        .unwrap()
}

/// A colour token as the pixel it gives with the test theme, opaque.
fn rgba(token: ColorToken) -> [u8; 4] {
    let color = theme().color(token);
    [color.r, color.g, color.b, 255]
}

/// A menu of one probe, opened below `anchor`.
fn open(
    ui: &mut Ui<Msg>,
    list: &mut DisplayList,
    anchor: Rect,
    owner: Option<WidgetId>,
) -> (WidgetId, WidgetId) {
    let menu = ui
        .open_menu(
            Flex::column(Space::None, Space::None),
            Popup {
                anchor,
                side: PopupSide::Below,
                owner,
            },
            list,
        )
        .unwrap();
    let item = ui
        .add(
            menu,
            Probe::new(100.0, 50.0, ColorToken::Accent).pointer().keys(),
            LayoutProps::default(),
        )
        .unwrap();
    ui.frame(list);
    (menu, item)
}

#[test]
fn a_menu_stays_above_widgets_added_after_it() {
    let mut ui = stacked();
    let root = ui.root();
    let mut list = DisplayList::new();
    ui.layout(bounds(), &mut list);
    let (_menu, item) = open(&mut ui, &mut list, Rect::new(20.0, 0.0, 20.0, 10.0), None);
    assert_eq!(ui.rect(item), Some(Rect::new(20.0, 10.0, 120.0, 60.0)));
    // A widget of the main interface recorded for the first time now, under the menu: before
    // the layers it would have been drawn over it.
    ui.add(
        root,
        Probe::new(400.0, 300.0, ColorToken::Error),
        LayoutProps::default(),
    )
    .unwrap();
    ui.frame(&mut list);
    assert_eq!(pixel(&list, 50, 30), rgba(ColorToken::Accent), "the menu");
    assert_eq!(
        pixel(&list, 300, 200),
        rgba(ColorToken::Error),
        "the new widget"
    );
}

#[test]
fn a_menu_is_placed_inside_the_window() {
    let mut ui = stacked();
    let mut list = DisplayList::new();
    ui.layout(bounds(), &mut list);
    // Near the bottom right corner: flipped above its anchor and pushed left.
    let (_menu, item) = open(
        &mut ui,
        &mut list,
        Rect::new(350.0, 280.0, 390.0, 295.0),
        None,
    );
    assert_eq!(ui.rect(item), Some(Rect::new(300.0, 230.0, 400.0, 280.0)));
    // A smaller window moves it again.
    ui.layout(Rect::from_origin_size(0.0, 0.0, 380.0, 300.0), &mut list);
    assert_eq!(ui.rect(item), Some(Rect::new(280.0, 230.0, 380.0, 280.0)));
}

#[test]
fn a_press_outside_closes_the_menus_and_is_consumed() {
    let mut ui = Ui::new(
        Flex::column(Space::None, Space::None),
        &theme(),
        orchestra_text::Faces::bundled().unwrap(),
    );
    let root = ui.root();
    // The menu's owner (a small title at the top) and, below it, a widget that takes presses.
    let owner = ui
        .add(
            root,
            Probe::new(40.0, 20.0, ColorToken::Control),
            LayoutProps::default(),
        )
        .unwrap();
    let under = ui
        .add(
            root,
            Probe::new(400.0, 280.0, ColorToken::Control).pointer(),
            LayoutProps::default(),
        )
        .unwrap();
    let mut list = DisplayList::new();
    ui.layout(bounds(), &mut list);
    let segments = list.segment_count();
    let anchor = ui.rect(owner).unwrap();
    let (menu, _item) = open(&mut ui, &mut list, anchor, Some(owner));
    assert_eq!(ui.menus().collect::<Vec<_>>(), vec![menu]);
    click(&mut ui, &mut list, 300.0, 200.0);
    assert_eq!(ui.menus().count(), 0);
    // The owner was told once; the widget under the press got neither it nor its release.
    assert_eq!(log(&ui, owner), vec!["menu-closed"]);
    assert_eq!(log(&ui, under), [] as [&str; 0]);
    assert_eq!(
        list.segment_count(),
        segments,
        "the menu's segments are gone"
    );
    // Afterwards a press reaches the main interface again.
    click(&mut ui, &mut list, 300.0, 200.0);
    assert_eq!(log(&ui, under), vec!["pressed", "released"]);
}

#[test]
fn the_owner_is_still_reached_while_its_menu_is_open() {
    let mut ui = Ui::new(
        Flex::column(Space::None, Space::None),
        &theme(),
        orchestra_text::Faces::bundled().unwrap(),
    );
    let root = ui.root();
    let title = ui
        .add(
            root,
            Probe::new(60.0, 20.0, ColorToken::Control).pointer(),
            LayoutProps::default(),
        )
        .unwrap();
    let other = ui
        .add(
            root,
            Probe::new(60.0, 20.0, ColorToken::Control).pointer(),
            LayoutProps::default(),
        )
        .unwrap();
    let mut list = DisplayList::new();
    ui.layout(bounds(), &mut list);
    let anchor = ui.rect(title).unwrap();
    let (_menu, _item) = open(&mut ui, &mut list, anchor, Some(title));
    // The other widget sits under the menu's item; next to the menu, at x = 200, it is blocked.
    click(&mut ui, &mut list, 10.0, 10.0);
    assert_eq!(log(&ui, title), vec!["pressed", "released"]);
    assert_eq!(ui.menus().count(), 1, "a press on the owner keeps the menu");
    assert_eq!(log(&ui, other), [] as [&str; 0]);
}

#[test]
fn keys_go_to_the_topmost_menu_and_the_focus_stays() {
    let mut ui = Ui::new(
        Flex::column(Space::None, Space::None),
        &theme(),
        orchestra_text::Faces::bundled().unwrap(),
    );
    let root = ui.root();
    let field = ui
        .add(
            root,
            Probe::new(200.0, 30.0, ColorToken::SurfaceSunken)
                .text()
                .keys(),
            LayoutProps::default(),
        )
        .unwrap();
    let mut list = DisplayList::new();
    ui.layout(bounds(), &mut list);
    ui.set_focus(Some(field), &mut list).unwrap();
    assert!(ui.text_input().is_some(), "the field takes text");
    // A menu whose root is itself a probe: keys go to the menu's root (the menu widget of F2.11d
    // moves its own highlight among its items).
    let item = ui
        .open_menu(
            Probe::new(100.0, 50.0, ColorToken::Accent).pointer().keys(),
            Popup {
                anchor: Rect::new(10.0, 40.0, 10.0, 40.0),
                side: PopupSide::Below,
                owner: None,
            },
            &mut list,
        )
        .unwrap();
    assert!(ui.text_input().is_none(), "the input method is off");
    press(&mut ui, &mut list, NamedKey::Tab);
    ui.input(&InputEvent::Text("a".to_owned()), &mut list);
    assert_eq!(log(&ui, item), vec!["key"]);
    assert_eq!(log(&ui, field), vec!["focus"], "no key, no text");
    assert_eq!(ui.focused(), Some(field), "Tab moved nothing");
    // A press in the menu moves no focus either.
    click(&mut ui, &mut list, 50.0, 60.0);
    assert_eq!(ui.focused(), Some(field));
    assert_eq!(log(&ui, item), vec!["key", "pressed", "released"]);
}

#[test]
fn closing_a_menu_closes_its_submenus_without_telling_the_owner() {
    let mut ui = stacked();
    let root = ui.root();
    let owner = ui
        .add(
            root,
            Probe::new(40.0, 20.0, ColorToken::Control),
            LayoutProps::default(),
        )
        .unwrap();
    let mut list = DisplayList::new();
    ui.layout(bounds(), &mut list);
    let (menu, item) = open(
        &mut ui,
        &mut list,
        Rect::new(0.0, 0.0, 0.0, 0.0),
        Some(owner),
    );
    let anchor = ui.rect(item).unwrap();
    let submenu = ui
        .open_menu(
            Flex::column(Space::None, Space::None),
            Popup {
                anchor,
                side: PopupSide::Right,
                owner: Some(item),
            },
            &mut list,
        )
        .unwrap();
    assert_eq!(ui.menus().collect::<Vec<_>>(), vec![menu, submenu]);
    ui.close_menu(menu, &mut list).unwrap();
    assert_eq!(ui.menus().count(), 0);
    assert_eq!(log(&ui, owner), [] as [&str; 0]);
    assert!(ui.close_menu(menu, &mut list).is_err(), "no longer open");
}

#[test]
fn the_window_losing_the_focus_closes_the_menus() {
    let mut ui = stacked();
    let mut list = DisplayList::new();
    ui.layout(bounds(), &mut list);
    open(&mut ui, &mut list, Rect::new(0.0, 0.0, 0.0, 0.0), None);
    ui.input(&InputEvent::Focus(false), &mut list);
    assert_eq!(ui.menus().count(), 0);
}

/// An interface with two buttons side by side, each too narrow for its text (so both are elided
/// and have a tooltip). Side by side, so that moving from one to the other does not cross the
/// first one's tooltip, which opens below the pointer.
fn two_buttons() -> (Ui<Msg>, DisplayList, WidgetId, WidgetId) {
    let mut ui = Ui::new(
        Flex::row(Space::None, Space::None),
        &theme(),
        orchestra_text::Faces::bundled().unwrap(),
    );
    let root = ui.root();
    let narrow = LayoutProps::default().width(Length::Fixed(60.0));
    let first = ui
        .add(root, Button::new("Reverb send level", Msg::Clicked), narrow)
        .unwrap();
    let second = ui
        .add(
            root,
            Button::new("Delay feedback amount", Msg::Clicked),
            narrow,
        )
        .unwrap();
    let mut list = DisplayList::new();
    ui.layout(bounds(), &mut list);
    (ui, list, first, second)
}

/// The centre of `id`'s rectangle.
fn centre(ui: &Ui<Msg>, id: WidgetId) -> (f32, f32) {
    let rect = ui.rect(id).unwrap();
    (
        f32::midpoint(rect.x0, rect.x1),
        f32::midpoint(rect.y0, rect.y1),
    )
}

#[test]
fn a_tooltip_shows_after_500_ms_with_the_whole_text() {
    let (mut ui, mut list, first, _second) = two_buttons();
    let start = ui.now();
    let (x, y) = centre(&ui, first);
    move_to(&mut ui, &mut list, x, y);
    assert_eq!(ui.next_wakeup(), Some(start + Duration::from_millis(500)));
    ui.advance(start + Duration::from_millis(499), &mut list);
    assert_eq!(ui.tooltip_text(), None, "not yet");
    // Moving over the same button does not restart the timer.
    move_to(&mut ui, &mut list, x + 1.0, y);
    ui.advance(start + Duration::from_millis(500), &mut list);
    assert_eq!(ui.tooltip_text(), Some("Reverb send level"));
    assert_eq!(ui.next_wakeup(), None, "nothing more to wait for");
    // Below the pointer, inside the window, at most 320 pixels wide.
    let rect = ui.tooltip_rect().unwrap();
    assert_eq!(
        rect.y0,
        y.round() + 20.0,
        "20 px below the hot spot, rounded"
    );
    assert!(rect.width() <= 320.0 && rect.x1 <= 400.0);
}

#[test]
fn the_next_tooltip_shows_at_once_after_a_move() {
    let (mut ui, mut list, first, second) = two_buttons();
    let start = ui.now();
    let (x, y) = centre(&ui, first);
    move_to(&mut ui, &mut list, x, y);
    ui.advance(start + Duration::from_millis(500), &mut list);
    let (x, y) = centre(&ui, second);
    // 400 ms later, on the next button: shown at once, with its own text.
    ui.advance(start + Duration::from_millis(900), &mut list);
    move_to(&mut ui, &mut list, x, y);
    assert_eq!(ui.tooltip_text(), Some("Delay feedback amount"));
    // Off both buttons: gone; 600 ms later back on one: the 500 ms again.
    move_to(&mut ui, &mut list, 300.0, 250.0);
    assert_eq!(ui.tooltip_text(), None);
    ui.advance(start + Duration::from_millis(1500), &mut list);
    move_to(&mut ui, &mut list, x, y);
    assert_eq!(ui.tooltip_text(), None, "the warm time is over");
}

#[test]
fn a_press_a_key_or_the_wheel_dismisses_the_tooltip_until_the_pointer_leaves() {
    let (mut ui, mut list, first, _second) = two_buttons();
    let start = ui.now();
    let (x, y) = centre(&ui, first);
    let mut at = 0;
    let mut show = |ui: &mut Ui<Msg>, list: &mut DisplayList| {
        move_to(ui, list, 300.0, 250.0);
        at += 2000;
        ui.advance(start + Duration::from_millis(at), list);
        move_to(ui, list, x, y);
        ui.advance(start + Duration::from_millis(at + 500), list);
        assert!(ui.tooltip_text().is_some());
    };
    show(&mut ui, &mut list);
    press(&mut ui, &mut list, NamedKey::Escape);
    assert_eq!(ui.tooltip_text(), None, "a key");
    // Resting on the same button brings nothing back.
    move_to(&mut ui, &mut list, x + 2.0, y);
    assert_eq!(ui.next_wakeup(), None);
    show(&mut ui, &mut list);
    click(&mut ui, &mut list, x, y);
    assert_eq!(ui.tooltip_text(), None, "a press");
    assert_eq!(
        ui.take_messages().collect::<Vec<_>>(),
        vec![Msg::Clicked],
        "the press still clicks"
    );
    show(&mut ui, &mut list);
    ui.input(
        &InputEvent::Wheel {
            delta: WheelDelta::Lines { x: 0.0, y: 1.0 },
        },
        &mut list,
    );
    assert_eq!(ui.tooltip_text(), None, "the wheel");
    show(&mut ui, &mut list);
    ui.input(&InputEvent::PointerLeft, &mut list);
    assert_eq!(ui.tooltip_text(), None, "the pointer left the window");
}

#[test]
fn the_pointer_can_move_onto_the_tooltip() {
    let (mut ui, mut list, first, _second) = two_buttons();
    let start = ui.now();
    let (x, y) = centre(&ui, first);
    move_to(&mut ui, &mut list, x, y);
    ui.advance(start + Duration::from_millis(500), &mut list);
    let rect = ui.tooltip_rect().unwrap();
    // Down onto the tooltip, off the button: the tooltip stays.
    move_to(&mut ui, &mut list, rect.x0 + 4.0, rect.y0 + 4.0);
    assert_eq!(ui.tooltip_text(), Some("Reverb send level"));
}

#[test]
fn a_whole_text_has_no_tooltip_and_the_tooltip_is_not_for_assistive_technologies() {
    let (mut ui, mut list, first, _second) = two_buttons();
    let root = ui.root();
    let wide = ui
        .add(
            root,
            Button::new("OK", Msg::Clicked),
            LayoutProps::default(),
        )
        .unwrap();
    ui.frame(&mut list);
    let nodes = ui.take_access_update().into_tree_update().nodes.len();
    let start = ui.now();
    let (x, y) = centre(&ui, wide);
    move_to(&mut ui, &mut list, x, y);
    ui.advance(start + Duration::from_millis(600), &mut list);
    assert_eq!(ui.tooltip_text(), None, "the whole text shows");
    let (x, y) = centre(&ui, first);
    move_to(&mut ui, &mut list, x, y);
    ui.advance(start + Duration::from_millis(1200), &mut list);
    assert!(ui.tooltip_text().is_some());
    ui.reset_access();
    assert_eq!(
        ui.take_access_update().into_tree_update().nodes.len(),
        nodes,
        "no node for the tooltip"
    );
}

#[test]
fn a_menu_hides_the_tooltip_and_none_shows_while_it_is_open() {
    let (mut ui, mut list, first, _second) = two_buttons();
    let start = ui.now();
    let (x, y) = centre(&ui, first);
    move_to(&mut ui, &mut list, x, y);
    ui.advance(start + Duration::from_millis(500), &mut list);
    assert!(ui.tooltip_text().is_some());
    open(
        &mut ui,
        &mut list,
        Rect::new(200.0, 100.0, 200.0, 100.0),
        None,
    );
    assert_eq!(ui.tooltip_text(), None);
    move_to(&mut ui, &mut list, x + 1.0, y);
    assert_eq!(ui.next_wakeup(), None);
}
