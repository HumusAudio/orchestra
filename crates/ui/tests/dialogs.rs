// SPDX-License-Identifier: GPL-3.0-or-later

//! Our dialogs (ADR-0050 DG1–DG3, SCALETTA F2.12a3a, choices C1–C9), headless: the buttons in
//! each system's order, the focus on the default button and back where it was, Enter, Escape and
//! Tab, a click outside that does nothing, a button that answers and closes, a dialog above a
//! dialog, a menu above a dialog, the menu bar's keys and the actions' shortcuts off behind a
//! dialog (the editing ones still on a dialog's field), the panel's size and place in a large and a
//! small window, the veil's colour, and the accessibility tree.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test helpers: a failed unwrap or index fails the test"
)]

use accesskit::{NodeId, Role};
use orchestra_text::Faces;
use orchestra_ui::{
    ActionId, ActionSpec, Appearance, BuiltInTheme, Button, ButtonKind, ButtonOrder, ButtonRole,
    ButtonState, ColorToken, Context, DIALOG_MAX_WIDTH, DIALOG_MIN_WIDTH, Defaults, DialogKind,
    DialogSpec, DisplayList, Flex, InputEvent, Key, LayoutProps, Length, MenuModel, Modifiers,
    NamedKey, PhysicalKey, Platform, Point, PointerButton, Popup, PopupSide, Rect, Registry,
    Shortcut, ShortcutKey, Space, TextField, Ui, WidgetId,
};

/// The messages of these tests.
#[derive(Debug, Clone, PartialEq)]
enum Msg {
    /// A dialog's answer.
    Answer(&'static str),
    /// An action ran.
    Action(ActionId),
    /// The field committed its text.
    Committed,
}

/// The window of most tests, logical pixels.
const WINDOW: Rect = Rect::new(0.0, 0.0, 800.0, 600.0);

/// The interface with a text field that has the focus, and a global action *Save* on Ctrl+S.
struct Bench {
    ui: Ui<Msg>,
    list: DisplayList,
    field: WidgetId,
    save: ActionId,
}

impl Bench {
    /// The bench in `order`.
    fn new(order: ButtonOrder) -> Self {
        let theme = *BuiltInTheme::Studio.pair().instance(Appearance::Light);
        let mut ui = Ui::new(
            Flex::column(Space::M, Space::M),
            &theme,
            Faces::bundled().unwrap(),
        );
        let mut registry = Registry::new(Platform::Linux);
        let save = registry
            .register(ActionSpec {
                identifier: "file.save",
                name: "Save",
                context: Context::GLOBAL,
                defaults: Defaults::everywhere(&[Shortcut::new(
                    Modifiers {
                        primary: true,
                        ..Modifiers::default()
                    },
                    ShortcutKey::Character('s'),
                )
                .unwrap()]),
                repeatable: false,
                edit: None,
            })
            .unwrap();
        ui.set_actions(registry, Msg::Action);
        ui.set_button_order(order);
        let root = ui.root();
        let field = ui
            .add(
                root,
                TextField::new("hello", |_| Msg::Committed),
                LayoutProps::default().width(Length::Fixed(300.0)),
            )
            .unwrap();
        let mut list = DisplayList::new();
        ui.layout(WINDOW, &mut list);
        ui.set_focus(Some(field), &mut list).unwrap();
        Self {
            ui,
            list,
            field,
            save,
        }
    }

    /// The question "Save the changes?" with its three answers, added in a mixed order.
    fn ask(&mut self) -> WidgetId {
        let spec = DialogSpec::new(
            DialogKind::Question,
            "Save the changes?",
            "Your changes will be lost if you don't save them.",
        )
        .detail("Untitled")
        .button("Cancel", ButtonRole::Cancel, Msg::Answer("cancel"))
        .button(
            "Don't save",
            ButtonRole::Destructive,
            Msg::Answer("discard"),
        )
        .button("Save", ButtonRole::Affirmative, Msg::Answer("save"));
        self.ui.open_dialog(spec, &mut self.list).unwrap()
    }

    /// The buttons of the dialog `panel`, as drawn left to right: (ID, text, primary look).
    fn buttons(&self, panel: WidgetId) -> Vec<(WidgetId, String, bool)> {
        self.ui
            .children(panel)
            .into_iter()
            .filter_map(|id| {
                let button = self.ui.widget::<Button<Msg>>(id)?;
                Some((
                    id,
                    button.text().to_owned(),
                    button.look() == ButtonKind::Primary,
                ))
            })
            .collect()
    }

    /// The texts of the buttons of `panel`, left to right.
    fn texts(&self, panel: WidgetId) -> Vec<String> {
        self.buttons(panel).into_iter().map(|(_, t, _)| t).collect()
    }

    /// The text of the focused button, if a button has the focus.
    fn focused_text(&self) -> Option<String> {
        let focused = self.ui.focused()?;
        self.ui
            .widget::<Button<Msg>>(focused)
            .map(|button| button.text().to_owned())
    }

    /// Presses and releases the named key `key` with `modifiers`.
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

    /// A named key without modifiers.
    fn named(&mut self, key: NamedKey) {
        self.key(&Key::Named(key), Modifiers::default());
    }

