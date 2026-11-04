// SPDX-License-Identifier: GPL-3.0-or-later

//! The editor: the document with its one undo history, and the transactions that change it
//! (ADR-0003 §4, ADR-0026 §2–§3; SCALETTA F1.11b).
//!
//! # Transactions
//! Every change runs inside a **transaction** ([`Editor::transact`]): a sequence of primitive
//! edits (`crate::edit`) that becomes **one step of undo**, whatever its size (pasting 200 notes,
//! deleting 40 bars). If the closure returns an error, the edits already applied are reverted in
//! reverse order before the error is returned: the document is never left half-changed.
//!
//! Derived data is brought up to date once, when the transaction ends: the tempo map is derived
//! again from the tempo marks (ADR-0024 §11) only if a tempo mark or a bar start changed; the
//! other indices are kept up to date by each edit. A tempo map that fails to derive (overflow,
//! absurd tempi) fails the transaction, which is then reverted.
//!
//! # The history
//! Linear (ADR-0026 U7): undo and redo walk one list, and a new transaction after some undos
//! discards the redo steps. Each step is the list of edits that undoes it; applying them returns
//! the list that redoes it, and the other way round, so undo and redo are one mechanism.
//!
//! Bounded by **memory** (U3): every step records an estimate of the memory its edits hold, and
//! when the total passes the limit ([`DEFAULT_HISTORY_LIMIT`], 128 MB, unless set otherwise) the
//! oldest steps are dropped. The most recent step is always kept, however large. The history is
//! not saved in the project file (U6).
//!
//! # What is not here yet
//! Gestures (a drag as one open transaction, U4) and the commands that express what the user asks
//! for come with the second part of F1.11b; this module gives them the transaction they build on.

use std::collections::VecDeque;
use std::error::Error;
use std::fmt;

use orchestra_core_time::{Position, TempoMap};

use crate::change::ChangeSet;
use crate::document::{Document, DocumentError};
use crate::edit::Edit;

/// The default memory limit of the history: 128 MB (ADR-0026 §3, U3). Entering a note costs tens
/// of bytes of history; deleting the whole heavy reference score (255 776 notes) about 66 MB.
pub const DEFAULT_HISTORY_LIMIT: usize = 128 * 1024 * 1024;

/// Why a transaction, an undo or a redo failed.
///
/// The `Display` texts are English and meant for developers; the interface maps each variant to a
/// Fluent key (CLAUDE.md §7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CommandError {
    /// An edit was refused, or the tempo map could not be derived at the end. The document is
    /// unchanged.
    Document(DocumentError),
    /// A transaction was started inside another one.
    TransactionOpen,
    /// Reverting a failed transaction (or a failed undo or redo) failed too. The first error is
    /// `cause`, the second `rollback`. Never expected: every edit is reverted by its own exact
    /// inverse, on the very state it produced, and the tests check it. Reported rather than
    /// hidden, because the document may then differ from before.
    Rollback {
        /// Why the transaction failed.
        cause: DocumentError,
        /// Why reverting it failed.
        rollback: DocumentError,
    },
}

impl fmt::Display for CommandError {
    /// Writes a short English description of the error, for developers.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Document(e) => write!(f, "{e}"),
            Self::TransactionOpen => f.write_str("a transaction is already open"),
            Self::Rollback { cause, rollback } => {
                write!(f, "{cause}; reverting it failed too: {rollback}")
            }
        }
    }
}

impl Error for CommandError {
    /// The underlying document error, if any.
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Document(e) | Self::Rollback { cause: e, .. } => Some(e),
            Self::TransactionOpen => None,
        }
    }
}

impl From<DocumentError> for CommandError {
    /// Wraps a refused edit, so that `?` works inside a transaction.
    fn from(e: DocumentError) -> Self {
        Self::Document(e)
    }
}

/// One step of the history: the edits that undo (or redo) it, in the order they are applied, and
/// the estimate of the memory they hold.
#[derive(Debug, Clone)]
struct Step {
    /// The edits, applied first to last.
    edits: Vec<Edit>,
    /// Their memory estimate in bytes ([`Edit::heap_bytes`], summed).
    bytes: usize,
}

impl Step {
    /// A step from edits listed in the order they were **applied**; the step holds their inverses
    /// in the order that undoes them (last applied, first undone). O(edits).
    fn undoing(applied_inverses: Vec<Edit>) -> Self {
        let mut edits = applied_inverses;
        edits.reverse();
        let bytes = edits
            .iter()
            .map(Edit::heap_bytes)
            .fold(0, usize::saturating_add);
        Self { edits, bytes }
    }
}

