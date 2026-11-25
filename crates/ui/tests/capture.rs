// SPDX-License-Identifier: GPL-3.0-or-later

//! The shortcut editor's widgets (ADR-0048 AM7, SCALETTA F2.12c1), headless: the capture box
//! records a combination with its modifiers, waits past a modifier alone, refuses the fixed keys
//! and keeps waiting, cancels with Escape and when the focus leaves with Tab, swallows the key
//! that would run an action (even the shortcut that closes a large dialog), and sends one message
//! at most; the list shows a detail column, is named "item, detail" for assistive technologies,
//! takes new items, and may be empty.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test helpers: a failed unwrap or index fails the test"
)]

use orchestra_text::Faces;
use orchestra_ui::{
    ActionId, ActionSpec, Appearance, BuiltInTheme, ButtonState, Context, Defaults, DisplayList,
    Flex, InputEvent, Key, LargeDialog, LayoutProps, ListBox, Modifiers, NamedKey, PhysicalKey,
    Platform, Rect, Registry, Shortcut, ShortcutCapture, ShortcutKey, Space, Ui, WidgetId,
};

/// The messages of these tests.
#[derive(Debug, Clone, PartialEq)]
enum Msg {
    /// A shortcut was captured.
    Captured(Shortcut),
    /// A fixed key was pressed.
    Refused(Shortcut),
    /// The capture was cancelled.
    Cancelled,
    /// An action ran.
    Action(ActionId),
    /// The large dialog closed.
    Closed,
    /// The list's new item.
    Item(usize),
}

/// The window of the tests, logical pixels.
const WINDOW: Rect = Rect::new(0.0, 0.0, 1000.0, 700.0);

/// *Primary* alone.
const PRIMARY: Modifiers = Modifiers {
    primary: true,
    secondary: false,
    shift: false,
    alt: false,
};

/// An interface with a global action *Preferences* on Ctrl+, that opened a large dialog holding
/// a capture box (focused) after a list.
struct Bench {
    ui: Ui<Msg>,
    list: DisplayList,
    capture: WidgetId,
    items: WidgetId,
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
                    PRIMARY,
                    ShortcutKey::Character(','),
                )
                .unwrap()]),
                repeatable: false,
                edit: None,
            })
            .unwrap();
        ui.set_actions(registry, Msg::Action);
        let mut list = DisplayList::new();
        ui.layout(WINDOW, &mut list);
        let (_, content) = ui
            .open_large_dialog(
                LargeDialog {
                    title: "Preferences".into(),
                    close_text: "Close".into(),
                    close_message: Msg::Closed,
                    closed_by: Some(preferences),
                },
                &mut list,
            )
            .unwrap();
        let column = ui
            .add(
                content,
                Flex::column(Space::M, Space::None),
                LayoutProps::default(),
            )
            .unwrap();
        let items = ui
            .add(
                column,
                ListBox::new(vec!["New".into(), "Open".into()], 0, Msg::Item)
                    .details(vec!["Ctrl+N".into()])
                    .label("Commands"),
                LayoutProps::default(),
            )
            .unwrap();
        let capture = ui
            .add(
                column,
                ShortcutCapture::new(
                    "Press the shortcut…",
                    Platform::Linux,
                    Msg::Captured,
                    Msg::Refused,
                    || Msg::Cancelled,
                )
                .labelled("New shortcut for Open"),
                LayoutProps::default(),
            )
            .unwrap();
        ui.set_focus(Some(capture), &mut list).unwrap();
        ui.frame(&mut list);
        let _ = ui.take_messages().count();
        Self {
            ui,
            list,
            capture,
            items,
        }
    }

    /// Presses and releases `key` with `modifiers`, then answers the messages.
    fn press(&mut self, key: &Key, physical: PhysicalKey, modifiers: Modifiers) -> Vec<Msg> {
        for state in [ButtonState::Pressed, ButtonState::Released] {
            self.ui.input(
                &InputEvent::Key {
                    key: key.clone(),
                    physical,
                    state,
                    repeat: false,
                    modifiers,
                },
                &mut self.list,
            );
        }
        self.ui.take_messages().collect()
    }
}

/// The shortcut made of `modifiers` and the character `c`.
fn ch(modifiers: Modifiers, c: char) -> Shortcut {
    Shortcut::new(modifiers, ShortcutKey::Character(c)).unwrap()
}

