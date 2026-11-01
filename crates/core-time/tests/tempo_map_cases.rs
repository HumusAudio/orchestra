// SPDX-License-Identifier: GPL-3.0-or-later

//! Hand-written cases of the tempo map (ADR-0020, ADR-0021): every number below is computed by
//! hand in the comments, so a failure points at a precise rule.
//!
//! Speeds are in whole notes per second: ♩ = 120 is 1/2, ♩ = 60 is 1/4.

// Helper functions (not `#[test]` functions) are outside the scope of clippy.toml's
// `allow-unwrap-in-tests`, so the exemption is stated here, for this test file only: a helper that
// unwraps fails the calling test with a clear message, which is exactly what a test wants.
#![allow(
    clippy::unwrap_used,
    reason = "test helpers: a failed unwrap fails the test"
)]
// Same reasoning for the two helpers that fail the test when a time has the wrong kind.
#![allow(clippy::panic, reason = "test helpers: a panic fails the test")]

use orchestra_core_time::{
    Duration, GradualChange, Position, Rational, Seconds, TIME_GRID, Tempo, TempoChange, TempoMap,
    TempoMapError, TimeAt,
};

/// The rational `n/d`.
fn q(n: i64, d: i64) -> Rational {
    Rational::new(n, d).unwrap()
}

/// The position `n/d` whole notes.
fn at(n: i64, d: i64) -> Position {
    Position::from_rational(q(n, d))
}

/// The time `n/d` seconds.
fn secs(n: i64, d: i64) -> Seconds {
    Seconds::from_rational(q(n, d))
}

/// ♩ = `bpm`.
fn quarter(bpm: i64) -> Tempo {
    Tempo::new(Duration::from_rational(q(1, 4)), q(bpm, 1)).unwrap()
}

/// An immediate change.
fn immediate(tempo: Tempo) -> TempoChange {
    TempoChange::Immediate(tempo)
}

/// A gradual change to `to`, ending at `end`, from the tempo in force.
fn gradual(to: Tempo, end: Position) -> TempoChange {
    TempoChange::Gradual(GradualChange {
        from: None,
        to,
        end,
    })
}

/// The exact time of `position`, failing the test if it is approximate.
fn exact(map: &TempoMap, position: Position) -> Seconds {
    match map.time_at(position).unwrap() {
        TimeAt::Exact(time) => time,
        TimeAt::Approximate(time) => panic!("expected an exact time at {position:?}, got {time}"),
    }
}

/// The approximate time of `position`, failing the test if it is exact.
fn approximate(map: &TempoMap, position: Position) -> f64 {
    match map.time_at(position).unwrap() {
        TimeAt::Approximate(time) => time,
        TimeAt::Exact(time) => panic!("expected an approximate time at {position:?}, got {time:?}"),
    }
}

/// At ♩ = 120 a whole note lasts 2 s, in both directions, also before the start.
#[test]
fn constant_tempo_is_exact_both_ways() {
    let map = TempoMap::new(quarter(120)).unwrap();
    for (n, d) in [(0, 1), (1, 4), (7, 12), (100, 1), (-1, 4)] {
        let time = exact(&map, at(n, d));
        assert_eq!(time, secs(2 * n, d));
        assert_eq!(map.position_at(time).unwrap(), at(n, d));
    }
    // ♩. = 80 has the same speed as ♩ = 120.
    let dotted = Tempo::new(Duration::from_rational(q(3, 8)), q(80, 1)).unwrap();
    let same = TempoMap::new(dotted).unwrap();
    assert_eq!(exact(&same, at(3, 1)), secs(6, 1));
}

/// Immediate changes: the times of later positions add up segment by segment.
#[test]
fn immediate_changes_add_up() {
    let mut map = TempoMap::new(quarter(120)).unwrap();
    // From position 2 (4 s), ♩ = 60: one whole note per 4 s.
    assert_eq!(map.set_change(at(2, 1), immediate(quarter(60))), Ok(None));
    assert_eq!(exact(&map, at(2, 1)), secs(4, 1));
    assert_eq!(exact(&map, at(5, 2)), secs(6, 1));
    assert_eq!(map.position_at(secs(6, 1)).unwrap(), at(5, 2));
    // ♩ = 90 from position 3 (8 s): a whole note lasts 8/3 s.
    map.set_change(at(3, 1), immediate(quarter(90))).unwrap();
    assert_eq!(exact(&map, at(4, 1)), secs(8 * 3 + 8, 3));
    assert_eq!(map.position_at(secs(32, 3)).unwrap(), at(4, 1));
}

