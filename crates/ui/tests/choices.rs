// SPDX-License-Identifier: GPL-3.0-or-later

//! The widgets that choose a value and the large dialog (ADR-0050 DG8–DG9, SCALETTA F2.12a4a,
//! owner's choices 3, 5 and 6), headless: a drop-down's list opened by a click and by the
//! keyboard, a value chosen, a list closed without a choice, a second click that closes it; the
//! group of options and the list moved by the arrows and chosen by a click; the large dialog's
//! size, its content area, *Close*, Escape and the shortcut that opened it; the caret that stops
//! asking wake-ups when its blink is turned off.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test helpers: a failed unwrap or index fails the test"
)]

use orchestra_text::Faces;
use orchestra_ui::{
    ActionId, ActionSpec, Appearance, BuiltInTheme, ButtonState, Context, DIALOG_LARGE_MIN_SIZE,
    DIALOG_LARGE_SIZE, Defaults, DisplayList, DropDown, Flex, InputEvent, Key, LargeDialog,
    LayoutProps, Length, ListBox, Modifiers, NamedKey, PhysicalKey, Platform, Point, PointerButton,
    RadioGroup, Rect, Registry, Shortcut, ShortcutKey, Space, TextField, Ui, WidgetId,
};

/// The messages of these tests.
#[derive(Debug, Clone, PartialEq)]
enum Msg {
    /// The drop-down's new value.
    Pair(usize),
    /// The group's new value.
    Mode(usize),
    /// The list's new item.
    Section(usize),
    /// An action ran.
    Action(ActionId),
    /// The large dialog closed.
    Closed,
    /// The field committed its text.
    Committed,
}

/// The window of the tests, logical pixels.
const WINDOW: Rect = Rect::new(0.0, 0.0, 1000.0, 700.0);

/// An interface with a drop-down, a group of options and a list, and a global action
/// *Preferences* on Ctrl+, that the tests use to open a large dialog.
struct Bench {
    ui: Ui<Msg>,
    list: DisplayList,
    drop: WidgetId,
    group: WidgetId,
    sections: WidgetId,
    preferences: ActionId,
}

/// Three values, already "localised".
fn values(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| (*name).to_owned()).collect()
}

impl Bench {
    fn new() -> Self {
        let theme = *BuiltInTheme::Studio.pair().instance(Appearance::Light);
        let mut ui = Ui::new(
            Flex::column(Space::M, Space::M),
            &theme,
            Faces::bundled().unwrap(),
        );
        let mut registry = Registry::new(Platform::Linux);
        let preferences = registry
            .register(ActionSpec {
                identifier: "edit.preferences",
                name: "Preferences",
                context: Context::GLOBAL,
                defaults: Defaults::everywhere(&[Shortcut::new(
                    Modifiers {
                        primary: true,
                        ..Modifiers::default()
                    },
                    ShortcutKey::Character(','),
                )
                .unwrap()]),
                repeatable: false,
                edit: None,
            })
            .unwrap();
        ui.set_actions(registry, Msg::Action);
        let root = ui.root();
        let drop = ui
            .add(
                root,
                DropDown::new(values(&["Studio", "Forest", "Snow"]), 0, Msg::Pair).label("Theme"),
                LayoutProps::default(),
            )
            .unwrap();
        let group = ui
            .add(
                root,
                RadioGroup::new(values(&["System", "Light", "Dark"]), 0, Msg::Mode).label("Mode"),
                LayoutProps::default(),
            )
            .unwrap();
        let sections = ui
            .add(
                root,
                ListBox::new(values(&["Appearance", "Language"]), 0, Msg::Section),
                LayoutProps::default(),
            )
            .unwrap();
        let mut list = DisplayList::new();
        ui.layout(WINDOW, &mut list);
        Self {
            ui,
            list,
            drop,
            group,
            sections,
            preferences,
        }
    }

    fn key(&mut self, key: &Key, modifiers: Modifiers) {
        for state in [ButtonState::Pressed, ButtonState::Released] {
            self.ui.input(
                &InputEvent::Key {
                    key: key.clone(),
                    physical: PhysicalKey::Other,
                    state,
                    repeat: false,
                    modifiers,
                },
                &mut self.list,
            );
        }
    }

    fn named(&mut self, key: NamedKey) {
        self.key(&Key::Named(key), Modifiers::default());
    }

