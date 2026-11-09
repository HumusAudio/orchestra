// SPDX-License-Identifier: GPL-3.0-or-later

//! The commands on marks, form markings, tuplets and the per-bar data (ADR-0026 §9, §11;
//! SCALETTA F1.11b, part 2b).
//!
//! # What lives here
//! The second half of the vocabulary of [`Command`](crate::Command): adding, changing and
//! removing every mark (slurs, dynamics, hairpins, tempo marks, clef and key changes, key
//! overrides) and every form marking (repeat barlines, voltas, markers, jumps, bar repeats, pass
//! rules); the tuplets; the values the document has once (the initial tempo, key, clefs and
//! numbering style); the per-bar data that does not move a barline (beat grouping, uncounted
//! bars, numbering restarts). `crate::command` dispatches to the functions here.
//!
//! # A mark is described without its ID
//! [`MarkSpec`] says what a mark is and where it sits, with no ID: the ID of a new mark comes from
//! the document's counter, and a changed mark keeps its own. So the same description serves
//! [`Command::AddMark`](crate::Command::AddMark) and
//! [`Command::ChangeMark`](crate::Command::ChangeMark), and a caller never builds an object with
//! an ID of its own choosing.
//!
//! # The decisions written down here (owner, 2026-11-05, ADR-0026 §11)
//! - **X1**: voltas are added a whole group at a time ([`NewVolta`],
//!   [`Command::AddVoltas`](crate::Command::AddVoltas)), together with the end repeats the group
//!   lacks; a repeated section's two barlines are added together
//!   ([`Command::AddRepeat`](crate::Command::AddRepeat)). A volta alone is never valid, so adding
//!   one at a time could never succeed.
//! - **X2**: removing a mark removes it and then deletes in **cascade** the form markings
//!   it left invalid (a jump whose segno is gone, the rest of a volta group), as deleting bars does
//!   (ADR-0026 W5).
//! - **X3**: adding, moving and changing never delete anything: a result that breaks a
//!   rule is refused with the typed violation of the local check.
//! - **X4**: a bar repeat over bars that hold music is refused (the local check reports
//!   [`Violation::BarRepeatNotEmpty`](crate::Violation::BarRepeatNotEmpty)).
//! - **X5**: changing a mark is one command with its whole new description, keeping its ID
//!   and its kind; a drag sends it inside a gesture, and the compaction keeps one old value.
//! - **X6**: clefs and keys never change a pitch (pitches are stored as spelled at concert
//!   pitch); respelling after a key change will be a command of its own.
//! - **X7**: a tuplet is added empty, changed in place (look or ratio, its parent kept)
//!   and removed **with the events it contains** and the tuplets nested in it.
//! - **X8**: the values the document has once are set, never removed.
//! - **X9**: the per-bar data that does not move a barline is here; irregular and free
//!   bars and meter changes, which do, belong to the meter change of F1.11c.
//! - **X10**: the local check of a mark command looks only at what the command touched
//!   (the slur, the hairpins of its instrument, the tempo marks, the form): see
//!   `Document::check_objects`.

use std::num::NonZeroU64;

use orchestra_core_time::BeatGrouping;

use crate::bar_repeat::{BarRepeat, BarRepeatSpan, PassRule};
use crate::document::{Document, DocumentError};
use crate::dynamic::{Dynamic, DynamicMark, DynamicPlacement, DynamicScope, Hairpin, HairpinShape};
use crate::edit::{Edit, Initial, MeterEdit, Object, ObjectRef};
use crate::editor::{CommandError, Transaction};
use crate::event::Tuplet;
use crate::form::{PassSet, RepeatEnd, RepeatStart, Volta};
use crate::id::{
    BarId, BarRepeatId, ClefId, DynamicId, EventId, HairpinId, InstrumentId, JumpId, KeyChangeId,
    MarkerId, PassRuleId, RepeatId, SlurId, StaffId, TempoMarkId, TupletId, VoiceId, VoltaId,
};
use crate::jump::{Jump, JumpSpec, Marker, MarkerKind};
use crate::marks::{Clef, KeyOverride, Mark, ScorePoint};
use crate::pitch::Key;
use crate::slur::{Slur, SlurPlacement};
use crate::tempo_mark::TempoMark;

// ------------------------------------------------------------------------------------------------
// The vocabulary
// ------------------------------------------------------------------------------------------------

