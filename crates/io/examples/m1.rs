// SPDX-License-Identifier: GPL-3.0-or-later

//! The **test command of milestone M1** (SCALETTA §7, F1.17): "the musical engine without an
//! interface". A developer tool, not part of the program.
//!
//! ```text
//! cargo run -p orchestra-io --example m1 -- OUT_DIR
//! cargo run -p orchestra-io --example m1 -- OUT_DIR --heavy --notes
//! ```
//!
//! What it does, in the order of the milestone:
//! 1. **creates** the reference score: the "full" preset of `orchestra-test-scores` (ADR-0031),
//!    with repeats, voltas, jumps, gradual tempo changes, dynamics and a real orchestra; the
//!    normal size, or the heavy one with `--heavy`;
//! 2. **saves** it as `OUT_DIR/m1.orchestra` (the project file of ADR-0029, safe write);
//! 3. **reopens** it and checks that it is **identical**: the same music (`same_content`), the
//!    same description (IDs and counters included, `to_parts`) and the same bytes when saved
//!    again; any difference stops the command with an error;
//! 4. **prints the unrolled performance** of the reopened document (ADR-0025, ADR-0027): every
//!    played bar in playing order with its written bar, pass, play of its section and time, then
//!    the played notes per instrument; every note too with `--notes`;
//! 5. **exports** the performance as `OUT_DIR/m1.mid` (ADR-0032), each instrument with a General
//!    MIDI program from its name, to be listened to with any player.
//!
//! Output in English (for developers, like every message of the examples); the warnings of the
//! MIDI export go to standard error.

#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "a command-line developer tool: printing is its output"
)]

#[path = "common/gm.rs"]
mod gm;

use std::error::Error;
use std::path::PathBuf;

use gm::program_for;
use orchestra_core_model::Document;
use orchestra_io::midi::{self, MidiOptions};
use orchestra_io::{load, save, to_bytes};
use orchestra_playback::{Performance, Unrolled};
use orchestra_test_scores::{Size, full};

/// The command line: the output directory and the two options.
struct Args {
    /// Where the project file and the MIDI file are written (created if missing).
    out: PathBuf,
    /// `--heavy`: the heavy full score instead of the normal one.
    heavy: bool,
    /// `--notes`: print every played note, not only the counts.
    notes: bool,
}

/// The usage line, printed on a wrong command line.
const USAGE: &str = "usage: m1 OUT_DIR [--heavy] [--notes]";

/// Parses the arguments: one directory and the options, in any order. O(arguments).
///
/// # Errors
/// The usage line, for an unknown option or not exactly one directory.
fn parse() -> Result<Args, Box<dyn Error>> {
    let mut out = None;
    let (mut heavy, mut notes) = (false, false);
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--heavy" => heavy = true,
            "--notes" => notes = true,
            // Anything else starting with a dash is a mistyped option, not a directory.
            _ if arg.starts_with('-') => return Err(USAGE.into()),
            // A second directory is a mistake too.
            _ if out.is_some() => return Err(USAGE.into()),
            _ => out = Some(PathBuf::from(arg)),
        }
    }
    Ok(Args {
        out: out.ok_or(USAGE)?,
        heavy,
        notes,
    })
}

/// Steps 2 and 3: saves `doc` to `path`, loads it back and checks that nothing changed, the same
/// way the round-trip tests of `crates/io/tests/project_cases.rs` do. Returns the reopened
/// document (the rest of the command works on it, so that it really is the file that plays) and
/// the size of the file in bytes. O(size of the document).
///
/// # Errors
/// The save and load errors; a message naming the first check that failed.
fn round_trip(doc: &Document, path: &std::path::Path) -> Result<(Document, usize), Box<dyn Error>> {
    save(doc, path)?;
    let back = load(path)?;
    if !back.same_content(doc) {
        return Err("the reopened document holds different music".into());
    }
    if back.to_parts() != doc.to_parts() {
        return Err("the reopened document has a different description (IDs, counters)".into());
    }
    let bytes = to_bytes(doc)?;
    if to_bytes(&back)? != bytes {
        return Err("saving the reopened document gives different bytes".into());
    }
    Ok((back, bytes.len()))
}