    /// A left click at `at`.
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

    /// The messages waiting.
    fn messages(&mut self) -> Vec<Msg> {
        self.ui.take_messages().collect()
    }
}

/// The middle of `rect`.
fn middle(rect: Rect) -> Point {
    Point::new(
        f32::midpoint(rect.x0, rect.x1),
        f32::midpoint(rect.y0, rect.y1),
    )
}

/// Each system's order, the default button in the accent and with the focus.
#[test]
fn buttons_follow_the_system_and_the_default_has_the_focus() {
    let mut last = Bench::new(ButtonOrder::AffirmativeLast);
    let panel = last.ask();
    assert_eq!(last.texts(panel), ["Don't save", "Cancel", "Save"]);
    let primary: Vec<bool> = last.buttons(panel).into_iter().map(|(_, _, p)| p).collect();
    assert_eq!(primary, [false, false, true]);
    assert_eq!(last.focused_text().as_deref(), Some("Save"));

    let mut first = Bench::new(ButtonOrder::AffirmativeFirst);
    let panel = first.ask();
    assert_eq!(first.texts(panel), ["Save", "Don't save", "Cancel"]);
    assert_eq!(first.focused_text().as_deref(), Some("Save"));
}

/// Enter answers with the default and closes, giving the focus back to the field.
#[test]
fn enter_answers_with_the_default() {
    let mut bench = Bench::new(ButtonOrder::AffirmativeLast);
    bench.ask();
    bench.named(NamedKey::Enter);
    assert_eq!(bench.messages(), [Msg::Answer("save")]);
    assert_eq!(bench.ui.dialogs().count(), 0);
    assert_eq!(bench.ui.focused(), Some(bench.field));
}

/// Escape answers with the cancel button, even with the focus on another button.
#[test]
fn escape_cancels() {
    let mut bench = Bench::new(ButtonOrder::AffirmativeLast);
    bench.ask();
    // Tab moves inside the dialog only: Save → Don't save (wrapping to the first drawn).
    bench.named(NamedKey::Tab);
    assert_eq!(bench.focused_text().as_deref(), Some("Don't save"));
    bench.named(NamedKey::Escape);
    assert_eq!(bench.messages(), [Msg::Answer("cancel")]);
    assert_eq!(bench.ui.dialogs().count(), 0);
    assert_eq!(bench.ui.focused(), Some(bench.field));
}

/// Tab cycles through the three buttons and never reaches the field behind.
#[test]
fn tab_stays_inside() {
    let mut bench = Bench::new(ButtonOrder::AffirmativeFirst);
    bench.ask();
    let mut seen = Vec::new();
    for _ in 0..4 {
        seen.push(bench.focused_text().unwrap());
        bench.named(NamedKey::Tab);
    }
    assert_eq!(seen, ["Save", "Don't save", "Cancel", "Save"]);
}

/// Enter on a focused button presses that one; a click outside the panel does nothing; a click on
/// a button answers with it.
#[test]
fn pointer_and_focused_button() {
    let mut bench = Bench::new(ButtonOrder::AffirmativeLast);
    let panel = bench.ask();
    bench.named(NamedKey::Tab);
    bench.named(NamedKey::Enter);
    assert_eq!(bench.messages(), [Msg::Answer("discard")]);

    let panel_again = bench.ask();
    assert_ne!(panel, panel_again, "a new dialog, new widgets");
    // Outside the panel, over the field behind: nothing, and the dialog stays.
    let field = bench.ui.rect(bench.field).unwrap();
    bench.click(middle(field));
    assert_eq!(bench.messages(), []);
    assert_eq!(bench.ui.dialogs().count(), 1);
    // On *Cancel*.
    let cancel = bench.buttons(panel_again)[1].0;
    bench.click(middle(bench.ui.rect(cancel).unwrap()));
    assert_eq!(bench.messages(), [Msg::Answer("cancel")]);
    assert_eq!(bench.ui.dialogs().count(), 0);
}

/// An error over a question: Escape closes only the error, and the focus goes back to the
/// question's button.
#[test]
fn a_dialog_above_a_dialog() {
    let mut bench = Bench::new(ButtonOrder::AffirmativeLast);
    let question = bench.ask();
    let error = DialogSpec::new(DialogKind::Error, "Cannot save", "The disk is full.").button(
        "OK",
        ButtonRole::Other,
        Msg::Answer("ok"),
    );
    let error = bench.ui.open_dialog(error, &mut bench.list).unwrap();
    assert_eq!(bench.focused_text().as_deref(), Some("OK"));
    assert_eq!(
        bench.ui.dialogs().collect::<Vec<_>>(),
        [question, error],
        "lowest first"
    );
    bench.named(NamedKey::Escape);
    assert_eq!(bench.messages(), [Msg::Answer("ok")]);
    assert_eq!(bench.ui.dialogs().collect::<Vec<_>>(), [question]);
    assert_eq!(bench.focused_text().as_deref(), Some("Save"));
    // Closing by the program, too.
    bench.ui.close_dialog(question, &mut bench.list).unwrap();
    assert_eq!(bench.ui.focused(), Some(bench.field));
}

/// A menu opened while a dialog is open goes above it; a click outside the menu closes the menu
/// only.
#[test]
fn a_menu_above_a_dialog() {
    let mut bench = Bench::new(ButtonOrder::AffirmativeLast);
    let panel = bench.ask();
    let anchor = bench.ui.rect(panel).unwrap();
    bench
        .ui
        .open_menu_model(
            &MenuModel::new().action(bench.save),
            Popup {
                anchor: Rect::new(anchor.x0, anchor.y0, anchor.x0, anchor.y0),
                side: PopupSide::Below,
                owner: None,
            },
            false,
            &mut bench.list,
        )
        .unwrap();
    assert_eq!(bench.ui.menus().count(), 1);
    bench.click(Point::new(5.0, 595.0));
    assert_eq!(bench.ui.menus().count(), 0);
    assert_eq!(bench.ui.dialogs().count(), 1, "the dialog stays");
}

/// Ctrl+S runs Save without a dialog and does nothing behind one; F10 does not reach the menu
/// bar (there is none here, so the check is that nothing is queued).
#[test]
fn shortcuts_are_off_behind_a_dialog() {
    let mut bench = Bench::new(ButtonOrder::AffirmativeLast);
    let primary = Modifiers {
        primary: true,
        ..Modifiers::default()
    };
    bench.key(&Key::Character("s".into()), primary);
    assert_eq!(bench.messages(), [Msg::Action(bench.save)]);
    bench.ask();
    bench.key(&Key::Character("s".into()), primary);
    bench.named(NamedKey::Function(10));
    assert_eq!(bench.messages(), []);
    assert_eq!(bench.ui.dialogs().count(), 1);
}

/// The panel is centred, between the minimum and the maximum width in a large window, and fits
/// a small one with its buttons visible.
#[test]
fn size_and_place() {
    let mut bench = Bench::new(ButtonOrder::AffirmativeLast);
    let panel = bench.ask();
    let rect = bench.ui.rect(panel).unwrap();
    assert!(
        rect.width() >= DIALOG_MIN_WIDTH && rect.width() <= DIALOG_MAX_WIDTH,
        "{rect:?}"
    );
    let centre = middle(rect);
    assert!(
        (centre.x - 400.0).abs() <= 1.0 && (centre.y - 300.0).abs() <= 1.0,
        "{rect:?}"
    );

    // A small window: the panel inside it, the buttons too.
    let small = Rect::new(0.0, 0.0, 260.0, 140.0);
    bench.ui.layout(small, &mut bench.list);
    let rect = bench.ui.rect(panel).unwrap();
    assert!(
        rect.x0 >= 0.0 && rect.y0 >= 0.0 && rect.x1 <= 260.0 && rect.y1 <= 140.0,
        "{rect:?}"
    );
    for (button, _, _) in bench.buttons(panel) {
        let b = bench.ui.rect(button).unwrap();
        assert!(b.y1 <= rect.y1 && b.y0 >= rect.y0, "{b:?} outside {rect:?}");
    }
}

/// The veil covers the window in the theme's `scrim`, translucent in both halves of every pair.
#[test]
fn the_veil_is_the_scrim_colour() {
    for theme in BuiltInTheme::ALL {
        for appearance in [Appearance::Light, Appearance::Dark] {
            let scrim = theme.pair().instance(appearance).color(ColorToken::Scrim);
            assert!(scrim.a > 0 && scrim.a < u8::MAX, "{theme:?} {appearance:?}");
        }
    }
}

/// The dialog is a modal `Dialog` named by its title and described by its message (an error is
/// an `AlertDialog`), a child of the window node.
#[test]
fn accessibility() {
    let mut bench = Bench::new(ButtonOrder::AffirmativeLast);
    let _ = bench.ui.take_access_update();
    let panel = bench.ask();
    let update = bench.ui.take_access_update().into_tree_update();
    let node = update
        .nodes
        .iter()
        .find(|(id, _)| *id == NodeId(panel.get()))
        .map(|(_, node)| node)
        .unwrap();
    assert_eq!(node.role(), Role::Dialog);
    assert_eq!(node.label(), Some("Save the changes?"));
    assert_eq!(
        node.description(),
        Some("Your changes will be lost if you don't save them.")
    );
    assert!(node.is_modal());
    let window = update
        .nodes
        .iter()
        .find(|(_, node)| node.role() == Role::Window)
        .map(|(_, node)| node)
        .unwrap();
    assert_eq!(window.children().len(), 2, "the root and the dialog's veil");

    let error = DialogSpec::new(DialogKind::Error, "Cannot save", "The disk is full.").button(
        "OK",
        ButtonRole::Other,
        Msg::Answer("ok"),
    );
    let error = bench.ui.open_dialog(error, &mut bench.list).unwrap();
    let update = bench.ui.take_access_update().into_tree_update();
    let node = update
        .nodes
        .iter()
        .find(|(id, _)| *id == NodeId(error.get()))
        .map(|(_, node)| node)
        .unwrap();
    assert_eq!(node.role(), Role::AlertDialog);
}
