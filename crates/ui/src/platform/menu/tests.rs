// SPDX-License-Identifier: GPL-3.0-or-later

//! The description of the system's menu, on every platform (F2.11e1, F2.11e2): the items with a
//! role moved into macOS's application menu and renamed the macOS way, the system's own items
//! around them, the *Window* menu, the menus they left tidied, and shortcuts turned into macOS's
//! modifiers and logical keys.

#![allow(
    clippy::unwrap_used,
    reason = "test helpers: a failed unwrap fails the test"
)]

use super::{SystemCommand, SystemItem, SystemKey, SystemMenu, SystemShortcut, SystemTexts};
use crate::action::registry::{ActionId, ActionState, Context, Defaults, Registry};
use crate::action::shortcut::{Platform, Shortcut, ShortcutKey};
use crate::widget::menu::MenuRole;
use crate::window::input::{Modifiers, NamedKey};

/// `count` made-up actions of a registry, to name items with.
fn ids(count: usize) -> Vec<ActionId> {
    let mut registry = Registry::new(Platform::MacOs);
    (0..count)
        .map(|index| {
            let identifier: &'static str = Box::leak(format!("test.item-{index}").into_boxed_str());
            registry
                .register(crate::action::registry::ActionSpec {
                    identifier,
                    name: identifier,
                    context: Context::GLOBAL,
                    defaults: Defaults::default(),
                    repeatable: false,
                    edit: None,
                })
                .unwrap()
        })
        .collect()
}

/// An action item named `name` with `role`.
fn action(id: ActionId, name: &str, role: Option<MenuRole>) -> SystemItem {
    SystemItem::Action {
        action: id,
        name: name.to_owned(),
        shortcut: None,
        state: ActionState::ENABLED,
        role,
    }
}

/// A submenu named `name`.
fn submenu(name: &str, role: Option<MenuRole>, items: Vec<SystemItem>) -> SystemItem {
    SystemItem::Submenu {
        name: name.to_owned(),
        role,
        items,
    }
}

/// The names of `items`, "-" for a separator.
fn names(items: &[SystemItem]) -> Vec<String> {
    items
        .iter()
        .map(|item| match item {
            SystemItem::Action { name, .. }
            | SystemItem::Submenu { name, .. }
            | SystemItem::System { name, .. } => name.clone(),
            SystemItem::Separator => "-".to_owned(),
        })
        .collect()
}

/// macOS's names in en-US, as the catalogues give them (with the program's name filled in).
fn texts() -> SystemTexts {
    SystemTexts {
        settings: "Settings…".to_owned(),
        services: "Services".to_owned(),
        hide: "Hide Orchestra".to_owned(), // i18n-ignore: test data, the catalogues hold the real names
        hide_others: "Hide Others".to_owned(), // i18n-ignore: test data, the catalogues hold the real names
        show_all: "Show All".to_owned(), // i18n-ignore: test data, the catalogues hold the real names
        quit: "Quit Orchestra".to_owned(), // i18n-ignore: test data, the catalogues hold the real names
        window: "Window".to_owned(),
        minimize: "Minimize".to_owned(),
        zoom: "Zoom".to_owned(),
        bring_all_to_front: "Bring All to Front".to_owned(), // i18n-ignore: test data, the catalogues hold the real names
    }
}

/// The application menu's system items and their lines, without role items.
const SYSTEM_APP_ITEMS: [&str; 5] = [
    "Services",
    "-",
    "Hide Orchestra", // i18n-ignore: test data, the catalogues hold the real names
    "Hide Others",    // i18n-ignore: test data, the catalogues hold the real names
    "Show All",       // i18n-ignore: test data, the catalogues hold the real names
];

/// The *Window* menu's system items.
const SYSTEM_WINDOW_ITEMS: [&str; 4] = ["Minimize", "Zoom", "-", "Bring All to Front"]; // i18n-ignore: test data, the catalogues hold the real names

/// The items of submenu number `index` of `menu`.
fn items_of(menu: &SystemMenu, index: usize) -> &[SystemItem] {
    match menu.items.get(index) {
        Some(SystemItem::Submenu { items, .. }) => items,
        _ => &[],
    }
}

#[test]
fn the_items_with_a_role_go_into_the_application_menu_and_the_rest_is_tidied() {
    let id = ids(8);
    let menus = vec![
        submenu(
            "File",
            None,
            vec![
                action(id[0], "New", None),
                action(id[1], "Save", None),
                SystemItem::Separator,
                action(id[2], "Quit", Some(MenuRole::Quit)),
            ],
        ),
        submenu(
            "Edit",
            None,
            vec![
                action(id[3], "Undo", None),
                SystemItem::Separator,
                SystemItem::Separator,
                action(id[4], "Copy", None),
                SystemItem::Separator,
                action(id[5], "Preferences…", Some(MenuRole::Preferences)),
            ],
        ),
        submenu(
            "Help",
            Some(MenuRole::Help),
            vec![action(id[6], "Guide", None)],
        ),
        // A menu holding only a role's item disappears once it is taken.
        submenu(
            "Info",
            None,
            vec![
                SystemItem::Separator,
                action(id[7], "About", Some(MenuRole::About)),
            ],
        ),
    ];
    let menu = SystemMenu::for_macos(menus, "Orchestra", &texts());
    // A Window menu before Help.
    assert_eq!(
        names(&menu.items),
        ["Orchestra", "File", "Edit", "Window", "Help"]
    );
    // The application menu in Apple's order: About, Settings (Preferences renamed), Services,
    // the three Hide/Show items, Quit with the program's name; each group apart.
    assert_eq!(
        names(items_of(&menu, 0)),
        [
            "About",
            "-",
            "Settings…",
            "-",
            "Services",
            "-",
            "Hide Orchestra", // i18n-ignore: test data, the catalogues hold the real names
            "Hide Others",    // i18n-ignore: test data, the catalogues hold the real names
            "Show All",       // i18n-ignore: test data, the catalogues hold the real names
            "-",
            "Quit Orchestra" // i18n-ignore: test data, the catalogues hold the real names
        ]
    );
    assert_eq!(names(items_of(&menu, 3)), SYSTEM_WINDOW_ITEMS);
    // File lost Quit and its separator; Edit lost Preferences, its separator and a double one.
    assert_eq!(names(items_of(&menu, 1)), ["New", "Save"]);
    assert_eq!(names(items_of(&menu, 2)), ["Undo", "-", "Copy"]);
    // The Help menu keeps its role, for the system; the Window menu has its own.
    assert!(matches!(
        menu.items.get(3),
        Some(SystemItem::Submenu {
            role: Some(MenuRole::Window),
            ..
        })
    ));
    assert!(matches!(
        menu.items.get(4),
        Some(SystemItem::Submenu {
            role: Some(MenuRole::Help),
            ..
        })
    ));
    // Every action is still somewhere, once; the system's items are not actions.
    assert_eq!(menu.actions().count(), 8);
}

