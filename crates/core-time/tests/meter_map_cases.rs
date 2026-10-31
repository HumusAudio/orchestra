// SPDX-License-Identifier: GPL-3.0-or-later

//! Hand-written cases of [`Meter`], [`BeatGrouping`] and [`MeterMap`] (ADR-0018, ADR-0019): the
//! list of SCALETTA F1.04b, each written out so that a reader can check it on paper.
//!
//! Positions and lengths are in whole notes: in 4/4 bar *k* starts at *k*, a quarter is 1/4.
//! Beat numbers are 0-based: the musician's "beat 4" is beat 3.

// Helper functions (not `#[test]` functions) are outside the scope of clippy.toml's
// `allow-unwrap-in-tests`, so the exemption is stated here, for this test file only: a helper that
// unwraps fails the calling test with a clear message, which is exactly what a test wants.
#![allow(
    clippy::unwrap_used,
    reason = "test helpers: a failed unwrap fails the test"
)]

use orchestra_core_time::{
    BarIndex, BarKind, BarPosition, BeatGrouping, BeatPosition, Duration, IrregularBar, Meter,
    MeterChange, MeterError, MeterMap, MeterMapError, Position, Rational,
};

/// Shorthand: the rational `n/d`.
fn r(n: i64, d: i64) -> Rational {
    Rational::new(n, d).unwrap()
}

/// Shorthand: a duration of `n/d` whole notes.
fn d(n: i64, den: i64) -> Duration {
    Duration::from_rational(r(n, den))
}

/// Shorthand: the position `n/d` whole notes from the start.
fn p(n: i64, den: i64) -> Position {
    Position::from_rational(r(n, den))
}

/// Shorthand: bar `k`.
fn bar(k: u32) -> BarIndex {
    BarIndex::new(k)
}

/// Shorthand: a meter.
fn meter(groups: &[u8], den: u16) -> Meter {
    Meter::new(groups, den).unwrap()
}

/// Shorthand: a grouping.
fn grouping(groups: &[u8], unit: u16) -> BeatGrouping {
    BeatGrouping::new(groups, unit).unwrap()
}

/// A map with one meter for the whole score, counted with the default rule.
fn map_of(groups: &[u8], den: u16) -> MeterMap {
    MeterMap::new(MeterChange::new(meter(groups, den))).unwrap()
}

/// The beat (and offset in the beat) of the point `offset` inside `k`.
fn beat_at(map: &MeterMap, k: u32, offset: Duration) -> Option<(u32, Duration)> {
    map.beat_of(BarPosition {
        bar: bar(k),
        offset,
    })
    .unwrap()
    .map(|b| (b.beat, b.offset))
}

/// An irregular bar.
fn irregular(length: Duration, kind: BarKind) -> IrregularBar {
    IrregularBar { length, kind }
}

// ------------------------------------------------------------------------------------------------
// Meter and BeatGrouping
// ------------------------------------------------------------------------------------------------

/// Invalid meters and groupings are typed errors, checked in a fixed order.
#[test]
fn meter_and_grouping_errors() {
    assert_eq!(Meter::new(&[], 4), Err(MeterError::NoGroups));
    assert_eq!(Meter::new(&[4], 0), Err(MeterError::ZeroDenominator));
    assert_eq!(Meter::new(&[3, 0, 2], 8), Err(MeterError::ZeroGroup));
    assert_eq!(Meter::new(&[1; 9], 8), Err(MeterError::TooManyGroups));
    assert!(Meter::new(&[1; 8], 8).is_ok());
    assert_eq!(
        BeatGrouping::new(&[1; 17], 8),
        Err(MeterError::TooManyGroups)
    );
    assert!(BeatGrouping::new(&[1; 16], 8).is_ok());
    assert_eq!(
        BeatGrouping::new(&[3, 3], 0),
        Err(MeterError::ZeroDenominator)
    );
}

