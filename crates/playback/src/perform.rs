// SPDX-License-Identifier: GPL-3.0-or-later

//! The played notes: the written intent along the path, turned into notes that sound (ADR-0027
//! §1–§8; SCALETTA F1.13b, N1–N10).
//!
//! # What it produces
//! A [`Performance`]: for every instrument, its [`PlayedNote`]s sorted by start, then by pitch. A
//! played note starts and ends on the **unrolled timeline**, in exact musical time (ADR-0027 §1:
//! the seconds come from the path's `time_at`, so a tempo edit recomputes nothing here), sounds
//! at an **exact pitch** in semitones (`Pitch::sounding`, C4 = 60, microtones kept), and has a
//! velocity on the **16-bit scale** of MIDI 2.0. It links back to the score: the first notehead of
//! its tie chain, its event, its voice and staff, the played bar where it starts and the pass of
//! its notes.
//!
//! # The steps, voice by voice (ADR-0027 N10)
//! 1. **The items**: the events the voice plays, bar by bar along the path. On a staff with bar
//!    repeats the events come from the source bar the path names, with that source's pass
//!    (ADR-0025 P3); events the pass rules silence are left out (ADR-0025 P4, ADR-0027 N1).
//! 2. **The chains** (ADR-0027 §2, N1, N2): every notehead is followed along its tie only if the
//!    notehead it is tied to sounds right after it on the path (it starts where this one ends,
//!    in the same or the next played bar); a chain is one played note.
//! 3. **Articulations** (§3, ADR-0027 N3, ADR-0027 N4): the shortening ones on the **last**
//!    notehead of the chain, the accents on the **first**; tenuto with staccato is a portato.
//! 4. **Grace notes** (§5, ADR-0027 N5, ADR-0027 N6): appoggiaturas on the beat, taking time from
//!    their main note; acciaccaturas **before** the beat, taking time from the previous note of the
//!    voice (the main note starts when it is written); `After` graces at the end of the note they
//!    follow.
//! 5. **Arpeggios** (§6, ADR-0027 N7): the notes of the chord start one after the other, 1/64
//!    apart.
//! 6. **Offsets and bounds** (§8, ADR-0027 N8): the performance offsets of the chain are added,
//!    with the **pass-specific** offsets of the pass being played (ADR-0028 §5, T3: the onset and
//!    the velocity of the first notehead in its pass, the lengths of every notehead in its own
//!    pass), then a note lasts at least 1/1024 of a whole note.
//! 7. **Loudness** (§4, §14, ADR-0027 H1–H17; module `loudness`): the level in force at
//!    the note's written point along the path (dynamics and hairpins), the dynamic accent written
//!    exactly there, the articulation accents of its first notehead and its velocity offset give
//!    its **attack profile**: the attack velocity, how long it is held, and how long the sound
//!    takes to settle to which level (seconds, so that the speed is fixed and a tempo edit
//!    recomputes nothing).
//!
//! # The incremental update
//! [`Performance::update`] (module `perform::update`, ADR-0027 H11 and ADR-0027 H12) recomputes,
//! after a transaction, only the velocities of an instrument whose dynamics changed and a window of
//! played bars around every changed cell, with the same code as the full computation
//! (`perform_range` on a range of played bars).
//!
//! # Complexity
//! O(path × events of a bar) items per voice, O(1) average per tie followed (an index by notehead),
//! O(n log n) to sort the n notes of an instrument.

use orchestra_core_time::invariant::convert_or;
use std::error::Error;
use std::fmt;

use orchestra_core_model::{
    Arpeggio, Articulation, ArticulationSet, Chord, Document, DocumentError, Event, EventBody,
    EventId, GraceAttach, GraceKind, IdMap, IdSet, InstrumentId, NoteId, PassOffsets,
    PerformanceOffsets, PitchError, StaffId, Tie, VoiceId,
};
use orchestra_core_time::{Duration, Position, Rational, RationalError, Seconds};

use crate::loudness::{self, PathPoint, Timeline};
use crate::unroll::{UnrollError, Unrolled};

mod update;

// ------------------------------------------------------------------------------------------------
// Constants of ADR-0027
// ------------------------------------------------------------------------------------------------

/// The duration of one acciaccatura and of one `After` grace note: 1/32 of a whole note, about
/// 65 ms at ♩ = 120 (ADR-0027 §5, `MuseScore`'s duration).
const GRACE_LENGTH: Duration = Duration::from_rational(orchestra_core_time::rational!(1, 32));

/// The distance between two notes of an arpeggio: 1/64 of a whole note (ADR-0027 §6).
const ARPEGGIO_STEP: Duration = Duration::from_rational(orchestra_core_time::rational!(1, 64));

/// The shortest a played note lasts: 1/1024 of a whole note (ADR-0027 §8).
const MIN_LENGTH: Duration = Duration::from_rational(orchestra_core_time::rational!(1, 1024));

// ------------------------------------------------------------------------------------------------
// Errors
// ------------------------------------------------------------------------------------------------

/// Why the performance could not be computed. On a valid document only the arithmetic can fail;
/// the other variants report a bug of the model rather than a panic.
///
/// The `Display` texts are English and meant for developers (CLAUDE.md §7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PerformError {
    /// The path could not be read.
    Unroll(UnrollError),
    /// The document answered with an error.
    Document(DocumentError),
    /// A pitch could not be turned into its sounding pitch.
    Pitch(PitchError),
    /// The exact arithmetic overflowed.
    Arithmetic(RationalError),
}

