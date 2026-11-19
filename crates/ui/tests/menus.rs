// SPDX-License-Identifier: GPL-3.0-or-later

//! Drawn menus and context menus (F2.11d2a, ADR-0048 AM9, AM11–AM13), headless: shortcuts
//! written in each platform's style and language; the registry's texts and its marks of changed
//! actions; a text field's context menu opened by the secondary button (on the press on Linux, on
//! the release on Windows), by the Menu key and Shift+F10, and by an assistive technology; the
//! keyboard inside a menu (wrapping, skipping separators and disabled items, Home and End,
//! submenus with Right and Left, Escape); the pointer (the release that opened nothing chooses
//! nothing, press-drag-release, submenus after 250 ms with made-up times); the items acting on the
//! focused field (AM13); states changed while a menu is open; the accessibility tree.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test helpers: a failed unwrap or index fails the test"
)]

use std::time::Duration;

use accesskit::{Action, NodeId, Role};
use orchestra_text::Faces;
use orchestra_ui::{
    AccessAction, AccessActionKind, ActionId, ActionSpec, ActionState, Appearance, BuiltInTheme,
    ButtonState, Context, Defaults, DisplayList, EditConvention, EditRole, Flex, InputEvent, Key,
    KeyName, KeyNames, LayoutProps, Length, MenuModel, Modifiers, NamedKey, PhysicalKey, Platform,
    Point, PointerButton, Rect, Registry, Shortcut, ShortcutKey, Space, TextField, Ui, WidgetId,
};

