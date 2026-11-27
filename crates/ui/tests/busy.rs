// SPDX-License-Identifier: GPL-3.0-or-later

//! The interface while the application is busy (ADR-0054 AR1, SCALETTA F2.14a), headless: the busy
//! layer blocks the pointer, the keys, typed text and the shortcuts; the pointer has the wait
//! shape over the whole window; the window is marked busy for assistive technologies and lists no
//! dialog; ending it gives the focus and the shortcuts back, also above a dialog; a second begin is
//! refused and a second end changes nothing.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test helpers: a failed unwrap or index fails the test"
)]

use accesskit::Role;
use orchestra_text::Faces;
use orchestra_ui::{
    ActionId, ActionSpec, Appearance, BuiltInTheme, ButtonRole, ButtonState, Context, Defaults,
    DialogKind, DialogSpec, DisplayList, Flex, InputEvent, Key, LayoutProps, Length, Modifiers,
    NamedKey, PhysicalKey, Platform, Point, PointerButton, PointerShape, Rect, Registry, Shortcut,
    ShortcutKey, Space, TextField, Ui, WidgetId,
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

/// The window of the tests, logical pixels.
const WINDOW: Rect = Rect::new(0.0, 0.0, 800.0, 600.0);

/// The interface with a focused text field and a global action *Save* on Ctrl+S.
struct Bench {
    /// The interface.
    ui: Ui<Msg>,
    /// Its display list.
    list: DisplayList,
    /// The text field.
    field: WidgetId,
    /// The *Save* action.
    save: ActionId,
}

impl Bench {
    /// The bench, laid out, the field focused.
    fn new() -> Self {
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

    /// Presses and releases `key` with `modifiers`.
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

    /// Ctrl+S.
    fn ctrl_s(&mut self) {
        let primary = Modifiers {
            primary: true,
            ..Modifiers::default()
        };
        self.key(&Key::Character("s".into()), primary);
    }

    /// Moves the pointer to `at`.
    fn hover(&mut self, at: Point) {
        self.ui
            .input(&InputEvent::PointerMoved { position: at }, &mut self.list);
    }

    /// A left click at `at`.
    fn click(&mut self, at: Point) {
        self.hover(at);
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

    /// The field's text.
    fn text(&self) -> String {
        self.ui
            .widget::<TextField<Msg>>(self.field)
            .unwrap()
            .text()
            .to_owned()
    }

    /// The messages waiting.
    fn messages(&mut self) -> Vec<Msg> {
        self.ui.take_messages().collect()
    }

    /// The window's accessibility node in the next update, if the update has it.
    fn window_node(&mut self) -> Option<accesskit::Node> {
        let update = self.ui.take_access_update().into_tree_update();
        update
            .nodes
            .into_iter()
            .find(|(_, node)| node.role() == Role::Window)
            .map(|(_, node)| node)
    }
}

/// The middle of `rect`.
fn middle(rect: Rect) -> Point {
    Point::new(
        f32::midpoint(rect.x0, rect.x1),
        f32::midpoint(rect.y0, rect.y1),
    )
}

/// One busy layer at a time; ending it twice changes nothing.
#[test]
fn one_layer_at_a_time() {
    let mut bench = Bench::new();
    assert!(!bench.ui.is_busy());
    assert!(bench.ui.begin_busy(&mut bench.list));
    assert!(bench.ui.is_busy());
    assert!(!bench.ui.begin_busy(&mut bench.list), "already busy");
    bench.ui.end_busy(&mut bench.list);
    assert!(!bench.ui.is_busy());
    bench.ui.end_busy(&mut bench.list);
    assert!(!bench.ui.is_busy());
    assert_eq!(bench.ui.focused(), Some(bench.field));
}

/// While busy nothing under the layer answers: typing, Tab, a click on the field, the shortcut
/// that would start a second save, the menu bar's key. It is no dialog for the application.
#[test]
fn the_layer_blocks_everything() {
    let mut bench = Bench::new();
    bench.ctrl_s();
    assert_eq!(bench.messages(), [Msg::Action(bench.save)]);
    bench.ui.begin_busy(&mut bench.list);
    assert_eq!(bench.ui.focused(), None, "the focus left the field");
    bench
        .ui
        .input(&InputEvent::Text("x".into()), &mut bench.list);
    bench.key(&Key::Named(NamedKey::Tab), Modifiers::default());
    bench.key(&Key::Named(NamedKey::Backspace), Modifiers::default());
    assert_eq!(bench.ui.focused(), None, "Tab found nothing to move to");
    let field = bench.ui.rect(bench.field).unwrap();
    bench.click(middle(field));
    assert_eq!(bench.ui.focused(), None, "a click reached nothing");
    bench.ctrl_s();
    bench.key(&Key::Named(NamedKey::Function(10)), Modifiers::default());
    assert_eq!(bench.messages(), []);
    assert_eq!(bench.text(), "hello");
    assert_eq!(bench.ui.dialogs().count(), 0, "no dialog to show");
}

/// The pointer has the wait shape over the whole window while busy, the field's text beam and the
/// arrow again afterwards.
#[test]
fn the_pointer_waits_over_the_whole_window() {
    let mut bench = Bench::new();
    let field = middle(bench.ui.rect(bench.field).unwrap());
    let empty = Point::new(700.0, 500.0);
    bench.hover(field);
    assert_eq!(bench.ui.pointer_shape(), PointerShape::Text);
    bench.ui.begin_busy(&mut bench.list);
    assert_eq!(
        bench.ui.pointer_shape(),
        PointerShape::Wait,
        "over the field"
    );
    bench.hover(empty);
    assert_eq!(bench.ui.pointer_shape(), PointerShape::Wait, "over nothing");
    bench.hover(field);
    // The end finds what is under the pointer at once, without waiting for it to move.
    bench.ui.end_busy(&mut bench.list);
    assert_eq!(bench.ui.pointer_shape(), PointerShape::Text);
    bench.hover(empty);
    assert_eq!(bench.ui.pointer_shape(), PointerShape::Default);
}

/// Ending it gives the interface back: the focus is on the field again (which selects its text, as
/// any focus coming back to a field does), and the shortcut works.
#[test]
fn the_end_gives_the_interface_back() {
    let mut bench = Bench::new();
    bench.ui.begin_busy(&mut bench.list);
    bench.ui.end_busy(&mut bench.list);
    assert_eq!(bench.ui.focused(), Some(bench.field));
    bench.ctrl_s();
    assert_eq!(bench.messages(), [Msg::Action(bench.save)]);
    bench
        .ui
        .input(&InputEvent::Text("!".into()), &mut bench.list);
    assert_eq!(bench.text(), "!", "typing replaced the selected text");
}

/// Busy above a dialog (a save asked from "Save the changes?" runs while it is closing, or a
/// future dialog that saves): Enter does not press the dialog's default button, and the end leaves
/// the dialog open with its focus.
#[test]
fn above_a_dialog() {
    let mut bench = Bench::new();
    let spec = DialogSpec::new(DialogKind::Question, "Save?", "Save the changes?")
        .button("Cancel", ButtonRole::Cancel, Msg::Answer("cancel"))
        .button("Save", ButtonRole::Affirmative, Msg::Answer("save"));
    let panel = bench.ui.open_dialog(spec, &mut bench.list).unwrap();
    let in_dialog = bench.ui.focused();
    assert!(in_dialog.is_some());
    bench.ui.begin_busy(&mut bench.list);
    bench.key(&Key::Named(NamedKey::Enter), Modifiers::default());
    bench.key(&Key::Named(NamedKey::Escape), Modifiers::default());
    assert_eq!(bench.messages(), []);
    bench.ui.end_busy(&mut bench.list);
    assert_eq!(bench.ui.dialogs().collect::<Vec<_>>(), [panel]);
    assert_eq!(bench.ui.focused(), in_dialog);
}

/// Assistive technologies: the window node is marked busy and lists no dialog while the layer is
/// up, and is no longer busy after it.
#[test]
fn the_window_is_busy_for_assistive_technologies() {
    let mut bench = Bench::new();
    let window = bench.window_node().unwrap();
    assert!(!window.is_busy());
    bench.ui.begin_busy(&mut bench.list);
    let window = bench.window_node().unwrap();
    assert!(window.is_busy());
    assert_eq!(window.children().len(), 1, "only the interface's root");
    bench.ui.end_busy(&mut bench.list);
    let window = bench.window_node().unwrap();
    assert!(!window.is_busy());
}
