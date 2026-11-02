// SPDX-License-Identifier: GPL-3.0-or-later

//! The skeleton of the document: tracks, instruments, staves, voices, bars, and the two time maps
//! (ADR-0022 §1–3, SCALETTA F1.08).
//!
//! # What lives here
//! - [`Document`]: the one project document of ADR-0003. Today it holds the structure only; the
//!   events of the voices arrive in F1.09, the form in F1.10.
//! - [`Track`] → [`Instrument`] → [`Staff`] and [`Voice`]: one instrument per instrument track,
//!   one or more staves per instrument, voices that **belong to the instrument** and have a home
//!   staff (ADR-0022 §2), so that a melody crossing from one staff to the other stays one voice.
//! - [`BarList`]: the ordered list of [`BarId`]s (ADR-0022 §3). It says *which* bar is which and
//!   how many bars there are, and nothing else: bar lengths live only in the [`MeterMap`].
//! - [`DocumentBuilder`]: how a **new** document is created (by the file loader, the generator of
//!   the reference score, tests and benchmarks). Owner's decision of 2026-11-02: an existing
//!   document changes only through the commands of F1.11 (ADR-0003 §4); creating one is not
//!   changing one, so the builder does not need commands.
//!
//! # Limits
//! At most [`MAX_STAVES_PER_INSTRUMENT`] staves and [`MAX_VOICES_PER_INSTRUMENT`] voices per
//! instrument (owner's decision of 2026-11-02). They are not musical limits (the interface may
//! present four voices per staff, or any other number) but technical ones: no real score gets
//! close, and they stop a corrupt or malicious file from asking for millions of voices and
//! exhausting the memory when it is opened. Raising them later changes a constant and a test, not
//! the file format.
//!
//! # Lookups and their cost
//! Bars are many (thousands), so [`BarList`] keeps an `IdMap` from `BarId` to index: O(1) on
//! average in both directions. Tracks, staves and voices are few (tens to a few hundred in a large
//! orchestral score), so they are found by a linear scan: a few hundred comparisons of `u64`, which
//! is cheaper than keeping a hash index in step with every structural change. ADR-0022 §1's
//! `ID → location` index is for events, which are hundreds of thousands (F1.09).

use std::error::Error;
use std::fmt;

use orchestra_core_time::{BarIndex, MeterMap, MeterMapError, Position, TempoMap};

use crate::id::{BarId, IdCounter, IdMap, InstrumentId, StaffId, TrackId, VoiceId};
use crate::pitch::Interval;

/// The most staves one instrument can have (owner's decision, 2026-11-02). Organ music uses
/// three, some piano and harp writing four; 16 leaves ample room for unusual layouts.
pub const MAX_STAVES_PER_INSTRUMENT: usize = 16;

/// The most voices one instrument can have (owner's decision, 2026-11-02). Notation programs
/// commonly offer four voices per staff; 256 per instrument is far beyond any real score.
pub const MAX_VOICES_PER_INSTRUMENT: usize = 256;

// ------------------------------------------------------------------------------------------------
// Errors
// ------------------------------------------------------------------------------------------------

/// Why building or querying the document skeleton failed.
///
/// The `Display` texts are English and meant for developers; the interface maps each variant to a
/// Fluent key, like every other user-visible text (CLAUDE.md §7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DocumentError {
    /// The document's ID counter has handed out every `u64` value (ADR-0022 §1). Unreachable in
    /// practice; reported rather than wrapping around, which would reuse IDs.
    IdsExhausted,
    /// An instrument was requested with no staves: every instrument has at least one.
    NoStaves,
    /// An instrument was requested with more than [`MAX_STAVES_PER_INSTRUMENT`] staves.
    TooManyStaves,
    /// A voice would be the instrument's voice number [`MAX_VOICES_PER_INSTRUMENT`] + 1.
    TooManyVoices,
    /// The document already has the most bars a [`BarIndex`] can address (2³² − 1, the last index
    /// being 2³² − 2 so that the count itself fits in a `u32`).
    TooManyBars,
    /// No staff of the document has this ID.
    UnknownStaff(StaffId),
    /// No bar of the document has this ID.
    UnknownBar(BarId),
    /// A meter map lookup failed (only reachable with absurd meters, ADR-0018).
    MeterMap(MeterMapError),
}

