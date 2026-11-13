// SPDX-License-Identifier: GPL-3.0-or-later

//! The text field's editing core (ADR-0047, step F2.10b1), headless: the key tables of the three
//! platforms (TF3), typing and pasting (TF2, TF5), movements and deletions on graphemes and words
//! (TF1), the selection, and the local undo with its groups (TF7); a property test of random
//! edit sequences against a plain `String` model, with undo and redo round trips.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test helpers: a failed unwrap or index fails the test"
)]

use orchestra_text::is_grapheme_boundary;
use orchestra_ui::{
    Direction, EditAction, EditConvention, EditResponse, Key, Modifiers, NamedKey, TextEditor,
    Unit, edit_action,
};
use proptest::prelude::*;

use Direction::{Backward, Forward};
use EditConvention::{Linux, MacOs, Windows};

/// Modifiers from a short code: `c` primary (Ctrl, ⌘), `s` shift, `a` alt (⌥), `w` secondary.
fn mods(code: &str) -> Modifiers {
    Modifiers {
        primary: code.contains('c'),
        shift: code.contains('s'),
        alt: code.contains('a'),
        secondary: code.contains('w'),
    }
}

/// A named key.
const fn named(key: NamedKey) -> Key {
    Key::Named(key)
}

/// A character key.
fn ch(text: &str) -> Key {
    Key::Character(text.to_owned())
}

/// A movement.
#[allow(
    clippy::unnecessary_wraps,
    reason = "compared with `edit_action`'s answer, which is an `Option`"
)]
const fn mv(direction: Direction, unit: Unit, extend: bool) -> Option<EditAction> {
    Some(EditAction::Move {
        direction,
        unit,
        extend,
    })
}

/// A deletion.
#[allow(
    clippy::unnecessary_wraps,
    reason = "compared with `edit_action`'s answer, which is an `Option`"
)]
const fn del(direction: Direction, unit: Unit) -> Option<EditAction> {
    Some(EditAction::Delete { direction, unit })
}

#[test]
fn the_linux_and_windows_table() {
    for convention in [Linux, Windows] {
        let action = |key: &Key, code: &str| edit_action(key, mods(code), convention);
        assert_eq!(
            action(&named(NamedKey::ArrowLeft), ""),
            mv(Backward, Unit::Grapheme, false)
        );
        assert_eq!(
            action(&named(NamedKey::ArrowRight), "s"),
            mv(Forward, Unit::Grapheme, true)
        );
        assert_eq!(
            action(&named(NamedKey::ArrowLeft), "c"),
            mv(Backward, Unit::Word, false)
        );
        assert_eq!(
            action(&named(NamedKey::ArrowRight), "cs"),
            mv(Forward, Unit::Word, true)
        );
        assert_eq!(
            action(&named(NamedKey::Home), ""),
            mv(Backward, Unit::Line, false)
        );
        assert_eq!(
            action(&named(NamedKey::End), "s"),
            mv(Forward, Unit::Line, true)
        );
        assert_eq!(
            action(&named(NamedKey::Backspace), ""),
            del(Backward, Unit::Grapheme)
        );
        // Shift+Backspace deletes like Backspace.
        assert_eq!(
            action(&named(NamedKey::Backspace), "s"),
            del(Backward, Unit::Grapheme)
        );
        assert_eq!(
            action(&named(NamedKey::Backspace), "c"),
            del(Backward, Unit::Word)
        );
        assert_eq!(
            action(&named(NamedKey::Delete), ""),
            del(Forward, Unit::Grapheme)
        );
        assert_eq!(
            action(&named(NamedKey::Delete), "c"),
            del(Forward, Unit::Word)
        );
        assert_eq!(action(&named(NamedKey::Delete), "s"), Some(EditAction::Cut));
        assert_eq!(
            action(&named(NamedKey::Insert), "c"),
            Some(EditAction::Copy)
        );
        assert_eq!(
            action(&named(NamedKey::Insert), "s"),
            Some(EditAction::Paste)
        );
        assert_eq!(action(&ch("a"), "c"), Some(EditAction::SelectAll));
        assert_eq!(action(&ch("x"), "c"), Some(EditAction::Cut));
        assert_eq!(action(&ch("c"), "c"), Some(EditAction::Copy));
        assert_eq!(action(&ch("v"), "c"), Some(EditAction::Paste));
        assert_eq!(action(&ch("z"), "c"), Some(EditAction::Undo));
        // Shift makes the layout report a capital.
        assert_eq!(action(&ch("Z"), "cs"), Some(EditAction::Redo));
        assert_eq!(action(&ch("y"), "c"), Some(EditAction::Redo));
        // Not editing keys here: they bubble.
        assert_eq!(action(&named(NamedKey::ArrowUp), ""), None);
        assert_eq!(action(&named(NamedKey::ArrowLeft), "a"), None);
        assert_eq!(action(&named(NamedKey::ArrowLeft), "w"), None);
        assert_eq!(action(&ch("a"), ""), None);
        assert_eq!(action(&ch("s"), "c"), None);
        assert_eq!(action(&named(NamedKey::Enter), ""), None);
        assert_eq!(action(&named(NamedKey::Escape), ""), None);
        assert_eq!(action(&named(NamedKey::Tab), ""), None);
    }
}