impl fmt::Display for PerformError {
    /// Writes a short English description of the error, for developers.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unroll(e) => write!(f, "path: {e}"),
            Self::Document(e) => write!(f, "document: {e}"),
            Self::Pitch(e) => write!(f, "pitch: {e}"),
            Self::Arithmetic(e) => write!(f, "arithmetic: {e}"),
        }
    }
}

impl Error for PerformError {
    /// The underlying error.
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Unroll(e) => Some(e),
            Self::Document(e) => Some(e),
            Self::Pitch(e) => Some(e),
            Self::Arithmetic(e) => Some(e),
        }
    }
}

impl From<UnrollError> for PerformError {
    /// Wraps a path error, so that `?` works.
    fn from(e: UnrollError) -> Self {
        Self::Unroll(e)
    }
}

impl From<DocumentError> for PerformError {
    /// Wraps a document error, so that `?` works.
    fn from(e: DocumentError) -> Self {
        Self::Document(e)
    }
}

impl From<PitchError> for PerformError {
    /// Wraps a pitch error, so that `?` works.
    fn from(e: PitchError) -> Self {
        Self::Pitch(e)
    }
}

impl From<RationalError> for PerformError {
    /// Wraps an arithmetic error, so that `?` works.
    fn from(e: RationalError) -> Self {
        Self::Arithmetic(e)
    }
}

impl From<orchestra_core_time::MeterMapError> for PerformError {
    /// Wraps a meter map error (through the path's error), so that `?` works.
    fn from(e: orchestra_core_time::MeterMapError) -> Self {
        Self::Unroll(UnrollError::MeterMap(e))
    }
}

// ------------------------------------------------------------------------------------------------
// The output
// ------------------------------------------------------------------------------------------------

/// A note that sounds (ADR-0027 §1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayedNote {
    /// The first notehead of its tie chain: the link back to the score.
    pub note: NoteId,
    /// The event of that notehead.
    pub event: EventId,
    /// The instrument.
    pub instrument: InstrumentId,
    /// The voice.
    pub voice: VoiceId,
    /// The staff the first notehead is drawn on.
    pub staff: StaffId,
    /// The played bar (index on the path) where the chain starts.
    pub played: u32,
    /// The pass of the first notehead's notes (F1.12; the source's pass on a staff with bar
    /// repeats).
    pub pass: u32,
    /// Where it starts on the unrolled timeline, in whole notes (negative only for acciaccaturas
    /// before the very first note, ADR-0027 §5).
    pub start: Position,
    /// Where it ends (excluded).
    pub end: Position,
    /// The sounding pitch in semitones, C4 = 60, exact.
    pub pitch: Rational,
    /// The velocity of the attack on the 16-bit scale, 1..=65 535 (ADR-0027 §14, H14).
    pub velocity: u16,
    /// How long the sound stays at the attack's level, in seconds (ADR-0027 H14, ADR-0027 H16: *fp*
    /// and *pf*); zero for most notes.
    pub hold: Seconds,
    /// How long the sound then takes to go, linearly, to `settle_to`, in seconds
    /// (ADR-0027 H14–H16): the fixed speed at which an accent fades back to the dynamic;
    /// zero without an accent. A note that ends earlier simply ends.
    pub settle_in: Seconds,
    /// The level the sound settles to and keeps, on the 16-bit scale, 1..=65 535: the dynamic in
    /// force for an accent, the second level of *fp* and the like; equal to `velocity` (with
    /// `hold` and `settle_in` zero) for a note without an accent.
    pub settle_to: u16,
}

/// The played notes of a document along its path (see the module documentation), with what the
/// incremental update needs ([`update`](Self::update)).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Performance {
    /// One part per instrument, in score order.
    parts: Vec<Part>,
}

/// The played notes of one instrument and what is kept to update them (internal).
///
/// # Why the extras are kept
/// A change of the dynamics recomputes only the velocities (ADR-0027 H11), so every note keeps the
/// inputs of its loudness ([`Basis`]); a change of a cell recomputes a window of played bars
/// (ADR-0027 H12), so every note keeps how far along the path it **reaches** (the last played bar
/// it sounds in or is tied into), which says which bars depend on it. Both are derived data like
/// the notes: thrown away and rebuilt with them.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Part {
    /// The instrument.
    id: InstrumentId,
    /// Its notes, in the order of [`order_key`]: by start, then pitch, then voice, notehead and
    /// played bar, a total order, so that an update and a full computation give the very same list.
    notes: Vec<PlayedNote>,
    /// For each note (same index), its extras.
    extras: Vec<Extra>,
    /// The loudness of each voice of the instrument along the path, by voice index
    /// (ADR-0027 H1–H9).
    timelines: Vec<Timeline>,
}

/// What a part keeps of each note besides the note itself (see [`Part`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Extra {
    /// The last played bar the note reaches: the bar of the last notehead of its tie chain, or
    /// later if it sounds past that bar's end (an overfull bar, a length offset).
    reach: u32,
    /// The inputs of its loudness.
    basis: Basis,
}

/// The inputs of the loudness of one note (ADR-0027 H2, ADR-0027 H10, ADR-0027 H17): where it is
/// read, and what changes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Basis {
    /// The voice, by index in the instrument.
    voice: usize,
    /// The path point of its first notehead (grace notes: their own item's).
    point: PathPoint,
    /// The written position of that point (hairpins interpolate in it).
    written: Position,
    /// Whether a dynamic accent written at the point applies (main notes) or not (grace notes,
    /// ADR-0027 H10).
    accent: bool,
    /// The articulations of its first notehead's chord.
    articulations: ArticulationSet,
    /// The velocity offset of its first notehead (ADR-0024 §10).
    offset: i32,
}

