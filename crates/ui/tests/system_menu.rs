// SPDX-License-Identifier: GPL-3.0-or-later

//! The interface's side of the system's menu bar (F2.11e1, ADR-0048 AM2, AM9, AM13), on every
//! platform: the description built from the model and the registry (names, shortcuts, states,
//! the roles moved into macOS's application menu), the changed states handed to the window once
//! each (the registry's marks and the focused field's editing actions), and an action chosen in
//! the system's menu run on the shortcuts' path.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test helpers: a failed unwrap or index fails the test"
)]

use orchestra_text::Faces;
use orchestra_ui::{
    ActionId, ActionSpec, ActionState, Appearance, BuiltInTheme, ButtonState, Context, Defaults,
    DisplayList, EditConvention, EditRole, Flex, InputEvent, Key, KeyNames, LayoutProps, Length,
    MenuModel, MenuRole, Modifiers, NamedKey, PhysicalKey, Platform, Rect, Registry, Shortcut,
    ShortcutKey, Space, SystemItem, SystemMenu, TextField, Ui, WidgetId,
};

/// The messages of these tests.
#[derive(Debug, Clone, PartialEq)]
enum Msg {
    /// An action ran.
    Action(ActionId),
    /// The field committed its text.
    Committed(String),
}

/// The actions of the bench.
#[derive(Debug, Clone, Copy)]
struct Ids {
    save: ActionId,
    quit: ActionId,
    copy: ActionId,
    preferences: ActionId,
    help: ActionId,
}

/// The bench: an interface with a text field, its registry (macOS), and the model of the bar.
struct Bench {
    ui: Ui<Msg>,
    list: DisplayList,
    field: WidgetId,
    ids: Ids,
    model: MenuModel,
}

impl Bench {
    fn new() -> Self {
        let theme = *BuiltInTheme::Studio.pair().instance(Appearance::Light);
        let mut registry = Registry::new(Platform::MacOs);
        let primary = Modifiers {
            primary: true,
            ..Modifiers::default()
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
            save: add("file.save", "Save", 's', None),
            quit: add("file.quit", "Quit", 'q', None),
            copy: add("edit.copy", "Copy", 'c', Some(EditRole::Copy)),
            preferences: add("edit.preferences", "Settings…", ',', None),
            help: add("help.contents", "User guide", '?', None),
        };
        registry.set_state(ids.save, ActionState::DISABLED).unwrap();
        // As in the program: the editing actions are disabled outside a text field (AM13).
        registry.set_state(ids.copy, ActionState::DISABLED).unwrap();
        registry.set_texts(KeyNames::new(|key| format!("{key:?}")), str::to_owned);
        let model = MenuModel::new()
            .submenu(
                "File",
                None,
                MenuModel::new()
                    .action(ids.save)
                    .separator()
                    .action(ids.quit)
                    .role(MenuRole::Quit),
            )
            .submenu(
                "Edit",
                None,
                MenuModel::new()
                    .action(ids.copy)
                    .separator()
                    .action(ids.preferences)
                    .role(MenuRole::Preferences),
            )
            .submenu("Help", None, MenuModel::new().action(ids.help))
            .role(MenuRole::Help);
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
                TextField::new("hello", |text| Msg::Committed(text.to_owned()))
                    .convention(EditConvention::MacOs),
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
            model,
        }
    }

    /// The updates since the last call.
    fn updates(&mut self) -> Vec<(ActionId, ActionState)> {
        let mut out = Vec::new();
        self.ui.take_menu_updates(&mut out);
        out
    }

    /// Presses and releases `key` with the primary modifier.
    fn primary(&mut self, key: &str) {
        for state in [ButtonState::Pressed, ButtonState::Released] {
            self.ui.input(
                &InputEvent::Key {
                    key: Key::Character(key.to_owned()),
                    physical: PhysicalKey::Other,
                    state,
                    repeat: false,
                    modifiers: Modifiers {
                        primary: true,
                        ..Modifiers::default()
                    },
                },
                &mut self.list,
            );
        }
    }
}

/// The names of `items`, "-" for a separator.
fn names(items: &[SystemItem]) -> Vec<String> {
    items
        .iter()
        .map(|item| match item {
            SystemItem::Action { name, .. } | SystemItem::Submenu { name, .. } => name.clone(),
            SystemItem::Separator => "-".to_owned(),
        })
        .collect()
}

/// The items of submenu number `index`.
fn items(menu: &SystemMenu, index: usize) -> &[SystemItem] {
    match &menu.items[index] {
        SystemItem::Submenu { items, .. } => items,
        _ => &[],
    }
}