/// The messages of these tests.
#[derive(Debug, Clone, PartialEq)]
enum Msg {
    /// An action ran (the interface's message for it).
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

/// The primary modifier.
const PRIMARY: Modifiers = Modifiers {
    primary: true,
    ..NONE
};

/// The primary modifier and Shift.
const PRIMARY_SHIFT: Modifiers = Modifiers {
    shift: true,
    ..PRIMARY
};

/// Shift alone.
const SHIFT: Modifiers = Modifiers {
    shift: true,
    ..NONE
};

/// A shortcut of a character key.
fn ch(modifiers: Modifiers, character: char) -> Shortcut {
    Shortcut::new(modifiers, ShortcutKey::Character(character)).unwrap()
}

/// The keys' names in a made-up English: the variant's name, "F" and the number for a function
/// key, "Num " and the digit for the keypad.
fn english_keys() -> KeyNames {
    KeyNames::new(|key| match key {
        KeyName::Function(number) => format!("F{number}"),
        KeyName::Numpad(digit) => format!("Num {digit}"),
        other => format!("{other:?}"),
    })
}

/// The keys' names as the German catalogue has them (the glossary's choices).
fn german_keys() -> KeyNames {
    KeyNames::new(|key| match key {
        KeyName::Ctrl => "Strg".to_owned(),
        KeyName::Shift => "Umschalt".to_owned(),
        KeyName::Delete => "Entf".to_owned(),
        KeyName::Space => "Leertaste".to_owned(),
        KeyName::Function(number) => format!("F{number}"),
        other => format!("{other:?}"),
    })
}

/// The actions of the bench.
#[derive(Debug, Clone, Copy)]
struct Ids {
    undo: ActionId,
    redo: ActionId,
    cut: ActionId,
    copy: ActionId,
    paste: ActionId,
    select_all: ActionId,
    save: ActionId,
    grid: ActionId,
}

/// An action of the global context with `shortcuts` everywhere, named by `name`.
fn spec(
    identifier: &'static str,
    name: &'static str,
    shortcuts: &[Shortcut],
    edit: Option<EditRole>,
) -> ActionSpec {
    ActionSpec {
        identifier,
        name,
        context: Context::GLOBAL,
        defaults: Defaults::everywhere(shortcuts),
        repeatable: false,
        edit,
    }
}

/// The registry of the bench on `platform`: the six editing actions, a disabled Save and a toggle
/// without a shortcut (*Show grid*, unchecked), with the made-up English texts.
fn registry(platform: Platform) -> (Registry, Ids) {
    let mut registry = Registry::new(platform);
    let mut add = |spec: ActionSpec| registry.register(spec).unwrap();
    let ids = Ids {
        undo: add(spec(
            "edit.undo",
            "Undo",
            &[ch(PRIMARY, 'z')],
            Some(EditRole::Undo),
        )),
        redo: add(spec(
            "edit.redo",
            "Redo",
            &[ch(PRIMARY_SHIFT, 'z')],
            Some(EditRole::Redo),
        )),
        cut: add(spec(
            "edit.cut",
            "Cut",
            &[ch(PRIMARY, 'x')],
            Some(EditRole::Cut),
        )),
        copy: add(spec(
            "edit.copy",
            "Copy",
            &[ch(PRIMARY, 'c')],
            Some(EditRole::Copy),
        )),
        paste: add(spec(
            "edit.paste",
            "Paste",
            &[ch(PRIMARY, 'v')],
            Some(EditRole::Paste),
        )),
        select_all: add(spec(
            "edit.select-all",
            "Select all",
            &[ch(PRIMARY, 'a')],
            Some(EditRole::SelectAll),
        )),
        save: add(spec("file.save", "Save", &[ch(PRIMARY, 's')], None)),
        grid: add(spec("view.grid", "Show grid", &[], None)),
    };
    registry.set_state(ids.save, ActionState::DISABLED).unwrap();
    registry
        .set_state(
            ids.grid,
            ActionState {
                enabled: true,
                checked: Some(false),
            },
        )
        .unwrap();
    // The specs' "names" are already the English texts: the catalogue is the identity.
    registry.set_texts(english_keys(), str::to_owned);
    (registry, ids)
}

/// The field's context menu: the editing items in their groups, and a submenu *More* holding the
/// disabled Save, the toggle, and a submenu *Deeper* with Copy.
fn context_menu(ids: Ids) -> MenuModel {
    let deeper = MenuModel::new().action(ids.copy);
    let more =
        MenuModel::new()
            .action(ids.save)
            .action(ids.grid)
            .submenu("Deeper", Some('d'), deeper);
    MenuModel::new()
        .action(ids.undo)
        .action(ids.redo)
        .separator()
        .action(ids.cut)
        .action(ids.copy)
        .action(ids.paste)
        .separator()
        .action(ids.select_all)
        .separator()
        .submenu("More", Some('m'), more)
}

/// The places of the context menu's entries (see [`context_menu`]).
mod entry {
    /// Undo.
    pub(super) const UNDO: usize = 0;
    /// Cut.
    pub(super) const CUT: usize = 3;
    /// Copy.
    pub(super) const COPY: usize = 4;
    /// Paste.
    pub(super) const PASTE: usize = 5;
    /// Select all.
    pub(super) const SELECT_ALL: usize = 7;
    /// More (a submenu).
    pub(super) const MORE: usize = 9;
}

/// The bench: an interface with a text field holding "hello world" and its context menu.
struct Bench {
    ui: Ui<Msg>,
    list: DisplayList,
    field: WidgetId,
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
        let (registry, ids) = registry(platform);
        let mut ui = Ui::new(
            Flex::column(Space::M, Space::M),
            &theme,
            Faces::bundled().unwrap(),
        );
        ui.set_actions(registry, Msg::Action);
        let root = ui.root();
        let field = ui
            .add(
                root,
                TextField::new("hello world", |text| Msg::Committed(text.to_owned()))
                    .convention(EditConvention::Linux)
                    .with_context_menu(context_menu(ids)),
                LayoutProps::default().width(Length::Fixed(300.0)),
            )
            .unwrap();
        let mut list = DisplayList::new();
        ui.layout(Rect::new(0.0, 0.0, 800.0, 600.0), &mut list);
        Self {
            ui,
            list,
            field,
            ids,
        }
    }

    /// A point inside the field, `x` logical pixels from its left edge.
    fn in_field(&self, x: f32) -> Point {
        let rect = self.ui.rect(self.field).unwrap();
        Point::new(rect.x0 + x, f32::midpoint(rect.y0, rect.y1))
    }

    /// Moves the pointer to `at`.
    fn move_to(&mut self, at: Point) {
        self.ui
            .input(&InputEvent::PointerMoved { position: at }, &mut self.list);
    }

    /// Presses or releases `button` at `at` (the pointer moves there first).
    fn button(&mut self, button: PointerButton, state: ButtonState, at: Point) {
        self.move_to(at);
        self.ui.input(
            &InputEvent::PointerButton {
                button,
                state,
                position: at,
                clicks: 1,
            },
            &mut self.list,
        );
    }

    /// A whole click of `button` at `at`.
    fn click(&mut self, button: PointerButton, at: Point) {
        self.button(button, ButtonState::Pressed, at);
        self.button(button, ButtonState::Released, at);
    }

