// SPDX-License-Identifier: GPL-3.0-or-later

//! Property tests of [`MeterMap`] against a naive oracle (ADR-0018, ADR-0019, ADR-0016 §4).
//!
//! # The oracle
//! The library derives segments, cumulates their starts and answers with binary searches. The
//! oracle does none of that: it reads the **source data** back from the map's accessors and walks
//! bar by bar from bar 0, finding each bar's meter with a linear scan, summing lengths one bar at
//! a time, and listing the beats of a bar explicitly before walking through them. It is slow
//! (O(bars) per question) and obviously right; if the segment logic is wrong, the two disagree.
//!
//! # The inputs
//! A random sequence of edits (meter changes, irregular bars of the three kinds, per-bar and
//! section groupings) on the first [`BARS`] bars. Many edits are invalid (a grouping of the wrong
//! length, a pickup too long): those must fail **and leave the map unchanged**, which is checked
//! too. Then random points inside the first bars are looked up both ways.

// Helper functions (not `#[test]` functions) are outside the scope of clippy.toml's
// `allow-unwrap-in-tests`, so the exemption is stated here, for this test file only: a helper that
// unwraps fails the calling test with a clear message, which is exactly what a test wants.
#![allow(
    clippy::unwrap_used,
    reason = "test helpers: a failed unwrap fails the test"
)]

use orchestra_core_time::{
    BarIndex, BarKind, BarPosition, BeatGrouping, BeatPosition, Duration, IrregularBar, Meter,
    MeterChange, MeterMap, Position, Rational,
};
use proptest::prelude::*;

/// How many bars the edits and the checks cover.
const BARS: u32 = 48;

/// Shorthand: a duration of `n/d` whole notes.
fn d(n: i64, den: i64) -> Duration {
    Duration::from_rational(Rational::new(n, den).unwrap())
}

/// The meters the edits choose from: simple, compound, additive, non-binary, odd.
fn meters() -> Vec<Meter> {
    [
        (&[4][..], 4),
        (&[3], 4),
        (&[6], 8),
        (&[9], 8),
        (&[3, 2, 2], 8),
        (&[4], 3),
        (&[5], 16),
        (&[7], 8),
    ]
    .iter()
    .map(|(g, den)| Meter::new(g, *den).unwrap())
    .collect()
}

/// The groupings the edits choose from; most fit only some meters, so many edits fail.
fn groupings() -> Vec<BeatGrouping> {
    [
        (&[3, 2, 2, 2][..], 8),
        (&[3, 3], 8),
        (&[2, 2, 3], 8),
        (&[1, 1, 1, 1], 4),
        (&[2, 1], 4),
        (&[1, 1, 1, 1, 1], 16),
    ]
    .iter()
    .map(|(g, unit)| BeatGrouping::new(g, *unit).unwrap())
    .collect()
}

/// One edit of the map.
#[derive(Debug, Clone)]
enum Edit {
    /// Set a meter change, maybe with a section grouping.
    Meter(u32, usize, Option<usize>),
    /// Set an irregular bar of `n/d` whole notes.
    Irregular(u32, i64, i64, BarKind),
    /// Set a per-bar grouping.
    Grouping(u32, usize),
    /// Remove whatever is at a bar (meter change, irregular bar, grouping).
    Remove(u32),
}

/// A random edit.
fn edit() -> impl Strategy<Value = Edit> {
    let kind = prop_oneof![
        Just(BarKind::AlignStart),
        Just(BarKind::AlignEnd),
        Just(BarKind::Free)
    ];
    prop_oneof![
        (0..BARS, 0..8_usize, proptest::option::of(0..6_usize))
            .prop_map(|(b, m, g)| Edit::Meter(b, m, g)),
        (
            0..BARS,
            1_i64..=24,
            prop_oneof![Just(4_i64), Just(8), Just(12), Just(16)],
            kind
        )
            .prop_map(|(b, n, den, k)| Edit::Irregular(b, n, den, k)),
        (0..BARS, 0..6_usize).prop_map(|(b, g)| Edit::Grouping(b, g)),
        (0..BARS).prop_map(Edit::Remove),
    ]
}

/// Applies an edit; on failure, checks that the map is unchanged (all-or-nothing editing).
fn apply(map: &mut MeterMap, edit: &Edit) {
    let (meters, groupings) = (meters(), groupings());
    let before = map.clone();
    let ok = match *edit {
        Edit::Meter(b, m, g) => {
            let meter = meters[m];
            let change = match g {
                Some(g) => MeterChange::with_grouping(meter, groupings[g]),
                None => MeterChange::new(meter),
            };
            map.set_meter_change(BarIndex::new(b), change).is_ok()
        }
        Edit::Irregular(b, n, den, kind) => map
            .set_irregular_bar(
                BarIndex::new(b),
                IrregularBar {
                    length: d(n, den),
                    kind,
                },
            )
            .is_ok(),
        Edit::Grouping(b, g) => map.set_bar_grouping(BarIndex::new(b), groupings[g]).is_ok(),
        Edit::Remove(b) => {
            let bar = BarIndex::new(b);
            let a = b == 0 || map.remove_meter_change(bar).is_ok();
            let c = map.remove_irregular_bar(bar).is_ok();
            let e = map.remove_bar_grouping(bar).is_ok();
            a && c && e
        }
    };
    if !ok && !matches!(edit, Edit::Remove(_)) {
        assert_eq!(*map, before, "a failed edit changed the map: {edit:?}");
    }
}

