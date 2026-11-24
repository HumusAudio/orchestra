// SPDX-License-Identifier: GPL-3.0-or-later

//! The program's actions (ADR-0048 AM6, AM10; F2.11d1): the registry builds on the three
//! platforms with the defaults of AM6 and the initial states of choice E, every action's name is
//! in the six catalogues, and in the demonstration window Quit and Full screen reach the window
//! while the other actions, disabled, do nothing. Since F2.11d2a: the shortcuts written in each
//! language and platform's style, the ellipsis of the items that open a window, and the track
//! name field's context menu. Since F2.11e2: the names of macOS's own menu items in the six
//! catalogues, with the program's name and without Fluent's isolation marks; ⌘H, ⌥⌘H and ⌘M given
//! to no action on macOS.

#![allow(
    clippy::unwrap_used,
    reason = "test helpers: a failed unwrap fails the test"
)]

use orchestra_app::{ActionIds, Demo, SceneContext, action_texts, registry, system_texts};
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
        // Choice E: only Quit, Full screen, About (F2.12a3b) and Preferences (F2.12a4a) are
        // enabled outside a field.
        for id in ids.all() {
            let enabled = registry.state(id).unwrap().enabled;
            let expected =
                id == ids.quit || id == ids.full_screen || id == ids.about || id == ids.preferences;
            assert_eq!(enabled, expected, "{id:?}");
        }
        // About has no shortcut on any platform (ADR-0050 DG10).
        assert_eq!(registry.shortcuts(ids.about), []);
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

/// *About Orchestra* opens its dialog (F2.12a3b): an information dialog with the logo, the
/// version and the repository's address, the focus on *Close*; Escape closes it.
#[test]
fn about_opens_its_dialog_and_escape_closes_it() {
    let (mut demo, mut list) = demo();
    let ids = demo.actions().unwrap();
    demo.run_action(ids.about, &mut list);
    let dialogs: Vec<_> = demo.ui().dialogs().collect();
    assert_eq!(dialogs.len(), 1);
    // The dialog's texts are the catalogue's, with the version and the address filled in.
    let texts = &SceneContext::new().unwrap().demo.about;
    assert_eq!(texts.title, "About Orchestra");
    assert_eq!(texts.version, format!("Version {}", orchestra_app::VERSION));
    assert!(texts.source.ends_with(orchestra_app::REPOSITORY));
    assert!(!texts.source.contains(['\u{2068}', '\u{2069}']));
    press(
        &mut demo,
        &mut list,
        &Key::Named(NamedKey::Escape),
        Modifiers::default(),
    );
    assert_eq!(demo.ui().dialogs().count(), 0);
    assert_eq!(demo.take_window_request(), None);
}