    /// Presses and releases the named key `key` with `modifiers`.
    fn key(&mut self, key: NamedKey, modifiers: Modifiers) {
        for state in [ButtonState::Pressed, ButtonState::Released] {
            self.ui.input(
                &InputEvent::Key {
                    key: Key::Named(key),
                    physical: PhysicalKey::Other,
                    state,
                    repeat: false,
                    modifiers,
                },
                &mut self.list,
            );
        }
    }

    /// The open menus.
    fn menus(&self) -> Vec<WidgetId> {
        self.ui.menus().collect()
    }

    /// The entries of the open menu number `menu` (0 the first).
    fn entries(&self, menu: usize) -> Vec<WidgetId> {
        self.ui.menu_items(self.menus()[menu]).collect()
    }

    /// The middle of entry `entry` of the open menu `menu`.
    fn entry_point(&self, menu: usize, entry: usize) -> Point {
        let rect = self.ui.rect(self.entries(menu)[entry]).unwrap();
        Point::new(rect.x0 + 20.0, f32::midpoint(rect.y0, rect.y1))
    }

    /// The place of the highlighted entry in the topmost menu.
    fn highlighted(&self) -> Option<usize> {
        let menu = self.menus().len().checked_sub(1)?;
        let item = self.ui.highlighted_menu_item()?;
        self.entries(menu).iter().position(|&id| id == item)
    }

    /// The field's text.
    fn text(&self) -> String {
        self.ui
            .widget::<TextField<Msg>>(self.field)
            .unwrap()
            .text()
            .to_owned()
    }

    /// The field's selection, in bytes.
    fn selection(&self) -> std::ops::Range<usize> {
        self.ui
            .widget::<TextField<Msg>>(self.field)
            .unwrap()
            .selection()
    }

    /// The text on the interface's clipboard.
    fn clipboard(&mut self) -> Option<String> {
        self.ui.clipboard().read_text().unwrap()
    }

    /// The messages since the last call.
    fn messages(&mut self) -> Vec<Msg> {
        self.ui.take_messages().collect()
    }

    /// Advances the interface's time by `millis` from `start`.
    fn advance(&mut self, start: std::time::Instant, millis: u64) {
        self.ui
            .advance(start + Duration::from_millis(millis), &mut self.list);
    }
}

#[test]
fn shortcuts_are_written_in_each_platforms_style() {
    let write = |shortcut: Shortcut, platform: Platform, names: &KeyNames| {
        let mut out = String::new();
        shortcut.write_label(platform, names, &mut out);
        out
    };
    let english = english_keys();
    let german = german_keys();
    let save_as = ch(PRIMARY_SHIFT, 's');
    // Linux and Windows: the names joined by "+", the letter in upper case; translated.
    assert_eq!(write(save_as, Platform::Linux, &english), "Ctrl+Shift+S");
    assert_eq!(
        write(save_as, Platform::Windows, &german),
        "Strg+Umschalt+S"
    );
    // macOS: Apple's symbols in Apple's order, no separator.
    assert_eq!(
        write(save_as, Platform::MacOs, &german),
        "\u{21E7}\u{2318}S"
    );
    // *Secondary*: the logo key, named per platform; ⌃ on macOS, before ⌘.
    let secondary = Modifiers {
        secondary: true,
        ..PRIMARY
    };
    assert_eq!(
        write(ch(secondary, 'f'), Platform::Linux, &english),
        "Ctrl+Super+F"
    );
    assert_eq!(
        write(ch(secondary, 'f'), Platform::Windows, &english),
        "Ctrl+Windows+F"
    );
    assert_eq!(
        write(ch(secondary, 'f'), Platform::MacOs, &english),
        "\u{2303}\u{2318}F"
    );
    // A character that is not a letter stays as it is; Shift is never part of it (AM5).
    assert_eq!(
        write(ch(PRIMARY, '?'), Platform::MacOs, &english),
        "\u{2318}?"
    );
    assert_eq!(write(ch(PRIMARY, ','), Platform::Linux, &english), "Ctrl+,");
    // Named keys: the catalogue's names, Apple's symbols where Apple has one.
    let named = |modifiers: Modifiers, key: NamedKey| {
        Shortcut::new(modifiers, ShortcutKey::Named(key)).unwrap()
    };
    assert_eq!(
        write(
            named(NONE, NamedKey::Function(11)),
            Platform::Linux,
            &english
        ),
        "F11"
    );
    assert_eq!(
        write(named(PRIMARY, NamedKey::Delete), Platform::Linux, &german),
        "Strg+Entf"
    );
    assert_eq!(
        write(named(PRIMARY, NamedKey::Delete), Platform::MacOs, &german),
        "\u{2318}\u{2326}"
    );
    assert_eq!(
        write(named(PRIMARY, NamedKey::Space), Platform::MacOs, &german),
        "\u{2318}Leertaste"
    );
    // Digits: the digit row as the digit, the keypad by the catalogue.
    let digit = Shortcut::new(PRIMARY, ShortcutKey::Digit(5)).unwrap();
    let numpad = Shortcut::new(PRIMARY, ShortcutKey::Numpad(5)).unwrap();
    assert_eq!(write(digit, Platform::Linux, &english), "Ctrl+5");
    assert_eq!(write(numpad, Platform::Linux, &english), "Ctrl+Num 5");
    // Every modifier at once, in the fixed order of choice S1.
    let all = Modifiers {
        primary: true,
        secondary: true,
        shift: true,
        alt: true,
    };
    assert_eq!(
        write(ch(all, 'k'), Platform::Linux, &english),
        "Ctrl+Alt+Shift+Super+K"
    );
    assert_eq!(
        write(ch(all, 'k'), Platform::MacOs, &english),
        "\u{2303}\u{2325}\u{21E7}\u{2318}K"
    );
    // No names given: the key's name is empty, nothing panics.
    assert_eq!(KeyNames::default().get(KeyName::Ctrl), "");
    assert_eq!(english.get(KeyName::Function(25)), "");
    assert_eq!(english.get(KeyName::Function(24)), "F24");
}