/// The ritardando of ADR-0020's table: ♩ = 120 → 60 over 16 quarters lasts 32/3 s.
#[test]
fn ritardando_matches_adr_0020() {
    let mut map = TempoMap::new(quarter(120)).unwrap();
    map.set_change(at(1, 1), gradual(quarter(60), at(5, 1)))
        .unwrap();
    // Start of the change: 2 s, exact. End: 2 + 32/3 = 38/3 s, exact (on the grid).
    assert_eq!(exact(&map, at(1, 1)), secs(2, 1));
    assert_eq!(exact(&map, at(5, 1)), secs(38, 3));
    // After the change ♩ = 60 holds: one more whole note is 4 s.
    assert_eq!(exact(&map, at(6, 1)), secs(50, 3));

    // Inside: s₀ = 1/2, h = (s₁² − s₀²)/(4L) = (1/16 − 1/4)/16 = −3/256. Two whole notes into the
    // change, the integral of 1/s(x) is (√(s₀² + 4hx) − s₀)/(2h): another formula than the
    // library's, so it checks the library's algebra too.
    let (s0, h, x) = (0.5_f64, -3.0 / 256.0, 2.0);
    let expected = 2.0 + ((s0 * s0 + 4.0 * h * x).sqrt() - s0) / (2.0 * h);
    let got = approximate(&map, at(3, 1));
    assert!((got - expected).abs() < 1e-12, "{got} vs {expected}");

    // Seconds → position, one second into the change: x = s₀·1 + h·1 = 1/2 − 3/256 = 125/256,
    // which lies on the grid (256 divides 705 600 000), so the rounding changes nothing.
    assert_eq!(map.position_at(secs(3, 1)).unwrap(), at(256 + 125, 256));
    // Past the end, the constant tempo again.
    assert_eq!(map.position_at(secs(50, 3)).unwrap(), at(6, 1));
}

/// Inside a gradual change, the position of an arbitrary time is rounded to the grid.
#[test]
fn positions_inside_a_change_are_rounded_to_the_grid() {
    // The ritardando of the previous test: s₀ = 1/2, h = −3/256, starting at 2 s.
    let mut map = TempoMap::new(quarter(120)).unwrap();
    map.set_change(at(1, 1), gradual(quarter(60), at(5, 1)))
        .unwrap();
    // τ = 1/3 s into the change: x = (1/2)(1/3) − (3/256)(1/9) = 1/6 − 1/768 = 127/768.
    // 768 = 2⁸·3 divides the grid, so this is exact as well.
    assert_eq!(map.position_at(secs(7, 3)).unwrap(), at(768 + 127, 768));
    // τ = 1/7 s: x = 1/14 − 3/12 544 = 893/12 544... 12 544 = 2⁸·7², divides the grid: exact.
    assert_eq!(
        map.position_at(secs(15, 7)).unwrap(),
        Position::from_rational(q(1, 1).checked_add(q(893, 12_544)).unwrap())
    );
    // τ = 1/11 s: 11 does not divide the grid. x = 1/22 − 3/30 976 = 1405/30 976; on the grid it
    // is 1405·705 600 000 / 30 976 = 32 004 390.495… ticks → 32 004 390 (nearest).
    let got = map.position_at(secs(23, 11)).unwrap();
    let expected = q(1, 1).checked_add(q(32_004_390, TIME_GRID)).unwrap();
    assert_eq!(got, Position::from_rational(expected));
}

