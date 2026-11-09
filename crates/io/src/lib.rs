// SPDX-License-Identifier: GPL-3.0-or-later

//! # orchestra-io — files in and out
//!
//! ## Purpose
//! Everything that crosses the boundary between the model and the outside world as files or
//! streams: the **project file format**, MIDI, MusicXML.
//!
//! ## Responsibilities (planned)
//! - The native project file: **versioned from day one** with a chain of migrations, each tested
//!   against "golden" files of every previous version (ADR-0010, CLAUDE.md §9).
//! - Import/export of Standard MIDI Files and MusicXML, mapping their concepts onto the two-layer
//!   note model (written intent vs. performance) without losing information where possible.
//! - Robust handling of paths (`std::path`), line endings and encodings on every platform.
//!
//! ## What this crate must NOT do
//! - Change the model except through the same commands the views use (an import is a command, so
//!   it can be undone).
//!
//! ## Allowed dependencies
//! `orchestra-core-time`, `orchestra-core-model`, `orchestra-playback` (the played notes the MIDI
//! export writes, ADR-0032 E11).
//!
//! ## Unsafe code
//! `unsafe` is allowed here only for FFI or memory-mapped file access, should we ever need it;
//! every block carries a `// SAFETY:` comment. Pure-Rust parsing is strongly preferred.
//!
//! ## What exists so far
//! - [`project`]: the project file, version 1 (ADR-0029; SCALETTA F1.15b): [`project::save`],
//!   [`project::load`], [`project::to_bytes`], [`project::from_bytes`].
//! - [`midi`]: the export of the played notes as a Standard MIDI File 1.0 (ADR-0032; SCALETTA
//!   F1.18): [`midi::export`], [`midi::save`].
//! - `safe_write` (internal): the safe write shared by both (ADR-0029 S13).

pub mod midi;
mod platform;
pub mod project;
mod safe_write;

pub use project::{
    FORMAT_VERSION, LoadError, MAX_FILE_BYTES, MigrationError, SaveError, from_bytes, load, save,
    to_bytes,
};
