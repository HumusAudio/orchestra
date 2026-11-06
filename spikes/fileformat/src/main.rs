// SPDX-License-Identifier: GPL-3.0-or-later

//! Spike F1.15: how big and how fast are the candidate encodings of a project file?
//!
//! The scores are the real normal and heavy scores of `core-model`'s tests (the generator is
//! included from `crates/core-model/tests/common/score.rs`), read through the model's public API
//! into a **file schema** of our own (plain structs with serde derives, separate from the model's
//! memory layout: this is also the shape proposed by the ADR). Only the voice contents are
//! encoded: they are well over 99% of a score's data (a few hundred marks against 8 690 or 255 000
//! noteheads), so the numbers are those of the whole file within a few percent.
//!
//! Two schemas:
//! - **verbose**: structured objects for pitches and note values (`{"step":"C","alter":"0",...}`);
//! - **compact**: short strings (`"C4"`, `"q"`, `"3/16"`), as a hand-designed format would use.
//!
//! For each encoding: size, size after deflate (what a zip container would store), native time to
//! write, to read back into the typed schema, and to read through a generic tree first (the path a
//! file of an older version takes when its migrations run on the tree). Times are the best of five
//! runs of a release build, on the development machine (not the reference laptop).

use std::time::Instant;

use orchestra_core_model::{BarId, Document, EventBody, RestValue};
use serde::{Deserialize, Serialize};

#[path = "../../../crates/core-model/tests/common/score.rs"]
mod score;

/// A rational as the text `n/d`, or `n` for an integer: exact and readable.
fn rational(r: orchestra_core_time::Rational) -> String {
    if r.denominator() == 1 {
        r.numerator().to_string()
    } else {
        format!("{}/{}", r.numerator(), r.denominator())
    }
}

// ---- verbose schema ---------------------------------------------------------------------------

/// The voice contents of a score, one entry per non-empty (voice, bar) cell.
#[derive(Serialize, Deserialize)]
struct VFile {
    /// The format version, first so that it can be read without parsing the rest.
    format: u32,
    /// The cells.
    cells: Vec<VCell>,
}

/// The events of one voice in one bar.
#[derive(Serialize, Deserialize)]
struct VCell {
    /// The voice ID.
    voice: u64,
    /// The bar ID.
    bar: u64,
    /// The events in chunk order.
    events: Vec<VEvent>,
}

/// One event.
#[derive(Serialize, Deserialize)]
struct VEvent {
    /// The event ID.
    id: u64,
    /// The offset from the barline, as a rational text.
    at: String,
    /// The staff ID.
    staff: u64,
    /// Chord or rest.
    body: VBody,
}

/// Chord or rest.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum VBody {
    /// A chord.
    Chord {
        /// Its value.
        value: VValue,
        /// Its noteheads.
        notes: Vec<VNote>,
    },
    /// A rest.
    Rest {
        /// Its value.
        value: VValue,
    },
}

/// A note value.
#[derive(Serialize, Deserialize)]
struct VValue {
    /// The base value's name.
    base: String,
    /// The dots, omitted when zero.
    // No `skip_serializing_if`: postcard is not self-describing and cannot read a skipped field
    // back (it failed with DeserializeBadUtf8), so every field is always written.
    #[serde(default)]
    dots: u8,
}

/// Whether a dot count is zero (serde helper).
#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_zero(n: &u8) -> bool {
    *n == 0
}

/// A notehead.
#[derive(Serialize, Deserialize)]
struct VNote {
    /// The notehead ID.
    id: u64,
    /// The spelled pitch.
    pitch: VPitch,
}

/// A spelled pitch.
#[derive(Serialize, Deserialize)]
struct VPitch {
    /// The letter.
    step: String,
    /// The alteration in semitones, as a rational text.
    alter: String,
    /// The scientific octave.
    octave: i8,
}

// ---- compact schema ---------------------------------------------------------------------------

/// Same content, short strings.
#[derive(Serialize, Deserialize)]
struct CFile {
    /// The format version.
    format: u32,
    /// The cells.
    cells: Vec<CCell>,
}

/// One cell.
#[derive(Serialize, Deserialize)]
struct CCell {
    /// The voice ID.
    v: u64,
    /// The bar ID.
    b: u64,
    /// The events.
    e: Vec<CEvent>,
}

/// One event: chords have `n`, rests do not.
#[derive(Serialize, Deserialize)]
struct CEvent {
    /// The event ID.
    id: u64,
    /// The offset.
    at: String,
    /// The staff.
    s: u64,
    /// The value as a short code (`q`, `e.`, ...).
    d: String,
    /// The noteheads as `[id, "C4"]` pairs; absent for a rest.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    n: Option<Vec<(u64, String)>>,
}

