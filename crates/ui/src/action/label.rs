// SPDX-License-Identifier: GPL-3.0-or-later

//! How a shortcut is written for people (ADR-0048 AM12, choice S1 of F2.11d2a): in the menus, in
//! the accessibility tree's `keyboard_shortcut`, in tooltips.
//!
//! # The two styles
//! - **Linux and Windows:** the names of the keys, **translated**, joined by "+" with no spaces:
//!   "Ctrl+Shift+S" in English, "Strg+Umschalt+S" in German, "Ctrl+Maiusc+S" in Italian,
//!   "Ctrl+Maj+S" in French, "Ctrl+Mayús+S" in Spanish. The modifiers come in a fixed order:
//!   Ctrl, Alt, Shift, then the logo key (Super on Linux, Win on Windows), the order the owner
//!   approved in S1. The names come from the catalogues (CLAUDE.md §7), through [`KeyNames`]:
//!   the research behind each one is in `crates/i18n/glossary.toml` (entries `key_*`).
//! - **macOS:** Apple's symbols, in Apple's order and with no separator: ⌃⌥⇧⌘ and then the
//!   key ("⇧⌘S"). The modifier symbols are not words, so they are not translated (Apple's own
//!   menus show the same symbols in every language); the named keys that have a symbol on Apple's
//!   keyboards use it too (↩ ⎋ ⇥ ⌫ ⌦ ↖ ↘ ⇞ ⇟ and the arrows), the others (Space, the function keys,
//!   the keypad) take the translated name, as macOS writes "Space" in words.
//!
//! The "+" separator and the macOS symbols are notation, not language: Windows, GTK and Qt use
//! the same "+" in all six of our languages (checked in their translations, see the glossary),
//! so they are constants here and nothing in them can need a translator.
//!
//! # The key itself
//! A letter is written in **upper case** ("Ctrl+S", as every platform's menus write it, even
//! though the shortcut stores the lower case, ADR-0048 AM5); a digit as the digit; any other
//! character as itself ("Ctrl+,", "⌘?"); a named key by its translated name ("Ctrl+Entf").
//!
//! # Cost
//! The text is written **once**, when the shortcuts or the language change, and kept by the
//! registry next to the action ([`Registry::shortcut_text`](super::registry::Registry)): drawing
//! a menu copies no text and formats nothing. Writing one is O(length), appending to a reused
//! string.

use super::shortcut::{Platform, Shortcut, ShortcutKey};
use crate::window::input::NamedKey;

/// A key whose name a shortcut may need, to be looked up in the catalogues (choice S1 of
/// F2.11d2a). The application turns each into the Fluent message of its language
/// ([`KeyNames::new`]); the interface never writes a key's name itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyName {
    /// The Control key (Linux, Windows): "Ctrl", "Strg".
    Ctrl,
    /// The Shift key: "Shift", "Maiusc", "Maj", "Umschalt", "Mayús".
    Shift,
    /// The Alt key: "Alt" everywhere.
    Alt,
    /// The logo key on Linux: "Super" (GNOME's and GTK's name; KDE says "Meta").
    Super,
    /// The logo key on Windows: "Win".
    Windows,
    /// Enter (Return).
    Enter,
    /// Escape.
    Escape,
    /// Tab.
    Tab,
    /// The space bar.
    Space,
    /// Backspace.
    Backspace,
    /// Delete (forward).
    Delete,
    /// Insert.
    Insert,
    /// Home.
    Home,
    /// End.
    End,
    /// Page up.
    PageUp,
    /// Page down.
    PageDown,
    /// The left arrow.
    Left,
    /// The right arrow.
    Right,
    /// The up arrow.
    Up,
    /// The down arrow.
    Down,
    /// The Menu (application) key.
    Menu,
    /// A function key, F1 to F24 (the number); written by the catalogue, so a language could
    /// write it differently (all six write "F1").
    Function(u8),
    /// A digit of the numeric keypad, 0 to 9: "Num 1".
    Numpad(u8),
}

