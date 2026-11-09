// SPDX-License-Identifier: GPL-3.0-or-later

//! Commands: what the user asks for, as data, expanded into primitive edits (ADR-0026 §1, §6;
//! SCALETTA F1.11b, part 2).
//!
//! # Two levels
//! A [`Command`] says *what* the user wants ("enter this chord here", "delete bars 5–8"). It is
//! executed inside an open transaction ([`crate::Transaction::execute`], or
//! [`crate::Editor::execute`] for a transaction of its own): it checks its preconditions, takes
//! new IDs from the document's counter and emits a sequence of primitive edits (`crate::edit`).
//! The history stores the edits' inverses, never the command, so redo never runs a command again
//! and never hands out new IDs (ADR-0026 §1).
//!
//! # Who keeps the musical rules
//! The edits keep only the structure. The commands keep the music valid: no overlap within a
//! voice, ties to the same pitch, grace notes with their main event, the form valid. Each command
//! does it in one of two ways:
//! - the **event commands** emit their edits and then pass the **local check**
//!   (`Document::check_cells`): the rules of `validate` that a change of events can break,
//!   checked only on the cells (voice, bar) the command touched (owner's decision ADR-0026 V8 of
//!   2026-11-04). A refusal is a typed [`DocumentError::Invalid`]: for example entering a note
//!   where the voice already has one is a [`crate::Violation::Overlap`] (ADR-0026 V2);
//! - the **bar commands** keep the rules by construction (§6 of ADR-0026), and delete in cascade
//!   the form markings made invalid (ADR-0026 W5).
//!
//! The property tests check both with the full [`Document::validate`] after every command.
//!
//! # Atomic
//! A command that fails leaves the document exactly as before it, even inside a larger
//! transaction: the editor reverts the edits the command had applied (`crate::editor`).
//!
//! # The decisions written down here (owner, 2026-11-04, ADR-0026)
//! - **V2**: an event is entered only into free space; entering over other events is
//!   refused.
//! - **V3**: an event that crosses a barline becomes a **tie chain** (a rest: several
//!   rests): each part is written with the longest values first, with at most two dots.
//! - **V4** (lifted by **Z4**, F1.11c part 2a): a tuplet note that crosses a
//!   barline is split into tied notes that stay in the same tuplet, written in its own values
//!   (ADR-0026 Z3).
//! - **V5**: an event entered past the end of a **free** bar lengthens the bar instead.
//! - **V6**: deleting an event leaves a gap (no rest is added) and deletes with it the
//!   slurs on it, the grace notes that ornament it and the tuplets left without members; the ties
//!   leading into it are cleared.
//! - **W1**: moving an event never splits it: past the barline the bar becomes overfull, a
//!   valid state (ADR-0022 §4).
//! - **W2**: changing the pitch of a tied note changes the whole tie chain.
//! - **W3**: entering an event that would reach past the last bar is refused.
//! - **ADR-0026 W4–W6**: see [`Command::DeleteBars`] and [`Command::InsertBars`].

use orchestra_core_time::{
    BarIndex, BarKind, BarSpan, BeatGrouping, Duration, IrregularBar, MeterChange,
};

use crate::bar_repeat::BarRepeat;
use crate::chain::{chain_ids, push_parts, tie_to_next};
use crate::daw_command::{self, MovePlayed, ResizePlayed};
use crate::document::{Document, DocumentError, Placement};
use crate::edit::{Edit, Initial, MeterEdit, Object, ObjectRef};
use crate::editor::{CommandError, Transaction};
use crate::event::{
    BaseValue, Chord, Event, EventBody, Note, NoteValue, PlacedEvent, Rest, RestValue, Tie, Tuplet,
};
use crate::id::{BarId, EventId, IdMap, IdSet, NoteId, StaffId, TrackId, TupletId, VoiceId};
use crate::mark_command::{self, MarkSpec, NewVolta};
use crate::marking::{
    Arpeggio, ArticulationSet, Fermata, Grace, GraceAttach, Ornament, PerformanceOffsets,
};
use crate::marks::{Mark, ScorePoint, point_key};
use crate::meter_command::{self, MeterMode};
use crate::pitch::Pitch;
use crate::tempo_mark::{Metronome, TempoMark, TempoMarkChange};
use crate::tolerance::Tolerance;
use crate::validate::Violation;

/// The most events one part of a tie chain may be written with (ADR-0026 V3). A part of a bar is
/// written with the longest values first; any length a bar can have needs a handful of them, so the
/// bound only stops a pathological length (thousands of 1024ths) from producing thousands of
/// events.
const MAX_VALUES_PER_PART: usize = 16;

// ------------------------------------------------------------------------------------------------
// The vocabulary
// ------------------------------------------------------------------------------------------------

/// What the user asks for (see the module documentation).
///
/// The list is the part of ADR-0026 §9 that F1.11b implements: the event commands and the bar
/// commands (part 2a), the commands on marks, form markings, tuplets, the initial values and the
/// per-bar data (part 2b, `crate::mark_command`); F1.11c adds the changes of bar lengths
/// (`crate::meter_command`), whose `Rewrite` mode comes in its part 2. `#[non_exhaustive]` lets
/// new variants come without breaking callers.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Command {
    /// Enters a new event into free space (ADR-0026 V2), as a tie chain if it crosses a barline
    /// (ADR-0026 V3, and inside its tuplet for a tuplet note: ADR-0026 Z3, ADR-0026 Z4),
    /// lengthening a free bar instead (ADR-0026 V5).
    EnterEvent(NewEvent),
    /// Deletes an event, leaving a gap (ADR-0026 V6), with the slurs on it, the grace notes
    /// ornamenting it and the tuplets left without members; the ties leading into it are cleared.
    DeleteEvent(EventId),
    /// Changes one aspect of an existing event, keeping its ID (ADR-0026 V7). This is also what a
    /// drag sends, many times, inside a gesture (ADR-0026 §4).
    ChangeEvent {
        /// The event.
        event: EventId,
        /// What changes.
        change: EventChange,
    },
    /// Inserts `count` empty bars before the bar at `at` (`at` = the number of bars appends them)
    /// (ADR-0026 §6). The new bars take the meter, clef, key and tempo in force just before `at`
    /// (at `at` = 0, those of the first bar); what stood at the start of bar `at` moves on with
    /// it. Ranges spanning the point grow, those ending exactly at it do not (ADR-0026 Y8); between
    /// two voltas of a group the new bars join the earlier one (ADR-0026 X11). Refused between the
    /// source and the end of a bar repeat (ADR-0026 W6).
    InsertBars {
        /// Where the new bars go.
        at: BarIndex,
        /// How many (0 does nothing).
        count: u32,
    },
    /// Deletes the `count` bars from `at` on (ADR-0026 §6): their events (as
    /// [`DeleteEvent`](Self::DeleteEvent) does), their dynamics and the form markings on them;
    /// the meter, clef, key and tempo last in force inside them move to the first bar after them,
    /// unless it has its own; ranges are clipped (gradual tempo changes too, ADR-0026 W4) and
    /// deleted when they become empty; the form markings made invalid are deleted in cascade
    /// (ADR-0026 W5). At least one bar remains.
    DeleteBars {
        /// The first bar deleted.
        at: BarIndex,
        /// How many (0 does nothing).
        count: u32,
    },
    /// Adds a mark or a form marking (ADR-0026 X3: refused, never deleting anything, if the result
    /// breaks a rule). A volta is normally added with its group, [`AddVoltas`](Self::AddVoltas)
    /// (ADR-0026 X1).
    AddMark(MarkSpec),
    /// Gives an existing mark a new description, keeping its ID and its kind (ADR-0026 X5): a move,
    /// a new text, a new end… A drag sends it inside a gesture. Refused if the result breaks a rule
    /// (ADR-0026 X3).
    ChangeMark {
        /// The mark.
        mark: ObjectRef,
        /// Its new description, of the same kind.
        to: MarkSpec,
    },
    /// Removes a mark or a form marking, and deletes in cascade the form markings it leaves
    /// invalid (ADR-0026 X2, as ADR-0026 W5).
    RemoveMark(ObjectRef),
    /// Adds a repeated section: a start repeat at the start of `start` and an end repeat at the
    /// end of `end` (ADR-0026 X1).
    AddRepeat {
        /// The first bar of the section.
        start: BarId,
        /// The last bar of the section.
        end: BarId,
        /// The total number of plays, 2 to [`MAX_PLAYS`](crate::MAX_PLAYS).
        times: u8,
    },
    /// Adds a whole group of voltas, in order, with the end repeats it lacks (ADR-0026 X1).
    AddVoltas(Vec<NewVolta>),
    /// Adds an empty tuplet to a voice, nested in `parent` if given (ADR-0026 X7). Its notes are
    /// then entered with [`EnterEvent`](Self::EnterEvent).
    AddTuplet {
        /// The voice.
        voice: VoiceId,
        /// The ratio, unit and look (its own parent field is ignored).
        tuplet: Tuplet,
        /// The tuplet it is nested in, of the same voice.
        parent: Option<TupletId>,
    },
    /// Changes a tuplet's ratio, unit and look, keeping its parent (ADR-0026 X7). Refused if its
    /// events would then overlap.
    ChangeTuplet {
        /// The tuplet.
        tuplet: TupletId,
        /// The new ratio, unit and look (its parent field is ignored).
        to: Tuplet,
    },
    /// Removes a tuplet with the events it contains and the tuplets nested in it (ADR-0026 X7).
    RemoveTuplet(TupletId),
    /// Sets a value the document has once: the initial tempo (immediate), key, a staff's clef, the
    /// numbering style (ADR-0026 X8). No pitch changes (ADR-0026 X6).
    SetInitial(Initial),
    /// Sets the beat grouping of one bar, or `None` for its section's counting (X9, ADR-0019).
    SetBarGrouping {
        /// The bar.
        bar: BarId,
        /// The grouping.
        grouping: Option<BeatGrouping>,
    },
    /// Leaves a bar out of the displayed numbering, or counts it again (ADR-0026 X9).
    SetUncounted {
        /// The bar.
        bar: BarId,
        /// `true` to leave it uncounted.
        uncounted: bool,
    },
    /// Restarts the displayed numbering at a bar, or removes the restart with `None` (ADR-0026 X9).
    SetNumberingRestart {
        /// The bar.
        bar: BarId,
        /// The number it restarts with.
        number: Option<u32>,
    },
    /// Sets the meter change at a bar, or removes it with `None` (never at bar 0), in a mode that
    /// says what happens to the music already written (ADR-0026 §7, Y1–Y5).
    ChangeMeter {
        /// The bar.
        bar: BarId,
        /// The new meter change (meter and section grouping), or `None`.
        change: Option<MeterChange>,
        /// What happens to the music.
        mode: MeterMode,
    },
    /// Makes a bar irregular (another length, a pickup, a free bar), or regular again with `None`,
    /// in a mode as [`ChangeMeter`](Self::ChangeMeter) (ADR-0026 Y1).
    SetIrregularBar {
        /// The bar.
        bar: BarId,
        /// Its new length and kind, or `None`.
        irregular: Option<IrregularBar>,
        /// What happens to the music.
        mode: MeterMode,
    },
    /// Moves a played note from the DAW view (ADR-0028 §2–§5, `crate::daw_command`): within the
    /// tolerance threshold of its track an onset offset, beyond it a written move to the grid
    /// with the residual as the offset, unless the user chose
    /// [`MoveMode::PerformanceOnly`](crate::MoveMode::PerformanceOnly); a move of one pass is
    /// always an offset of that pass. A drag sends it inside a gesture.
    MovePlayed(MovePlayed),
    /// Changes the length of a played note from the DAW view (ADR-0028 §6 and §11,
    /// `crate::daw_command`): within the threshold a length offset, beyond it the written tie chain
    /// rewritten with its new length (tied across barlines) and the residual as the offset.
    ResizePlayed(ResizePlayed),
    /// Sets the project's tolerance threshold (`track: None`) or a track's override, or removes the
    /// override with `tolerance: None` (ADR-0028 §1).
    SetTolerance {
        /// The track, or `None` for the project value.
        track: Option<TrackId>,
        /// The threshold, or `None` to remove the track's override.
        tolerance: Option<Tolerance>,
    },
}