// ---- conversion -------------------------------------------------------------------------------

/// The verbose and compact schemas of a document's voice contents.
fn schemas(doc: &Document) -> (VFile, CFile) {
    let bars: Vec<BarId> = doc.bars().ids().to_vec();
    let mut v = VFile {
        format: 1,
        cells: Vec::new(),
    };
    let mut c = CFile {
        format: 1,
        cells: Vec::new(),
    };
    for track in doc.tracks() {
        for voice in track.instrument().voices() {
            for &bar in &bars {
                let events = doc.events(voice.id(), bar).unwrap();
                if events.is_empty() {
                    continue;
                }
                let mut ve = Vec::new();
                let mut ce = Vec::new();
                for placed in events {
                    let ev = placed.event();
                    let at = rational(placed.offset().as_rational());
                    let (value, notes) = match ev.body() {
                        EventBody::Chord(ch) => (ch.value(), Some(ch.notes().collect::<Vec<_>>())),
                        EventBody::Rest(r) => match r.value() {
                            RestValue::Value(value) => (value, None),
                            other => panic!("unexpected rest {other:?}"),
                        },
                    };
                    let vv = VValue {
                        base: format!("{:?}", value.base()),
                        dots: value.dots(),
                    };
                    let code = format!(
                        "{}{}",
                        match format!("{:?}", value.base()).as_str() {
                            "Whole" => "w",
                            "Half" => "h",
                            "Quarter" => "q",
                            "Eighth" => "e",
                            "Sixteenth" => "s",
                            other => panic!("unexpected value {other}"),
                        },
                        ".".repeat(usize::from(value.dots()))
                    );
                    let body = match &notes {
                        Some(ns) => VBody::Chord {
                            value: vv,
                            notes: ns
                                .iter()
                                .map(|n| VNote {
                                    id: n.id().get(),
                                    pitch: VPitch {
                                        step: format!("{:?}", n.pitch().step()),
                                        alter: rational(n.pitch().alteration().semitones()),
                                        octave: n.pitch().octave(),
                                    },
                                })
                                .collect(),
                        },
                        None => VBody::Rest { value: vv },
                    };
                    ve.push(VEvent {
                        id: ev.id().get(),
                        at: at.clone(),
                        staff: ev.staff().get(),
                        body,
                    });
                    ce.push(CEvent {
                        id: ev.id().get(),
                        at,
                        s: ev.staff().get(),
                        d: code,
                        n: notes.map(|ns| {
                            ns.iter()
                                .map(|n| {
                                    (
                                        n.id().get(),
                                        format!("{:?}{}", n.pitch().step(), n.pitch().octave()),
                                    )
                                })
                                .collect()
                        }),
                    });
                }
                v.cells.push(VCell {
                    voice: voice.id().get(),
                    bar: bar.get(),
                    events: ve,
                });
                c.cells.push(CCell {
                    v: voice.id().get(),
                    b: bar.get(),
                    e: ce,
                });
            }
        }
    }
    (v, c)
}

// ---- measurement ------------------------------------------------------------------------------

/// The best of five runs of `f`, in milliseconds.
fn best<T>(mut f: impl FnMut() -> T) -> f64 {
    (0..5)
        .map(|_| {
            let t = Instant::now();
            std::hint::black_box(f());
            t.elapsed().as_secs_f64() * 1000.0
        })
        .fold(f64::INFINITY, f64::min)
}

/// The size of `bytes` after deflate at the default level, as a zip entry would store them.
fn deflated(bytes: &[u8]) -> usize {
    use std::io::Write;
    let mut e = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
    e.write_all(bytes).unwrap();
    e.finish().unwrap().len()
}

/// One table row.
fn row(name: &str, bytes: &[u8], write: f64, read: f64, via_tree: Option<f64>) {
    let t = Instant::now();
    let z = deflated(bytes);
    let zt = t.elapsed().as_secs_f64() * 1000.0;
    println!(
        "| {name:<22} | {:>10.2} MB | {:>8.2} MB | {:>7.1} | {:>8.1} | {:>8.1} | {:>8} |",
        bytes.len() as f64 / 1e6,
        z as f64 / 1e6,
        zt,
        write,
        read,
        via_tree.map_or("—".to_string(), |t| format!("{t:.1}")),
    );
}

