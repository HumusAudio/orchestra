// SPDX-License-Identifier: GPL-3.0-or-later

//! Our own title bar (F2.11b2, ADR-0049 TB4–TB9), headless: its layout (buttons at either edge,
//! the menus compact at the leading side, the free space between), what it asks of the window
//! (drag only after the pointer moved, maximise on a double click, the window menu on a secondary
//! click, the buttons on release over them), the window's state (Restore while maximised, no
//! title, buttons or drag in full screen, the menus still there), and its accessibility (a
//! `TitleBar` node, buttons outside the Tab chain).

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test helpers: a failed unwrap or index fails the test"
)]

use accesskit::{NodeId, Role};
use orchestra_text::Faces;
use orchestra_ui::{
    ActionId, ActionSpec, Appearance, BuiltInTheme, ButtonState, Context, Defaults, DisplayList,
    Flex, InputEvent, KeyNames, LayoutProps, Length, MenuModel, Platform, Point, PointerButton,
    Rect, Registry, Space, TitleBarSpec, Ui, WidgetId, WindowButton, WindowButtonNames,
    WindowRequest, WindowState,
};

/// The messages of these tests.
#[derive(Debug, Clone, PartialEq)]
enum Msg {
    /// An action ran.
    Action(ActionId),
}

/// The window's width in these tests, logical pixels.
const WIDTH: f32 = 800.0;

/// The window's height in these tests, logical pixels.
const HEIGHT: f32 = 600.0;

/// The resize band of our frame (ADR-0049 TB7): the content leaves it free on the left, the
/// right and the bottom while the window is neither maximised nor in full screen.
const BAND: f32 = 6.0;

/// An interface with our title bar: a File menu, the buttons of `spec`, a button under the bar.
struct Bench {
    ui: Ui<Msg>,
    list: DisplayList,
    bar: WidgetId,
    content: WidgetId,
}

/// The buttons' names, in English (the catalogues hold the real ones).
fn names() -> WindowButtonNames {
    WindowButtonNames {
        close: "Close".to_owned(),
        maximize: "Maximize".to_owned(),
        restore: "Restore".to_owned(),
        minimize: "Minimize".to_owned(),
    }
}

impl Bench {
    /// A bar with `leading` and `trailing` buttons.
    fn new(leading: &[WindowButton], trailing: &[WindowButton]) -> Self {
        let theme = *BuiltInTheme::Studio.pair().instance(Appearance::Light);
        let mut registry = Registry::new(Platform::Linux);
        let quit = registry
            .register(ActionSpec {
                identifier: "file.quit",
                name: "Quit",
                context: Context::GLOBAL,
                defaults: Defaults::default(),
                repeatable: false,
                edit: None,
            })
            .unwrap();
        registry.set_texts(KeyNames::new(|key| format!("{key:?}")), str::to_owned);
        let model = MenuModel::new().submenu("File", Some('f'), MenuModel::new().action(quit));
        let mut ui = Ui::new(
            Flex::column(Space::None, Space::None),
            &theme,
            Faces::bundled().unwrap(),
        );
        ui.set_actions(registry, Msg::Action);
        let root = ui.root();
        let spec = TitleBarSpec {
            title: "Orchestra".to_owned(),
            leading: leading.to_vec(),
            trailing: trailing.to_vec(),
            names: names(),
        };
        let bar = ui.add_title_bar(root, &model, &spec).unwrap();
        let content = ui
            .add(
                root,
                Flex::column(Space::None, Space::None),
                LayoutProps::default()
                    .width(Length::Fill(1.0))
                    .height(Length::Fill(1.0)),
            )
            .unwrap();
        let mut list = DisplayList::new();
        ui.layout(Rect::new(0.0, 0.0, WIDTH, HEIGHT), &mut list);
        Self {
            ui,
            list,
            bar,
            content,
        }
    }

    /// The window buttons, in order.
    fn buttons(&self) -> Vec<WidgetId> {
        self.ui.title_bar_buttons().collect()
    }

    /// A press or release of `button` at `at`, the `clicks`-th of a series.
    fn button(&mut self, button: PointerButton, state: ButtonState, at: Point, clicks: u8) {
        self.ui.input(
            &InputEvent::PointerButton {
                button,
                state,
                position: at,
                clicks,
            },
            &mut self.list,
        );
    }

    /// A move of the pointer to `at`.
    fn move_to(&mut self, at: Point) {
        self.ui
            .input(&InputEvent::PointerMoved { position: at }, &mut self.list);
    }

