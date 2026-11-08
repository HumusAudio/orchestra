// SPDX-License-Identifier: GPL-3.0-or-later

//! Exports a score as a Standard MIDI File, **to listen to the model** with any player (ADR-0032
//! E11, E12; SCALETTA F1.18). A developer tool, not part of the program: the real command of
//! milestone M1 comes in F1.17.
//!
//! ```text
//! cargo run -p orchestra-io --example export_midi -- full normal out.mid
//! cargo run -p orchestra-io --example export_midi -- full heavy out.mid
//! cargo run -p orchestra-io --example export_midi -- plain normal out.mid
//! cargo run -p orchestra-io --example export_midi -- listening out.mid
//! ```
//!
//! - `full` / `plain`: a reference score of `orchestra-test-scores` (ADR-0031), each instrument
//!   given a General MIDI program from its (English) name, so that the orchestra does not all
//!   sound like a piano (E7).
//! - `listening`: the file for **tuning the attack profiles by ear** (E12, ADR-0027 §14): one bar
//!   per mark (none, stress, accent, marcato, *rf*, *sf*, *sffz*, *fp*, *sfp*, *pf*), each with a
//!   long note (a half) and a short one (an eighth) carrying the mark, on a string ensemble, first
//!   at ♩ = 60 and then the same bars at ♩ = 120.
//!
//! The warnings of the export are printed (English, for developers).

#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "a command-line developer tool: printing is its output"
)]

use std::error::Error;
use std::path::PathBuf;

use orchestra_core_model::{
    Alteration, Articulation, ArticulationSet, BaseValue, Clef, Document, DocumentBuilder,
    DynamicLevel, DynamicMark, DynamicPlacement, Interval, Key, Metronome, Mode, NoteValue, Pitch,
    Placement, ScorePoint, Step, TempoMark,
};
use orchestra_core_time::{Duration, Meter, MeterChange, MeterMap, Rational, Tempo};
use orchestra_io::midi::{self, MidiOptions};
use orchestra_playback::{Performance, Unrolled};
use orchestra_test_scores::Size;

#[path = "common/gm.rs"]
mod gm;

use gm::program_for;

/// `n/den` of a whole note.
fn d(n: i64, den: i64) -> Result<Duration, Box<dyn Error>> {
    Ok(Duration::from_rational(Rational::new(n, den)?))
}

/// The marks of the listening file, one bar each: an articulation or a dynamic accent.
#[derive(Debug, Clone)]
enum Mark {
    /// No mark: the reference.
    None,
    /// An articulation (stress, accent, marcato).
    Articulation(Articulation),
    /// A dynamic accent (*rf*, *sf*, *sffz*, *fp*, *sfp*, *pf*).
    Dynamic(DynamicMark),
}

/// The listening file (see the module documentation).
fn listening() -> Result<Document, Box<dyn Error>> {
    let marks = [
        Mark::None,
        Mark::Articulation(Articulation::Stress),
        Mark::Articulation(Articulation::Accent),
        Mark::Articulation(Articulation::Marcato),
        Mark::Dynamic(DynamicMark::Rf),
        Mark::Dynamic(DynamicMark::Sf),
        Mark::Dynamic(DynamicMark::Sffz),
        Mark::Dynamic(DynamicMark::Fp),
        Mark::Dynamic(DynamicMark::Sfp),
        Mark::Dynamic(DynamicMark::Pf),
    ];
    let slow = Tempo::new(d(1, 4)?, Rational::from_integer(60)?)?;
    let fast = Tempo::new(d(1, 4)?, Rational::from_integer(120)?)?;
    let meters = MeterMap::new(MeterChange::new(Meter::new(&[4], 4)?))?;
    let mut b = DocumentBuilder::new(
        meters,
        TempoMark::immediate(None, Metronome::shown(slow))?,
        Key::traditional(0, Mode::Major)?,
    )?;
    let track =
        b.add_instrument_track("Strings", Interval::new(0, Rational::ZERO), &[Clef::TREBLE])?;
    let (instrument, voice, staff) = {
        let i = b.document().track(track).ok_or("no track")?.instrument();
        (i.id(), i.voices()[0].id(), i.staves()[0].id())
    };
    let a4 = Pitch::new(Step::A, Alteration::NATURAL, 4);
    for pass in 0..2 {
        for (k, mark) in marks.iter().enumerate() {
            let bar = b.add_bar()?;
            if pass == 1 && k == 0 {
                b.add_tempo_mark(
                    ScorePoint::new(bar, Duration::ZERO),
                    TempoMark::immediate(None, Metronome::shown(fast))?,
                )?;
            }
            // A long note at the bar's start, a short one on beat 4; both carry the mark.
            for (offset, value) in [(d(0, 1)?, BaseValue::Half), (d(3, 4)?, BaseValue::Eighth)] {
                let event = b
                    .add_chord(
                        Placement::new(voice, bar, offset),
                        NoteValue::plain(value),
                        &[a4],
                    )?
                    .event;
                match mark {
                    Mark::None => {}
                    Mark::Articulation(a) => {
                        b.set_articulations(event, ArticulationSet::EMPTY.with(*a))?;
                    }
                    Mark::Dynamic(m) => {
                        b.add_dynamic(
                            instrument,
                            ScorePoint::new(bar, offset),
                            DynamicPlacement::instrument(staff),
                            m.clone(),
                        )?;
                    }
                }
            }
            // Back to mf before the next bar, with no note at the point (*fp* and *pf* leave
            // another level in force, ADR-0027 H4).
            b.add_dynamic(
                instrument,
                ScorePoint::new(bar, d(7, 8)?),
                DynamicPlacement::instrument(staff),
                DynamicMark::Level(DynamicLevel::Mf),
            )?;
        }
    }
    Ok(b.build()?)
}

/// Parses the arguments, builds the score, exports and saves it, prints the warnings.
fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let usage =
        "usage: export_midi (full|plain) (normal|heavy) OUT.mid | export_midi listening OUT.mid";
    let (doc, out) = match args.as_slice() {
        [preset, size, out] if preset == "full" || preset == "plain" => {
            let size = match size.as_str() {
                "normal" => Size::Normal,
                "heavy" => Size::Heavy,
                _ => return Err(usage.into()),
            };
            let doc = if preset == "full" {
                orchestra_test_scores::full(size)
            } else {
                orchestra_test_scores::plain(size, false)
            };
            (doc, PathBuf::from(out))
        }
        [preset, out] if preset == "listening" => (listening()?, PathBuf::from(out)),
        _ => return Err(usage.into()),
    };
    let path = Unrolled::new(&doc)?;
    let performance = Performance::new(&doc, &path)?;
    let mut options = MidiOptions::new();
    options.set_lead_in_marker(Some("lead-in".to_owned()));
    for (id, _) in performance.instruments() {
        let name = doc.instrument(id).map_or("", |i| i.name());
        options.set_program(id, program_for(name))?;
    }
    let export = midi::export(&doc, &path, &performance, &options)?;
    midi::save(&export, &out)?;
    let notes: usize = performance.instruments().map(|(_, n)| n.len()).sum();
    println!(
        "{}: {} bytes, {} instruments, {} played notes, {} warnings",
        out.display(),
        export.bytes.len(),
        performance.instruments().count(),
        notes,
        export.warnings.len()
    );
    for warning in &export.warnings {
        eprintln!("warning: {warning:?}");
    }
    Ok(())
}
