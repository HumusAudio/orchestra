// SPDX-License-Identifier: GPL-3.0-or-later

//! Shortcuts (ADR-0048 AM5, AM6, AM8): what one is, how a key press becomes the shortcuts it may
//! mean, and which keys stay fixed.
//!
//! # What a shortcut is (AM5, precision A of F2.11d1)
//! The abstract modifiers of [`Modifiers`] plus **one key** ([`ShortcutKey`]). The key is the
//! **logical** key, the character the keyboard layout gives, normalised so that the same
//! shortcut has one spelling only:
//! - a **letter** (a character that has an upper and a lower case, in any script) is kept in
//!   lower case, and Shift is a modifier of its own: Ctrl+Shift+Z is `Primary+Shift+z`, whether
//!   the layout reported `"Z"` or `"z"`;
//! - a **digit** is [`ShortcutKey::Digit`], the key of the digit row, with Shift as a modifier;
//! - **any other character** (punctuation, symbols) is the character produced, and Shift is
//!   **not** part of the shortcut: the character already says whether Shift was needed. ⌘? is
//!   `Primary+?`, and it fires whether the layout gives `?` with Shift (US) or without it.
//!   Comparing Shift there would make the same shortcut unreachable on half the layouts;
//! - the keypad's digits are [`ShortcutKey::Numpad`], apart from the digit row (AM5);
//! - a named key (F1, Delete, an arrow) is [`ShortcutKey::Named`].
//!
//! # A key press, and the two fallbacks (AM5 (i), (ii))
//! [`candidates`] turns one press into at most **two** shortcuts, tried in order: the logical
//! one, then a fallback by **physical** position:
//! - (ii) a key of the **digit row** also counts as its digit, so on French AZERTY the key that
//!   types "&" (and "1" with Shift) is also the digit 1, as the same key is on QWERTY;
//! - (i) a letter **outside the Latin script** (Cyrillic, Greek…) also counts as the Latin letter
//!   of the US layout at the same position, so Ctrl+Z still undoes with a Russian layout.
//!
//! The logical shortcut comes first: a user who bound `Primary+/` on German QWERTZ (where "/" is
//! Shift+7) gets their action, and only a press that matches nothing logically falls back to the
//! position.
//!
//! # Fixed keys (AM8, precision B of F2.11d1)
//! [`Shortcut::is_fixed`] answers the keys an action can never take: the widgets' navigation
//! (Tab, the arrows, Enter, Escape, Space) **with no modifier or with Shift alone**, F10 and
//! Shift+F10, the Menu key, and on Linux and Windows Alt+letter (the menus' mnemonics). With the
//! primary modifier the arrows stay free for actions (Ctrl+↑ in the score). The editing keys of a
//! text field are not refused: inside a field the field wins anyway (AM4), and the shortcut
//! editor (F2.12c) will warn about them.
//!
//! # Cost
//! Everything here is a few comparisons on `Copy` values: no allocation, O(1).

use crate::window::input::{Key, Modifiers, NamedKey, PhysicalKey};

/// The platform whose conventions apply (AM6): its default shortcuts and its fixed keys.
///
/// Chosen **by value**, not by `cfg`, so the tests on Linux check the tables of all three
/// (ADR-0047 does the same for the editing keys); [`Platform::CURRENT`] is the one the program
/// was built for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Platform {
    /// Linux (and the other Unix-like systems that are not macOS).
    Linux,
    /// Windows.
    Windows,
    /// macOS.
    MacOs,
}

impl Platform {
    /// The platform this program was built for.
    pub const CURRENT: Self = if cfg!(target_os = "macos") {
        Self::MacOs
    } else if cfg!(target_os = "windows") {
        Self::Windows
    } else {
        Self::Linux
    };
}

/// The key of a shortcut, normalised (see the module documentation).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShortcutKey {
    /// A character key: a letter in lower case (Shift is then a modifier of the shortcut), or
    /// another character as produced (Shift is then never part of the shortcut). Never an ASCII
    /// digit (that is [`ShortcutKey::Digit`]), never a control character or a space.
    Character(char),
    /// A key of the digit row, 0 to 9.
    Digit(u8),
    /// A digit of the numeric keypad, 0 to 9: a key apart from the digit row (AM5).
    Numpad(u8),
    /// A named key (F1, Delete, an arrow…).
    Named(NamedKey),
}

