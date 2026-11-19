// SPDX-License-Identifier: GPL-3.0-or-later

//! The drawn menu bar and the mnemonics (F2.11d2b, ADR-0048 AM9, AM10, AM12), headless: the
//! pointer on the titles (open, switch by moving, close by pressing again, press-drag-release);
//! F10 and Alt pressed and released alone (and not when another key or a press came in between);
//! the keys on the active bar and in its menus (Left and Right across the titles, Escape back to
//! the title and out); Alt+letter before a focused text field, `AltGr` untouched; letters choosing
//! items; the underlined letters while Alt is held or the bar is used from the keyboard, never on
//! macOS; the accessibility tree.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test helpers: a failed unwrap or index fails the test"
)]

use accesskit::{NodeId, Role};
use orchestra_text::Faces;
use orchestra_ui::{
    ActionId, ActionSpec, Appearance, BuiltInTheme, ButtonState, Context, Defaults, DisplayList,
    EditConvention, EditRole, Flex, InputEvent, Key, KeyNames, LayoutProps, Length, MenuModel,
    Modifiers, NamedKey, PhysicalKey, Platform, Point, PointerButton, Rect, Registry, Shortcut,
    ShortcutKey, Space, TextField, Ui, WidgetId,
};

/// The messages of these tests.
#[derive(Debug, Clone, PartialEq)]
enum Msg {
    /// An action ran.
    Action(ActionId),
    /// The field committed its text.
    Committed(String),
}

/// No modifier.
const NONE: Modifiers = Modifiers {
    primary: false,
    secondary: false,
    shift: false,
    alt: false,
};

/// Alt alone.
const ALT: Modifiers = Modifiers { alt: true, ..NONE };

/// The actions of the bench.
#[derive(Debug, Clone, Copy)]
struct Ids {
    new: ActionId,
    save: ActionId,
    undo: ActionId,
    copy: ActionId,
    full_screen: ActionId,
}

/// The bench: a bar (File, Edit, View) above a text field holding "hello".
struct Bench {
    ui: Ui<Msg>,
    list: DisplayList,
    field: WidgetId,
    bar: WidgetId,
    ids: Ids,
}

impl Bench {
    /// The bench on Linux.
    fn new() -> Self {
        Self::on(Platform::Linux)
    }

    /// The bench on `platform`.
    fn on(platform: Platform) -> Self {
        let theme = *BuiltInTheme::Studio.pair().instance(Appearance::Light);
        let mut registry = Registry::new(platform);
        let primary = Modifiers {
            primary: true,
            ..NONE
        };
        let mut add = |identifier: &'static str, name: &'static str, key: char, edit| {
            registry
                .register(ActionSpec {
                    identifier,
                    name,
                    context: Context::GLOBAL,
                    defaults: Defaults::everywhere(&[Shortcut::new(
                        primary,
                        ShortcutKey::Character(key),
                    )
                    .unwrap()]),
                    repeatable: false,
                    edit,
                })
                .unwrap()
        };
        let ids = Ids {
            new: add("file.new", "New", 'n', None),
            save: add("file.save", "Save", 's', None),
            undo: add("edit.undo", "Undo", 'z', Some(EditRole::Undo)),
            copy: add("edit.copy", "Copy", 'c', Some(EditRole::Copy)),
            full_screen: add("view.full-screen", "Full screen", 'f', None),
        };
        registry.set_texts(KeyNames::new(|key| format!("{key:?}")), str::to_owned);
        // The first letter of each name, in lower case, as the mnemonic.
        registry.set_mnemonics(|name| name.chars().next());
        let mut ui = Ui::new(
            Flex::column(Space::None, Space::None),
            &theme,
            Faces::bundled().unwrap(),
        );
        ui.set_actions(registry, Msg::Action);
        let model = MenuModel::new()
            .submenu(
                "File",
                Some('F'),
                MenuModel::new().action(ids.new).action(ids.save),
            )
            .submenu(
                "Edit",
                Some('e'),
                MenuModel::new().action(ids.undo).action(ids.copy),
            )
            .submenu("View", Some('v'), MenuModel::new().action(ids.full_screen));
        let root = ui.root();
        let bar = ui
            .add_menu_bar(
                root,
                &model,
                LayoutProps::default().width(Length::Fill(1.0)),
            )
            .unwrap();
        let field = ui
            .add(
                root,
                TextField::new("hello", |text| Msg::Committed(text.to_owned()))
                    .convention(EditConvention::Linux),
                LayoutProps::default().width(Length::Fixed(300.0)),
            )
            .unwrap();
        let mut list = DisplayList::new();
        ui.layout(Rect::new(0.0, 0.0, 800.0, 600.0), &mut list);
        Self {
            ui,
            list,
            field,
            bar,
            ids,
        }
    }