#[test]
fn a_combination_is_captured_once_with_its_modifiers() {
    let mut bench = Bench::new();
    // Ctrl alone (an unknown key to the interface) is waited past.
    assert_eq!(
        bench.press(&Key::Unknown, PhysicalKey::Other, PRIMARY),
        vec![]
    );
    assert_eq!(
        bench.press(
            &Key::Named(NamedKey::Alt),
            PhysicalKey::Other,
            Modifiers::default()
        ),
        vec![]
    );
    let shift_primary = Modifiers {
        shift: true,
        ..PRIMARY
    };
    assert_eq!(
        bench.press(
            &Key::Character("K".into()),
            PhysicalKey::Other,
            shift_primary
        ),
        vec![Msg::Captured(ch(shift_primary, 'k'))]
    );
    // Done: nothing more, not even a cancel when the focus leaves.
    assert_eq!(
        bench.press(&Key::Character("j".into()), PhysicalKey::Other, PRIMARY),
        vec![]
    );
    bench
        .ui
        .set_focus(Some(bench.items), &mut bench.list)
        .unwrap();
    bench.ui.frame(&mut bench.list);
    assert_eq!(bench.ui.take_messages().collect::<Vec<_>>(), vec![]);
}

#[test]
fn the_shortcut_that_closes_the_dialog_is_captured_instead() {
    let mut bench = Bench::new();
    assert_eq!(
        bench.press(&Key::Character(",".into()), PhysicalKey::Other, PRIMARY),
        vec![Msg::Captured(ch(PRIMARY, ','))]
    );
    assert_eq!(bench.ui.dialogs().count(), 1, "the dialog is still open");
}

#[test]
fn a_keypad_digit_is_captured_as_the_keypad() {
    let mut bench = Bench::new();
    assert_eq!(
        bench.press(&Key::Character("4".into()), PhysicalKey::Numpad(4), PRIMARY),
        vec![Msg::Captured(
            Shortcut::new(PRIMARY, ShortcutKey::Numpad(4)).unwrap()
        )]
    );
}

#[test]
fn fixed_keys_are_refused_and_the_capture_goes_on() {
    let mut bench = Bench::new();
    let alt = Modifiers {
        alt: true,
        ..Modifiers::default()
    };
    assert_eq!(
        bench.press(&Key::Character("f".into()), PhysicalKey::Other, alt),
        vec![Msg::Refused(ch(alt, 'f'))],
        "Alt+letter is a mnemonic on Linux"
    );
    let enter = Shortcut::new(Modifiers::default(), ShortcutKey::Named(NamedKey::Enter)).unwrap();
    assert_eq!(
        bench.press(
            &Key::Named(NamedKey::Enter),
            PhysicalKey::Other,
            Modifiers::default()
        ),
        vec![Msg::Refused(enter)]
    );
    assert_eq!(
        bench.ui.dialogs().count(),
        1,
        "Enter did not close the dialog"
    );
    assert_eq!(
        bench.press(&Key::Character("e".into()), PhysicalKey::Other, PRIMARY),
        vec![Msg::Captured(ch(PRIMARY, 'e'))]
    );
}

#[test]
fn escape_cancels_and_leaves_the_dialog_open() {
    let mut bench = Bench::new();
    assert_eq!(
        bench.press(
            &Key::Named(NamedKey::Escape),
            PhysicalKey::Other,
            Modifiers::default()
        ),
        vec![Msg::Cancelled]
    );
    assert_eq!(bench.ui.dialogs().count(), 1);
}

#[test]
fn tab_moves_on_and_cancels() {
    let mut bench = Bench::new();
    let messages = bench.press(
        &Key::Named(NamedKey::Tab),
        PhysicalKey::Other,
        Modifiers::default(),
    );
    assert_eq!(messages, vec![Msg::Cancelled]);
    assert_ne!(bench.ui.focused(), Some(bench.capture));
}

#[test]
fn the_list_has_a_detail_column_new_items_and_may_be_empty() {
    let mut bench = Bench::new();
    let with_details = bench.ui.rect(bench.items).unwrap().width();
    let list = bench.ui.widget_mut::<ListBox<Msg>>(bench.items).unwrap();
    assert_eq!(list.len(), 2);
    list.set_items(vec!["Save".into()], vec![], 4);
    assert_eq!(list.selected(), 0, "clamped");
    bench.ui.request_layout(bench.items);
    bench.ui.frame(&mut bench.list);
    assert!(
        bench.ui.rect(bench.items).unwrap().width() < with_details,
        "no detail column, a shorter item"
    );
    let list = bench.ui.widget_mut::<ListBox<Msg>>(bench.items).unwrap();
    list.set_items(Vec::new(), Vec::new(), 0);
    assert!(list.is_empty());
    bench.ui.request_layout(bench.items);
    bench.ui.frame(&mut bench.list);
    assert_eq!(bench.ui.rect(bench.items).unwrap().height(), 0.0);
    // The arrows do nothing in an empty list.
    bench
        .ui
        .set_focus(Some(bench.items), &mut bench.list)
        .unwrap();
    let _ = bench.ui.take_messages().count();
    assert_eq!(
        bench.press(
            &Key::Named(NamedKey::ArrowDown),
            PhysicalKey::Other,
            Modifiers::default()
        ),
        vec![]
    );
}
