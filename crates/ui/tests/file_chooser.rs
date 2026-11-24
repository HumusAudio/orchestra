// SPDX-License-Identifier: GPL-3.0-or-later

//! The interface while a file chooser is open (ADR-0050 DG4, SCALETTA F2.12a5a), headless: the
//! request waits for the window once, a second one is refused, the empty layer blocks the pointer,
//! the keys, typed text and the shortcuts, it is no dialog for the application or for assistive
//! technologies, and the answer removes it and gives the focus back, also above a dialog.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test helpers: a failed unwrap or index fails the test"
)]

use accesskit::Role;
use orchestra_text::Faces;
use orchestra_ui::{
    ActionId, ActionSpec, Appearance, BuiltInTheme, ButtonRole, ButtonState, Context, Defaults,
    DialogKind, DialogSpec, DisplayList, FileRequest, Flex, InputEvent, Key, LayoutProps, Length,
    Modifiers, NamedKey, PhysicalKey, Platform, Point, PointerButton, Rect, Registry, Shortcut,
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

/// The interface with a focused text field and a global action *Open* on Ctrl+O.
struct Bench {
    ui: Ui<Msg>,
    list: DisplayList,
    field: WidgetId,
    open: ActionId,
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
        let open = registry
            .register(ActionSpec {
                identifier: "file.open",
                name: "Open",
                context: Context::GLOBAL,
                defaults: Defaults::everywhere(&[Shortcut::new(
                    Modifiers {
                        primary: true,
                        ..Modifiers::default()
                    },
                    ShortcutKey::Character('o'),
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
            open,
        }
    }

    /// A request to open a project.
    fn request() -> FileRequest {
        FileRequest::open("Open project", "Orchestra project", "orchestra")
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

    /// Ctrl+O.
    fn ctrl_o(&mut self) {
        let primary = Modifiers {
            primary: true,
            ..Modifiers::default()
        };
        self.key(&Key::Character("o".into()), primary);
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
}

/// The middle of `rect`.
fn middle(rect: Rect) -> Point {
    Point::new(
        f32::midpoint(rect.x0, rect.x1),
        f32::midpoint(rect.y0, rect.y1),
    )
}

/// The request is handed to the window once; while it is pending or open a second one is refused.
#[test]
fn one_request_at_a_time() {
    let mut bench = Bench::new();
    assert!(!bench.ui.choosing_file());
    assert!(bench.ui.choose_file(Bench::request(), &mut bench.list));
    assert!(bench.ui.choosing_file());
    assert!(!bench.ui.choose_file(Bench::request(), &mut bench.list));
    assert_eq!(bench.ui.take_file_request(), Some(Bench::request()));
    assert_eq!(bench.ui.take_file_request(), None, "taken once");
    assert!(!bench.ui.choose_file(Bench::request(), &mut bench.list));
}

/// While the chooser is open nothing under the layer answers: typing, Tab, a click on the field,
/// the shortcut that would open a second chooser. It is no dialog for the application.
#[test]
fn the_layer_blocks_everything() {
    let mut bench = Bench::new();
    bench.ctrl_o();
    assert_eq!(bench.messages(), [Msg::Action(bench.open)]);
    bench.ui.choose_file(Bench::request(), &mut bench.list);
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
    bench.ctrl_o();
    bench.key(&Key::Named(NamedKey::Function(10)), Modifiers::default());
    assert_eq!(bench.messages(), []);
    assert_eq!(bench.text(), "hello");
    assert_eq!(bench.ui.dialogs().count(), 0, "no dialog to show");
}

/// The answer removes the layer: the focus is back on the field, typing and the shortcut work
/// again, and a second answer changes nothing.
#[test]
fn the_answer_gives_the_interface_back() {
    let mut bench = Bench::new();
    bench.ui.choose_file(Bench::request(), &mut bench.list);
    bench.ui.file_chosen(&mut bench.list);
    assert!(!bench.ui.choosing_file());
    assert_eq!(bench.ui.focused(), Some(bench.field));
    bench.ctrl_o();
    assert_eq!(bench.messages(), [Msg::Action(bench.open)]);
    bench.ui.file_chosen(&mut bench.list);
    assert_eq!(bench.ui.focused(), Some(bench.field));
    // A request the window never took goes with the answer.
    bench.ui.choose_file(Bench::request(), &mut bench.list);
    bench.ui.file_chosen(&mut bench.list);
    assert_eq!(bench.ui.take_file_request(), None);
}

/// A chooser opened from a dialog goes above it; the answer leaves the dialog open with its
/// focus.
#[test]
fn above_a_dialog() {
    let mut bench = Bench::new();
    let spec = DialogSpec::new(DialogKind::Question, "Import?", "Choose a file to import.")
        .button("Cancel", ButtonRole::Cancel, Msg::Answer("cancel"))
        .button("Choose…", ButtonRole::Affirmative, Msg::Answer("choose"));
    let panel = bench.ui.open_dialog(spec, &mut bench.list).unwrap();
    let in_dialog = bench.ui.focused();
    assert!(in_dialog.is_some());
    bench.ui.choose_file(Bench::request(), &mut bench.list);
    // Enter would press the dialog's default button: blocked.
    bench.key(&Key::Named(NamedKey::Enter), Modifiers::default());
    assert_eq!(bench.messages(), []);
    bench.ui.file_chosen(&mut bench.list);
    assert_eq!(bench.ui.dialogs().collect::<Vec<_>>(), [panel]);
    assert_eq!(bench.ui.focused(), in_dialog);
}

/// Assistive technologies are told of no dialog: the window node keeps only the interface's
/// root while the chooser is open.
#[test]
fn no_dialog_for_assistive_technologies() {
    let mut bench = Bench::new();
    let _ = bench.ui.take_access_update();
    bench.ui.choose_file(Bench::request(), &mut bench.list);
    let update = bench.ui.take_access_update().into_tree_update();
    let window = update
        .nodes
        .iter()
        .find(|(_, node)| node.role() == Role::Window)
        .map(|(_, node)| node)
        .unwrap();
    assert_eq!(window.children().len(), 1, "only the interface's root");
}