#[test]
fn the_macos_table() {
    let action = |key: &Key, code: &str| edit_action(key, mods(code), MacOs);
    // ⌥ for words, ⌘ for the line.
    assert_eq!(
        action(&named(NamedKey::ArrowLeft), "a"),
        mv(Backward, Unit::Word, false)
    );
    assert_eq!(
        action(&named(NamedKey::ArrowRight), "as"),
        mv(Forward, Unit::Word, true)
    );
    assert_eq!(
        action(&named(NamedKey::ArrowLeft), "c"),
        mv(Backward, Unit::Line, false)
    );
    assert_eq!(
        action(&named(NamedKey::ArrowRight), "cs"),
        mv(Forward, Unit::Line, true)
    );
    // ↑ and ↓ to the ends of a one-line field; Home and End too.
    assert_eq!(
        action(&named(NamedKey::ArrowUp), ""),
        mv(Backward, Unit::Line, false)
    );
    assert_eq!(
        action(&named(NamedKey::ArrowDown), "s"),
        mv(Forward, Unit::Line, true)
    );
    assert_eq!(
        action(&named(NamedKey::End), ""),
        mv(Forward, Unit::Line, false)
    );
    assert_eq!(
        action(&named(NamedKey::Backspace), "a"),
        del(Backward, Unit::Word)
    );
    assert_eq!(
        action(&named(NamedKey::Backspace), "c"),
        del(Backward, Unit::Line)
    );
    assert_eq!(
        action(&named(NamedKey::Delete), "a"),
        del(Forward, Unit::Word)
    );
    assert_eq!(action(&ch("a"), "c"), Some(EditAction::SelectAll));
    assert_eq!(action(&ch("z"), "c"), Some(EditAction::Undo));
    assert_eq!(action(&ch("Z"), "cs"), Some(EditAction::Redo));
    // ⌘Y is not redo on macOS; ⌃ keys are left to the application.
    assert_eq!(action(&ch("y"), "c"), None);
    assert_eq!(action(&ch("a"), "w"), None);
    assert_eq!(action(&named(NamedKey::ArrowLeft), "w"), None);
    // Shift+Delete is not cut there.
    assert_eq!(
        action(&named(NamedKey::Delete), "s"),
        del(Forward, Unit::Grapheme)
    );
}

/// An editor with `text` and the caret at `caret`, Linux conventions.
fn editor(text: &str, caret: usize) -> TextEditor {
    let mut editor = TextEditor::new(text, Linux);
    editor.select(caret, caret);
    editor
}

/// Applies a movement.
fn go(editor: &mut TextEditor, direction: Direction, unit: Unit, extend: bool) -> EditResponse {
    editor.apply(EditAction::Move {
        direction,
        unit,
        extend,
    })
}

#[test]
fn typing_inserts_at_the_caret_and_replaces_the_selection() {
    let mut e = editor("Vln", 1);
    assert_eq!(e.type_text("io"), EditResponse::TextChanged);
    assert_eq!(e.text(), "Violn");
    assert_eq!(e.focus(), 3);
    e.select(0, 5);
    e.type_text("Viola");
    assert_eq!(e.text(), "Viola");
    assert_eq!((e.anchor(), e.focus()), (5, 5));
    // Control characters never enter; typing nothing changes nothing.
    assert_eq!(e.type_text("\u{7}"), EditResponse::Unchanged);
    assert_eq!(e.type_text(""), EditResponse::Unchanged);
}

#[test]
fn pasting_and_setting_keep_one_line() {
    let mut e = editor("", 0);
    e.paste("Flute\r\nOboe\nClarinet\rBassoon\tHorn\u{0}");
    assert_eq!(e.text(), "Flute Oboe Clarinet Bassoon Horn");
    e.set_text("A\nB");
    assert_eq!(e.text(), "A B");
    assert_eq!(e.focus(), 3);
    assert!(!e.can_undo(), "setting the text is not an edit");
}