impl fmt::Display for DocumentError {
    /// Writes a short English description of the error, for developers (see the type's docs).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::IdsExhausted => f.write_str("the document's ID counter is exhausted"),
            Self::NoStaves => f.write_str("an instrument needs at least one staff"),
            Self::TooManyStaves => write!(
                f,
                "an instrument can have at most {MAX_STAVES_PER_INSTRUMENT} staves"
            ),
            Self::TooManyVoices => write!(
                f,
                "an instrument can have at most {MAX_VOICES_PER_INSTRUMENT} voices"
            ),
            Self::TooManyBars => f.write_str("the document has the most bars it can address"),
            Self::UnknownStaff(id) => write!(f, "no staff has the ID {}", id.get()),
            Self::UnknownBar(id) => write!(f, "no bar has the ID {}", id.get()),
            Self::MeterMap(e) => write!(f, "meter map: {e}"),
        }
    }
}

impl Error for DocumentError {
    /// The underlying meter map error, if any.
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::MeterMap(e) => Some(e),
            _ => None,
        }
    }
}

impl From<MeterMapError> for DocumentError {
    /// Wraps a meter map error, so that `?` works on `core-time` lookups.
    fn from(e: MeterMapError) -> Self {
        Self::MeterMap(e)
    }
}

// ------------------------------------------------------------------------------------------------
// Staves, voices, instruments, tracks
// ------------------------------------------------------------------------------------------------

/// A staff of an instrument (ADR-0022 §2).
///
/// Today a staff is only its identity and its place among the instrument's staves (top to
/// bottom). Its clef and the changes of clef and key signature are designed with the events in
/// F1.09 (owner's decision of 2026-11-02), because they change along the score like other marks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Staff {
    /// The staff's identity.
    id: StaffId,
}

impl Staff {
    /// The staff's identity. O(1).
    pub const fn id(&self) -> StaffId {
        self.id
    }
}

/// A voice of an instrument (ADR-0022 §2).
///
/// A voice is an independent line, with its own rhythm and stems. It belongs to the instrument,
/// not to a staff: its **home staff** is where its events are drawn by default, and each event
/// (F1.09) records the staff it is actually drawn on, so a melody passing from the right hand to
/// the left hand of a piano part stays one voice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Voice {
    /// The voice's identity.
    id: VoiceId,
    /// The staff its events are drawn on by default: always a staff of the same instrument.
    home_staff: StaffId,
}

impl Voice {
    /// The voice's identity. O(1).
    pub const fn id(&self) -> VoiceId {
        self.id
    }

    /// The staff the voice's events are drawn on by default (a staff of the same instrument).
    /// O(1).
    pub const fn home_staff(&self) -> StaffId {
        self.home_staff
    }
}

/// The instrument of an instrument track (ADR-0022 §2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Instrument {
    /// The instrument's identity.
    id: InstrumentId,
    /// The name the user gave it ("Flute", "Clarinetto in Si♭ 1"…). It is the user's own text,
    /// not a Fluent key: it is shown as written, in whatever language the user wrote it. Default
    /// names in the interface language come with the interface (F2), not from the model.
    name: String,
    /// The interval from written to sounding pitch (ADR-0023 §7): a major second down for a B♭
    /// clarinet, a unison for a concert-pitch instrument. Pitches are stored at concert pitch and
    /// the written ones derived (ADR-0004), so this is the only place the transposition lives.
    transposition: Interval,
    /// The staves, top to bottom; between 1 and [`MAX_STAVES_PER_INSTRUMENT`].
    staves: Vec<Staff>,
    /// The voices, in creation order; between 1 and [`MAX_VOICES_PER_INSTRUMENT`]. Each home
    /// staff is one of `staves`.
    voices: Vec<Voice>,
}

impl Instrument {
    /// The instrument's identity. O(1).
    pub const fn id(&self) -> InstrumentId {
        self.id
    }

    /// The name the user gave the instrument. O(1).
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The interval from written to sounding pitch (ADR-0023 §7). O(1).
    pub const fn transposition(&self) -> Interval {
        self.transposition
    }

    /// The staves, top to bottom (never empty). O(1).
    pub fn staves(&self) -> &[Staff] {
        &self.staves
    }

    /// The voices, in creation order (never empty). O(1).
    pub fn voices(&self) -> &[Voice] {
        &self.voices
    }

    /// The staff with this ID, if it belongs to this instrument. O(staves) ≤ 16 comparisons.
    pub fn staff(&self, id: StaffId) -> Option<&Staff> {
        self.staves.iter().find(|s| s.id == id)
    }

