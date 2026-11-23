// SPDX-License-Identifier: GPL-3.0-or-later

//! The **settings text** of a shortcut (ADR-0048 AM7, SCALETTA F2.12a2 choice S3): how a
//! [`Shortcut`] is written in the settings file and read back, such as `Primary+Shift+z`.
//!
//! # Why a text form of its own
//! The settings file must mean the same thing on every system (AM7), so it is written with the
//! **abstract** modifiers (`Primary`, never "Ctrl" or "Cmd") and with the shortcut's normalised
//! key. It is **not** what the user reads: the menus and the shortcut editor show the platform's
//! own spelling, translated (`crate::action::label`, F2.11d2a). This is why it is a pair of
//! methods named after the settings, not `Display`/`FromStr`, which would invite using it on
//! screen.
//!
//! # The grammar
//! ```text
//! shortcut  = { modifier "+" } key
//! modifier  = "Primary" | "Secondary" | "Alt" | "Shift"
//! key       = one character | "Numpad" digit | name
//! name      = "Enter" | "Escape" | "Tab" | "Space" | "Backspace" | "Delete" | "Insert"
//!           | "ArrowUp" | "ArrowDown" | "ArrowLeft" | "ArrowRight" | "Home" | "End"
//!           | "PageUp" | "PageDown" | "F1" … "F24" | "Menu" | "Alt"
//! ```
//! - Written: the modifiers always in the order Primary, Secondary, Alt, Shift (the order of the
//!   menus' labels on Linux and Windows), each once; the key last.
//! - Read: the modifiers in **any** order, each at most once, with exactly this capitalisation
//!   (the file is written by the program; a hand-written "ctrl+s" is refused, not guessed). A
//!   modifier word is a modifier only when a `+` follows it, so `Primary+Alt` is the Alt key with
//!   the primary modifier (a fixed key, AM8, which the registry then drops) and `Primary++` is the
//!   plus key: the modifiers are taken from the front and whatever is left is the key.
//! - A single character is a character key, normalised by [`Shortcut::new`]: `Primary+S` reads as
//!   `Primary+s` (Shift is a modifier of its own, never implied by the case), and an ASCII digit
//!   is the digit row's key, written as the digit itself (`Primary+1`).
//!
//! # Cost
//! Writing allocates the returned `String` (a few bytes); reading is a scan of the text with no
//! allocation. Both run only when the settings file is read or written, never per frame.

use super::shortcut::{Shortcut, ShortcutError, ShortcutKey};
use crate::window::input::{Modifiers, NamedKey};

/// The flag of one modifier inside a [`Modifiers`] set, to read or to set.
type Flag = fn(&mut Modifiers) -> &mut bool;

/// The modifiers' words in their written order, with the flag of each.
///
/// A table, so that writing and reading cannot disagree on a spelling or an order.
const MODIFIERS: [(&str, Flag); 4] = [
    ("Primary", |m| &mut m.primary),
    ("Secondary", |m| &mut m.secondary),
    ("Alt", |m| &mut m.alt),
    ("Shift", |m| &mut m.shift),
];

/// The named keys and their words (every [`NamedKey`] but the function keys, which are written
/// `F1`–`F24`). Linear lookups over 17 entries: the text is read once per settings file.
const NAMES: [(NamedKey, &str); 17] = [
    (NamedKey::Enter, "Enter"),
    (NamedKey::Escape, "Escape"),
    (NamedKey::Tab, "Tab"),
    (NamedKey::Space, "Space"),
    (NamedKey::Backspace, "Backspace"),
    (NamedKey::Delete, "Delete"),
    (NamedKey::Insert, "Insert"),
    (NamedKey::ArrowUp, "ArrowUp"),
    (NamedKey::ArrowDown, "ArrowDown"),
    (NamedKey::ArrowLeft, "ArrowLeft"),
    (NamedKey::ArrowRight, "ArrowRight"),
    (NamedKey::Home, "Home"),
    (NamedKey::End, "End"),
    (NamedKey::PageUp, "PageUp"),
    (NamedKey::PageDown, "PageDown"),
    (NamedKey::ContextMenu, "Menu"),
    (NamedKey::Alt, "Alt"),
];

/// The highest function key a shortcut may name (`NamedKey::Function` covers F1 to F24).
const MAX_FUNCTION_KEY: u8 = 24;

/// Why a settings text is not a shortcut. Typed, so the settings loader can report it (and the
/// tests match it) without parsing an English message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShortcutTextError {
    /// The text is empty, or has modifiers and no key (`"Primary+"`).
    NoKey,
    /// A modifier appears twice (`"Shift+Shift+a"`).
    RepeatedModifier(&'static str),
    /// The key is neither one character nor a known name (`"Ctrl+s"`: "Ctrl" is not a modifier
    /// word, so `Ctrl+s` is taken as a key and is unknown).
    UnknownKey(String),
    /// The key is a character that cannot be a shortcut's key (a control character, white space).
    Invalid(ShortcutError),
}