#[test]
fn movements_go_by_graphemes_words_and_lines() {
    // "Crème" holds a two-byte è; "e" + combining acute is one grapheme of three bytes.
    let text = "Crème bru\u{0302}le";
    let mut e = editor(text, 0);
    go(&mut e, Forward, Unit::Grapheme, false);
    go(&mut e, Forward, Unit::Grapheme, false);
    go(&mut e, Forward, Unit::Grapheme, false);
    // Past C, r, è: 1 + 1 + 2 bytes.
    assert_eq!(e.focus(), 4);
    go(&mut e, Forward, Unit::Word, false);
    assert_eq!(e.focus(), "Crème".len());
    go(&mut e, Forward, Unit::Word, false);
    assert_eq!(e.focus(), text.len());
    // Back over "le", then over "u" + combining circumflex as one step.
    go(&mut e, Backward, Unit::Grapheme, false);
    go(&mut e, Backward, Unit::Grapheme, false);
    go(&mut e, Backward, Unit::Grapheme, false);
    assert_eq!(e.focus(), "Crème br".len());
    go(&mut e, Backward, Unit::Word, false);
    assert_eq!(e.focus(), "Crème ".len());
    go(&mut e, Backward, Unit::Line, false);
    assert_eq!(e.focus(), 0);
    // At the start, ← changes nothing.
    assert_eq!(
        go(&mut e, Backward, Unit::Grapheme, false),
        EditResponse::Unchanged
    );
}

#[test]
fn ctrl_right_ends_the_word_on_linux_and_starts_the_next_on_windows() {
    let text = "Violin solo";
    let mut linux = TextEditor::new(text, Linux);
    let mut windows = TextEditor::new(text, Windows);
    linux.select(0, 0);
    windows.select(0, 0);
    go(&mut linux, Forward, Unit::Word, false);
    go(&mut windows, Forward, Unit::Word, false);
    assert_eq!(linux.focus(), 6);
    assert_eq!(windows.focus(), 7);
    // Deleting a word forwards follows the same target.
    linux.select(0, 0);
    windows.select(0, 0);
    linux.apply(EditAction::Delete {
        direction: Forward,
        unit: Unit::Word,
    });
    windows.apply(EditAction::Delete {
        direction: Forward,
        unit: Unit::Word,
    });
    assert_eq!(linux.text(), " solo");
    assert_eq!(windows.text(), "solo");
}

#[test]
fn selections_extend_and_collapse() {
    let mut e = editor("Horn in F", 0);
    go(&mut e, Forward, Unit::Word, true);
    assert_eq!(e.selected_text(), "Horn");
    go(&mut e, Forward, Unit::Grapheme, true);
    assert_eq!(e.selected_text(), "Horn ");
    // → without Shift collapses to the end, ← to the start: no further movement.
    go(&mut e, Forward, Unit::Grapheme, false);
    assert_eq!((e.anchor(), e.focus()), (5, 5));
    e.select(2, 7);
    go(&mut e, Backward, Unit::Grapheme, false);
    assert_eq!(e.focus(), 2);
    // A word movement goes on from the edge: from "in"'s end (7) to the next word's end.
    e.select(2, 7);
    go(&mut e, Forward, Unit::Word, false);
    assert_eq!(e.focus(), 9);
    e.apply(EditAction::SelectAll);
    assert_eq!(e.selected_text(), "Horn in F");
    e.select_word_at(6);
    assert_eq!(e.selected_text(), "in");
    assert_eq!(e.focus(), 7);
}

#[test]
fn deleting_graphemes_words_and_the_selection() {
    let mut e = editor("Cello e\u{301}", 9);
    // The accented e is one grapheme: one Backspace removes both code points.
    e.apply(EditAction::Delete {
        direction: Backward,
        unit: Unit::Grapheme,
    });
    assert_eq!(e.text(), "Cello ");
    e.apply(EditAction::Delete {
        direction: Backward,
        unit: Unit::Word,
    });
    assert_eq!(e.text(), "");
    let mut e = editor("Double bass", 0);
    e.select(0, 7);
    e.apply(EditAction::Delete {
        direction: Forward,
        unit: Unit::Grapheme,
    });
    assert_eq!(e.text(), "bass");
    // Nothing to delete at the end.
    e.select(4, 4);
    assert_eq!(
        e.apply(EditAction::Delete {
            direction: Forward,
            unit: Unit::Grapheme
        }),
        EditResponse::Unchanged
    );
}

