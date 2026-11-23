// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of the shortcuts and the registry (ADR-0048 AM5–AM8, F2.11d1), without a display: the
//! normalisation of shortcuts, the candidates of a key press with the two fallbacks by position
//! (made-up layouts: French AZERTY, Russian, German QWERTZ), the fixed keys of the three
//! platforms, registration, the user's shortcuts and the loading rules of AM7.

use super::registry::{
    ActionId, ActionSpec, ActionState, Context, Defaults, LoadWarning, Registry, RegistryError,
};
use super::shortcut::{Platform, Shortcut, ShortcutError, ShortcutKey, candidates};
use crate::window::input::{Key, Modifiers, NamedKey, PhysicalKey};

/// No modifier.
const NONE: Modifiers = Modifiers {
    primary: false,
    secondary: false,
    shift: false,
    alt: false,
};

/// The primary modifier alone.
const PRIMARY: Modifiers = Modifiers {
    primary: true,
    ..NONE
};

/// Primary and Shift.
const PRIMARY_SHIFT: Modifiers = Modifiers {
    shift: true,
    ..PRIMARY
};

/// Shift alone.
const SHIFT: Modifiers = Modifiers {
    shift: true,
    ..NONE
};

/// Alt alone.
const ALT: Modifiers = Modifiers { alt: true, ..NONE };

/// A shortcut of a character key; the tests' characters are all valid keys.
fn ch(modifiers: Modifiers, character: char) -> Shortcut {
    Shortcut::new(modifiers, ShortcutKey::Character(character))
        .unwrap_or_else(|error| panic!("{character:?}: {error}"))
}

/// A shortcut of a named key.
fn named(modifiers: Modifiers, key: NamedKey) -> Shortcut {
    Shortcut::new(modifiers, ShortcutKey::Named(key)).unwrap_or_else(|error| panic!("{error}"))
}

/// A shortcut of the digit row.
fn digit(modifiers: Modifiers, value: u8) -> Shortcut {
    Shortcut::new(modifiers, ShortcutKey::Digit(value)).unwrap_or_else(|error| panic!("{error}"))
}

/// The candidates of a press of a character key.
fn press(text: &str, physical: PhysicalKey, modifiers: Modifiers) -> [Option<Shortcut>; 2] {
    candidates(&Key::Character(text.into()), physical, modifiers)
}

// ----- Shortcuts -------------------------------------------------------------------------------

#[test]
fn letters_lose_their_case_and_keep_shift() {
    // Ctrl+Shift+Z is the same shortcut whether the layout reported "Z" or "z".
    assert_eq!(ch(PRIMARY_SHIFT, 'Z'), ch(PRIMARY_SHIFT, 'z'));
    assert_eq!(ch(PRIMARY_SHIFT, 'Z').key(), ShortcutKey::Character('z'));
    assert!(ch(PRIMARY_SHIFT, 'Z').modifiers().shift);
    // Shift is a modifier of its own for letters: Ctrl+Z and Ctrl+Shift+Z differ.
    assert_ne!(ch(PRIMARY, 'z'), ch(PRIMARY_SHIFT, 'z'));
    // Any bicameral script: a Cyrillic letter in lower case too.
    assert_eq!(ch(PRIMARY, 'Я').key(), ShortcutKey::Character('я'));
}

#[test]
fn other_characters_drop_shift() {
    // ⌘? is one shortcut whether "?" needed Shift (US) or not (precision A of F2.11d1).
    assert_eq!(ch(PRIMARY_SHIFT, '?'), ch(PRIMARY, '?'));
    assert!(!ch(PRIMARY_SHIFT, '?').modifiers().shift);
    assert_eq!(ch(SHIFT, ',').modifiers(), NONE);
}

#[test]
fn ascii_digits_become_the_digit_row() {
    assert_eq!(ch(PRIMARY, '1'), digit(PRIMARY, 1));
    // Shift stays with a digit, as with a letter.
    assert!(ch(PRIMARY_SHIFT, '1').modifiers().shift);
}

#[test]
fn malformed_keys_are_refused() {
    assert_eq!(
        Shortcut::new(NONE, ShortcutKey::Digit(10)),
        Err(ShortcutError::DigitOutOfRange(10))
    );
    assert_eq!(
        Shortcut::new(NONE, ShortcutKey::Numpad(12)),
        Err(ShortcutError::DigitOutOfRange(12))
    );
    assert_eq!(
        Shortcut::new(NONE, ShortcutKey::Character(' ')),
        Err(ShortcutError::NotAKey(' '))
    );
    assert_eq!(
        Shortcut::new(NONE, ShortcutKey::Character('\u{1}')),
        Err(ShortcutError::NotAKey('\u{1}'))
    );
}

