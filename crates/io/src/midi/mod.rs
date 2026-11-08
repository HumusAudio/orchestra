// SPDX-License-Identifier: GPL-3.0-or-later

//! The export of the **played notes** as a Standard MIDI File 1.0 (ADR-0032; SCALETTA F1.18): the
//! first way to *listen* to the model, with any player, long before the audio engine (F3).
//!
//! # MIDI 1.0 only here
//! The played notes of ADR-0027 carry at least the resolution of MIDI 2.0 (16-bit velocity, exact
//! microtonal pitch, a per-note volume profile). MIDI 1.0 appears **only** in this encoder,
//! because today nearly every player reads only that; every reduction to 7 bits or to a key plus a
//! bend happens here, never earlier (ADR-0032 E1). Inside the program the default will be MIDI 2.0
//! (owner's requirement, SCALETTA F3.01).
//!
//! # What the file contains (ADR-0032 E1)
//! A format-1 file at 960 ticks per quarter note:
//! - **track 0, the conductor**: the tempo events rebuilt from the path's seconds (E4, module
//!   `timing`), a time signature for every played bar whose length differs from the previous one
//!   (E5, module `meters`), markers where a length cannot be written;
//! - **one track per instrument**, in score order, named with the instrument's name, on its own
//!   channel (shared beyond 15 instruments, E6), with the notes, the pitch bends of the microtones
//!   (E9) and the CC 11 curve of the attack profile (E8) (module `notes`).
//!
//! # How it is used (E11)
//! ```ignore
//! let path = Unrolled::new(&doc)?;
//! let performance = Performance::new(&doc, &path)?;
//! let export = orchestra_io::midi::export(&doc, &path, &performance, &MidiOptions::new())?;
//! orchestra_io::midi::save(&export, Path::new("score.mid"))?;
//! ```
//! The **warnings** come back with the bytes, never printed: the export never fails for a musical
//! reason, only for an arithmetic error or a path it cannot read.
//!
//! # Complexity
//! O(n log n) for n played notes (one sort of the events of each channel and track), plus
//! O(path × log) for the tempo and the time signatures; memory O(n).

mod meters;
mod notes;
mod smf;
mod timing;

use std::error::Error;
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use orchestra_core_model::{Document, InstrumentId, NoteId};
use orchestra_core_time::{Position, Rational, RationalError};
use orchestra_playback::{Performance, UnrollError, Unrolled};

use smf::{Event, Message};

/// The largest General MIDI program number (programs are 0 ..= 127 on the wire, shown to users
/// as 1 ..= 128).
pub const MAX_PROGRAM: u8 = 127;

// ------------------------------------------------------------------------------------------------
// Options, result, warnings, errors
// ------------------------------------------------------------------------------------------------

/// What the caller can choose about the file (ADR-0032 E5, E7).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MidiOptions {
    /// The General MIDI program of some instruments (E7), sorted by instrument; the others get no
    /// program change and the player chooses (usually a piano).
    programs: Vec<(InstrumentId, u8)>,
    /// The text of the marker at the start of a lead-in (E5), if any. The library writes no text
    /// of its own in any language (CLAUDE.md §7): the caller, which knows the interface language,
    /// provides it, or there is no marker.
    lead_in_marker: Option<String>,
}

impl MidiOptions {
    /// No programs, no lead-in marker. O(1).
    #[must_use]
    pub const fn new() -> Self {
        Self {
            programs: Vec::new(),
            lead_in_marker: None,
        }
    }

    /// Gives `instrument` the General MIDI program `program` (0 ..= 127), replacing any earlier
    /// one. O(p) for the p programs set.
    ///
    /// # Errors
    /// [`MidiError::InvalidProgram`] above [`MAX_PROGRAM`]; the options are then unchanged.
    pub fn set_program(&mut self, instrument: InstrumentId, program: u8) -> Result<(), MidiError> {
        if program > MAX_PROGRAM {
            return Err(MidiError::InvalidProgram(program));
        }
        match self.programs.binary_search_by_key(&instrument, |(i, _)| *i) {
            Ok(at) => {
                if let Some(entry) = self.programs.get_mut(at) {
                    entry.1 = program;
                }
            }
            Err(at) => self.programs.insert(at, (instrument, program)),
        }
        Ok(())
    }

    /// The General MIDI program of `instrument`, if one was set. O(log p).
    #[must_use]
    pub fn program(&self, instrument: InstrumentId) -> Option<u8> {
        self.programs
            .binary_search_by_key(&instrument, |(i, _)| *i)
            .ok()
            .and_then(|at| self.programs.get(at))
            .map(|(_, p)| *p)
    }

