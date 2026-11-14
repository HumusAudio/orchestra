// SPDX-License-Identifier: GPL-3.0-or-later

//! The field's own undo history (ADR-0047 TF7): edits grouped deterministically, by words and by
//! kinds of edit, never by time.
//!
//! # What an edit is
//! Every change of the text is one *edit*: at byte `at`, the text `removed` was replaced by the
//! text `inserted` (typing removes nothing, deleting inserts nothing, typing over a selection
//! does both). Undoing an edit puts `removed` back; redoing it puts `inserted` again. Each edit
//! also keeps the selection before and after it, so undo and redo restore the caret too.
//!
//! # Groups
//! Edits belong to **groups**; Undo and Redo act on a whole group. A new edit joins the open
//! group when it continues it:
//! - same kind (typing, deleting backwards, deleting forwards; anything else, such as a paste, a
//!   cut or a deleted word, is a group of its own);
//! - at the place where the last edit left the caret (no movement in between: any movement or
//!   selection change *seals* the group, [`History::seal`]);
//! - and without crossing into a new word: typing a letter after a space starts a new group, so
//!   "Violin solo" typed in one go undoes as "solo", then "Violin " (the space belongs to the
//!   word before it, as in word processors). Deleting mirrors it, so an undo brings back the same
//!   pieces typing made: Backspace over "solo" is one group, and the space with "Violin" before it
//!   is the next (the deleted space starts a new group, the letters before it join it).
//!
//! Grouping by time ("edits within 500 ms") is rejected (ADR-0026 rejects it for the document's
//! history): the result would depend on how fast someone types, and no test could pin it.
//!
//! # Storage: one arena, no allocation once reserved
//! The removed and inserted texts of every edit live one after the other in one `String`, the
//! arena; an edit keeps byte ranges into it. Typing that continues a group appends to the arena
//! and widens the last edit's range, so a word typed letter by letter is one edit, not one per
//! letter. A new edit after some undos drops the undone edits and their bytes (the redo branch)
//! by truncating both vectors. With capacity reserved ([`History::reserve`]) nothing allocates;
//! otherwise the vectors grow by doubling, which ADR-0047 TF13 allows (the history is not a hot
//! path).
//!
//! # Complexity
//! Recording is O(length of the edit's texts) (copying them into the arena) plus O(1); undo and
//! redo are O(text length) per edit of the group (`String::replace_range` moves the bytes after
//! the edit).

use std::ops::Range;

/// The kind of an edit, for grouping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EditKind {
    /// Typed text (also typing over a selection, which starts a group).
    Typing,
    /// One grapheme deleted before the caret (Backspace).
    DeleteBackward,
    /// One grapheme deleted after the caret (Delete).
    DeleteForward,
    /// Anything else: a paste, a cut, a deleted word or line, a deleted selection. Always a group
    /// of its own.
    Other,
}

/// A selection: (anchor, focus), byte offsets.
pub(crate) type Selection = (usize, usize);

/// One recorded edit.
#[derive(Debug, Clone)]
struct Edit {
    /// Where in the text the replacement happened, bytes.
    at: usize,
    /// The removed text, as a range of the arena.
    removed: Range<usize>,
    /// The inserted text, as a range of the arena (always after `removed`).
    inserted: Range<usize>,
    /// The selection before the edit.
    before: Selection,
    /// The selection after it.
    after: Selection,
    /// The group it belongs to.
    group: u32,
    /// Its kind.
    kind: EditKind,
}

/// The undo history of one field (see the module's documentation).
#[derive(Debug, Clone, Default)]
pub(crate) struct History {
    /// The removed and inserted texts of every edit, one after the other.
    arena: String,
    /// The edits, oldest first: `edits[..done]` are applied, the rest were undone (redo branch).
    edits: Vec<Edit>,
    /// How many edits are applied.
    done: usize,
    /// The next group's number.
    next_group: u32,
    /// `false` after a movement, an undo or a redo: the next edit starts a new group.
    open: bool,
}

impl History {
    /// Reserves room for `bytes` of edited text and `edits` edits, so recording them allocates
    /// nothing.
    pub(crate) fn reserve(&mut self, bytes: usize, edits: usize) {
        self.arena.reserve(bytes);
        self.edits.reserve(edits);
    }

    /// Forgets everything (the field lost the focus, or its text was replaced by the
    /// application), keeping the capacity.
    pub(crate) fn clear(&mut self) {
        self.arena.clear();
        self.edits.clear();
        self.done = 0;
        self.open = false;
    }

    /// Closes the open group: the next edit starts a new one. Called on every movement and
    /// selection change.
    pub(crate) const fn seal(&mut self) {
        self.open = false;
    }

    /// `true` when there is something to undo.
    pub(crate) const fn can_undo(&self) -> bool {
        self.done > 0
    }

    /// `true` when there is something to redo.
    pub(crate) const fn can_redo(&self) -> bool {
        self.done < self.edits.len()
    }

    /// Records that at `at` the text `removed` was replaced by `inserted`, moving the selection
    /// from `before` to `after`, as an edit of `kind` (see the module's documentation for how it
    /// joins a group).
    pub(crate) fn record(
        &mut self,
        at: usize,
        removed: &str,
        inserted: &str,
        (before, after): (Selection, Selection),
        kind: EditKind,
    ) {
        // A new edit after undos drops the redo branch, its edits and their bytes.
        if self.done < self.edits.len() {
            self.edits.truncate(self.done);
            let end = self.edits.last().map_or(0, |edit| edit.inserted.end);
            self.arena.truncate(end);
            self.open = false;
        }
        if self.open
            && let Some(last) = self.edits.last_mut()
            && let Some(merged) = merge(&mut self.arena, last, at, removed, inserted, kind)
        {
            last.after = after;
            if merged {
                return;
            }
            // Same group, but a new edit (a deletion's bytes come before the last ones).
            let group = last.group;
            self.push(at, removed, inserted, (before, after), kind, group);
            return;
        }
        let group = self.next_group;
        self.next_group = self.next_group.wrapping_add(1);
        self.push(at, removed, inserted, (before, after), kind, group);
        // Only the kinds that continue (typing and single deletions) leave the group open.
        self.open = kind != EditKind::Other;
    }