    /// A primary click (press and release) at `at`.
    fn click(&mut self, at: Point) {
        self.move_to(at);
        self.button(PointerButton::Left, ButtonState::Pressed, at, 1);
        self.button(PointerButton::Left, ButtonState::Released, at, 1);
    }

    /// Every request the bar made since the last call.
    fn requests(&mut self) -> Vec<WindowRequest> {
        std::iter::from_fn(|| self.ui.take_window_request()).collect()
    }

    /// A point of the free space, well away from the menus and the buttons.
    fn free_point(&self) -> Point {
        let bar = self.ui.rect(self.bar).unwrap();
        Point::new(bar.x0 + bar.width() * 0.6, bar.y0 + bar.height() / 2.0)
    }

    /// The centre of `id`.
    fn centre(&self, id: WidgetId) -> Point {
        let rect = self.ui.rect(id).unwrap();
        Point::new(rect.x0 + rect.width() / 2.0, rect.y0 + rect.height() / 2.0)
    }
}

/// All three buttons, as GNOME's `:minimize,maximize,close`.
const THREE: [WindowButton; 3] = [
    WindowButton::Minimize,
    WindowButton::Maximize,
    WindowButton::Close,
];

#[test]
fn the_bar_spans_the_window_with_the_menus_at_the_start_and_the_buttons_at_the_end() {
    let bench = Bench::new(&[], &THREE);
    let bar = bench.ui.rect(bench.bar).unwrap();
    // Edge to edge but for the resize band at the sides; at the very top (the band lies over it).
    assert_eq!((bar.x0, bar.x1, bar.y0), (BAND, WIDTH - BAND, 0.0));
    // The content starts under the bar: one row, the menus inside it (ADR-0049 TB5).
    let content = bench.ui.rect(bench.content).unwrap();
    assert!((content.y0 - bar.y1).abs() < 0.01);
    let title = bench.ui.menu_bar_titles().next().unwrap();
    let file = bench.ui.rect(title).unwrap();
    assert!(file.x0 < 20.0, "File at the leading edge: {file:?}");
    let buttons: Vec<Rect> = bench
        .buttons()
        .iter()
        .map(|&id| bench.ui.rect(id).unwrap())
        .collect();
    assert_eq!(buttons.len(), 3);
    // Minimise, Maximise, Close, left to right, Close at the bar's end; square, its height.
    assert!((buttons[2].x1 - bar.x1).abs() < 0.01);
    assert!(buttons[0].x1 <= buttons[1].x0 && buttons[1].x1 <= buttons[2].x0);
    for button in &buttons {
        assert!((button.width() - bar.height()).abs() < 0.01, "{button:?}");
    }
}

#[test]
fn leading_buttons_come_before_the_menus() {
    let bench = Bench::new(&[WindowButton::Close], &[]);
    let close = bench.ui.rect(bench.buttons()[0]).unwrap();
    let file = bench
        .ui
        .rect(bench.ui.menu_bar_titles().next().unwrap())
        .unwrap();
    assert!(
        (close.x0 - BAND).abs() < 0.01,
        "Close at the bar's leading edge: {close:?}"
    );
    assert!(file.x0 >= close.x1, "File after it: {file:?}");
}

#[test]
fn the_free_space_drags_only_after_the_pointer_moves() {
    let mut bench = Bench::new(&[], &[WindowButton::Close]);
    let at = bench.free_point();
    bench.move_to(at);
    bench.button(PointerButton::Left, ButtonState::Pressed, at, 1);
    // A small move is still a click (a double click stays possible).
    bench.move_to(Point::new(at.x + 2.0, at.y + 1.0));
    assert_eq!(bench.requests(), vec![]);
    // Past the threshold: drag, once.
    bench.move_to(Point::new(at.x + 10.0, at.y));
    bench.move_to(Point::new(at.x + 20.0, at.y));
    assert_eq!(bench.requests(), vec![WindowRequest::Drag]);
    bench.button(PointerButton::Left, ButtonState::Released, at, 1);
    assert_eq!(bench.requests(), vec![]);
}

#[test]
fn a_double_click_maximises_and_a_secondary_click_shows_the_window_menu() {
    let mut bench = Bench::new(&[], &[WindowButton::Close]);
    let at = bench.free_point();
    bench.click(at);
    bench.button(PointerButton::Left, ButtonState::Pressed, at, 2);
    bench.button(PointerButton::Left, ButtonState::Released, at, 2);
    assert_eq!(bench.requests(), vec![WindowRequest::ToggleMaximized]);
    bench.button(PointerButton::Right, ButtonState::Pressed, at, 1);
    bench.button(PointerButton::Right, ButtonState::Released, at, 1);
    assert_eq!(bench.requests(), vec![WindowRequest::ShowWindowMenu { at }]);
}