    /// A press and a release at `at`, with a move there first.
    fn click(&mut self, at: Point) {
        self.ui
            .input(&InputEvent::PointerMoved { position: at }, &mut self.list);
        for state in [ButtonState::Pressed, ButtonState::Released] {
            self.ui.input(
                &InputEvent::PointerButton {
                    button: PointerButton::Left,
                    state,
                    position: at,
                    clicks: 1,
                },
                &mut self.list,
            );
        }
    }

    /// The middle of `id`.
    fn centre(&self, id: WidgetId) -> Point {
        let rect = self.ui.rect(id).unwrap();
        Point::new(
            f32::midpoint(rect.x0, rect.x1),
            f32::midpoint(rect.y0, rect.y1),
        )
    }

    fn messages(&mut self) -> Vec<Msg> {
        self.ui.take_messages().collect()
    }

    fn focus(&mut self, id: WidgetId) {
        self.ui.set_focus(Some(id), &mut self.list).unwrap();
    }
}

#[test]
fn a_click_opens_the_list_and_a_click_on_a_value_chooses_it() {
    let mut bench = Bench::new();
    let at = bench.centre(bench.drop);
    bench.click(at);
    let menu = bench.ui.menus().next().expect("the list is open");
    let items: Vec<_> = bench.ui.menu_items(menu).collect();
    assert_eq!(items.len(), 3);
    // Nothing chosen by the release of the press that opened it.
    assert_eq!(bench.messages(), Vec::<Msg>::new());
    let snow = bench.centre(items[2]);
    bench.click(snow);
    assert_eq!(bench.messages(), vec![Msg::Pair(2)]);
    assert_eq!(bench.ui.menus().count(), 0);
    assert_eq!(
        bench
            .ui
            .widget::<DropDown<Msg>>(bench.drop)
            .unwrap()
            .selected(),
        2
    );
}

#[test]
fn the_keyboard_opens_the_list_on_the_value_in_use() {
    let mut bench = Bench::new();
    bench.focus(bench.drop);
    bench.named(NamedKey::Enter);
    assert_eq!(bench.ui.menus().count(), 1);
    // The value in use is highlighted: Enter keeps it and sends nothing.
    bench.named(NamedKey::Enter);
    assert_eq!(bench.messages(), Vec::<Msg>::new());
    assert_eq!(bench.ui.menus().count(), 0);
    // Down opens it too; Down again and Enter choose the next value.
    bench.named(NamedKey::ArrowDown);
    bench.named(NamedKey::ArrowDown);
    bench.named(NamedKey::Enter);
    assert_eq!(bench.messages(), vec![Msg::Pair(1)]);
    // The focus stayed on the drop-down.
    assert_eq!(bench.ui.focused(), Some(bench.drop));
}

#[test]
fn escape_and_a_second_click_close_the_list_without_a_choice() {
    let mut bench = Bench::new();
    bench.focus(bench.drop);
    bench.named(NamedKey::Space);
    assert_eq!(bench.ui.menus().count(), 1);
    bench.named(NamedKey::Escape);
    assert_eq!(bench.ui.menus().count(), 0);
    let drop = bench.ui.widget::<DropDown<Msg>>(bench.drop).unwrap();
    assert!(!drop.is_open());
    let at = bench.centre(bench.drop);
    bench.click(at);
    assert_eq!(bench.ui.menus().count(), 1);
    bench.click(at);
    assert_eq!(bench.ui.menus().count(), 0);
    assert_eq!(bench.messages(), Vec::<Msg>::new());
}

#[test]
fn the_group_moves_with_the_arrows_and_chooses_with_a_click() {
    let mut bench = Bench::new();
    bench.focus(bench.group);
    bench.named(NamedKey::ArrowRight);
    bench.named(NamedKey::End);
    // At the end: one more step does nothing.
    bench.named(NamedKey::ArrowRight);
    assert_eq!(bench.messages(), vec![Msg::Mode(1), Msg::Mode(2)]);
    let rect = bench.ui.rect(bench.group).unwrap();
    // The first segment of three.
    let first = Point::new(
        rect.x0 + rect.width() / 6.0,
        f32::midpoint(rect.y0, rect.y1),
    );
    bench.click(first);
    assert_eq!(bench.messages(), vec![Msg::Mode(0)]);
    // Tab leaves the group in one step (it is one stop).
    bench.named(NamedKey::Tab);
    assert_eq!(bench.ui.focused(), Some(bench.sections));
}

