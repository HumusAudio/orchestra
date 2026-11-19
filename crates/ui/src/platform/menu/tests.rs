// SPDX-License-Identifier: GPL-3.0-or-later

//! The description of the system's menu, on every platform (F2.11e1): the items with a role
//! moved into macOS's application menu, the menus they left tidied, and shortcuts turned into
//! macOS's modifiers and logical keys.

#![allow(
    clippy::unwrap_used,
    reason = "test helpers: a failed unwrap fails the test"
)]

use super::{SystemItem, SystemKey, SystemMenu, SystemShortcut};
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
            SystemItem::Action { name, .. } | SystemItem::Submenu { name, .. } => name.clone(),
            SystemItem::Separator => "-".to_owned(),
        })
        .collect()
}

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
    let menu = SystemMenu::for_macos(menus, "Orchestra");
    assert_eq!(names(&menu.items), ["Orchestra", "File", "Edit", "Help"]);
    // The application menu: About, then Preferences, then Quit, each group apart.
    assert_eq!(
        names(items_of(&menu, 0)),
        ["About", "-", "Preferences…", "-", "Quit"]
    );
    // File lost Quit and its separator; Edit lost Preferences, its separator and a double one.
    assert_eq!(names(items_of(&menu, 1)), ["New", "Save"]);
    assert_eq!(names(items_of(&menu, 2)), ["Undo", "-", "Copy"]);
    // The Help menu keeps its role, for the system.
    assert!(matches!(
        menu.items.get(3),
        Some(SystemItem::Submenu {
            role: Some(MenuRole::Help),
            ..
        })
    ));
    // Every action is still somewhere, once.
    assert_eq!(menu.actions().count(), 8);
}

#[test]
fn without_roles_there_is_still_an_application_menu() {
    let id = ids(1);
    let menu = SystemMenu::for_macos(
        vec![submenu("File", None, vec![action(id[0], "New", None)])],
        "Orchestra",
    );
    assert_eq!(names(&menu.items), ["Orchestra", "File"]);
    assert_eq!(items_of(&menu, 0).len(), 0);
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
