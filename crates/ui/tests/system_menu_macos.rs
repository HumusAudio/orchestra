// SPDX-License-Identifier: GPL-3.0-or-later

//! The system's menu bar built for real with `muda` (F2.11e1, ADR-0048 AM15: "on macOS `muda` is
//! compiled and its menu built in CI"). `AppKit`'s menus must be made on the main thread, which the
//! test harness does not give: this file has its own `main` (`harness = false` in the manifest)
//! and runs there. On Linux and Windows it lists no test: the system's menu is macOS's.
//!
//! # Speaking `nextest`'s language
//! `cargo nextest` (what `cargo xtask check` runs) first asks every test binary for its tests
//! with `--list --format terse` and then runs each one in a process of its own, by name. So this
//! `main` answers the listing with its one test (none for `--ignored`, and none off macOS), and
//! otherwise runs it: on the process's main thread, which is exactly what `AppKit` needs.

#![allow(
    clippy::unwrap_used,
    clippy::print_stdout,
    reason = "a test binary: a failed unwrap fails it, and it reports what it built"
)]

/// The one test's name, as `nextest` lists and runs it.
const TEST: &str = "system_menu_built_by_muda";

/// Answers `nextest`'s listing, or runs the test (see the module documentation).
fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|arg| arg == "--list") {
        // Only on macOS, and never among the ignored tests.
        if cfg!(target_os = "macos") && !args.iter().any(|arg| arg == "--ignored") {
            println!("{TEST}: test");
        }
        return;
    }
    // Run when asked by name (or with no filter at all, as `cargo test` does).
    let filtered = args.iter().skip(1).any(|arg| !arg.starts_with('-'));
    if !filtered || args.iter().any(|arg| arg == TEST) {
        run();
    }
}

/// Builds a description with every kind of item and shortcut, and the menu from it with `muda`.
#[cfg(target_os = "macos")]
fn run() {
    use orchestra_ui::{
        ActionSpec, ActionState, Context, Defaults, MenuRole, Modifiers, Platform, Registry,
        Shortcut, ShortcutKey, SystemItem, SystemMenu,
    };
    // The description by hand, every kind of item and shortcut the program uses: a toggle, a
    // disabled item, a character key with ⇧, "?" and ",", F11, a nested submenu, separators.
    let mut registry = Registry::new(Platform::MacOs);
    let primary = Modifiers {
        primary: true,
        ..Modifiers::default()
    };
    let shortcut = |modifiers: Modifiers, key: ShortcutKey| Shortcut::new(modifiers, key).unwrap();
    let mut ids = Vec::new();
    for (index, key) in [
        ShortcutKey::Character('s'),
        ShortcutKey::Character('?'),
        ShortcutKey::Character(','),
        ShortcutKey::Named(orchestra_ui::NamedKey::Function(11)),
    ]
    .into_iter()
    .enumerate()
    {
        let identifier: &'static str = Box::leak(format!("test.item-{index}").into_boxed_str());
        ids.push(
            registry
                .register(ActionSpec {
                    identifier,
                    name: identifier,
                    context: Context::GLOBAL,
                    defaults: Defaults::everywhere(&[shortcut(primary, key)]),
                    repeatable: false,
                    edit: None,
                })
                .unwrap(),
        );
    }
    let action = |index: usize, state: ActionState, role: Option<MenuRole>| SystemItem::Action {
        action: ids[index],
        name: format!("Item & {index}"),
        shortcut: registry.shortcuts(ids[index]).first().copied(),
        state,
        role,
    };
    let toggle = ActionState {
        enabled: true,
        checked: Some(true),
    };
    let menus = vec![
        SystemItem::Submenu {
            name: "File".to_owned(),
            role: None,
            items: vec![
                action(0, ActionState::ENABLED, None),
                SystemItem::Separator,
                action(1, ActionState::DISABLED, None),
                SystemItem::Submenu {
                    name: "More".to_owned(),
                    role: None,
                    items: vec![action(3, toggle, None)],
                },
            ],
        },
        SystemItem::Submenu {
            name: "Help".to_owned(),
            role: Some(MenuRole::Help),
            items: vec![action(2, ActionState::ENABLED, Some(MenuRole::Preferences))],
        },
    ];
    let menu = SystemMenu::for_macos(menus, "Orchestra");
    let built = orchestra_ui::build_system_menu(&menu).unwrap();
    assert_eq!(built, 4, "every action item was built");
    println!("system menu built with muda: {built} action items");
}

/// Nothing off macOS.
#[cfg(not(target_os = "macos"))]
fn run() {
    // The system's menu is macOS's: nothing to build here.
}