/// Accessors, lengths and the `Debug` form.
#[test]
fn meter_accessors() {
    let additive = meter(&[3, 2, 2], 8);
    assert_eq!(additive.groups(), &[3, 2, 2]);
    assert_eq!(additive.numerator(), 7);
    assert_eq!(additive.denominator(), 8);
    assert!(additive.is_additive());
    assert!(!meter(&[7], 8).is_additive());
    assert_eq!(additive.nominal_length().unwrap(), d(7, 8));
    // The largest meter: 8 groups of 255 sixteenths.
    assert_eq!(meter(&[255; 8], 16).nominal_length().unwrap(), d(2040, 16));
    assert_eq!(format!("{additive:?}"), "Meter(3+2+2/8)");
    assert_eq!(format!("{:?}", meter(&[4], 4)), "Meter(4/4)");
    let g = grouping(&[3, 2, 2, 2], 8);
    assert_eq!(g.beat_count(), 4);
    assert_eq!(g.unit(), 8);
    assert_eq!(g.total().unwrap(), d(9, 8));
    assert_eq!(format!("{g:?}"), "BeatGrouping(3+2+2+2/8)");
    // Equal meters are equal, whatever was in the (zeroed) unused slots.
    assert_eq!(meter(&[3, 2, 2], 8), meter(&[3, 2, 2], 8));
    assert_ne!(meter(&[3, 2, 2], 8), meter(&[2, 2, 3], 8));
}

// ------------------------------------------------------------------------------------------------
// Bars and positions
// ------------------------------------------------------------------------------------------------

/// Plain 4/4: bar k starts at k, positions locate in the right bar.
#[test]
fn plain_four_four() {
    let map = map_of(&[4], 4);
    assert_eq!(map.bar_start(bar(0)).unwrap(), Position::ZERO);
    assert_eq!(map.bar_start(bar(3)).unwrap(), p(3, 1));
    assert_eq!(map.bar_length(bar(1000)).unwrap(), d(1, 1));
    let at = map.locate(p(5, 2)).unwrap();
    assert_eq!(
        at,
        BarPosition {
            bar: bar(2),
            offset: d(1, 2)
        }
    );
    assert_eq!(map.position_of(at).unwrap(), p(5, 2));
    // Exactly on a barline: the new bar, offset zero.
    assert_eq!(
        map.locate(p(2, 1)).unwrap(),
        BarPosition {
            bar: bar(2),
            offset: Duration::ZERO
        }
    );
    assert_eq!(map.display_number(bar(0)).unwrap(), Some(1));
    assert_eq!(map.display_number(bar(9)).unwrap(), Some(10));
}

/// Meter changes: 4/4 for bars 0–1, 3/4 for bars 2–3, 6/8 from bar 4.
#[test]
fn meter_changes() {
    let mut map = map_of(&[4], 4);
    map.set_meter_change(bar(2), MeterChange::new(meter(&[3], 4)))
        .unwrap();
    map.set_meter_change(bar(4), MeterChange::new(meter(&[6], 8)))
        .unwrap();
    assert_eq!(map.bar_start(bar(2)).unwrap(), p(2, 1));
    assert_eq!(map.bar_start(bar(3)).unwrap(), p(11, 4));
    assert_eq!(map.bar_start(bar(4)).unwrap(), p(7, 2));
    assert_eq!(map.bar_start(bar(5)).unwrap(), p(17, 4));
    assert_eq!(map.meter_at(bar(3)).unwrap(), meter(&[3], 4));
    assert_eq!(map.meter_at(bar(100)).unwrap(), meter(&[6], 8));
    assert_eq!(
        map.locate(p(3, 1)).unwrap(),
        BarPosition {
            bar: bar(3),
            offset: d(1, 4)
        }
    );
    // Removing the 3/4 makes bars 2–3 4/4 again, so 6/8 starts at 4.
    assert_eq!(
        map.remove_meter_change(bar(2)).unwrap(),
        Some(MeterChange::new(meter(&[3], 4)))
    );
    assert_eq!(map.bar_start(bar(4)).unwrap(), p(4, 1));
}

/// Pickup (anacrusis): a quarter in 4/4, numbered 0, counted as beat 3 (the musician's beat 4).
#[test]
fn pickup_bar() {
    let mut map = map_of(&[4], 4);
    map.set_irregular_bar(bar(0), irregular(d(1, 4), BarKind::AlignEnd))
        .unwrap();
    assert_eq!(map.bar_length(bar(0)).unwrap(), d(1, 4));
    assert_eq!(map.bar_start(bar(1)).unwrap(), p(1, 4));
    assert_eq!(map.bar_start(bar(2)).unwrap(), p(5, 4));
    assert_eq!(beat_at(&map, 0, Duration::ZERO), Some((3, Duration::ZERO)));
    assert_eq!(map.beat_range(bar(0)).unwrap(), Some(3..4));
    assert_eq!(
        map.beat_position(
            bar(0),
            BeatPosition {
                beat: 3,
                offset: Duration::ZERO
            }
        )
        .unwrap(),
        Position::ZERO
    );
    // Beat 0 of the pickup lies before the bar.
    assert_eq!(
        map.beat_position(
            bar(0),
            BeatPosition {
                beat: 0,
                offset: Duration::ZERO
            }
        ),
        Err(MeterMapError::BeatOutsideBar)
    );
    assert_eq!(map.display_number(bar(0)).unwrap(), Some(0));
    assert_eq!(map.display_number(bar(1)).unwrap(), Some(1));
    assert_eq!(map.display_number(bar(2)).unwrap(), Some(2));
    // An eighth pickup is the second half of beat 3: no beat starts in it.
    map.set_irregular_bar(bar(0), irregular(d(1, 8), BarKind::AlignEnd))
        .unwrap();
    assert_eq!(beat_at(&map, 0, Duration::ZERO), Some((3, d(1, 8))));
    assert_eq!(map.beat_range(bar(0)).unwrap(), Some(4..4));
}