/// The fixed names of [`KeyName`] without a number, in the order [`KeyNames`] stores them.
const FIXED: [KeyName; 21] = [
    KeyName::Ctrl,
    KeyName::Shift,
    KeyName::Alt,
    KeyName::Super,
    KeyName::Windows,
    KeyName::Enter,
    KeyName::Escape,
    KeyName::Tab,
    KeyName::Space,
    KeyName::Backspace,
    KeyName::Delete,
    KeyName::Insert,
    KeyName::Home,
    KeyName::End,
    KeyName::PageUp,
    KeyName::PageDown,
    KeyName::Left,
    KeyName::Right,
    KeyName::Up,
    KeyName::Down,
    KeyName::Menu,
];

/// How many function keys have a name (F1–F24, as [`NamedKey::Function`]).
const FUNCTION_KEYS: usize = 24;

/// The names of the keys in one language (choice S1 of F2.11d2a), asked once of the application
/// and kept: every shortcut's text is written from them.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct KeyNames {
    /// The names of [`FIXED`], in that order.
    fixed: Vec<String>,
    /// F1 to F24.
    function: Vec<String>,
    /// The keypad's 0 to 9.
    numpad: Vec<String>,
}

impl KeyNames {
    /// Asks `name` for every key's name, once: the application answers from its catalogues
    /// (`key-ctrl`, `key-function` with the number…).
    ///
    /// # Cost
    /// 55 calls and the strings they return; done when the language changes, never per frame.
    #[must_use]
    pub fn new(mut name: impl FnMut(KeyName) -> String) -> Self {
        Self {
            fixed: FIXED.iter().map(|&key| name(key)).collect(),
            // 1..=24 and 0..=9 fit a u8; the ranges are written as u8 so no cast is needed.
            function: (1..=24_u8).map(|n| name(KeyName::Function(n))).collect(),
            numpad: (0..=9_u8).map(|d| name(KeyName::Numpad(d))).collect(),
        }
    }

    /// The name of `key`; empty for a key that has none (a function key past F24, or names that
    /// were never given: [`KeyNames::default`]).
    #[must_use]
    pub fn get(&self, key: KeyName) -> &str {
        let found = match key {
            KeyName::Function(n) => usize::from(n)
                .checked_sub(1)
                .filter(|&index| index < FUNCTION_KEYS)
                .and_then(|index| self.function.get(index)),
            KeyName::Numpad(d) => self.numpad.get(usize::from(d)),
            fixed => FIXED
                .iter()
                .position(|&known| known == fixed)
                .and_then(|index| self.fixed.get(index)),
        };
        found.map_or("", String::as_str)
    }
}

/// The separator of the Linux and Windows style (see the module documentation).
const PLUS: char = '+';

/// Apple's modifier symbols, in Apple's order (Control, Option, Shift, Command).
const MAC_CONTROL: char = '\u{2303}';
/// See [`MAC_CONTROL`]: Option.
const MAC_OPTION: char = '\u{2325}';
/// See [`MAC_CONTROL`]: Shift.
const MAC_SHIFT: char = '\u{21E7}';
/// See [`MAC_CONTROL`]: Command.
const MAC_COMMAND: char = '\u{2318}';