#[test]
fn without_roles_the_application_menu_has_the_system_items_and_window_comes_last() {
    let id = ids(1);
    let menu = SystemMenu::for_macos(
        vec![submenu("File", None, vec![action(id[0], "New", None)])],
        "Orchestra",
        &texts(),
    );
    // No Help menu: the Window menu goes last.
    assert_eq!(names(&menu.items), ["Orchestra", "File", "Window"]);
    assert_eq!(names(items_of(&menu, 0)), SYSTEM_APP_ITEMS);
    assert_eq!(names(items_of(&menu, 2)), SYSTEM_WINDOW_ITEMS);
}

#[test]
fn a_window_menu_of_the_model_gets_the_system_items_first() {
    let id = ids(1);
    let menu = SystemMenu::for_macos(
        vec![
            submenu(
                "Windows",
                Some(MenuRole::Window),
                vec![action(id[0], "Mixer", None)],
            ),
            submenu("Help", Some(MenuRole::Help), vec![]),
        ],
        "Orchestra",
        &texts(),
    );
    // No second Window menu; the empty Help menu was tidied away before.
    assert_eq!(names(&menu.items), ["Orchestra", "Windows"]);
    assert_eq!(
        names(items_of(&menu, 1)),
        ["Minimize", "Zoom", "-", "Bring All to Front", "-", "Mixer"] // i18n-ignore: test data, the catalogues hold the real names
    );
}

#[test]
fn the_system_items_say_what_the_system_does() {
    let menu = SystemMenu::for_macos(Vec::new(), "Orchestra", &texts());
    let commands = |index: usize| -> Vec<SystemCommand> {
        items_of(&menu, index)
            .iter()
            .filter_map(|item| match item {
                SystemItem::System { command, .. } => Some(*command),
                _ => None,
            })
            .collect()
    };
    assert_eq!(
        commands(0),
        [
            SystemCommand::Services,
            SystemCommand::Hide,
            SystemCommand::HideOthers,
            SystemCommand::ShowAll
        ]
    );
    assert_eq!(
        commands(1),
        [
            SystemCommand::Minimize,
            SystemCommand::Zoom,
            SystemCommand::BringAllToFront
        ]
    );
}

#[test]
fn shortcuts_become_macos_modifiers_and_logical_keys() {
    let shortcut = |modifiers: Modifiers, key: ShortcutKey| {
        SystemShortcut::from_shortcut(Shortcut::new(modifiers, key).unwrap())
    };
    let primary = Modifiers {
        primary: true,
        ..Modifiers::default()
    };
    // ⇧⌘S: *Primary* is ⌘, the letter stays in lower case (macOS adds ⇧ from the modifiers).
    let save_as = shortcut(
        Modifiers {
            shift: true,
            ..primary
        },
        ShortcutKey::Character('s'),
    )
    .unwrap();
    assert!(save_as.command && save_as.shift && !save_as.control && !save_as.option);
    assert_eq!(save_as.key, SystemKey::Character('s'));
    // ⌘? and ⌘,: the characters themselves, whatever the layout needs to type them.
    assert_eq!(
        shortcut(primary, ShortcutKey::Character('?')).unwrap().key,
        SystemKey::Character('?')
    );
    assert_eq!(
        shortcut(primary, ShortcutKey::Character(',')).unwrap().key,
        SystemKey::Character(',')
    );
    // ⌃⌘F: *Secondary* is ⌃.
    let full_screen = shortcut(
        Modifiers {
            secondary: true,
            ..primary
        },
        ShortcutKey::Character('f'),
    )
    .unwrap();
    assert!(full_screen.command && full_screen.control);
    // Digits, the keypad's included, are characters; Space too; F11 a named key.
    assert_eq!(
        shortcut(primary, ShortcutKey::Numpad(3)).unwrap().key,
        SystemKey::Character('3')
    );
    assert_eq!(
        shortcut(primary, ShortcutKey::Named(NamedKey::Space))
            .unwrap()
            .key,
        SystemKey::Character(' ')
    );
    assert_eq!(
        shortcut(
            Modifiers::default(),
            ShortcutKey::Named(NamedKey::Function(11))
        )
        .unwrap()
        .key,
        SystemKey::Named(NamedKey::Function(11))
    );
}