#[test]
fn clipboard_actions_are_handed_to_the_widget() {
    let mut e = editor("Harp", 0);
    e.apply(EditAction::SelectAll);
    assert_eq!(
        e.apply(EditAction::Copy),
        EditResponse::Clipboard(EditAction::Copy)
    );
    assert_eq!(e.selected_text(), "Harp");
    // Cut = copy the selected text, then delete the selection.
    assert_eq!(e.delete_selection(), EditResponse::TextChanged);
    assert_eq!(e.text(), "");
    assert_eq!(e.delete_selection(), EditResponse::Unchanged);
}

/// Types `text` one character at a time, as keys arrive.
fn type_slowly(editor: &mut TextEditor, text: &str) {
    for c in text.chars() {
        editor.type_text(c.encode_utf8(&mut [0; 4]));
    }
}

#[test]
fn undo_takes_typing_back_word_by_word() {
    let mut e = editor("", 0);
    type_slowly(&mut e, "Violin solo");
    e.apply(EditAction::Undo);
    assert_eq!(e.text(), "Violin ");
    e.apply(EditAction::Undo);
    assert_eq!(e.text(), "");
    assert_eq!(e.apply(EditAction::Undo), EditResponse::Unchanged);
    e.apply(EditAction::Redo);
    assert_eq!(e.text(), "Violin ");
    e.apply(EditAction::Redo);
    assert_eq!(e.text(), "Violin solo");
    assert_eq!(e.focus(), 11);
    assert_eq!(e.apply(EditAction::Redo), EditResponse::Unchanged);
}

#[test]
fn backspace_groups_mirror_typing() {
    let mut e = editor("Violin solo", 11);
    for _ in 0..11 {
        e.apply(EditAction::Delete {
            direction: Backward,
            unit: Unit::Grapheme,
        });
    }
    assert_eq!(e.text(), "");
    e.apply(EditAction::Undo);
    assert_eq!(e.text(), "Violin ");
    e.apply(EditAction::Undo);
    assert_eq!(e.text(), "Violin solo");
    assert_eq!(e.focus(), 11);
}

#[test]
fn delete_forward_groups_and_movements_seal_a_group() {
    let mut e = editor("ab cd", 0);
    for _ in 0..5 {
        e.apply(EditAction::Delete {
            direction: Forward,
            unit: Unit::Grapheme,
        });
    }
    e.apply(EditAction::Undo);
    assert_eq!(e.text(), "cd");
    e.apply(EditAction::Undo);
    assert_eq!(e.text(), "ab cd");
    // Typing, a movement, typing again: two groups even inside one word.
    let mut e = editor("", 0);
    type_slowly(&mut e, "Ob");
    go(&mut e, Backward, Unit::Grapheme, false);
    go(&mut e, Forward, Unit::Grapheme, false);
    type_slowly(&mut e, "oe");
    e.apply(EditAction::Undo);
    assert_eq!(e.text(), "Ob");
    assert_eq!(e.focus(), 2);
}

#[test]
fn typing_over_a_selection_and_pasting_are_steps_of_their_own() {
    let mut e = editor("Flute", 0);
    e.apply(EditAction::SelectAll);
    type_slowly(&mut e, "Oboe");
    e.paste(" d'amore");
    e.apply(EditAction::Undo);
    assert_eq!(e.text(), "Oboe");
    e.apply(EditAction::Undo);
    assert_eq!(e.text(), "Flute");
    // The selection comes back with the text.
    assert_eq!(e.selected_text(), "Flute");
    // A new edit after undos drops the redo branch.
    e.type_text("X");
    assert!(!e.can_redo());
    assert_eq!(e.text(), "X");
}

#[test]
fn a_combining_mark_typed_after_a_letter_joins_it() {
    let mut e = editor("e", 1);
    e.type_text("\u{301}");
    assert_eq!(e.text(), "e\u{301}");
    assert_eq!(e.focus(), 3);
    go(&mut e, Backward, Unit::Grapheme, false);
    assert_eq!(e.focus(), 0);
    // A regional indicator typed before another one makes a flag with it: the caret, which
    // would fall between the two halves, goes after the whole flag (TF1).
    let mut e = editor("a\u{1F1F9}", 1);
    e.type_text("\u{1F1EE}");
    assert_eq!(e.text(), "a\u{1F1EE}\u{1F1F9}");
    assert_eq!(e.focus(), 9);
    e.apply(EditAction::Undo);
    assert_eq!(e.text(), "a\u{1F1F9}");
    assert_eq!(e.focus(), 1);
}

