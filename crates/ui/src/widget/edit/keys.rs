// SPDX-License-Identifier: GPL-3.0-or-later

//! From a key to an editing action (ADR-0047 TF3): the platform's text conventions, fixed inside
//! a field.
//!
//! # Why a value and not `cfg`
//! The convention is an [`EditConvention`] value, set from the platform the program was built
//! for ([`EditConvention::current`]) but passed around like any other value. So the tests on the
//! Linux CI check the macOS and Windows tables too, and the three tables are written side by side
//! in one function, where a reader sees how they differ.
//!
//! # The tables (TF3)
//! | Action | Linux, Windows | macOS |
//! |---|---|---|
//! | Grapheme left / right | ← / → | ← / → |
//! | Word left / right | Ctrl+← / Ctrl+→ | ⌥← / ⌥→ |
//! | Start / end of the line | Home / End | ⌘← / ⌘→, ↑ / ↓, Home / End |
//! | Extend the selection | Shift + any of the above | Shift + any of the above |
//! | Delete before / after | Backspace / Delete | ⌫ / ⌦ |
//! | Delete the word before / after | Ctrl+Backspace / Ctrl+Delete | ⌥⌫ / ⌥⌦ |
//! | Delete to the line's start / end | — | ⌘⌫ / ⌘⌦ |
//! | Select all | Ctrl+A | ⌘A |
//! | Cut, copy, paste | Ctrl+X, C, V; Shift+Delete, Ctrl+Insert, Shift+Insert | ⌘X, C, V |
//! | Undo, redo | Ctrl+Z; Ctrl+Shift+Z, Ctrl+Y | ⌘Z; ⌘⇧Z |
//!
//! Precisions found while writing them (2026-11-13, within TF3's "the platform's convention"):
//! - Ctrl+→ goes to the **end** of the word on Linux (GTK, Qt) and macOS, to the **start of the
//!   next word** on Windows: the editor chooses the target from the convention
//!   ([`EditConvention::word_forward_to_start`]).
//! - ↑ and ↓ in a one-line field go to its start and end on macOS (`NSTextField`); on Linux and
//!   Windows they do nothing in a one-line field, so they are not mapped and bubble (a list
//!   around the field may use them).
//! - Shift+Backspace deletes like Backspace (GTK and Windows do so), so a held Shift while
//!   correcting a capital does not stop the deletion.
//! - macOS's Emacs-style keys (⌃A, ⌃E, ⌃K…) are not mapped: the ADR does not list them, and the
//!   secondary modifier stays free for the application. To add if Mac users ask.
//!
//! Keys with modifiers the table does not use (Alt on Linux and Windows, ⌃ on macOS, or the
//! secondary modifier anywhere) map to nothing, so they bubble to the application's shortcuts.

use crate::window::input::{Key, Modifiers, NamedKey};

/// Which platform's text conventions a field follows (ADR-0047 TF3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EditConvention {
    /// GTK and Qt on Linux: Ctrl for words, Home and End for the line.
    Linux,
    /// Windows: as Linux, but Ctrl+→ goes to the start of the next word.
    Windows,
    /// macOS: ⌥ for words, ⌘ for the line, ↑ and ↓ to the ends of a one-line field.
    MacOs,
}

impl EditConvention {
    /// The convention of the platform this program was built for.
    #[must_use]
    pub const fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::MacOs
        } else if cfg!(target_os = "windows") {
            Self::Windows
        } else {
            Self::Linux
        }
    }

    /// `true` when moving (or deleting) one word forwards goes to the **start of the next word**
    /// (Windows), `false` when it goes to the **end of the word** (Linux, macOS).
    #[must_use]
    pub const fn word_forward_to_start(self) -> bool {
        matches!(self, Self::Windows)
    }
}

/// Which way a movement or a deletion goes, in text order (left to right only, ADR-0044 TX5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Direction {
    /// Towards the start of the text (left).
    Backward,
    /// Towards the end of the text (right).
    Forward,
}

/// How far a movement or a deletion goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Unit {
    /// One grapheme cluster: what a reader sees as one character (TF1).
    Grapheme,
    /// One word (where it ends depends on the convention, see [`EditConvention`]).
    Word,
    /// To the edge of the line: in a one-line field, the start or the end of the text.
    Line,
}

/// What a key asks the field to do (ADR-0047 TF3). Typing is not here: it arrives as text
/// (TF5), not as a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EditAction {
    /// Move the caret; with `extend`, the selection's anchor stays where it is.
    Move {
        /// Which way.
        direction: Direction,
        /// How far.
        unit: Unit,
        /// Shift held: extend the selection instead of collapsing it.
        extend: bool,
    },
    /// Delete the selection, or, when nothing is selected, from the caret by `unit`.
    Delete {
        /// Which way.
        direction: Direction,
        /// How far.
        unit: Unit,
    },
    /// Select the whole text.
    SelectAll,
    /// Undo the last group of edits (TF7).
    Undo,
    /// Redo what was undone.
    Redo,
    /// Cut the selection to the clipboard (the clipboard comes in F2.10c, TF12).
    Cut,
    /// Copy the selection to the clipboard.
    Copy,
    /// Paste the clipboard's text.
    Paste,
}