/// A new event: where it goes and what it is (the input of [`Command::EnterEvent`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewEvent {
    /// The voice, bar and offset, and optionally another staff and the innermost tuplet.
    pub at: Placement,
    /// A chord or a rest.
    pub body: NewBody,
}

/// What a new event is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NewBody {
    /// A chord (a single note is a chord of one pitch), or a grace chord.
    Chord {
        /// The written value.
        value: NoteValue,
        /// The pitches, at concert pitch, in any order (at least one).
        pitches: Vec<Pitch>,
        /// `Some` for a grace chord: it lasts zero and is never split.
        grace: Option<Grace>,
    },
    /// A rest.
    Rest(RestValue),
}

/// One aspect of an event that [`Command::ChangeEvent`] changes.
///
/// `#[non_exhaustive]` (ADR-0034 D3), like [`Command`]: new aspects come with later steps, and
/// callers build these values rather than match them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum EventChange {
    /// Moves the event to another bar and offset of its voice. The grace notes ornamenting it
    /// move with it. It is never split (ADR-0026 W1): past the barline the bar becomes overfull.
    Move {
        /// The new bar.
        bar: BarId,
        /// The new offset, ≥ 0.
        offset: Duration,
    },
    /// A new written value, for a chord or a rest. The `After` grace notes ornamenting the event
    /// follow its new end.
    Value(NoteValue),
    /// A new pitch for one notehead, and for every notehead of its tie chain (ADR-0026 W2).
    Pitch {
        /// The notehead.
        note: NoteId,
        /// Its new pitch, at concert pitch.
        pitch: Pitch,
    },
    /// A new notehead in the chord.
    AddNote(Pitch),
    /// Removes a notehead from the chord, clearing the ties leading into it. Removing the last
    /// notehead deletes the event (ADR-0026 V6).
    RemoveNote(NoteId),
    /// A new tie from one notehead.
    Tie {
        /// The notehead the tie starts from.
        note: NoteId,
        /// The tie.
        tie: Tie,
    },
    /// New articulations (chords only).
    Articulations(ArticulationSet),
    /// A new ornament, or none (chords only).
    Ornament(Option<Ornament>),
    /// A new fermata, or none.
    Fermata(Option<Fermata>),
    /// A new arpeggio sign, or none (chords only).
    Arpeggio(Option<Arpeggio>),
    /// New performance offsets for one notehead (ADR-0024 §10): what the DAW view changes.
    Offsets {
        /// The notehead.
        note: NoteId,
        /// The offsets.
        offsets: PerformanceOffsets,
    },
    /// Another staff of the instrument to draw the event on (cross-staff notation).
    Staff(StaffId),
}

// ------------------------------------------------------------------------------------------------
// Running a command
// ------------------------------------------------------------------------------------------------

/// Runs a command inside the open transaction `tx`: emits its edits. The local check and the
/// revert on error are the editor's (`crate::editor`).
///
/// # Errors
/// The typed error of the first precondition or edit that fails.
pub(crate) fn run(tx: &mut Transaction<'_>, command: &Command) -> Result<(), CommandError> {
    match command {
        Command::EnterEvent(new) => enter_event(tx, new),
        Command::DeleteEvent(id) => delete_events(tx, &[*id]),
        Command::ChangeEvent { event, change } => change_event(tx, *event, *change),
        Command::InsertBars { at, count } => insert_bars(tx, *at, *count),
        Command::DeleteBars { at, count } => delete_bars(tx, *at, *count),
        Command::AddMark(spec) => mark_command::add_mark(tx, spec),
        Command::ChangeMark { mark, to } => mark_command::change_mark(tx, *mark, to),
        Command::RemoveMark(mark) => mark_command::remove_mark(tx, *mark),
        Command::AddRepeat { start, end, times } => {
            mark_command::add_repeat(tx, *start, *end, *times)
        }
        Command::AddVoltas(voltas) => mark_command::add_voltas(tx, voltas),
        Command::AddTuplet {
            voice,
            tuplet,
            parent,
        } => mark_command::add_tuplet(tx, *voice, *tuplet, *parent),
        Command::ChangeTuplet { tuplet, to } => mark_command::change_tuplet(tx, *tuplet, *to),
        Command::RemoveTuplet(tuplet) => mark_command::remove_tuplet(tx, *tuplet),
        Command::SetInitial(initial) => mark_command::set_initial(tx, initial),
        Command::SetBarGrouping { bar, grouping } => {
            mark_command::set_bar_grouping(tx, *bar, *grouping)
        }
        Command::SetUncounted { bar, uncounted } => {
            mark_command::set_uncounted(tx, *bar, *uncounted)
        }
        Command::SetNumberingRestart { bar, number } => {
            mark_command::set_numbering_restart(tx, *bar, *number)
        }
        Command::ChangeMeter { bar, change, mode } => {
            meter_command::change_meter(tx, *bar, *change, *mode)
        }
        Command::SetIrregularBar {
            bar,
            irregular,
            mode,
        } => meter_command::set_irregular_bar(tx, *bar, *irregular, *mode),
        Command::MovePlayed(order) => daw_command::move_played(tx, order),
        Command::ResizePlayed(order) => daw_command::resize_played(tx, order),
        Command::SetTolerance { track, tolerance } => {
            daw_command::set_tolerance(tx, *track, *tolerance)
        }
    }
}

/// The local check that follows a command (ADR-0026 V8, ADR-0026 X10): the rules its edits may have
/// broken, checked only where it touched the document. A refusal is reverted by the editor.
///
/// - The **bar commands** keep the rules by construction (see the module documentation), and
///   **removing a mark** ends with the cascade, which leaves the form valid (ADR-0026 X2): nothing
///   to check.
/// - Every other command passes `Document::check_cells` on the cells it touched (the event
///   commands, and the tuplet commands whose events change) and `Document::check_objects` on the
///   marks it inserted or replaced (a slur's ends, a hairpin's neighbours, the tempo marks, the
///   form).
///
/// # Errors
/// [`DocumentError::Invalid`] with the first violation found.
pub(crate) fn check(
    doc: &Document,
    command: &Command,
    changes: &crate::change::ChangeSet,
) -> Result<(), DocumentError> {
    match command {
        Command::InsertBars { .. } | Command::DeleteBars { .. } | Command::RemoveMark(_) => Ok(()),
        _ => {
            doc.check_cells(changes)?;
            doc.check_objects(changes)
        }
    }
}

// ------------------------------------------------------------------------------------------------
// Written values
// ------------------------------------------------------------------------------------------------

/// Every written value with at most two dots, longest first: the palette a tie chain is written
/// with (ADR-0026 V3). 14 base values × 3 = 42 entries. Built on each call (no lazy static, no
/// allocation: an array), O(42).
fn palette() -> Result<[(NoteValue, Duration); 42], DocumentError> {
    let mut out = [(NoteValue::plain(BaseValue::Whole), Duration::ZERO); 42];
    let mut i = 0;
    for base in BaseValue::ALL {
        for dots in 0..=2 {
            let value = NoteValue::new(base, dots)?;
            if let Some(slot) = out.get_mut(i) {
                *slot = (value, value.length()?);
            }
            i += 1;
        }
    }
    // A double-dotted value is shorter than the next longer plain value, so sorting by length
    // (longest first) interleaves them correctly: a dotted half (3/4) before a half (1/2).
    out.sort_by_key(|entry| std::cmp::Reverse(entry.1));
    Ok(out)
}

/// The written values that add up to `length`, longest first (ADR-0026 V3): at each step the
/// longest value with at most two dots that still fits. Five eighths are a half and an eighth;
/// seven eighths a double-dotted half.
///
/// # Complexity
/// O(42 × values), at most [`MAX_VALUES_PER_PART`] values.
///
/// # Errors
/// [`DocumentError::NotWritable`] if the length is not a sum of such values (a third of a beat
/// left after a tuplet) or would need more than [`MAX_VALUES_PER_PART`] of them.
fn written_values(length: Duration) -> Result<Vec<NoteValue>, DocumentError> {
    let palette = palette()?;
    let mut left = length;
    let mut values = Vec::new();
    while left.is_positive() {
        if values.len() >= MAX_VALUES_PER_PART {
            return Err(DocumentError::NotWritable);
        }
        let &(value, value_length) = palette
            .iter()
            .find(|(_, l)| *l <= left)
            .ok_or(DocumentError::NotWritable)?;
        values.push(value);
        left = left.checked_sub(value_length)?;
    }
    Ok(values)
}