#[test]
fn the_registry_keeps_texts_and_marks_what_changed() {
    let (mut registry, ids) = registry(Platform::Linux);
    assert_eq!(registry.name_text(ids.copy), "Copy");
    assert_eq!(registry.shortcut_text(ids.copy), "Ctrl+C");
    assert_eq!(registry.shortcut_text(ids.redo), "Ctrl+Shift+Z");
    assert_eq!(registry.shortcut_text(ids.grid), "", "no shortcut, no text");
    let drained = |registry: &mut Registry| {
        let mut marked = Vec::new();
        registry.drain_changed(|id| marked.push(id));
        marked
    };
    // The texts were just given: every action is marked, once.
    assert_eq!(drained(&mut registry).len(), 8);
    assert_eq!(drained(&mut registry), vec![]);
    // The same state again marks nothing; another state marks the action.
    registry.set_state(ids.save, ActionState::DISABLED).unwrap();
    assert_eq!(drained(&mut registry), vec![]);
    registry.set_state(ids.save, ActionState::ENABLED).unwrap();
    assert_eq!(drained(&mut registry), vec![ids.save]);
    // The user's shortcut: a new text, marked.
    registry
        .set_shortcuts(ids.copy, &[ch(PRIMARY_SHIFT, 'c')])
        .unwrap();
    assert_eq!(registry.shortcut_text(ids.copy), "Ctrl+Shift+C");
    assert_eq!(drained(&mut registry), vec![ids.copy]);
    // Back to the defaults: every action's text written again.
    registry.restore_all();
    assert_eq!(registry.shortcut_text(ids.copy), "Ctrl+C");
    assert_eq!(drained(&mut registry).len(), 8);
}

#[test]
fn a_secondary_press_opens_the_context_menu_on_linux_and_copies_the_selection() {
    let mut bench = Bench::new();
    // Focused by the keyboard: everything selected.
    bench
        .ui
        .set_focus(Some(bench.field), &mut bench.list)
        .unwrap();
    let _ = bench.messages();
    assert_eq!(bench.selection(), 0..11);
    // A secondary press inside the selection: the menu opens, the selection stays.
    let at = bench.in_field(20.0);
    bench.button(PointerButton::Right, ButtonState::Pressed, at);
    assert_eq!(bench.menus().len(), 1);
    assert_eq!(bench.selection(), 0..11);
    assert_eq!(bench.highlighted(), None, "opened by the pointer");
    // The release of the press that opened it chooses nothing, wherever it lands.
    bench.button(PointerButton::Right, ButtonState::Released, at);
    assert_eq!(bench.menus().len(), 1);
    // The pointer over Copy highlights it; a click chooses it.
    let copy = bench.entry_point(0, entry::COPY);
    bench.move_to(copy);
    assert_eq!(bench.highlighted(), Some(entry::COPY));
    bench.click(PointerButton::Left, copy);
    assert_eq!(bench.menus().len(), 0, "choosing closes the menu");
    assert_eq!(bench.clipboard().as_deref(), Some("hello world"));
    // The field kept the focus; the action reached the field, not the application.
    assert_eq!(bench.ui.focused(), Some(bench.field));
    assert_eq!(bench.messages(), vec![]);
}