/// A boundary whose exact time is not on the grid is rounded to the nearest tick, and both
/// conversions clamp to the segment (ADR-0021 §1, §2).
#[test]
fn boundaries_round_to_the_grid_and_clamp() {
    // ♩ = 77 for one whole note: 240/77 s = 2 199 272 727.27… ticks → 2 199 272 727 (down).
    let mut map = TempoMap::new(quarter(77)).unwrap();
    map.set_change(at(1, 1), immediate(quarter(120))).unwrap();
    let boundary = secs(2_199_272_727, TIME_GRID);
    assert_eq!(exact(&map, at(1, 1)), boundary);
    // A position a hair before the boundary would land 0.27 ticks after it: clamped.
    let hair = Position::from_rational(q(1, 1).checked_sub(q(1, 1 << 40)).unwrap());
    assert_eq!(exact(&map, hair), boundary);
    // The next segment continues from the rounded boundary.
    assert_eq!(
        exact(&map, at(2, 1)),
        Seconds::from_rational(boundary.as_rational().checked_add(q(2, 1)).unwrap())
    );

    // ♩ = 71 for one whole note: 240/71 s = 2 385 126 760.56… ticks → 2 385 126 761 (up).
    let mut map = TempoMap::new(quarter(71)).unwrap();
    map.set_change(at(1, 1), immediate(quarter(120))).unwrap();
    assert_eq!(exact(&map, at(1, 1)), secs(2_385_126_761, TIME_GRID));
    // A time between the exact end (…760.56) and the rounded start (…761) would map past position
    // 1: clamped to it.
    let between = secs(2_385_126_760 * 5 + 4, TIME_GRID * 5);
    assert_eq!(map.position_at(between).unwrap(), at(1, 1));
}

/// "Meno mosso, poco a poco accel.": a jump to the starting tempo, then the gradual change.
#[test]
fn gradual_change_with_a_starting_tempo() {
    let mut map = TempoMap::new(quarter(120)).unwrap();
    let change = TempoChange::Gradual(GradualChange {
        from: Some(quarter(60)),
        to: quarter(120),
        end: at(4, 1),
    });
    map.set_change(at(1, 1), change).unwrap();
    // 2 s to position 1; then from 1/4 to 1/2 over 3 whole notes: D = 6/(3/4) = 8 s.
    assert_eq!(exact(&map, at(1, 1)), secs(2, 1));
    assert_eq!(exact(&map, at(4, 1)), secs(10, 1));
    // The speed starts at 1/4: very shortly after the start the slope is that of ♩ = 60.
    let near = approximate(
        &map,
        Position::from_rational(q(1, 1).checked_add(q(1, 1000)).unwrap()),
    );
    assert!((near - (2.0 + 0.004)).abs() < 1e-6, "{near}");
}

/// Back-to-back gradual changes, and "rit. … a tempo" (an immediate change exactly at the end).
#[test]
fn adjacent_changes() {
    let mut map = TempoMap::new(quarter(120)).unwrap();
    // accel. 120 → 240 over 3 whole notes (D = 6/(3/2) = 4 s), then rit. 240 → 120 over 3
    // (4 s again), then a tempo ♩ = 60 at once.
    map.set_change(at(1, 1), gradual(quarter(240), at(4, 1)))
        .unwrap();
    map.set_change(at(4, 1), gradual(quarter(120), at(7, 1)))
        .unwrap();
    map.set_change(at(7, 1), immediate(quarter(60))).unwrap();
    assert_eq!(exact(&map, at(1, 1)), secs(2, 1));
    assert_eq!(exact(&map, at(4, 1)), secs(6, 1));
    assert_eq!(exact(&map, at(7, 1)), secs(10, 1));
    assert_eq!(exact(&map, at(8, 1)), secs(14, 1));
    // The second change starts from the first one's target (240), not from the initial 120: at its
    // very start the slope is 1 whole note per second.
    let near = approximate(
        &map,
        Position::from_rational(q(4, 1).checked_add(q(1, 1000)).unwrap()),
    );
    assert!((near - 6.001).abs() < 1e-6, "{near}");
}