// ------------------------------------------------------------------------------------------------
// The naive oracle
// ------------------------------------------------------------------------------------------------

/// The meter change in force at bar `k`, by a linear scan of the source data.
fn naive_change(map: &MeterMap, k: u32) -> MeterChange {
    let mut found = None;
    for (bar, change) in map.meter_changes() {
        if bar.get() <= k {
            found = Some(*change);
        }
    }
    found.unwrap()
}

/// The irregular setting of bar `k`, by a linear scan.
fn naive_irregular(map: &MeterMap, k: u32) -> Option<IrregularBar> {
    map.irregular_bars()
        .iter()
        .find(|(bar, _)| bar.get() == k)
        .map(|(_, i)| *i)
}

/// The actual length of bar `k`.
fn naive_length(map: &MeterMap, k: u32) -> Duration {
    match naive_irregular(map, k) {
        Some(irregular) => irregular.length,
        None => naive_change(map, k).meter.nominal_length().unwrap(),
    }
}

/// The start of bar `k`, summing the lengths of the bars before it one by one.
fn naive_start(map: &MeterMap, k: u32) -> Position {
    let mut start = Position::ZERO;
    for j in 0..k {
        start = start.checked_add(naive_length(map, j)).unwrap();
    }
    start
}

/// The beat lengths of one pass of bar `k`'s counting, written out explicitly; `None` if free.
fn naive_beats(map: &MeterMap, k: u32) -> Option<Vec<Duration>> {
    if matches!(
        naive_irregular(map, k),
        Some(IrregularBar {
            kind: BarKind::Free,
            ..
        })
    ) {
        return None;
    }
    let change = naive_change(map, k);
    let per_bar = map
        .bar_groupings()
        .iter()
        .find(|(bar, _)| bar.get() == k)
        .map(|(_, g)| *g);
    if let Some(g) = per_bar.or(change.grouping) {
        let unit = i64::from(g.unit());
        return Some(g.groups().iter().map(|&n| d(i64::from(n), unit)).collect());
    }
    let meter = change.meter;
    let den = i64::from(meter.denominator());
    if meter.groups().len() > 1 {
        return Some(
            meter
                .groups()
                .iter()
                .map(|&n| d(i64::from(n), den))
                .collect(),
        );
    }
    let n = meter.numerator();
    if n > 3 && n.is_multiple_of(3) {
        Some(vec![d(3, den); usize::from(n / 3)])
    } else {
        Some(vec![d(1, den); usize::from(n)])
    }
}

/// The beat containing `offset` in bar `k`, walking the explicit beat list pass after pass from
/// the virtual start of the nominal bar.
fn naive_beat(map: &MeterMap, k: u32, offset: Duration) -> Option<BeatPosition> {
    let beats = naive_beats(map, k)?;
    let nominal = naive_change(map, k).meter.nominal_length().unwrap();
    let shift = match naive_irregular(map, k) {
        Some(IrregularBar {
            length,
            kind: BarKind::AlignEnd,
        }) => nominal.checked_sub(length).unwrap(),
        _ => Duration::ZERO,
    };
    let mut target = offset.checked_add(shift).unwrap();
    let mut index = 0_u32;
    loop {
        for length in &beats {
            if target < *length {
                return Some(BeatPosition {
                    beat: index,
                    offset: target,
                });
            }
            target = target.checked_sub(*length).unwrap();
            index += 1;
        }
    }
}

// ------------------------------------------------------------------------------------------------
// Properties
// ------------------------------------------------------------------------------------------------

proptest! {
    // Each case builds a map with up to 24 edits and checks every bar: 256 cases are plenty.
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Bar starts and lengths agree with the bar-by-bar oracle on every bar.
    #[test]
    fn bars_match_oracle(edits in prop::collection::vec(edit(), 0..24)) {
        let mut map = MeterMap::new(MeterChange::new(meters()[0])).unwrap();
        for e in &edits {
            apply(&mut map, e);
        }
        for k in 0..BARS {
            let bar = BarIndex::new(k);
            prop_assert_eq!(map.bar_start(bar).unwrap(), naive_start(&map, k), "bar {}", k);
            prop_assert_eq!(map.bar_length(bar).unwrap(), naive_length(&map, k), "bar {}", k);
        }
    }

    /// Points inside the first bars: locate ⇄ position_of are inverse, and beat_of agrees with the
    /// oracle and is inverted by beat_position.
    #[test]
    fn points_round_trip(
        edits in prop::collection::vec(edit(), 0..24),
        points in prop::collection::vec((0..BARS, 0_i64..64), 1..16),
    ) {
        let mut map = MeterMap::new(MeterChange::new(meters()[0])).unwrap();
        for e in &edits {
            apply(&mut map, e);
        }
        for (k, t) in points {
            // A point `t/64` of the way through bar k.
            let length = naive_length(&map, k);
            let offset = length.checked_mul(Rational::new(t, 64).unwrap()).unwrap();
            let position = naive_start(&map, k).checked_add(offset).unwrap();
            let at = map.locate(position).unwrap();
            prop_assert_eq!(at, BarPosition { bar: BarIndex::new(k), offset });
            prop_assert_eq!(map.position_of(at).unwrap(), position);
            let beat = map.beat_of(at).unwrap();
            prop_assert_eq!(beat, naive_beat(&map, k, offset), "bar {} offset {:?}", k, offset);
            if let Some(beat) = beat {
                prop_assert_eq!(map.beat_position(BarIndex::new(k), beat).unwrap(), position);
            }
        }
    }
}