impl Shortcut {
    /// Writes this shortcut for people, in `platform`'s style with `names` (see the module
    /// documentation), appending to `out`.
    ///
    /// # Cost
    /// O(length of the text); no allocation beyond `out` growing.
    pub fn write_label(&self, platform: Platform, names: &KeyNames, out: &mut String) {
        let modifiers = self.modifiers();
        if platform == Platform::MacOs {
            // *Secondary* is ⌃ and *Primary* is ⌘ on macOS (ADR-0040 W5).
            for (held, symbol) in [
                (modifiers.secondary, MAC_CONTROL),
                (modifiers.alt, MAC_OPTION),
                (modifiers.shift, MAC_SHIFT),
                (modifiers.primary, MAC_COMMAND),
            ] {
                if held {
                    out.push(symbol);
                }
            }
            write_key(self.key(), platform, names, out);
            return;
        }
        // *Primary* is Ctrl, *Secondary* the logo key, on Linux and Windows.
        let logo = if platform == Platform::Windows {
            KeyName::Windows
        } else {
            KeyName::Super
        };
        for (held, key) in [
            (modifiers.primary, KeyName::Ctrl),
            (modifiers.alt, KeyName::Alt),
            (modifiers.shift, KeyName::Shift),
            (modifiers.secondary, logo),
        ] {
            if held {
                out.push_str(names.get(key));
                out.push(PLUS);
            }
        }
        write_key(self.key(), platform, names, out);
    }
}

/// Writes the key of a shortcut (see the module documentation).
fn write_key(key: ShortcutKey, platform: Platform, names: &KeyNames, out: &mut String) {
    match key {
        // A letter in upper case, as every platform's menus write it; a character with no single
        // upper case ("ß" → "SS") is written as its upper case anyway, which is what a keyboard's
        // label would show.
        ShortcutKey::Character(character) => out.extend(character.to_uppercase()),
        ShortcutKey::Digit(digit) => out.push(char::from(b'0'.saturating_add(digit))),
        ShortcutKey::Numpad(digit) => out.push_str(names.get(KeyName::Numpad(digit))),
        ShortcutKey::Named(named) => {
            if platform == Platform::MacOs
                && let Some(symbol) = mac_symbol(named)
            {
                out.push(symbol);
            } else {
                out.push_str(names.get(key_name(named)));
            }
        }
    }
}

/// The catalogue name of a named key.
const fn key_name(key: NamedKey) -> KeyName {
    match key {
        NamedKey::Enter => KeyName::Enter,
        NamedKey::Escape => KeyName::Escape,
        NamedKey::Tab => KeyName::Tab,
        NamedKey::Space => KeyName::Space,
        NamedKey::Backspace => KeyName::Backspace,
        NamedKey::Delete => KeyName::Delete,
        NamedKey::Insert => KeyName::Insert,
        NamedKey::ArrowUp => KeyName::Up,
        NamedKey::ArrowDown => KeyName::Down,
        NamedKey::ArrowLeft => KeyName::Left,
        NamedKey::ArrowRight => KeyName::Right,
        NamedKey::Home => KeyName::Home,
        NamedKey::End => KeyName::End,
        NamedKey::PageUp => KeyName::PageUp,
        NamedKey::PageDown => KeyName::PageDown,
        NamedKey::Function(number) => KeyName::Function(number),
        NamedKey::ContextMenu => KeyName::Menu,
    }
}

/// The symbol Apple's menus show for a named key, if it has one (Apple's Human Interface
/// Guidelines, "Keyboard shortcuts"): Return ↩, Escape ⎋, Tab ⇥, Delete (backwards) ⌫, forward
/// delete ⌦, Home ↖, End ↘, Page Up ⇞, Page Down ⇟ and the four arrows. Space, Insert, the
/// function keys and the Menu key are written in words.
const fn mac_symbol(key: NamedKey) -> Option<char> {
    Some(match key {
        NamedKey::Enter => '\u{21A9}',
        NamedKey::Escape => '\u{238B}',
        NamedKey::Tab => '\u{21E5}',
        NamedKey::Backspace => '\u{232B}',
        NamedKey::Delete => '\u{2326}',
        NamedKey::Home => '\u{2196}',
        NamedKey::End => '\u{2198}',
        NamedKey::PageUp => '\u{21DE}',
        NamedKey::PageDown => '\u{21DF}',
        NamedKey::ArrowUp => '\u{2191}',
        NamedKey::ArrowDown => '\u{2193}',
        NamedKey::ArrowLeft => '\u{2190}',
        NamedKey::ArrowRight => '\u{2192}',
        _ => return None,
    })
}