impl Basis {
    /// The attack profile of a note with this basis, read on the voice's timeline
    /// (ADR-0027 H14–H17). O(log n).
    ///
    /// # Errors
    /// The arithmetic errors; [`UnrollError::Endless`] for an unknown voice index (a bug).
    fn profile(&self, timelines: &[Timeline]) -> Result<loudness::Profile, PerformError> {
        let timeline = timelines.get(self.voice).ok_or(UnrollError::Endless)?;
        let accent = if self.accent {
            timeline.accent_at(self.point)
        } else {
            None
        };
        Ok(loudness::profile(
            timeline.level_at(self.point, self.written)?,
            accent,
            self.articulations,
            self.offset,
        )?)
    }
}

/// The total order of the notes of a part: by start, pitch, voice, notehead and played bar. A
/// notehead is played at most once per played bar, so no two notes share a key. O(1).
fn order_key(note: &PlayedNote) -> (Position, Rational, VoiceId, NoteId, u32) {
    (note.start, note.pitch, note.voice, note.note, note.played)
}

impl Part {
    /// The part of `instrument`, computed whole. O(see the module documentation).
    ///
    /// # Errors
    /// As [`Performance::new`].
    fn new(
        doc: &Document,
        path: &Unrolled,
        instrument: &orchestra_core_model::Instrument,
    ) -> Result<Self, PerformError> {
        let timelines = Timeline::all(doc, path, instrument)?;
        let mut slots = match last_played(path) {
            Some(last) => perform_range(doc, path, instrument, &timelines, 0, last)?,
            None => Vec::new(),
        };
        slots.sort_unstable_by_key(|a| order_key(&a.0));
        let (notes, extras) = slots.into_iter().unzip();
        Ok(Self {
            id: instrument.id(),
            notes,
            extras,
            timelines,
        })
    }
}

/// The played bar a note ending (excluded) at `end` last sounds in: the last played bar starting
/// before `end`, 0 if none. O(log path).
fn bar_of(path: &Unrolled, end: Position) -> u32 {
    let i = path.played().partition_point(|p| p.start < end);
    // A path has at most `u32::MAX` played bars (the unroller's bound), so this always fits.
    convert_or(i.saturating_sub(1), u32::MAX)
}

/// The index of the last played bar, or `None` for an empty path. O(1).
fn last_played(path: &Unrolled) -> Option<u32> {
    path.played()
        .len()
        .checked_sub(1)
        .and_then(|l| u32::try_from(l).ok())
}

impl Performance {
    /// The played notes of `doc` along `path` (see the module documentation).
    ///
    /// # Errors
    /// [`PerformError`]: arithmetic only on a valid document.
    pub fn new(doc: &Document, path: &Unrolled) -> Result<Self, PerformError> {
        let mut parts = Vec::new();
        for track in doc.tracks() {
            parts.push(Part::new(doc, path, track.instrument())?);
        }
        Ok(Self { parts })
    }

    /// The played notes of an instrument, sorted by start, then by pitch (then by voice, notehead
    /// and played bar, so that the order is total); empty for an unknown instrument.
    /// O(instruments).
    #[must_use]
    pub fn notes(&self, instrument: InstrumentId) -> &[PlayedNote] {
        self.parts
            .iter()
            .find(|p| p.id == instrument)
            .map_or(&[], |p| p.notes.as_slice())
    }

    /// Every instrument with its notes, in score order. O(1) per instrument.
    pub fn instruments(&self) -> impl Iterator<Item = (InstrumentId, &[PlayedNote])> {
        self.parts.iter().map(|p| (p.id, p.notes.as_slice()))
    }
}

// ------------------------------------------------------------------------------------------------
// Items: the events a voice plays along the path
// ------------------------------------------------------------------------------------------------

/// One event played by a voice at one place of the path.
///
/// It borrows the event from the document (`'a`): an event is played once per pass of its bar, and
/// copying it each time would cost an allocation for every chord of two or more noteheads.
#[derive(Debug, Clone, Copy)]
struct Item<'a> {
    /// The played bar.
    played: u32,
    /// The pass of its notes.
    pass: u32,
    /// Its offset inside the written bar of the played bar: with `played`, the path point at which
    /// its loudness is read (ADR-0027 H2).
    offset: Duration,
    /// Its written position: the start of the played bar's written bar plus `offset` (ADR-0027 H2,
    /// ADR-0027 H8: the coordinate hairpins are interpolated in).
    written: Position,
    /// Where it starts on the unrolled timeline.
    start: Position,
    /// Its metric duration (zero for a grace note).
    length: Duration,
    /// The event, in the document.
    event: &'a Event,
}

impl Item<'_> {
    /// Where it ends on the unrolled timeline, as written. O(1).
    fn end(&self) -> Result<Position, RationalError> {
        self.start.checked_add(self.length)
    }
}