/// A pickup longer than its meter is rejected, and the map is left unchanged.
#[test]
fn pickup_longer_than_meter_is_rejected() {
    let mut map = map_of(&[3], 4);
    let before = map.clone();
    assert_eq!(
        map.set_irregular_bar(bar(0), irregular(d(1, 1), BarKind::AlignEnd)),
        Err(MeterMapError::PickupLongerThanMeter { bar: bar(0) })
    );
    assert_eq!(map, before);
    // A valid pickup, then a meter change that would make it too long: rejected as a whole.
    map.set_irregular_bar(bar(0), irregular(d(1, 2), BarKind::AlignEnd))
        .unwrap();
    let before = map.clone();
    assert_eq!(
        map.set_meter_change(bar(0), MeterChange::new(meter(&[2], 8))),
        Err(MeterMapError::PickupLongerThanMeter { bar: bar(0) })
    );
    assert_eq!(map, before);
    // Lengths must be positive.
    assert_eq!(
        map.set_irregular_bar(bar(4), irregular(Duration::ZERO, BarKind::AlignStart)),
        Err(MeterMapError::NonPositiveLength { bar: bar(4) })
    );
}

// ------------------------------------------------------------------------------------------------
// Beats: default rule
// ------------------------------------------------------------------------------------------------

/// Simple meters: one beat per unit (3/4, 3/8, and 7/8 by default, ADR-0019 §6).
#[test]
fn simple_meters() {
    let map = map_of(&[3], 4);
    assert_eq!(map.beat_range(bar(0)).unwrap(), Some(0..3));
    assert_eq!(beat_at(&map, 0, d(5, 8)), Some((2, d(1, 8))));
    assert_eq!(map_of(&[3], 8).beat_range(bar(0)).unwrap(), Some(0..3));
    // The default for 7/8 is seven eighths, until the user chooses a grouping.
    assert_eq!(map_of(&[7], 8).beat_range(bar(0)).unwrap(), Some(0..7));
    assert_eq!(map_of(&[5], 8).beat_range(bar(0)).unwrap(), Some(0..5));
}

/// Compound meters: 6/8 in two, 9/8 in three, 12/8 in four, 6/4 in two dotted halves.
#[test]
fn compound_meters() {
    let map = map_of(&[6], 8);
    assert_eq!(map.beat_range(bar(0)).unwrap(), Some(0..2));
    assert_eq!(beat_at(&map, 0, d(3, 8)), Some((1, Duration::ZERO)));
    assert_eq!(beat_at(&map, 0, d(1, 2)), Some((1, d(1, 8))));
    assert_eq!(map_of(&[9], 8).beat_range(bar(0)).unwrap(), Some(0..3));
    assert_eq!(map_of(&[12], 8).beat_range(bar(0)).unwrap(), Some(0..4));
    let six_four = map_of(&[6], 4);
    assert_eq!(beat_at(&six_four, 0, d(3, 4)), Some((1, Duration::ZERO)));
}

