// SPDX-License-Identifier: GPL-3.0-or-later

//! Actions run from the keyboard through the interface (ADR-0048 AM2–AM5, AM13; F2.11d1),
//! headless: a key no widget handles becomes the application's message; a widget always wins
//! over a shortcut; a view's context covers a global shortcut while the focus is inside it; a
//! disabled action consumes its key; repeats run only repeatable actions; the release of a
//! shortcut's key is consumed; the fallbacks by position reach the action with a Russian layout
//! and with the digit row of AZERTY; and the standard editing actions act on a focused text
//! field's text and local undo, never on the document, even after the user moved Undo to another
//! key.

#![allow(
    clippy::unwrap_used,
    reason = "test helpers: a failed unwrap fails the test"
)]

use orchestra_text::Faces;
use orchestra_ui::{
    ActionId, ActionSpec, ActionState, Appearance, BuiltInTheme, ButtonState, Constraints, Context,
    Defaults, DisplayList, EditConvention, EditRole, EventCx, Handled, InputEvent, Key, LayoutCx,
    LayoutProps, Modifiers, PhysicalKey, Platform, RecordCx, Recorder, Rect, Registry, Shortcut,
    ShortcutKey, Size, TextField, Ui, Widget, WidgetEvent, WidgetId,
};

/// The messages of these tests.
#[derive(Debug, Clone, PartialEq)]
enum Msg {
    /// An action ran (the interface's message for it).
    Action(ActionId),
    /// A key's release reached the root: nothing consumed it.
    Released(Key),
    /// The field committed its text.
    Committed(String),
}

/// The root: a column of its children, logging the releases that reach it.
#[derive(Debug)]
struct Root;

impl Widget<Msg> for Root {
    fn measure(&mut self, cx: &mut LayoutCx<'_, Msg>, constraints: Constraints) -> Size {
        for child in 0..cx.child_count() {
            cx.measure_child(child, Constraints::tight(Size::new(200.0, 28.0)));
        }
        constraints.constrain(Size::new(400.0, 200.0))
    }

    fn arrange(&mut self, cx: &mut LayoutCx<'_, Msg>, rect: Rect) {
        for child in 0..cx.child_count() {
            #[allow(clippy::cast_precision_loss, reason = "two children")]
            let y = rect.y0 + 10.0 + 40.0 * child as f32;
            cx.place_child(
                child,
                Rect::from_origin_size(rect.x0 + 10.0, y, 200.0, 28.0),
            );
        }
    }

    fn record(&self, _cx: &RecordCx, _recorder: &mut Recorder<'_>) {}

    fn event(&mut self, cx: &mut EventCx<'_, Msg>, event: &WidgetEvent<'_>) -> Handled {
        if let WidgetEvent::Key {
            key,
            state: ButtonState::Released,
            ..
        } = event
        {
            cx.emit(Msg::Released((*key).clone()));
        }
        Handled::No
    }
}

/// A focusable stand-in for a view (the score): it declares a context and handles no key.
#[derive(Debug)]
struct View {
    /// Its context.
    context: Context,
}

impl Widget<Msg> for View {
    fn measure(&mut self, _cx: &mut LayoutCx<'_, Msg>, constraints: Constraints) -> Size {
        constraints.constrain(Size::new(200.0, 28.0))
    }

