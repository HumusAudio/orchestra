// SPDX-License-Identifier: GPL-3.0-or-later

//! Properties of the MIDI export on random small scores (ADR-0032, "Tests and benchmarks"):
//! random tempi (integer and not), an optional accelerando or ritardando, notes on a grid of
//! eighths and triplet eighths, quarter tones, two voices. For every score, read back with
//! `midly`:
//! - every note-on has its note-off, in its own track, and no velocity is 0;
//! - every note starting inside a gradual change, on an exact tick, sounds within 1 µs of the
//!   path's time (it is a breakpoint, E4); every other one within the bound of its stretch;
//! - exporting twice gives the same bytes.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic,
    reason = "test helpers: a failed unwrap, index or panic fails the test"
)]

#[path = "common/midi.rs"]
mod midi;

use midi::{Score, common_time, d, decode, export_doc};
use orchestra_core_model::{
    Alteration, BaseValue, GradualMark, NoteValue, Pitch, Placement, ScorePoint, Step, TempoMark,
    TempoMarkChange,
};
use orchestra_core_time::{Position, Rational, Tempo};
use orchestra_io::midi::MidiOptions;
use orchestra_playback::{Performance, Unrolled};
use proptest::prelude::*;

/// One random note: voice (0 or 1), bar, eighth of the bar (0 ..= 7), step (C ..= B), quarter
/// tone sharp or not, quarter note (otherwise an eighth).
type NoteSpec = (usize, usize, i64, u8, bool, bool);

/// A random score.
#[derive(Debug, Clone)]
struct Spec {
    /// Number of 4/4 bars.
    bars: usize,
    /// The first tempo, ♩ = num / den: an integer or not (a non-integer number of microseconds
    /// per quarter note is the case that needs two tempo steps per stretch, E4).
    tempo: (i64, i64),
    /// An accelerando or ritardando over one bar: (bar, target ♩).
    gradual: Option<(usize, i64)>,
    /// The notes.
    notes: Vec<NoteSpec>,
}

/// The strategy for a [`Spec`].
fn spec() -> impl Strategy<Value = Spec> {
    (
        2_usize..5,
        (30_i64..240, 1_i64..4),
        proptest::option::of((0_usize..3, 30_i64..240)),
    )
        .prop_flat_map(|(bars, tempo, gradual)| {
            let note = (
                0_usize..2,
                0..bars,
                0_i64..8,
                0_u8..7,
                any::<bool>(),
                any::<bool>(),
            );
            (
                Just(bars),
                Just(tempo),
                // The gradual change needs a bar after it, where it ends.
                Just(gradual.filter(|(b, _)| *b + 1 < bars)),
                proptest::collection::vec(note, 1..24),
            )
        })
        .prop_map(|(bars, tempo, gradual, notes)| Spec {
            bars,
            tempo,
            gradual,
            notes,
        })
}