/// The open transaction: the inverses of the edits applied so far, what they changed, and the
/// tempo map from before (to tell where the rebuilt one differs).
#[derive(Debug)]
struct Open {
    /// The inverse of every edit applied, in the order the edits were applied.
    inverses: Vec<Edit>,
    /// What the edits changed.
    changes: ChangeSet,
    /// The tempo map when the transaction started.
    tempo_before: TempoMap,
}

/// The one project document with its undo history (ADR-0003 §4, ADR-0026).
///
/// It is the only way to change a document once built: every change is a transaction of edits,
/// and every transaction is a step of undo. It belongs to the editing thread (ADR-0022 §7);
/// background readers get snapshots of [`document`](Self::document).
#[derive(Debug)]
pub struct Editor {
    /// The document.
    document: Document,
    /// The steps that undo the last transactions, oldest first.
    undo: VecDeque<Step>,
    /// The steps that redo the undone transactions, the next one to redo last.
    redo: Vec<Step>,
    /// The memory held by every step of `undo` and `redo`, in bytes.
    bytes: usize,
    /// The memory limit of the history, in bytes.
    limit: usize,
    /// The transaction in progress, if any.
    open: Option<Open>,
}

impl Editor {
    /// An editor for `document`, with an empty history bounded by [`DEFAULT_HISTORY_LIMIT`]. O(1).
    #[must_use]
    pub fn new(document: Document) -> Self {
        Self::with_history_limit(document, DEFAULT_HISTORY_LIMIT)
    }

    /// An editor whose history holds at most about `bytes` bytes (the most recent step is always
    /// kept, whatever its size). O(1).
    #[must_use]
    pub const fn with_history_limit(document: Document, bytes: usize) -> Self {
        Self {
            document,
            undo: VecDeque::new(),
            redo: Vec::new(),
            bytes: 0,
            limit: bytes,
            open: None,
        }
    }

    /// The document. O(1).
    #[must_use]
    pub const fn document(&self) -> &Document {
        &self.document
    }

    /// Gives the document back, dropping the history. O(1).
    #[must_use]
    pub fn into_document(self) -> Document {
        self.document
    }

    /// How many steps can be undone. O(1).
    #[must_use]
    pub fn undo_steps(&self) -> usize {
        self.undo.len()
    }

    /// How many steps can be redone. O(1).
    #[must_use]
    pub const fn redo_steps(&self) -> usize {
        self.redo.len()
    }

    /// The memory the history holds, by its estimate, in bytes. O(1).
    #[must_use]
    pub const fn history_bytes(&self) -> usize {
        self.bytes
    }

    /// The memory limit of the history, in bytes. O(1).
    #[must_use]
    pub const fn history_limit(&self) -> usize {
        self.limit
    }