    fn record(&self, _cx: &RecordCx, _recorder: &mut Recorder<'_>) {}

    fn focusable(&self) -> bool {
        true
    }

    fn context(&self) -> Option<Context> {
        Some(self.context)
    }
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

/// A shortcut of a character key.
fn ch(modifiers: Modifiers, character: char) -> Shortcut {
    Shortcut::new(modifiers, ShortcutKey::Character(character)).unwrap()
}

/// The actions of the bench.
struct Ids {
    undo: ActionId,
    copy: ActionId,
    save: ActionId,
    zoom: ActionId,
    find: ActionId,
    note: ActionId,
    digit: ActionId,
    disabled: ActionId,
}

/// The bench: the interface with a field and a view, and its actions.
struct Bench {
    ui: Ui<Msg>,
    list: DisplayList,
    field: WidgetId,
    view: WidgetId,
    ids: Ids,
}

/// An action of `context` with `shortcuts` everywhere.
fn spec(identifier: &'static str, context: Context, shortcuts: &[Shortcut]) -> ActionSpec {
    ActionSpec {
        identifier,
        name: "action-test",
        context,
        defaults: Defaults::everywhere(shortcuts),
        repeatable: false,
        edit: None,
    }
}

impl Bench {
    fn new() -> Self {
        let theme = *BuiltInTheme::Studio.pair().instance(Appearance::Light);
        let mut registry = Registry::new(Platform::Linux);
        let notation = registry.add_context("notation").unwrap();
        let global = Context::GLOBAL;
        let ids = Ids {
            undo: registry
                .register(ActionSpec {
                    edit: Some(EditRole::Undo),
                    ..spec("edit.undo", global, &[ch(PRIMARY, 'z')])
                })
                .unwrap(),
            copy: registry
                .register(ActionSpec {
                    edit: Some(EditRole::Copy),
                    ..spec("edit.copy", global, &[ch(PRIMARY, 'c')])
                })
                .unwrap(),
            save: registry
                .register(spec("file.save", global, &[ch(PRIMARY, 's')]))
                .unwrap(),
            zoom: registry
                .register(ActionSpec {
                    repeatable: true,
                    ..spec("view.zoom-in", global, &[ch(PRIMARY, '+')])
                })
                .unwrap(),
            find: registry
                .register(spec("view.find", global, &[ch(NONE, 'f')]))
                .unwrap(),
            note: registry
                .register(spec("note.f", notation, &[ch(NONE, 'f')]))
                .unwrap(),
            digit: registry
                .register(spec(
                    "note.quarter",
                    notation,
                    &[Shortcut::new(NONE, ShortcutKey::Digit(5)).unwrap()],
                ))
                .unwrap(),
            disabled: registry
                .register(spec("file.export", global, &[ch(PRIMARY, 'e')]))
                .unwrap(),
        };
        registry
            .set_state(ids.disabled, ActionState::DISABLED)
            .unwrap();
        let mut ui = Ui::new(Root, &theme, Faces::bundled().unwrap());
        ui.set_actions(registry, Msg::Action);
        let root = ui.root();
        let field = ui
            .add(
                root,
                TextField::new("", |text| Msg::Committed(text.to_owned()))
                    .convention(EditConvention::Linux),
                LayoutProps::default(),
            )
            .unwrap();
        let view = ui
            .add(root, View { context: notation }, LayoutProps::default())
            .unwrap();
        let mut list = DisplayList::new();
        ui.layout(Rect::new(0.0, 0.0, 400.0, 200.0), &mut list);
        Self {
            ui,
            list,
            field,
            view,
            ids,
        }
    }

    /// One key event.
    fn key(
        &mut self,
        key: &str,
        physical: PhysicalKey,
        state: ButtonState,
        repeat: bool,
        modifiers: Modifiers,
    ) {
        self.ui.input(
            &InputEvent::Key {
                key: Key::Character(key.to_owned()),
                physical,
                state,
                repeat,
                modifiers,
            },
            &mut self.list,
        );
    }

    /// Presses and releases a character key at an unknown place.
    fn press(&mut self, key: &str, modifiers: Modifiers) {
        self.press_at(key, PhysicalKey::Other, modifiers);
    }

    /// Presses and releases a character key at `physical`.
    fn press_at(&mut self, key: &str, physical: PhysicalKey, modifiers: Modifiers) {
        self.key(key, physical, ButtonState::Pressed, false, modifiers);
        self.key(key, physical, ButtonState::Released, false, modifiers);
    }

    /// Types `text` into the focused widget (each character's key, then its text).
    fn type_text(&mut self, text: &str) {
        for c in text.chars() {
            let key = c.to_string();
            self.key(&key, PhysicalKey::Other, ButtonState::Pressed, false, NONE);
            self.ui
                .input(&InputEvent::Text(key.clone()), &mut self.list);
            self.key(&key, PhysicalKey::Other, ButtonState::Released, false, NONE);
        }
    }

    /// The messages since the last call.
    fn messages(&mut self) -> Vec<Msg> {
        self.ui.take_messages().collect()
    }

    /// Focuses `id`.
    fn focus(&mut self, id: Option<WidgetId>) {
        self.ui.set_focus(id, &mut self.list).unwrap();
        let _ = self.messages();
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
fn a_key_no_widget_handles_runs_its_action() {
    let mut bench = Bench::new();
    // No focus: the global context only.
    bench.press("s", PRIMARY);
    assert_eq!(bench.messages(), vec![Msg::Action(bench.ids.save)]);
    // A key with no action does nothing.
    bench.press("k", PRIMARY);
    assert_eq!(bench.messages(), vec![]);
}

#[test]
fn the_release_of_a_shortcut_is_consumed_and_others_are_not() {
    let mut bench = Bench::new();
    bench.focus(Some(bench.view));
    bench.press("s", PRIMARY);
    // The press ran Save; its release reached nothing.
    assert_eq!(bench.messages(), vec![Msg::Action(bench.ids.save)]);
    bench.press("k", PRIMARY);
    assert_eq!(
        bench.messages(),
        vec![Msg::Released(Key::Character("k".into()))]
    );
    // Ctrl released before S: the key alone is compared, the release is still consumed.
    bench.key(
        "s",
        PhysicalKey::Letter(18),
        ButtonState::Pressed,
        false,
        PRIMARY,
    );
    bench.key(
        "s",
        PhysicalKey::Letter(18),
        ButtonState::Released,
        false,
        NONE,
    );
    assert_eq!(bench.messages(), vec![Msg::Action(bench.ids.save)]);
}

#[test]
fn repeats_run_only_repeatable_actions() {
    let mut bench = Bench::new();
    bench.key(
        "s",
        PhysicalKey::Other,
        ButtonState::Pressed,
        false,
        PRIMARY,
    );
    bench.key("s", PhysicalKey::Other, ButtonState::Pressed, true, PRIMARY);
    bench.key("s", PhysicalKey::Other, ButtonState::Pressed, true, PRIMARY);
    assert_eq!(bench.messages(), vec![Msg::Action(bench.ids.save)]);
    bench.key(
        "+",
        PhysicalKey::Other,
        ButtonState::Pressed,
        false,
        PRIMARY,
    );
    bench.key("+", PhysicalKey::Other, ButtonState::Pressed, true, PRIMARY);
    assert_eq!(
        bench.messages(),
        vec![Msg::Action(bench.ids.zoom), Msg::Action(bench.ids.zoom)]
    );
}

#[test]
fn a_view_covers_a_global_shortcut_while_the_focus_is_inside() {
    let mut bench = Bench::new();
    bench.press("f", NONE);
    assert_eq!(bench.messages(), vec![Msg::Action(bench.ids.find)]);
    bench.focus(Some(bench.view));
    bench.press("f", NONE);
    assert_eq!(bench.messages(), vec![Msg::Action(bench.ids.note)]);
    // The global ones still work inside the view.
    bench.press("s", PRIMARY);
    assert_eq!(bench.messages(), vec![Msg::Action(bench.ids.save)]);
    // And a focused field keeps its letters (a widget wins over a shortcut, AM4).
    bench.focus(Some(bench.field));
    bench.type_text("f");
    assert_eq!(bench.text(), "f");
    assert_eq!(bench.messages(), vec![]);
}

#[test]
fn a_disabled_action_consumes_its_key() {
    let mut bench = Bench::new();
    bench.focus(Some(bench.view));
    bench.press("e", PRIMARY);
    // Nothing ran, and the release was consumed too.
    assert_eq!(bench.messages(), vec![]);
    assert_eq!(
        bench.ui.action_state(bench.ids.disabled),
        Some(ActionState::DISABLED)
    );
}

#[test]
fn the_fallbacks_by_position_reach_the_actions() {
    let mut bench = Bench::new();
    // A Russian layout: the key of the US Z types "я".
    bench.press_at("я", PhysicalKey::Letter(25), PRIMARY);
    assert_eq!(bench.messages(), vec![Msg::Action(bench.ids.undo)]);
    // French AZERTY in the score: the key that types "(" is the digit 5 of step input.
    bench.focus(Some(bench.view));
    bench.press_at("(", PhysicalKey::Digit(5), NONE);
    assert_eq!(bench.messages(), vec![Msg::Action(bench.ids.digit)]);
}

#[test]
fn editing_actions_act_on_the_focused_field_and_never_on_the_document() {
    let mut bench = Bench::new();
    // Without a field, Undo is the application's (the document's history).
    bench.press("z", PRIMARY);
    assert_eq!(bench.messages(), vec![Msg::Action(bench.ids.undo)]);

    bench.focus(Some(bench.field));
    // An empty local history: Undo is disabled, and Ctrl+Z does not reach the document.
    assert_eq!(
        bench
            .ui
            .action_state(bench.ids.undo)
            .map(|state| state.enabled),
        Some(false)
    );
    bench.press("z", PRIMARY);
    assert_eq!(bench.messages(), vec![]);

    // The user moves Undo to Ctrl+U: in the field Ctrl+U undoes the field's typing.
    bench
        .ui
        .actions_mut()
        .unwrap()
        .set_shortcuts(bench.ids.undo, &[ch(PRIMARY, 'u')])
        .unwrap();
    bench.type_text("abc");
    assert_eq!(bench.text(), "abc");
    assert_eq!(
        bench
            .ui
            .action_state(bench.ids.undo)
            .map(|state| state.enabled),
        Some(true)
    );
    bench.press("u", PRIMARY);
    assert_eq!(bench.text(), "");
    assert_eq!(bench.messages(), vec![], "the document's Undo never runs");
    // Ctrl+Z, the field's own fixed key (ADR-0047 TF3), still works there too.
    bench.type_text("d");
    bench.press("z", PRIMARY);
    assert_eq!(bench.text(), "");
}

#[test]
fn copy_follows_the_fields_selection() {
    let mut bench = Bench::new();
    bench.focus(Some(bench.field));
    bench.type_text("abc");
    // The caret after the typing: nothing selected, nothing to copy.
    assert_eq!(
        bench
            .ui
            .action_state(bench.ids.copy)
            .map(|state| state.enabled),
        Some(false)
    );
    bench.press("a", PRIMARY);
    assert_eq!(
        bench
            .ui
            .action_state(bench.ids.copy)
            .map(|state| state.enabled),
        Some(true)
    );
    // Without the focus in a field, the registry's state again.
    bench.focus(None);
    assert_eq!(
        bench.ui.action_state(bench.ids.copy),
        Some(ActionState::ENABLED)
    );
}