    /// The middle of title `title`.
    fn title(&self, title: usize) -> Point {
        self.ui.menu_bar_title_point(title).unwrap()
    }

    /// Moves the pointer to `at`.
    fn move_to(&mut self, at: Point) {
        self.ui
            .input(&InputEvent::PointerMoved { position: at }, &mut self.list);
    }

    /// Presses or releases the primary button at `at` (the pointer moves there first).
    fn button(&mut self, state: ButtonState, at: Point) {
        self.move_to(at);
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

    /// A click at `at`.
    fn click(&mut self, at: Point) {
        self.button(ButtonState::Pressed, at);
        self.button(ButtonState::Released, at);
    }

    /// One key event, with the modifiers also reported as held.
    fn key_event(&mut self, key: Key, state: ButtonState, modifiers: Modifiers) {
        self.ui
            .input(&InputEvent::ModifiersChanged { modifiers }, &mut self.list);
        self.ui.input(
            &InputEvent::Key {
                key,
                physical: PhysicalKey::Other,
                state,
                repeat: false,
                modifiers,
            },
            &mut self.list,
        );
    }

    /// Presses and releases `key` with `modifiers` (which are then released too).
    fn press(&mut self, key: Key, modifiers: Modifiers) {
        self.key_event(key.clone(), ButtonState::Pressed, modifiers);
        self.key_event(key, ButtonState::Released, modifiers);
        self.ui.input(
            &InputEvent::ModifiersChanged { modifiers: NONE },
            &mut self.list,
        );
    }

    /// Presses and releases the named key `key` with no modifier.
    fn named(&mut self, key: NamedKey) {
        self.press(Key::Named(key), NONE);
    }

    /// Alt pressed and released alone.
    fn alt_alone(&mut self) {
        self.key_event(Key::Named(NamedKey::Alt), ButtonState::Pressed, ALT);
        self.ui.input(
            &InputEvent::ModifiersChanged { modifiers: NONE },
            &mut self.list,
        );
        self.key_event(Key::Named(NamedKey::Alt), ButtonState::Released, NONE);
    }

    /// The open menus.
    fn menus(&self) -> Vec<WidgetId> {
        self.ui.menus().collect()
    }

    /// The middle of item `item` of the open menu number `menu`.
    fn item(&self, menu: usize, item: usize) -> Point {
        let id = self.ui.menu_items(self.menus()[menu]).nth(item).unwrap();
        let rect = self.ui.rect(id).unwrap();
        Point::new(rect.x0 + 20.0, f32::midpoint(rect.y0, rect.y1))
    }

    /// The place of the highlighted item of the topmost menu.
    fn highlighted(&self) -> Option<usize> {
        let menu = *self.menus().last()?;
        let item = self.ui.highlighted_menu_item()?;
        self.ui.menu_items(menu).position(|id| id == item)
    }

    /// The messages since the last call.
    fn messages(&mut self) -> Vec<Msg> {
        self.ui.take_messages().collect()
    }

    /// The field's text.
    fn text(&self) -> String {
        self.ui
            .widget::<TextField<Msg>>(self.field)
            .unwrap()
            .text()
            .to_owned()
    }
}

#[test]
fn the_pointer_opens_switches_and_closes_the_bars_menus() {
    let mut bench = Bench::new();
    bench.click(bench.title(0));
    assert_eq!(bench.menus().len(), 1);
    assert_eq!(bench.ui.menu_bar_open(), Some(0));
    assert_eq!(bench.highlighted(), None, "opened by the pointer");
    // Moving over Edit opens Edit's menu instead.
    bench.move_to(bench.title(1));
    assert_eq!(bench.ui.menu_bar_open(), Some(1));
    assert_eq!(bench.menus().len(), 1);
    // A second press on the open title closes it.
    bench.click(bench.title(1));
    assert_eq!(bench.menus().len(), 0);
    assert_eq!(bench.ui.menu_bar_open(), None);
    // Press on File, drag to Save, release: Save runs.
    bench.button(ButtonState::Pressed, bench.title(0));
    let save = bench.item(0, 1);
    bench.move_to(save);
    bench.button(ButtonState::Released, save);
    assert_eq!(bench.menus().len(), 0);
    assert_eq!(bench.messages(), vec![Msg::Action(bench.ids.save)]);
}

#[test]
fn a_press_outside_leaves_the_bar() {
    let mut bench = Bench::new();
    bench.click(bench.title(0));
    bench.click(Point::new(700.0, 500.0));
    assert_eq!(bench.menus().len(), 0);
    assert_eq!(bench.ui.menu_bar_open(), None);
    assert_eq!(bench.ui.menu_bar_active(), None);
}

#[test]
fn f10_and_the_arrows_walk_the_bar_and_its_menus() {
    let mut bench = Bench::new();
    bench
        .ui
        .set_focus(Some(bench.field), &mut bench.list)
        .unwrap();
    bench.named(NamedKey::Function(10));
    assert_eq!(bench.ui.menu_bar_active(), Some(0));
    assert_eq!(
        bench.menus().len(),
        0,
        "F10 reaches the bar, it opens nothing"
    );
    assert!(bench.ui.mnemonics_underlined());
    // Right moves to Edit; Down opens it with its first item highlighted (Undo is disabled in
    // the field with an empty history: Copy).
    bench.named(NamedKey::ArrowRight);
    assert_eq!(bench.ui.menu_bar_active(), Some(1));
    bench.named(NamedKey::ArrowDown);
    assert_eq!(bench.ui.menu_bar_open(), Some(1));
    assert_eq!(bench.highlighted(), Some(1));
    // Right on an item without a submenu: View's menu; Left: back to Edit's.
    bench.named(NamedKey::ArrowRight);
    assert_eq!(bench.ui.menu_bar_open(), Some(2));
    assert_eq!(bench.highlighted(), Some(0));
    bench.named(NamedKey::ArrowLeft);
    assert_eq!(bench.ui.menu_bar_open(), Some(1));
    // Left from the first title wraps to the last.
    bench.named(NamedKey::ArrowLeft);
    bench.named(NamedKey::ArrowLeft);
    assert_eq!(bench.ui.menu_bar_open(), Some(2));
    // Escape: back to the title, the bar still active; a second Escape leaves it.
    bench.named(NamedKey::Escape);
    assert_eq!(bench.menus().len(), 0);
    assert_eq!(bench.ui.menu_bar_active(), Some(2));
    bench.named(NamedKey::Escape);
    assert_eq!(bench.ui.menu_bar_active(), None);
    assert!(!bench.ui.mnemonics_underlined());
    // The field kept the focus and its text through all of it.
    assert_eq!(bench.ui.focused(), Some(bench.field));
    assert_eq!(bench.text(), "hello");
    assert_eq!(bench.messages(), vec![]);
}

#[test]
fn a_letter_on_the_active_bar_opens_its_title_and_another_chooses_an_item() {
    let mut bench = Bench::new();
    bench.named(NamedKey::Function(10));
    bench.press(Key::Character("v".into()), NONE);
    assert_eq!(bench.ui.menu_bar_open(), Some(2));
    // "f" in the View menu: Full screen.
    bench.press(Key::Character("f".into()), NONE);
    assert_eq!(bench.menus().len(), 0);
    assert_eq!(bench.messages(), vec![Msg::Action(bench.ids.full_screen)]);
    // F10 inside a menu leaves everything.
    bench.named(NamedKey::Function(10));
    bench.named(NamedKey::Enter);
    assert_eq!(bench.menus().len(), 1);
    bench.named(NamedKey::Function(10));
    assert_eq!(bench.menus().len(), 0);
    assert_eq!(bench.ui.menu_bar_active(), None);
}

#[test]
fn alt_pressed_and_released_alone_reaches_the_bar() {
    let mut bench = Bench::new();
    bench.alt_alone();
    assert_eq!(bench.ui.menu_bar_active(), Some(0));
    // Alt again leaves it.
    bench.alt_alone();
    assert_eq!(bench.ui.menu_bar_active(), None);
    // Alt with a key in between (Alt+Tab, Alt+x) is not Alt alone.
    bench.key_event(Key::Named(NamedKey::Alt), ButtonState::Pressed, ALT);
    bench.press(Key::Character("x".into()), ALT);
    bench.key_event(Key::Named(NamedKey::Alt), ButtonState::Released, NONE);
    assert_eq!(bench.ui.menu_bar_active(), None);
    // Nor with a press in between (Alt+drag moves the window on KDE).
    bench.key_event(Key::Named(NamedKey::Alt), ButtonState::Pressed, ALT);
    bench.click(Point::new(700.0, 500.0));
    bench.key_event(Key::Named(NamedKey::Alt), ButtonState::Released, NONE);
    assert_eq!(bench.ui.menu_bar_active(), None);
    // Ctrl+Alt is not Alt alone either.
    let ctrl_alt = Modifiers {
        primary: true,
        ..ALT
    };
    bench.key_event(Key::Named(NamedKey::Alt), ButtonState::Pressed, ctrl_alt);
    bench.key_event(Key::Named(NamedKey::Alt), ButtonState::Released, NONE);
    assert_eq!(bench.ui.menu_bar_active(), None);
}

#[test]
fn alt_and_a_letter_open_a_title_before_the_focused_field() {
    let mut bench = Bench::new();
    bench
        .ui
        .set_focus(Some(bench.field), &mut bench.list)
        .unwrap();
    // The field selected everything when it got the focus; Alt+E opens Edit, the field types
    // nothing.
    bench.press(Key::Character("e".into()), ALT);
    assert_eq!(bench.ui.menu_bar_open(), Some(1));
    assert_eq!(bench.text(), "hello");
    // "c" chooses Copy, which copies the field's selection (ADR-0048 AM13).
    bench.press(Key::Character("c".into()), NONE);
    assert_eq!(bench.menus().len(), 0);
    assert_eq!(
        bench.ui.clipboard().read_text().unwrap().as_deref(),
        Some("hello")
    );
    assert_eq!(bench.messages(), vec![]);
    // Alt with the primary modifier (`AltGr` on Windows) is not a mnemonic: nothing opens.
    let alt_gr = Modifiers {
        primary: true,
        ..ALT
    };
    bench.press(Key::Character("e".into()), alt_gr);
    assert_eq!(bench.menus().len(), 0);
    // A letter that is no title's opens nothing.
    bench.press(Key::Character("q".into()), ALT);
    assert_eq!(bench.menus().len(), 0);
}

#[test]
fn the_letters_are_underlined_only_while_alt_is_held_or_the_keyboard_uses_the_bar() {
    let mut bench = Bench::new();
    assert!(!bench.ui.mnemonics_underlined());
    bench.ui.input(
        &InputEvent::ModifiersChanged { modifiers: ALT },
        &mut bench.list,
    );
    assert!(bench.ui.mnemonics_underlined());
    bench.ui.input(
        &InputEvent::ModifiersChanged { modifiers: NONE },
        &mut bench.list,
    );
    assert!(!bench.ui.mnemonics_underlined());
    // Opened by the pointer: not underlined.
    bench.click(bench.title(0));
    assert!(!bench.ui.mnemonics_underlined());
    bench.named(NamedKey::Escape);
    // macOS has no mnemonics, and no drawn bar: F10 and Alt do nothing there.
    let mut mac = Bench::on(Platform::MacOs);
    mac.ui.input(
        &InputEvent::ModifiersChanged { modifiers: ALT },
        &mut mac.list,
    );
    assert!(!mac.ui.mnemonics_underlined());
    mac.named(NamedKey::Function(10));
    mac.alt_alone();
    assert_eq!(mac.ui.menu_bar_active(), None);
}

#[test]
fn the_bar_describes_itself_to_assistive_technologies() {
    let mut bench = Bench::new();
    bench.ui.reset_access();
    let update = bench.ui.take_access_update();
    let node = |id: WidgetId| {
        update
            .tree_update()
            .nodes
            .iter()
            .find(|(node, _)| *node == NodeId(id.get()))
            .map(|(_, node)| node.clone())
            .unwrap()
    };
    assert_eq!(node(bench.bar).role(), Role::MenuBar);
    let titles: Vec<WidgetId> = bench.ui.menu_bar_titles().collect();
    assert_eq!(titles.len(), 3);
    let file = node(titles[0]);
    assert_eq!(file.role(), Role::MenuItem);
    assert_eq!(file.label(), Some("File"));
    assert_eq!(file.access_key(), Some("f"), "kept in lower case");
    assert_eq!(file.is_expanded(), Some(false));
    // F10: the focus assistive technologies see is the first title.
    bench.named(NamedKey::Function(10));
    let update = bench.ui.take_access_update();
    assert_eq!(update.tree_update().focus, NodeId(titles[0].get()));
    // A click from an assistive technology on Edit opens its menu, its title expanded.
    bench
        .ui
        .access_action(
            orchestra_ui::AccessAction {
                target: titles[1],
                kind: orchestra_ui::AccessActionKind::Click,
            },
            &mut bench.list,
        )
        .unwrap();
    assert_eq!(bench.ui.menu_bar_open(), Some(1));
    let update = bench.ui.take_access_update();
    let edit = update
        .tree_update()
        .nodes
        .iter()
        .find(|(node, _)| *node == NodeId(titles[1].get()))
        .map(|(_, node)| node.clone())
        .unwrap();
    assert_eq!(edit.is_expanded(), Some(true));
    let _ = bench.ids.new;
}