#[test]
fn a_secondary_press_outside_the_selection_moves_the_caret() {
    let mut bench = Bench::new();
    // Unfocused: the press gives the focus and puts the caret where it lands, not "select all".
    let at = bench.in_field(2.0);
    bench.button(PointerButton::Right, ButtonState::Pressed, at);
    assert_eq!(bench.ui.focused(), Some(bench.field));
    assert!(bench.selection().is_empty());
    assert_eq!(bench.menus().len(), 1);
}

#[test]
fn press_drag_release_chooses_an_item() {
    let mut bench = Bench::new();
    bench
        .ui
        .set_focus(Some(bench.field), &mut bench.list)
        .unwrap();
    let at = bench.in_field(20.0);
    bench.button(PointerButton::Right, ButtonState::Pressed, at);
    // Still pressed, the pointer goes to Cut, and the release there chooses it.
    let cut = bench.entry_point(0, entry::CUT);
    bench.move_to(cut);
    bench.button(PointerButton::Right, ButtonState::Released, cut);
    assert_eq!(bench.menus().len(), 0);
    assert_eq!(bench.text(), "");
    assert_eq!(bench.clipboard().as_deref(), Some("hello world"));
}

#[test]
fn windows_opens_context_menus_on_the_release() {
    let mut bench = Bench::on(Platform::Windows);
    let at = bench.in_field(20.0);
    bench.button(PointerButton::Right, ButtonState::Pressed, at);
    assert_eq!(bench.menus().len(), 0, "nothing on the press on Windows");
    bench.button(PointerButton::Right, ButtonState::Released, at);
    assert_eq!(bench.menus().len(), 1);
}

#[test]
fn disabled_items_are_shown_and_never_chosen() {
    let mut bench = Bench::new();
    let at = bench.in_field(20.0);
    bench.click(PointerButton::Right, at);
    // Undo: the field's local history is empty, so it is disabled (AM13).
    let undo = bench.entry_point(0, entry::UNDO);
    bench.move_to(undo);
    assert_eq!(
        bench.highlighted(),
        None,
        "a disabled item is not highlighted"
    );
    bench.click(PointerButton::Left, undo);
    assert_eq!(bench.menus().len(), 1, "still open: nothing was chosen");
    assert_eq!(bench.messages(), vec![]);
}

#[test]
fn the_keyboard_walks_the_menu_and_its_submenus() {
    let mut bench = Bench::new();
    bench
        .ui
        .set_focus(Some(bench.field), &mut bench.list)
        .unwrap();
    // The Menu key: the menu opens at the caret with its first enabled item highlighted (Undo
    // and Redo are disabled: the field's history is empty).
    bench.key(NamedKey::ContextMenu, NONE);
    assert_eq!(bench.menus().len(), 1);
    assert_eq!(bench.highlighted(), Some(entry::CUT));
    assert!(
        bench.ui.text_input().is_none(),
        "the input method is off while a menu is open"
    );
    // Down skips the separators; it wraps from the last item to the first enabled one.
    bench.key(NamedKey::ArrowDown, NONE);
    assert_eq!(bench.highlighted(), Some(entry::COPY));
    bench.key(NamedKey::ArrowDown, NONE);
    assert_eq!(bench.highlighted(), Some(entry::PASTE));
    bench.key(NamedKey::ArrowDown, NONE);
    assert_eq!(bench.highlighted(), Some(entry::SELECT_ALL));
    bench.key(NamedKey::ArrowDown, NONE);
    assert_eq!(bench.highlighted(), Some(entry::MORE));
    bench.key(NamedKey::ArrowDown, NONE);
    assert_eq!(bench.highlighted(), Some(entry::CUT));
    bench.key(NamedKey::ArrowUp, NONE);
    assert_eq!(bench.highlighted(), Some(entry::MORE));
    bench.key(NamedKey::Home, NONE);
    assert_eq!(bench.highlighted(), Some(entry::CUT));
    bench.key(NamedKey::End, NONE);
    assert_eq!(bench.highlighted(), Some(entry::MORE));
    // Right opens the submenu with its first enabled item (Save is disabled: the toggle).
    bench.key(NamedKey::ArrowRight, NONE);
    assert_eq!(bench.menus().len(), 2);
    assert_eq!(bench.highlighted(), Some(1));
    // Tab moves nothing anywhere; the focus stays in the field.
    bench.key(NamedKey::Tab, NONE);
    assert_eq!(bench.menus().len(), 2);
    assert_eq!(bench.ui.focused(), Some(bench.field));
    // Left closes the submenu, back to More in the first menu.
    bench.key(NamedKey::ArrowLeft, NONE);
    assert_eq!(bench.menus().len(), 1);
    assert_eq!(bench.highlighted(), Some(entry::MORE));
    // Left in the first menu does nothing (the menu bar's, F2.11d2b).
    bench.key(NamedKey::ArrowLeft, NONE);
    assert_eq!(bench.menus().len(), 1);
    // Escape in a submenu closes only it; in the first menu, the menu.
    bench.key(NamedKey::Enter, NONE);
    assert_eq!(bench.menus().len(), 2);
    bench.key(NamedKey::Escape, NONE);
    assert_eq!(bench.menus().len(), 1);
    bench.key(NamedKey::Escape, NONE);
    assert_eq!(bench.menus().len(), 0);
    assert_eq!(bench.ui.focused(), Some(bench.field));
    assert!(bench.ui.text_input().is_some(), "the input method is back");
    // The field still holds its text: Escape reached the menu, not the field.
    assert_eq!(bench.text(), "hello world");
    assert_eq!(bench.messages(), vec![]);
}