/// A chord with these noteheads, sorted as every chord is (by sounding pitch, then by letter:
/// `crate::event::Chord`), and the markings of `like` (value, tuplet, grace, articulations,
/// ornament, fermata, arpeggio) unless replaced. O(n log n) for n noteheads.
///
/// # Errors
/// [`DocumentError::EmptyChord`] for no notehead; [`DocumentError::Pitch`] if a sounding pitch
/// overflows.
pub(crate) fn chord_from(
    value: NoteValue,
    tuplet: Option<TupletId>,
    grace: Option<Grace>,
    notes: Vec<Note>,
    like: Option<&Chord>,
) -> Result<Chord, DocumentError> {
    let mut notes = notes;
    // Every sounding pitch is computed once up front, so that an overflow is reported (it cannot
    // be from inside the comparison below).
    for note in &notes {
        note.pitch().sounding()?;
    }
    // Sorted in place, without a list of keys: every event loaded goes through here, and a list
    // would cost an allocation per chord; a single notehead needs no sorting at all.
    // The keys cannot fail any more (checked above); `sort_by_key` is stable, so two noteheads
    // sounding the same with the same letter keep their order.
    if notes.len() > 1 {
        let key = |note: &Note| (note.pitch().sounding().ok(), note.pitch().diatonic_index());
        notes.sort_by_key(key);
    }
    let mut sorted = notes.into_iter();
    let lowest = sorted.next().ok_or(DocumentError::EmptyChord)?;
    let mut chord = Chord::from_sorted(value, tuplet, grace, lowest, sorted.collect());
    if let Some(like) = like {
        chord.set_articulations(like.articulations());
        chord.set_ornament(like.ornament());
        chord.set_arpeggio(like.arpeggio());
        chord.set_fermata(like.fermata());
    }
    Ok(chord)
}

/// A notehead with a new pitch, keeping its ID, tie and performance offsets. O(1).
fn with_pitch(note: &Note, pitch: Pitch) -> Note {
    let mut changed = Note::new(note.id(), pitch);
    changed.set_tie(note.tie());
    changed.set_offsets(note.offsets());
    changed
}

/// The event as an [`Object`] at its current place, ready to be changed and sent back with
/// [`Edit::Replace`]. O(1) average (the event index) + O(voices).
pub(crate) fn current(
    doc: &Document,
    id: EventId,
) -> Result<(VoiceId, BarId, PlacedEvent), DocumentError> {
    let found = doc.event(id).ok_or(DocumentError::UnknownEvent(id))?;
    Ok((
        found.voice(),
        found.bar(),
        PlacedEvent::new(found.offset(), found.event().clone()),
    ))
}

/// Puts an existing event (same ID) at `bar` and `offset` of `voice` with the content `event`,
/// unless it is already exactly there: the move of the meter change that rewrites the music
/// (`crate::rewrite`), which leaves an event that does not move untouched (no history). O(event
/// lookup) + the replacement.
///
/// # Errors
/// [`DocumentError::UnknownEvent`]; the edit's.
pub(crate) fn replace_event_at(
    tx: &mut Transaction<'_>,
    voice: VoiceId,
    bar: BarId,
    offset: Duration,
    event: Event,
) -> Result<(), CommandError> {
    let found = tx
        .document()
        .event(event.id())
        .ok_or(DocumentError::UnknownEvent(event.id()))?;
    if found.voice() == voice
        && found.bar() == bar
        && found.offset() == offset
        && *found.event() == event
    {
        return Ok(());
    }
    replace_event(tx, voice, bar, PlacedEvent::new(offset, event))
}

/// Sends an event back with a new content or place ([`Edit::Replace`], `place: None`: in place
/// when its sort key is unchanged, otherwise after the events with the same key at its new
/// place).
pub(crate) fn replace_event(
    tx: &mut Transaction<'_>,
    voice: VoiceId,
    bar: BarId,
    placed: PlacedEvent,
) -> Result<(), CommandError> {
    tx.apply(Edit::Replace(Object::Event {
        voice,
        bar,
        placed,
        place: None,
    }))
}

// ------------------------------------------------------------------------------------------------
// Entering an event (ADR-0026 V2, ADR-0026 V3, ADR-0026 V4, ADR-0026 V5, ADR-0026 W3)
// ------------------------------------------------------------------------------------------------

/// Executes [`Command::EnterEvent`].
///
/// # How
/// 1. The placement is checked as the builder checks it (voice, bar, staff, tuplet, offset).
/// 2. A grace chord and a whole-bar rest are entered as they are: they last zero, or exactly
///    their bar, so they never cross a barline.
/// 3. An event that ends inside its bar is entered with the value the user gave. One that ends
///    past the end of a **free** bar lengthens the bar to its end (ADR-0026 V5).
/// 4. Otherwise (it crosses a barline) it is cut at every barline, each part is written with
///    [`split_values`], and a chord's parts are tied notehead by notehead (ADR-0026 V3). Rests are
///    not tied. Inside a tuplet every part stays in the same tuplet, written in its own values
///    (ADR-0026 Z3, ADR-0026 Z4; ADR-0024 C4, Dorico's "spans barline"). Past the last bar it is
///    refused (ADR-0026 W3).
///
/// Free space (ADR-0026 V2) is not checked here: the local check that follows refuses an overlap.
///
/// # Complexity
/// O(voices + staves) to resolve, O(events × (k + noteheads)) to insert, k events per chunk.
#[allow(
    clippy::too_many_lines,
    reason = "the checks of the placement and the four cases of the How list, in order"
)]
fn enter_event(tx: &mut Transaction<'_>, new: &NewEvent) -> Result<(), CommandError> {
    let at = new.at;
    let doc = tx.document();
    let (instrument, voice) = doc
        .voice(at.voice)
        .ok_or(DocumentError::UnknownVoice(at.voice))?;
    let index = doc.bar_index(at.bar)?;
    if at.offset.is_negative() {
        return Err(DocumentError::NegativeOffset.into());
    }
    let staff = at.staff.unwrap_or(voice.home_staff());
    if instrument.staff(staff).is_none() {
        return Err(DocumentError::StaffOfAnotherInstrument(staff).into());
    }
    if let Some(tuplet) = at.tuplet
        && voice.tuplet(tuplet).is_none()
    {
        return Err(DocumentError::UnknownTuplet(tuplet).into());
    }
    let (kind, value) = match &new.body {
        NewBody::Chord { pitches, .. } if pitches.is_empty() => {
            return Err(DocumentError::EmptyChord.into());
        }
        NewBody::Chord { grace: Some(_), .. } if at.tuplet.is_some() => {
            return Err(DocumentError::GraceInTuplet.into());
        }
        NewBody::Rest(RestValue::WholeBar) if at.tuplet.is_some() => {
            return Err(DocumentError::WholeBarRestInTuplet.into());
        }
        NewBody::Chord {
            value,
            pitches,
            grace,
        } => (
            PartKind::Chord {
                pitches,
                grace: *grace,
            },
            *value,
        ),
        NewBody::Rest(RestValue::Value(value)) => (PartKind::Rest, *value),
        NewBody::Rest(RestValue::WholeBar) => {
            // Its value is ignored: a whole-bar rest lasts whatever its bar lasts.
            (PartKind::WholeBarRest, NoteValue::plain(BaseValue::Whole))
        }
    };
    let single = [(at.bar, at.offset, value)];
    // Grace chords and whole-bar rests are never split.
    if matches!(
        kind,
        PartKind::WholeBarRest | PartKind::Chord { grace: Some(_), .. }
    ) {
        return insert_chain(tx, &at, staff, kind, &single);
    }

    // The metric duration of the value at this place (with its tuplets): a probe event, never
    // inserted, so its ID does not matter.
    let bar_length = doc.meter_map().bar_length(index)?;
    let free = doc.meter_map().is_free(index)?;
    let probe = Event::new(
        EventId::from_counter(std::num::NonZeroU64::MIN),
        staff,
        EventBody::Rest(Rest::new(RestValue::Value(value), at.tuplet)),
    );
    let end = at
        .offset
        .checked_add(voice.metric_duration(&probe, bar_length)?)?;
    // An event starts inside its bar; a free bar also accepts one at its very end, which it then
    // grows to hold (ADR-0026 V5).
    let starts_past = if free {
        at.offset > bar_length
    } else {
        at.offset >= bar_length
    };
    if starts_past {
        return Err(DocumentError::StartsPastBarline.into());
    }
    // It fits in its bar: one event with the value the user gave.
    if end <= bar_length {
        return insert_chain(tx, &at, staff, kind, &single);
    }
    // A free bar grows to hold it (ADR-0026 V5).
    if free {
        tx.apply(Edit::Meter(MeterEdit::IrregularBar {
            bar: at.bar,
            irregular: Some(IrregularBar {
                length: end,
                kind: BarKind::Free,
            }),
        }))?;
        return insert_chain(tx, &at, staff, kind, &single);
    }
    // Cut at every barline: the first part fills its bar, the next ones fill whole bars, the last
    // one what is left (ADR-0026 V3). Past the last bar it is refused (ADR-0026 W3). Inside a
    // tuplet the parts stay in the tuplet and are written in its own values (ADR-0026 Z3,
    // ADR-0026 Z4): how much a written value lasts in the bar is scaled by the tuplet chain's
    // ratio, the same for every value of the chain.
    let ratio = tuplet_ratio(voice, &probe, value)?;
    let doc = tx.document();
    let mut chain: Vec<(BarId, Duration, NoteValue)> = Vec::new();
    push_parts(
        &mut chain,
        at.bar,
        at.offset,
        bar_length.checked_sub(at.offset)?,
        ratio,
    )?;
    let mut left = end.checked_sub(bar_length)?;
    let mut next = index;
    while left.is_positive() {
        next = BarIndex::new(
            next.get()
                .checked_add(1)
                .ok_or(DocumentError::PastLastBar)?,
        );
        let bar = doc.bars().id_at(next).ok_or(DocumentError::PastLastBar)?;
        let take = left.min(doc.meter_map().bar_length(next)?);
        push_parts(&mut chain, bar, Duration::ZERO, take, ratio)?;
        left = left.checked_sub(take)?;
    }
    insert_chain(tx, &at, staff, kind, &chain)
}