    /// Sets the text of the marker written at the start of a lead-in (`None`: no marker). O(1).
    pub fn set_lead_in_marker(&mut self, text: Option<String>) {
        self.lead_in_marker = text;
    }
}

/// A finished export: the bytes of the file and what could not be written exactly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MidiExport {
    /// The bytes of the Standard MIDI File.
    pub bytes: Vec<u8>,
    /// The warnings, in a fixed order: the lead-in, the shared channels, the notes (in channel
    /// and time order), the bars, the tempo.
    pub warnings: Vec<MidiWarning>,
}

/// Something the MIDI file could not say exactly (ADR-0032 E6). Each names what it concerns, so
/// that the interface (later) and the example (now) can show it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum MidiWarning {
    /// The notes before the first beat (an acciaccatura before the first note) made the whole
    /// file start later, by this many quarter notes, in a first bar of its own (E5).
    LeadIn {
        /// The length of the lead-in, in quarter notes.
        quarters: u32,
    },
    /// The instrument shares its channel with an earlier one (more than 15 instruments, E6): the
    /// CC 11 curves, the bends and the programs of the two affect each other.
    ChannelShared {
        /// The instrument.
        instrument: InstrumentId,
        /// The channel, 0 ..= 15 as on the wire (shown to users as 1 ..= 16).
        channel: u8,
    },
    /// A microtonal note met a different bend still sounding on its channel: it was played on the
    /// nearest key under the channel's current bend (E9).
    BendRounded {
        /// The note (the first notehead of its tie chain).
        note: NoteId,
        /// The played bar where it starts (index on the path).
        played: u32,
    },
    /// A note outside the 128 MIDI keys was not written (E9).
    PitchOutOfRange {
        /// The note.
        note: NoteId,
        /// The played bar where it starts.
        played: u32,
    },
    /// The length of a played bar cannot be written as a time signature (E5): a marker with the
    /// meter's text was written instead.
    MeterNotWritable {
        /// The played bar (index on the path).
        played: u32,
    },
    /// A tempo outside the format's range (1 ..= 16 777 215 µs per quarter note) was clamped at
    /// this tick (E4).
    TempoClamped {
        /// The tick (from the start of the file).
        tick: u64,
    },
}

/// Why the export failed. On a valid document only the arithmetic (absurd lengths) and the size
/// limits of the format can fail.
///
/// The `Display` texts are English and meant for developers; the interface maps each variant to a
/// Fluent key (CLAUDE.md §7).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum MidiError {
    /// The path could not be read (a bug of the model rather than a panic, or absurd values).
    Path(UnrollError),
    /// The exact arithmetic overflowed.
    Arithmetic(RationalError),
    /// The music is too long for the format: a delta time above 2²⁸ − 1 ticks, or a track above
    /// 4 GiB.
    TooLong,
    /// More instruments than a file can hold tracks for (65 534).
    TooManyTracks,
    /// A General MIDI program above 127.
    InvalidProgram(u8),
    /// The file could not be written; the previous file, if any, is intact.
    Io {
        /// What went wrong.
        kind: io::ErrorKind,
        /// The file (or its temporary file, or its directory).
        path: PathBuf,
    },
}

impl fmt::Display for MidiError {
    /// A short English description, for developers.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Path(e) => write!(f, "cannot read the path: {e}"),
            Self::Arithmetic(e) => write!(f, "MIDI export arithmetic failed: {e}"),
            Self::TooLong => f.write_str("the music is too long for a MIDI file"),
            Self::TooManyTracks => f.write_str("too many instruments for a MIDI file"),
            Self::InvalidProgram(p) => write!(f, "invalid General MIDI program {p}"),
            Self::Io { kind, path } => write!(f, "cannot write {}: {kind}", path.display()),
        }
    }
}

impl Error for MidiError {
    /// The error of the path or of the arithmetic, if that is the cause.
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Path(e) => Some(e),
            Self::Arithmetic(e) => Some(e),
            _ => None,
        }
    }
}

impl From<UnrollError> for MidiError {
    /// Wraps an error of the path.
    fn from(e: UnrollError) -> Self {
        Self::Path(e)
    }
}

impl From<RationalError> for MidiError {
    /// Wraps an arithmetic error.
    fn from(e: RationalError) -> Self {
        Self::Arithmetic(e)
    }
}

impl From<smf::TooLong> for MidiError {
    /// A value the format cannot hold.
    fn from(_: smf::TooLong) -> Self {
        Self::TooLong
    }
}