#[test]
fn enter_runs_the_highlighted_item_on_the_focused_field() {
    let mut bench = Bench::new();
    bench
        .ui
        .set_focus(Some(bench.field), &mut bench.list)
        .unwrap();
    // Shift+F10 opens the menu as the Menu key does; Cut is highlighted; Enter cuts.
    bench.key(NamedKey::Function(10), SHIFT);
    assert_eq!(bench.highlighted(), Some(entry::CUT));
    bench.key(NamedKey::Enter, NONE);
    assert_eq!(bench.menus().len(), 0);
    assert_eq!(bench.text(), "");
    assert_eq!(bench.clipboard().as_deref(), Some("hello world"));
    // Undo is now enabled (the field's local history), and runs from the menu too.
    bench.key(NamedKey::ContextMenu, NONE);
    assert_eq!(bench.highlighted(), Some(entry::UNDO));
    bench.key(NamedKey::Enter, NONE);
    assert_eq!(bench.text(), "hello world");
    assert_eq!(bench.messages(), vec![]);
}

#[test]
fn f10_alone_opens_no_context_menu() {
    let mut bench = Bench::new();
    bench
        .ui
        .set_focus(Some(bench.field), &mut bench.list)
        .unwrap();
    bench.key(NamedKey::Function(10), NONE);
    assert_eq!(
        bench.menus().len(),
        0,
        "F10 alone is the menu bar's (F2.11d2b)"
    );
    bench.key(NamedKey::Function(10), PRIMARY);
    assert_eq!(bench.menus().len(), 0);
}

#[test]
fn a_submenu_opens_after_the_pointer_rests_and_closes_the_same_way() {
    let mut bench = Bench::new();
    let start = bench.ui.now();
    bench.click(PointerButton::Right, bench.in_field(20.0));
    let more = bench.entry_point(0, entry::MORE);
    bench.move_to(more);
    assert_eq!(bench.highlighted(), Some(entry::MORE));
    assert!(bench.ui.next_wakeup().is_some(), "the window wakes for it");
    bench.advance(start, 249);
    assert_eq!(bench.menus().len(), 1);
    bench.advance(start, 250);
    assert_eq!(bench.menus().len(), 2, "open after 250 ms");
    assert_eq!(
        bench.highlighted(),
        None,
        "opened by the pointer: nothing highlighted"
    );
    // Into the submenu: its item is highlighted, More stays highlighted below it.
    let grid = bench.entry_point(1, 1);
    bench.move_to(grid);
    assert_eq!(bench.highlighted(), Some(1));
    // Back to another item of the first menu: the submenu closes after the same delay.
    let select_all = bench.entry_point(0, entry::SELECT_ALL);
    bench.move_to(select_all);
    bench.advance(start, 400);
    assert_eq!(bench.menus().len(), 2);
    bench.advance(start, 600);
    assert_eq!(bench.menus().len(), 1);
    assert_eq!(bench.highlighted(), Some(entry::SELECT_ALL));
    // A press on More opens its submenu at once.
    bench.button(PointerButton::Left, ButtonState::Pressed, more);
    assert_eq!(bench.menus().len(), 2);
    bench.button(PointerButton::Left, ButtonState::Released, more);
    assert_eq!(
        bench.menus().len(),
        2,
        "releasing on a submenu's item chooses nothing"
    );
    // The toggle runs as the application's action, and every menu closes.
    let grid = bench.entry_point(1, 1);
    bench.click(PointerButton::Left, grid);
    assert_eq!(bench.menus().len(), 0);
    assert_eq!(bench.messages(), vec![Msg::Action(bench.ids.grid)]);
}