/// Deflate at `level`, then inflate: (deflated size in MB, deflate ms, inflate ms).
fn level(bytes: &[u8], level: u32) -> (f64, f64, f64) {
    use std::io::{Read, Write};
    let pack = || {
        let mut e = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::new(level));
        e.write_all(bytes).unwrap();
        e.finish().unwrap()
    };
    let z = pack();
    let w = best(pack);
    let r = best(|| {
        let mut out = Vec::with_capacity(bytes.len());
        flate2::read::DeflateDecoder::new(&z[..]).read_to_end(&mut out).unwrap();
        out
    });
    (z.len() as f64 / 1e6, w, r)
}

fn main() {
    for (label, size) in [
        ("normal", score::Size::Normal),
        ("heavy", score::Size::Heavy),
    ] {
        let build = best(|| score::score(size, false));
        let doc = score::score(size, false);
        let (v, c) = schemas(&doc);
        let events: usize = v.cells.iter().map(|c| c.events.len()).sum();
        let notes: usize = v
            .cells
            .iter()
            .flat_map(|c| &c.events)
            .map(|e| match &e.body {
                VBody::Chord { notes, .. } => notes.len(),
                VBody::Rest { .. } => 0,
            })
            .sum();
        println!(
            "\n### {label}: {} cells, {events} events, {notes} noteheads; building the document \
             with the real builder: {build:.1} ms\n",
            v.cells.len()
        );
        println!(
            "| encoding               |       size    | deflated    | deflate ms | write ms | read ms  | via tree |"
        );
        println!("|---|---|---|---|---|---|---|");

        let j = serde_json::to_vec(&v).unwrap();
        row(
            "JSON verbose",
            &j,
            best(|| serde_json::to_vec(&v).unwrap()),
            best(|| serde_json::from_slice::<VFile>(&j).unwrap()),
            Some(best(|| {
                let t: serde_json::Value = serde_json::from_slice(&j).unwrap();
                serde_json::from_value::<VFile>(t).unwrap()
            })),
        );
        let jp = serde_json::to_vec_pretty(&v).unwrap();
        row(
            "JSON verbose pretty",
            &jp,
            best(|| serde_json::to_vec_pretty(&v).unwrap()),
            best(|| serde_json::from_slice::<VFile>(&jp).unwrap()),
            None,
        );
        let jc = serde_json::to_vec(&c).unwrap();
        row(
            "JSON compact",
            &jc,
            best(|| serde_json::to_vec(&c).unwrap()),
            best(|| serde_json::from_slice::<CFile>(&jc).unwrap()),
            Some(best(|| {
                let t: serde_json::Value = serde_json::from_slice(&jc).unwrap();
                serde_json::from_value::<CFile>(t).unwrap()
            })),
        );
        let r = ron::to_string(&v).unwrap().into_bytes();
        row(
            "RON verbose",
            &r,
            best(|| ron::to_string(&v).unwrap()),
            best(|| ron::de::from_bytes::<VFile>(&r).unwrap()),
            // ron::Value loses enum variants (it reads them back as maps), so a migration
            // on RON's generic tree cannot round-trip our schema: measured as not possible.
            None,
        );
        let rp = ron::ser::to_string_pretty(&v, ron::ser::PrettyConfig::default())
            .unwrap()
            .into_bytes();
        row(
            "RON verbose pretty",
            &rp,
            best(|| ron::ser::to_string_pretty(&v, ron::ser::PrettyConfig::default()).unwrap()),
            best(|| ron::de::from_bytes::<VFile>(&rp).unwrap()),
            None,
        );
        let p = postcard::to_stdvec(&v).unwrap();
        row(
            "postcard (binary)",
            &p,
            best(|| postcard::to_stdvec(&v).unwrap()),
            best(|| postcard::from_bytes::<VFile>(&p).unwrap()),
            None,
        );
        for (name, bytes) in [("JSON verbose", &j), ("JSON compact", &jc)] {
            for l in [1, 6] {
                let (mb, w, r) = level(bytes, l);
                println!("deflate level {l} of {name}: {mb:.2} MB, {w:.1} ms to pack, {r:.1} ms to unpack");
            }
        }
        if label == "normal" {
            // A sample of each text form, for the ADR.
            let one = |s: &[u8]| String::from_utf8_lossy(&s[..s.len().min(400)]).into_owned();
            println!("\nJSON verbose pretty sample:\n{}\n", one(&jp[..]));
            println!("JSON compact sample:\n{}\n", one(&jc[..]));
            println!("RON pretty sample:\n{}\n", one(&rp[..]));
        }
    }
}