/// Additive meters: one beat per written group (3+2+2/8).
#[test]
fn additive_meter() {
    let map = map_of(&[3, 2, 2], 8);
    assert_eq!(map.bar_length(bar(0)).unwrap(), d(7, 8));
    assert_eq!(map.bar_start(bar(2)).unwrap(), p(7, 4));
    assert_eq!(map.beat_range(bar(0)).unwrap(), Some(0..3));
    assert_eq!(beat_at(&map, 0, Duration::ZERO), Some((0, Duration::ZERO)));
    assert_eq!(beat_at(&map, 0, d(3, 8)), Some((1, Duration::ZERO)));
    assert_eq!(beat_at(&map, 0, d(5, 8)), Some((2, Duration::ZERO)));
    assert_eq!(beat_at(&map, 0, d(6, 8)), Some((2, d(1, 8))));
    assert_eq!(
        map.beat_position(
            bar(1),
            BeatPosition {
                beat: 2,
                offset: Duration::ZERO
            }
        )
        .unwrap(),
        p(7, 8).checked_add(d(5, 8)).unwrap()
    );
}

/// A denominator that is not a power of two (D04): 4/3, four triplet half notes.
#[test]
fn non_binary_meter() {
    let map = map_of(&[4], 3);
    assert_eq!(map.bar_length(bar(0)).unwrap(), d(4, 3));
    assert_eq!(map.bar_start(bar(3)).unwrap(), p(4, 1));
    assert_eq!(map.beat_range(bar(0)).unwrap(), Some(0..4));
    assert_eq!(beat_at(&map, 1, d(2, 3)), Some((2, Duration::ZERO)));
}

/// A bar longer than its meter (aligned at the start): the pattern starts again, beat numbers
/// continue, the last beat is cut by the barline (ADR-0019 §4, option "a").
#[test]
fn bar_longer_than_meter() {
    let mut map = map_of(&[4], 4);
    map.set_irregular_bar(bar(1), irregular(d(5, 4), BarKind::AlignStart))
        .unwrap();
    assert_eq!(map.beat_range(bar(1)).unwrap(), Some(0..5));
    assert_eq!(beat_at(&map, 1, d(1, 1)), Some((4, Duration::ZERO)));
    assert_eq!(map.bar_start(bar(2)).unwrap(), p(9, 4));
    // 3+2+2/8 lengthened to 9/8: beat 3 is a new 3/8 group, cut to 2/8 by the barline.
    let mut map = map_of(&[3, 2, 2], 8);
    map.set_irregular_bar(bar(0), irregular(d(9, 8), BarKind::AlignStart))
        .unwrap();
    assert_eq!(beat_at(&map, 0, d(7, 8)), Some((3, Duration::ZERO)));
    assert_eq!(beat_at(&map, 0, d(8, 8)), Some((3, d(1, 8))));
    assert_eq!(map.beat_range(bar(0)).unwrap(), Some(0..4));
}

// ------------------------------------------------------------------------------------------------
// Free bars (meter suspended)
// ------------------------------------------------------------------------------------------------

/// A free cadenza: no beats, explicit length, the meter resumes afterwards, counted by default.
#[test]
fn free_cadenza() {
    let mut map = map_of(&[4], 4);
    map.set_irregular_bar(bar(2), irregular(d(7, 2), BarKind::Free))
        .unwrap();
    assert!(map.is_free(bar(2)).unwrap());
    assert!(!map.is_free(bar(3)).unwrap());
    assert_eq!(map.bar_start(bar(3)).unwrap(), p(11, 2));
    assert_eq!(beat_at(&map, 2, d(3, 1)), None);
    assert_eq!(map.beat_range(bar(2)).unwrap(), None);
    assert_eq!(
        map.beat_position(
            bar(2),
            BeatPosition {
                beat: 0,
                offset: Duration::ZERO
            }
        ),
        Err(MeterMapError::NoBeats)
    );
    // The meter is still recorded during the cadenza, and resumes without a new meter change.
    assert_eq!(map.meter_at(bar(2)).unwrap(), meter(&[4], 4));
    assert_eq!(beat_at(&map, 3, d(1, 4)), Some((1, Duration::ZERO)));
    // A position inside the cadenza is found normally.
    assert_eq!(
        map.locate(p(5, 1)).unwrap(),
        BarPosition {
            bar: bar(2),
            offset: d(3, 1)
        }
    );
    // Counted by default (owner's choice): bar 2 is number 3, bar 3 number 4.
    assert_eq!(map.display_number(bar(2)).unwrap(), Some(3));
    assert_eq!(map.display_number(bar(3)).unwrap(), Some(4));
    // ...unless marked uncounted.
    assert!(!map.set_uncounted(bar(2), true));
    assert_eq!(map.display_number(bar(2)).unwrap(), None);
    assert_eq!(map.display_number(bar(3)).unwrap(), Some(3));
}