// ----- Candidates of a key press (AM5) ---------------------------------------------------------

#[test]
fn a_latin_letter_has_no_fallback() {
    // US and French AZERTY alike: the logical letter, wherever the layout puts it.
    assert_eq!(
        press("z", PhysicalKey::Letter(25), PRIMARY),
        [Some(ch(PRIMARY, 'z')), None]
    );
    // AZERTY's "a" is on the US Q key: still the letter a.
    assert_eq!(
        press("a", PhysicalKey::Letter(16), PRIMARY),
        [Some(ch(PRIMARY, 'a')), None]
    );
    // Shift with a letter: the layout reports the capital, the shortcut has Shift.
    assert_eq!(
        press("Z", PhysicalKey::Letter(25), PRIMARY_SHIFT),
        [Some(ch(PRIMARY_SHIFT, 'z')), None]
    );
}

#[test]
fn a_non_latin_letter_falls_back_to_the_us_letter_at_its_place() {
    // Russian layout: the key of the US Z types "я". Ctrl+Z must still undo (AM5 (i)).
    assert_eq!(
        press("я", PhysicalKey::Letter(25), PRIMARY),
        [Some(ch(PRIMARY, 'я')), Some(ch(PRIMARY, 'z'))]
    );
    // Greek, with Shift: the fallback keeps the modifiers.
    assert_eq!(
        press("Ζ", PhysicalKey::Letter(25), PRIMARY_SHIFT),
        [Some(ch(PRIMARY_SHIFT, 'ζ')), Some(ch(PRIMARY_SHIFT, 'z'))]
    );
}

#[test]
fn the_digit_row_counts_by_position() {
    // French AZERTY: a plain press of the key that types "&" is also the digit 1 (AM5 (ii)).
    assert_eq!(
        press("&", PhysicalKey::Digit(1), NONE),
        [Some(ch(NONE, '&')), Some(digit(NONE, 1))]
    );
    // Its "é" (a Latin letter) on the key of 2 is the digit 2 as well.
    assert_eq!(
        press("é", PhysicalKey::Digit(2), NONE),
        [Some(ch(NONE, 'é')), Some(digit(NONE, 2))]
    );
    // With Shift AZERTY types the digit itself: the digit with Shift, as on QWERTY.
    assert_eq!(
        press("1", PhysicalKey::Digit(1), SHIFT),
        [Some(digit(SHIFT, 1)), None]
    );
    // QWERTY: Shift+1 types "!", which is also Shift+1 by position.
    assert_eq!(
        press("!", PhysicalKey::Digit(1), PRIMARY_SHIFT),
        [Some(ch(PRIMARY, '!')), Some(digit(PRIMARY_SHIFT, 1))]
    );
}

#[test]
fn the_logical_shortcut_comes_before_the_position() {
    // German QWERTZ: "/" is Shift+7. A user's Primary+/ is tried first; only then Shift+7.
    let [first, second] = press("/", PhysicalKey::Digit(7), PRIMARY_SHIFT);
    assert_eq!(first, Some(ch(PRIMARY, '/')));
    assert_eq!(second, Some(digit(PRIMARY_SHIFT, 7)));
}

#[test]
fn the_keypad_is_apart_from_the_digit_row() {
    assert_eq!(
        press("1", PhysicalKey::Numpad(1), NONE),
        [Shortcut::new(NONE, ShortcutKey::Numpad(1)).ok(), None]
    );
    // Num Lock off: the keypad's 8 is an arrow, and that is what counts.
    assert_eq!(
        candidates(&Key::Named(NamedKey::ArrowUp), PhysicalKey::Numpad(8), NONE),
        [Some(named(NONE, NamedKey::ArrowUp)), None]
    );
}

#[test]
fn unknown_keys_and_strings_have_no_logical_shortcut() {
    assert_eq!(
        candidates(&Key::Unknown, PhysicalKey::Other, PRIMARY),
        [None, None]
    );
    // A key that types two characters is no shortcut's key, but its position still counts.
    assert_eq!(
        press("ab", PhysicalKey::Digit(3), NONE),
        [None, Some(digit(NONE, 3))]
    );
}

// ----- Fixed keys (AM8) ------------------------------------------------------------------------

