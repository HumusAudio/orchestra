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
//! # The steps, voice by voice (N10)
//! 1. **The items**: the events the voice plays, bar by bar along the path. On a staff with bar
//!    repeats the events come from the source bar the path names, with that source's pass (P3);
//!    events the pass rules silence are left out (P4, N1).
//! 2. **The chains** (ADR-0027 §2, N1, N2): every notehead is followed along its tie only if the
//!    notehead it is tied to sounds right after it on the path (it starts where this one ends,
//!    in the same or the next played bar); a chain is one played note.
//! 3. **Articulations** (§3, N3, N4): the shortening ones on the **last** notehead of the chain,
//!    the accents on the **first**; tenuto with staccato is a portato.
//! 4. **Grace notes** (§5, N5, N6): appoggiaturas on the beat, taking time from their main note;
//!    acciaccaturas **before** the beat, taking time from the previous note of the voice (the main
//!    note starts when it is written); `After` graces at the end of the note they follow.
//! 5. **Arpeggios** (§6, N7): the notes of the chord start one after the other, 1/64 apart.
//! 6. **Offsets and bounds** (§8, N8): the performance offsets of the chain are added, then a note
//!    lasts at least 1/1024 of a whole note and has a velocity in 1..=65 535.
//!
//! The velocity before articulations and offsets is *mf* for every note (N9): the dynamics come
//! with F1.13c, and so does the incremental update.
//!
//! # Complexity
//! O(path × events of a bar) items per voice, O(1) average per tie followed (an index by notehead),
//! O(n log n) to sort the n notes of an instrument.

use std::collections::HashMap;
use std::error::Error;
use std::fmt;

use orchestra_core_model::{
    Arpeggio, Articulation, ArticulationSet, Chord, Document, DocumentError, Event, EventBody,
    EventId, GraceAttach, GraceKind, InstrumentId, NoteId, PitchError, StaffId, Tie, VoiceId,
};
use orchestra_core_time::{Duration, Position, Rational, RationalError};

use crate::unroll::{UnrollError, Unrolled};

// ------------------------------------------------------------------------------------------------
// Constants of ADR-0027
// ------------------------------------------------------------------------------------------------

/// The provisional velocity of every note before articulations and offsets: *mf*, 75 on the MIDI
/// 1.0 scale (ADR-0027 §4), up-scaled to 16 bits with `v << 9 | v << 2 | v >> 5` (N9).
pub const PROVISIONAL_VELOCITY: u16 = (75 << 9) | (75 << 2) | (75 >> 5);

/// The highest velocity of the 16-bit scale.
const MAX_VELOCITY: i64 = 65_535;

/// The duration of one acciaccatura and of one `After` grace note: 1/32 of a whole note, about
/// 65 ms at ♩ = 120 (ADR-0027 §5, `MuseScore`'s duration).
fn grace_length() -> Result<Duration, RationalError> {
    Ok(Duration::from_rational(Rational::new(1, 32)?))
}

/// The distance between two notes of an arpeggio: 1/64 of a whole note (ADR-0027 §6).
fn arpeggio_step() -> Result<Duration, RationalError> {
    Ok(Duration::from_rational(Rational::new(1, 64)?))
}

/// The shortest a played note lasts: 1/1024 of a whole note (ADR-0027 §8).
fn min_length() -> Result<Duration, RationalError> {
    Ok(Duration::from_rational(Rational::new(1, 1024)?))
}

// ------------------------------------------------------------------------------------------------
// Errors
// ------------------------------------------------------------------------------------------------

/// Why the performance could not be computed. On a valid document only the arithmetic can fail;
/// the other variants report a bug of the model rather than a panic.
///
/// The `Display` texts are English and meant for developers (CLAUDE.md §7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
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
    /// The velocity on the 16-bit scale, 1..=65 535.
    pub velocity: u16,
}

/// The played notes of a document along its path (see the module documentation).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Performance {
    /// For each instrument, in score order, its notes sorted by start, then by pitch.
    instruments: Vec<(InstrumentId, Vec<PlayedNote>)>,
}