#[test]
fn the_description_has_the_application_menu_names_shortcuts_and_states() {
    let mut bench = Bench::new();
    let model = bench.model.clone();
    let menu = bench.ui.system_menu(&model, "Orchestra");
    assert_eq!(names(&menu.items), ["Orchestra", "File", "Edit", "Help"]);
    assert_eq!(names(items(&menu, 0)), ["Settings…", "-", "Quit"]);
    assert_eq!(names(items(&menu, 1)), ["Save"]);
    assert_eq!(names(items(&menu, 2)), ["Copy"]);
    assert!(matches!(
        menu.items[3],
        SystemItem::Submenu {
            role: Some(MenuRole::Help),
            ..
        }
    ));
    // Save is disabled, its shortcut ⌘S kept; Help's shortcut is the character "?".
    let SystemItem::Action {
        state, shortcut, ..
    } = &items(&menu, 1)[0]
    else {
        panic!("Save is an action");
    };
    assert!(!state.enabled);
    assert_eq!(
        shortcut.map(|shortcut| shortcut.key()),
        Some(ShortcutKey::Character('s'))
    );
    let SystemItem::Action { shortcut, .. } = &items(&menu, 3)[0] else {
        panic!("User guide is an action");
    };
    assert_eq!(
        shortcut.map(|shortcut| shortcut.key()),
        Some(ShortcutKey::Character('?'))
    );
    assert_eq!(menu.actions().count(), 5);
    let _ = (bench.ids.quit, bench.ids.preferences, bench.ids.help);
}

#[test]
fn changed_states_reach_the_window_once_each() {
    let mut bench = Bench::new();
    // Before a system menu exists there is nothing to hand on.
    bench
        .ui
        .actions_mut()
        .unwrap()
        .set_state(bench.ids.save, ActionState::ENABLED)
        .unwrap();
    bench.ui.frame(&mut bench.list);
    assert_eq!(bench.updates(), vec![]);
    let model = bench.model.clone();
    let _ = bench.ui.system_menu(&model, "Orchestra");
    // The registry's change, once; then nothing.
    bench
        .ui
        .actions_mut()
        .unwrap()
        .set_state(bench.ids.save, ActionState::DISABLED)
        .unwrap();
    bench.ui.frame(&mut bench.list);
    assert_eq!(
        bench.updates(),
        vec![(bench.ids.save, ActionState::DISABLED)]
    );
    assert_eq!(bench.updates(), vec![]);
    // The focused field decides Copy (AM13): enabled with a selection, without the registry
    // knowing. Focus by the keyboard selects everything.
    bench
        .ui
        .set_focus(Some(bench.field), &mut bench.list)
        .unwrap();
    assert_eq!(
        bench.updates(),
        vec![(bench.ids.copy, ActionState::ENABLED)]
    );
    // The caret moves to the end (→ ends the selection on macOS): Copy disabled again.
    bench.ui.input(
        &InputEvent::Key {
            key: Key::Named(NamedKey::ArrowRight),
            physical: PhysicalKey::Other,
            state: ButtonState::Pressed,
            repeat: false,
            modifiers: Modifiers::default(),
        },
        &mut bench.list,
    );
    assert_eq!(
        bench.updates(),
        vec![(bench.ids.copy, ActionState::DISABLED)]
    );
    assert_eq!(bench.updates(), vec![]);
}

#[test]
fn an_action_chosen_in_the_system_menu_runs_like_a_shortcut() {
    let mut bench = Bench::new();
    let model = bench.model.clone();
    let _ = bench.ui.system_menu(&model, "Orchestra");
    // Copy on the focused field: it copies the field's text, the application hears nothing.
    bench
        .ui
        .set_focus(Some(bench.field), &mut bench.list)
        .unwrap();
    bench.ui.run_action(bench.ids.copy, &mut bench.list);
    assert_eq!(
        bench.ui.clipboard().read_text().unwrap().as_deref(),
        Some("hello")
    );
    assert_eq!(bench.ui.take_messages().count(), 0);
    // A context menu still open closes first; Quit becomes the application's message.
    bench.primary("a");
    bench.ui.input(
        &InputEvent::Key {
            key: Key::Named(NamedKey::ContextMenu),
            physical: PhysicalKey::Other,
            state: ButtonState::Pressed,
            repeat: false,
            modifiers: Modifiers::default(),
        },
        &mut bench.list,
    );
    bench.ui.run_action(bench.ids.quit, &mut bench.list);
    assert_eq!(bench.ui.menus().count(), 0);
    assert_eq!(
        bench.ui.take_messages().collect::<Vec<_>>(),
        vec![Msg::Action(bench.ids.quit)]
    );
}