/// The Preferences (F2.12a4a, ADR-0050 DG8–DG9): Ctrl+, opens them; a mode chosen applies at
/// once (a request to the window) and goes into the settings, written later; the blink turned
/// off reaches the interface; Ctrl+, again closes them.
#[test]
fn the_preferences_apply_at_once_and_their_shortcut_closes_them() {
    let (mut demo, mut list) = demo();
    let ids = demo.actions().unwrap();
    demo.run_action(ids.preferences, &mut list);
    let panel = demo.preferences().expect("the Preferences are open");
    assert_eq!(demo.ui().dialogs().collect::<Vec<_>>(), vec![panel]);
    // The first control has the focus: the list of sections.
    assert!(demo.ui().focused().is_some());
    // Tab to the mode's group (sections → theme → mode), End chooses Dark.
    for key in [NamedKey::Tab, NamedKey::Tab, NamedKey::End] {
        press(&mut demo, &mut list, &Key::Named(key), Modifiers::default());
    }
    match demo.take_window_request() {
        Some(WindowRequest::SetTheme { mode, .. }) => {
            assert_eq!(mode, orchestra_ui::ThemeMode::Dark);
        }
        other => panic!("expected a theme request, got {other:?}"),
    }
    assert_eq!(
        demo.settings().settings().theme.mode,
        orchestra_io::settings::Mode::Dark
    );
    // Tab to the blinking cursor's check box, Space turns it off.
    for key in [NamedKey::Tab, NamedKey::Space] {
        press(&mut demo, &mut list, &Key::Named(key), Modifiers::default());
    }
    assert!(!demo.ui().caret_blink());
    assert!(!demo.settings().settings().caret_blink);
    // Ctrl+, (⌘, on macOS) closes them instead of opening them again.
    press(&mut demo, &mut list, &Key::Character(",".into()), PRIMARY);
    assert_eq!(demo.ui().dialogs().count(), 0);
    assert_eq!(demo.preferences(), None);
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

/// The registry of `platform` with the texts of `locale` (F2.11d2a).
fn named_registry(platform: Platform, locale: Locale) -> (orchestra_ui::Registry, ActionIds) {
    let (mut registry, ids) = registry(platform).unwrap();
    let localizer = Localizer::new(locale).unwrap();
    action_texts(&localizer).unwrap().apply(&mut registry);
    (registry, ids)
}

#[test]
fn shortcuts_are_written_in_each_language_and_platform_style() {
    // Choice S1 of F2.11d2a, with the catalogues' key names (glossary entries key_*).
    let cases = [
        (Locale::EnUs, Platform::Linux, "Ctrl+Shift+S", "F11"),
        (Locale::EnGb, Platform::Windows, "Ctrl+Shift+S", "F11"),
        (Locale::It, Platform::Linux, "Ctrl+Maiusc+S", "F11"),
        (Locale::Fr, Platform::Windows, "Ctrl+Maj+S", "F11"),
        (Locale::De, Platform::Linux, "Strg+Umschalt+S", "F11"),
        (Locale::Es, Platform::Windows, "Ctrl+Mayús+S", "F11"),
        // macOS: Apple's symbols, the same in every language.
        (
            Locale::De,
            Platform::MacOs,
            "\u{21E7}\u{2318}S",
            "\u{2303}\u{2318}F",
        ),
    ];
    for (locale, platform, save_as, full_screen) in cases {
        let (registry, ids) = named_registry(platform, locale);
        assert_eq!(
            registry.shortcut_text(ids.save_as),
            save_as,
            "{locale:?} {platform:?}"
        );
        assert_eq!(
            registry.shortcut_text(ids.full_screen),
            full_screen,
            "{locale:?} {platform:?}"
        );
        // No isolation marks from Fluent's arguments in a shortcut's text.
        assert!(
            !full_screen.contains(['\u{2068}', '\u{2069}'])
                && !registry
                    .shortcut_text(ids.full_screen)
                    .contains(['\u{2068}', '\u{2069}'])
        );
    }
    // Windows: Quit has no shortcut of ours, so no text.
    let (registry, ids) = named_registry(Platform::Windows, Locale::EnUs);
    assert_eq!(registry.shortcut_text(ids.quit), "");
}

#[test]
fn the_items_that_open_a_window_end_with_an_ellipsis() {
    // Choice S2 of F2.11d2a: Open…, Save as…, Preferences…; New, Save, Quit and the guide don't.
    for locale in Locale::ALL {
        let (registry, ids) = named_registry(Platform::Linux, locale);
        for id in [ids.open, ids.save_as, ids.preferences] {
            let name = registry.name_text(id);
            assert!(name.ends_with('\u{2026}'), "{name:?} in {locale:?}");
            assert!(!name.ends_with(" \u{2026}"), "no space before it: {name:?}");
        }
        for id in [ids.new, ids.save, ids.quit, ids.help, ids.undo] {
            assert!(!registry.name_text(id).ends_with('\u{2026}'));
        }
    }
}

#[test]
fn the_track_name_field_has_the_editing_context_menu() {
    let (mut demo, mut list) = demo();
    let field = demo.track_name();
    demo.ui_mut().set_focus(Some(field), &mut list).unwrap();
    // The Menu key opens it, with the editing actions named from the catalogue.
    press(
        &mut demo,
        &mut list,
        &Key::Named(NamedKey::ContextMenu),
        Modifiers::default(),
    );
    let menus: Vec<_> = demo.ui().menus().collect();
    assert_eq!(menus.len(), 1);
    let items: Vec<_> = demo.ui().menu_items(menus[0]).collect();
    // Undo, Redo, a separator, Cut, Copy, Paste, a separator, Select all.
    assert_eq!(items.len(), 8);
    // The field is empty: only Paste and Select all can do something, Paste comes first.
    let ids = demo.actions().unwrap();
    assert_eq!(demo.ui().highlighted_menu_item(), Some(items[5]));
    assert!(demo.ui().action_state(ids.paste).unwrap().enabled);
    press(
        &mut demo,
        &mut list,
        &Key::Named(NamedKey::Escape),
        Modifiers::default(),
    );
    assert_eq!(demo.ui().menus().count(), 0);
    assert_eq!(demo.ui().focused(), Some(field));
}

/// The siblings of a menu: each item's name and mnemonic, from the registry for actions and from
/// the model for submenus.
fn siblings(
    registry: &orchestra_ui::Registry,
    model: &orchestra_ui::MenuModel,
) -> Vec<(String, Option<char>)> {
    model
        .items
        .iter()
        .filter_map(|item| match item {
            orchestra_ui::MenuItem::Action { action, .. } => Some((
                registry.name_text(*action).to_owned(),
                registry.mnemonic(*action),
            )),
            orchestra_ui::MenuItem::Submenu { name, mnemonic, .. } => {
                Some((name.clone(), *mnemonic))
            }
            orchestra_ui::MenuItem::Separator | orchestra_ui::MenuItem::Choice { .. } => None,
        })
        .collect()
}

/// Every item has a mnemonic that is a letter of its name, and no two siblings share one.
fn check_siblings(items: &[(String, Option<char>)], locale: Locale) {
    let mut seen = Vec::new();
    for (name, mnemonic) in items {
        assert!(mnemonic.is_some(), "{name:?} in {locale:?}: no mnemonic");
        let letter = mnemonic.unwrap();
        assert!(
            name.to_lowercase()
                .contains(&letter.to_lowercase().to_string()),
            "{locale:?}: {letter:?} is not a letter of {name:?}"
        );
        assert!(
            letter.is_alphanumeric(),
            "{locale:?}: {letter:?} of {name:?} is not a letter"
        );
        let lower = letter.to_lowercase().to_string();
        assert!(
            !seen.contains(&lower),
            "{locale:?}: {letter:?} of {name:?} is taken by a sibling"
        );
        seen.push(lower);
    }
}

#[test]
fn every_mnemonic_is_a_letter_of_its_name_and_unique_among_its_siblings() {
    // ADR-0048 AM10, in the six catalogues (F2.11d2b): the bar's titles, each menu of the bar,
    // and the text field's context menu.
    for locale in Locale::ALL {
        let (registry, ids) = named_registry(Platform::Linux, locale);
        let texts = action_texts(&Localizer::new(locale).unwrap()).unwrap();
        let bar = orchestra_app::menu_bar(&ids, &texts);
        check_siblings(&siblings(&registry, &bar), locale);
        for item in &bar.items {
            if let orchestra_ui::MenuItem::Submenu { items, .. } = item {
                check_siblings(&siblings(&registry, items), locale);
            }
        }
        check_siblings(
            &siblings(&registry, &orchestra_app::field_menu(&ids)),
            locale,
        );
    }
}

#[test]
fn the_demonstrations_menu_bar_opens_from_the_keyboard() {
    let (mut demo, mut list) = demo();
    if Platform::CURRENT == Platform::MacOs {
        // The system's bar comes with F2.11e: no drawn bar on macOS.
        assert_eq!(demo.menu_bar(), None);
        return;
    }
    assert!(demo.menu_bar().is_some());
    // F10, then Enter: File's menu, New highlighted (disabled, so the first enabled: Quit).
    press(
        &mut demo,
        &mut list,
        &Key::Named(NamedKey::Function(10)),
        Modifiers::default(),
    );
    assert_eq!(demo.ui().menu_bar_active(), Some(0));
    press(
        &mut demo,
        &mut list,
        &Key::Named(NamedKey::Enter),
        Modifiers::default(),
    );
    let menus: Vec<_> = demo.ui().menus().collect();
    assert_eq!(menus.len(), 1);
    // New, Open…, Save, Save as…, a separator, Quit.
    let items: Vec<_> = demo.ui().menu_items(menus[0]).collect();
    assert_eq!(items.len(), 6);
    assert_eq!(demo.ui().highlighted_menu_item(), Some(items[5]));
    // Enter quits (on Linux; on Windows Quit has no shortcut but still works from the menu).
    press(
        &mut demo,
        &mut list,
        &Key::Named(NamedKey::Enter),
        Modifiers::default(),
    );
    assert_eq!(demo.take_window_request(), Some(WindowRequest::Close));
}

#[test]
fn macos_names_its_own_items_in_every_language_with_the_programs_name() {
    for locale in Locale::ALL {
        let localizer = Localizer::new(locale).unwrap();
        let app = localizer.format("app-name", None).unwrap();
        let texts = system_texts(&localizer).unwrap();
        let all = [
            &texts.settings,
            &texts.services,
            &texts.hide,
            &texts.hide_others,
            &texts.show_all,
            &texts.quit,
            &texts.window,
            &texts.minimize,
            &texts.zoom,
            &texts.bring_all_to_front,
        ];
        for name in all {
            assert!(!name.is_empty(), "{locale:?}: an empty name");
            // The marks Fluent puts around { app-name } are dropped (see `system_texts`).
            assert!(
                !name.contains(['\u{2068}', '\u{2069}']),
                "{locale:?}: {name:?} keeps an isolation mark"
            );
        }
        // Hide and Quit name the program, wherever the language puts it.
        assert!(
            texts.hide.contains(app.as_str()),
            "{locale:?}: {:?}",
            texts.hide
        );
        assert!(
            texts.quit.contains(app.as_str()),
            "{locale:?}: {:?}",
            texts.quit
        );
        // Settings opens a window: it ends with the ellipsis, like Preferences (choice S2).
        assert!(
            texts.settings.ends_with('…'),
            "{locale:?}: {:?}",
            texts.settings
        );
    }
}

#[test]
fn no_action_takes_a_shortcut_of_macos_own_items() {
    let (registry, _) = registry(Platform::MacOs).unwrap();
    let alt_primary = Modifiers {
        alt: true,
        ..PRIMARY
    };
    // ⌘H, ⌥⌘H and ⌘M are the system's (F2.11e2): the defaults leave them free, and the
    // registry refuses them.
    for shortcut in [ch(PRIMARY, 'h'), ch(alt_primary, 'h'), ch(PRIMARY, 'm')] {
        assert!(shortcut.is_fixed(Platform::MacOs), "{shortcut:?}");
        assert!(
            registry.lookup(Context::GLOBAL, shortcut).is_none(),
            "{shortcut:?} is an action's"
        );
    }
}