#[test]
fn a_diagonal_move_across_a_neighbour_keeps_the_submenu() {
    let mut bench = Bench::new();
    let start = bench.ui.now();
    bench.click(PointerButton::Right, bench.in_field(20.0));
    let more = bench.entry_point(0, entry::MORE);
    bench.move_to(more);
    bench.advance(start, 250);
    assert_eq!(bench.menus().len(), 2);
    // Across Select all on the way, then into the submenu before the delay ends.
    bench.move_to(bench.entry_point(0, entry::SELECT_ALL));
    bench.advance(start, 300);
    bench.move_to(bench.entry_point(1, 1));
    bench.advance(start, 1000);
    assert_eq!(bench.menus().len(), 2, "the submenu stays");
    // The first menu points at More again, under its open submenu.
    bench.key(NamedKey::ArrowLeft, NONE);
    assert_eq!(bench.highlighted(), Some(entry::MORE));
}

#[test]
fn a_state_changed_while_the_menu_is_open_reaches_its_item() {
    let mut bench = Bench::new();
    bench.click(PointerButton::Right, bench.in_field(20.0));
    bench.click(PointerButton::Left, bench.entry_point(0, entry::MORE));
    let save = bench.entries(1)[0];
    let disabled = |bench: &mut Bench| {
        let update = bench.ui.take_access_update();
        update
            .tree_update()
            .nodes
            .iter()
            .find(|(id, _)| *id == NodeId(save.get()))
            .map(|(_, node)| node.is_disabled())
    };
    bench.ui.reset_access();
    assert_eq!(disabled(&mut bench), Some(true));
    // The application enables Save while the menu is open: the item follows at the next frame.
    bench
        .ui
        .actions_mut()
        .unwrap()
        .set_state(bench.ids.save, ActionState::ENABLED)
        .unwrap();
    bench.ui.frame(&mut bench.list);
    assert_eq!(disabled(&mut bench), Some(false));
    // And it can be chosen now.
    bench.click(PointerButton::Left, bench.entry_point(1, 0));
    assert_eq!(bench.messages(), vec![Msg::Action(bench.ids.save)]);
}