impl Performance {
    /// The played notes of `doc` along `path` (see the module documentation).
    ///
    /// # Errors
    /// [`PerformError`]: arithmetic only on a valid document.
    pub fn new(doc: &Document, path: &Unrolled) -> Result<Self, PerformError> {
        let mut instruments = Vec::new();
        for track in doc.tracks() {
            let instrument = track.instrument();
            let mut notes = Vec::new();
            perform_instrument(doc, path, instrument, &mut notes)?;
            notes.sort_by(|a, b| a.start.cmp(&b.start).then(a.pitch.cmp(&b.pitch)));
            instruments.push((instrument.id(), notes));
        }
        Ok(Self { instruments })
    }

    /// The played notes of an instrument, sorted by start, then by pitch; empty for an unknown
    /// instrument. O(instruments).
    #[must_use]
    pub fn notes(&self, instrument: InstrumentId) -> &[PlayedNote] {
        self.instruments
            .iter()
            .find(|(id, _)| *id == instrument)
            .map_or(&[], |(_, notes)| notes.as_slice())
    }

    /// Every instrument with its notes, in score order. O(1).
    #[must_use]
    pub fn instruments(&self) -> &[(InstrumentId, Vec<PlayedNote>)] {
        &self.instruments
    }
}

// ------------------------------------------------------------------------------------------------
// Items: the events a voice plays along the path
// ------------------------------------------------------------------------------------------------

/// One event played by a voice at one place of the path.
#[derive(Debug, Clone)]
struct Item {
    /// The played bar.
    played: u32,
    /// The pass of its notes.
    pass: u32,
    /// Where it starts on the unrolled timeline.
    start: Position,
    /// Its metric duration (zero for a grace note).
    length: Duration,
    /// The event.
    event: Event,
}

impl Item {
    /// Where it ends on the unrolled timeline, as written. O(1).
    fn end(&self) -> Result<Position, RationalError> {
        self.start.checked_add(self.length)
    }
}

