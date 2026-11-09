// SPDX-License-Identifier: GPL-3.0-or-later

//! Inserting, deleting and replacing runs of bars in the [`MeterMap`] (ADR-0022 §8, ADR-0026 §1
//! and §11 ADR-0026 Z1): hand-written cases and property tests against a naive oracle.
//!
//! # The oracle
//! The library splices sorted vectors and rebuilds its segments. The oracle keeps every source
//! list as a `BTreeMap` from bar to value and moves the entries **one by one** with the textbook
//! rule ("an entry at or after the insertion point moves `count` bars later"; "an entry inside
//! the removed run leaves, one after it moves `count` bars earlier"). It shares no code with the
//! library. On top of that, the two operations must be exact inverses (an undo depends on it),
//! a failure must leave the map unchanged, and the bars before the edit point must not move.

// Helper functions (not `#[test]` functions) are outside the scope of clippy.toml's
// `allow-unwrap-in-tests`, so the exemption is stated here, for this test file only: a helper that
// unwraps fails the calling test with a clear message, which is exactly what a test wants.
#![allow(
    clippy::unwrap_used,
    reason = "test helpers: a failed unwrap fails the test"
)]

use std::collections::{BTreeMap, BTreeSet};

use orchestra_core_time::{
    BarIndex, BarKind, BarSpan, Duration, IrregularBar, Meter, MeterChange, MeterMap,
    MeterMapError, Position, Rational,
};
use proptest::prelude::*;

/// Shorthand: a duration of `n/d` whole notes.
fn d(n: i64, den: i64) -> Duration {
    Duration::from_rational(Rational::new(n, den).unwrap())
}

/// Shorthand: a position `n/d` whole notes after the start.
fn p(n: i64, den: i64) -> Position {
    Position::from_rational(Rational::new(n, den).unwrap())
}

/// Shorthand: a meter change to `n/den` with no section grouping.
fn meter(n: u8, den: u16) -> MeterChange {
    MeterChange::new(Meter::new(&[n], den).unwrap())
}

/// Shorthand: bar index `i`.
const fn bar(i: u32) -> BarIndex {
    BarIndex::new(i)
}

/// 4/4 from bar 0, 3/4 from bar 4, a 1/4 pickup at bar 0, bar 6 uncounted, numbering restarting
/// at 10 on bar 8: one entry of every kind but the per-bar grouping.
fn sample() -> MeterMap {
    let mut map = MeterMap::new(meter(4, 4)).unwrap();
    map.set_meter_change(bar(4), meter(3, 4)).unwrap();
    map.set_irregular_bar(
        bar(0),
        IrregularBar {
            length: d(1, 4),
            kind: BarKind::AlignEnd,
        },
    )
    .unwrap();
    map.set_numbering(bar(6), orchestra_core_time::BarCounting::Uncounted);
    map.set_numbering_restart(bar(8), Some(10));
    map
}

/// Inserting empty bars before bar 2 makes them 4/4 bars (the meter in force there) and moves
/// every later entry two bars on, so the 3/4 still starts on the same written bar.
#[test]
fn inserted_bars_follow_the_meter_in_force() {
    let mut map = sample();
    map.insert_bars(bar(2), &BarSpan::empty(2)).unwrap();
    assert_eq!(map.meter_changes()[1].0, bar(6));
    assert_eq!(map.uncounted_bars(), &[bar(8)]);
    assert_eq!(map.numbering_restarts(), &[(bar(10), 10)]);
    // The pickup before the insertion point does not move.
    assert_eq!(map.irregular_bars()[0].0, bar(0));
    // Bars 2 and 3 are the new 4/4 bars: pickup (1/4) + bar 1 (1) = 5/4 before bar 2.
    assert_eq!(map.bar_start(bar(2)).unwrap(), p(5, 4));
    assert_eq!(map.bar_length(bar(3)).unwrap(), d(1, 1));
    // The old bar 2 is now bar 4, still 4/4, and the 3/4 starts at the old bar 4, now bar 6.
    assert_eq!(map.bar_length(bar(6)).unwrap(), d(3, 4));
}