/// One random step of the property test.
#[derive(Debug, Clone)]
enum Step {
    /// Type one of the alphabet's pieces.
    Type(&'static str),
    /// Paste a piece with a line break.
    Paste,
    /// Move.
    Move(bool, u8, bool),
    /// Delete.
    Delete(bool, u8),
    /// Select all.
    SelectAll,
    /// Select between two arbitrary byte offsets (not necessarily boundaries).
    Select(usize, usize),
}

/// The pieces typed: letters, an accented letter, a combining mark, a space, punctuation, a flag
/// half, digits.
const PIECES: [&str; 9] = ["a", "B", "é", "\u{301}", " ", ",", "\u{1F1EE}", "7", "ss"];

/// A random step.
fn step() -> impl Strategy<Value = Step> {
    let unit = 0u8..3;
    prop_oneof![
        (0..PIECES.len()).prop_map(|i| Step::Type(PIECES[i])),
        Just(Step::Paste),
        (any::<bool>(), unit.clone(), any::<bool>()).prop_map(|(f, u, x)| Step::Move(f, u, x)),
        (any::<bool>(), unit).prop_map(|(f, u)| Step::Delete(f, u)),
        Just(Step::SelectAll),
        (0usize..40, 0usize..40).prop_map(|(a, b)| Step::Select(a, b)),
    ]
}

/// A unit from a number.
const fn unit_of(n: u8) -> Unit {
    match n {
        0 => Unit::Grapheme,
        1 => Unit::Word,
        _ => Unit::Line,
    }
}

/// A direction from a flag.
const fn direction_of(forward: bool) -> Direction {
    if forward { Forward } else { Backward }
}

proptest! {
    /// Random edit sequences keep the invariants (offsets on grapheme boundaries inside the text,
    /// no control character), typing agrees with a plain `String` model, and undoing everything
    /// gives back the starting text while redoing everything gives back the final one.
    #[test]
    fn random_edits_keep_the_invariants_and_undo_exactly(
        start in "[a-z ]{0,8}",
        steps in proptest::collection::vec(step(), 0..40),
        convention in prop_oneof![Just(Linux), Just(Windows), Just(MacOs)],
    ) {
        let mut e = TextEditor::new(&start, convention);
        let mut edits = 0;
        for step in &steps {
            let before = (e.text().to_owned(), e.selection());
            let response = match *step {
                Step::Type(piece) => e.type_text(piece),
                Step::Paste => e.paste("x\ny"),
                Step::Move(f, u, x) => e.apply(EditAction::Move {
                    direction: direction_of(f),
                    unit: unit_of(u),
                    extend: x,
                }),
                Step::Delete(f, u) => e.apply(EditAction::Delete {
                    direction: direction_of(f),
                    unit: unit_of(u),
                }),
                Step::SelectAll => e.apply(EditAction::SelectAll),
                Step::Select(a, b) => e.select(a, b),
            };
            if let Step::Type(piece) = *step {
                // Typing replaces the selection with the piece (then the caret snaps).
                let (text, range) = &before;
                let expected = format!(
                    "{}{}{}",
                    text.get(..range.start).unwrap(),
                    piece,
                    text.get(range.end..).unwrap()
                );
                prop_assert_eq!(e.text(), expected.as_str());
            }
            if response == EditResponse::TextChanged {
                edits += 1;
            }
            let text = e.text();
            prop_assert!(e.anchor() <= text.len() && e.focus() <= text.len());
            prop_assert!(is_grapheme_boundary(text, e.anchor()), "anchor {} in {:?}", e.anchor(), text);
            prop_assert!(is_grapheme_boundary(text, e.focus()), "focus {} in {:?}", e.focus(), text);
            prop_assert!(!text.chars().any(char::is_control));
        }
        let end = e.text().to_owned();
        // Undo everything (each undo takes back at least one edit).
        for _ in 0..=edits {
            e.apply(EditAction::Undo);
        }
        prop_assert_eq!(e.text(), start.as_str());
        prop_assert!(!e.can_undo());
        for _ in 0..=edits {
            e.apply(EditAction::Redo);
        }
        prop_assert_eq!(e.text(), end.as_str());
        prop_assert!(!e.can_redo());
    }
}