/// How much longer (or shorter) a written value lasts in the bar inside the tuplet chain of
/// `probe` than outside: the metric duration of the probe divided by the length of its written
/// value. 1 outside any tuplet, 2/3 in a triplet, 2/3 × 4/5 in a triplet inside a quintuplet. The
/// probe carries the innermost tuplet, so the voice's table gives the whole chain
/// (`Voice::metric_duration`). O(depth), at most four links.
///
/// # Errors
/// The arithmetic errors of the durations (absurd values only).
fn tuplet_ratio(
    voice: &crate::document::Voice,
    probe: &Event,
    value: NoteValue,
) -> Result<orchestra_core_time::Rational, DocumentError> {
    // A whole-bar length is never asked for: the probe is a rest of a written value.
    let metric = voice.metric_duration(probe, Duration::ZERO)?;
    Ok(metric.checked_ratio(value.length()?)?)
}

/// The written values of one part of a chain that lasts `length` in its bar, inside a tuplet
/// chain whose ratio is `ratio` (ADR-0026 Z3): the part is brought back to the tuplet's own written
/// values (divided by the ratio) and written with the longest values first, as outside a tuplet
/// (ADR-0026 V3). A triplet quarter (1/6) with 1/12 left in the bar becomes two triplet eighths:
/// 1/12 ÷ 2/3 = 1/8. Shared with the meter change that rewrites the music (F1.11c part 2,
/// ADR-0026 Z5).
///
/// # Complexity
/// As [`written_values`].
///
/// # Errors
/// [`DocumentError::NotWritable`] if the written length is no sum of values with at most two dots
/// (a part of a quintuplet cut at a third of a beat); the arithmetic errors.
pub(crate) fn split_values(
    length: Duration,
    ratio: orchestra_core_time::Rational,
) -> Result<Vec<NoteValue>, DocumentError> {
    written_values(length.checked_div(ratio)?)
}

/// What the events of an entered chain are.
#[derive(Clone, Copy)]
enum PartKind<'a> {
    /// Chords of these pitches (in the order the user gave them), or grace chords.
    Chord {
        /// The pitches.
        pitches: &'a [Pitch],
        /// `Some` for grace chords.
        grace: Option<Grace>,
    },
    /// Rests of the chain's values.
    Rest,
    /// One whole-bar rest (the chain's value is ignored).
    WholeBarRest,
}

/// Inserts the events of an entered chain, given as (bar, offset, value), in order. Chords get
/// new noteheads in every event; the notehead of the k-th given pitch is tied to the notehead of
/// the k-th given pitch of the next event (same pitch, so the tie is legal however the chords
/// sort them). Rests are not tied.
///
/// The IDs are taken in the order of the builder (an event's, then one per notehead), event by
/// event, so a chain is reproducible. O(events × (k + noteheads)).
fn insert_chain(
    tx: &mut Transaction<'_>,
    at: &Placement,
    staff: StaffId,
    kind: PartKind<'_>,
    chain: &[(BarId, Duration, NoteValue)],
) -> Result<(), CommandError> {
    let pitches: &[Pitch] = match kind {
        PartKind::Chord { pitches, .. } => pitches,
        PartKind::Rest | PartKind::WholeBarRest => &[],
    };
    // Every ID first (`chain_ids`: nothing is kept, the chain is new).
    let ids = chain_ids(tx, chain.len(), pitches.len(), &[])?;
    for (i, (&(bar, offset, value), (event, notes))) in chain.iter().zip(&ids).enumerate() {
        let body = match kind {
            PartKind::Rest => EventBody::Rest(Rest::new(RestValue::Value(value), at.tuplet)),
            PartKind::WholeBarRest => EventBody::Rest(Rest::new(RestValue::WholeBar, None)),
            PartKind::Chord { grace, .. } => {
                let mut made = Vec::with_capacity(pitches.len());
                for (k, (&id, &pitch)) in notes.iter().zip(pitches).enumerate() {
                    // Tied to the same pitch in the next part; the last part is not tied.
                    let mut note = Note::new(id, pitch);
                    note.set_tie(tie_to_next(&ids, i, k, Tie::None));
                    made.push(note);
                }
                EventBody::Chord(chord_from(value, at.tuplet, grace, made, None)?)
            }
        };
        tx.apply(Edit::Insert(Object::Event {
            voice: at.voice,
            bar,
            placed: PlacedEvent::new(offset, Event::new(*event, staff, body)),
            place: None,
        }))?;
    }
    Ok(())
}

// ------------------------------------------------------------------------------------------------
// Deleting events (ADR-0026 V6)
// ------------------------------------------------------------------------------------------------

/// The grace notes that ornament the main event `id` (ADR-0024 §5): the `Before` graces at its
/// offset, and the `After` graces right after it at the offset where it ends. A grace chord
/// ornaments nothing. O(k) for the k events of the chunk.
///
/// # Errors
/// The lookup and duration errors of the document (an unknown event).
pub(crate) fn graces_of(doc: &Document, id: EventId) -> Result<Vec<EventId>, DocumentError> {
    let found = doc.event(id).ok_or(DocumentError::UnknownEvent(id))?;
    if found.event().grace().is_some() {
        return Ok(Vec::new());
    }
    let (_, voice) = doc
        .voice(found.voice())
        .ok_or(DocumentError::UnknownVoice(found.voice()))?;
    let events = voice.events_at(doc.bar_index(found.bar())?);
    let offset = found.offset();
    let end = offset.checked_add(doc.metric_duration(id)?)?;
    let place = events
        .iter()
        .position(|p| p.event().id() == id)
        .ok_or(DocumentError::inconsistent())?;
    let mut graces = Vec::new();
    // The `Before` graces at its offset sort just before it (same offset, earlier slot).
    for placed in events.get(..place).unwrap_or(&[]).iter().rev() {
        match placed.event().grace() {
            Some(g) if g.attach == GraceAttach::Before && placed.offset() == offset => {
                graces.push(placed.event().id());
            }
            _ => break,
        }
    }
    // The `After` graces at its end come after it, before the next main event: at one offset the
    // `After` graces sort first, and no main event can start between it and its end.
    for placed in events.get(place.saturating_add(1)..).unwrap_or(&[]) {
        if placed.offset() > end {
            break;
        }
        match placed.event().grace() {
            Some(g) if g.attach == GraceAttach::After && placed.offset() == end => {
                graces.push(placed.event().id());
            }
            Some(_) => {}
            None => break,
        }
    }
    Ok(graces)
}

/// Deletes events (ADR-0026 V6): each one with the grace notes ornamenting it, the slurs starting
/// or ending on any of them, the ties leading into them from events that stay (set to
/// [`Tie::None`]), and the tuplets left without members (nested ones first, then their parents).
///
/// Used by [`Command::DeleteEvent`], by removing the last notehead of a chord and by
/// [`Command::DeleteBars`] (for every event of the deleted bars).
///
/// # Complexity
/// O(e × (k + noteheads + slurs)) for e events, plus O(events of the voice) per tuplet that may
/// have lost its last member (the "in use" check of the edit).
pub(crate) fn delete_events(tx: &mut Transaction<'_>, ids: &[EventId]) -> Result<(), CommandError> {
    let doc = tx.document();
    // Everything that goes: the events and their grace notes, without duplicates, in a stable
    // order (the given one, each followed by its graces).
    let mut gone: Vec<EventId> = Vec::new();
    let mut seen: IdSet<EventId> = IdSet::default();
    for &id in ids {
        if doc.event(id).is_none() {
            return Err(DocumentError::UnknownEvent(id).into());
        }
        for event in std::iter::once(id).chain(graces_of(doc, id)?) {
            if seen.insert(event) {
                gone.push(event);
            }
        }
    }
    // The slurs on them, and the ties leading into them from events that stay, grouped by the
    // event they start from (one replacement per such event).
    let mut slurs: Vec<crate::id::SlurId> = Vec::new();
    let mut untie: IdMap<EventId, Vec<NoteId>> = IdMap::default();
    let mut untie_order: Vec<EventId> = Vec::new();
    let mut tuplets: Vec<(VoiceId, TupletId)> = Vec::new();
    for &id in &gone {
        slurs.extend_from_slice(doc.slurs_at(id));
        let found = doc.event(id).ok_or(DocumentError::UnknownEvent(id))?;
        if let Some(tuplet) = found.event().tuplet() {
            tuplets.push((found.voice(), tuplet));
        }
        for note in found.event().chord().into_iter().flat_map(Chord::notes) {
            for &source in doc.ties_into(note.id()) {
                let (from, _) = doc.note(source).ok_or(DocumentError::inconsistent())?;
                let owner = from.event().id();
                if !seen.contains(&owner) {
                    untie
                        .entry(owner)
                        .or_insert_with(|| {
                            untie_order.push(owner);
                            Vec::new()
                        })
                        .push(source);
                }
            }
        }
    }
    slurs.sort_unstable();
    slurs.dedup();
    for slur in slurs {
        tx.apply(Edit::Remove(ObjectRef::Slur(slur)))?;
    }
    for owner in untie_order {
        let notes = untie.get(&owner).cloned().unwrap_or_default();
        let (voice, bar, mut placed) = current(tx.document(), owner)?;
        if let EventBody::Chord(chord) = placed.event_mut().body_mut() {
            for note in notes {
                if let Some(n) = chord.note_mut(note) {
                    n.set_tie(Tie::None);
                }
            }
        }
        replace_event(tx, voice, bar, placed)?;
    }
    for &id in &gone {
        tx.apply(Edit::Remove(ObjectRef::Event(id)))?;
    }
    // Tuplets left without members, then their parents: a tuplet still used (by an event or a
    // nested tuplet) is refused by the edit with `TupletInUse`, which here simply means "keep it".
    tuplets.sort_unstable();
    tuplets.dedup();
    for (voice, tuplet) in tuplets {
        let mut next = Some(tuplet);
        while let Some(t) = next {
            let parent = tx
                .document()
                .voice(voice)
                .and_then(|(_, v)| v.tuplet(t))
                .and_then(|tu| tu.parent());
            match tx.apply(Edit::Remove(ObjectRef::Tuplet(t))) {
                Ok(()) => next = parent,
                Err(CommandError::Document(
                    DocumentError::TupletInUse(_) | DocumentError::UnknownObject(_),
                )) => next = None,
                Err(e) => return Err(e),
            }
        }
    }
    Ok(())
}

