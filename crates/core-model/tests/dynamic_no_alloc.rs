// SPDX-License-Identifier: GPL-3.0-or-later

//! The reads of dynamics, hairpins and tempo marks are allocation-free (ADR-0007 rule 1,
//! ADR-0024 §8 and §11): playback reads them once per note it compiles, and the derived tempo map
//! is what the audio side converts with. Validating a document with them allocates at most the
//! voice → instrument map and the scope list of the hairpin rule.
//!
//! This file is its own test binary with the counting allocator installed (ADR-0016 §5). The
//! inputs go through `black_box` so that the compiler cannot evaluate the lookups at compile time.

// Helper functions (not `#[test]` functions) are outside the scope of clippy.toml's
// `allow-unwrap-in-tests`, so the exemption is stated here, for this test file only.
#![allow(
    clippy::unwrap_used,
    reason = "test helpers: a failed unwrap fails the test"
)]

use std::hint::black_box;

use orchestra_core_model::{
    BarId, Clef, Document, DocumentBuilder, DynamicLevel, DynamicMark, DynamicPlacement,
    GradualMark, HairpinShape, InstrumentId, Interval, Key, Metronome, Mode, ScorePoint, TempoMark,
    TempoMarkChange,
};
use orchestra_core_time::{
    BarIndex, Duration, Meter, MeterChange, MeterMap, Position, Rational, Tempo,
};
use orchestra_test_support::assert_no_allocations;

orchestra_test_support::install_counting_allocator!();

/// The duration `n/d` of a whole note.
fn d(n: i64, den: i64) -> Duration {
    Duration::from_rational(Rational::new(n, den).unwrap())
}

/// ♩ = `bpm`.
fn quarter(bpm: i64) -> Tempo {
    Tempo::new(d(1, 4), Rational::from_integer(bpm).unwrap()).unwrap()
}

/// Sixteen bars of a piano in 4/4: a dynamic every bar (alternating the whole piano and the right
/// hand), a crescendo and a diminuendo every two bars, and a tempo mark every four bars, every
/// other one a ritardando to the next.
fn document() -> (Document, InstrumentId, Vec<BarId>) {
    let meters = MeterMap::new(MeterChange::new(Meter::new(&[4], 4).unwrap())).unwrap();
    let mut b = DocumentBuilder::new(
        meters,
        TempoMark::immediate(Some("Allegro"), Metronome::shown(quarter(132))).unwrap(),
        Key::traditional(0, Mode::Major).unwrap(),
    )
    .unwrap();
    let bars: Vec<_> = (0..16).map(|_| b.add_bar().unwrap()).collect();
    let track = b
        .add_instrument_track(
            "Piano",
            Interval::new(0, Rational::ZERO),
            &[Clef::TREBLE, Clef::BASS],
        )
        .unwrap();
    let (piano, upper, right) = {
        let i = b.document().track(track).unwrap().instrument();
        (i.id(), i.staves()[0].id(), i.voices()[0].id())
    };
    let levels = [DynamicLevel::P, DynamicLevel::Mf, DynamicLevel::F];
    for (i, &bar) in bars.iter().enumerate() {
        let placement = if i % 2 == 0 {
            DynamicPlacement::instrument(upper)
        } else {
            DynamicPlacement::voice(right, upper)
        };
        b.add_dynamic(
            piano,
            ScorePoint::start_of(bar),
            placement,
            DynamicMark::Level(levels[i % 3]),
        )
        .unwrap();
        if i % 2 == 0 && i + 2 <= bars.len() {
            let mid = ScorePoint::new(bar, d(1, 1));
            let shape = if i % 4 == 0 {
                HairpinShape::CRESCENDO
            } else {
                HairpinShape::DIMINUENDO
            };
            b.add_hairpin(piano, ScorePoint::new(bar, d(1, 4)), mid, placement, shape)
                .unwrap();
        }
        if i % 4 == 0 && i > 0 {
            let change = if i % 8 == 0 {
                TempoMarkChange::Immediate(Metronome::shown(quarter(132)))
            } else {
                TempoMarkChange::Gradual(GradualMark {
                    from: None,
                    to: quarter(96),
                    end: ScorePoint::start_of(bars[(i + 4).min(bars.len() - 1)]),
                })
            };
            b.add_tempo_mark(
                ScorePoint::start_of(bar),
                TempoMark::new(None, Some(change)).unwrap(),
            )
            .unwrap();
        }
    }
    (b.build().unwrap(), piano, bars)
}

/// Walking the dynamics and hairpins of an instrument, and finding the tempo mark in force at a
/// point, never allocate.
#[test]
fn reads_do_not_allocate() {
    let (document, piano, bars) = document();
    assert_no_allocations(|| {
        let instrument = black_box(&document).instrument(black_box(piano)).unwrap();
        let mut louder = 0_usize;
        for dynamic in instrument.dynamics() {
            if matches!(dynamic.mark(), DynamicMark::Level(l) if *l >= DynamicLevel::Mf) {
                louder += 1;
            }
        }
        for hairpin in instrument.hairpins() {
            black_box(hairpin.start());
            black_box(hairpin.end());
        }
        for index in 0..16_u32 {
            black_box(document.tempo_marks().in_force_at(
                document.bars(),
                BarIndex::new(black_box(index)),
                d(1, 2),
            ));
        }
        black_box(louder);
    });
    assert_eq!(bars.len(), 16);
}

/// Converting with the derived tempo map never allocates (it is the same `TempoMap` as before
/// F1.09c, now built from the marks).
#[test]
fn the_derived_tempo_map_converts_without_allocating() {
    let (document, _, _) = document();
    assert_no_allocations(|| {
        for n in 0..64 {
            let position = Position::from_rational(Rational::new(black_box(n), 4).unwrap());
            black_box(document.tempo_map().time_at(position)).unwrap();
        }
    });
}

/// Validating checks the hairpins and the tempo marks with at most two allocations: the voice →
/// instrument map and the scope list of the hairpin rule; the tempo map is not rebuilt.
#[test]
fn validate_allocates_a_bounded_amount() {
    let (document, _, _) = document();
    let ((), stats) = orchestra_test_support::count_allocations(|| {
        black_box(document.validate()).unwrap();
    })
    .unwrap();
    assert!(stats.allocations <= 2, "validate allocated: {stats:?}");
    assert_eq!(stats.reallocations, 0, "validate reallocated: {stats:?}");
}