#[test]
fn navigation_keys_are_fixed_without_modifiers_or_with_shift() {
    for key in [
        NamedKey::Tab,
        NamedKey::ArrowUp,
        NamedKey::ArrowDown,
        NamedKey::ArrowLeft,
        NamedKey::ArrowRight,
        NamedKey::Enter,
        NamedKey::Escape,
        NamedKey::Space,
        NamedKey::Function(10),
    ] {
        for platform in [Platform::Linux, Platform::Windows, Platform::MacOs] {
            assert!(named(NONE, key).is_fixed(platform), "{key:?}");
            assert!(named(SHIFT, key).is_fixed(platform), "Shift+{key:?}");
            // With the primary modifier they are free for actions (Ctrl+↑ in the score).
            assert!(!named(PRIMARY, key).is_fixed(platform), "Primary+{key:?}");
        }
    }
    // The Menu key, whatever is held.
    assert!(named(PRIMARY, NamedKey::ContextMenu).is_fixed(Platform::Linux));
    // F11 and F1 are ordinary keys.
    assert!(!named(NONE, NamedKey::Function(11)).is_fixed(Platform::Linux));
}

#[test]
fn alt_and_a_letter_are_the_mnemonics_except_on_macos() {
    assert!(ch(ALT, 'f').is_fixed(Platform::Linux));
    assert!(ch(ALT, 'f').is_fixed(Platform::Windows));
    // macOS has no mnemonics (AM10).
    assert!(!ch(ALT, 'f').is_fixed(Platform::MacOs));
    // With the primary modifier, or on a non-letter, Alt is a modifier like any other.
    let primary_alt = Modifiers {
        alt: true,
        ..PRIMARY
    };
    assert!(!ch(primary_alt, 'f').is_fixed(Platform::Linux));
    assert!(!ch(ALT, ',').is_fixed(Platform::Linux));
}

#[test]
fn the_system_menus_shortcuts_are_fixed_on_macos_only() {
    let primary_alt = Modifiers {
        alt: true,
        ..PRIMARY
    };
    // ⌘H, ⌥⌘H and ⌘M belong to macOS's own menu items (F2.11e2).
    assert!(ch(PRIMARY, 'h').is_fixed(Platform::MacOs));
    assert!(ch(primary_alt, 'h').is_fixed(Platform::MacOs));
    assert!(ch(PRIMARY, 'm').is_fixed(Platform::MacOs));
    // ⌥⌘M, ⇧⌘H and ⌃⌘H are free; so is Ctrl+H elsewhere (Replace in many programs).
    assert!(!ch(primary_alt, 'm').is_fixed(Platform::MacOs));
    assert!(!ch(PRIMARY_SHIFT, 'h').is_fixed(Platform::MacOs));
    let primary_secondary = Modifiers {
        secondary: true,
        ..PRIMARY
    };
    assert!(!ch(primary_secondary, 'h').is_fixed(Platform::MacOs));
    assert!(!ch(PRIMARY, 'h').is_fixed(Platform::Linux));
    assert!(!ch(PRIMARY, 'm').is_fixed(Platform::Windows));
}

// ----- The registry ----------------------------------------------------------------------------

/// An action of the global context with the same defaults everywhere.
fn spec(identifier: &'static str, shortcuts: &[Shortcut]) -> ActionSpec {
    ActionSpec {
        identifier,
        name: "action-test",
        context: Context::GLOBAL,
        defaults: Defaults::everywhere(shortcuts),
        repeatable: false,
        edit: None,
    }
}

/// Registers `spec`, which the test knows to be valid.
fn add(registry: &mut Registry, spec: ActionSpec) -> ActionId {
    registry
        .register(spec)
        .unwrap_or_else(|error| panic!("{error}"))
}

#[test]
fn identifiers_are_checked() {
    let mut registry = Registry::new(Platform::Linux);
    for bad in [
        "undo",
        "Edit.undo",
        "edit.",
        ".undo",
        "edit.un do",
        "edit.undo.x",
        "1.a",
    ] {
        assert_eq!(
            registry.register(spec(bad, &[])),
            Err(RegistryError::MalformedIdentifier(bad)),
            "{bad}"
        );
    }
    let undo = add(&mut registry, spec("edit.undo", &[]));
    assert_eq!(registry.find("edit.undo"), Some(undo));
    assert_eq!(registry.find("view.full-screen"), None);
    assert_eq!(
        registry.register(spec("edit.undo", &[])),
        Err(RegistryError::DuplicateIdentifier("edit.undo"))
    );
    // Hyphens and digits after the first letter are fine.
    add(&mut registry, spec("view.zoom-2x", &[]));
}