/// The editing action of `key` pressed with `modifiers` under `convention`, or `None` when the
/// key is not an editing key there (it then bubbles: WB7, TF3).
///
/// Character keys are compared without case, because Shift changes the character a layout
/// reports (`"Z"` for Ctrl+Shift+Z). O(1), no allocation.
#[must_use]
pub fn edit_action(
    key: &Key,
    modifiers: Modifiers,
    convention: EditConvention,
) -> Option<EditAction> {
    match convention {
        EditConvention::Linux | EditConvention::Windows => pc_action(key, modifiers),
        EditConvention::MacOs => mac_action(key, modifiers),
    }
}

/// A movement action.
const fn move_by(direction: Direction, unit: Unit, extend: bool) -> EditAction {
    EditAction::Move {
        direction,
        unit,
        extend,
    }
}

/// A deletion action.
const fn delete_by(direction: Direction, unit: Unit) -> EditAction {
    EditAction::Delete { direction, unit }
}

/// The single lower-case ASCII letter a character key reports, if it reports one.
fn letter(key: &Key) -> Option<char> {
    let Key::Character(text) = key else {
        return None;
    };
    let mut chars = text.chars();
    let first = chars.next()?;
    // A key reporting several characters is not a shortcut letter.
    if chars.next().is_some() {
        return None;
    }
    Some(first.to_ascii_lowercase())
}

/// The table of Linux and Windows (Ctrl is the primary modifier there, ADR-0040 W5).
fn pc_action(key: &Key, modifiers: Modifiers) -> Option<EditAction> {
    use Direction::{Backward, Forward};
    // Alt and the logo key are not editing modifiers on these platforms: such keys bubble.
    if modifiers.alt || modifiers.secondary {
        return None;
    }
    let (ctrl, shift) = (modifiers.primary, modifiers.shift);
    let word_or = |plain: Unit| if ctrl { Unit::Word } else { plain };
    match key {
        Key::Named(NamedKey::ArrowLeft) => Some(move_by(Backward, word_or(Unit::Grapheme), shift)),
        Key::Named(NamedKey::ArrowRight) => Some(move_by(Forward, word_or(Unit::Grapheme), shift)),
        Key::Named(NamedKey::Home) => Some(move_by(Backward, Unit::Line, shift)),
        Key::Named(NamedKey::End) => Some(move_by(Forward, Unit::Line, shift)),
        Key::Named(NamedKey::Backspace) => Some(delete_by(Backward, word_or(Unit::Grapheme))),
        Key::Named(NamedKey::Delete) if shift && !ctrl => Some(EditAction::Cut),
        Key::Named(NamedKey::Delete) => Some(delete_by(Forward, word_or(Unit::Grapheme))),
        Key::Named(NamedKey::Insert) if ctrl && !shift => Some(EditAction::Copy),
        Key::Named(NamedKey::Insert) if shift && !ctrl => Some(EditAction::Paste),
        _ if ctrl => match (letter(key)?, shift) {
            ('a', false) => Some(EditAction::SelectAll),
            ('x', false) => Some(EditAction::Cut),
            ('c', false) => Some(EditAction::Copy),
            ('v', false) => Some(EditAction::Paste),
            ('z', false) => Some(EditAction::Undo),
            ('z', true) | ('y', false) => Some(EditAction::Redo),
            _ => None,
        },
        _ => None,
    }
}

/// The table of macOS (⌘ is the primary modifier, ⌥ is `alt`, ⌃ is the secondary one).
fn mac_action(key: &Key, modifiers: Modifiers) -> Option<EditAction> {
    use Direction::{Backward, Forward};
    // ⌃ is left to the application (no Emacs keys, see the module's documentation).
    if modifiers.secondary {
        return None;
    }
    let (cmd, option, shift) = (modifiers.primary, modifiers.alt, modifiers.shift);
    // ⌘ goes to the line's edge, ⌥ by words; ⌘ wins if both are held, as in AppKit.
    let unit = if cmd {
        Unit::Line
    } else if option {
        Unit::Word
    } else {
        Unit::Grapheme
    };
    match key {
        Key::Named(NamedKey::ArrowLeft) => Some(move_by(Backward, unit, shift)),
        Key::Named(NamedKey::ArrowRight) => Some(move_by(Forward, unit, shift)),
        Key::Named(NamedKey::ArrowUp | NamedKey::Home) => {
            Some(move_by(Backward, Unit::Line, shift))
        }
        Key::Named(NamedKey::ArrowDown | NamedKey::End) => {
            Some(move_by(Forward, Unit::Line, shift))
        }
        Key::Named(NamedKey::Backspace) => Some(delete_by(Backward, unit)),
        Key::Named(NamedKey::Delete) => Some(delete_by(Forward, unit)),
        _ if cmd && !option => match (letter(key)?, shift) {
            ('a', false) => Some(EditAction::SelectAll),
            ('x', false) => Some(EditAction::Cut),
            ('c', false) => Some(EditAction::Copy),
            ('v', false) => Some(EditAction::Paste),
            ('z', false) => Some(EditAction::Undo),
            ('z', true) => Some(EditAction::Redo),
            _ => None,
        },
        _ => None,
    }
}