/// A mark or a form marking as the user describes it: what it is and where it sits, without an
/// ID (see the module documentation). What [`Command::AddMark`](crate::Command::AddMark) adds and
/// what [`Command::ChangeMark`](crate::Command::ChangeMark) puts in place of an existing mark.
///
/// Each variant carries what the builder's `add_*` method of the same object takes, so that a
/// document built in one step and one built command by command hold the same objects.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum MarkSpec {
    /// A slur from one event to a later one of the same instrument (ADR-0024 §7). Its instrument
    /// is the one of its first event.
    Slur {
        /// The event it starts on.
        start: EventId,
        /// The event it ends on.
        end: EventId,
        /// Above or below.
        placement: SlurPlacement,
    },
    /// A dynamic sign at a point (ADR-0024 §8).
    Dynamic {
        /// The instrument it belongs to.
        instrument: InstrumentId,
        /// Where it is.
        at: ScorePoint,
        /// What it applies to and where it is drawn.
        placement: DynamicPlacement,
        /// What it says.
        mark: DynamicMark,
    },
    /// A hairpin from one point (included) to a later one (excluded) (ADR-0024 §8).
    Hairpin {
        /// The instrument it belongs to.
        instrument: InstrumentId,
        /// Where it starts.
        start: ScorePoint,
        /// Where it ends.
        end: ScorePoint,
        /// What it applies to and where it is drawn.
        placement: DynamicPlacement,
        /// Its direction and look.
        shape: HairpinShape,
    },
    /// A tempo mark after the very start of the score (ADR-0024 §11); the one at the start is the
    /// initial tempo ([`Command::SetInitial`](crate::Command::SetInitial)).
    Tempo {
        /// Where it is.
        at: ScorePoint,
        /// What it says and does.
        mark: TempoMark,
    },
    /// A clef change on a staff (ADR-0024 §9).
    Clef {
        /// The staff.
        staff: StaffId,
        /// Where it is.
        at: ScorePoint,
        /// The new clef.
        clef: Clef,
    },
    /// A key change of the whole score, at concert pitch (ADR-0023).
    Key {
        /// Where it is.
        at: ScorePoint,
        /// The new key.
        key: Key,
    },
    /// A key override of one instrument (ADR-0024 §9).
    KeyOverride {
        /// The instrument.
        instrument: InstrumentId,
        /// Where it is.
        at: ScorePoint,
        /// The override.
        key: KeyOverride,
    },
    /// A start repeat barline at the start of a bar (ADR-0025 §2).
    RepeatStart {
        /// The bar it starts.
        bar: BarId,
    },
    /// An end repeat barline at the end of a bar, with the number of plays (ADR-0025 §2).
    RepeatEnd {
        /// The bar it ends.
        bar: BarId,
        /// The total number of plays, 2 to [`MAX_PLAYS`](crate::MAX_PLAYS).
        times: u8,
    },
    /// A volta (ADR-0025 §3). Added a whole group at a time with
    /// [`Command::AddVoltas`](crate::Command::AddVoltas) (ADR-0026 X1); here for
    /// [`Command::ChangeMark`](crate::Command::ChangeMark), and for adding a volta to a group
    /// that stays valid with it.
    Volta(NewVolta),
    /// A segno, coda, Fine or "To Coda" on a barline (ADR-0025 §4).
    Marker {
        /// Its bar.
        bar: BarId,
        /// What it is.
        kind: MarkerKind,
        /// The user's own text instead of the usual sign or word; never empty.
        text: Option<Box<str>>,
    },
    /// A jump at the end of a bar (ADR-0025 §4).
    Jump {
        /// The bar at whose end it is.
        bar: BarId,
        /// What it does.
        spec: JumpSpec,
        /// The user's own text instead of the derived words; never empty.
        text: Option<Box<str>>,
    },
    /// Bars of one staff that repeat the bars before them (ADR-0025 §6). Refused over bars that
    /// hold music (ADR-0026 X4).
    BarRepeat {
        /// The staff.
        staff: StaffId,
        /// The first repeated bar.
        first: BarId,
        /// How many bars repeat, a positive multiple of the span.
        count: u32,
        /// The sign.
        span: BarRepeatSpan,
    },
    /// A passage played only on some passes of its repeated section (ADR-0025 §7).
    PassRule {
        /// The instrument.
        instrument: InstrumentId,
        /// Where the passage starts (included).
        start: ScorePoint,
        /// Where it ends (excluded).
        end: ScorePoint,
        /// What it applies to.
        scope: DynamicScope,
        /// The passes on which it is played; never empty.
        plays: PassSet,
        /// The user's own text instead of the derived words; never empty.
        text: Option<Box<str>>,
    },
}

