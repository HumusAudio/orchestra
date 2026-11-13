// SPDX-License-Identifier: GPL-3.0-or-later

//! The editing core of the text field (ADR-0047, step F2.10b1): the text, the caret and the
//! selection, the editing actions of the platform's keys, typing, pasting and the field's own
//! undo history. No drawing, no window: the widget of F2.10b2 drives a [`TextEditor`] and draws
//! what it says, and every behaviour here is tested headless.
//!
//! - `keys`: from a key to an [`EditAction`], per platform ([`EditConvention`]);
//! - `history`: the undo history, grouped by words and kinds of edit (TF7);
//! - this file: [`TextEditor`].
//!
//! # Invariants
//! - The text holds one line: no line break nor other control character ever enters it (TF2):
//!   typed text has none (ADR-0047 TF5), pasted text has its line breaks turned into spaces and
//!   its other control characters dropped.
//! - The anchor and the focus are always grapheme boundaries of the text (TF1), in `0..=len`.
//!   Every operation keeps them so: movements go from boundary to boundary, and after an edit
//!   the caret is brought to the boundary at or after it (an inserted combining accent can
//!   merge with the letter before the caret, moving a boundary).
//!
//! # Allocation
//! Once [`TextEditor::reserve`] gave the text and the history their capacity, typing, moving,
//! deleting, undoing and redoing allocate nothing (TF13, tested with the allocation counter):
//! boundaries come from `orchestra-text`'s iterators, edits are recorded into the history's
//! arena.

pub(crate) mod history;
pub(crate) mod keys;

use std::ops::Range;

use orchestra_text::{
    ceil_grapheme, next_grapheme, next_word_end, next_word_start, prev_grapheme, prev_word_start,
    word_at,
};

use history::{EditKind, History};
use keys::{Direction, EditAction, EditConvention, Unit};

/// What an action did, so the widget knows what to redraw and what to tell the application.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditResponse {
    /// Nothing changed (a movement at the end, Undo with nothing to undo…).
    Unchanged,
    /// The caret or the selection moved; the text is the same.
    SelectionChanged,
    /// The text changed (and usually the selection too).
    TextChanged,
    /// A clipboard action: the editor does not own the clipboard (F2.10c, TF12), so the widget
    /// carries it out with [`TextEditor::selected_text`], [`TextEditor::delete_selection`] and
    /// [`TextEditor::paste`].
    Clipboard(EditAction),
}

/// The text of a one-line field with its caret, selection and undo history (ADR-0047).
///
/// ```
/// use orchestra_ui::{EditAction, EditConvention, Key, Modifiers, NamedKey, TextEditor, edit_action};
///
/// let mut editor = TextEditor::new("Violin", EditConvention::Linux);
/// editor.type_text(" solo");
/// let key = Key::Named(NamedKey::ArrowLeft);
/// let primary = Modifiers { primary: true, ..Modifiers::default() };
/// if let Some(action) = edit_action(&key, primary, editor.convention()) {
///     editor.apply(action); // Ctrl+← on Linux: to the start of "solo"
/// }
/// assert_eq!(editor.focus(), 7);
/// editor.apply(EditAction::Undo); // takes back " solo", one word
/// assert_eq!(editor.text(), "Violin");
/// ```
#[derive(Debug, Clone)]
pub struct TextEditor {
    /// The text (one line, no control characters).
    text: String,
    /// Where the selection started, bytes.
    anchor: usize,
    /// Where the caret is (the selection's moving end), bytes.
    focus: usize,
    /// The field's own undo history (TF7).
    history: History,
    /// The platform's conventions (TF3).
    convention: EditConvention,
}

impl TextEditor {
    /// An editor holding `text` (cleaned as a paste would be: line breaks become spaces, other
    /// control characters go), with the caret at its end and an empty history.
    #[must_use]
    pub fn new(text: &str, convention: EditConvention) -> Self {
        let mut editor = Self {
            text: String::with_capacity(text.len()),
            anchor: 0,
            focus: 0,
            history: History::default(),
            convention,
        };
        editor.set_text(text);
        editor
    }

    /// Reserves room for a text of `text_bytes`, and for `history_bytes` of edited text in
    /// `history_edits` edits, so editing within those limits allocates nothing.
    pub fn reserve(&mut self, text_bytes: usize, history_bytes: usize, history_edits: usize) {
        self.text
            .reserve(text_bytes.saturating_sub(self.text.len()));
        self.history.reserve(history_bytes, history_edits);
    }

    /// The text.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Where the selection started (equal to [`TextEditor::focus`] when nothing is selected).
    #[must_use]
    pub const fn anchor(&self) -> usize {
        self.anchor
    }

    /// Where the caret is.
    #[must_use]
    pub const fn focus(&self) -> usize {
        self.focus
    }

    /// The selected bytes, in text order (empty when nothing is selected).
    #[must_use]
    pub fn selection(&self) -> Range<usize> {
        self.anchor.min(self.focus)..self.anchor.max(self.focus)
    }