/// An entry exactly at the insertion point moves with its bar: inserting before bar 4 leaves the
/// new bars in 4/4 and the 3/4 on the old bar 4.
#[test]
fn an_entry_at_the_insertion_point_moves_with_its_bar() {
    let mut map = sample();
    map.insert_bars(bar(4), &BarSpan::empty(3)).unwrap();
    assert_eq!(map.meter_changes()[1].0, bar(7));
    assert_eq!(map.bar_length(bar(6)).unwrap(), d(1, 1));
    assert_eq!(map.bar_length(bar(7)).unwrap(), d(3, 4));
}

/// Inserting at bar 0 moves the meter change of bar 0 too, so an empty span is refused (the map
/// would have no meter at the start), and a span with its own meter change is accepted.
#[test]
fn inserting_at_the_start_needs_a_meter_in_the_span() {
    let mut map = sample();
    let before = map.clone();
    assert_eq!(
        map.insert_bars(bar(0), &BarSpan::empty(1)),
        Err(MeterMapError::NoMeterAtStart)
    );
    assert_eq!(map, before);
    let mut span = BarSpan::empty(1);
    span.set_meter_change(bar(0), meter(2, 4));
    map.insert_bars(bar(0), &span).unwrap();
    assert_eq!(map.bar_length(bar(0)).unwrap(), d(2, 4));
    // The old bar 0 (the pickup) is now bar 1 and keeps its 4/4 meter and its pickup.
    assert_eq!(map.meter_changes()[1], (bar(1), meter(4, 4)));
    assert_eq!(map.bar_length(bar(1)).unwrap(), d(1, 4));
}

/// A span entry outside the span's own bars is refused: it would land on an existing bar.
#[test]
fn a_span_entry_outside_its_bars_is_refused() {
    let mut map = sample();
    let before = map.clone();
    let mut span = BarSpan::empty(2);
    span.set_meter_change(bar(2), meter(5, 4));
    assert_eq!(
        map.insert_bars(bar(1), &span),
        Err(MeterMapError::IndexOutOfRange)
    );
    assert_eq!(map, before);
}

/// Removing bars that hold the 3/4 change takes the change away: the later bars fall back to the
/// 4/4 in force before the run (moving it is the document command's job, ADR-0026 §6).
#[test]
fn removed_entries_leave_with_their_bars() {
    let mut map = sample();
    let span = map.remove_bars(bar(3), 2).unwrap();
    assert_eq!(span.count(), 2);
    assert_eq!(span.meter_changes(), &[(bar(1), meter(3, 4))]);
    assert_eq!(map.meter_changes(), &[(bar(0), meter(4, 4))]);
    assert_eq!(map.bar_length(bar(3)).unwrap(), d(1, 1));
    assert_eq!(map.uncounted_bars(), &[bar(4)]);
    assert_eq!(map.numbering_restarts(), &[(bar(6), 10)]);
}

/// Removing from bar 0 without a meter change left at the start is refused; with one there
/// (the command moves it first) it is accepted.
#[test]
fn removing_the_start_needs_a_meter_after_it() {
    let mut map = sample();
    let before = map.clone();
    assert_eq!(
        map.remove_bars(bar(0), 2),
        Err(MeterMapError::NoMeterAtStart)
    );
    assert_eq!(map, before);
    map.set_meter_change(bar(2), meter(4, 4)).unwrap();
    let span = map.remove_bars(bar(0), 2).unwrap();
    assert_eq!(map.meter_changes()[0], (bar(0), meter(4, 4)));
    assert_eq!(map.bar_start(bar(2)).unwrap(), p(2, 1));
    assert_eq!(span.irregular_bars()[0].0, bar(0));
}

/// Removing past the last representable bar is refused.
#[test]
fn removing_past_the_last_bar_is_refused() {
    let mut map = sample();
    let before = map.clone();
    assert_eq!(
        map.remove_bars(bar(u32::MAX), 1),
        Err(MeterMapError::IndexOutOfRange)
    );
    assert_eq!(map, before);
}