/// A volta as the user describes it, without an ID: one entry of
/// [`Command::AddVoltas`](crate::Command::AddVoltas), or the content of
/// [`MarkSpec::Volta`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewVolta {
    /// Its first bar.
    pub first: BarId,
    /// Its last bar (the same as `first` for a one-bar volta).
    pub last: BarId,
    /// The passes it is played on; never empty.
    pub passes: PassSet,
    /// Whether the bracket ends with a downward jog (usual for a first ending).
    pub closed: bool,
    /// The user's own text instead of the numbers; never empty.
    pub text: Option<Box<str>>,
}

// ------------------------------------------------------------------------------------------------
// Building the objects
// ------------------------------------------------------------------------------------------------

/// Refuses a free text that is empty or only white space. O(text length).
///
/// Shared with the builder (`DocumentBuilder::add_volta` and the others).
///
/// # Errors
/// [`DocumentError::EmptyText`].
pub(crate) fn check_text(text: Option<&str>) -> Result<(), DocumentError> {
    if text.is_some_and(|t| t.trim().is_empty()) {
        return Err(DocumentError::EmptyText);
    }
    Ok(())
}

/// The object a description makes, with the ID `raw` (a new one from the counter, or the ID of
/// the mark being changed).
///
/// Only what the edits do not check is checked here: the texts, the markers a jump names, and
/// the instrument of a slur (found from its first event). Everything structural (bars, staves,
/// voices, offsets, duplicates) is checked by the edit, and the musical rules by the local check
/// that follows the command (ADR-0026 X10).
///
/// # Complexity
/// O(1), plus O(voices) to find a slur's instrument and O(markers) for a jump's markers.
///
/// # Errors
/// [`DocumentError::EmptyText`], [`DocumentError::UnknownMarker`],
/// [`DocumentError::UnknownEvent`] or [`DocumentError::UnknownVoice`] for a slur's first event.
#[allow(
    clippy::too_many_lines,
    reason = "one short arm per kind of mark: splitting the match would only scatter it"
)]
fn object_of(doc: &Document, raw: NonZeroU64, spec: &MarkSpec) -> Result<Object, DocumentError> {
    Ok(match spec {
        MarkSpec::Slur {
            start,
            end,
            placement,
        } => {
            let voice = doc
                .event(*start)
                .ok_or(DocumentError::UnknownEvent(*start))?
                .voice();
            let (instrument, _) = doc.voice(voice).ok_or(DocumentError::UnknownVoice(voice))?;
            Object::Slur {
                instrument: instrument.id(),
                slur: Slur::new(SlurId::from_counter(raw), *start, *end, *placement),
            }
        }
        MarkSpec::Dynamic {
            instrument,
            at,
            placement,
            mark,
        } => Object::Dynamic {
            instrument: *instrument,
            dynamic: Dynamic::new(DynamicId::from_counter(raw), *at, *placement, mark.clone()),
        },
        MarkSpec::Hairpin {
            instrument,
            start,
            end,
            placement,
            shape,
        } => Object::Hairpin {
            instrument: *instrument,
            hairpin: Hairpin::new(
                HairpinId::from_counter(raw),
                *start,
                *end,
                *placement,
                *shape,
            ),
        },
        MarkSpec::Tempo { at, mark } => Object::TempoMark(Mark::new(
            TempoMarkId::from_counter(raw),
            at.bar,
            at.offset,
            mark.clone(),
        )),
        MarkSpec::Clef { staff, at, clef } => Object::ClefChange {
            staff: *staff,
            mark: Mark::new(ClefId::from_counter(raw), at.bar, at.offset, *clef),
        },
        MarkSpec::Key { at, key } => Object::KeyChange(Mark::new(
            KeyChangeId::from_counter(raw),
            at.bar,
            at.offset,
            *key,
        )),
        MarkSpec::KeyOverride {
            instrument,
            at,
            key,
        } => Object::KeyOverride {
            instrument: *instrument,
            mark: Mark::new(KeyChangeId::from_counter(raw), at.bar, at.offset, *key),
        },
        MarkSpec::RepeatStart { bar } => {
            Object::RepeatStart(RepeatStart::new(RepeatId::from_counter(raw), *bar))
        }
        MarkSpec::RepeatEnd { bar, times } => {
            Object::RepeatEnd(RepeatEnd::new(RepeatId::from_counter(raw), *bar, *times))
        }
        MarkSpec::Volta(volta) => {
            check_text(volta.text.as_deref())?;
            Object::Volta(Volta::new(
                VoltaId::from_counter(raw),
                volta.first,
                volta.last,
                volta.passes,
                volta.closed,
                volta.text.clone(),
            ))
        }
        MarkSpec::Marker { bar, kind, text } => {
            check_text(text.as_deref())?;
            Object::Marker(Marker::new(
                MarkerId::from_counter(raw),
                *bar,
                *kind,
                text.clone(),
            ))
        }
        MarkSpec::Jump { bar, spec, text } => {
            check_text(text.as_deref())?;
            // An edit does not check the markers a jump names (a later edit of the same
            // transaction could remove them: see `crate::edit`), so the command does.
            doc.check_jump_markers(spec)?;
            Object::Jump(Jump::new(
                JumpId::from_counter(raw),
                *bar,
                *spec,
                text.clone(),
            ))
        }
        MarkSpec::BarRepeat {
            staff,
            first,
            count,
            span,
        } => Object::BarRepeat(BarRepeat::new(
            BarRepeatId::from_counter(raw),
            *staff,
            *first,
            *count,
            *span,
        )),
        MarkSpec::PassRule {
            instrument,
            start,
            end,
            scope,
            plays,
            text,
        } => {
            check_text(text.as_deref())?;
            Object::PassRule {
                instrument: *instrument,
                rule: PassRule::new(
                    PassRuleId::from_counter(raw),
                    *start,
                    *end,
                    *scope,
                    *plays,
                    text.clone(),
                ),
            }
        }
    })
}

