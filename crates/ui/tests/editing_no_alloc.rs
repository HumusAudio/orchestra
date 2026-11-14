// SPDX-License-Identifier: GPL-3.0-or-later

//! Editing does not allocate (ADR-0047 TF13): once the text and the undo history have their
//! capacity ([`TextEditor::reserve`]), typing, moving by graphemes, words and lines, selecting,
//! deleting, pasting, undoing and redoing never touch the heap. The boundaries come from
//! `orchestra-text`'s segmenter iterators, which must not allocate either: this test covers them
//! too.
//!
//! This file is its own test binary with the counting allocator installed (ADR-0016 §5).

use std::hint::black_box;

use orchestra_test_support::assert_no_allocations;
use orchestra_ui::{Direction, EditAction, EditConvention, TextEditor, Unit};

orchestra_test_support::install_counting_allocator!();

/// One round of editing: what a user does in a name field, keys as they arrive.
fn round(editor: &mut TextEditor) {
    for c in "Violin I, solo é".chars() {
        let mut buffer = [0; 4];
        black_box(editor.type_text(c.encode_utf8(&mut buffer)));
    }
    for (direction, unit) in [
        (Direction::Backward, Unit::Word),
        (Direction::Backward, Unit::Grapheme),
        (Direction::Forward, Unit::Word),
        (Direction::Backward, Unit::Line),
        (Direction::Forward, Unit::Line),
    ] {
        black_box(editor.apply(EditAction::Move {
            direction,
            unit,
            extend: false,
        }));
    }
    black_box(editor.apply(EditAction::Move {
        direction: Direction::Backward,
        unit: Unit::Word,
        extend: true,
    }));
    black_box(editor.apply(EditAction::Delete {
        direction: Direction::Backward,
        unit: Unit::Grapheme,
    }));
    black_box(editor.apply(EditAction::Delete {
        direction: Direction::Backward,
        unit: Unit::Word,
    }));
    black_box(editor.paste("tutti\nsoli"));
    black_box(editor.select_word_at(2));
    black_box(editor.type_text("Viola"));
    for _ in 0..6 {
        black_box(editor.apply(EditAction::Undo));
    }
    for _ in 0..3 {
        black_box(editor.apply(EditAction::Redo));
    }
    black_box(editor.apply(EditAction::SelectAll));
    black_box(editor.apply(EditAction::Delete {
        direction: Direction::Forward,
        unit: Unit::Grapheme,
    }));
}

#[test]
fn editing_never_allocates_within_the_reserved_capacity() {
    for convention in [
        EditConvention::Linux,
        EditConvention::Windows,
        EditConvention::MacOs,
    ] {
        let mut editor = TextEditor::new("", convention);
        // Room for the text and for a few rounds of history.
        editor.reserve(256, 4096, 512);
        round(&mut editor);
        assert_no_allocations(|| round(&mut editor));
    }
}