    /// The voice with this ID, if it belongs to this instrument. O(voices) ≤ 256 comparisons.
    pub fn voice(&self, id: VoiceId) -> Option<&Voice> {
        self.voices.iter().find(|v| v.id == id)
    }
}

/// A track of the document (ADR-0022 §2).
///
/// Only instrument tracks exist today. Audio tracks (F3) and video tracks (F5) will make this a
/// choice of kinds, with a new version of the file format and its migration (ADR-0010); nothing
/// is written for them before they are needed (SCALETTA §1, rule 6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Track {
    /// The track's identity.
    id: TrackId,
    /// The instrument the track plays.
    instrument: Instrument,
}

impl Track {
    /// The track's identity. O(1).
    pub const fn id(&self) -> TrackId {
        self.id
    }

    /// The instrument of the track. O(1).
    pub const fn instrument(&self) -> &Instrument {
        &self.instrument
    }
}

// ------------------------------------------------------------------------------------------------
// The bar list
// ------------------------------------------------------------------------------------------------

/// The ordered list of the document's bars (ADR-0022 §3).
///
/// Position `i` of the list is the bar with [`BarIndex`] `i` in the meter map. The list holds no
/// length, no meter, no number: those are in the [`MeterMap`], and repeating them here would be
/// the duplication ADR-0003 forbids. What it gives is a **stable identity** for each bar: what is
/// attached to a bar (barlines, voltas, rehearsal marks, the event chunks of F1.09) is keyed by
/// `BarId`, so inserting a bar (F1.11) shifts this list and the meter map's few entries, and
/// nothing else.
///
/// # Invariant
/// `index` is exactly the inverse of `order`: it has one entry per bar, `index[order[i]] == i`.
/// It is a derived cache (ADR-0003 §5), rebuilt from `order` whenever bars move.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BarList {
    /// The bars in score order.
    order: Vec<BarId>,
    /// The inverse of `order`: the index of each bar. Rebuilt when bars move.
    index: IdMap<BarId, u32>,
}

impl BarList {
    /// An empty bar list. Does not allocate. O(1).
    fn new() -> Self {
        Self {
            order: Vec::new(),
            index: IdMap::default(),
        }
    }

    /// How many bars the document has. O(1).
    pub fn len(&self) -> usize {
        self.order.len()
    }

    /// Whether the document has no bars at all. O(1).
    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }

    /// The bars in score order. O(1).
    pub fn ids(&self) -> &[BarId] {
        &self.order
    }

    /// The bar at this index, if the document has that many bars. O(1).
    pub fn id_at(&self, index: BarIndex) -> Option<BarId> {
        // `u32 → usize` is lossless on every platform we support (usize has at least 32 bits).
        self.order.get(usize::try_from(index.get()).ok()?).copied()
    }

    /// The index of this bar, if it belongs to the document. O(1) on average (one hash lookup).
    pub fn index_of(&self, id: BarId) -> Option<BarIndex> {
        self.index.get(&id).copied().map(BarIndex::new)
    }

    /// Appends a bar at the end. O(1) amortised: the new bar's index is the old length, and no
    /// other bar moves, so the inverse index only gains one entry.
    ///
    /// # Errors
    /// [`DocumentError::TooManyBars`] when the new index would not fit a [`BarIndex`] with the
    /// count still fitting a `u32` (so at most 2³² − 1 bars).
    fn push(&mut self, id: BarId) -> Result<(), DocumentError> {
        let index = u32::try_from(self.order.len()).map_err(|_| DocumentError::TooManyBars)?;
        // `u32::MAX` as an index would make the count 2³², which does not fit a `u32`.
        if index == u32::MAX {
            return Err(DocumentError::TooManyBars);
        }
        self.order.push(id);
        self.index.insert(id, index);
        Ok(())
    }
}

// ------------------------------------------------------------------------------------------------
// The document
// ------------------------------------------------------------------------------------------------