    /// Runs `body` as one transaction and, if it succeeds, records it as one step of undo and
    /// returns what it changed. If `body` (or the final derivation of the tempo map) fails, every
    /// edit it applied is reverted and the error returned: the document is as before.
    ///
    /// A transaction that applies no edit changes nothing, records no step and returns an empty
    /// change set. A transaction that changes something discards the redo steps (the history is
    /// linear).
    ///
    /// # Complexity
    /// The cost of the edits, plus O(e log e) to tidy the change set of e entries, plus the tempo
    /// map derivation (≈ 0.6 ms for 250 marks, F1.05b numbers) only when a tempo mark or a bar
    /// start changed.
    ///
    /// # Errors
    /// [`CommandError::TransactionOpen`] if called from inside another transaction; the error of
    /// `body` or of the tempo map; [`CommandError::Rollback`] if reverting failed (never expected).
    pub fn transact<F>(&mut self, body: F) -> Result<ChangeSet, CommandError>
    where
        F: FnOnce(&mut Transaction<'_>) -> Result<(), CommandError>,
    {
        self.begin()?;
        let result = body(&mut Transaction { editor: self });
        match result {
            Ok(()) => self.commit(),
            Err(cause) => {
                self.rollback(cause)?;
                Err(cause)
            }
        }
    }

    /// Undoes the most recent step and returns what it changed, or `None` if there is nothing to
    /// undo. The step moves to the redo list. O(the step's edits) + the tempo map derivation if
    /// needed.
    ///
    /// # Errors
    /// [`CommandError::TransactionOpen`] inside a transaction; [`CommandError::Document`] if the
    /// step could not be applied (never expected), in which case the document and the history are
    /// as before; [`CommandError::Rollback`] if even that failed.
    pub fn undo(&mut self) -> Result<Option<ChangeSet>, CommandError> {
        if self.open.is_some() {
            return Err(CommandError::TransactionOpen);
        }
        let Some(step) = self.undo.pop_back() else {
            return Ok(None);
        };
        self.bytes = self.bytes.saturating_sub(step.bytes);
        match self.run(&step.edits) {
            Ok((redo, changes)) => {
                self.bytes = self.bytes.saturating_add(redo.bytes);
                self.redo.push(redo);
                Ok(Some(changes))
            }
            Err(e) => {
                self.bytes = self.bytes.saturating_add(step.bytes);
                self.undo.push_back(step);
                Err(e)
            }
        }
    }

    /// Redoes the most recently undone step and returns what it changed, or `None` if there is
    /// nothing to redo. As [`undo`](Self::undo), the other way round.
    ///
    /// # Errors
    /// As [`undo`](Self::undo).
    pub fn redo(&mut self) -> Result<Option<ChangeSet>, CommandError> {
        if self.open.is_some() {
            return Err(CommandError::TransactionOpen);
        }
        let Some(step) = self.redo.pop() else {
            return Ok(None);
        };
        self.bytes = self.bytes.saturating_sub(step.bytes);
        match self.run(&step.edits) {
            Ok((undo, changes)) => {
                self.bytes = self.bytes.saturating_add(undo.bytes);
                self.undo.push_back(undo);
                self.trim();
                Ok(Some(changes))
            }
            Err(e) => {
                self.bytes = self.bytes.saturating_add(step.bytes);
                self.redo.push(step);
                Err(e)
            }
        }
    }

    // --------------------------------------------------------------------------------------------
    // Inside
    // --------------------------------------------------------------------------------------------

    /// Opens a transaction. O(t) to keep a copy of the tempo map (t tempo segments, hundreds).
    fn begin(&mut self) -> Result<(), CommandError> {
        if self.open.is_some() {
            return Err(CommandError::TransactionOpen);
        }
        self.open = Some(Open {
            inverses: Vec::new(),
            changes: ChangeSet::default(),
            tempo_before: self.document.tempo_map().clone(),
        });
        Ok(())
    }

    /// Applies one edit inside the open transaction. On error the document is as before this
    /// edit (each edit is all or nothing); the transaction stays open.
    fn apply(&mut self, edit: Edit) -> Result<(), CommandError> {
        let open = self.open.as_mut().ok_or(CommandError::TransactionOpen)?;
        let inverse = self.document.apply_edit(edit, &mut open.changes)?;
        open.inverses.push(inverse);
        Ok(())
    }

    /// Closes the open transaction: derives the tempo map if needed, records the step, returns
    /// the change set. If the tempo map fails, the transaction is reverted and the error returned.
    fn commit(&mut self) -> Result<ChangeSet, CommandError> {
        let Some(open) = self.open.take() else {
            return Ok(ChangeSet::default());
        };
        let Open {
            inverses,
            mut changes,
            tempo_before,
        } = open;
        if inverses.is_empty() {
            return Ok(ChangeSet::default());
        }
        if changes.tempo_dirty {
            match self.document.derive_tempo_map() {
                Ok(map) => {
                    changes.set_tempo_from(first_difference(&tempo_before, &map));
                    self.document.tempo_map = map;
                }
                Err(cause) => {
                    self.revert(&inverses, cause)?;
                    return Err(CommandError::Document(cause));
                }
            }
        }
        changes.tempo_dirty = false;
        changes.finish();
        self.document.revision = self.document.revision.saturating_add(1);
        // A new change makes the undone steps unreachable: the history is linear.
        let dropped: usize = self
            .redo
            .drain(..)
            .map(|s| s.bytes)
            .fold(0, usize::saturating_add);
        self.bytes = self.bytes.saturating_sub(dropped);
        let step = Step::undoing(inverses);
        self.bytes = self.bytes.saturating_add(step.bytes);
        self.undo.push_back(step);
        self.trim();
        Ok(changes)
    }

    /// Reverts and closes the open transaction after `cause` made it fail. The tempo map was not
    /// touched while the transaction was open (it is derived only at the end), so reverting the
    /// edits is enough.
    fn rollback(&mut self, cause: CommandError) -> Result<(), CommandError> {
        let Some(open) = self.open.take() else {
            return Ok(());
        };
        let cause = match cause {
            CommandError::Document(e) | CommandError::Rollback { cause: e, .. } => e,
            CommandError::TransactionOpen => DocumentError::Inconsistent,
        };
        self.revert(&open.inverses, cause)
    }

    /// Applies the inverses of a failed transaction, last first, and puts the tempo map back if it
    /// had been rebuilt. O(edits).
    fn revert(&mut self, inverses: &[Edit], cause: DocumentError) -> Result<(), CommandError> {
        let mut scratch = ChangeSet::default();
        for inverse in inverses.iter().rev() {
            self.document
                .apply_edit(inverse.clone(), &mut scratch)
                .map_err(|rollback| CommandError::Rollback { cause, rollback })?;
        }
        Ok(())
    }

    /// Applies the edits of a history step in order and returns the step that reverses them, with
    /// what they changed; derives the tempo map if needed and bumps the revision. If anything
    /// fails, the edits already applied are reverted and the error returned.
    fn run(&mut self, edits: &[Edit]) -> Result<(Step, ChangeSet), CommandError> {
        let tempo_before = self.document.tempo_map().clone();
        let mut changes = ChangeSet::default();
        let mut inverses = Vec::with_capacity(edits.len());
        for edit in edits {
            match self.document.apply_edit(edit.clone(), &mut changes) {
                Ok(inverse) => inverses.push(inverse),
                Err(cause) => {
                    self.revert(&inverses, cause)?;
                    return Err(CommandError::Document(cause));
                }
            }
        }
        if changes.tempo_dirty {
            match self.document.derive_tempo_map() {
                Ok(map) => {
                    changes.set_tempo_from(first_difference(&tempo_before, &map));
                    self.document.tempo_map = map;
                }
                Err(cause) => {
                    self.revert(&inverses, cause)?;
                    return Err(CommandError::Document(cause));
                }
            }
        }
        changes.tempo_dirty = false;
        changes.finish();
        self.document.revision = self.document.revision.saturating_add(1);
        Ok((Step::undoing(inverses), changes))
    }

    /// Drops the oldest undo steps while the history holds more than its limit, always keeping
    /// the most recent one. O(steps dropped).
    fn trim(&mut self) {
        while self.bytes > self.limit && self.undo.len() > 1 {
            if let Some(oldest) = self.undo.pop_front() {
                self.bytes = self.bytes.saturating_sub(oldest.bytes);
            }
        }
    }
}

/// The first written position where two tempo maps differ, or `None` if they are equal. O(t) for
/// t tempo changes: the two lists of changes are compared entry by entry.
fn first_difference(before: &TempoMap, after: &TempoMap) -> Option<Position> {
    let (old, new) = (before.changes(), after.changes());
    match old.iter().zip(new).position(|(was, now)| was != now) {
        // The first differing pair: the change that moved, appeared or disappeared there starts
        // at the earlier of the two positions.
        Some(at) => match (old.get(at), new.get(at)) {
            (Some(was), Some(now)) => Some(was.0.min(now.0)),
            (was, now) => was.or(now).map(|change| change.0),
        },
        // Equal over the common length: the longer list's first extra entry, if any.
        None => old
            .get(new.len())
            .or_else(|| new.get(old.len()))
            .map(|change| change.0),
    }
}

/// The open transaction, as seen by the closure of [`Editor::transact`].
#[derive(Debug)]
pub struct Transaction<'a> {
    /// The editor whose transaction this is.
    editor: &'a mut Editor,
}