/// A free passage over several bars, and a meter change that takes effect after it.
#[test]
fn free_passage_over_several_bars() {
    let mut map = map_of(&[4], 4);
    map.set_irregular_bar(bar(2), irregular(d(2, 1), BarKind::Free))
        .unwrap();
    map.set_irregular_bar(bar(3), irregular(d(5, 4), BarKind::Free))
        .unwrap();
    assert_eq!(
        map.bar_start(bar(4)).unwrap(),
        p(2 + 2, 1).checked_add(d(5, 4)).unwrap()
    );
    // A meter change on a free bar is "the meter after the cadenza".
    map.set_meter_change(bar(3), MeterChange::new(meter(&[3], 4)))
        .unwrap();
    assert!(map.is_free(bar(3)).unwrap());
    assert_eq!(map.beat_range(bar(4)).unwrap(), Some(0..3));
    // A per-bar grouping on a free bar is rejected.
    assert_eq!(
        map.set_bar_grouping(bar(2), grouping(&[2, 2], 4)),
        Err(MeterMapError::GroupingOnFreeBar { bar: bar(2) })
    );
}

// ------------------------------------------------------------------------------------------------
// Beat groupings (ADR-0019)
// ------------------------------------------------------------------------------------------------

/// A section grouping (9/8 counted 3+2+2+2), and a per-bar grouping that wins over it.
#[test]
fn section_and_bar_groupings() {
    let nine_eight = meter(&[9], 8);
    let mut map = MeterMap::new(MeterChange::with_grouping(
        nine_eight,
        grouping(&[3, 2, 2, 2], 8),
    ))
    .unwrap();
    assert_eq!(map.beat_range(bar(0)).unwrap(), Some(0..4));
    assert_eq!(beat_at(&map, 0, d(3, 8)), Some((1, Duration::ZERO)));
    assert_eq!(beat_at(&map, 0, d(5, 8)), Some((2, Duration::ZERO)));
    assert_eq!(beat_at(&map, 0, d(8, 8)), Some((3, d(1, 8))));
    // Bar 1 counted 3+3+3 again, just that bar.
    assert_eq!(
        map.set_bar_grouping(bar(1), grouping(&[3, 3, 3], 8))
            .unwrap(),
        None
    );
    assert_eq!(map.beat_range(bar(1)).unwrap(), Some(0..3));
    assert_eq!(map.beat_range(bar(2)).unwrap(), Some(0..4));
    // The bar's length and position are not affected by how it is counted.
    assert_eq!(map.bar_start(bar(2)).unwrap(), p(9, 4));
    assert_eq!(
        map.remove_bar_grouping(bar(1)).unwrap(),
        Some(grouping(&[3, 3, 3], 8))
    );
    assert_eq!(map.beat_range(bar(1)).unwrap(), Some(0..4));
}

/// Hemiola: a 3/4 bar counted as two dotted quarters, with a unit of its own (ADR-0019 §2).
#[test]
fn hemiola() {
    let mut map = map_of(&[3], 4);
    map.set_bar_grouping(bar(1), grouping(&[3, 3], 8)).unwrap();
    assert_eq!(map.beat_range(bar(1)).unwrap(), Some(0..2));
    assert_eq!(beat_at(&map, 1, d(3, 8)), Some((1, Duration::ZERO)));
    assert_eq!(map.beat_range(bar(0)).unwrap(), Some(0..3));
}

/// A grouping must add up to the nominal length; an edit that breaks an existing grouping fails.
#[test]
fn grouping_length_is_checked() {
    assert_eq!(
        MeterMap::new(MeterChange::with_grouping(
            meter(&[9], 8),
            grouping(&[3, 2], 8)
        )),
        Err(MeterMapError::GroupingLengthMismatch { bar: bar(0) })
    );
    let mut map = map_of(&[7], 8);
    map.set_bar_grouping(bar(2), grouping(&[2, 2, 3], 8))
        .unwrap();
    let before = map.clone();
    // Changing the meter under the grouping would leave it the wrong length: rejected.
    assert_eq!(
        map.set_meter_change(bar(1), MeterChange::new(meter(&[3], 4))),
        Err(MeterMapError::GroupingLengthMismatch { bar: bar(2) })
    );
    assert_eq!(map, before);
}