/// The one project document (ADR-0003): today, its skeleton (ADR-0022 §1–3).
///
/// Create one with a [`DocumentBuilder`]. Every reader (the views, playback, the file writer)
/// reads it through the methods below; the commands of F1.11 will be the only way to change it.
///
/// # Example
/// ```
/// use orchestra_core_model::{DocumentBuilder, Interval};
/// use orchestra_core_time::{Duration, Meter, MeterChange, MeterMap, Rational, Tempo, TempoMap};
///
/// let meters = MeterMap::new(MeterChange::new(Meter::new(&[4], 4)?))?;
/// let quarter = Duration::from_rational(Rational::new(1, 4)?);
/// let tempi = TempoMap::new(Tempo::new(quarter, Rational::from_integer(96)?)?)?;
///
/// let mut builder = DocumentBuilder::new(meters, tempi);
/// for _ in 0..8 {
///     builder.add_bar()?;
/// }
/// // A piano: two staves, so two voices, one per staff.
/// let piano = builder.add_instrument_track("Piano", Interval::new(0, Rational::ZERO), 2)?;
/// let document = builder.build();
///
/// assert_eq!(document.bars().len(), 8);
/// let instrument = document.track(piano).map(|t| t.instrument());
/// assert_eq!(instrument.map(|i| i.voices().len()), Some(2));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    /// Where every ID of this document comes from (saved in the file, F1.15).
    ids: IdCounter,
    /// The bar lengths, meters, beat groupings and numbering (ADR-0018, ADR-0019): the only place
    /// a bar's length is stored.
    meter_map: MeterMap,
    /// The tempo changes on written positions (ADR-0020, ADR-0021).
    tempo_map: TempoMap,
    /// Which bar is which (ADR-0022 §3).
    bars: BarList,
    /// The tracks, top to bottom as the score lists them.
    tracks: Vec<Track>,
}

impl Document {
    /// The meter map: bar lengths, meters, beat groupings, numbering. O(1).
    pub const fn meter_map(&self) -> &MeterMap {
        &self.meter_map
    }

    /// The tempo map, on written positions. O(1).
    pub const fn tempo_map(&self) -> &TempoMap {
        &self.tempo_map
    }

    /// The bar list. O(1).
    pub const fn bars(&self) -> &BarList {
        &self.bars
    }

    /// The tracks, in score order. O(1).
    pub fn tracks(&self) -> &[Track] {
        &self.tracks
    }

    /// The track with this ID. O(tracks).
    pub fn track(&self, id: TrackId) -> Option<&Track> {
        self.tracks.iter().find(|t| t.id == id)
    }

    /// The instrument with this ID. O(tracks).
    pub fn instrument(&self, id: InstrumentId) -> Option<&Instrument> {
        self.instruments().find(|i| i.id == id)
    }

    /// The staff with this ID and the instrument it belongs to. O(staves of the whole document):
    /// a few hundred comparisons in a large orchestral score.
    pub fn staff(&self, id: StaffId) -> Option<(&Instrument, &Staff)> {
        self.instruments()
            .find_map(|i| i.staff(id).map(|staff| (i, staff)))
    }

    /// The voice with this ID and the instrument it belongs to. O(voices of the whole document).
    pub fn voice(&self, id: VoiceId) -> Option<(&Instrument, &Voice)> {
        self.instruments()
            .find_map(|i| i.voice(id).map(|voice| (i, voice)))
    }

    /// Where this bar starts, as an absolute written position: the bar's index in the bar list,
    /// then the meter map. O(1) average + O(log m) for m meter changes (ADR-0018).
    ///
    /// # Errors
    /// [`DocumentError::UnknownBar`] if the bar does not belong to the document,
    /// [`DocumentError::MeterMap`] if the meter map arithmetic overflows (absurd meters only).
    pub fn bar_start(&self, id: BarId) -> Result<Position, DocumentError> {
        let index = self
            .bars
            .index_of(id)
            .ok_or(DocumentError::UnknownBar(id))?;
        Ok(self.meter_map.bar_start(index)?)
    }

    /// Every instrument, in track order. O(1) to create, O(tracks) to exhaust.
    fn instruments(&self) -> impl Iterator<Item = &Instrument> {
        self.tracks.iter().map(|t| &t.instrument)
    }
}

// ------------------------------------------------------------------------------------------------
// The builder
// ------------------------------------------------------------------------------------------------