#[test]
fn the_list_selects_with_the_arrows_and_a_click() {
    let mut bench = Bench::new();
    bench.focus(bench.sections);
    bench.named(NamedKey::ArrowDown);
    bench.named(NamedKey::ArrowDown);
    bench.named(NamedKey::Home);
    assert_eq!(bench.messages(), vec![Msg::Section(1), Msg::Section(0)]);
    let rect = bench.ui.rect(bench.sections).unwrap();
    let second = Point::new(rect.x0 + 4.0, rect.y1 - 4.0);
    bench.click(second);
    assert_eq!(bench.messages(), vec![Msg::Section(1)]);
}

/// The large dialog of the tests, closed by the Preferences' action.
fn large(bench: &Bench) -> LargeDialog<Msg> {
    LargeDialog {
        title: "Preferences".to_owned(),
        close_text: "Close".to_owned(),
        close_message: Msg::Closed,
        closed_by: Some(bench.preferences),
    }
}

#[test]
fn the_large_dialog_has_its_size_and_a_content_area_to_fill() {
    let mut bench = Bench::new();
    let (panel, content) = bench
        .ui
        .open_large_dialog(large(&bench), &mut bench.list)
        .unwrap();
    let field = bench
        .ui
        .add(
            content,
            TextField::new("x", |_| Msg::Committed),
            LayoutProps::default().width(Length::Fill(1.0)),
        )
        .unwrap();
    bench.ui.focus_first(&mut bench.list);
    assert_eq!(bench.ui.focused(), Some(field));
    let rect = bench.ui.rect(panel).unwrap();
    assert!((rect.width() - DIALOG_LARGE_SIZE.0).abs() < 0.5, "{rect:?}");
    assert!(
        (rect.height() - DIALOG_LARGE_SIZE.1).abs() < 0.5,
        "{rect:?}"
    );
    // The content area spans the panel's width, less the padding.
    let area = bench.ui.rect(content).unwrap();
    assert!(area.width() > DIALOG_LARGE_SIZE.0 - 80.0, "{area:?}");
    // In a small window it shrinks, never below its minimum while the window allows it.
    bench
        .ui
        .layout(Rect::new(0.0, 0.0, 600.0, 400.0), &mut bench.list);
    let small = bench.ui.rect(panel).unwrap();
    assert!(small.width() >= DIALOG_LARGE_MIN_SIZE.0 - 0.5, "{small:?}");
    assert!(small.width() <= 600.0, "{small:?}");
}

#[test]
fn close_escape_and_the_opening_shortcut_close_the_large_dialog() {
    let mut bench = Bench::new();
    // Escape.
    bench
        .ui
        .open_large_dialog(large(&bench), &mut bench.list)
        .unwrap();
    bench.named(NamedKey::Escape);
    assert_eq!(bench.ui.dialogs().count(), 0);
    assert_eq!(bench.messages(), vec![Msg::Closed]);
    // The shortcut that opened it (Ctrl+,) closes it instead of running its action again.
    bench
        .ui
        .open_large_dialog(large(&bench), &mut bench.list)
        .unwrap();
    bench.key(
        &Key::Character(",".to_owned()),
        Modifiers {
            primary: true,
            ..Modifiers::default()
        },
    );
    assert_eq!(bench.ui.dialogs().count(), 0);
    assert_eq!(bench.messages(), vec![Msg::Closed]);
    // *Close*, which has the focus when nothing else was added.
    bench
        .ui
        .open_large_dialog(large(&bench), &mut bench.list)
        .unwrap();
    bench.named(NamedKey::Enter);
    assert_eq!(bench.ui.dialogs().count(), 0);
    assert_eq!(bench.messages(), vec![Msg::Closed]);
}

#[test]
fn a_caret_that_does_not_blink_asks_no_wake_up() {
    let mut bench = Bench::new();
    let root = bench.ui.root();
    let field = bench
        .ui
        .add(
            root,
            TextField::new("text", |_| Msg::Committed),
            LayoutProps::default().width(Length::Fixed(200.0)),
        )
        .unwrap();
    bench.ui.layout(WINDOW, &mut bench.list);
    bench.ui.set_caret_blink(false);
    bench.focus(field);
    assert_eq!(bench.ui.next_wakeup(), None);
    // Turned on again: the next key blinks it.
    bench.ui.set_caret_blink(true);
    bench.named(NamedKey::End);
    assert!(bench.ui.next_wakeup().is_some());
}
