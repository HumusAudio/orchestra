// SPDX-License-Identifier: GPL-3.0-or-later

//! # orchestra-core-model — the single source of truth
//!
//! ## Purpose
//! Hold **the one and only project document** (ADR-0003). The notation view, the DAW view and the
//! video view are projections of these data: they read them and change them only through commands.
//! There are no parallel copies to keep "in sync".
//!
//! ## Responsibilities (planned for Phase 1)
//! - Project structure: tracks (instrument, audio, video), staves, voices, measures, the form of
//!   the piece (repeats, voltas, jumps such as D.S. al Coda).
//! - Notes with their **two layers** (ADR-0004): the *written intent* (pitch with its spelling,
//!   rational duration, voice, articulations, dynamics) and the explicit *performance offsets*
//!   (microtiming, velocity, length changes) applied on top of what `orchestra-playback` derives.
//! - The **command** system: every change, from any view, is a reversible command; one unified
//!   undo/redo history for the whole document.
//! - Stable identifiers for every element, so that views, caches and the unrolled timeline can
//!   refer to model elements without holding copies of them.
//! - Change notifications precise enough to drive *incremental* layout and rendering (which notes,
//!   which measures changed), so no consumer ever has to recompute the whole score.
//!
//! ## Implemented so far
//! - [`pitch`] (SCALETTA F1.07, ADR-0023): [`Pitch`] with its spelling (step, rational alteration
//!   in semitones, scientific octave), the exact sounding pitch and the [`MidiNote`] it maps to,
//!   [`Interval`] with its derived [`Quality`], traditional key signatures ([`Key`],
//!   [`TraditionalKey`], [`Mode`]), the spelling of a MIDI key in a key, and transposition.
//! - [`id`] and [`document`] (SCALETTA F1.08, ADR-0022 §1–3): the typed IDs from one counter per
//!   document, and the skeleton of the [`Document`] — tracks, one [`Instrument`] per track with its
//!   [`Staff`]s and [`Voice`]s (voices belong to the instrument and have a home staff), the
//!   [`BarList`] of [`BarId`]s next to the shared meter and tempo maps — created with a
//!   [`DocumentBuilder`].
//! - [`event`], [`content`], [`marks`] and [`validate`] (SCALETTA F1.09b, ADR-0024 §1–§4 and §9):
//!   the events of the voices ([`Chord`]s of [`Note`]s and [`Rest`]s, at an offset in their bar),
//!   their written [`NoteValue`]s with the metric duration derived, [`Tuplet`]s, [`Tie`]s, the
//!   [`Clef`]s of each staff and the key signatures of the document with the instruments'
//!   overrides, and [`Document::validate`], which [`DocumentBuilder::build`] runs.
//! - [`marking`] and [`slur`] (SCALETTA F1.09c, ADR-0024 §5–§7 and §10): grace chords
//!   ([`Grace`]), [`ArticulationSet`]s, [`Ornament`]s, [`Fermata`]s and [`Arpeggio`]s on the
//!   events, the [`PerformanceOffsets`] of every notehead, the [`Slur`]s of each instrument with
//!   the derived index of the slurs at an event, and the score order of
//!   [`Document::comes_before`] that ties and slurs follow.
//! - [`dynamic`] and [`tempo_mark`] (SCALETTA F1.09c, ADR-0024 §8 and §11): the [`Dynamic`]s and
//!   [`Hairpin`]s of each instrument, with their [`DynamicScope`] (the whole instrument or one
//!   voice), and the [`TempoMark`]s of the document, from which the tempo map is derived.
//! - [`form`] (SCALETTA F1.10b, ADR-0025 §2–§3): the [`Form`] of the document — [`RepeatStart`]s
//!   and [`RepeatEnd`]s, [`Volta`]s with their [`PassSet`]s — and the [`BarNumbering`] style with
//!   the [`BarLabel`] of every bar.
//! - [`jump`] and [`bar_repeat`] (SCALETTA F1.10b, ADR-0025 §4, §6–§7): [`Marker`]s and
//!   [`Jump`]s ("go to / play until / continue at" by ID), the [`BarRepeat`]s (%) of each staff
//!   and the [`PassRule`]s of each instrument.
//! - [`edit`], [`change`] and [`editor`] (SCALETTA F1.11b part 1, ADR-0026 §1–§5): the primitive
//!   [`Edit`]s that return their inverse, the [`ChangeSet`] every change reports, and the
//!   [`Editor`] with its transactions and its one undo history bounded by memory.
//! - [`command`] (SCALETTA F1.11b part 2a, ADR-0026 §1, §4, §6): the [`Command`]s on events and
//!   bars (enter, delete and change events; insert and delete bars), the **gestures** of the
//!   editor (a drag as one step of undo), and the local check of the musical rules on the cells a
//!   command touched.
//! - [`mark_command`] (SCALETTA F1.11b part 2b, ADR-0026 §9 and §11 X1–X10): the commands on
//!   marks and form markings (added from a [`MarkSpec`], changed keeping their ID, removed with
//!   the cascade of the form), on tuplets, on the initial values and on the per-bar data that
//!   does not move a barline.
//! - [`meter_command`] (SCALETTA F1.11c, ADR-0026 §7 and §11 Y1–Y6): the changes of bar lengths,
//!   meter changes and irregular, pickup and free bars, in a [`MeterMode`] (`KeepBars` for now).
//! - [`tolerance`], [`pass_offsets`] and [`daw_command`] (SCALETTA F1.14b, ADR-0028): the
//!   [`Tolerance`] threshold of the project and of each track, the [`PassOffsets`] of each
//!   instrument (performance offsets of one pass of a repeat), and the DAW move
//!   ([`MovePlayed`]) that keeps a small move as an offset and turns a larger one into a written
//!   move, unless the user consciously chose [`MoveMode::PerformanceOnly`].
//!
//! ## What this crate must NOT do
//! - Lay out notation, render anything, play audio or touch files: those are separate crates that
//!   depend on this one, never the other way around.
//! - Contain view-specific state (scroll position, zoom, selection colours…): that belongs to `ui`.
//! - Duplicate data for the convenience of one view: derive it, cache it outside, or add it here
//!   once for everybody.
//!
//! ## Allowed dependencies
//! `orchestra-core-time` only.