#[test]
fn the_buttons_ask_on_release_over_them() {
    let mut bench = Bench::new(&[], &THREE);
    let buttons = bench.buttons();
    let expected = [
        WindowRequest::Minimize,
        WindowRequest::ToggleMaximized,
        WindowRequest::RequestClose,
    ];
    for (&button, request) in buttons.iter().zip(expected) {
        let at = bench.centre(button);
        bench.click(at);
        assert_eq!(bench.requests(), vec![request]);
    }
    // Pressed on Close, released elsewhere: cancelled, as every button.
    let at = bench.centre(buttons[2]);
    bench.move_to(at);
    bench.button(PointerButton::Left, ButtonState::Pressed, at, 1);
    let away = bench.free_point();
    bench.move_to(away);
    bench.button(PointerButton::Left, ButtonState::Released, away, 1);
    assert_eq!(bench.requests(), vec![]);
}

#[test]
fn the_menus_still_open_from_the_bar() {
    let mut bench = Bench::new(&[], &[WindowButton::Close]);
    let file = bench.ui.menu_bar_titles().next().unwrap();
    let at = bench.centre(file);
    bench.move_to(at);
    bench.button(PointerButton::Left, ButtonState::Pressed, at, 1);
    assert_eq!(bench.ui.menu_bar_open(), Some(0));
    assert_eq!(bench.requests(), vec![], "a title is not the free space");
}

#[test]
fn full_screen_hides_the_buttons_and_stops_the_drag_but_keeps_the_menus() {
    let mut bench = Bench::new(&[], &THREE);
    bench.ui.input(
        &InputEvent::WindowState(WindowState {
            maximized: false,
            fullscreen: true,
        }),
        &mut bench.list,
    );
    for button in bench.buttons() {
        assert!(
            bench.ui.rect(button).unwrap().width() < 0.01,
            "no room in full screen"
        );
    }
    let at = bench.free_point();
    bench.move_to(at);
    bench.button(PointerButton::Left, ButtonState::Pressed, at, 1);
    bench.move_to(Point::new(at.x + 30.0, at.y));
    bench.button(PointerButton::Left, ButtonState::Released, at, 1);
    bench.button(PointerButton::Left, ButtonState::Pressed, at, 2);
    assert_eq!(bench.requests(), vec![]);
    assert_eq!(bench.ui.menu_bar_titles().count(), 1);
    // Back to a window: the buttons return.
    bench.ui.input(
        &InputEvent::WindowState(WindowState::default()),
        &mut bench.list,
    );
    let close = bench.ui.rect(bench.buttons()[2]).unwrap();
    assert!(close.width() > 10.0);
}

#[test]
fn the_bar_and_its_buttons_describe_themselves_and_maximise_becomes_restore() {
    let mut bench = Bench::new(&[], &THREE);
    bench.ui.reset_access();
    let update = bench.ui.take_access_update();
    let node = |update: &orchestra_ui::AccessUpdate, id: WidgetId| {
        update
            .tree_update()
            .nodes
            .iter()
            .find(|(node, _)| *node == NodeId(id.get()))
            .map(|(_, node)| node.clone())
    };
    let bar = node(&update, bench.bar).unwrap();
    assert_eq!(bar.role(), Role::TitleBar);
    assert_eq!(bar.label(), Some("Orchestra"));
    let buttons = bench.buttons();
    let maximize = node(&update, buttons[1]).unwrap();
    assert_eq!(maximize.role(), Role::Button);
    assert_eq!(maximize.label(), Some("Maximize"));
    // Maximised: the same button is Restore, for assistive technologies too.
    bench.ui.input(
        &InputEvent::WindowState(WindowState {
            maximized: true,
            fullscreen: false,
        }),
        &mut bench.list,
    );
    let update = bench.ui.take_access_update();
    let restore = node(&update, buttons[1]).unwrap();
    assert_eq!(restore.label(), Some("Restore"));
}

#[test]
fn tab_never_stops_at_the_window_buttons() {
    let mut bench = Bench::new(&[], &THREE);
    let buttons = bench.buttons();
    for _ in 0..4 {
        bench.ui.input(
            &InputEvent::Key {
                key: orchestra_ui::Key::Named(orchestra_ui::NamedKey::Tab),
                physical: orchestra_ui::PhysicalKey::Other,
                state: ButtonState::Pressed,
                repeat: false,
                modifiers: orchestra_ui::Modifiers::default(),
            },
            &mut bench.list,
        );
        assert!(
            bench
                .ui
                .focused()
                .is_none_or(|focus| !buttons.contains(&focus)),
            "Tab reached a window button"
        );
    }
}