/// The raw ID of a mark named by `name`, or [`DocumentError::NotAMark`] for an event, a tuplet,
/// a bar or a value the document has once (they have commands of their own). O(1).
fn mark_id(name: ObjectRef) -> Result<NonZeroU64, DocumentError> {
    // Events, tuplets and bars have IDs but are not marks; the values a document has once have
    // no ID at all (`raw_id` is `None`).
    let raw = match name {
        ObjectRef::Bar(_) | ObjectRef::Event(_) | ObjectRef::Tuplet(_) => None,
        _ => name.raw_id(),
    }
    .ok_or(DocumentError::NotAMark(name))?;
    // IDs come from a counter that starts at 1, so 0 never names anything.
    NonZeroU64::new(raw).ok_or(DocumentError::UnknownObject(name))
}

/// Whether a description makes a mark of the kind `name` names (ADR-0026 X5: a mark keeps its
/// kind). The two kinds that share the key-change IDs are told apart by the variant of `name`.
/// O(1).
const fn same_kind(name: ObjectRef, spec: &MarkSpec) -> bool {
    matches!(
        (name, spec),
        (ObjectRef::Slur(_), MarkSpec::Slur { .. })
            | (ObjectRef::Dynamic(_), MarkSpec::Dynamic { .. })
            | (ObjectRef::Hairpin(_), MarkSpec::Hairpin { .. })
            | (ObjectRef::TempoMark(_), MarkSpec::Tempo { .. })
            | (ObjectRef::ClefChange(_), MarkSpec::Clef { .. })
            | (ObjectRef::KeyChange(_), MarkSpec::Key { .. })
            | (ObjectRef::KeyOverride(_), MarkSpec::KeyOverride { .. })
            | (ObjectRef::RepeatStart(_), MarkSpec::RepeatStart { .. })
            | (ObjectRef::RepeatEnd(_), MarkSpec::RepeatEnd { .. })
            | (ObjectRef::Volta(_), MarkSpec::Volta(_))
            | (ObjectRef::Marker(_), MarkSpec::Marker { .. })
            | (ObjectRef::Jump(_), MarkSpec::Jump { .. })
            | (ObjectRef::BarRepeat(_), MarkSpec::BarRepeat { .. })
            | (ObjectRef::PassRule(_), MarkSpec::PassRule { .. })
    )
}

// ------------------------------------------------------------------------------------------------
// Marks and form markings (ADR-0026 X1–X5)
// ------------------------------------------------------------------------------------------------