/// Creates a new [`Document`] (owner's decision of 2026-11-02).
///
/// The builder is for documents that do not exist yet: a file being opened (F1.15), the generated
/// reference score (F1.16), tests and benchmarks. Once [`build`](Self::build) returns, the document
/// changes only through commands (F1.11, ADR-0003 §4).
///
/// # Order of the IDs
/// Every method takes its IDs from the document's single counter in the order it creates the
/// objects: a bar takes one; an instrument track takes the track's, then the instrument's, then
/// one per staff top to bottom, then one per voice. Two builders given the same calls therefore
/// produce identical documents, IDs included, which keeps generated scores and golden files
/// reproducible.
///
/// # Failures leave the builder unchanged
/// Each method checks everything (limits, the staff, room in the counter) **before** creating
/// anything, so an error never leaves a half-made instrument behind.
#[derive(Debug, Clone)]
pub struct DocumentBuilder {
    /// The document under construction.
    doc: Document,
}

impl DocumentBuilder {
    /// Starts a document with these time maps, no bars and no tracks. O(1), does not allocate.
    ///
    /// There is deliberately no default meter or tempo: they are the caller's choice (the
    /// interface's "new score" dialog, a file, the generator), not the model's.
    #[must_use]
    pub fn new(meter_map: MeterMap, tempo_map: TempoMap) -> Self {
        Self {
            doc: Document {
                ids: IdCounter::new(),
                meter_map,
                tempo_map,
                bars: BarList::new(),
                tracks: Vec::new(),
            },
        }
    }

    /// Appends a bar at the end of the bar list and returns its ID. Its length, meter and number
    /// come from the meter map, as for every bar. O(1) amortised.
    ///
    /// # Errors
    /// [`DocumentError::TooManyBars`] past 2³² − 1 bars, [`DocumentError::IdsExhausted`] when the
    /// counter is exhausted. The builder is unchanged on error.
    pub fn add_bar(&mut self) -> Result<BarId, DocumentError> {
        // Check the bar limit first, so that a failure does not consume an ID.
        if u32::try_from(self.doc.bars.len()).map_or(true, |n| n == u32::MAX) {
            return Err(DocumentError::TooManyBars);
        }
        let id = BarId::from_counter(self.doc.ids.next().ok_or(DocumentError::IdsExhausted)?);
        self.doc.bars.push(id)?;
        Ok(id)
    }

    /// Appends an instrument track with `staves` staves and **one voice per staff**, each voice's
    /// home staff being its staff (owner's decision of 2026-11-02): enough to write one part per
    /// staff at once; [`add_voice`](Self::add_voice) adds more. O(staves).
    ///
    /// `transposition` is the interval from written to sounding pitch (ADR-0023 §7): a unison
    /// (`Interval::new(0, Rational::ZERO)`) for an instrument at concert pitch.
    ///
    /// # Errors
    /// [`DocumentError::NoStaves`] for 0 staves, [`DocumentError::TooManyStaves`] beyond
    /// [`MAX_STAVES_PER_INSTRUMENT`], [`DocumentError::IdsExhausted`] when the counter cannot hand
    /// out the 2 + 2 × `staves` IDs needed. The builder is unchanged on error.
    pub fn add_instrument_track(
        &mut self,
        name: impl Into<String>,
        transposition: Interval,
        staves: usize,
    ) -> Result<TrackId, DocumentError> {
        if staves == 0 {
            return Err(DocumentError::NoStaves);
        }
        if staves > MAX_STAVES_PER_INSTRUMENT {
            return Err(DocumentError::TooManyStaves);
        }
        // One track ID, one instrument ID, one per staff, one per voice. `staves` ≤ 16, so the
        // conversion and the arithmetic cannot fail; `checked_*` keeps that visible anyway.
        let needed = u64::try_from(staves)
            .ok()
            .and_then(|s| s.checked_mul(2))
            .and_then(|s| s.checked_add(2))
            .ok_or(DocumentError::TooManyStaves)?;
        if !self.doc.ids.has_room_for(needed) {
            return Err(DocumentError::IdsExhausted);
        }

        // From here on every `next()` succeeds (checked just above); the `?` stays because the
        // compiler cannot know that, and an unwrap is not allowed in library code.
        let ids = &mut self.doc.ids;
        let track_id = TrackId::from_counter(ids.next().ok_or(DocumentError::IdsExhausted)?);
        let instrument_id =
            InstrumentId::from_counter(ids.next().ok_or(DocumentError::IdsExhausted)?);
        let mut staff_list = Vec::with_capacity(staves);
        for _ in 0..staves {
            let id = StaffId::from_counter(ids.next().ok_or(DocumentError::IdsExhausted)?);
            staff_list.push(Staff { id });
        }
        let mut voices = Vec::with_capacity(staves);
        for staff in &staff_list {
            let id = VoiceId::from_counter(ids.next().ok_or(DocumentError::IdsExhausted)?);
            voices.push(Voice {
                id,
                home_staff: staff.id,
            });
        }

        self.doc.tracks.push(Track {
            id: track_id,
            instrument: Instrument {
                id: instrument_id,
                name: name.into(),
                transposition,
                staves: staff_list,
                voices,
            },
        });
        Ok(track_id)
    }