/// A grouping on a pickup counts from the virtual start of the nominal bar, like the default.
#[test]
fn grouping_on_a_pickup() {
    let mut map = map_of(&[7], 8);
    map.set_irregular_bar(bar(0), irregular(d(2, 8), BarKind::AlignEnd))
        .unwrap();
    map.set_bar_grouping(bar(0), grouping(&[2, 2, 3], 8))
        .unwrap();
    // The pickup is the last 2/8 of a 2+2+3 bar: inside beat 2 (which starts 1/8 before it).
    assert_eq!(beat_at(&map, 0, Duration::ZERO), Some((2, d(1, 8))));
    assert_eq!(map.beat_range(bar(0)).unwrap(), Some(3..3));
}

// ------------------------------------------------------------------------------------------------
// Numbering, editing, errors
// ------------------------------------------------------------------------------------------------

/// Uncounted bars and restarts.
#[test]
fn numbering_rules() {
    let mut map = map_of(&[4], 4);
    map.set_uncounted(bar(3), true);
    // Numbers: 1, 2, 3, —, 4, …
    assert_eq!(map.display_number(bar(2)).unwrap(), Some(3));
    assert_eq!(map.display_number(bar(3)).unwrap(), None);
    assert_eq!(map.display_number(bar(4)).unwrap(), Some(4));
    // A new movement restarts at bar 6 with number 1.
    assert_eq!(map.set_numbering_restart(bar(6), Some(1)), None);
    assert_eq!(map.display_number(bar(5)).unwrap(), Some(5));
    assert_eq!(map.display_number(bar(6)).unwrap(), Some(1));
    assert_eq!(map.display_number(bar(8)).unwrap(), Some(3));
    // A restart on an uncounted bar numbers the first counted bar after it.
    map.set_uncounted(bar(6), true);
    assert_eq!(map.display_number(bar(6)).unwrap(), None);
    assert_eq!(map.display_number(bar(7)).unwrap(), Some(1));
    // Removing rules returns their previous state.
    assert_eq!(map.set_numbering_restart(bar(6), None), Some(1));
    assert!(map.set_uncounted(bar(6), false));
    assert!(!map.set_uncounted(bar(6), false));
    assert_eq!(map.uncounted_bars(), &[bar(3)]);
    assert_eq!(map.numbering_restarts(), &[]);
}

/// Edits return the previous value; bar 0 always keeps a meter.
#[test]
fn edits_return_previous_values() {
    let mut map = map_of(&[4], 4);
    assert_eq!(
        map.set_meter_change(bar(0), MeterChange::new(meter(&[3], 4)))
            .unwrap(),
        Some(MeterChange::new(meter(&[4], 4)))
    );
    assert_eq!(
        map.remove_meter_change(bar(0)),
        Err(MeterMapError::NoMeterAtStart)
    );
    assert_eq!(map.remove_meter_change(bar(5)).unwrap(), None);
    let pickup = irregular(d(1, 4), BarKind::AlignEnd);
    assert_eq!(map.set_irregular_bar(bar(0), pickup).unwrap(), None);
    assert_eq!(map.irregular_bars(), &[(bar(0), pickup)]);
    assert_eq!(map.remove_irregular_bar(bar(0)).unwrap(), Some(pickup));
    assert_eq!(map.remove_irregular_bar(bar(0)).unwrap(), None);
    assert_eq!(map.meter_changes().len(), 1);
    assert_eq!(map.bar_groupings(), &[]);
}

/// Lookup errors: before the start, outside the bar, beyond the representable bars.
#[test]
fn lookup_errors() {
    let map = map_of(&[4], 4);
    assert_eq!(map.locate(p(-1, 8)), Err(MeterMapError::BeforeStart));
    let outside = BarPosition {
        bar: bar(1),
        offset: d(1, 1),
    };
    assert_eq!(map.beat_of(outside), Err(MeterMapError::OffsetOutsideBar));
    assert_eq!(
        map.position_of(outside),
        Err(MeterMapError::OffsetOutsideBar)
    );
    let negative = BarPosition {
        bar: bar(1),
        offset: d(-1, 4),
    };
    assert_eq!(
        map.position_of(negative),
        Err(MeterMapError::OffsetOutsideBar)
    );
    // 2⁴⁰ whole notes in 4/4 is bar 2⁴⁰, beyond `u32`.
    assert_eq!(
        map.locate(p(1 << 40, 1)),
        Err(MeterMapError::IndexOutOfRange)
    );
    // The last representable bar still has a start.
    assert_eq!(
        map.bar_start(bar(u32::MAX)).unwrap(),
        p(i64::from(u32::MAX), 1)
    );
    assert_eq!(
        format!("{}", MeterMapError::NoBeats),
        "a free bar has no beats"
    );
}