/// Inserting and removing zero bars changes nothing.
#[test]
fn zero_bars_change_nothing() {
    let mut map = sample();
    let before = map.clone();
    map.insert_bars(bar(3), &BarSpan::empty(0)).unwrap();
    assert_eq!(map, before);
    let span = map.remove_bars(bar(3), 0).unwrap();
    assert_eq!(map, before);
    assert!(span.has_no_entries());
}

/// Replacing bars 2–3 of the sample (4/4) with a span that makes bar 2 a 3/4 and bar 3 a bar of a
/// half: the bars after them start earlier, bar 4 keeps its own 3/4, and the returned span puts
/// the old data back exactly (ADR-0026 Z1).
#[test]
fn a_run_of_bars_is_replaced_at_once() {
    let original = sample();
    let mut map = original.clone();
    let mut span = BarSpan::empty(2);
    span.set_meter_change(bar(0), meter(3, 4));
    span.set_irregular_bar(
        bar(1),
        IrregularBar {
            length: d(1, 2),
            kind: BarKind::AlignStart,
        },
    );
    let old = map.replace_span(bar(2), &span).unwrap();
    assert!(old.has_no_entries());
    // Pickup 1/4, bar 1 a whole, bar 2 three quarters, bar 3 a half: bar 4 starts at 5/2.
    assert_eq!(map.bar_start(bar(4)).unwrap(), p(5, 2));
    assert_eq!(map.bar_length(bar(4)).unwrap(), d(3, 4));
    let back = map.replace_span(bar(2), &old).unwrap();
    assert_eq!(back, span);
    assert_eq!(map, original);
}

/// A span whose entries fall outside its bars, and a run at bar 0 without a meter change, are
/// refused and change nothing.
#[test]
fn a_replacement_that_breaks_the_map_is_refused() {
    let original = sample();
    let mut map = original.clone();
    let mut outside = BarSpan::empty(1);
    outside.set_numbering(bar(1), orchestra_core_time::BarCounting::Uncounted);
    assert_eq!(
        map.replace_span(bar(2), &outside),
        Err(MeterMapError::IndexOutOfRange)
    );
    assert_eq!(
        map.replace_span(bar(0), &BarSpan::empty(1)),
        Err(MeterMapError::NoMeterAtStart)
    );
    assert_eq!(map, original);
}

// ------------------------------------------------------------------------------------------------
// Property tests
// ------------------------------------------------------------------------------------------------

/// How many bars the random maps use.
const BARS: u32 = 40;

/// The oracle's view of a map: every source list as an ordered map from bar to value.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Lists {
    /// The meter changes.
    meters: BTreeMap<u32, MeterChange>,
    /// The irregular bars.
    irregular: BTreeMap<u32, IrregularBar>,
    /// The uncounted bars.
    uncounted: BTreeSet<u32>,
    /// The numbering restarts.
    restarts: BTreeMap<u32, u32>,
}

impl Lists {
    /// Reads the source lists back from a map's accessors.
    fn of(map: &MeterMap) -> Self {
        Self {
            meters: map
                .meter_changes()
                .iter()
                .map(|(b, v)| (b.get(), *v))
                .collect(),
            irregular: map
                .irregular_bars()
                .iter()
                .map(|(b, v)| (b.get(), *v))
                .collect(),
            uncounted: map.uncounted_bars().iter().map(|b| b.get()).collect(),
            restarts: map
                .numbering_restarts()
                .iter()
                .map(|(b, v)| (b.get(), *v))
                .collect(),
        }
    }

    /// The oracle's insertion of `count` empty bars before `at`: one entry at a time.
    fn inserted(&self, at: u32, count: u32) -> Self {
        /// Moves every key ≥ `at` by `count`.
        fn shift<V: Copy>(list: &BTreeMap<u32, V>, at: u32, count: u32) -> BTreeMap<u32, V> {
            list.iter()
                .map(|(&b, &v)| (if b >= at { b + count } else { b }, v))
                .collect()
        }
        Self {
            meters: shift(&self.meters, at, count),
            irregular: shift(&self.irregular, at, count),
            uncounted: self
                .uncounted
                .iter()
                .map(|&b| if b >= at { b + count } else { b })
                .collect(),
            restarts: shift(&self.restarts, at, count),
        }
    }

