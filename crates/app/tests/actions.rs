// SPDX-License-Identifier: GPL-3.0-or-later

//! The program's actions (ADR-0048 AM6, AM10; F2.11d1): the registry builds on the three
//! platforms with the defaults of AM6 and the initial states of choice E, every action's name is
//! in the six catalogues, and in the demonstration window Quit and Full screen reach the window
//! while the other actions, disabled, do nothing.

#![allow(
    clippy::unwrap_used,
    reason = "test helpers: a failed unwrap fails the test"
)]

use orchestra_app::{Demo, SceneContext, registry};
use orchestra_i18n::{Locale, Localizer};
use orchestra_ui::{
    ButtonState, Context, DisplayList, InputEvent, Key, Modifiers, NamedKey, PhysicalKey, Platform,
    Rect, Shortcut, ShortcutKey, WindowApp, WindowRequest,
};

/// *Primary*.
const PRIMARY: Modifiers = Modifiers {
    primary: true,
    secondary: false,
    shift: false,
    alt: false,
};

/// The shortcut of a character key.
fn ch(modifiers: Modifiers, character: char) -> Shortcut {
    Shortcut::new(modifiers, ShortcutKey::Character(character)).unwrap()
}

#[test]
fn the_registry_builds_on_every_platform_with_its_defaults() {
    for platform in [Platform::Linux, Platform::Windows, Platform::MacOs] {
        let (registry, ids) = registry(platform).unwrap();
        // Ctrl+Z (⌘Z) undoes everywhere; Ctrl+Y redoes on Windows only.
        assert_eq!(
            registry.lookup(Context::GLOBAL, ch(PRIMARY, 'z')),
            Some(ids.undo)
        );
        assert_eq!(
            registry.lookup(Context::GLOBAL, ch(PRIMARY, 'y')),
            (platform == Platform::Windows).then_some(ids.redo)
        );
        // Quit has no shortcut of ours on Windows (Alt+F4 is the window's close request).
        assert_eq!(
            registry.shortcuts(ids.quit).is_empty(),
            platform == Platform::Windows
        );
        let f11 = Shortcut::new(
            Modifiers::default(),
            ShortcutKey::Named(NamedKey::Function(11)),
        )
        .unwrap();
        assert_eq!(
            registry.lookup(Context::GLOBAL, f11),
            (platform != Platform::MacOs).then_some(ids.full_screen)
        );
        // Choice E: only Quit and Full screen are enabled outside a field.
        for id in ids.all() {
            let enabled = registry.state(id).unwrap().enabled;
            assert_eq!(enabled, id == ids.quit || id == ids.full_screen, "{id:?}");
        }
    }
}

#[test]
fn every_action_has_its_name_in_the_six_languages() {
    let (registry, ids) = registry(Platform::Linux).unwrap();
    for locale in Locale::ALL {
        let localizer = Localizer::new(locale).unwrap();
        for id in ids.all() {
            let key = registry.spec(id).unwrap().name;
            let name = localizer.format(key, None).unwrap();
            assert!(!name.is_empty(), "{key} in {locale:?}");
        }
    }
}

/// The demonstration, laid out.
fn demo() -> (Demo, DisplayList) {
    let context = SceneContext::new().unwrap();
    let mut demo = Demo::new(
        &context.faces,
        &orchestra_app::theme(),
        "Orchestra",
        &context.demo,
    );
    let mut list = DisplayList::new();
    demo.layout(Rect::from_origin_size(0.0, 0.0, 640.0, 370.0), &mut list);
    (demo, list)
}

/// Presses and releases `key` with `modifiers`.
fn press(demo: &mut Demo, list: &mut DisplayList, key: &Key, modifiers: Modifiers) {
    for state in [ButtonState::Pressed, ButtonState::Released] {
        demo.input(
            &InputEvent::Key {
                key: key.clone(),
                physical: PhysicalKey::Other,
                state,
                repeat: false,
                modifiers,
            },
            list,
        );
    }
}

#[test]
fn full_screen_and_quit_reach_the_window() {
    let (mut demo, mut list) = demo();
    assert_eq!(demo.take_window_request(), None);
    // Full screen: F11, or ⌃⌘F on macOS (the platform this test runs on).
    if Platform::CURRENT == Platform::MacOs {
        let modifiers = Modifiers {
            secondary: true,
            ..PRIMARY
        };
        press(&mut demo, &mut list, &Key::Character("f".into()), modifiers);
    } else {
        press(
            &mut demo,
            &mut list,
            &Key::Named(NamedKey::Function(11)),
            Modifiers::default(),
        );
    }
    assert_eq!(
        demo.take_window_request(),
        Some(WindowRequest::ToggleFullScreen)
    );
    // Quit: Ctrl+Q (⌘Q); on Windows the system's Alt+F4 closes the window instead.
    press(&mut demo, &mut list, &Key::Character("q".into()), PRIMARY);
    let expected = (Platform::CURRENT != Platform::Windows).then_some(WindowRequest::Close);
    assert_eq!(demo.take_window_request(), expected);
    assert_eq!(demo.take_window_request(), None);
}

#[test]
fn disabled_actions_do_nothing() {
    let (mut demo, mut list) = demo();
    for key in ['s', 'o', 'n', 'z'] {
        press(
            &mut demo,
            &mut list,
            &Key::Character(key.to_string()),
            PRIMARY,
        );
    }
    assert_eq!(demo.take_window_request(), None);
    assert!(demo.actions().is_some());
}
