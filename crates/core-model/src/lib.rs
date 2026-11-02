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

pub mod document;
pub mod id;
pub mod pitch;

pub use document::{
    BarList, Document, DocumentBuilder, DocumentError, Instrument, MAX_STAVES_PER_INSTRUMENT,
    MAX_VOICES_PER_INSTRUMENT, Staff, Track, Voice,
};
pub use id::{BarId, InstrumentId, StaffId, TrackId, VoiceId};
pub use pitch::{
    Alteration, Interval, Key, MidiNote, Mode, Pitch, PitchError, Quality, Step, TraditionalKey,
};