impl Transaction<'_> {
    /// Applies one primitive edit. On error the document is as before this edit; returning the
    /// error from the closure reverts the whole transaction.
    ///
    /// The musical invariants are the caller's responsibility (see `crate::edit`): the commands
    /// keep them, and [`Document::validate`] checks them in the tests.
    ///
    /// # Errors
    /// [`CommandError::Document`] with the reason the edit was refused.
    pub fn apply(&mut self, edit: Edit) -> Result<(), CommandError> {
        self.editor.apply(edit)
    }

    /// The document as the edits applied so far left it. Its tempo map is the one from before
    /// the transaction until the transaction ends (it is derived once, at the end). O(1).
    #[must_use]
    pub const fn document(&self) -> &Document {
        &self.editor.document
    }

    /// Hands out a new ID from the document's counter, for an object the transaction is about to
    /// insert. The ID is spent even if the transaction is reverted (ADR-0026 §2). O(1).
    ///
    /// # Errors
    /// [`CommandError::Document`] with [`DocumentError::IdsExhausted`] when the counter is
    /// exhausted.
    pub fn new_id(&mut self) -> Result<std::num::NonZeroU64, CommandError> {
        self.editor
            .document
            .ids
            .next()
            .ok_or(CommandError::Document(DocumentError::IdsExhausted))
    }
}