/// Executes [`Command::AddMark`](crate::Command::AddMark): a new ID from the counter, the object,
/// one [`Edit::Insert`]. The local check that follows refuses a mark that breaks a rule
/// (ADR-0026 X3).
///
/// # Complexity
/// [`object_of`], plus the insertion into its sorted list (O(n) for the n marks of the list).
///
/// # Errors
/// [`object_of`]'s and the edit's (an unknown bar, staff or instrument, a duplicate mark…).
pub(crate) fn add_mark(tx: &mut Transaction<'_>, spec: &MarkSpec) -> Result<(), CommandError> {
    // The object is built before the ID is taken, with a placeholder, only to check the
    // description: a refused description then spends no ID. Building it twice costs O(1).
    object_of(tx.document(), NonZeroU64::MIN, spec)?;
    let raw = tx.new_id()?;
    let object = object_of(tx.document(), raw, spec)?;
    tx.apply(Edit::Insert(object))
}

/// Executes [`Command::ChangeMark`](crate::Command::ChangeMark): the same ID with the new
/// description, one [`Edit::Replace`] (ADR-0026 X5). The local check that follows refuses a change
/// that breaks a rule (ADR-0026 X3).
///
/// # Complexity
/// [`object_of`], plus the replacement (a removal and an insertion in the sorted list, O(n)).
///
/// # Errors
/// [`DocumentError::NotAMark`], [`DocumentError::MarkKindMismatch`], [`object_of`]'s and the
/// edit's ([`DocumentError::UnknownObject`] for a mark the document does not have).
pub(crate) fn change_mark(
    tx: &mut Transaction<'_>,
    name: ObjectRef,
    spec: &MarkSpec,
) -> Result<(), CommandError> {
    let raw = mark_id(name)?;
    if !same_kind(name, spec) {
        return Err(DocumentError::MarkKindMismatch(name).into());
    }
    let object = object_of(tx.document(), raw, spec)?;
    tx.apply(Edit::Replace(object))
}

/// Executes [`Command::RemoveMark`](crate::Command::RemoveMark): one [`Edit::Remove`], then the
/// cascade of ADR-0026 W5 for the form markings the removal left invalid (ADR-0026 X2).
///
/// # Complexity
/// O(n) for the removal from its list, plus the cascade (O(f) form checks per marking it deletes,
/// plus one).
///
/// # Errors
/// [`DocumentError::NotAMark`], the edit's ([`DocumentError::UnknownObject`]), the cascade's.
pub(crate) fn remove_mark(tx: &mut Transaction<'_>, name: ObjectRef) -> Result<(), CommandError> {
    mark_id(name)?;
    tx.apply(Edit::Remove(name))?;
    crate::command::cascade(tx)
}

/// Executes [`Command::AddRepeat`](crate::Command::AddRepeat): a start repeat at the start of
/// `start` and an end repeat at the end of `end`, in one command (ADR-0026 X1). O(form markings).
///
/// # Errors
/// The edits' (an unknown bar, a barline already there, plays out of range); the local check
/// refuses a section that overlaps another one.
pub(crate) fn add_repeat(
    tx: &mut Transaction<'_>,
    start: BarId,
    end: BarId,
    times: u8,
) -> Result<(), CommandError> {
    add_mark(tx, &MarkSpec::RepeatStart { bar: start })?;
    add_mark(tx, &MarkSpec::RepeatEnd { bar: end, times })
}

/// Executes [`Command::AddVoltas`](crate::Command::AddVoltas): a whole group of voltas, with the
/// end repeats it lacks (ADR-0026 X1).
///
/// Every volta but the last needs an end repeat at its last bar (ADR-0025 §3). Where there is
/// none, one is added with `times` = n, the number of plays of the group (its highest pass): the
/// two values `validate` accepts are 2 and n, and n says what the voltas say. An end repeat that
/// is already there is kept as it is (the local check refuses it if its plays contradict the
/// voltas).
///
/// # Complexity
/// O(v × form markings) for the v voltas.
///
/// # Errors
/// [`DocumentError::EmptyPassSet`] for a volta without passes; the edits'; the local check's for
/// a group that is not valid (fewer than two voltas, passes that are not exactly `1..=n`…).
pub(crate) fn add_voltas(
    tx: &mut Transaction<'_>,
    voltas: &[NewVolta],
) -> Result<(), CommandError> {
    // The number of plays of the group: its highest pass.
    let mut plays = 0;
    for volta in voltas {
        plays = plays.max(volta.passes.last().ok_or(DocumentError::EmptyPassSet)?);
    }
    let Some((_, inner)) = voltas.split_last() else {
        return Ok(());
    };
    for volta in inner {
        let doc = tx.document();
        let has_end = doc
            .form()
            .repeat_ends()
            .iter()
            .any(|r| r.bar() == volta.last);
        if !has_end {
            add_mark(
                tx,
                &MarkSpec::RepeatEnd {
                    bar: volta.last,
                    times: plays,
                },
            )?;
        }
    }
    for volta in voltas {
        add_mark(tx, &MarkSpec::Volta(volta.clone()))?;
    }
    Ok(())
}