// ------------------------------------------------------------------------------------------------
// Changing an event (ADR-0026 V7, ADR-0026 W1, ADR-0026 W2)
// ------------------------------------------------------------------------------------------------

/// Executes [`Command::ChangeEvent`]: builds the changed event and sends it back with
/// [`Edit::Replace`] (same ID). The local check that follows refuses a change that breaks a rule
/// (an overlap, a tie to another pitch, a grace note left without its event).
///
/// # Complexity
/// O(voices + k + noteheads) for most changes; a move or a new value also moves the grace notes
/// ornamenting the event (O(k)); a new pitch walks the tie chain (O(chain)).
#[allow(
    clippy::too_many_lines,
    reason = "one short arm per kind of change: splitting the match would only scatter it"
)]
pub(crate) fn change_event(
    tx: &mut Transaction<'_>,
    id: EventId,
    change: EventChange,
) -> Result<(), CommandError> {
    let (voice, bar, mut placed) = current(tx.document(), id)?;
    match change {
        EventChange::Move {
            bar: to,
            offset: new_offset,
        } => {
            if new_offset.is_negative() {
                return Err(DocumentError::NegativeOffset.into());
            }
            tx.document().bar_index(to)?;
            // The grace notes move by the same amount (ADR-0026 W1: nothing is split). Before
            // graces sit at the event's offset, After graces at its end.
            let graces = graces_of(tx.document(), id)?;
            let shift = new_offset.checked_sub(placed.offset())?;
            replace_event(
                tx,
                voice,
                to,
                PlacedEvent::new(new_offset, placed.event().clone()),
            )?;
            for grace in graces {
                let (v, _, g) = current(tx.document(), grace)?;
                let offset = g.offset().checked_add(shift)?;
                if offset.is_negative() {
                    return Err(DocumentError::NegativeOffset.into());
                }
                replace_event(tx, v, to, PlacedEvent::new(offset, g.event().clone()))?;
            }
            Ok(())
        }
        EventChange::Value(value) => {
            let graces = graces_of(tx.document(), id)?;
            let old_end = placed
                .offset()
                .checked_add(tx.document().metric_duration(id)?)?;
            let event = placed.event_mut();
            let staff = event.staff();
            let body = match event.body() {
                EventBody::Chord(chord) => EventBody::Chord(chord_from(
                    value,
                    chord.tuplet(),
                    chord.grace(),
                    chord.notes().copied().collect(),
                    Some(chord),
                )?),
                EventBody::Rest(rest) => {
                    // A whole-bar rest becomes a rest of the value: never in a tuplet.
                    let mut changed = Rest::new(RestValue::Value(value), rest.tuplet());
                    changed.set_fermata(rest.fermata());
                    EventBody::Rest(changed)
                }
            };
            let changed = PlacedEvent::new(placed.offset(), Event::new(id, staff, body));
            replace_event(tx, voice, bar, changed)?;
            // The After graces follow the new end.
            let new_end = placed
                .offset()
                .checked_add(tx.document().metric_duration(id)?)?;
            let shift = new_end.checked_sub(old_end)?;
            for grace in graces {
                let (v, b, g) = current(tx.document(), grace)?;
                if g.event()
                    .grace()
                    .is_some_and(|x| x.attach == GraceAttach::After)
                {
                    let offset = g.offset().checked_add(shift)?;
                    replace_event(tx, v, b, PlacedEvent::new(offset, g.event().clone()))?;
                }
            }
            Ok(())
        }
        EventChange::Pitch { note, pitch } => change_pitch(tx, note, pitch),
        EventChange::AddNote(pitch) => {
            let new_note = NoteId::from_counter(tx.new_id()?);
            let EventBody::Chord(chord) = placed.event().body() else {
                return Err(DocumentError::NotAChord(id).into());
            };
            let mut notes: Vec<Note> = chord.notes().copied().collect();
            notes.push(Note::new(new_note, pitch));
            replace_noteheads(tx, voice, bar, &placed, notes)
        }
        EventChange::RemoveNote(note) => {
            let EventBody::Chord(chord) = placed.event().body() else {
                return Err(DocumentError::NotAChord(id).into());
            };
            if !chord.notes().any(|n| n.id() == note) {
                return Err(DocumentError::UnknownNote(note).into());
            }
            // The last notehead: the event goes, as a deletion (ADR-0026 V6).
            if chord.len() == 1 {
                return delete_events(tx, &[id]);
            }
            // The ties leading into the notehead are cleared first (they would point at nothing).
            let sources: Vec<NoteId> = tx.document().ties_into(note).to_vec();
            for source in sources {
                set_tie(tx, source, Tie::None)?;
            }
            let (voice, bar, placed) = current(tx.document(), id)?;
            let EventBody::Chord(chord) = placed.event().body() else {
                return Err(DocumentError::NotAChord(id).into());
            };
            let notes: Vec<Note> = chord.notes().filter(|n| n.id() != note).copied().collect();
            replace_noteheads(tx, voice, bar, &placed, notes)
        }
        EventChange::Tie { note, tie } => {
            if tie == Tie::To(note) {
                return Err(DocumentError::TieToItself.into());
            }
            if !note_of(tx.document(), id, note) {
                return Err(DocumentError::UnknownNote(note).into());
            }
            set_tie(tx, note, tie)
        }
        EventChange::Articulations(set) => {
            chord_mut(&mut placed, id)?.set_articulations(set);
            replace_event(tx, voice, bar, placed)
        }
        EventChange::Ornament(ornament) => {
            chord_mut(&mut placed, id)?.set_ornament(ornament);
            replace_event(tx, voice, bar, placed)
        }
        EventChange::Arpeggio(arpeggio) => {
            chord_mut(&mut placed, id)?.set_arpeggio(arpeggio);
            replace_event(tx, voice, bar, placed)
        }
        EventChange::Fermata(fermata) => {
            placed.event_mut().set_fermata(fermata);
            replace_event(tx, voice, bar, placed)
        }
        EventChange::Offsets { note, offsets } => {
            if !note_of(tx.document(), id, note) {
                return Err(DocumentError::UnknownNote(note).into());
            }
            if let Some(n) = chord_mut(&mut placed, id)?.note_mut(note) {
                n.set_offsets(offsets);
            }
            replace_event(tx, voice, bar, placed)
        }
        EventChange::Staff(staff) => {
            let doc = tx.document();
            let (instrument, _) = doc.voice(voice).ok_or(DocumentError::UnknownVoice(voice))?;
            if instrument.staff(staff).is_none() {
                return Err(DocumentError::StaffOfAnotherInstrument(staff).into());
            }
            let event = placed.event();
            let changed = Event::new(id, staff, event.body().clone());
            replace_event(tx, voice, bar, PlacedEvent::new(placed.offset(), changed))
        }
    }
}

/// Replaces the chord `placed` (in `voice`, `bar`) by the same chord with the noteheads `notes`:
/// same value, tuplet, grace and markings (`chord_from` with the old chord as model), sorted
/// again, same offset, staff and ID. O(noteheads log noteheads).
///
/// The one copy of a step shared by adding a notehead, removing one and changing a pitch.
///
/// # Errors
/// [`DocumentError::NotAChord`] if `placed` is a rest (callers have checked it already); those of
/// `chord_from` and of the replacement.
fn replace_noteheads(
    tx: &mut Transaction<'_>,
    voice: VoiceId,
    bar: BarId,
    placed: &PlacedEvent,
    notes: Vec<Note>,
) -> Result<(), CommandError> {
    let event = placed.event();
    let EventBody::Chord(chord) = event.body() else {
        return Err(DocumentError::NotAChord(event.id()).into());
    };
    let body = EventBody::Chord(chord_from(
        chord.value(),
        chord.tuplet(),
        chord.grace(),
        notes,
        Some(chord),
    )?);
    replace_event(
        tx,
        voice,
        bar,
        PlacedEvent::new(placed.offset(), Event::new(event.id(), event.staff(), body)),
    )
}

/// Whether `note` is a notehead of event `id`. O(1) average (the note index).
fn note_of(doc: &Document, id: EventId, note: NoteId) -> bool {
    doc.note(note).is_some_and(|(e, _)| e.event().id() == id)
}

/// The chord of an event being changed, or [`DocumentError::NotAChord`] for a rest. O(1).
pub(crate) fn chord_mut(
    placed: &mut PlacedEvent,
    id: EventId,
) -> Result<&mut Chord, DocumentError> {
    match placed.event_mut().body_mut() {
        EventBody::Chord(chord) => Ok(chord),
        EventBody::Rest(_) => Err(DocumentError::NotAChord(id)),
    }
}

/// Sets the tie of one notehead, replacing its event. O(1) average + O(voices + k).
fn set_tie(tx: &mut Transaction<'_>, note: NoteId, tie: Tie) -> Result<(), CommandError> {
    let event = tx
        .document()
        .note(note)
        .map(|(e, _)| e.event().id())
        .ok_or(DocumentError::UnknownNote(note))?;
    let (voice, bar, mut placed) = current(tx.document(), event)?;
    if let Some(n) = chord_mut(&mut placed, event)?.note_mut(note) {
        n.set_tie(tie);
    }
    replace_event(tx, voice, bar, placed)
}

