// SPDX-License-Identifier: GPL-3.0-or-later

//! The memory the document keeps per note on the heavy score (ADR-0007's RAM budget, ADR-0024
//! Consequences; SCALETTA F1.09c, owner's choice E7: a test with a ceiling per note).
//!
//! # The score
//! The shape of the heavy score of `spikes/model/` (ADR-0022): 1 000 bars of 4/4 with a passage of
//! 6/8 from bar 60 to 69 of every hundred; two pianos (two staves, two voices per staff) and 26
//! one-staff instruments, every fourth with a second voice: 30 staves, 41 voices. Every voice fills
//! every bar with a rhythmic unit chosen per voice every 8 bars, 15% rests, chords of 1–4 notes.
//! It is generated here with the real builder; the reusable generator of the reference score
//! comes with SCALETTA F1.16. Unlike the spike, there are no triplets and quintuplets (they would
//! need tuplets, which only add a small table per voice), so the note count is close to, but not
//! exactly, the spike's 255 776.
//!
//! # What is measured
//! The growth of the live heap while the document is built and kept
//! ([`AllocationStats::net_bytes`](orchestra_test_support::AllocationStats::net_bytes)): every
//! chunk, the derived indices, the tables; temporary buffers freed before the end cancel out. It
//! is divided by the number of noteheads, so it is "bytes of document per note", events and their
//! overhead included. It counts what the program requests, not the allocator's rounding: a lower
//! bound of the resident memory, the same on every platform.

// Helper functions (not `#[test]` functions) are outside the scope of clippy.toml's
// `allow-unwrap-in-tests`, so the exemption is stated here, for this test file only.
#![allow(
    clippy::unwrap_used,
    reason = "test helpers: a failed unwrap fails the test"
)]
// The generator indexes its own small tables with indices it computed itself.
#![allow(
    clippy::indexing_slicing,
    reason = "test generator over its own tables"
)]

use orchestra_core_model::{
    Alteration, BarId, BaseValue, Clef, Document, DocumentBuilder, EventBody, Interval, Key,
    Metronome, Mode, NoteValue, Pitch, Placement, RestValue, Step, TempoMark, VoiceId,
};
use orchestra_core_time::{BarIndex, Duration, Meter, MeterChange, MeterMap, Rational, Tempo};

orchestra_test_support::install_counting_allocator!();

/// The ceiling per note, in bytes: what this test measured when it was written (2026-11-03: 257
/// bytes per note, 221 169 noteheads, 56.9 MB), plus about 10%, rounded up to a multiple of 5
/// (owner's decision of the same day). A change of the data structures that crosses it must be a
/// deliberate decision: raise the ceiling in the same commit and say why.
const CEILING_BYTES_PER_NOTE: i128 = 285;

/// A small deterministic generator (64-bit xorshift): no dependency, the same sequence everywhere.
struct Rng(u64);

impl Rng {
    /// The next value in `0..n`.
    fn below(&mut self, n: u64) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0 % n
    }
}

/// The meter map of the heavy score: 4/4, with 6/8 from bar 60 to 69 of every hundred.
fn meters(bars: u32) -> MeterMap {
    let meter = |n, d| MeterChange::new(Meter::new(&[n], d).unwrap());
    let mut meters = MeterMap::new(meter(4, 4)).unwrap();
    let mut base = 0;
    while base + 70 < bars {
        meters
            .set_meter_change(BarIndex::new(base + 60), meter(6, 8))
            .unwrap();
        meters
            .set_meter_change(BarIndex::new(base + 70), meter(4, 4))
            .unwrap();
        base += 100;
    }
    meters
}

/// Adds the instruments of the heavy score and returns their voices as (voice, secondary?):
/// pianos first, two voices per staff, then the one-staff instruments.
fn instruments(b: &mut DocumentBuilder) -> Vec<(VoiceId, bool)> {
    let unison = Interval::new(0, Rational::ZERO);
    let mut voices: Vec<(VoiceId, bool)> = Vec::new();
    for _ in 0..2 {
        let track = b
            .add_instrument_track("Piano", unison, &[Clef::TREBLE, Clef::BASS])
            .unwrap();
        let staves: Vec<_> = b
            .document()
            .track(track)
            .unwrap()
            .instrument()
            .voices()
            .iter()
            .map(|v| (v.id(), v.home_staff()))
            .collect();
        for (voice, staff) in staves {
            voices.push((voice, false));
            voices.push((b.add_voice(staff).unwrap(), true));
        }
    }
    for i in 0..26 {
        let track = b
            .add_instrument_track("Instrument", unison, &[Clef::TREBLE])
            .unwrap();
        let (voice, staff) = {
            let v = &b.document().track(track).unwrap().instrument().voices()[0];
            (v.id(), v.home_staff())
        };
        voices.push((voice, false));
        if i % 4 == 0 {
            voices.push((b.add_voice(staff).unwrap(), true));
        }
    }
    voices
}