// ------------------------------------------------------------------------------------------------
// Tuplets (ADR-0026 X7)
// ------------------------------------------------------------------------------------------------

/// Executes [`Command::AddTuplet`](crate::Command::AddTuplet): an empty tuplet of `voice`,
/// nested in `parent` if given. An empty (incomplete) tuplet is a valid state (ADR-0024); its
/// notes are entered with [`Command::EnterEvent`](crate::Command::EnterEvent). O(1) average.
///
/// # Errors
/// The edit's: an unknown voice or parent, a nesting deeper than
/// [`MAX_TUPLET_DEPTH`](crate::MAX_TUPLET_DEPTH).
pub(crate) fn add_tuplet(
    tx: &mut Transaction<'_>,
    voice: VoiceId,
    tuplet: Tuplet,
    parent: Option<TupletId>,
) -> Result<(), CommandError> {
    let mut tuplet = tuplet;
    tuplet.set_parent(parent);
    let id = TupletId::from_counter(tx.new_id()?);
    tx.apply(Edit::Insert(Object::Tuplet { voice, id, tuplet }))
}

/// The voice holding a tuplet, and the tuplet. O(voices) average.
///
/// # Errors
/// [`DocumentError::UnknownTuplet`].
fn find_tuplet(doc: &Document, id: TupletId) -> Result<(VoiceId, Tuplet), DocumentError> {
    doc.instruments()
        .flat_map(crate::document::Instrument::voices)
        .find_map(|voice| voice.tuplet(id).map(|t| (voice.id(), *t)))
        .ok_or(DocumentError::UnknownTuplet(id))
}

/// Executes [`Command::ChangeTuplet`](crate::Command::ChangeTuplet): the tuplet's ratio, unit and
/// look become those of `to`, its parent stays. Replaced in place by the edit, which names the
/// cells of its events (their durations may change), so that the local check refuses a ratio
/// that makes them overlap. O(events of the voice) for the cells.
///
/// # Errors
/// [`DocumentError::UnknownTuplet`]; the local check's.
pub(crate) fn change_tuplet(
    tx: &mut Transaction<'_>,
    id: TupletId,
    to: Tuplet,
) -> Result<(), CommandError> {
    let (voice, old) = find_tuplet(tx.document(), id)?;
    let mut tuplet = to;
    tuplet.set_parent(old.parent());
    tx.apply(Edit::Replace(Object::Tuplet { voice, id, tuplet }))
}