/// Changes the pitch of a notehead and of every notehead of its tie chain (ADR-0026 W2): the chain
/// is followed forward through each [`Tie::To`] and backward through the tie index, so that it is
/// found from any of its noteheads. Each event of the chain is replaced once, its chord sorted
/// again. O(chain × (voices + k + noteheads)).
fn change_pitch(tx: &mut Transaction<'_>, note: NoteId, pitch: Pitch) -> Result<(), CommandError> {
    let doc = tx.document();
    if doc.note(note).is_none() {
        return Err(DocumentError::UnknownNote(note).into());
    }
    // The chain, by a walk over the ties in both directions; `seen` stops a loop (a tie that is
    // not forward never passes `validate`, but the walk must not depend on that).
    let mut chain: Vec<NoteId> = Vec::new();
    let mut seen: IdSet<NoteId> = IdSet::default();
    let mut todo = vec![note];
    while let Some(n) = todo.pop() {
        if !seen.insert(n) {
            continue;
        }
        let Some((_, found)) = doc.note(n) else {
            continue;
        };
        chain.push(n);
        if let Tie::To(next) = found.tie() {
            todo.push(next);
        }
        todo.extend_from_slice(doc.ties_into(n));
    }
    // Group by event, in a stable order (the order the walk found them).
    let mut events: Vec<(EventId, Vec<NoteId>)> = Vec::new();
    for n in chain {
        let event = doc
            .note(n)
            .map(|(e, _)| e.event().id())
            .ok_or(DocumentError::UnknownNote(n))?;
        match events.iter_mut().find(|(e, _)| *e == event) {
            Some((_, notes)) => notes.push(n),
            None => events.push((event, vec![n])),
        }
    }
    for (event, notes) in events {
        let (voice, bar, placed) = current(tx.document(), event)?;
        let EventBody::Chord(chord) = placed.event().body() else {
            return Err(DocumentError::NotAChord(event).into());
        };
        let changed: Vec<Note> = chord
            .notes()
            .map(|n| {
                if notes.contains(&n.id()) {
                    with_pitch(n, pitch)
                } else {
                    *n
                }
            })
            .collect();
        replace_noteheads(tx, voice, bar, &placed, changed)?;
    }
    Ok(())
}

// ------------------------------------------------------------------------------------------------
// Inserting bars (ADR-0026 §6, W6)
// ------------------------------------------------------------------------------------------------

/// Executes [`Command::InsertBars`]: one [`Edit::InsertBars`] with new IDs.
///
/// # What the new bars get
/// Nothing of their own: with an empty meter span they are regular bars of the meter in force
/// just before them (`MeterMap::insert_bars`), and the clef, key and tempo marks stay on their
/// bars, so before the new bars the same ones are in force. At bar 0 the span carries a copy of
/// the first bar's meter change, which also stays on the old first bar: the new bars and the old
/// first bar keep the same meter, and the initial clef, key and tempo now apply to the new bars
/// first. Ranges whose ends are on both sides of the point grow, because their ends are `BarId`s.
///
/// # Ends at the insertion point (ADR-0026 Y8)
/// A hairpin, a pass rule or a gradual tempo change that ends exactly at the start of bar `at`
/// (its end is exclusive) does not span the point: it keeps ending there, before the new bars,
/// anchored to the start of the first new bar (owner's choice, 2026-11-05).
///
/// # Between two voltas of a group (ADR-0026 X11)
/// A point between two voltas of one group (the first one ending on the bar before `at`, the
/// next one starting on bar `at`) is spanned by no range, so the new bars would fall between the
/// voltas, outside both, and the group would no longer be valid. There they **join the earlier
/// volta**, which ends on the last new bar, and the end repeat that closed it moves to the end of
/// that bar too (owner's choice, 2026-11-05): the new bars take what is in force just before them,
/// as everywhere else, and the form stays valid.
///
/// # Refused (ADR-0026 W6)
/// Between the first bar of a bar repeat's source and the end of the bar repeat (exclusive at
/// both ends): the new bars would either change what the repeat repeats or become repeated bars
/// of a span they do not fit. [`DocumentError::InsideBarRepeat`].
///
/// # Complexity
/// O(r + f) for the r bar repeats and the f form markings, then the edit: O(B × V) pointer moves
/// and the meter map rebuild.
fn insert_bars(tx: &mut Transaction<'_>, at: BarIndex, count: u32) -> Result<(), CommandError> {
    if count == 0 {
        return Ok(());
    }
    let doc = tx.document();
    let total = u32::try_from(doc.bars().len()).map_err(|_| DocumentError::TooManyBars)?;
    if at.get() > total {
        return Err(DocumentError::TooManyBars.into());
    }
    for repeat in doc
        .instruments()
        .flat_map(crate::document::Instrument::bar_repeats)
    {
        let (source, _, end) = repeat.covered(doc.bars());
        if source < at.get() && at.get() < end {
            return Err(DocumentError::InsideBarRepeat(repeat.id()).into());
        }
    }
    let mut meter = BarSpan::empty(count);
    if at == BarIndex::FIRST {
        // The map needs a meter change at bar 0: the new first bars take the old first bar's.
        let &(_, first) = doc
            .meter_map()
            .meter_changes()
            .first()
            .ok_or(DocumentError::inconsistent())?;
        meter.set_meter_change(BarIndex::FIRST, first);
    }
    // ADR-0026 X11: the volta ending just before the point, when the next volta of its group starts
    // right at it, and the end repeats at the end of its last bar. Read before the bars move.
    let mut grow: Vec<crate::form::Volta> = Vec::new();
    let mut repeat_ends: Vec<crate::form::RepeatEnd> = Vec::new();
    let before = at
        .get()
        .checked_sub(1)
        .and_then(|i| doc.bars().id_at(BarIndex::new(i)));
    if let (Some(before), Some(next)) = (before, doc.bars().id_at(at)) {
        for pair in doc.form().voltas().windows(2) {
            if let [earlier, later] = pair
                && earlier.last() == before
                && later.first() == next
                && !later.passes().contains(1)
            {
                grow.push(earlier.clone());
            }
        }
        if !grow.is_empty() {
            repeat_ends.extend(
                doc.form()
                    .repeat_ends()
                    .iter()
                    .filter(|r| r.bar() == before)
                    .copied(),
            );
        }
    }
    let mut ids = Vec::with_capacity(usize::try_from(count).unwrap_or(0));
    for _ in 0..count {
        ids.push(BarId::from_counter(tx.new_id()?));
    }
    let last = *ids.last().ok_or(DocumentError::inconsistent())?;
    let first_new = *ids.first().ok_or(DocumentError::inconsistent())?;
    // ADR-0026 Y8: the exclusive ends exactly at the start of bar `at`, read before the bars move.
    let doc = tx.document();
    let ends = match doc.bars().id_at(at) {
        Some(next) => ends_at(
            doc,
            ScorePoint::start_of(next),
            ScorePoint::start_of(first_new),
        )?,
        None => Vec::new(),
    };
    tx.apply(Edit::InsertBars { at, ids, meter })?;
    for edit in ends {
        tx.apply(edit)?;
    }
    for volta in grow {
        tx.apply(Edit::Replace(Object::Volta(
            volta.with_bars(volta.first(), last),
        )))?;
    }
    for repeat in repeat_ends {
        tx.apply(Edit::Replace(Object::RepeatEnd(repeat.with_bar(last))))?;
    }
    Ok(())
}

/// The edits that keep the exclusive ends lying exactly at `point` (the start of the bar before
/// which bars are inserted) where they are, before the new bars, by anchoring them to `to` (the
/// start of the first new bar) (ADR-0026 Y8): hairpins, pass rules and gradual tempo changes "up
/// to" that barline do not span the insertion point, so they must not grow over the new bars.
/// Otherwise a pass rule could reach out of its repeated section.
///
/// # Complexity
/// O(hairpins + pass rules + tempo marks).
///
/// # Errors
/// [`DocumentError::EmptyTempoMark`] cannot happen (a gradual mark keeps its change), but the
/// constructor's error is propagated rather than unwrapped.
fn ends_at(doc: &Document, point: ScorePoint, to: ScorePoint) -> Result<Vec<Edit>, DocumentError> {
    let mut edits = Vec::new();
    for instrument in doc.instruments() {
        for h in instrument.hairpins().iter().filter(|h| h.end() == point) {
            edits.push(Edit::Replace(Object::Hairpin {
                instrument: instrument.id(),
                hairpin: h.with_points(h.start(), to),
            }));
        }
        for r in instrument.pass_rules().iter().filter(|r| r.end() == point) {
            edits.push(Edit::Replace(Object::PassRule {
                instrument: instrument.id(),
                rule: r.with_points(r.start(), to),
            }));
        }
    }
    for mark in doc.tempo_marks().as_slice() {
        if let Some(TempoMarkChange::Gradual(g)) = mark.value().change()
            && g.end == point
        {
            edits.push(Edit::Replace(Object::TempoMark(Mark::new(
                mark.id(),
                mark.bar(),
                mark.offset(),
                mark.value().with_gradual_end(to)?,
            ))));
        }
    }
    Ok(edits)
}

// ------------------------------------------------------------------------------------------------
// Deleting bars (ADR-0026 §6, W4, W5)
// ------------------------------------------------------------------------------------------------

/// The deleted run of bars, and what a point in it becomes.
struct Cut {
    /// Index of the first deleted bar.
    first: u32,
    /// Index of the first bar after the run (it may not exist).
    end: u32,
    /// The deleted bars.
    gone: IdSet<BarId>,
    /// The first bar after the run, if any.
    next: Option<BarId>,
    /// Where a point inside the run goes: the start of the first bar after the run or, when the
    /// run is the end of the score, the end of the last remaining bar.
    point: ScorePoint,
}

impl Cut {
    /// Whether the bar is deleted. O(1) average.
    fn has(&self, bar: BarId) -> bool {
        self.gone.contains(&bar)
    }

    /// A point moved out of the run if it is inside it. O(1).
    fn clip(&self, point: ScorePoint) -> ScorePoint {
        if self.has(point.bar) {
            self.point
        } else {
            point
        }
    }
}