/// Rhythmic units in sixteenths, with their note values: quarter, eighth, sixteenth, half.
const UNITS: [(i64, BaseValue); 4] = [
    (4, BaseValue::Quarter),
    (2, BaseValue::Eighth),
    (1, BaseValue::Sixteenth),
    (8, BaseValue::Half),
];

/// The steps, in order, for the chords of thirds.
const STEPS: [Step; 7] = [
    Step::C,
    Step::D,
    Step::E,
    Step::F,
    Step::G,
    Step::A,
    Step::B,
];

/// Fills every bar of one voice: a unit chosen every 8 bars (main voices mostly quarters and
/// eighths, secondary ones halves and quarters), 15% rests, chords of 1–4 notes.
fn fill(b: &mut DocumentBuilder, bars: &[BarId], voice: VoiceId, secondary: bool, rng: &mut Rng) {
    let weights: [u64; 4] = if secondary {
        [50, 10, 0, 40]
    } else {
        [55, 27, 8, 10]
    };
    let mut unit = 0;
    for (index, &bar) in bars.iter().enumerate() {
        if index % 8 == 0 {
            let mut pick = rng.below(100);
            for (i, &w) in weights.iter().enumerate() {
                if pick < w {
                    unit = i;
                    break;
                }
                pick -= w;
            }
        }
        let length = b
            .document()
            .meter_map()
            .bar_length(BarIndex::new(u32::try_from(index).unwrap()))
            .unwrap()
            .as_rational();
        let length = length.numerator() * (16 / length.denominator());
        let mut offset = 0;
        while offset < length {
            // The unit, or a quarter when it does not fit what is left (6/8 and halves).
            let (span, base) = if UNITS[unit].0 <= length - offset {
                UNITS[unit]
            } else {
                UNITS[0]
            };
            let at = Placement::new(
                voice,
                bar,
                Duration::from_rational(Rational::new(offset, 16).unwrap()),
            );
            let value = NoteValue::plain(base);
            let notes = if rng.below(100) < 15 {
                0
            } else {
                match rng.below(100) {
                    0..82 => 1,
                    82..94 => 2,
                    94..98 => 3,
                    _ => 4,
                }
            };
            if notes == 0 {
                b.add_rest(at, RestValue::Value(value)).unwrap();
            } else {
                // A chord of thirds from a random root: distinct pitches, sorted.
                let root = usize::try_from(rng.below(21)).unwrap();
                let pitches: Vec<Pitch> = (0..notes)
                    .map(|k| {
                        let degree = root + 2 * k;
                        let octave = 3 + i8::try_from(degree / 7).unwrap();
                        Pitch::new(STEPS[degree % 7], Alteration::NATURAL, octave)
                    })
                    .collect();
                b.add_chord(at, value, &pitches).unwrap();
            }
            offset += span;
        }
    }
}

/// The heavy score described in the module documentation, built with the real builder.
fn heavy() -> Document {
    const BARS: u32 = 1000;
    let tempo = Tempo::new(
        Duration::from_rational(Rational::new(1, 4).unwrap()),
        Rational::from_integer(100).unwrap(),
    )
    .unwrap();
    let mut b = DocumentBuilder::new(
        meters(BARS),
        TempoMark::immediate(None, Metronome::shown(tempo)).unwrap(),
        Key::traditional(0, Mode::Major).unwrap(),
    )
    .unwrap();
    let bars: Vec<BarId> = (0..BARS).map(|_| b.add_bar().unwrap()).collect();
    let mut rng = Rng(0x0F1_09C);
    for (voice, secondary) in instruments(&mut b) {
        fill(&mut b, &bars, voice, secondary, &mut rng);
    }
    b.build().unwrap()
}

/// The number of noteheads of a document.
fn noteheads(document: &Document) -> usize {
    let bars = u32::try_from(document.bars().len()).unwrap();
    document
        .tracks()
        .iter()
        .flat_map(|t| t.instrument().voices())
        .flat_map(|v| (0..bars).flat_map(move |i| v.events_at(BarIndex::new(i))))
        .map(|placed| match placed.event().body() {
            EventBody::Chord(chord) => chord.notes().count(),
            EventBody::Rest(_) => 0,
        })
        .sum()
}

/// The document keeps at most [`CEILING_BYTES_PER_NOTE`] bytes per notehead on the heavy score.
#[test]
fn memory_per_note_on_the_heavy_score() {
    let (document, stats) = orchestra_test_support::count_allocations(heavy).unwrap();
    let notes = noteheads(&document);
    let per_note = stats.net_bytes() / i128::try_from(notes).unwrap();
    // Printed so that `cargo test -- --nocapture` shows the numbers the ceiling is based on.
    println!(
        "heavy score: {notes} noteheads, {} bytes kept, {per_note} bytes per note",
        stats.net_bytes()
    );
    assert!(
        per_note <= CEILING_BYTES_PER_NOTE,
        "{per_note} bytes per note, above the ceiling of {CEILING_BYTES_PER_NOTE}"
    );
}
