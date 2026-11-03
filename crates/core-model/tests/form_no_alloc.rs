// SPDX-License-Identifier: GPL-3.0-or-later

//! The reads of the form are allocation-free (ADR-0007 rule 1, ADR-0025): the bar number of a bar
//! is asked by layout at every system start and by the DAW's bar ruler, the form lists by the
//! unrolling of F1.12. Validating a document with repeats and voltas adds no allocation.
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
    BarId, BarNumbering, Document, DocumentBuilder, Key, Metronome, Mode, PassSet, TempoMark,
};
use orchestra_core_time::{BarIndex, Duration, Meter, MeterChange, MeterMap, Rational, Tempo};
use orchestra_test_support::assert_no_allocations;

orchestra_test_support::install_counting_allocator!();

/// 64 bars in sections of 8: a start repeat, five bars, a "1. :‖ 2. :‖ 3." group of one-bar
/// voltas; an uncounted bar in the third section; the lettered style.
fn document() -> (Document, Vec<BarId>) {
    let mut meters = MeterMap::new(MeterChange::new(Meter::new(&[4], 4).unwrap())).unwrap();
    meters.set_uncounted(BarIndex::new(19), true);
    let tempo = Tempo::new(
        Duration::from_rational(Rational::new(1, 4).unwrap()),
        Rational::from_integer(100).unwrap(),
    )
    .unwrap();
    let mut b = DocumentBuilder::new(
        meters,
        TempoMark::immediate(None, Metronome::shown(tempo)).unwrap(),
        Key::traditional(0, Mode::Major).unwrap(),
    )
    .unwrap();
    let bars: Vec<_> = (0..64).map(|_| b.add_bar().unwrap()).collect();
    for section in (0..64).step_by(8) {
        b.add_repeat_start(bars[section]).unwrap();
        for (k, at) in (section + 5..section + 8).enumerate() {
            let pass = u8::try_from(k + 1).unwrap();
            b.add_volta(
                bars[at],
                bars[at],
                PassSet::range(pass, pass).unwrap(),
                k < 2,
                None,
            )
            .unwrap();
            if k < 2 {
                b.add_repeat_end(bars[at], 3).unwrap();
            }
        }
    }
    b.set_bar_numbering(BarNumbering::SameAsFirstLettered);
    (b.build().unwrap(), bars)
}

/// The bar number of every bar, and walking the form lists, never allocate.
#[test]
fn reads_do_not_allocate() {
    let (document, bars) = document();
    assert_no_allocations(|| {
        for &bar in &bars {
            black_box(black_box(&document).bar_label(black_box(bar))).unwrap();
        }
        let form = document.form();
        let mut plays = 0_u32;
        for volta in form.voltas() {
            plays += volta.passes().len();
        }
        for end in form.repeat_ends() {
            plays += u32::from(end.times());
        }
        black_box((plays, form.repeat_starts().len(), form.numbering()));
    });
}

/// Rules 8 and 9 add no allocation to `validate` (still the voice → instrument map only, as there
/// are no hairpins here).
#[test]
fn validate_allocates_a_bounded_amount() {
    let (document, _) = document();
    let ((), stats) = orchestra_test_support::count_allocations(|| {
        black_box(document.validate()).unwrap();
    })
    .unwrap();
    assert!(stats.allocations <= 1, "validate allocated: {stats:?}");
    assert_eq!(stats.reallocations, 0, "validate reallocated: {stats:?}");
}