// ----- The frame's resize band (F2.11b3, ADR-0049 TB7) -----------------------------------------

#[test]
fn the_band_resizes_from_every_edge_and_corner() {
    use orchestra_ui::ResizeEdge;
    let mut bench = Bench::new(&[], &THREE);
    // The content leaves the band free at the sides and the bottom.
    let content = bench.ui.rect(bench.content).unwrap();
    assert_eq!(
        (content.x0, content.x1, content.y1),
        (BAND, WIDTH - BAND, HEIGHT - BAND)
    );
    for (at, edge) in [
        (Point::new(400.0, 1.0), ResizeEdge::North),
        (Point::new(400.0, HEIGHT - 1.0), ResizeEdge::South),
        (Point::new(1.0, 300.0), ResizeEdge::West),
        (Point::new(WIDTH - 1.0, 300.0), ResizeEdge::East),
        (Point::new(2.0, HEIGHT - 2.0), ResizeEdge::SouthWest),
        (Point::new(WIDTH - 2.0, 2.0), ResizeEdge::NorthEast),
    ] {
        // Over it: its resize shape; pressed: resize from it, and no widget gets the press.
        bench.move_to(at);
        assert_eq!(bench.ui.pointer_shape(), edge.pointer_shape(), "{edge:?}");
        bench.button(PointerButton::Left, ButtonState::Pressed, at, 1);
        bench.button(PointerButton::Left, ButtonState::Released, at, 1);
        assert_eq!(bench.requests(), vec![WindowRequest::Resize(edge)]);
    }
    // Back over the content: the arrow again.
    bench.move_to(Point::new(400.0, 300.0));
    assert_eq!(
        bench.ui.pointer_shape(),
        orchestra_ui::PointerShape::Default
    );
}

#[test]
fn the_top_of_the_band_lies_over_the_title_bar_not_above_it() {
    let mut bench = Bench::new(&[], &[WindowButton::Close]);
    // Just below the band the bar's free space drags as before.
    let at = Point::new(400.0, BAND + 2.0);
    bench.move_to(at);
    bench.button(PointerButton::Left, ButtonState::Pressed, at, 1);
    bench.move_to(Point::new(420.0, BAND + 2.0));
    assert_eq!(bench.requests(), vec![WindowRequest::Drag]);
}

#[test]
fn maximised_or_in_full_screen_there_is_no_band() {
    let mut bench = Bench::new(&[], &THREE);
    for state in [
        WindowState {
            maximized: true,
            fullscreen: false,
        },
        WindowState {
            maximized: false,
            fullscreen: true,
        },
    ] {
        bench
            .ui
            .input(&InputEvent::WindowState(state), &mut bench.list);
        // The content takes the whole window again.
        let content = bench.ui.rect(bench.content).unwrap();
        assert_eq!((content.x0, content.x1), (0.0, WIDTH), "{state:?}");
        let at = Point::new(1.0, 300.0);
        bench.move_to(at);
        assert_eq!(
            bench.ui.pointer_shape(),
            orchestra_ui::PointerShape::Default
        );
        bench.button(PointerButton::Left, ButtonState::Pressed, at, 1);
        bench.button(PointerButton::Left, ButtonState::Released, at, 1);
        assert_eq!(bench.requests(), vec![], "{state:?}");
    }
    // Restored: the band again.
    bench.ui.input(
        &InputEvent::WindowState(WindowState::default()),
        &mut bench.list,
    );
    let content = bench.ui.rect(bench.content).unwrap();
    assert_eq!(content.x0, BAND);
}

#[test]
fn an_open_menu_takes_the_press_on_the_band() {
    let mut bench = Bench::new(&[], &[WindowButton::Close]);
    let file = bench.ui.menu_bar_titles().next().unwrap();
    let at = bench.centre(file);
    bench.move_to(at);
    bench.button(PointerButton::Left, ButtonState::Pressed, at, 1);
    bench.button(PointerButton::Left, ButtonState::Released, at, 1);
    assert_eq!(bench.ui.menu_bar_open(), Some(0));
    // A press on the band closes the menu instead of resizing.
    let band = Point::new(1.0, 300.0);
    bench.move_to(band);
    bench.button(PointerButton::Left, ButtonState::Pressed, band, 1);
    assert_eq!(bench.requests(), vec![]);
    assert_eq!(bench.ui.menu_bar_open(), None);
}