    /// The selected text.
    #[must_use]
    pub fn selected_text(&self) -> &str {
        self.text.get(self.selection()).unwrap_or_default()
    }

    /// The conventions this editor follows.
    #[must_use]
    pub const fn convention(&self) -> EditConvention {
        self.convention
    }

    /// `true` when Undo would change something.
    #[must_use]
    pub const fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    /// `true` when Redo would change something.
    #[must_use]
    pub const fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    /// Replaces the whole text (the application set it, or Escape restored it, TF7): cleaned as
    /// a paste, the caret at the end, the history forgotten. Not an edit: nothing to undo.
    pub fn set_text(&mut self, text: &str) {
        self.text.clear();
        self.text.extend(cleaned(text));
        self.anchor = self.text.len();
        self.focus = self.text.len();
        self.history.clear();
    }

    /// Forgets the undo history (the field lost the focus: its history ends there, TF7).
    pub fn clear_history(&mut self) {
        self.history.clear();
    }

    /// Carries out `action` (from [`edit_action`](keys::edit_action), or from a menu).
    pub fn apply(&mut self, action: EditAction) -> EditResponse {
        match action {
            EditAction::Move {
                direction,
                unit,
                extend,
            } => self.move_caret(direction, unit, extend),
            EditAction::Delete { direction, unit } => self.delete(direction, unit),
            EditAction::SelectAll => self.select(0, self.text.len()),
            EditAction::Undo => self.undo(),
            EditAction::Redo => self.redo(),
            EditAction::Cut | EditAction::Copy | EditAction::Paste => {
                EditResponse::Clipboard(action)
            }
        }
    }

    /// Selects from `anchor` to `focus` (each brought to a grapheme boundary at or after it,
    /// inside the text): a click (both equal), a drag, a double click's word, or an assistive
    /// technology's selection (TF4, TF11).
    pub fn select(&mut self, anchor: usize, focus: usize) -> EditResponse {
        let anchor = ceil_grapheme(&self.text, anchor);
        let focus = ceil_grapheme(&self.text, focus);
        self.history.seal();
        if (anchor, focus) == (self.anchor, self.focus) {
            return EditResponse::Unchanged;
        }
        self.anchor = anchor;
        self.focus = focus;
        EditResponse::SelectionChanged
    }

    /// Selects the word (or the run of spaces, or the punctuation) containing the grapheme that
    /// starts at `offset`: a double click (TF4). The caret goes to the word's end.
    pub fn select_word_at(&mut self, offset: usize) -> EditResponse {
        let word = word_at(&self.text, offset);
        self.select(word.start, word.end)
    }

    /// Types `text` at the caret, replacing the selection (TF5, TF7). Control characters are
    /// dropped; typing nothing changes nothing.
    pub fn type_text(&mut self, text: &str) -> EditResponse {
        // A selection replaced by typing starts a new group; typing on continues it.
        self.replace_selection(text, EditKind::Typing)
    }

    /// Pastes `text` at the caret, replacing the selection: its line breaks (`\r\n`, `\r`, `\n`)
    /// and tabs become spaces, its other control characters go (TF2). One undo step of its own.
    pub fn paste(&mut self, text: &str) -> EditResponse {
        self.replace_selection(text, EditKind::Other)
    }

    /// Deletes the selection (for Cut, after the widget copied it), one undo step of its own.
    pub fn delete_selection(&mut self) -> EditResponse {
        if self.anchor == self.focus {
            return EditResponse::Unchanged;
        }
        self.replace_selection("", EditKind::Other)
    }

    /// Moves the caret by `unit` in `direction`; with `extend` the anchor stays.
    ///
    /// Without `extend`, a selection first collapses to its edge in `direction` (← to its start,
    /// → to its end); a grapheme movement stops there (the platform's convention everywhere),
    /// a word or line movement goes on from that edge.
    fn move_caret(&mut self, direction: Direction, unit: Unit, extend: bool) -> EditResponse {
        let selection = self.selection();
        if !extend && !selection.is_empty() {
            let edge = match direction {
                Direction::Backward => selection.start,
                Direction::Forward => selection.end,
            };
            let target = if unit == Unit::Grapheme {
                edge
            } else {
                self.target(edge, direction, unit)
            };
            return self.select(target, target);
        }
        let target = self.target(self.focus, direction, unit);
        let anchor = if extend { self.anchor } else { target };
        self.select(anchor, target)
    }

    /// Where a movement by `unit` in `direction` from `from` goes.
    fn target(&self, from: usize, direction: Direction, unit: Unit) -> usize {
        let text = &self.text;
        match (direction, unit) {
            (Direction::Backward, Unit::Grapheme) => prev_grapheme(text, from),
            (Direction::Forward, Unit::Grapheme) => next_grapheme(text, from),
            (Direction::Backward, Unit::Word) => prev_word_start(text, from),
            (Direction::Forward, Unit::Word) if self.convention.word_forward_to_start() => {
                next_word_start(text, from)
            }
            (Direction::Forward, Unit::Word) => next_word_end(text, from),
            (Direction::Backward, Unit::Line) => 0,
            (Direction::Forward, Unit::Line) => text.len(),
        }
    }

