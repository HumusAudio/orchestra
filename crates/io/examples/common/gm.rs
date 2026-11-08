// SPDX-License-Identifier: GPL-3.0-or-later

//! General MIDI programs for the instrument names of the reference scores, shared by the examples
//! `export_midi` and `m1` (included with `#[path]`: Cargo builds every file directly in
//! `examples/` as an example of its own, never the files of a subdirectory without a `main.rs`).
//! Only for these developer tools: the program will get real sound settings in F3 (ADR-0032 E7).

/// The General MIDI program (0-based) for an instrument name of the presets: the first prefix
/// that matches. A list and not a map: sixteen entries, read once per instrument.
const PROGRAMS: [(&str, u8); 16] = [
    ("Piano", 0),
    ("Harp", 46),
    ("Timpani", 47),
    ("Piccolo", 72),
    ("Flute", 73),
    ("Oboe", 68),
    ("Clarinet", 71),
    ("Bassoon", 70),
    ("Horn", 60),
    ("Trumpet", 56),
    ("Trombone", 57),
    ("Tuba", 58),
    ("Violin", 40),
    ("Viola", 41),
    ("Violoncello", 42),
    ("Contrabass", 43),
];

/// The string ensemble program: for the listening file and for any name not in [`PROGRAMS`].
const STRINGS: u8 = 48;

/// The program for an instrument name (see [`PROGRAMS`]); [`STRINGS`] for an unknown one. O(1)
/// (sixteen prefixes at most).
pub(crate) fn program_for(name: &str) -> u8 {
    PROGRAMS
        .iter()
        .find(|(prefix, _)| name.starts_with(prefix))
        .map_or(STRINGS, |(_, p)| *p)
}