/// The items of a voice, in playing order: for each played bar, the events of the voice there (or,
/// on a staff with bar repeats, in its source bar), in chunk order, those silenced by the pass
/// rules left out. O(path × staves × events of a bar).
///
/// # Errors
/// The path's and the document's errors (a bug of the model).
fn items_of(
    doc: &Document,
    path: &Unrolled,
    voice: &orchestra_core_model::Voice,
    staves: &[StaffId],
) -> Result<Vec<Item>, PerformError> {
    let bars = doc.bars();
    let meter = doc.meter_map();
    let mut items = Vec::new();
    for (k, played) in path.played().iter().enumerate() {
        // The written bars the voice's events come from in this played bar, one per staff.
        let mut sources: Vec<(StaffId, orchestra_core_model::BarId, u32)> = Vec::new();
        for &staff in staves {
            let content = path.content(k, staff).ok_or(UnrollError::Endless)?;
            sources.push((staff, content.bar, content.pass));
        }
        let mut here: Vec<(Duration, u8, usize, Item)> = Vec::new();
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
                        start,
                        length: if event.grace().is_some() {
                            Duration::ZERO
                        } else {
                            metric
                        },
                        event: event.clone(),
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

/// The played notes of one instrument, appended to `out` (unsorted).
///
/// # Errors
/// As [`Performance::new`].
fn perform_instrument(
    doc: &Document,
    path: &Unrolled,
    instrument: &orchestra_core_model::Instrument,
    out: &mut Vec<PlayedNote>,
) -> Result<(), PerformError> {
    let staves: Vec<StaffId> = instrument
        .staves()
        .iter()
        .map(orchestra_core_model::Staff::id)
        .collect();
    let voices = instrument.voices();
    let mut items: Vec<Vec<Item>> = Vec::with_capacity(voices.len());
    for voice in voices {
        items.push(items_of(doc, path, voice, &staves)?);
    }

    // Every notehead of a main event, by ID: where it is played (voice, item), for the ties. A
    // notehead played several times (a repeat) has several places.
    let mut places: HashMap<NoteId, Vec<(usize, usize)>> = HashMap::new();
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

    // N1, N2: chains, followed notehead by notehead along the ties that sound next on the path.
    let mut continued: std::collections::HashSet<(usize, usize, NoteId)> =
        std::collections::HashSet::new();
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

    // The notes of the chains, before the grace notes move their starts and ends.
    let mut notes: Vec<(Chain, PlayedNote)> = Vec::new();
    for chain in chains {
        let note = chain_note(&items, instrument.id(), voices, &chain)?;
        notes.push((chain, note));
    }
    // The notes of the grace chords, and the time they take from their neighbours (§5, N5, N6).
    let mut graces: Vec<PlayedNote> = Vec::new();
    for (v, list) in items.iter().enumerate() {
        place_graces(list, v, instrument.id(), voices, &mut notes, &mut graces)?;
    }
    // Arpeggios (N7), then the bounds (§8).
    let mut all: Vec<PlayedNote> = notes.into_iter().map(|(_, n)| n).collect();
    all.extend(graces);
    arpeggiate(&items, &mut all)?;
    for note in &mut all {
        bound(note)?;
    }
    out.extend(all);
    Ok(())
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

/// The change of velocity of the accents of the first notehead of a chain, in percent (ADR-0027
/// §3): accent +20, marcato +30, soft accent and stress +10, unstress −10, added. O(articulations).
fn velocity_percent(set: ArticulationSet) -> i64 {
    [
        (Articulation::Accent, 20),
        (Articulation::Marcato, 30),
        (Articulation::SoftAccent, 10),
        (Articulation::Stress, 10),
        (Articulation::Unstress, -10),
    ]
    .into_iter()
    .filter(|(a, _)| set.contains(*a))
    .map(|(_, p)| p)
    .sum()
}

/// A velocity changed by `percent` and by `delta` steps, bounded to the 16-bit scale. O(1).
fn velocity(base: u16, percent: i64, delta: i32) -> u16 {
    let scaled = i64::from(base).saturating_mul(100_i64.saturating_add(percent)) / 100;
    let v = scaled
        .saturating_add(i64::from(delta))
        .clamp(1, MAX_VELOCITY);
    u16::try_from(v).unwrap_or(u16::MAX)
}

/// The played note of a chain, before grace notes and arpeggios (ADR-0027 §2, §3, §8, N3, N8).
///
/// # Errors
/// [`PerformError::Unroll`] for a link to nothing (a bug); the arithmetic and pitch errors.
fn chain_note(
    items: &[Vec<Item>],
    instrument: InstrumentId,
    voices: &[orchestra_core_model::Voice],
    chain: &Chain,
) -> Result<PlayedNote, PerformError> {
    let link = |k: usize| -> Result<(&Item, orchestra_core_model::Note), PerformError> {
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
    // N3: the shortening on the last notehead, of its own written length.
    let last_length = last
        .length
        .checked_mul(length_factor(last_chord.articulations())?)?;
    let mut end = last.start.checked_add(last_length)?;
    // N8: the onset and the velocity from the first notehead, the lengths of every notehead added.
    let mut length_offset = Duration::ZERO;
    for k in 0..chain.links.len() {
        let (_, note) = link(k)?;
        length_offset = length_offset.checked_add(note.offsets().length)?;
    }
    let offsets = first_note.offsets();
    let start = first.start.checked_add(offsets.onset)?;
    end = end.checked_add(offsets.onset)?.checked_add(length_offset)?;
    let &(v, _, _) = chain.links.first().ok_or(UnrollError::Endless)?;
    let voice = voices.get(v).ok_or(UnrollError::Endless)?.id();
    Ok(PlayedNote {
        note: first_note.id(),
        event: first.event.id(),
        instrument,
        voice,
        staff: first.event.staff(),
        played: first.played,
        pass: first.pass,
        start,
        end,
        pitch: first_note.pitch().sounding()?,
        velocity: velocity(
            PROVISIONAL_VELOCITY,
            velocity_percent(first_chord.articulations()),
            offsets.velocity,
        ),
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
    list: &[Item],
    v: usize,
    instrument: InstrumentId,
    voices: &[orchestra_core_model::Voice],
    notes: &mut [(Chain, PlayedNote)],
    graces: &mut Vec<PlayedNote>,
) -> Result<(), PerformError> {
    let voice = voices.get(v).ok_or(UnrollError::Endless)?.id();
    let unit = grace_length()?;
    let half = Rational::new(1, 2)?;
    let mut i = 0;
    while i < list.len() {
        let Some(item) = list.get(i) else { break };
        let Some(grace) = item.event.grace() else {
            i = i.saturating_add(1);
            continue;
        };
        if grace.attach == GraceAttach::After {
            // A run of `After` graces: they end where the note they follow ends.
            let mut run = vec![i];
            let mut j = i.saturating_add(1);
            while let Some(next) = list.get(j) {
                if next
                    .event
                    .grace()
                    .is_some_and(|g| g.attach == GraceAttach::After)
                    && next.start == item.start
                {
                    run.push(j);
                    j = j.saturating_add(1);
                } else {
                    break;
                }
            }
            let (at, limit) = previous_end(notes, v, item.start)?;
            let mut total = unit.checked_mul(Rational::from_integer(
                i64::try_from(run.len()).unwrap_or(1),
            )?)?;
            if let Some(limit) = limit {
                let max = limit.checked_mul(half)?;
                if total > max {
                    total = max;
                }
            }
            let each = total.checked_div(Rational::from_integer(
                i64::try_from(run.len()).unwrap_or(1),
            )?)?;
            // N6: acciaccaturas of the next note at the same point come after these graces, so
            // these end where those begin (each acciaccatura lasts one unit, before the beat).
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
                .filter(|x| {
                    x.event
                        .grace()
                        .is_some_and(|g| g.kind == GraceKind::Acciaccatura)
                })
                .count();
            let reserve = unit.checked_mul(Rational::from_integer(
                i64::try_from(following).unwrap_or(0),
            )?)?;
            let from = at.checked_sub(reserve)?.checked_sub(total)?;
            shorten_previous(notes, v, item.start, from);
            for (k, &g) in run.iter().enumerate() {
                let start = from.checked_add(
                    each.checked_mul(Rational::from_integer(i64::try_from(k).unwrap_or(0))?)?,
                )?;
                push_grace(list, g, instrument, voice, start, each, graces)?;
            }
            i = j;
            continue;
        }
        // A run of `Before` graces, then their main event.
        let mut run = vec![i];
        let mut j = i.saturating_add(1);
        while let Some(next) = list.get(j) {
            if next
                .event
                .grace()
                .is_some_and(|g| g.attach == GraceAttach::Before)
                && next.start == item.start
            {
                run.push(j);
                j = j.saturating_add(1);
            } else {
                break;
            }
        }
        let accs: Vec<usize> = run
            .iter()
            .copied()
            .filter(|&g| {
                list.get(g)
                    .and_then(|x| x.event.grace())
                    .is_some_and(|x| x.kind == GraceKind::Acciaccatura)
            })
            .collect();
        let apps: Vec<usize> = run.iter().copied().filter(|g| !accs.contains(g)).collect();
        let beat = item.start;
        // Acciaccaturas before the beat, taking time from the previous note (N5).
        if !accs.is_empty() {
            let n = Rational::from_integer(i64::try_from(accs.len()).unwrap_or(1))?;
            let mut total = unit.checked_mul(n)?;
            let (prev_end, limit) = previous_end(notes, v, beat)?;
            if let Some(limit) = limit
                && prev_end == beat
            {
                let max = limit.checked_mul(half)?;
                if total > max {
                    total = max;
                }
            }
            let each = total.checked_div(n)?;
            let from = beat.checked_sub(total)?;
            shorten_previous(notes, v, beat, from);
            for (k, &g) in accs.iter().enumerate() {
                let start = from.checked_add(
                    each.checked_mul(Rational::from_integer(i64::try_from(k).unwrap_or(0))?)?,
                )?;
                push_grace(list, g, instrument, voice, start, each, graces)?;
            }
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
            let n = Rational::from_integer(i64::try_from(apps.len()).unwrap_or(1))?;
            let each = total.checked_div(n)?;
            for (k, &g) in apps.iter().enumerate() {
                let start = beat.checked_add(
                    each.checked_mul(Rational::from_integer(i64::try_from(k).unwrap_or(0))?)?,
                )?;
                push_grace(list, g, instrument, voice, start, each, graces)?;
            }
            // The main note (every chain starting at that item) starts after them.
            let new_start = beat.checked_add(total)?;
            for (chain, note) in notes.iter_mut() {
                if chain
                    .links
                    .first()
                    .is_some_and(|&(cv, ci, _)| cv == v && ci == j)
                {
                    note.start = new_start;
                }
            }
        }
        i = j;
    }
    Ok(())
}

/// The end of the previous note of voice `v` before `at` (the latest end at or before `at` among
/// its chains), with its length, or `(at, None)` when there is none. O(notes).
fn previous_end(
    notes: &[(Chain, PlayedNote)],
    v: usize,
    at: Position,
) -> Result<(Position, Option<Duration>), PerformError> {
    let mut best: Option<&PlayedNote> = None;
    for (chain, note) in notes {
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
fn shorten_previous(notes: &mut [(Chain, PlayedNote)], v: usize, at: Position, from: Position) {
    for (chain, note) in notes.iter_mut() {
        if chain.links.first().is_some_and(|&(cv, _, _)| cv == v)
            && note.end > from
            && note.end <= at
            && note.start < from
        {
            note.end = from;
        }
    }
}

/// Appends the notes of the grace chord of item `g`, from `start`, lasting `length`. O(noteheads).
#[allow(
    clippy::too_many_arguments,
    reason = "the grace's place and its note's identity: a struct for one call would only rename them"
)]
fn push_grace(
    list: &[Item],
    g: usize,
    instrument: InstrumentId,
    voice: VoiceId,
    start: Position,
    length: Duration,
    graces: &mut Vec<PlayedNote>,
) -> Result<(), PerformError> {
    let item = list.get(g).ok_or(UnrollError::Endless)?;
    let Some(chord) = item.event.chord() else {
        return Ok(());
    };
    for note in chord.notes() {
        let offsets = note.offsets();
        graces.push(PlayedNote {
            note: note.id(),
            event: item.event.id(),
            instrument,
            voice,
            staff: item.event.staff(),
            played: item.played,
            pass: item.pass,
            start: start.checked_add(offsets.onset)?,
            end: start
                .checked_add(length)?
                .checked_add(offsets.onset)?
                .checked_add(offsets.length)?,
            pitch: note.pitch().sounding()?,
            velocity: velocity(
                PROVISIONAL_VELOCITY,
                velocity_percent(chord.articulations()),
                offsets.velocity,
            ),
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
fn arpeggiate(items: &[Vec<Item>], notes: &mut [PlayedNote]) -> Result<(), PerformError> {
    let step = arpeggio_step()?;
    // The events with an arpeggio sign, and their direction.
    let mut signs: HashMap<EventId, bool> = HashMap::new();
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
    let mut groups: HashMap<(EventId, u32), Vec<usize>> = HashMap::new();
    for (k, note) in notes.iter().enumerate() {
        if signs.contains_key(&note.event) {
            groups.entry((note.event, note.played)).or_default().push(k);
        }
    }
    for ((event, _), mut group) in groups {
        let up = signs.get(&event).copied().unwrap_or(true);
        group.sort_by(|&a, &b| {
            let (pa, pb) = (notes.get(a).map(|n| n.pitch), notes.get(b).map(|n| n.pitch));
            if up { pa.cmp(&pb) } else { pb.cmp(&pa) }
        });
        for (k, index) in group.into_iter().enumerate() {
            if let Some(note) = notes.get_mut(index) {
                let shift =
                    step.checked_mul(Rational::from_integer(i64::try_from(k).unwrap_or(0))?)?;
                note.start = note.start.checked_add(shift)?;
            }
        }
    }
    Ok(())
}

/// The bounds of ADR-0027 §8: a note lasts at least 1/1024 of a whole note (the velocity is already
/// bounded where it is computed). O(1).
///
/// # Errors
/// The arithmetic errors.
fn bound(note: &mut PlayedNote) -> Result<(), PerformError> {
    let shortest = note.start.checked_add(min_length()?)?;
    if note.end < shortest {
        note.end = shortest;
    }
    Ok(())
}