impl std::fmt::Display for ShortcutTextError {
    /// A short English description, for developers and the log (never shown on screen).
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoKey => f.write_str("the shortcut has no key"),
            Self::RepeatedModifier(word) => write!(f, "the modifier {word} appears twice"),
            Self::UnknownKey(key) => write!(f, "{key:?} is not a key"),
            Self::Invalid(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for ShortcutTextError {}

impl Shortcut {
    /// The shortcut as the settings file writes it (see the module documentation), such as
    /// `Primary+Shift+z`. [`Shortcut::from_settings_text`] reads it back to the same shortcut.
    ///
    /// O(length); allocates the returned text.
    #[must_use]
    pub fn settings_text(&self) -> String {
        let mut text = String::new();
        let mut modifiers = self.modifiers();
        for (word, flag) in MODIFIERS {
            if *flag(&mut modifiers) {
                text.push_str(word);
                text.push('+');
            }
        }
        match self.key() {
            ShortcutKey::Character(character) => text.push(character),
            ShortcutKey::Digit(digit) => text.push(char::from(b'0'.saturating_add(digit))),
            ShortcutKey::Numpad(digit) => {
                text.push_str("Numpad");
                text.push(char::from(b'0'.saturating_add(digit)));
            }
            ShortcutKey::Named(NamedKey::Function(number)) => {
                text.push('F');
                text.push_str(&number.to_string());
            }
            ShortcutKey::Named(named) => {
                // Every other named key is in the table (a unit test checks the round trip of
                // each), so the fallback never writes anything in practice.
                if let Some((_, word)) = NAMES.iter().find(|(key, _)| *key == named) {
                    text.push_str(word);
                }
            }
        }
        text
    }

    /// Reads a shortcut written by [`Shortcut::settings_text`] (see the module documentation
    /// for the grammar). O(length), no allocation except for an error's copy of the key.
    ///
    /// # Errors
    /// [`ShortcutTextError`]: no key, a modifier twice, an unknown key, or a character that
    /// cannot be a key.
    pub fn from_settings_text(text: &str) -> Result<Self, ShortcutTextError> {
        let mut modifiers = Modifiers::default();
        let mut rest = text;
        // Take modifier words from the front while one is followed by "+": what remains is the
        // key, so "Primary++" leaves "+", and "Primary+Alt" leaves "Alt".
        'words: loop {
            for (word, flag) in MODIFIERS {
                if let Some(after) = rest.strip_prefix(word).and_then(|r| r.strip_prefix('+')) {
                    let held = flag(&mut modifiers);
                    if *held {
                        return Err(ShortcutTextError::RepeatedModifier(word));
                    }
                    *held = true;
                    rest = after;
                    continue 'words;
                }
            }
            break;
        }
        let key = key_of(rest)?;
        Self::new(modifiers, key).map_err(ShortcutTextError::Invalid)
    }
}

/// The key written as `text`: one character, `Numpad` and a digit, `F1`–`F24`, or a name.
///
/// # Errors
/// [`ShortcutTextError::NoKey`] for an empty text, [`ShortcutTextError::UnknownKey`] otherwise.
fn key_of(text: &str) -> Result<ShortcutKey, ShortcutTextError> {
    let mut characters = text.chars();
    match (characters.next(), characters.next()) {
        (None, _) => return Err(ShortcutTextError::NoKey),
        // One character: `Shortcut::new` normalises it (case, digit row) or refuses it.
        (Some(character), None) => return Ok(ShortcutKey::Character(character)),
        _ => {}
    }
    if let Some(digit) = text.strip_prefix("Numpad").and_then(single_digit) {
        return Ok(ShortcutKey::Numpad(digit));
    }
    if let Some(number) = text.strip_prefix('F').and_then(function_number) {
        return Ok(ShortcutKey::Named(NamedKey::Function(number)));
    }
    NAMES
        .iter()
        .find(|(_, word)| *word == text)
        .map(|&(key, _)| ShortcutKey::Named(key))
        .ok_or_else(|| ShortcutTextError::UnknownKey(text.to_owned()))
}

/// The digit of a text made of exactly one ASCII digit.
fn single_digit(text: &str) -> Option<u8> {
    match text.as_bytes() {
        &[d] if d.is_ascii_digit() => Some(d.wrapping_sub(b'0')),
        _ => None,
    }
}

/// The number of a function key, 1 to 24, written without a leading zero.
fn function_number(text: &str) -> Option<u8> {
    if text.starts_with('0') {
        return None;
    }
    text.parse::<u8>()
        .ok()
        .filter(|n| (1..=MAX_FUNCTION_KEY).contains(n))
}

#[cfg(test)]
mod tests {
    //! The settings text: the examples of ADR-0048 AM7, every named key's round trip, the edge
    //! cases of the grammar and a property test of the round trip.
    #![allow(clippy::unwrap_used, reason = "a failed unwrap fails the test")]

    use proptest::prelude::*;

    use super::*;