#[test]
fn defaults_follow_the_platform() {
    let defaults = Defaults {
        linux: vec![ch(PRIMARY_SHIFT, 'z')],
        windows: vec![ch(PRIMARY, 'y'), ch(PRIMARY_SHIFT, 'z')],
        macos: vec![ch(PRIMARY_SHIFT, 'z')],
    };
    let mut windows = Registry::new(Platform::Windows);
    let redo = add(
        &mut windows,
        ActionSpec {
            defaults: defaults.clone(),
            ..spec("edit.redo", &[])
        },
    );
    assert_eq!(windows.shortcuts(redo).len(), 2);
    assert_eq!(
        windows.lookup(Context::GLOBAL, ch(PRIMARY, 'y')),
        Some(redo)
    );
    let mut linux = Registry::new(Platform::Linux);
    let redo = add(
        &mut linux,
        ActionSpec {
            defaults,
            ..spec("edit.redo", &[])
        },
    );
    assert_eq!(linux.lookup(Context::GLOBAL, ch(PRIMARY, 'y')), None);
    assert_eq!(linux.shortcuts(redo), &[ch(PRIMARY_SHIFT, 'z')]);
}

#[test]
fn defaults_that_conflict_or_take_a_fixed_key_are_refused() {
    let mut registry = Registry::new(Platform::Linux);
    let save = add(&mut registry, spec("file.save", &[ch(PRIMARY, 's')]));
    assert_eq!(
        registry.register(spec("file.save-all", &[ch(PRIMARY, 'S')])),
        Err(RegistryError::Conflict {
            shortcut: ch(PRIMARY, 's'),
            owner: save
        })
    );
    assert_eq!(
        registry.register(spec("view.next", &[named(NONE, NamedKey::Tab)])),
        Err(RegistryError::FixedKey(named(NONE, NamedKey::Tab)))
    );
    // A refused registration leaves nothing behind.
    assert_eq!(registry.find("file.save-all"), None);
    assert_eq!(
        registry.lookup(Context::GLOBAL, ch(PRIMARY, 's')),
        Some(save)
    );
}

#[test]
fn a_context_may_cover_a_global_shortcut() {
    let mut registry = Registry::new(Platform::Linux);
    let notation = registry
        .add_context("notation")
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(
        registry.add_context("notation"),
        Err(RegistryError::DuplicateContext("notation"))
    );
    let global = add(&mut registry, spec("view.find", &[ch(NONE, 'f')]));
    let note = add(
        &mut registry,
        ActionSpec {
            context: notation,
            ..spec("note.f", &[ch(NONE, 'f')])
        },
    );
    // Not a conflict (AM3): each context has its own.
    assert_eq!(
        registry.lookup(Context::GLOBAL, ch(NONE, 'f')),
        Some(global)
    );
    assert_eq!(registry.lookup(notation, ch(NONE, 'f')), Some(note));
}

#[test]
fn an_unknown_context_is_refused() {
    let mut other = Registry::new(Platform::Linux);
    let daw = other
        .add_context("daw")
        .unwrap_or_else(|error| panic!("{error}"));
    let mut registry = Registry::new(Platform::Linux);
    assert_eq!(
        registry.register(ActionSpec {
            context: daw,
            ..spec("daw.split", &[])
        }),
        Err(RegistryError::UnknownContext(daw))
    );
}