    /// Sets the selection after the last recorded edit: the editor knows the caret's final place
    /// only once the edit is complete (a grapheme boundary of the new text).
    pub(crate) fn amend_after(&mut self, after: Selection) {
        if let Some(last) = self.edits.last_mut() {
            last.after = after;
        }
    }

    /// Appends one edit to the arena and the list.
    fn push(
        &mut self,
        at: usize,
        removed: &str,
        inserted: &str,
        (before, after): (Selection, Selection),
        kind: EditKind,
        group: u32,
    ) {
        let start = self.arena.len();
        self.arena.push_str(removed);
        let middle = self.arena.len();
        self.arena.push_str(inserted);
        self.edits.push(Edit {
            at,
            removed: start..middle,
            inserted: middle..self.arena.len(),
            before,
            after,
            group,
            kind,
        });
        self.done = self.edits.len();
    }

    /// Undoes the last group on `text`, returning the selection to restore, or `None` when
    /// there is nothing to undo.
    pub(crate) fn undo(&mut self, text: &mut String) -> Option<Selection> {
        let group = self.edits.get(self.done.checked_sub(1)?)?.group;
        let mut selection = None;
        while let Some(edit) = self
            .done
            .checked_sub(1)
            .and_then(|index| self.edits.get(index))
            .filter(|edit| edit.group == group)
        {
            let inserted = edit.at..edit.at + edit.inserted.len();
            let removed = self.arena.get(edit.removed.clone()).unwrap_or_default();
            text.replace_range(inserted, removed);
            selection = Some(edit.before);
            self.done -= 1;
        }
        self.open = false;
        selection
    }

    /// Redoes the next undone group on `text`, returning the selection after it, or `None` when
    /// there is nothing to redo.
    pub(crate) fn redo(&mut self, text: &mut String) -> Option<Selection> {
        let group = self.edits.get(self.done)?.group;
        let mut selection = None;
        while let Some(edit) = self.edits.get(self.done).filter(|edit| edit.group == group) {
            let removed = edit.at..edit.at + edit.removed.len();
            let inserted = self.arena.get(edit.inserted.clone()).unwrap_or_default();
            text.replace_range(removed, inserted);
            selection = Some(edit.after);
            self.done += 1;
        }
        self.open = false;
        selection
    }
}

/// Tries to add a new edit to the open group ending with `last`. Returns `None` when it does not
/// continue the group, `Some(true)` when it was merged into `last` itself (typing appended to its
/// inserted text, or a forward deletion appended to its removed text), and `Some(false)` when it
/// belongs to the group but must be a new edit (a backward deletion, whose bytes come before
/// `last`'s in the text).
fn merge(
    arena: &mut String,
    last: &mut Edit,
    at: usize,
    removed: &str,
    inserted: &str,
    kind: EditKind,
) -> Option<bool> {
    if kind != last.kind || kind == EditKind::Other {
        return None;
    }
    let last_inserted = arena.get(last.inserted.clone()).unwrap_or_default();
    let last_removed = arena.get(last.removed.clone()).unwrap_or_default();
    match kind {
        EditKind::Typing => {
            // Continues where the last typing ended, removing nothing, and not starting a new
            // word after a space.
            let continues = removed.is_empty() && at == last.at + last.inserted.len();
            let new_word = ends_with_space(last_inserted) && !starts_with_space(inserted);
            if !continues || new_word {
                return None;
            }
            // `last.inserted` ends the arena (it is the last edit, and the redo branch was
            // dropped), so appending widens it.
            arena.push_str(inserted);
            last.inserted.end = arena.len();
            Some(true)
        }
        EditKind::DeleteForward => {
            // Deleting forwards from the same place; a new word starts when the last deletion
            // ended with a space and this one does not start with one (the mirror of typing).
            let continues = inserted.is_empty() && at == last.at;
            let new_word = ends_with_space(last_removed) && !starts_with_space(removed);
            if !continues || new_word {
                return None;
            }
            // `last.removed` is followed by `last.inserted` in the arena, which is empty for a
            // deletion, so the removed text ends the arena too.
            arena.push_str(removed);
            last.removed.end = arena.len();
            last.inserted = arena.len()..arena.len();
            Some(true)
        }
        EditKind::DeleteBackward => {
            // Deleting backwards just before the last deletion; a new word starts when the
            // deleted grapheme is a space and the group so far began with a letter.
            let continues = inserted.is_empty() && at + removed.len() == last.at;
            let new_word = starts_with_space(removed) && !starts_with_space(last_removed);
            if !continues || new_word {
                return None;
            }
            Some(false)
        }
        EditKind::Other => None,
    }
}

/// `true` when `text` ends with white space.
fn ends_with_space(text: &str) -> bool {
    text.chars().next_back().is_some_and(char::is_whitespace)
}

/// `true` when `text` starts with white space.
fn starts_with_space(text: &str) -> bool {
    text.chars().next().is_some_and(char::is_whitespace)
}