/// Why a shortcut could not be made.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShortcutError {
    /// A digit above 9.
    DigitOutOfRange(u8),
    /// A character that cannot be a key of a shortcut: a control character or white space (the
    /// space bar is [`NamedKey::Space`]).
    NotAKey(char),
}

impl std::fmt::Display for ShortcutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DigitOutOfRange(digit) => write!(f, "{digit} is not a digit from 0 to 9"),
            Self::NotAKey(character) => {
                write!(f, "{character:?} cannot be the key of a shortcut")
            }
        }
    }
}

impl std::error::Error for ShortcutError {}

/// A shortcut: the abstract modifiers plus one key (ADR-0048 AM5), always normalised (see the
/// module documentation), so two shortcuts that mean the same press are equal and hash the same.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Shortcut {
    /// The modifiers held; `shift` is always `false` for a non-letter character.
    modifiers: Modifiers,
    /// The key.
    key: ShortcutKey,
}

impl Shortcut {
    /// The shortcut of `key` with `modifiers`, normalised: a letter in lower case, an ASCII digit
    /// as [`ShortcutKey::Digit`], Shift dropped for another character.
    ///
    /// # Errors
    /// [`ShortcutError`] for a digit above 9 or a character that is not a key.
    pub fn new(modifiers: Modifiers, key: ShortcutKey) -> Result<Self, ShortcutError> {
        let mut modifiers = modifiers;
        let key = match key {
            ShortcutKey::Digit(digit) | ShortcutKey::Numpad(digit) if digit > 9 => {
                return Err(ShortcutError::DigitOutOfRange(digit));
            }
            ShortcutKey::Character(character) => {
                if character.is_control() || character.is_whitespace() {
                    return Err(ShortcutError::NotAKey(character));
                }
                if let Some(digit) = character.to_digit(10).filter(|_| character.is_ascii()) {
                    // `to_digit(10)` answers 0–9 only, so the cast cannot truncate.
                    ShortcutKey::Digit(u8::try_from(digit).unwrap_or(0))
                } else if has_case(character) {
                    ShortcutKey::Character(lower(character))
                } else {
                    // The character already says whether Shift was needed (precision A).
                    modifiers.shift = false;
                    ShortcutKey::Character(character)
                }
            }
            other => other,
        };
        Ok(Self { modifiers, key })
    }

    /// The modifiers.
    #[must_use]
    pub const fn modifiers(&self) -> Modifiers {
        self.modifiers
    }

    /// The key.
    #[must_use]
    pub const fn key(&self) -> ShortcutKey {
        self.key
    }

    /// `true` when no action may take this shortcut on `platform` (AM8, precision B of F2.11d1;
    /// see the module documentation).
    #[must_use]
    pub fn is_fixed(&self, platform: Platform) -> bool {
        let Modifiers {
            primary,
            secondary,
            // Shift changes nothing here: Shift+Tab and Shift+arrows are navigation too, and
            // Alt+Shift+letter still opens a mnemonic on Windows.
            shift: _,
            alt,
        } = self.modifiers;
        // No modifier, or Shift alone: the plain navigation keys (Shift+Tab, Shift+arrow extend a
        // selection, Shift+F10 opens a context menu).
        let plain = !primary && !secondary && !alt;
        match self.key {
            ShortcutKey::Named(
                NamedKey::Tab
                | NamedKey::ArrowUp
                | NamedKey::ArrowDown
                | NamedKey::ArrowLeft
                | NamedKey::ArrowRight
                | NamedKey::Enter
                | NamedKey::Escape
                | NamedKey::Space
                | NamedKey::Function(10),
            ) => plain,
            // The Menu key opens the context menu whatever is held.
            ShortcutKey::Named(NamedKey::ContextMenu) => true,
            // Alt+letter opens a menu by its mnemonic on Linux and Windows (AM10, AM12); macOS has
            // no mnemonics, and ⌥ there types special characters or is a modifier like any other.
            ShortcutKey::Character(character) => {
                alt && !primary && !secondary && has_case(character) && !is_mac(platform)
            }
            _ => false,
        }
    }
}

