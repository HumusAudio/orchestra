// SPDX-License-Identifier: GPL-3.0-or-later

//! The generator of the normal and heavy scores, shared by the memory tests and the editor's
//! benchmarks (included with `#[path]`, so it is compiled into each of them; it is not a test
//! crate of its own because it lives in a subdirectory of `tests/`).
//!
//! # The scores
//! The shapes of `spikes/model/` (ADR-0022):
//! - **normal**: 100 bars, one piano (two staves, two voices per staff) and 8 one-staff
//!   instruments, every fourth with a second voice: 10 staves;
//! - **heavy**: 1 000 bars, two pianos and 26 one-staff instruments: 30 staves, 41 voices.
//!
//! Both have 4/4 with a passage of 6/8 from bar 60 to 69 of every hundred. Every voice fills
//! every bar with a rhythmic unit chosen per voice every 8 bars, 15% rests, chords of 1–4 notes.
//! There are no tuplets (they would only add a small table per voice), so the note counts are
//! close to, but not exactly, the spike's (8 690 and 255 776). The reusable generator of the
//! reference score, with everything a score holds, comes with SCALETTA F1.16.
//!
//! Optionally an **editing instrument** is added last: one staff, one voice, no events, where
//! the benchmarks of the editor enter their notes into free space.

// Helper functions are outside the scope of clippy.toml's `allow-unwrap-in-tests`, so the
// exemption is stated here, for this generator only.
#![allow(
    clippy::unwrap_used,
    reason = "test and benchmark setup: a failed unwrap stops it loudly"
)]
// The generator indexes its own small tables with indices it computed itself.
#![allow(
    clippy::indexing_slicing,
    reason = "test generator over its own tables"
)]
// Each includer uses its own part of the generator.
#![allow(dead_code, reason = "shared by several test and benchmark crates")]

use orchestra_core_model::{
    Alteration, BarId, BaseValue, Clef, Document, DocumentBuilder, Interval, Key, Metronome, Mode,
    NoteValue, Pitch, Placement, RestValue, Step, TempoMark, VoiceId,
};
use orchestra_core_time::{BarIndex, Duration, Meter, MeterChange, MeterMap, Rational, Tempo};

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

/// Adds the instruments of a score and returns their voices as (voice, secondary?): `pianos`
/// pianos first (two staves, two voices per staff), then `others` one-staff instruments, every
/// fourth with a second voice.
fn instruments(b: &mut DocumentBuilder, pianos: usize, others: usize) -> Vec<(VoiceId, bool)> {
    let unison = Interval::new(0, Rational::ZERO);
    let mut voices: Vec<(VoiceId, bool)> = Vec::new();
    for _ in 0..pianos {
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
    for i in 0..others {
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

/// The size of a generated score.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Size {
    /// 100 bars, 10 staves.
    Normal,
    /// 1 000 bars, 30 staves.
    Heavy,
}

/// The score of this size, built with the real builder, with the editing instrument last if
/// `editing` (see the module documentation). The same seed for both sizes, so the heavy score is
/// exactly the one `tests/memory.rs` measured when its ceiling was set.
pub(crate) fn score(size: Size, editing: bool) -> Document {
    let (bars, pianos, others) = match size {
        Size::Normal => (100, 1, 8),
        Size::Heavy => (1000, 2, 26),
    };
    let tempo = Tempo::new(
        Duration::from_rational(Rational::new(1, 4).unwrap()),
        Rational::from_integer(100).unwrap(),
    )
    .unwrap();
    let mut b = DocumentBuilder::new(
        meters(bars),
        TempoMark::immediate(None, Metronome::shown(tempo)).unwrap(),
        Key::traditional(0, Mode::Major).unwrap(),
    )
    .unwrap();
    let ids: Vec<BarId> = (0..bars).map(|_| b.add_bar().unwrap()).collect();
    let mut rng = Rng(0x0F1_09C);
    for (voice, secondary) in instruments(&mut b, pianos, others) {
        fill(&mut b, &ids, voice, secondary, &mut rng);
    }
    if editing {
        let unison = Interval::new(0, Rational::ZERO);
        b.add_instrument_track("Editing", unison, &[Clef::TREBLE])
            .unwrap();
    }
    b.build().unwrap()
}