/// The items of a voice, in playing order: for each played bar from `from` to `to` (included), the
/// events of the voice there (or, on a staff with bar repeats, in its source bar), in chunk order,
/// those silenced by the pass rules left out. O(bars × staves × events of a bar).
///
/// # Errors
/// The path's and the document's errors (a bug of the model).
fn items_of<'a>(
    doc: &Document,
    path: &Unrolled,
    voice: &'a orchestra_core_model::Voice,
    staves: &[StaffId],
    (from, to): (u32, u32),
) -> Result<Vec<Item<'a>>, PerformError> {
    let bars = doc.bars();
    let meter = doc.meter_map();
    let mut items = Vec::new();
    let (from, to) = (
        usize::try_from(from).unwrap_or(usize::MAX),
        usize::try_from(to).unwrap_or(usize::MAX),
    );
    let played_bars = path.played().get(from..=to).unwrap_or(&[]);
    for (k, played) in (from..).zip(played_bars) {
        // Where the played bar's own written bar starts: the loudness of every event played here
        // is read at this bar's points, also on a staff whose notes come from a bar repeat's
        // source (ADR-0027 H2: the dynamics written under the % bar count).
        let bar_start = played.written_start;
        // The written bars the voice's events come from in this played bar, one per staff.
        let mut sources: Vec<(StaffId, orchestra_core_model::BarId, u32)> = Vec::new();
        for &staff in staves {
            let content = path.content(k, staff).ok_or(UnrollError::Endless)?;
            sources.push((staff, content.bar, content.pass));
        }
        let mut here: Vec<(Duration, u8, usize, Item<'a>)> = Vec::new();
        let mut order = 0_usize;
        // Each distinct source bar once, keeping only its events on the staves it is the source
        // for (the bar itself for most staves, a source bar for a staff with a bar repeat).
        let mut seen: Vec<orchestra_core_model::BarId> = Vec::new();
        for &(_, bar, _) in &sources {
            if seen.contains(&bar) {
                continue;
            }
            seen.push(bar);
            let index = bars.index_of(bar).ok_or(DocumentError::UnknownBar(bar))?;
            let length = meter.bar_length(index)?;
            for placed in voice.events_at(index) {
                let event = placed.event();
                let Some(&(_, _, pass)) = sources
                    .iter()
                    .find(|(s, b, _)| *s == event.staff() && *b == bar)
                else {
                    continue;
                };
                let offset = placed.offset();
                if !path.plays(doc, voice.id(), k, offset)? {
                    continue;
                }
                let metric = voice.metric_duration(event, length)?;
                let start = played.start.checked_add(offset)?;
                let slot = u8::from(event.grace().is_none());
                here.push((
                    offset,
                    slot,
                    order,
                    Item {
                        played: u32::try_from(k).map_err(|_| UnrollError::Endless)?,
                        pass,
                        offset,
                        written: bar_start.checked_add(offset)?,
                        start,
                        length: if event.grace().is_some() {
                            Duration::ZERO
                        } else {
                            metric
                        },
                        event,
                    },
                ));
                order = order.saturating_add(1);
            }
        }
        // Chunk order: by offset, graces before main events at one offset (`Before` graces), then
        // as written. `After` graces sit at the end of their main event, which sorts them right.
        here.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
        items.extend(here.into_iter().map(|(_, _, _, item)| item));
    }
    Ok(items)
}

// ------------------------------------------------------------------------------------------------
// One instrument
// ------------------------------------------------------------------------------------------------

/// A chain being built: the items (by index) and noteheads it goes through.
#[derive(Debug, Clone)]
struct Chain {
    /// (voice index, item index, notehead) of every notehead of the chain, first to last.
    links: Vec<(usize, usize, NoteId)>,
}

/// A played note while it is being made, with what its part will keep of it.
#[derive(Debug, Clone, Copy)]
struct Slot {
    /// The note.
    note: PlayedNote,
    /// The played bar of its last notehead (its own for a grace note).
    last: u32,
    /// The inputs of its loudness.
    basis: Basis,
}

/// The played notes of one instrument made from the played bars `from` to `to` (included), with
/// their extras, unsorted. On the whole path this is the instrument's performance; on a window it
/// is what the incremental update keeps the middle of (see `update`), because every rule looks at
/// the items it is given only.
///
/// # Errors
/// As [`Performance::new`].
fn perform_range(
    doc: &Document,
    path: &Unrolled,
    instrument: &orchestra_core_model::Instrument,
    timelines: &[Timeline],
    from: u32,
    to: u32,
) -> Result<Vec<(PlayedNote, Extra)>, PerformError> {
    let staves: Vec<StaffId> = instrument
        .staves()
        .iter()
        .map(orchestra_core_model::Staff::id)
        .collect();
    let voices = instrument.voices();
    let mut items: Vec<Vec<Item<'_>>> = Vec::with_capacity(voices.len());
    for voice in voices {
        items.push(items_of(doc, path, voice, &staves, (from, to))?);
    }

    let chains = find_chains(&items)?;

    // The notes of the chains, before the grace notes move their starts and ends.
    let mut notes: Vec<(Chain, Slot)> = Vec::new();
    for chain in chains {
        let note = chain_note(
            &items,
            timelines,
            instrument.id(),
            voices,
            instrument.pass_offsets(),
            &chain,
        )?;
        notes.push((chain, note));
    }
    // The notes of the grace chords, and the time they take from their neighbours (§5, ADR-0027 N5,
    // ADR-0027 N6).
    let mut graces: Vec<Slot> = Vec::new();
    for (v, list) in items.iter().enumerate() {
        place_graces(
            list,
            v,
            timelines,
            (instrument.id(), voices, instrument.pass_offsets()),
            &mut notes,
            &mut graces,
        )?;
    }
    // Arpeggios (ADR-0027 N7), then the bounds (§8), then how far each note reaches along the path.
    let mut all: Vec<Slot> = notes.into_iter().map(|(_, n)| n).collect();
    all.extend(graces);
    arpeggiate(&items, &mut all)?;
    let mut out = Vec::with_capacity(all.len());
    for mut slot in all {
        bound(&mut slot.note)?;
        let reach = slot.last.max(bar_of(path, slot.note.end));
        out.push((
            slot.note,
            Extra {
                reach,
                basis: slot.basis,
            },
        ));
    }
    Ok(out)
}