/// Executes [`Command::RemoveTuplet`](crate::Command::RemoveTuplet): the events inside the tuplet
/// (directly or through nested tuplets) are deleted as
/// [`Command::DeleteEvent`](crate::Command::DeleteEvent) deletes them (ADR-0026 V6, which also
/// removes the tuplets left without members), then whatever of the tuplet is left (nested tuplets
/// that held no event, deepest first, then the tuplet itself) (ADR-0026 X7).
///
/// # Complexity
/// O(events of the voice × depth) to find the members, at most [`MAX_TUPLET_DEPTH`] links per
/// event, plus the deletion.
///
/// [`MAX_TUPLET_DEPTH`]: crate::MAX_TUPLET_DEPTH
///
/// # Errors
/// [`DocumentError::UnknownTuplet`]; the deletion's.
pub(crate) fn remove_tuplet(tx: &mut Transaction<'_>, id: TupletId) -> Result<(), CommandError> {
    let doc = tx.document();
    let (voice_id, _) = find_tuplet(doc, id)?;
    let (_, voice) = doc
        .voice(voice_id)
        .ok_or(DocumentError::UnknownVoice(voice_id))?;
    // Whether `t` is `id` or nested in it, following the parent links (at most the depth bound).
    let inside = |t: TupletId| {
        let mut next = Some(t);
        while let Some(current) = next {
            if current == id {
                return true;
            }
            next = voice.tuplet(current).and_then(|tu| tu.parent());
        }
        false
    };
    let members: Vec<EventId> = (0..doc.bars().len())
        .filter_map(|i| u32::try_from(i).ok())
        .flat_map(|i| voice.events_at(orchestra_core_time::BarIndex::new(i)))
        .filter(|placed| placed.event().tuplet().is_some_and(inside))
        .map(|placed| placed.event().id())
        .collect();
    // The nested tuplets, deepest first (a child is removed before its parent): sorted by depth,
    // descending, then by ID, so that the order does not depend on the hash map's. Collected
    // before the deletion, which may already remove some of them.
    let mut nested: Vec<(usize, TupletId)> = voice
        .content
        .tuplets
        .keys()
        .copied()
        .filter(|&t| t != id && inside(t))
        .map(|t| (depth(voice, t), t))
        .collect();
    nested.sort_unstable_by(|a, b| b.cmp(a));
    crate::command::delete_events(tx, &members)?;
    for t in nested
        .into_iter()
        .map(|(_, t)| t)
        .chain(std::iter::once(id))
    {
        match tx.apply(Edit::Remove(ObjectRef::Tuplet(t))) {
            // Already removed with its last member by the deletion.
            Ok(()) | Err(CommandError::Document(DocumentError::UnknownObject(_))) => {}
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

/// How deep a tuplet is nested (1 for a top-level one). O(depth), at most the depth bound.
fn depth(voice: &crate::document::Voice, id: TupletId) -> usize {
    let mut depth = 0;
    let mut next = Some(id);
    while let Some(current) = next {
        depth += 1;
        next = voice.tuplet(current).and_then(|t| t.parent());
    }
    depth
}

// ------------------------------------------------------------------------------------------------
// Values the document has once (ADR-0026 X8) and per-bar data (ADR-0026 X9)
// ------------------------------------------------------------------------------------------------

/// Executes [`Command::SetInitial`](crate::Command::SetInitial): one [`Edit::SetInitial`]. A new
/// initial clef or key changes no pitch (ADR-0026 X6). O(1), or O(staves) for a clef.
///
/// # Errors
/// The edit's: [`DocumentError::InitialTempoNotImmediate`], an unknown staff.
pub(crate) fn set_initial(tx: &mut Transaction<'_>, initial: &Initial) -> Result<(), CommandError> {
    tx.apply(Edit::SetInitial(initial.clone()))
}

/// Executes [`Command::SetBarGrouping`](crate::Command::SetBarGrouping): the beat grouping of one
/// bar, or the section's counting again with `None` (ADR-0019). O(m log m) for the meter map
/// rebuild.
///
/// # Errors
/// The edit's: an unknown bar, a grouping that does not fit the bar (the meter map's error).
pub(crate) fn set_bar_grouping(
    tx: &mut Transaction<'_>,
    bar: BarId,
    grouping: Option<BeatGrouping>,
) -> Result<(), CommandError> {
    tx.apply(Edit::Meter(MeterEdit::BarGrouping { bar, grouping }))
}

/// Executes [`Command::SetUncounted`](crate::Command::SetUncounted): whether a bar is left out of
/// the displayed numbering (ADR-0018). O(m) for the meter map.
///
/// # Errors
/// The edit's: an unknown bar.
pub(crate) fn set_uncounted(
    tx: &mut Transaction<'_>,
    bar: BarId,
    uncounted: bool,
) -> Result<(), CommandError> {
    tx.apply(Edit::Meter(MeterEdit::Uncounted { bar, uncounted }))
}

/// Executes [`Command::SetNumberingRestart`](crate::Command::SetNumberingRestart): a restart of
/// the displayed numbering at a bar, or none (ADR-0018). O(m) for the meter map.
///
/// # Errors
/// The edit's: an unknown bar.
pub(crate) fn set_numbering_restart(
    tx: &mut Transaction<'_>,
    bar: BarId,
    number: Option<u32>,
) -> Result<(), CommandError> {
    tx.apply(Edit::Meter(MeterEdit::NumberingRestart { bar, number }))
}