/// `true` on macOS.
const fn is_mac(platform: Platform) -> bool {
    matches!(platform, Platform::MacOs)
}

/// `true` for a character with an upper and a lower case (a letter of a bicameral script).
fn has_case(character: char) -> bool {
    character.is_lowercase() || character.is_uppercase()
}

/// The lower case of `character` when it is a single character, otherwise `character` itself (a
/// few characters lower to two, such as "İ"; they stay as typed, which still matches the same
/// press, since a press is normalised the same way).
fn lower(character: char) -> char {
    let mut lowered = character.to_lowercase();
    match (lowered.next(), lowered.next()) {
        (Some(single), None) => single,
        _ => character,
    }
}

/// `true` for a letter of the Latin script: ASCII, Latin-1 Supplement, Latin Extended-A and -B,
/// IPA Extensions, Latin Extended Additional and the Latin ligatures. Enough for every Latin
/// keyboard layout; everything else with a case (Cyrillic, Greek, Armenian, Georgian…) takes the
/// fallback by position (AM5 (i)). A table of ranges instead of a Unicode script crate: the
/// question is asked once per key press and these ranges never change.
const fn is_latin(character: char) -> bool {
    matches!(character,
        'a'..='z'
        | 'A'..='Z'
        | '\u{00C0}'..='\u{024F}'
        | '\u{0250}'..='\u{02AF}'
        | '\u{1E00}'..='\u{1EFF}'
        | '\u{FB00}'..='\u{FB06}')
}

/// The shortcuts one key press may mean, in the order they are tried: the logical one, then the
/// fallback by position (see the module documentation). `None` where there is none.
///
/// `key` and `physical` are the press's ([`crate::InputEvent::Key`]), `modifiers` those held.
///
/// # Cost
/// O(1), no allocation: the logical key's text is read, never copied.
#[must_use]
pub(crate) fn candidates(
    key: &Key,
    physical: PhysicalKey,
    modifiers: Modifiers,
) -> [Option<Shortcut>; 2] {
    // The digit of the keypad, whatever the logical key says (it says the digit when Num Lock is
    // on; with Num Lock off the logical key is an arrow or Home, and that is what counts).
    if let (PhysicalKey::Numpad(digit), Key::Character(_)) = (physical, key) {
        return [
            Shortcut::new(modifiers, ShortcutKey::Numpad(digit)).ok(),
            None,
        ];
    }
    let logical = match key {
        Key::Named(named) => Shortcut::new(modifiers, ShortcutKey::Named(*named)).ok(),
        Key::Character(text) => single_char(text)
            .and_then(|character| Shortcut::new(modifiers, ShortcutKey::Character(character)).ok()),
        Key::Unknown => None,
    };
    // The fallback by position: the digit row (ii) for any press that is not already a digit,
    // the Latin letter (i) for a letter outside the Latin script.
    let fallback = match (physical, logical.map(|shortcut| shortcut.key)) {
        (PhysicalKey::Digit(_), Some(ShortcutKey::Digit(_))) => None,
        (PhysicalKey::Digit(digit), _) => Shortcut::new(modifiers, ShortcutKey::Digit(digit)).ok(),
        (PhysicalKey::Letter(letter), Some(ShortcutKey::Character(character)))
            if has_case(character) && !is_latin(character) =>
        {
            Shortcut::new(
                modifiers,
                ShortcutKey::Character(char::from(b'a'.saturating_add(letter))),
            )
            .ok()
        }
        _ => None,
    };
    [logical, fallback]
}

/// The only character of `text`, or `None` when it has none or several (a few layouts give a
/// short string for one key; it cannot be a shortcut's key).
fn single_char(text: &str) -> Option<char> {
    let mut characters = text.chars();
    match (characters.next(), characters.next()) {
        (Some(character), None) => Some(character),
        _ => None,
    }
}