/// Invalid edits fail with their typed error and leave the map unchanged.
#[test]
fn invalid_edits_are_rejected_all_or_nothing() {
    let mut map = TempoMap::new(quarter(120)).unwrap();
    map.set_change(at(2, 1), gradual(quarter(60), at(4, 1)))
        .unwrap();
    let before = map.clone();
    let cases = [
        (
            at(3, 1),
            immediate(quarter(90)),
            TempoMapError::ChangeInsideGradual {
                at: at(3, 1),
                gradual: at(2, 1),
            },
        ),
        // A new gradual change that would contain the existing one.
        (
            at(1, 1),
            gradual(quarter(90), at(3, 1)),
            TempoMapError::ChangeInsideGradual {
                at: at(2, 1),
                gradual: at(1, 1),
            },
        ),
        (
            at(5, 1),
            gradual(quarter(90), at(5, 1)),
            TempoMapError::NonPositiveGradual { at: at(5, 1) },
        ),
        (
            at(5, 1),
            gradual(quarter(90), at(4, 1)),
            TempoMapError::NonPositiveGradual { at: at(5, 1) },
        ),
        (
            at(0, 1),
            gradual(quarter(90), at(1, 1)),
            TempoMapError::GradualAtStart,
        ),
        (
            at(-1, 4),
            immediate(quarter(90)),
            TempoMapError::BeforeStart,
        ),
    ];
    for (position, change, error) in cases {
        assert_eq!(map.set_change(position, change), Err(error));
        assert_eq!(map, before, "a failed edit changed the map");
    }
    assert_eq!(
        map.remove_change(at(0, 1)),
        Err(TempoMapError::NoTempoAtStart)
    );
    assert_eq!(map, before);
}

/// Edits return the previous value, as an undo needs.
#[test]
fn edits_return_the_previous_value() {
    let mut map = TempoMap::new(quarter(120)).unwrap();
    assert_eq!(
        map.set_change(at(0, 1), immediate(quarter(100))),
        Ok(Some(immediate(quarter(120))))
    );
    assert_eq!(map.set_change(at(1, 1), immediate(quarter(60))), Ok(None));
    assert_eq!(
        map.set_change(at(1, 1), immediate(quarter(90))),
        Ok(Some(immediate(quarter(60))))
    );
    assert_eq!(
        map.remove_change(at(1, 1)),
        Ok(Some(immediate(quarter(90))))
    );
    assert_eq!(map.remove_change(at(1, 1)), Ok(None));
    assert_eq!(map.changes(), &[(at(0, 1), immediate(quarter(100)))]);
    // Undoing everything gives back the original map.
    map.set_change(at(0, 1), immediate(quarter(120))).unwrap();
    assert_eq!(map, TempoMap::new(quarter(120)).unwrap());
}

/// Removing the change after a gradual one makes the implicit constant segment appear.
#[test]
fn removing_the_change_after_a_gradual_one() {
    let mut map = TempoMap::new(quarter(120)).unwrap();
    map.set_change(at(1, 1), gradual(quarter(60), at(5, 1)))
        .unwrap();
    map.set_change(at(5, 1), immediate(quarter(240))).unwrap();
    assert_eq!(exact(&map, at(6, 1)), secs(38 + 3, 3));
    map.remove_change(at(5, 1)).unwrap();
    // ♩ = 60 holds after the change again.
    assert_eq!(exact(&map, at(6, 1)), secs(50, 3));
}

/// Before the start, the first tempo extends backwards.
#[test]
fn before_the_start() {
    let mut map = TempoMap::new(quarter(60)).unwrap();
    map.set_change(at(1, 1), gradual(quarter(120), at(2, 1)))
        .unwrap();
    assert_eq!(exact(&map, at(-1, 2)), secs(-2, 1));
    assert_eq!(map.position_at(secs(-1, 1)).unwrap(), at(-1, 4));
}

/// Both conversions are non-decreasing across every kind of segment and boundary.
#[test]
fn conversions_are_monotonic() {
    let mut map = TempoMap::new(quarter(77)).unwrap();
    map.set_change(at(1, 1), gradual(quarter(131), at(3, 1)))
        .unwrap();
    map.set_change(at(3, 1), gradual(quarter(53), at(9, 2)))
        .unwrap();
    map.set_change(at(6, 1), immediate(quarter(97))).unwrap();
    let mut last_time = f64::NEG_INFINITY;
    let mut last_position = at(-10, 1);
    for k in -48..=8 * 48 {
        let position = at(k, 48);
        let time = map.time_at(position).unwrap().to_f64();
        assert!(time >= last_time, "time went back at {position:?}");
        last_time = time;
        let back = map.position_at(secs(k, 30)).unwrap();
        assert!(back >= last_position, "position went back at {k}/30 s");
        last_position = back;
    }
}