    /// The oracle's replacement of bars `at..at + count` with the entries of `span` (whose bars
    /// are relative to `at`): drop the run's entries, add the span's moved by `at`.
    fn replaced(&self, at: u32, count: u32, span: &Self) -> Self {
        /// Keeps the keys outside the run and adds the span's, moved.
        fn swap<V: Copy>(
            list: &BTreeMap<u32, V>,
            span: &BTreeMap<u32, V>,
            at: u32,
            count: u32,
        ) -> BTreeMap<u32, V> {
            list.iter()
                .filter(|&(&b, _)| b < at || b >= at + count)
                .map(|(&b, &v)| (b, v))
                .chain(span.iter().map(|(&b, &v)| (b + at, v)))
                .collect()
        }
        Self {
            meters: swap(&self.meters, &span.meters, at, count),
            irregular: swap(&self.irregular, &span.irregular, at, count),
            uncounted: self
                .uncounted
                .iter()
                .copied()
                .filter(|&b| b < at || b >= at + count)
                .chain(span.uncounted.iter().map(|&b| b + at))
                .collect(),
            restarts: swap(&self.restarts, &span.restarts, at, count),
        }
    }

    /// The oracle's view of a span, read the same way as a map's lists.
    fn of_span(span: &BarSpan) -> Self {
        Self {
            meters: span
                .meter_changes()
                .iter()
                .map(|(b, v)| (b.get(), *v))
                .collect(),
            irregular: span
                .irregular_bars()
                .iter()
                .map(|(b, v)| (b.get(), *v))
                .collect(),
            uncounted: span.uncounted_bars().iter().map(|b| b.get()).collect(),
            restarts: span
                .numbering_restarts()
                .iter()
                .map(|(b, v)| (b.get(), *v))
                .collect(),
        }
    }

    /// The oracle's removal of bars `at..at + count`: one entry at a time.
    fn removed(&self, at: u32, count: u32) -> Self {
        /// Drops the keys in the run and moves the later ones back.
        fn cut<V: Copy>(list: &BTreeMap<u32, V>, at: u32, count: u32) -> BTreeMap<u32, V> {
            list.iter()
                .filter(|&(&b, _)| b < at || b >= at + count)
                .map(|(&b, &v)| (if b >= at + count { b - count } else { b }, v))
                .collect()
        }
        Self {
            meters: cut(&self.meters, at, count),
            irregular: cut(&self.irregular, at, count),
            uncounted: self
                .uncounted
                .iter()
                .filter(|&&b| b < at || b >= at + count)
                .map(|&b| if b >= at + count { b - count } else { b })
                .collect(),
            restarts: cut(&self.restarts, at, count),
        }
    }
}

/// One random edit used to build a map: (kind, bar, choice).
fn edits() -> impl Strategy<Value = Vec<(u8, u32, u8)>> {
    prop::collection::vec((0_u8..4, 0..BARS, 0_u8..4), 0..16)
}

/// Builds a map from random edits; invalid edits fail and are simply skipped.
fn build(edits: &[(u8, u32, u8)]) -> MeterMap {
    let meters = [meter(4, 4), meter(3, 4), meter(6, 8), meter(5, 8)];
    let mut map = MeterMap::new(meters[0]).unwrap();
    for &(kind, b, choice) in edits {
        let choice = usize::from(choice);
        // Ignoring the result is the point: an invalid edit must fail and change nothing, which
        // the oracle test of the meter map already checks.
        let _ = match kind {
            0 => map
                .set_meter_change(bar(b), meters[choice % meters.len()])
                .map(|_| ()),
            1 => map
                .set_irregular_bar(
                    bar(b),
                    IrregularBar {
                        length: d(i64::try_from(choice).unwrap() + 1, 8),
                        kind: [BarKind::AlignStart, BarKind::AlignEnd, BarKind::Free][choice % 3],
                    },
                )
                .map(|_| ()),
            2 => {
                map.set_numbering(bar(b), orchestra_core_time::BarCounting::Uncounted);
                Ok(())
            }
            _ => {
                map.set_numbering_restart(bar(b), Some(u32::try_from(choice).unwrap() + 1));
                Ok(())
            }
        };
    }
    map
}