    /// Adds a voice to the instrument that owns `home_staff`, with that staff as its home, and
    /// returns its ID. O(staves of the document) to find the instrument.
    ///
    /// Only the staff is asked for: staff IDs are unique in the whole document, so the staff names
    /// its instrument too, and a voice can never be given the staff of another instrument.
    ///
    /// # Errors
    /// [`DocumentError::UnknownStaff`] if no staff has this ID, [`DocumentError::TooManyVoices`]
    /// if the instrument already has [`MAX_VOICES_PER_INSTRUMENT`] voices,
    /// [`DocumentError::IdsExhausted`] when the counter is exhausted. The builder is unchanged on
    /// error.
    pub fn add_voice(&mut self, home_staff: StaffId) -> Result<VoiceId, DocumentError> {
        // Find the owning track by position rather than by reference: the ID comes from the
        // counter, another field of the same document, and holding a `&mut` into the tracks
        // while taking it would borrow the document twice.
        let track_index = self
            .doc
            .tracks
            .iter()
            .position(|t| t.instrument.staff(home_staff).is_some())
            .ok_or(DocumentError::UnknownStaff(home_staff))?;
        let track = self
            .doc
            .tracks
            .get_mut(track_index)
            .ok_or(DocumentError::UnknownStaff(home_staff))?;
        // Every check happens before the counter is touched, so an error consumes no ID.
        if track.instrument.voices.len() >= MAX_VOICES_PER_INSTRUMENT {
            return Err(DocumentError::TooManyVoices);
        }
        let id = VoiceId::from_counter(self.doc.ids.next().ok_or(DocumentError::IdsExhausted)?);
        track.instrument.voices.push(Voice { id, home_staff });
        Ok(id)
    }

    /// The document as built so far, for callers that need to read it while building (a
    /// generator asking for the staves of the track it just added). O(1).
    pub const fn document(&self) -> &Document {
        &self.doc
    }

    /// Finishes the document. O(1).
    #[must_use]
    pub fn build(self) -> Document {
        self.doc
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU64;

    use orchestra_core_time::{Duration, Meter, MeterChange, Rational, Tempo};

    use super::*;

    /// An empty builder in 4/4 at ♩ = 120.
    fn builder() -> DocumentBuilder {
        let meters = MeterMap::new(MeterChange::new(Meter::new(&[4], 4).unwrap())).unwrap();
        let quarter = Duration::from_rational(Rational::new(1, 4).unwrap());
        let tempo = Tempo::new(quarter, Rational::from_integer(120).unwrap()).unwrap();
        DocumentBuilder::new(meters, TempoMap::new(tempo).unwrap())
    }

    /// With the counter near its end, an instrument that does not fit is refused without consuming
    /// any ID or adding anything, and a smaller one that fits takes exactly the last IDs.
    #[test]
    fn exhaustion_leaves_the_builder_unchanged() {
        let mut b = builder();
        // Room for exactly 6 IDs: an instrument with 2 staves (2 + 4 = 6) fits, one with 3 does not.
        b.doc.ids = IdCounter::starting_at(NonZeroU64::new(u64::MAX - 5).unwrap());
        let unison = Interval::new(0, Rational::ZERO);
        let before = b.doc.clone();
        assert_eq!(
            b.add_instrument_track("Organ", unison, 3),
            Err(DocumentError::IdsExhausted)
        );
        assert_eq!(b.doc, before);
        let piano = b.add_instrument_track("Piano", unison, 2).unwrap();
        assert_eq!(piano.get(), u64::MAX - 5);
        let staff = b.doc.tracks[0].instrument.staves[0].id;
        let before = b.doc.clone();
        assert_eq!(b.add_voice(staff), Err(DocumentError::IdsExhausted));
        assert_eq!(b.add_bar(), Err(DocumentError::IdsExhausted));
        assert_eq!(b.doc, before);
    }
}