/// Executes [`Command::DeleteBars`] (see its documentation for what it does). The steps, in
/// order, each with ordinary edits, so that the final [`Edit::RemoveBars`] finds the bars empty:
///
/// 1. the events of the run, as [`Command::DeleteEvent`] does (ADR-0026 V6);
/// 2. the dynamics on the run (ADR-0026 U10); hairpins and pass rules clipped or deleted;
/// 3. voltas and bar repeats clipped or deleted (a bar repeat whose source loses a bar is
///    deleted; one that loses repeated bars keeps a multiple of its span, ADR-0026 W5);
/// 4. the repeat barlines, markers and jumps on the run;
/// 5. the state marks: the meter, the clef of each staff, the key, the key of each instrument and
///    the tempo last in force inside the run move to the start of the first bar after it, unless
///    that bar has its own (ADR-0026 U10); at the start of the score they become the initial
///    values; gradual tempo changes are clipped (ADR-0026 W4);
/// 6. the bars;
/// 7. the cascade (ADR-0026 W5): the form checks of `validate` run again, and each marking they
///    report is deleted, until the form is valid.
///
/// # Complexity
/// O(events of the run) for step 1, O(marks + spanners + form markings of the score) for the
/// others, O(B × V) for the bars, and O(f) form checks per marking deleted in cascade (few).
fn delete_bars(tx: &mut Transaction<'_>, at: BarIndex, count: u32) -> Result<(), CommandError> {
    if count == 0 {
        return Ok(());
    }
    let doc = tx.document();
    let total = doc.bars().len();
    let first = usize::try_from(at.get()).map_err(|_| DocumentError::TooManyBars)?;
    let end = first
        .checked_add(usize::try_from(count).map_err(|_| DocumentError::TooManyBars)?)
        .filter(|&end| end <= total)
        .ok_or(DocumentError::TooManyBars)?;
    if first == 0 && end == total {
        return Err(DocumentError::NoBarsLeft.into());
    }
    let ids = doc.bars().ids();
    let next = ids.get(end).copied();
    let point = if let Some(bar) = next {
        ScorePoint::start_of(bar)
    } else {
        // The run is the end of the score: a point in it goes to the end of the bar before.
        let last = first.checked_sub(1).ok_or(DocumentError::NoBarsLeft)?;
        let bar = *ids.get(last).ok_or(DocumentError::inconsistent())?;
        let index = doc.bar_index(bar)?;
        ScorePoint::new(bar, doc.meter_map().bar_length(index)?)
    };
    let cut = Cut {
        first: at.get(),
        end: u32::try_from(end).map_err(|_| DocumentError::TooManyBars)?,
        gone: ids.get(first..end).unwrap_or(&[]).iter().copied().collect(),
        next,
        point,
    };

    // 1. The events of the run.
    let mut events = Vec::new();
    for instrument in doc.instruments() {
        for voice in instrument.voices() {
            for index in cut.first..cut.end {
                events.extend(
                    voice
                        .events_at(BarIndex::new(index))
                        .iter()
                        .map(|p| p.event().id()),
                );
            }
        }
    }
    delete_events(tx, &events)?;

    clip_spanners(tx, &cut)?;
    clip_form(tx, &cut)?;
    carry_state(tx, &cut)?;
    carry_tempo(tx, &cut)?;

    // 6. The bars, now empty.
    tx.apply(Edit::RemoveBars { at, count })?;

    // 7. The cascade (ADR-0026 W5).
    cascade(tx)
}

/// Step 2 of [`delete_bars`]: dynamics on the run deleted; hairpins and pass rules clipped,
/// deleted when nothing of them is left. O(dynamics + hairpins + pass rules).
fn clip_spanners(tx: &mut Transaction<'_>, cut: &Cut) -> Result<(), CommandError> {
    let doc = tx.document();
    let bars = doc.bars();
    let mut edits = Vec::new();
    for instrument in doc.instruments() {
        for dynamic in instrument.dynamics() {
            if cut.has(dynamic.at().bar) {
                edits.push(Edit::Remove(ObjectRef::Dynamic(dynamic.id())));
            }
        }
        for hairpin in instrument.hairpins() {
            let (start, end) = (cut.clip(hairpin.start()), cut.clip(hairpin.end()));
            if point_key(bars, end) <= point_key(bars, start) {
                edits.push(Edit::Remove(ObjectRef::Hairpin(hairpin.id())));
            } else if (start, end) != (hairpin.start(), hairpin.end()) {
                edits.push(Edit::Replace(Object::Hairpin {
                    instrument: instrument.id(),
                    hairpin: hairpin.with_points(start, end),
                }));
            }
        }
        for rule in instrument.pass_rules() {
            let (start, end) = (cut.clip(rule.start()), cut.clip(rule.end()));
            if point_key(bars, end) <= point_key(bars, start) {
                edits.push(Edit::Remove(ObjectRef::PassRule(rule.id())));
            } else if (start, end) != (rule.start(), rule.end()) {
                edits.push(Edit::Replace(Object::PassRule {
                    instrument: instrument.id(),
                    rule: rule.with_points(start, end),
                }));
            }
        }
    }
    for edit in edits {
        tx.apply(edit)?;
    }
    Ok(())
}

/// Steps 3 and 4 of [`delete_bars`]: voltas and bar repeats clipped or deleted; repeat
/// barlines, markers and jumps on the run deleted. O(form markings + bar repeats).
fn clip_form(tx: &mut Transaction<'_>, cut: &Cut) -> Result<(), CommandError> {
    let doc = tx.document();
    let bars = doc.bars();
    let index = |bar: BarId| bars.key(bar);
    let mut edits = Vec::new();
    let form = doc.form();
    for volta in form.voltas() {
        let first = if cut.has(volta.first()) {
            cut.next
        } else {
            Some(volta.first())
        };
        let last = if cut.has(volta.last()) {
            // The bar before the run (there is one, or every bar from the start was deleted
            // up to past the volta's first bar, which the line above handles).
            cut.first
                .checked_sub(1)
                .and_then(|i| bars.id_at(BarIndex::new(i)))
        } else {
            Some(volta.last())
        };
        match (first, last) {
            (Some(f), Some(l)) if index(f) <= index(l) => {
                if (f, l) != (volta.first(), volta.last()) {
                    edits.push(Edit::Replace(Object::Volta(volta.with_bars(f, l))));
                }
            }
            _ => edits.push(Edit::Remove(ObjectRef::Volta(volta.id()))),
        }
    }
    for repeat in doc
        .instruments()
        .flat_map(crate::document::Instrument::bar_repeats)
    {
        if let Some(edit) = clip_bar_repeat(doc, cut, repeat) {
            edits.push(edit);
        }
    }
    for r in form.repeat_starts() {
        if cut.has(r.bar()) {
            edits.push(Edit::Remove(ObjectRef::RepeatStart(r.id())));
        }
    }
    for r in form.repeat_ends() {
        if cut.has(r.bar()) {
            edits.push(Edit::Remove(ObjectRef::RepeatEnd(r.id())));
        }
    }
    for m in form.markers() {
        if cut.has(m.bar()) {
            edits.push(Edit::Remove(ObjectRef::Marker(m.id())));
        }
    }
    for j in form.jumps() {
        if cut.has(j.bar()) {
            edits.push(Edit::Remove(ObjectRef::Jump(j.id())));
        }
    }
    for edit in edits {
        tx.apply(edit)?;
    }
    Ok(())
}

/// What happens to one bar repeat when bars are deleted (ADR-0026 W5): deleted if its source loses
/// a bar or nothing of it is left, shortened to a multiple of its span if it loses repeated bars,
/// and moved to the first bar after the run if its first bar is deleted. `None` if it is untouched.
/// O(1).
fn clip_bar_repeat(doc: &Document, cut: &Cut, repeat: &BarRepeat) -> Option<Edit> {
    let span = repeat.span().bars();
    let (source, first, end) = repeat.covered(doc.bars());
    // Bars of [a, b) inside the run.
    let overlap = |a: u32, b: u32| b.min(cut.end).saturating_sub(a.max(cut.first));
    let remove = Some(Edit::Remove(ObjectRef::BarRepeat(repeat.id())));
    if overlap(source, first) > 0 {
        return remove;
    }
    let lost = overlap(first, end);
    if lost == 0 {
        return None;
    }
    let left = repeat.count().saturating_sub(lost);
    let count = left.saturating_sub(left % span);
    if count == 0 {
        return remove;
    }
    let first_bar = if cut.has(repeat.first()) {
        cut.next?
    } else {
        repeat.first()
    };
    Some(Edit::Replace(Object::BarRepeat(
        repeat.with_bars(first_bar, count),
    )))
}

/// Step 5 of [`delete_bars`] for the meter, the clefs, the key and the key overrides
/// (ADR-0026 U10).
///
/// For each kind (and each staff or instrument): the marks on the run are deleted, except the
/// last one, which moves to the start of the first bar after the run unless that bar already has
/// one there. When the run starts the score, what lands at the start becomes the initial value
/// (a clef or key change at the very start would collide with it), including a change the first
/// bar after the run already had. Without a bar after the run everything on it is deleted.
///
/// # Complexity
/// O(marks of each list).
fn carry_state(tx: &mut Transaction<'_>, cut: &Cut) -> Result<(), CommandError> {
    let doc = tx.document();
    let at_start = cut.first == 0;
    let mut edits = Vec::new();

    // The meter: the map's own entries (the last change inside the run moves to the next bar).
    // The list is sorted: the last entry before the run's end is found by a binary search, and
    // it is inside the run if it is not before its first bar.
    let changes = doc.meter_map().meter_changes();
    let last_meter = changes
        .partition_point(|(bar, _)| bar.get() < cut.end)
        .checked_sub(1)
        .and_then(|i| changes.get(i))
        .filter(|(bar, _)| bar.get() >= cut.first);
    if let (Some(&(_, change)), Some(next)) = (last_meter, cut.next) {
        let has_own = doc
            .meter_map()
            .meter_change_at(BarIndex::new(cut.end))
            .is_some();
        if !has_own {
            edits.push(Edit::Meter(MeterEdit::MeterChange {
                bar: next,
                change: Some(change),
            }));
        }
    }

    // Clef changes, staff by staff.
    for staff in doc
        .instruments()
        .flat_map(crate::document::Instrument::staves)
    {
        let marks = staff.clef_changes().as_slice();
        carry_marks(
            cut,
            at_start,
            marks,
            |m| ObjectRef::ClefChange(m.id()),
            |mark| {
                Edit::Replace(Object::ClefChange {
                    staff: staff.id(),
                    mark,
                })
            },
            |value| {
                Some(Edit::SetInitial(Initial::Clef {
                    staff: staff.id(),
                    clef: value,
                }))
            },
            &mut edits,
        );
    }
    // Key changes of the score.
    carry_marks(
        cut,
        at_start,
        doc.key_changes().as_slice(),
        |m| ObjectRef::KeyChange(m.id()),
        |mark| Edit::Replace(Object::KeyChange(mark)),
        |value| Some(Edit::SetInitial(Initial::Key(value))),
        &mut edits,
    );
    // Key overrides, instrument by instrument: they have no initial value, so at the start of the
    // score they simply stay at the start of the first bar.
    for instrument in doc.instruments() {
        carry_marks(
            cut,
            false,
            instrument.key_overrides().as_slice(),
            |m| ObjectRef::KeyOverride(m.id()),
            |mark| {
                Edit::Replace(Object::KeyOverride {
                    instrument: instrument.id(),
                    mark,
                })
            },
            |_| None,
            &mut edits,
        );
    }
    for edit in edits {
        tx.apply(edit)?;
    }
    Ok(())
}