#[test]
fn menus_describe_themselves_to_assistive_technologies() {
    let mut bench = Bench::new();
    bench
        .ui
        .set_focus(Some(bench.field), &mut bench.list)
        .unwrap();
    let _ = bench.ui.take_access_update();
    // The field offers its context menu.
    bench.ui.reset_access();
    let update = bench.ui.take_access_update();
    let field = update
        .tree_update()
        .nodes
        .iter()
        .find(|(id, _)| *id == NodeId(bench.field.get()))
        .map(|(_, node)| node.clone())
        .unwrap();
    assert!(field.supports_action(Action::ShowContextMenu));
    // An assistive technology asks for it: the menu opens with its first item highlighted.
    bench
        .ui
        .access_action(
            AccessAction {
                target: bench.field,
                kind: AccessActionKind::ShowContextMenu,
            },
            &mut bench.list,
        )
        .unwrap();
    assert_eq!(bench.menus().len(), 1);
    let menu = bench.menus()[0];
    let entries = bench.entries(0);
    let update = bench.ui.take_access_update();
    let tree = update.tree_update();
    let node = |id: WidgetId| {
        tree.nodes
            .iter()
            .find(|(node, _)| *node == NodeId(id.get()))
            .map(|(_, node)| node.clone())
    };
    // The window lists the menu after the root; the focus is the highlighted item.
    let window = tree
        .nodes
        .iter()
        .find(|(id, _)| *id == NodeId(0))
        .map(|(_, node)| node.clone())
        .unwrap();
    assert_eq!(
        window.children(),
        &[NodeId(bench.ui.root().get()), NodeId(menu.get())]
    );
    assert_eq!(tree.focus, NodeId(entries[entry::CUT].get()));
    assert_eq!(node(menu).unwrap().role(), Role::Menu);
    let copy = node(entries[entry::COPY]).unwrap();
    assert_eq!(copy.role(), Role::MenuItem);
    assert_eq!(copy.label(), Some("Copy"));
    assert_eq!(copy.keyboard_shortcut(), Some("Ctrl+C"));
    // Seven items without the three separators: Copy is the fourth.
    assert_eq!(copy.position_in_set(), Some(4));
    assert_eq!(copy.size_of_set(), Some(7));
    assert!(
        node(entries[2]).unwrap().is_hidden(),
        "a separator is hidden"
    );
    let more = node(entries[entry::MORE]).unwrap();
    assert!(more.has_popup().is_some());
    assert_eq!(more.is_expanded(), Some(false));
    assert!(node(entries[entry::UNDO]).unwrap().is_disabled());
    // Down moves the focus the assistive technology sees.
    bench.key(NamedKey::ArrowDown, NONE);
    let update = bench.ui.take_access_update();
    assert_eq!(
        update.tree_update().focus,
        NodeId(entries[entry::COPY].get())
    );
    // A click from the assistive technology chooses the item.
    bench
        .ui
        .access_action(
            AccessAction {
                target: entries[entry::COPY],
                kind: AccessActionKind::Click,
            },
            &mut bench.list,
        )
        .unwrap();
    assert_eq!(bench.menus().len(), 0);
    assert_eq!(bench.clipboard().as_deref(), Some("hello world"));
    // The window node no longer lists the menu, and the focus is the field's again.
    let update = bench.ui.take_access_update();
    let tree = update.tree_update();
    let window = tree
        .nodes
        .iter()
        .find(|(id, _)| *id == NodeId(0))
        .map(|(_, node)| node.clone())
        .unwrap();
    assert_eq!(window.children(), &[NodeId(bench.ui.root().get())]);
    assert_eq!(tree.focus, NodeId(bench.field.get()));
}

#[test]
fn a_widget_without_a_context_menu_opens_none() {
    let mut bench = Bench::new();
    // The root has no context menu: a secondary press on it opens nothing.
    bench.click(PointerButton::Right, Point::new(700.0, 500.0));
    assert_eq!(bench.menus().len(), 0);
}

#[test]
fn a_release_under_a_menu_that_just_opened_chooses_nothing_until_the_pointer_moves() {
    use orchestra_ui::{Popup, PopupSide};
    let mut bench = Bench::new();
    bench
        .ui
        .set_focus(Some(bench.field), &mut bench.list)
        .unwrap();
    let model = context_menu(bench.ids);
    let popup = |at: Point| Popup {
        anchor: Rect::new(at.x, at.y, at.x, at.y),
        side: PopupSide::Below,
        owner: None,
    };
    // Where Copy lies inside a menu opened at (100, 100): measured on a first opening.
    let origin = Point::new(100.0, 100.0);
    bench
        .ui
        .open_menu_model(&model, popup(origin), false, &mut bench.list)
        .unwrap();
    let copy = bench.entry_point(0, entry::COPY);
    bench.key(NamedKey::Escape, NONE);
    // The pointer rests at P, and a menu opens so that Copy is right under it (a menu pushed
    // inside a small window can open over the pointer, as this one does on purpose).
    let pointer = Point::new(300.0, 300.0);
    bench.move_to(pointer);
    let at = Point::new(
        pointer.x - (copy.x - origin.x),
        pointer.y - (copy.y - origin.y),
    );
    bench
        .ui
        .open_menu_model(&model, popup(at), false, &mut bench.list)
        .unwrap();
    assert_eq!(bench.entry_point(0, entry::COPY), pointer);
    // The release of a press made before the menu opened: not armed, nothing chosen.
    bench.ui.input(
        &InputEvent::PointerButton {
            button: PointerButton::Right,
            state: ButtonState::Released,
            position: pointer,
            clicks: 1,
        },
        &mut bench.list,
    );
    assert_eq!(bench.menus().len(), 1);
    assert_eq!(bench.clipboard(), None);
    // A move arms the menu: the next release over Copy chooses it.
    let moved = Point::new(pointer.x + 1.0, pointer.y);
    bench.button(PointerButton::Right, ButtonState::Released, moved);
    assert_eq!(bench.menus().len(), 0);
    assert_eq!(bench.clipboard().as_deref(), Some("hello world"));
}