    /// A shortcut from its parts.
    fn shortcut(primary: bool, shift: bool, alt: bool, key: ShortcutKey) -> Shortcut {
        let modifiers = Modifiers {
            primary,
            secondary: false,
            shift,
            alt,
        };
        Shortcut::new(modifiers, key).unwrap()
    }

    /// AM7's own example, and the order of the written modifiers.
    #[test]
    fn examples_are_written_and_read() {
        let redo = shortcut(true, true, false, ShortcutKey::Character('Z'));
        assert_eq!(redo.settings_text(), "Primary+Shift+z");
        assert_eq!(Shortcut::from_settings_text("Primary+Shift+z"), Ok(redo));
        // Any order is read; the written order is fixed.
        assert_eq!(Shortcut::from_settings_text("Shift+Primary+z"), Ok(redo));
        let all = Shortcut::new(
            Modifiers {
                primary: true,
                secondary: true,
                shift: true,
                alt: true,
            },
            ShortcutKey::Named(NamedKey::Function(5)),
        )
        .unwrap();
        assert_eq!(all.settings_text(), "Primary+Secondary+Alt+Shift+F5");
    }

    /// Digits, the keypad, the plus key and the Alt key as a key.
    #[test]
    fn edge_cases_of_the_grammar() {
        let one = shortcut(true, false, false, ShortcutKey::Digit(1));
        assert_eq!(one.settings_text(), "Primary+1");
        assert_eq!(Shortcut::from_settings_text("Primary+1"), Ok(one));
        let pad = shortcut(false, false, false, ShortcutKey::Numpad(7));
        assert_eq!(pad.settings_text(), "Numpad7");
        assert_eq!(Shortcut::from_settings_text("Numpad7"), Ok(pad));
        let plus = shortcut(true, false, false, ShortcutKey::Character('+'));
        assert_eq!(plus.settings_text(), "Primary++");
        assert_eq!(Shortcut::from_settings_text("Primary++"), Ok(plus));
        let alt_key = shortcut(true, false, false, ShortcutKey::Named(NamedKey::Alt));
        assert_eq!(Shortcut::from_settings_text("Primary+Alt"), Ok(alt_key));
        // A capital letter reads as the lower-case key, without Shift.
        assert_eq!(
            Shortcut::from_settings_text("Primary+S"),
            Ok(shortcut(true, false, false, ShortcutKey::Character('s')))
        );
    }

    /// What is refused, and why.
    #[test]
    fn invalid_texts_are_refused() {
        assert_eq!(
            Shortcut::from_settings_text(""),
            Err(ShortcutTextError::NoKey)
        );
        assert_eq!(
            Shortcut::from_settings_text("Primary+"),
            Err(ShortcutTextError::NoKey)
        );
        assert_eq!(
            Shortcut::from_settings_text("Shift+Shift+a"),
            Err(ShortcutTextError::RepeatedModifier("Shift"))
        );
        for unknown in ["Ctrl+s", "primary+s", "F0", "F25", "F05", "Numpad10", "Up"] {
            assert!(
                matches!(
                    Shortcut::from_settings_text(unknown),
                    Err(ShortcutTextError::UnknownKey(_))
                ),
                "{unknown}"
            );
        }
        assert!(matches!(
            Shortcut::from_settings_text("Primary+ "),
            Err(ShortcutTextError::Invalid(ShortcutError::NotAKey(' ')))
        ));
    }

    /// Every named key and every function key survives the round trip.
    #[test]
    fn every_named_key_round_trips() {
        let names = NAMES
            .iter()
            .map(|&(key, _)| key)
            .chain((1..=MAX_FUNCTION_KEY).map(NamedKey::Function));
        for named in names {
            let key = shortcut(true, false, false, ShortcutKey::Named(named));
            assert_eq!(
                Shortcut::from_settings_text(&key.settings_text()),
                Ok(key),
                "{named:?}"
            );
        }
    }

    /// A strategy over every kind of key.
    fn any_key() -> impl Strategy<Value = ShortcutKey> {
        prop_oneof![
            any::<char>().prop_map(ShortcutKey::Character),
            (0..=9u8).prop_map(ShortcutKey::Digit),
            (0..=9u8).prop_map(ShortcutKey::Numpad),
            (1..=MAX_FUNCTION_KEY).prop_map(|n| ShortcutKey::Named(NamedKey::Function(n))),
            (0..NAMES.len()).prop_map(|i| ShortcutKey::Named(NAMES[i].0)),
        ]
    }

    proptest! {
        /// Whatever shortcut can be made reads back from its text identically.
        #[test]
        fn text_round_trips(
            primary in any::<bool>(),
            secondary in any::<bool>(),
            shift in any::<bool>(),
            alt in any::<bool>(),
            key in any_key(),
        ) {
            let modifiers = Modifiers { primary, secondary, shift, alt };
            if let Ok(shortcut) = Shortcut::new(modifiers, key) {
                prop_assert_eq!(Shortcut::from_settings_text(&shortcut.settings_text()), Ok(shortcut));
            }
        }
    }
}