/// Step 4, the bars: one line per played bar, in playing order. The bar numbers are 1-based, as
/// a musician counts them; the time is the start of the bar in seconds from the start of the
/// performance (approximate inside a gradual tempo change, followed by `~`). O(played bars ×
/// log t) for t tempo changes.
///
/// # Errors
/// The errors of the path's time (never on a path built from this document).
fn print_bars(doc: &Document, path: &Unrolled) -> Result<(), Box<dyn Error>> {
    println!("Unrolled path: {} played bars", path.played().len());
    println!("  played  written  pass  section play  start (s)");
    for (k, bar) in path.played().iter().enumerate() {
        let time = path.time_at(doc, k, orchestra_core_time::Duration::ZERO)?;
        let mark = match time {
            orchestra_core_time::TimeAt::Exact(_) => ' ',
            orchestra_core_time::TimeAt::Approximate(_) => '~',
        };
        let section = bar
            .section_play
            .map_or_else(|| "-".to_owned(), |p| p.to_string());
        println!(
            "  {:>6}  {:>7}  {:>4}  {:>12}  {:>9.3}{mark}",
            k + 1,
            bar.index.get() + 1,
            bar.pass + 1,
            section,
            time.to_f64()
        );
    }
    Ok(())
}

/// Step 4, the notes: the count per instrument, in score order, and with `all` every note: its
/// played bar, its start and end on the unrolled timeline (whole notes, exact), its time in
/// seconds, its sounding pitch (semitones, C4 = 60, a fraction for microtones) and its attack on the
/// 7-bit scale. O(notes).
///
/// # Errors
/// The errors of the path's time (never on a performance built from this path).
fn print_notes(
    doc: &Document,
    path: &Unrolled,
    performance: &Performance,
    all: bool,
) -> Result<(), Box<dyn Error>> {
    let total: usize = performance.instruments().map(|(_, n)| n.len()).sum();
    println!("Performance: {total} played notes");
    for (id, notes) in performance.instruments() {
        let name = doc.instrument(id).map_or("?", |i| i.name());
        println!("  {name}: {} notes", notes.len());
        if !all {
            continue;
        }
        for note in notes {
            // The time of the note: its offset from the start of its played bar, read on the
            // path. The offset can be negative (an acciaccatura sounds before its bar's
            // barline, ADR-0027 N5): `Duration` is signed and the path takes it as it is.
            let index = usize::try_from(note.played)?;
            let bar = path.played().get(index).ok_or("a note outside the path")?;
            let offset = note.start.checked_duration_since(bar.start)?;
            let time = path.time_at(doc, index, offset)?;
            println!(
                "    bar {:>4}  {:?} .. {:?}  {:>9.3} s  pitch {:?}  velocity {}",
                index + 1,
                note.start,
                note.end,
                time.to_f64(),
                note.pitch,
                note.velocity >> 9
            );
        }
    }
    Ok(())
}

/// Runs the five steps of the module documentation.
fn main() -> Result<(), Box<dyn Error>> {
    let args = parse()?;
    std::fs::create_dir_all(&args.out)?;
    let size = if args.heavy {
        Size::Heavy
    } else {
        Size::Normal
    };

    // 1. Create.
    let doc = full(size);
    println!(
        "Created the {} full score: {} bars, {} instruments",
        if args.heavy { "heavy" } else { "normal" },
        doc.bars().len(),
        doc.tracks().len()
    );

    // 2–3. Save, reopen, compare.
    let project = args.out.join("m1.orchestra");
    let (doc, bytes) = round_trip(&doc, &project)?;
    println!(
        "Saved {} ({bytes} bytes) and reopened it: identical (music, description, bytes)",
        project.display()
    );

    // 4. The unrolled performance of the reopened document.
    let path = Unrolled::new(&doc)?;
    let performance = Performance::new(&doc, &path)?;
    print_bars(&doc, &path)?;
    print_notes(&doc, &path, &performance, args.notes)?;

    // 5. The MIDI file.
    let mut options = MidiOptions::new();
    options.set_lead_in_marker(Some("lead-in".to_owned()));
    for (id, _) in performance.instruments() {
        let name = doc.instrument(id).map_or("", |i| i.name());
        options.set_program(id, program_for(name))?;
    }
    let export = midi::export(&doc, &path, &performance, &options)?;
    let file = args.out.join("m1.mid");
    midi::save(&export, &file)?;
    println!(
        "Exported {} ({} bytes, {} warnings): listen to it with any player",
        file.display(),
        export.bytes.len(),
        export.warnings.len()
    );
    for warning in &export.warnings {
        eprintln!("warning: {warning:?}");
    }
    Ok(())
}
