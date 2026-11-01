// SPDX-License-Identifier: GPL-3.0-or-later

//! Every pitch operation is allocation-free (ADR-0023, ADR-0007 rule 1), on its success and on
//! its error paths: the playback compile calls them once per note, and nothing in them needs the
//! heap.
//!
//! This file is its own test binary with the counting allocator installed (ADR-0016 §5). The
//! inputs go through `black_box` so that the compiler cannot evaluate the operations at compile
//! time and leave nothing to measure.

// Helper functions (not `#[test]` functions) are outside the scope of clippy.toml's
// `allow-unwrap-in-tests`, so the exemption is stated here, for this test file only.
#![allow(
    clippy::unwrap_used,
    reason = "test helpers: a failed unwrap fails the test"
)]

use std::hint::black_box;

use orchestra_core_model::pitch::{Alteration, Interval, Key, Mode, Pitch, Quality, Step};
use orchestra_core_time::Rational;
use orchestra_test_support::assert_no_allocations;

orchestra_test_support::install_counting_allocator!();

/// Pitches that exercise every path: chromatic, microtonal, at the ends of the MIDI range and
/// beyond it (errors), with alterations that transposition pushes beyond a triple sharp (errors).
fn pitches() -> [Pitch; 6] {
    [
        Pitch::new(Step::C, Alteration::NATURAL, 4),
        Pitch::new(Step::B, Alteration::SHARP, 3),
        Pitch::new(
            Step::E,
            Alteration::new(Rational::new(-1, 2).unwrap()).unwrap(),
            4,
        ),
        Pitch::new(Step::G, Alteration::chromatic(3).unwrap(), 9),
        Pitch::new(Step::C, Alteration::chromatic(-3).unwrap(), -1),
        Pitch::new(Step::F, Alteration::chromatic(2).unwrap(), 0),
    ]
}

/// Intervals: chromatic in both directions, a compound one, a microtonal one (an error on keys).
fn intervals() -> [Interval; 4] {
    [
        Interval::from_quality(Quality::Major, 2, true).unwrap(),
        Interval::from_quality(Quality::Augmented(2), 6, false).unwrap(),
        Interval::from_quality(Quality::Perfect, 15, true).unwrap(),
        Interval::new(2, Rational::new(7, 2).unwrap()),
    ]
}

/// Every method on every combination of the samples.
#[test]
fn pitch_operations_do_not_allocate() {
    let pitches = black_box(pitches());
    let intervals = black_box(intervals());
    let keys = black_box([
        Key::traditional(0, Mode::Major).unwrap(),
        Key::traditional(-14, Mode::Minor).unwrap(),
        Key::traditional(13, Mode::None).unwrap(),
    ]);
    assert_no_allocations(|| {
        black_box(Alteration::new(black_box(Rational::new(7, 2).unwrap()))).ok();
        black_box(Alteration::chromatic(black_box(-4))).ok();
        black_box(Interval::from_quality(
            black_box(Quality::Perfect),
            3,
            false,
        ))
        .ok();
        black_box(Key::traditional(black_box(15), Mode::Major)).ok();
        for p in pitches {
            black_box(p.sounding()).ok();
            black_box(p.midi()).ok();
            for q in pitches {
                black_box(p.sounds_like(q)).ok();
                if let Ok(i) = p.interval_to(q) {
                    black_box(i.quality()).ok();
                    black_box(i.deviation()).ok();
                }
            }
            for i in intervals {
                black_box(p.transposed(i)).ok();
                black_box((i.number(), i.reversed(), i.fifths().ok()));
            }
        }
        for key in keys {
            for midi in [0_u8, 60, 61, 127] {
                black_box(Pitch::spell_midi(black_box(midi), key));
            }
            for step in Step::ALL {
                black_box(key.alteration_of(step));
            }
            for i in intervals {
                black_box(key.transposed(i)).ok();
            }
            let Key::Traditional(k) = key;
            black_box(k.tonic());
        }
    });
}