proptest! {
    /// Inserting empty bars moves the entries exactly as the oracle does, keeps the bars before
    /// the insertion point where they were, and removing the same bars restores the map and
    /// returns an empty span.
    #[test]
    fn insertion_matches_the_oracle(edits in edits(), at in 0..BARS, count in 0_u32..6) {
        let original = build(&edits);
        let mut map = original.clone();
        match map.insert_bars(bar(at), &BarSpan::empty(count)) {
            Ok(()) => {
                prop_assert_eq!(Lists::of(&map), Lists::of(&original).inserted(at, count));
                for b in 0..at {
                    prop_assert_eq!(map.bar_start(bar(b)), original.bar_start(bar(b)));
                }
                let span = map.remove_bars(bar(at), count).unwrap();
                prop_assert!(span.has_no_entries());
                prop_assert_eq!(&map, &original);
            }
            // The only refusal for an empty span: inserting at bar 0 with bars (the meter of bar 0
            // moves away); the map is then unchanged.
            Err(e) => {
                prop_assert_eq!(e, MeterMapError::NoMeterAtStart);
                prop_assert!(at == 0 && count > 0);
                prop_assert_eq!(&map, &original);
            }
        }
    }

    /// Removing bars moves the entries exactly as the oracle does, keeps the bars before the run
    /// where they were, and inserting the returned span restores the very same map. A refused
    /// removal (from bar 0 with no meter change after the run, or a map made invalid, such as a
    /// pickup under a new meter) leaves the map unchanged.
    #[test]
    fn removal_matches_the_oracle(edits in edits(), at in 0..BARS, count in 0_u32..6) {
        let original = build(&edits);
        let mut map = original.clone();
        if let Ok(span) = map.remove_bars(bar(at), count) {
            prop_assert_eq!(span.count(), count);
            prop_assert_eq!(Lists::of(&map), Lists::of(&original).removed(at, count));
            for b in 0..at {
                prop_assert_eq!(map.bar_start(bar(b)), original.bar_start(bar(b)));
            }
            map.insert_bars(bar(at), &span).unwrap();
            prop_assert_eq!(&map, &original);
        } else {
            prop_assert_eq!(&map, &original);
        }
    }
}

proptest! {
    /// Replacing a run of bars with the data of a run of another random map gives what the oracle
    /// gives, returns the run's old data, and replacing it back restores the very same map; a
    /// refused replacement (the result is not a valid map) changes nothing (ADR-0026 Z1).
    #[test]
    fn replacement_matches_the_oracle(
        edits in edits(),
        other in edits(),
        at in 0..BARS,
        from in 0..BARS,
        count in 0_u32..6,
    ) {
        let original = build(&edits);
        // The span: the data of `count` bars of another map, taken with `remove_bars` (which
        // may refuse at bar 0; then the span is simply empty).
        let span = build(&other)
            .remove_bars(bar(from), count)
            .unwrap_or_else(|_| BarSpan::empty(count));
        let mut map = original.clone();
        if let Ok(old) = map.replace_span(bar(at), &span) {
            let expected = Lists::of(&original).replaced(at, count, &Lists::of_span(&span));
            prop_assert_eq!(Lists::of(&map), expected);
            prop_assert_eq!(old.count(), count);
            for b in 0..at {
                prop_assert_eq!(map.bar_start(bar(b)), original.bar_start(bar(b)));
            }
            let back = map.replace_span(bar(at), &old).unwrap();
            prop_assert_eq!(&back, &span);
            prop_assert_eq!(&map, &original);
        } else {
            prop_assert_eq!(&map, &original);
        }
    }
}