#[test]
fn the_user_replaces_restores_and_the_differences_are_listed() {
    let mut registry = Registry::new(Platform::Linux);
    let undo = add(&mut registry, spec("edit.undo", &[ch(PRIMARY, 'z')]));
    let save = add(&mut registry, spec("file.save", &[ch(PRIMARY, 's')]));
    assert_eq!(registry.overrides().count(), 0);

    // Replace: the old shortcut is free again, the new one is the action's.
    registry
        .set_shortcuts(undo, &[ch(PRIMARY, 'u'), ch(PRIMARY, 'u')])
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(registry.shortcuts(undo), &[ch(PRIMARY, 'u')]);
    assert_eq!(registry.lookup(Context::GLOBAL, ch(PRIMARY, 'z')), None);
    assert_eq!(
        registry.lookup(Context::GLOBAL, ch(PRIMARY, 'u')),
        Some(undo)
    );

    // Refused, all or nothing: a conflict, a fixed key.
    assert_eq!(
        registry.set_shortcuts(undo, &[ch(PRIMARY, 'x'), ch(PRIMARY, 's')]),
        Err(RegistryError::Conflict {
            shortcut: ch(PRIMARY, 's'),
            owner: save
        })
    );
    assert_eq!(
        registry.set_shortcuts(undo, &[ch(ALT, 'e')]),
        Err(RegistryError::FixedKey(ch(ALT, 'e')))
    );
    assert_eq!(registry.shortcuts(undo), &[ch(PRIMARY, 'u')]);

    // No shortcut at all is a difference too.
    registry
        .set_shortcuts(save, &[])
        .unwrap_or_else(|error| panic!("{error}"));
    let overrides: Vec<_> = registry.overrides().collect();
    assert_eq!(
        overrides,
        vec![
            ("edit.undo", &[ch(PRIMARY, 'u')][..]),
            ("file.save", &[][..])
        ]
    );

    // Restoring one action, then everything.
    registry
        .restore(undo)
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(
        registry.lookup(Context::GLOBAL, ch(PRIMARY, 'z')),
        Some(undo)
    );
    assert_eq!(registry.lookup(Context::GLOBAL, ch(PRIMARY, 'u')), None);
    registry.restore_all();
    assert_eq!(registry.overrides().count(), 0);
    assert_eq!(
        registry.lookup(Context::GLOBAL, ch(PRIMARY, 's')),
        Some(save)
    );
}

#[test]
fn restoring_a_default_given_to_another_action_is_a_conflict() {
    let mut registry = Registry::new(Platform::Linux);
    let undo = add(&mut registry, spec("edit.undo", &[ch(PRIMARY, 'z')]));
    let other = add(&mut registry, spec("edit.other", &[]));
    registry
        .set_shortcuts(undo, &[])
        .unwrap_or_else(|error| panic!("{error}"));
    registry
        .set_shortcuts(other, &[ch(PRIMARY, 'z')])
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(
        registry.restore(undo),
        Err(RegistryError::Conflict {
            shortcut: ch(PRIMARY, 'z'),
            owner: other
        })
    );
}

#[test]
fn loading_lets_the_later_entry_win_and_reports_every_problem() {
    let mut registry = Registry::new(Platform::Linux);
    let undo = add(&mut registry, spec("edit.undo", &[ch(PRIMARY, 'z')]));
    let save = add(&mut registry, spec("file.save", &[ch(PRIMARY, 's')]));
    let warnings = registry.load_overrides([
        // A user file written by hand: Save takes Ctrl+Z, which Undo has by default.
        ("file.save".to_owned(), vec![ch(PRIMARY, 'z'), ch(ALT, 's')]),
        // An action removed since.
        ("edit.gone".to_owned(), vec![ch(PRIMARY, 'g')]),
    ]);
    assert_eq!(
        warnings,
        vec![
            LoadWarning::Conflict {
                shortcut: ch(PRIMARY, 'z'),
                winner: save,
                loser: undo
            },
            LoadWarning::FixedKey {
                action: save,
                shortcut: ch(ALT, 's')
            },
            LoadWarning::UnknownAction("edit.gone".to_owned()),
        ]
    );
    assert_eq!(
        registry.lookup(Context::GLOBAL, ch(PRIMARY, 'z')),
        Some(save)
    );
    assert_eq!(registry.lookup(Context::GLOBAL, ch(PRIMARY, 's')), None);
    assert_eq!(registry.shortcuts(undo), &[]);
    // The differences: both actions, and the unknown entry kept to be written back.
    let overrides: Vec<_> = registry.overrides().collect();
    assert_eq!(
        overrides,
        vec![
            ("edit.undo", &[][..]),
            ("file.save", &[ch(PRIMARY, 'z')][..]),
            ("edit.gone", &[ch(PRIMARY, 'g')][..]),
        ]
    );
    // Loading again starts from the defaults.
    assert_eq!(registry.load_overrides([]), Vec::new());
    assert_eq!(
        registry.lookup(Context::GLOBAL, ch(PRIMARY, 'z')),
        Some(undo)
    );
    assert_eq!(registry.overrides().count(), 0);
}

#[test]
fn states_change_and_say_so() {
    let mut registry = Registry::new(Platform::Linux);
    let undo = add(&mut registry, spec("edit.undo", &[]));
    assert_eq!(registry.state(undo), Some(ActionState::ENABLED));
    assert_eq!(registry.set_state(undo, ActionState::DISABLED), Ok(true));
    assert_eq!(registry.set_state(undo, ActionState::DISABLED), Ok(false));
    assert_eq!(registry.state(undo), Some(ActionState::DISABLED));
}