/// Builds the score of a spec. A note that would overlap an earlier note of its voice (the model
/// refuses overlaps when the document is built) is left out: the property is about the export,
/// not the builder.
fn build(spec: &Spec) -> orchestra_core_model::Document {
    let bpm = Rational::new(spec.tempo.0 * spec.tempo.1 + 1, spec.tempo.1).unwrap();
    let tempo = Tempo::new(d(1, 4), bpm).unwrap();
    let mut s = Score::with_tempo(spec.bars, common_time(), tempo, &["Clarinet"]);
    let staff = s.parts[0].2;
    let second = s.b.add_voice(staff).unwrap();
    let voices = [s.parts[0].1, second];
    if let Some((bar, target)) = spec.gradual {
        let to = Tempo::new(d(1, 4), Rational::from_integer(target).unwrap()).unwrap();
        s.b.add_tempo_mark(
            ScorePoint::new(s.bars[bar], d(1, 4)),
            TempoMark::new(
                None,
                Some(TempoMarkChange::Gradual(GradualMark {
                    from: None,
                    to,
                    end: ScorePoint::new(s.bars[bar + 1], d(1, 4)),
                })),
            )
            .unwrap(),
        )
        .unwrap();
    }
    let steps = [
        Step::C,
        Step::D,
        Step::E,
        Step::F,
        Step::G,
        Step::A,
        Step::B,
    ];
    let quarter_sharp = Alteration::new(Rational::new(1, 2).unwrap()).unwrap();
    // The eighths already taken, per voice and bar.
    let mut taken = vec![vec![[false; 8]; spec.bars]; 2];
    for (voice, bar, slot, step, microtone, long) in &spec.notes {
        let first = usize::try_from(*slot).unwrap();
        let span = if *long { 2 } else { 1 };
        let cells = &mut taken[*voice][*bar];
        if first + span > 8 || cells[first..first + span].iter().any(|t| *t) {
            continue;
        }
        cells[first..first + span]
            .iter_mut()
            .for_each(|t| *t = true);
        let alteration = if *microtone {
            quarter_sharp
        } else {
            Alteration::NATURAL
        };
        let pitch = Pitch::new(steps[usize::from(*step)], alteration, 4);
        let value = if *long {
            BaseValue::Quarter
        } else {
            BaseValue::Eighth
        };
        let _ = s.b.add_chord(
            Placement::new(voices[*voice], s.bars[*bar], d(*slot, 8)),
            NoteValue::plain(value),
            &[pitch],
        );
    }
    s.b.build().unwrap()
}

/// Whether `at` lies inside the gradual change of the spec (written position = unrolled one:
/// these scores have no repeats).
fn in_gradual(spec: &Spec, at: Position) -> bool {
    spec.gradual.is_some_and(|(bar, _)| {
        let bar = i64::try_from(bar).unwrap();
        let start = Rational::new(4 * bar + 1, 4).unwrap();
        let end = Rational::new(4 * bar + 5, 4).unwrap();
        at.as_rational() >= start && at.as_rational() < end
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// See the module documentation.
    #[test]
    fn exported_files_are_sound(spec in spec()) {
        let doc = build(&spec);
        let out = export_doc(&doc, &MidiOptions::new());
        prop_assert_eq!(&out.bytes, &export_doc(&doc, &MidiOptions::new()).bytes);
        let decoded = decode(&out.bytes);
        // `notes` asserts that every note-on is closed in its own track.
        let notes = decoded.notes(0);
        prop_assert!(notes.iter().all(|n| n.4 >= 1));

        let path = Unrolled::new(&doc).unwrap();
        let performance = Performance::new(&doc, &path).unwrap();
        let units = |at: Position| {
            let played = path.played();
            let i = played.partition_point(|b| b.start <= at).saturating_sub(1);
            let offset = at.checked_duration_since(played[i].start).unwrap();
            path.time_at(&doc, i, offset).unwrap().to_f64() * 960_000_000.0
        };
        for (_, list) in performance.instruments() {
            for note in list {
                let ticks = note.start.as_rational().checked_mul(Rational::from_integer(3_840).unwrap()).unwrap();
                if !ticks.is_integer() {
                    continue;
                }
                let tick = u64::try_from(ticks.numerator()).unwrap();
                #[allow(clippy::cast_precision_loss, reason = "test tolerance on f64 seconds")]
                let error = (decoded.units_at(tick) as f64 - units(note.start)).abs();
                // Inside the gradual change a note start is a breakpoint: within 1 µs (plus the
                // f64 rounding of the path's time). Elsewhere the bound of a constant stretch:
                // Δ/4 units for a stretch of Δ ticks, here at most the whole score (5 bars).
                let bound = if in_gradual(&spec, note.start) { 961.0 } else { 3_840.0 * 5.0 / 4.0 };
                prop_assert!(error <= bound, "tick {}: error {} units", tick, error);
            }
        }
    }
}