/// The rule of [`carry_state`] for one list of marks, sorted in score order: pushes the edits.
/// `name` names a mark, `moved` sends one back at another point, `initial` makes a value the
/// initial one (or `None` for a kind without one). O(marks).
fn carry_marks<I: Copy, T: Clone>(
    cut: &Cut,
    at_start: bool,
    marks: &[Mark<I, T>],
    name: impl Fn(&Mark<I, T>) -> ObjectRef,
    moved: impl Fn(Mark<I, T>) -> Edit,
    initial: impl Fn(T) -> Option<Edit>,
    edits: &mut Vec<Edit>,
) {
    let inside: Vec<&Mark<I, T>> = marks.iter().filter(|m| cut.has(m.bar())).collect();
    // The first bar after the run has its own mark at its start.
    let own = cut.next.and_then(|next| {
        marks
            .iter()
            .find(|m| m.bar() == next && m.offset().is_zero())
    });
    let Some((last, others)) = inside.split_last() else {
        // Nothing inside; at the start of the score the next bar's own mark becomes initial.
        if at_start
            && let Some(own) = own
            && let Some(edit) = initial(own.value().clone())
        {
            edits.push(edit);
            edits.push(Edit::Remove(name(own)));
        }
        return;
    };
    for m in others {
        edits.push(Edit::Remove(name(m)));
    }
    let Some(next) = cut.next else {
        edits.push(Edit::Remove(name(last)));
        return;
    };
    match (own, at_start) {
        // The next bar's own mark wins; at the start it becomes the initial value.
        (Some(own), true) => {
            edits.push(Edit::Remove(name(last)));
            if let Some(edit) = initial(own.value().clone()) {
                edits.push(edit);
                edits.push(Edit::Remove(name(own)));
            }
        }
        (Some(_), false) => edits.push(Edit::Remove(name(last))),
        // The last mark of the run becomes the initial value…
        (None, true) => match initial(last.value().clone()) {
            Some(edit) => {
                edits.push(edit);
                edits.push(Edit::Remove(name(last)));
            }
            None => edits.push(moved(last.moved_to(ScorePoint::start_of(next)))),
        },
        // …or moves to the start of the next bar.
        (None, false) => edits.push(moved(last.moved_to(ScorePoint::start_of(next)))),
    }
}

/// Step 5 of [`delete_bars`] for the tempo marks (ADR-0026 U10, ADR-0026 W4).
///
/// - A gradual change that starts before the run and ends inside it is clipped: its end moves to
///   the cut point.
/// - Of the marks starting inside the run, only the last one that changes the tempo survives,
///   at the start of the first bar after the run (unless that bar has its own mark there):
///   - a gradual change that goes on past the run keeps its end and starts there (clipped);
///   - a gradual change that ends inside the run arrives as an **immediate** mark of its target
///     tempo, without text (ADR-0026 W4): the remaining music keeps the tempo it had;
///   - an immediate mark moves as it is.
///
///   At the start of the score it becomes the initial tempo mark instead (which must be
///   immediate). A gradual change going on past the run cannot start the score (ADR-0021: the
///   first tempo is immediate): the score then starts at the tempo the change **arrives at**, and
///   the change is deleted (owner's choice, 2026-11-04). Only the rest of the change plays at
///   the wrong tempo (too fast for an accelerando, too slow for a rallentando); from its end on
///   the music plays as before, as it does when a change lies wholly inside the run. Starting at
///   the tempo the change started from would instead leave everything after it at that tempo,
///   up to the next tempo mark.
/// - Every other mark starting inside the run (text-only marks included) is deleted.
///
/// # Complexity
/// O(t) for the t tempo marks.
#[allow(
    clippy::too_many_lines,
    reason = "one rule per case of W4, each short; splitting them would scatter the rule"
)]
fn carry_tempo(tx: &mut Transaction<'_>, cut: &Cut) -> Result<(), CommandError> {
    let doc = tx.document();
    let bars = doc.bars();
    let marks = doc.tempo_marks().as_slice();
    let mut edits = Vec::new();
    // The initial mark: its text stays when a carried tempo replaces it.
    let initial = doc.initial_tempo().1;
    // The last mark starting inside the run that changes the tempo, so far.
    let mut carried: Option<&Mark<crate::id::TempoMarkId, TempoMark>> = None;
    for mark in marks {
        if !cut.has(mark.bar()) {
            // Starts outside: only a gradual change ending inside the run is touched (clipped).
            if let Some(TempoMarkChange::Gradual(g)) = mark.value().change()
                && cut.has(g.end.bar)
            {
                edits.push(Edit::Replace(Object::TempoMark(Mark::new(
                    mark.id(),
                    mark.bar(),
                    mark.offset(),
                    mark.value().with_gradual_end(cut.point)?,
                ))));
            }
            continue;
        }
        // Starts inside: deleted, unless it is the last one that changes the tempo.
        if mark.value().change().is_some() {
            if let Some(previous) = carried.replace(mark) {
                edits.push(Edit::Remove(ObjectRef::TempoMark(previous.id())));
            }
        } else {
            edits.push(Edit::Remove(ObjectRef::TempoMark(mark.id())));
        }
    }
    let at_start = cut.first == 0;
    let own = cut.next.and_then(|next| {
        marks
            .iter()
            .find(|m| m.bar() == next && m.offset().is_zero())
    });
    if let Some(last) = carried {
        let remove = Edit::Remove(ObjectRef::TempoMark(last.id()));
        match (cut.next, last.value().change()) {
            // No bar after the run: nothing to carry.
            (None, _) => edits.push(remove),
            // A gradual change going on past the run.
            (Some(next), Some(TempoMarkChange::Gradual(g)))
                if point_key(bars, g.end) > point_key(bars, ScorePoint::start_of(next)) =>
            {
                if at_start {
                    // It cannot start the score: the score starts at the tempo it arrives at.
                    edits.push(Edit::SetInitial(Initial::Tempo(TempoMark::immediate(
                        initial.text(),
                        Metronome::shown(g.to),
                    )?)));
                    edits.push(remove);
                } else {
                    edits.push(Edit::Replace(Object::TempoMark(
                        last.moved_to(ScorePoint::start_of(next)),
                    )));
                }
            }
            // The next bar has its own mark at its start: it wins.
            (Some(_), _) if own.is_some() => edits.push(remove),
            // An immediate mark, or a gradual change ending inside the run (ADR-0026 W4).
            (Some(next), change) => {
                let value = match change {
                    Some(TempoMarkChange::Gradual(g)) => {
                        TempoMark::immediate(None, Metronome::shown(g.to))?
                    }
                    _ => last.value().clone(),
                };
                if at_start {
                    edits.push(Edit::SetInitial(Initial::Tempo(value)));
                    edits.push(remove);
                } else {
                    edits.push(Edit::Replace(Object::TempoMark(Mark::new(
                        last.id(),
                        next,
                        Duration::ZERO,
                        value,
                    ))));
                }
            }
        }
    }
    // At the start of the score, the next bar's own mark becomes the initial tempo mark.
    if at_start && let Some(own) = own {
        edits.push(Edit::SetInitial(Initial::Tempo(own.value().clone())));
        edits.push(Edit::Remove(ObjectRef::TempoMark(own.id())));
    }
    for edit in edits {
        tx.apply(edit)?;
    }
    Ok(())
}

/// Step 7 of [`delete_bars`], the cascade (ADR-0026 W5): runs the form checks of `validate` (rules
/// 8–11) and deletes the marking each one reports, until the form is valid. Every marking deleted
/// is in the change set, so the interface can say what went.
///
/// # Complexity
/// O(f) per check for the f form markings, once per marking deleted plus one; bounded by the
/// number of form markings, since each round deletes one.
///
/// # Errors
/// A violation that names no form marking (never expected after the steps before), or the
/// edit's own error.
pub(crate) fn cascade(tx: &mut Transaction<'_>) -> Result<(), CommandError> {
    loop {
        let doc = tx.document();
        let result = doc
            .check_form()
            .and_then(|()| doc.check_jumps())
            .and_then(|()| {
                doc.instruments()
                    .try_for_each(|i| doc.check_bar_repeats_and_pass_rules(i))
            });
        let violation = match result {
            Ok(()) => return Ok(()),
            Err(DocumentError::Invalid(v)) => v,
            Err(e) => return Err(e.into()),
        };
        let name = match violation {
            Violation::RepeatsOverlap(id) => {
                if doc.form().repeat_starts().iter().any(|r| r.id() == id) {
                    ObjectRef::RepeatStart(id)
                } else {
                    ObjectRef::RepeatEnd(id)
                }
            }
            Violation::RepeatTimesMismatch(id) => ObjectRef::RepeatEnd(id),
            Violation::VoltaOverlap(id)
            | Violation::VoltaOrder(id)
            | Violation::VoltaPassesIncomplete(id)
            | Violation::VoltaWithoutRepeat(id) => ObjectRef::Volta(id),
            Violation::JumpTargetKind(id)
            | Violation::JumpUntilOutOfRange(id)
            | Violation::JumpPassOutOfRange(id) => ObjectRef::Jump(id),
            Violation::BarRepeatNoSource(id)
            | Violation::BarRepeatOverlap(id)
            | Violation::BarRepeatNotEmpty(id)
            | Violation::BarRepeatAcrossForm(id) => ObjectRef::BarRepeat(id),
            Violation::PassRuleOutsideRepeat(id)
            | Violation::PassRulePasses(id)
            | Violation::PassRuleOverlap(id) => ObjectRef::PassRule(id),
            other => return Err(DocumentError::Invalid(other).into()),
        };
        tx.apply(Edit::Remove(name))?;
    }
}