/// The tie chains of the items of an instrument (ADR-0027 §2, N1, N2): every notehead of a main
/// event starts a chain unless a chain already went through it, and a chain follows a tie only to
/// a notehead that sounds right after it on the path (it starts where this one ends, in the same or
/// the next played bar). O(noteheads), with an index by notehead.
///
/// # Errors
/// [`UnrollError::Endless`] for a link to nothing (a bug of the model).
fn find_chains(items: &[Vec<Item<'_>>]) -> Result<Vec<Chain>, PerformError> {
    // Every notehead of a main event, by ID: where it is played (voice, item), for the ties. A
    // notehead played several times (a repeat) has several places.
    let mut places: IdMap<NoteId, Vec<(usize, usize)>> = IdMap::default();
    for (v, list) in items.iter().enumerate() {
        for (i, item) in list.iter().enumerate() {
            if item.event.grace().is_some() {
                continue;
            }
            for note in item.event.chord().into_iter().flat_map(Chord::notes) {
                places.entry(note.id()).or_default().push((v, i));
            }
        }
    }

    // ADR-0027 N1, ADR-0027 N2: chains, followed notehead by notehead along the ties that sound
    // next on the path.
    let mut continued: IdSet<(usize, usize, NoteId)> = IdSet::default();
    let mut chains: Vec<Chain> = Vec::new();
    for (v, list) in items.iter().enumerate() {
        for (i, item) in list.iter().enumerate() {
            let Some(chord) = item.event.chord() else {
                continue;
            };
            if item.event.grace().is_some() {
                continue;
            }
            for note in chord.notes() {
                if continued.contains(&(v, i, note.id())) {
                    continue;
                }
                let mut links = vec![(v, i, note.id())];
                let (mut cv, mut ci, mut cnote) = (v, i, *note);
                while let Tie::To(target) = cnote.tie() {
                    let current = items
                        .get(cv)
                        .and_then(|l| l.get(ci))
                        .ok_or(UnrollError::Endless)?;
                    let end = current.end()?;
                    let next = places.get(&target).and_then(|list| {
                        list.iter().copied().find(|&(tv, ti)| {
                            items.get(tv).and_then(|l| l.get(ti)).is_some_and(|t| {
                                t.start == end
                                    && (t.played == current.played
                                        || t.played == current.played.saturating_add(1))
                            })
                        })
                    });
                    let Some((tv, ti)) = next else {
                        break;
                    };
                    let target_note = items
                        .get(tv)
                        .and_then(|l| l.get(ti))
                        .and_then(|t| t.event.chord())
                        .and_then(|c| c.notes().find(|n| n.id() == target))
                        .copied()
                        .ok_or(UnrollError::Endless)?;
                    if !continued.insert((tv, ti, target)) {
                        break;
                    }
                    links.push((tv, ti, target));
                    (cv, ci, cnote) = (tv, ti, target_note);
                }
                chains.push(Chain { links });
            }
        }
    }

    Ok(chains)
}

/// The shortening of the articulations of the last notehead of a chain, as a factor of its written
/// value (ADR-0027 §3, N4): tenuto with staccato is a portato (3/4); tenuto alone is never
/// shortened; otherwise staccato 1/2, staccatissimo and spiccato 1/4, portato and marcato 3/4,
/// multiplied together. O(articulations).
fn length_factor(set: ArticulationSet) -> Result<Rational, RationalError> {
    if set.contains(Articulation::Tenuto) {
        return if set.contains(Articulation::Staccato) {
            Rational::new(3, 4)
        } else {
            Ok(Rational::ONE)
        };
    }
    let mut factor = Rational::ONE;
    for (articulation, n, d) in [
        (Articulation::Staccato, 1, 2),
        (Articulation::Staccatissimo, 1, 4),
        (Articulation::Spiccato, 1, 4),
        (Articulation::Portato, 3, 4),
        (Articulation::Marcato, 3, 4),
    ] {
        if set.contains(articulation) {
            factor = factor.checked_mul(Rational::new(n, d)?)?;
        }
    }
    Ok(factor)
}

/// The played note of a chain, before grace notes and arpeggios (ADR-0027 §2, §3, §8, §14, N3, N8,
/// ADR-0027 H2, ADR-0027 H17): its loudness is read at the written point of its first notehead,
/// with the accents of that notehead.
///
/// # Errors
/// [`PerformError::Unroll`] for a link to nothing (a bug); the arithmetic and pitch errors.
fn chain_note(
    items: &[Vec<Item<'_>>],
    timelines: &[Timeline],
    instrument: InstrumentId,
    voices: &[orchestra_core_model::Voice],
    passes: &PassOffsets,
    chain: &Chain,
) -> Result<Slot, PerformError> {
    let link = |k: usize| -> Result<(&Item<'_>, orchestra_core_model::Note), PerformError> {
        let &(v, i, id) = chain.links.get(k).ok_or(UnrollError::Endless)?;
        let item = items
            .get(v)
            .and_then(|l| l.get(i))
            .ok_or(UnrollError::Endless)?;
        let note = item
            .event
            .chord()
            .and_then(|c| c.notes().find(|n| n.id() == id))
            .copied()
            .ok_or(UnrollError::Endless)?;
        Ok((item, note))
    };
    let (first, first_note) = link(0)?;
    let (last, _) = link(chain.links.len().saturating_sub(1))?;
    let first_chord = first.event.chord().ok_or(UnrollError::Endless)?;
    let last_chord = last.event.chord().ok_or(UnrollError::Endless)?;
    // ADR-0027 N3: the shortening on the last notehead, of its own written length.
    let last_length = last
        .length
        .checked_mul(length_factor(last_chord.articulations())?)?;
    let mut end = last.start.checked_add(last_length)?;
    // ADR-0027 N8: the onset and the velocity from the first notehead, the lengths of every
    // notehead added; ADR-0028 T3: each notehead's pass-specific offsets of its own pass added to
    // its inline ones.
    let mut length_offset = Duration::ZERO;
    for k in 0..chain.links.len() {
        let (item, note) = link(k)?;
        length_offset = length_offset.checked_add(note.offsets().length)?;
        // Most tables are empty: skip the lookup and the (gcd-costing) addition of a zero then.
        if !passes.is_empty() {
            length_offset = length_offset.checked_add(passes.get(note.id(), item.pass).length)?;
        }
    }
    let offsets = with_pass(
        first_note.offsets(),
        passes.get(first_note.id(), first.pass),
    )?;
    let start = first.start.checked_add(offsets.onset)?;
    end = end.checked_add(offsets.onset)?.checked_add(length_offset)?;
    let &(v, _, _) = chain.links.first().ok_or(UnrollError::Endless)?;
    let voice = voices.get(v).ok_or(UnrollError::Endless)?.id();
    let basis = Basis {
        voice: v,
        point: (first.played, first.offset),
        written: first.written,
        accent: true,
        articulations: first_chord.articulations(),
        offset: offsets.velocity,
    };
    let loud = basis.profile(timelines)?;
    let note = played_note(&first_note, first, (instrument, voice), (start, end), loud)?;
    Ok(Slot {
        note,
        last: last.played,
        basis,
    })
}

/// A played note of the notehead `note` of the item `item`, sounding from `start` to `end` with
/// the attack profile `loud`. O(1).
///
/// The one place where a [`PlayedNote`] is assembled: tie chains (`chain_note`) and grace notes
/// (`push_grace`) both use it instead of filling in the fourteen fields themselves.
///
/// # Errors
/// The pitch error of an absurd alteration (see `Pitch::sounding`).
fn played_note(
    note: &orchestra_core_model::Note,
    item: &Item<'_>,
    (instrument, voice): (InstrumentId, VoiceId),
    (start, end): (Position, Position),
    loud: loudness::Profile,
) -> Result<PlayedNote, PerformError> {
    Ok(PlayedNote {
        note: note.id(),
        event: item.event.id(),
        instrument,
        voice,
        staff: item.event.staff(),
        played: item.played,
        pass: item.pass,
        start,
        end,
        pitch: note.pitch().sounding()?,
        velocity: loud.velocity,
        hold: loud.hold,
        settle_in: loud.settle_in,
        settle_to: loud.settle_to,
    })
}

/// The grace notes of one voice (ADR-0027 §5, N5, N6): their notes appended to `graces`, and the
/// time they take moved on the notes of `notes` (the main note's start for appoggiaturas, the
/// previous note's end for acciaccaturas and `After` graces).
///
/// # How
/// The items are walked in playing order. A run of `Before` graces right before a main event is
/// split into its acciaccaturas (before the beat) and its appoggiaturas (on the beat); the `After`
/// graces right after a main event belong to it. The previous note of the voice is the last chain
/// ending at or before the main event's start.
///
/// # Errors
/// The arithmetic and pitch errors.
#[allow(
    clippy::too_many_lines,
    reason = "the three kinds of grace runs (after, before the beat, on the beat), in reading order"
)]
fn place_graces(
    list: &[Item<'_>],
    v: usize,
    timelines: &[Timeline],
    (instrument, voices, passes): (InstrumentId, &[orchestra_core_model::Voice], &PassOffsets),
    notes: &mut [(Chain, Slot)],
    graces: &mut Vec<Slot>,
) -> Result<(), PerformError> {
    let voice = voices.get(v).ok_or(UnrollError::Endless)?.id();
    // What every grace note pushed below shares: its voice, instrument and pass offsets.
    let context = (v, instrument, voice, passes);
    let unit = GRACE_LENGTH;
    let mut i = 0;
    while i < list.len() {
        let Some(item) = list.get(i) else { break };
        let Some(grace) = item.event.grace() else {
            i = i.saturating_add(1);
            continue;
        };
        // The run of graces of the same attachment at the same point: items `i..j`.
        let j = grace_run_end(list, i, grace.attach);
        if grace.attach == GraceAttach::After {
            // A run of `After` graces: they end where the note they follow ends.
            let n = count(j.saturating_sub(i))?;
            let (at, limit) = previous_end(notes, v, item.start)?;
            let total = at_most_half_of(unit.checked_mul(n)?, limit)?;
            let each = total.checked_div(n)?;
            // ADR-0027 N6: acciaccaturas of the next note at the same point come after these
            // graces, so these end where those begin (each acciaccatura lasts one unit, before the
            // beat).
            let following = list
                .get(j..)
                .unwrap_or(&[])
                .iter()
                .take_while(|x| {
                    x.start == item.start
                        && x.event
                            .grace()
                            .is_some_and(|g| g.attach == GraceAttach::Before)
                })
                .filter(|x| is_acciaccatura(x))
                .count();
            let from = at
                .checked_sub(unit.checked_mul(count(following)?)?)?
                .checked_sub(total)?;
            shorten_previous(notes, v, item.start, from);
            spread(list, i..j, from, each, timelines, context, graces)?;
            i = j;
            continue;
        }
        // A run of `Before` graces, then their main event, split into acciaccaturas (before the
        // beat) and appoggiaturas (on it).
        let (accs, apps): (Vec<usize>, Vec<usize>) =
            (i..j).partition(|&g| list.get(g).is_some_and(is_acciaccatura));
        let beat = item.start;
        // Acciaccaturas before the beat, taking time from the previous note (ADR-0027 N5).
        if !accs.is_empty() {
            let n = count(accs.len())?;
            let (prev_end, limit) = previous_end(notes, v, beat)?;
            // Only a previous note ending right on the beat gives up time.
            let limit = limit.filter(|_| prev_end == beat);
            let total = at_most_half_of(unit.checked_mul(n)?, limit)?;
            let each = total.checked_div(n)?;
            let from = beat.checked_sub(total)?;
            shorten_previous(notes, v, beat, from);
            spread(list, accs, from, each, timelines, context, graces)?;
        }
        // Appoggiaturas on the beat, taking time from the main note.
        if !apps.is_empty()
            && let Some(main) = list.get(j)
            && main.event.grace().is_none()
            && main.start == beat
        {
            let dots = main.event.chord().map_or(0, |c| c.value().dots());
            let share = match dots {
                0 => Rational::new(1, 2)?,
                1 => Rational::new(2, 3)?,
                _ => Rational::new(4, 7)?,
            };
            let total = main.length.checked_mul(share)?;
            let each = total.checked_div(count(apps.len())?)?;
            spread(list, apps, beat, each, timelines, context, graces)?;
            // The main note (every chain starting at that item) starts after them.
            let new_start = beat.checked_add(total)?;
            for (chain, note) in notes.iter_mut() {
                if chain
                    .links
                    .first()
                    .is_some_and(|&(cv, ci, _)| cv == v && ci == j)
                {
                    note.note.start = new_start;
                }
            }
        }
        i = j;
    }
    Ok(())
}

/// The end (exclusive) of the run of grace notes attached by `attach` that starts at `i`: the
/// following items that are graces with the same attachment and the same start as item `i`. `i`
/// itself is assumed to be one of them, so the result is at least `i + 1` when `i` is in range.
/// O(run length).
///
/// Shared by both attachments in `place_graces`.
fn grace_run_end(list: &[Item<'_>], i: usize, attach: GraceAttach) -> usize {
    let Some(first) = list.get(i) else {
        return i;
    };
    let mut j = i.saturating_add(1);
    while list.get(j).is_some_and(|next| {
        next.event.grace().is_some_and(|g| g.attach == attach) && next.start == first.start
    }) {
        j = j.saturating_add(1);
    }
    j
}

/// Whether the item is an acciaccatura (a crushed grace note, ADR-0027 N5). O(1).
fn is_acciaccatura(item: &Item<'_>) -> bool {
    item.event
        .grace()
        .is_some_and(|g| g.kind == GraceKind::Acciaccatura)
}

/// A count of grace notes as a rational, for multiplying and dividing durations. Counts are tiny;
/// a count beyond `i64` cannot happen and is reported as an overflow rather than replaced by a
/// made-up value. O(1).
fn count(n: usize) -> Result<Rational, RationalError> {
    Rational::from_integer(i64::try_from(n).map_err(|_| RationalError::Overflow)?)
}

/// `total`, but no more than half of `limit` when there is one: grace notes never take more than
/// half of the note they borrow time from (ADR-0027 N5, N6). O(1).
fn at_most_half_of(total: Duration, limit: Option<Duration>) -> Result<Duration, RationalError> {
    Ok(match limit {
        Some(limit) => total.min(limit.checked_mul(Rational::new(1, 2)?)?),
        None => total,
    })
}

/// Plays the grace notes at the indices `run`, one after the other from `from`, each lasting
/// `each`: the `k`-th starts at `from + k × each`. O(run length × noteheads).
///
/// The same loop was written three times (`After` graces, acciaccaturas, appoggiaturas; review
/// point B17).
#[allow(
    clippy::too_many_arguments,
    reason = "the run, its timing and the context of `push_grace`, all needed; a struct would only \
              rename them"
)]
fn spread(
    list: &[Item<'_>],
    run: impl IntoIterator<Item = usize>,
    from: Position,
    each: Duration,
    timelines: &[Timeline],
    context: (usize, InstrumentId, VoiceId, &PassOffsets),
    graces: &mut Vec<Slot>,
) -> Result<(), PerformError> {
    let mut start = from;
    for g in run {
        push_grace(list, g, timelines, context, start, each, graces)?;
        start = start.checked_add(each)?;
    }
    Ok(())
}

/// The end of the previous note of voice `v` before `at` (the latest end at or before `at` among
/// its chains), with its length, or `(at, None)` when there is none. O(notes).
fn previous_end(
    notes: &[(Chain, Slot)],
    v: usize,
    at: Position,
) -> Result<(Position, Option<Duration>), PerformError> {
    let mut best: Option<&PlayedNote> = None;
    for (chain, slot) in notes {
        let note = &slot.note;
        if chain.links.first().is_none_or(|&(cv, _, _)| cv != v) || note.end > at {
            continue;
        }
        if best.is_none_or(|b| note.end > b.end) {
            best = Some(note);
        }
    }
    Ok(match best {
        Some(note) => (note.end, Some(note.end.checked_duration_since(note.start)?)),
        None => (at, None),
    })
}

/// Ends at `from` every note of voice `v` that ends after `from` and at or before `at` (the
/// previous notes a grace takes its time from). O(notes).
fn shorten_previous(notes: &mut [(Chain, Slot)], v: usize, at: Position, from: Position) {
    for (chain, slot) in notes.iter_mut() {
        let note = &mut slot.note;
        if chain.links.first().is_some_and(|&(cv, _, _)| cv == v)
            && note.end > from
            && note.end <= at
            && note.start < from
        {
            note.end = from;
        }
    }
}

/// The inline offsets of a notehead with the pass-specific offsets of the pass being played added
/// (ADR-0028 §5): onsets and lengths added exactly, velocities added with saturation (both are
/// unbounded deltas; the bounds of ADR-0027 §8 come later). O(1).
///
/// Without an entry (the common case: most notes have none, most tables are empty) the inline
/// offsets are returned as they are: adding a zero rational still costs a gcd, and doing it for
/// every note of the heavy score measured +7% on the whole performance.
///
/// # Errors
/// [`RationalError`] if an addition of durations overflows (absurd values only).
fn with_pass(
    inline: PerformanceOffsets,
    pass: PerformanceOffsets,
) -> Result<PerformanceOffsets, RationalError> {
    if pass.is_none() {
        return Ok(inline);
    }
    Ok(PerformanceOffsets {
        onset: inline.onset.checked_add(pass.onset)?,
        length: inline.length.checked_add(pass.length)?,
        velocity: inline.velocity.saturating_add(pass.velocity),
    })
}

/// Appends the notes of the grace chord of item `g`, from `start`, lasting `length`. Their loudness
/// is the level in force at their written point (their main note's), hairpins included, with
/// their own articulations but **without** the dynamic accent written there, which belongs to
/// the main note (ADR-0027 H10). O(noteheads).
fn push_grace(
    list: &[Item<'_>],
    g: usize,
    timelines: &[Timeline],
    (v, instrument, voice, passes): (usize, InstrumentId, VoiceId, &PassOffsets),
    start: Position,
    length: Duration,
    graces: &mut Vec<Slot>,
) -> Result<(), PerformError> {
    let item = list.get(g).ok_or(UnrollError::Endless)?;
    let Some(chord) = item.event.chord() else {
        return Ok(());
    };
    for note in chord.notes() {
        let offsets = with_pass(note.offsets(), passes.get(note.id(), item.pass))?;
        let basis = Basis {
            voice: v,
            point: (item.played, item.offset),
            written: item.written,
            accent: false,
            articulations: chord.articulations(),
            offset: offsets.velocity,
        };
        let loud = basis.profile(timelines)?;
        let played = played_note(
            note,
            item,
            (instrument, voice),
            (
                start.checked_add(offsets.onset)?,
                start
                    .checked_add(length)?
                    .checked_add(offsets.onset)?
                    .checked_add(offsets.length)?,
            ),
            loud,
        )?;
        graces.push(Slot {
            note: played,
            last: item.played,
            basis,
        });
    }
    Ok(())
}

/// Spreads the notes of every arpeggiated chord (ADR-0027 §6, N7): the notes starting at an event
/// with an arpeggio sign start one after the other, 1/64 apart, from the lowest (`Plain`, `Up`) or
/// the highest (`Down`); they all end as before. O(n log n) for the n notes.
///
/// # Errors
/// The arithmetic errors.
fn arpeggiate(items: &[Vec<Item<'_>>], notes: &mut [Slot]) -> Result<(), PerformError> {
    let step = ARPEGGIO_STEP;
    // The events with an arpeggio sign, and their direction.
    let mut signs: IdMap<EventId, bool> = IdMap::default();
    for list in items {
        for item in list {
            if let EventBody::Chord(chord) = item.event.body() {
                match chord.arpeggio() {
                    Some(Arpeggio::Plain | Arpeggio::Up) => {
                        signs.insert(item.event.id(), true);
                    }
                    Some(Arpeggio::Down) => {
                        signs.insert(item.event.id(), false);
                    }
                    _ => {}
                }
            }
        }
    }
    if signs.is_empty() {
        return Ok(());
    }
    // Group the notes of one arpeggiated event at one played start.
    let mut groups: IdMap<(EventId, u32), Vec<usize>> = IdMap::default();
    for (k, slot) in notes.iter().enumerate() {
        let note = &slot.note;
        if signs.contains_key(&note.event) {
            groups.entry((note.event, note.played)).or_default().push(k);
        }
    }
    for ((event, _), mut group) in groups {
        let up = signs.get(&event).copied().unwrap_or(true);
        group.sort_by(|&a, &b| {
            let (pa, pb) = (
                notes.get(a).map(|n| n.note.pitch),
                notes.get(b).map(|n| n.note.pitch),
            );
            if up { pa.cmp(&pb) } else { pb.cmp(&pa) }
        });
        for (k, index) in group.into_iter().enumerate() {
            if let Some(Slot { note, .. }) = notes.get_mut(index) {
                let shift = step.checked_mul(Rational::from_integer(convert_or(k, 0_i64))?)?;
                note.start = note.start.checked_add(shift)?;
            }
        }
    }
    Ok(())
}

/// The bounds of ADR-0027 §8: a note lasts at least 1/1024 of a whole note (the velocities are
/// already bounded where the profile is computed). O(1).
///
/// # Errors
/// The arithmetic errors.
fn bound(note: &mut PlayedNote) -> Result<(), PerformError> {
    let shortest = note.start.checked_add(MIN_LENGTH)?;
    if note.end < shortest {
        note.end = shortest;
    }
    Ok(())
}