    /// Deletes the selection, or, with nothing selected, from the caret by `unit`.
    fn delete(&mut self, direction: Direction, unit: Unit) -> EditResponse {
        if self.anchor != self.focus {
            return self.replace_selection("", EditKind::Other);
        }
        let target = self.target(self.focus, direction, unit);
        if target == self.focus {
            return EditResponse::Unchanged;
        }
        // One grapheme groups with the deletions next to it; a word or a line is a step alone.
        let kind = match (direction, unit) {
            (Direction::Backward, Unit::Grapheme) => EditKind::DeleteBackward,
            (Direction::Forward, Unit::Grapheme) => EditKind::DeleteForward,
            _ => EditKind::Other,
        };
        // Select what goes, then replace it with nothing: one path for every edit. The selection
        // recorded as "before" is the caret, not this temporary range.
        let before = (self.anchor, self.focus);
        let range = self.focus.min(target)..self.focus.max(target);
        self.edit(range, "", kind, before)
    }

    /// Replaces the selection with `text` (cleaned), as an edit of `kind`.
    fn replace_selection(&mut self, text: &str, kind: EditKind) -> EditResponse {
        let before = (self.anchor, self.focus);
        let range = self.selection();
        // Typing over a selection starts a new group (it removes something), and continues
        // like typing afterwards.
        if !range.is_empty() {
            self.history.seal();
        }
        self.edit(range, text, kind, before)
    }

    /// The one place where the text changes: `range` becomes `inserted` (cleaned, see
    /// [`cleaned`]), the caret goes after it, and the edit is recorded.
    ///
    /// # How, without a temporary string
    /// The cleaned text is first inserted **after** `range`, so for a moment the text holds both
    /// the removed bytes and the inserted ones side by side: the history copies both straight
    /// from there into its arena. Then `range` is removed. Each step moves only the bytes after
    /// the edit (O(tail length) per inserted character; a one-line field's tail is short), and
    /// nothing allocates while the text has capacity.
    fn edit(
        &mut self,
        range: Range<usize>,
        inserted: &str,
        kind: EditKind,
        before: (usize, usize),
    ) -> EditResponse {
        let old_len = self.text.len();
        let mut at = range.end;
        for c in cleaned(inserted) {
            self.text.insert(at, c);
            at += c.len_utf8();
        }
        let added = self.text.len() - old_len;
        if range.is_empty() && added == 0 {
            return EditResponse::Unchanged;
        }
        let removed = self.text.get(range.clone()).unwrap_or_default();
        let new_text = self.text.get(range.end..at).unwrap_or_default();
        // The caret's place is only known once the edit is complete (below): recorded now with
        // a provisional one, amended after.
        let provisional = range.start + added;
        self.history.record(
            range.start,
            removed,
            new_text,
            (before, (provisional, provisional)),
            kind,
        );
        self.text.replace_range(range.clone(), "");
        // An inserted combining mark may have joined the grapheme after the caret's place: the
        // caret goes to the boundary at or after it (TF1).
        let caret = ceil_grapheme(&self.text, range.start + added);
        self.history.amend_after((caret, caret));
        self.anchor = caret;
        self.focus = caret;
        EditResponse::TextChanged
    }

    /// Undoes the last group of edits (TF7).
    fn undo(&mut self) -> EditResponse {
        match self.history.undo(&mut self.text) {
            Some((anchor, focus)) => {
                self.anchor = anchor;
                self.focus = focus;
                EditResponse::TextChanged
            }
            None => EditResponse::Unchanged,
        }
    }

    /// Redoes the last undone group.
    fn redo(&mut self) -> EditResponse {
        match self.history.redo(&mut self.text) {
            Some((anchor, focus)) => {
                self.anchor = anchor;
                self.focus = focus;
                EditResponse::TextChanged
            }
            None => EditResponse::Unchanged,
        }
    }
}

/// The characters of `text` as the field keeps them (TF2): each line break (`\r\n`, `\r` or
/// `\n`) and each tab becomes one space, every other control character is dropped. A lazy
/// iterator: no allocation.
fn cleaned(text: &str) -> impl Iterator<Item = char> + '_ {
    // `after_cr` remembers a `\r`, so the `\n` of `\r\n` is not a second space.
    text.chars()
        .scan(false, |after_cr, c| {
            let was_cr = std::mem::replace(after_cr, c == '\r');
            Some(match c {
                '\n' if was_cr => None,
                '\r' | '\n' | '\t' => Some(' '),
                c if c.is_control() => None,
                c => Some(c),
            })
        })
        .flatten()
}