#![forbid(unsafe_code)]
// Stricter than the workspace (owner's decision, 2026-10-30, SCALETTA F0.16): the document is the
// single source of truth (ADR-0003) and must never panic on a bad index — `v[i]` panics when `i` is
// out of range, so use `get` and handle `None`, or iterate. Tests are exempt through clippy.toml.
#![deny(clippy::indexing_slicing)]

pub mod bar_repeat;
pub mod change;
pub mod command;
#[cfg(test)]
mod command_tests;
pub mod content;
pub mod daw_command;
#[cfg(test)]
mod daw_command_tests;
pub mod document;
pub mod dynamic;
pub mod edit;
pub mod editor;
#[cfg(test)]
mod editor_tests;
pub mod event;
pub mod form;
pub mod id;
pub mod jump;
pub mod mark_command;
#[cfg(test)]
mod mark_command_tests;
pub mod marking;
pub mod marks;
pub mod meter_command;
#[cfg(test)]
mod meter_command_tests;
pub mod parts;
#[cfg(test)]
mod parts_tests;
pub mod pass_offsets;
pub mod pitch;
mod rewrite;
pub mod slur;
pub mod tempo_mark;
pub mod tolerance;
pub mod validate;

pub use bar_repeat::{BarRepeat, BarRepeatSpan, PassRule};
pub use change::ChangeSet;
pub use command::{Command, EventChange, NewBody, NewEvent};
pub use content::Chunk;
pub use daw_command::{MoveMode, MovePlayed, ResizePlayed};
pub use document::{
    BarList, ChordIds, Document, DocumentBuilder, DocumentError, EventRef, Instrument,
    MAX_STAVES_PER_INSTRUMENT, MAX_TRACKS, MAX_VOICES_PER_INSTRUMENT, Placement, Staff, Track,
    Voice,
};
pub use dynamic::{
    Dynamic, DynamicLevel, DynamicMark, DynamicPlacement, DynamicScope, Hairpin, HairpinKind,
    HairpinShape, HairpinStyle, OtherDynamic,
};
pub use edit::{Edit, Initial, MeterEdit, Object, ObjectRef};
pub use editor::{CommandError, DEFAULT_HISTORY_LIMIT, Editor, Transaction};
pub use event::{
    BaseValue, Chord, Event, EventBody, MAX_DOTS, MAX_TUPLET_DEPTH, Note, NoteValue,
    NoteValueError, PlacedEvent, Rest, RestValue, Tie, Tuplet, TupletBracket, TupletDisplay,
    TupletNumber,
};
pub use form::{BarLabel, BarNumbering, Form, MAX_PLAYS, PassSet, RepeatEnd, RepeatStart, Volta};
pub use id::{
    BarId, BarRepeatId, ClefId, DynamicId, EventId, HairpinId, InstrumentId, JumpId, KeyChangeId,
    MarkerId, NoteId, PassRuleId, RepeatId, SlurId, StaffId, TempoMarkId, TrackId, TupletId,
    VoiceId, VoltaId,
};
pub use jump::{Jump, JumpFires, JumpSpec, JumpTarget, Marker, MarkerKind, RepeatsAfterJump};
pub use mark_command::{MarkSpec, NewVolta};
pub use marking::{
    Arpeggio, Articulation, ArticulationSet, Fermata, Grace, GraceAttach, GraceKind, Ornament,
    OrnamentError, PerformanceOffsets, TremoloStrokes,
};
pub use marks::{Clef, ClefSign, KeyOverride, KeySignature, Mark, MarkList, ScorePoint};
pub use meter_command::MeterMode;
pub use parts::{CellParts, DocumentParts, StaffParts, TrackParts, VoiceParts};
pub use pass_offsets::{PassOffset, PassOffsets};
pub use pitch::{
    Alteration, Interval, Key, MidiNote, Mode, Pitch, PitchError, Quality, Step, TraditionalKey,
};
pub use slur::{Slur, SlurPlacement};
pub use tempo_mark::{GradualMark, Metronome, TempoMark, TempoMarkChange};
pub use tolerance::{Tolerance, ToleranceSettings};
pub use validate::Violation;