// ------------------------------------------------------------------------------------------------
// The export
// ------------------------------------------------------------------------------------------------

/// The Standard MIDI File of the played notes of `doc` along `path` (see the module
/// documentation). `path` and `performance` must be those of `doc`, up to date.
///
/// # Errors
/// See [`MidiError`]: never for a musical reason (those are warnings).
///
/// # Complexity
/// See the module documentation.
pub fn export(
    doc: &Document,
    path: &Unrolled,
    performance: &Performance,
    options: &MidiOptions,
) -> Result<MidiExport, MidiError> {
    let mut warnings = Vec::new();
    // The lead-in (E5): the whole file is delayed by the smallest whole number of quarter notes
    // that covers the earliest note, when that note starts before position 0.
    let earliest = performance
        .instruments()
        .filter_map(|(_, notes)| notes.first().map(|n| n.start))
        .min()
        .unwrap_or(Position::ZERO);
    let quarters = if earliest < Position::ZERO {
        // ceil(−earliest × 4) quarter notes.
        earliest
            .as_rational()
            .checked_mul(Rational::from_integer(-4)?)?
            .ceil()
    } else {
        0
    };
    let origin = Position::from_rational(Rational::new(-quarters, 4)?);
    let lead = quarters.saturating_mul(i64::from(smf::TICKS_PER_QUARTER));
    if quarters > 0 {
        warnings.push(MidiWarning::LeadIn {
            quarters: u32::try_from(quarters).unwrap_or(u32::MAX),
        });
    }

    // The conductor track: tempo, time signatures, markers (E4, E5), in this order at one tick.
    let tempo = timing::TempoTrack::build(
        doc,
        path,
        performance
            .instruments()
            .flat_map(|(_, notes)| notes.iter()),
        origin,
        lead,
    )?;
    let signs = meters::signs(doc, path, lead)?;
    // (tick, order at that tick, event): sorted stably at the end.
    let mut conductor: Vec<(u64, u8, Message)> = Vec::new();
    for (tick, micros) in tempo.events() {
        conductor.push((tick, 0, smf::tempo(micros)));
    }
    if quarters > 0 {
        if let Some(meters::BarSign::Signature(n, e)) = meters::lead_in_sign(quarters) {
            conductor.push((0, 1, smf::time_signature(n, e)));
        }
        if let Some(text) = &options.lead_in_marker {
            conductor.push((0, 2, smf::marker(text)));
        }
    }
    for (tick, sign) in &signs.signs {
        match sign {
            meters::BarSign::Signature(n, e) => {
                conductor.push((*tick, 1, smf::time_signature(*n, *e)));
            }
            meters::BarSign::Marker(text) => conductor.push((*tick, 2, smf::marker(text))),
        }
    }
    conductor.sort_by_key(|(tick, order, _)| (*tick, *order));
    let conductor: Vec<Event> = conductor
        .into_iter()
        .map(|(tick, _, message)| Event { tick, message })
        .collect();

    // The instruments' tracks (E6–E10).
    let tracks = notes::tracks(doc, performance, &tempo, lead, options, &mut warnings)?;

    // The warnings of the bars and of the tempo, after those of the notes.
    warnings.extend(
        signs
            .unwritable
            .iter()
            .map(|&played| MidiWarning::MeterNotWritable { played }),
    );
    warnings.extend(
        tempo
            .clamped
            .iter()
            .map(|&tick| MidiWarning::TempoClamped { tick }),
    );

    // The bytes: the header, the conductor, one chunk per instrument.
    let count =
        u16::try_from(tracks.len().saturating_add(1)).map_err(|_| MidiError::TooManyTracks)?;
    let mut bytes = smf::header(count).to_vec();
    bytes.extend_from_slice(&smf::track_chunk(&conductor)?);
    for track in &tracks {
        bytes.extend_from_slice(&smf::track_chunk(track)?);
    }
    Ok(MidiExport { bytes, warnings })
}

/// Writes an export's bytes to `path` **safely**, as the project file is saved (ADR-0029 S13,
/// ADR-0032 E11): temporary file, sync, rename, sync of the directory. A failed save leaves the
/// previous file as it was.
///
/// # Errors
/// [`MidiError::Io`] with the failing path.
pub fn save(export: &MidiExport, path: &Path) -> Result<(), MidiError> {
    crate::safe_write::write(path, &export.bytes)
        .map_err(|(kind, path)| MidiError::Io { kind, path })
}
