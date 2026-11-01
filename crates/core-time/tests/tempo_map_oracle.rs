// SPDX-License-Identifier: GPL-3.0-or-later

//! Property tests of [`TempoMap`] against a naive oracle (ADR-0020, ADR-0021, ADR-0016 §4).
//!
//! # The oracle
//! The library derives segments once, keeps start times as integer ticks, finds segments with
//! binary searches and computes inside gradual changes with the "half acceleration" `h` and a
//! hand-expanded `i128` polynomial. The oracle shares none of that code and as few of its choices
//! as the specification allows:
//!
//! - it reads the **source data** back from [`TempoMap::changes`] and rebuilds its own segments
//!   with a linear walk;
//! - it computes with its own `i128` fraction type ([`Q`], reduced with Euclid's algorithm), so
//!   it does not depend on `Rational` either;
//! - it finds the segment of every query with a **linear scan**;
//! - inside a gradual change it uses the trapezoid form `x(τ) = τ·(s₀ + s(τ))/2`, with
//!   `s(τ) = s₀ + a·τ` and `a = (s₁ − s₀)/D`, `D = L / ((s₀ + s₁)/2)`; for the approximate times
//!   it uses the integral `(√(s₀² + 2a·x) − s₀)/a` instead of the library's cancellation-free form;
//! - it rounds to the grid with the textbook `⌊(2·n·G + d) / 2d⌋`.
//!
//! What it must share is the specification itself: the grid, the rounding rule (nearest, ties to
//! the later tick) and the clamping of ADR-0021 §2. Where the oracle's own fractions overflow, the
//! query is skipped; the library does not overflow on these inputs.
//!
//! # The properties
//! - exact times are equal; approximate times agree within 10⁻⁹ s;
//! - positions of times are equal (exact, or the same grid tick);
//! - both conversions are non-decreasing;
//! - every boundary time lies within `k/2` ticks of the exact cumulated time (computed in `f64`
//!   from the exact durations), `k` the number of earlier boundaries (ADR-0021 §1);
//! - a failed edit leaves the map unchanged.

// Helper functions (not `#[test]` functions) are outside the scope of clippy.toml's
// `allow-unwrap-in-tests`, so the exemption is stated here, for this test file only: a helper that
// unwraps fails the calling test with a clear message, which is exactly what a test wants.
#![allow(
    clippy::unwrap_used,
    reason = "test helpers: a failed unwrap fails the test"
)]
// The oracle writes the formulas of its module documentation with their one-letter names (s₀, τ,
// x, d…), which is what makes it easy to check against them.
#![allow(
    clippy::many_single_char_names,
    reason = "the oracle uses the names of the formulas it implements"
)]

use std::cmp::Ordering;

use orchestra_core_time::{
    Duration, GradualChange, Position, Rational, Seconds, TIME_GRID, Tempo, TempoChange, TempoMap,
    TimeAt,
};
use proptest::prelude::*;

/// The grid, widened.
const G: i128 = TIME_GRID as i128;

/// An exact fraction on `i128`, reduced, with a positive denominator: the oracle's own number type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Q {
    /// Numerator, with the sign.
    n: i128,
    /// Denominator, > 0.
    d: i128,
}

/// Euclid's greatest common divisor of two non-negative values.
fn gcd(mut a: i128, mut b: i128) -> i128 {
    while b != 0 {
        let r = a % b;
        a = b;
        b = r;
    }
    a
}

impl Q {
    /// `n/d` reduced; `None` if `d = 0` or on overflow.
    fn new(n: i128, d: i128) -> Option<Self> {
        if d == 0 {
            return None;
        }
        let g = gcd(n.checked_abs()?, d.checked_abs()?);
        let (mut n, mut d) = (n / g, d / g);
        if d < 0 {
            n = n.checked_neg()?;
            d = d.checked_neg()?;
        }
        Some(Self { n, d })
    }

    /// A library rational, converted.
    fn of(r: Rational) -> Self {
        Self::new(i128::from(r.numerator()), i128::from(r.denominator())).unwrap()
    }

    /// An integer.
    fn int(n: i128) -> Self {
        Self { n, d: 1 }
    }

    /// Sum.
    fn add(self, o: Self) -> Option<Self> {
        Self::new(
            self.n
                .checked_mul(o.d)?
                .checked_add(o.n.checked_mul(self.d)?)?,
            self.d.checked_mul(o.d)?,
        )
    }

    /// Difference.
    fn sub(self, o: Self) -> Option<Self> {
        self.add(Self {
            n: o.n.checked_neg()?,
            d: o.d,
        })
    }

    /// Product.
    fn mul(self, o: Self) -> Option<Self> {
        Self::new(self.n.checked_mul(o.n)?, self.d.checked_mul(o.d)?)
    }

    /// Quotient.
    fn div(self, o: Self) -> Option<Self> {
        Self::new(self.n.checked_mul(o.d)?, self.d.checked_mul(o.n)?)
    }

    /// Comparison by cross products; `None` on overflow.
    fn cmp(self, o: Self) -> Option<Ordering> {
        Some(self.n.checked_mul(o.d)?.cmp(&o.n.checked_mul(self.d)?))
    }

    /// `self ≤ o`, failing the test only if it cannot be decided (never on these inputs).
    fn le(self, o: Self) -> Option<bool> {
        Some(self.cmp(o)? != Ordering::Greater)
    }

    /// The nearest grid tick, `⌊(2·n·G + d) / 2d⌋`.
    fn ticks(self) -> Option<i128> {
        let num = self.n.checked_mul(2)?.checked_mul(G)?.checked_add(self.d)?;
        num.checked_div_euclid(self.d.checked_mul(2)?)
    }

    /// An approximation.
    #[allow(
        clippy::cast_precision_loss,
        reason = "an approximation is what is wanted"
    )]
    fn f(self) -> f64 {
        self.n as f64 / self.d as f64
    }
}

/// How the speed behaves in one oracle segment.
#[derive(Debug, Clone, Copy)]
enum Kind {
    /// Constant speed `s`.
    Constant(Q),
    /// From speed `s0` to `s1` over `length` whole notes.
    Gradual {
        /// Starting speed.
        s0: Q,
        /// Final speed.
        s1: Q,
        /// Length in whole notes.
        length: Q,
    },
}

/// One oracle segment.
#[derive(Debug, Clone, Copy)]
struct Seg {
    /// Start position.
    start: Q,
    /// Start time, on the grid.
    time: Q,
    /// Behaviour.
    kind: Kind,
}

/// A tempo's speed, `unit × bpm / 60`.
fn speed(t: Tempo) -> Q {
    Q::of(t.unit().as_rational())
        .mul(Q::of(t.bpm()))
        .unwrap()
        .div(Q::int(60))
        .unwrap()
}

/// The exact duration of a segment that ends at `end`.
fn duration(seg: &Seg, end: Q) -> Q {
    match seg.kind {
        Kind::Constant(s) => end.sub(seg.start).unwrap().div(s).unwrap(),
        Kind::Gradual { s0, s1, length } => {
            let mean = s0.add(s1).unwrap().div(Q::int(2)).unwrap();
            length.div(mean).unwrap()
        }
    }
}

/// The oracle's segments, rebuilt from the source data by a linear walk, plus the exact
/// (unrounded) durations of all but the last, for the error bound.
fn oracle_segments(map: &TempoMap) -> (Vec<Seg>, Vec<Q>) {
    let mut raw: Vec<(Q, Kind)> = Vec::new();
    let mut in_force = 0;
    let changes = map.changes();
    for (i, (at, change)) in changes.iter().enumerate() {
        let at = Q::of(at.as_rational());
        match change {
            TempoChange::Immediate(t) => {
                raw.push((at, Kind::Constant(speed(*t))));
                in_force = i;
            }
            TempoChange::Gradual(g) => {
                let from = g.from.map_or_else(
                    // The tempo in force: the target of the last change before, or its tempo.
                    || match changes[in_force].1 {
                        TempoChange::Immediate(t) => speed(t),
                        TempoChange::Gradual(prev) => speed(prev.to),
                    },
                    speed,
                );
                let end = Q::of(g.end.as_rational());
                raw.push((
                    at,
                    Kind::Gradual {
                        s0: from,
                        s1: speed(g.to),
                        length: end.sub(at).unwrap(),
                    },
                ));
                let next = changes.get(i + 1).map(|(p, _)| Q::of(p.as_rational()));
                if next != Some(end) {
                    raw.push((end, Kind::Constant(speed(g.to))));
                }
                in_force = i;
            }
        }
    }
    let mut segs: Vec<Seg> = Vec::new();
    let mut exact = Vec::new();
    let mut ticks: i128 = 0;
    for (k, &(start, kind)) in raw.iter().enumerate() {
        let seg = Seg {
            start,
            time: Q::new(ticks, G).unwrap(),
            kind,
        };
        if let Some(&(next, _)) = raw.get(k + 1) {
            let d = duration(&seg, next);
            exact.push(d);
            ticks += d.ticks().unwrap();
        }
        segs.push(seg);
    }
    (segs, exact)
}

/// The index of the segment that contains `value`, by a linear scan over `key` (start position or
/// start time); the first segment for values before the start.
fn scan(segs: &[Seg], value: Q, key: fn(&Seg) -> Q) -> usize {
    let mut found = 0;
    for (i, seg) in segs.iter().enumerate() {
        if key(seg).le(value).unwrap() {
            found = i;
        }
    }
    found
}

/// The oracle's time of a position: `Ok(exact)` or `Err(approximate)`; `None` on its own overflow.
fn oracle_time(segs: &[Seg], p: Q) -> Option<Result<Q, f64>> {
    let i = scan(segs, p, |s| s.start);
    let seg = segs[i];
    let next = segs.get(i + 1);
    let x = p.sub(seg.start)?;
    match seg.kind {
        Kind::Constant(s) => {
            let mut t = seg.time.add(x.div(s)?)?;
            if let Some(next) = next
                && !t.le(next.time)?
            {
                t = next.time;
            }
            Some(Ok(t))
        }
        Kind::Gradual { s0, s1, length } => {
            if x.n == 0 {
                return Some(Ok(seg.time));
            }
            let d = length.div(s0.add(s1)?.div(Q::int(2))?)?;
            let a = s1.sub(s0)?.div(d)?.f();
            let (s0, x) = (s0.f(), x.f());
            let tau = if a == 0.0 {
                x / s0
            } else {
                ((s0 * s0 + 2.0 * a * x).sqrt() - s0) / a
            };
            let t = (seg.time.f() + tau).min(next.unwrap().time.f());
            Some(Err(t))
        }
    }
}

/// The oracle's position of a time; `None` on its own overflow.
fn oracle_position(segs: &[Seg], t: Q) -> Option<Q> {
    let i = scan(segs, t, |s| s.time);
    let seg = segs[i];
    let next = segs.get(i + 1);
    let tau = t.sub(seg.time)?;
    match seg.kind {
        Kind::Constant(s) => {
            let p = seg.start.add(s.mul(tau)?)?;
            Some(match next {
                Some(next) if !p.le(next.start)? => next.start,
                _ => p,
            })
        }
        Kind::Gradual { s0, s1, length } => {
            let end = seg.start.add(length)?;
            let d = length.div(s0.add(s1)?.div(Q::int(2))?)?;
            if d.le(tau)? {
                return Some(end);
            }
            let a = s1.sub(s0)?.div(d)?;
            let s_tau = s0.add(a.mul(tau)?)?;
            let x = tau.mul(s0.add(s_tau)?)?.div(Q::int(2))?;
            let p = seg.start.add(Q::new(x.ticks()?, G)?)?;
            Some(if end.le(p)? { end } else { p })
        }
    }
}

/// The tempi the edits choose from: various units, integer and fractional marks, marks whose
/// durations fall on the grid and marks whose durations do not.
fn tempi() -> Vec<Tempo> {
    [
        ((1, 4), (120, 1)),
        ((1, 4), (60, 1)),
        ((1, 4), (77, 1)),
        ((1, 4), (71, 1)),
        ((3, 8), (80, 1)),
        ((1, 2), (63, 1)),
        ((1, 4), (185, 2)),
        ((1, 8), (211, 1)),
        ((1, 4), (132, 1)),
        ((1, 4), (43, 1)),
    ]
    .iter()
    .map(|&((un, ud), (bn, bd))| {
        Tempo::new(
            Duration::from_rational(Rational::new(un, ud).unwrap()),
            Rational::new(bn, bd).unwrap(),
        )
        .unwrap()
    })
    .collect()
}

/// One edit of the map; positions are in quarters (or thirds, for odd spots) of a whole note.
#[derive(Debug, Clone)]
enum Edit {
    /// An immediate change at `n/d`.
    Immediate(i64, i64, usize),
    /// A gradual change at `n/d`, `len` quarters long, maybe with a starting tempo.
    Gradual(i64, i64, i64, Option<usize>, usize),
    /// Remove whatever is at `n/4`.
    Remove(i64),
}

/// A random edit.
fn edit() -> impl Strategy<Value = Edit> {
    let den = prop_oneof![Just(4_i64), Just(3)];
    prop_oneof![
        (0_i64..96, den.clone(), 0..10_usize).prop_map(|(n, d, t)| Edit::Immediate(n, d, t)),
        (
            0_i64..96,
            den,
            1_i64..=40,
            proptest::option::of(0..10_usize),
            0..10_usize
        )
            .prop_map(|(n, d, l, f, t)| Edit::Gradual(n, d, l, f, t)),
        (1_i64..96).prop_map(Edit::Remove),
    ]
}

/// The position `n/d` whole notes.
fn at(n: i64, d: i64) -> Position {
    Position::from_rational(Rational::new(n, d).unwrap())
}

/// Applies an edit; on failure, checks that the map is unchanged (all-or-nothing editing).
fn apply(map: &mut TempoMap, edit: &Edit) {
    let tempi = tempi();
    let before = map.clone();
    let result = match *edit {
        Edit::Immediate(n, d, t) => map.set_change(at(n, d), TempoChange::Immediate(tempi[t])),
        Edit::Gradual(n, d, len, from, to) => {
            let start = at(n, d);
            let end = start
                .checked_add(Duration::from_rational(Rational::new(len, 4).unwrap()))
                .unwrap();
            map.set_change(
                start,
                TempoChange::Gradual(GradualChange {
                    from: from.map(|f| tempi[f]),
                    to: tempi[to],
                    end,
                }),
            )
        }
        Edit::Remove(n) => map.remove_change(at(n, 4)),
    };
    if result.is_err() {
        assert_eq!(*map, before, "a failed edit changed the map: {edit:?}");
    }
}

/// A query position: `n/d` whole notes, with denominators of note values and tuplets.
fn query_position() -> impl Strategy<Value = (i64, i64)> {
    (
        -40_i64..1600,
        prop_oneof![Just(16_i64), Just(12), Just(10), Just(7), Just(1)],
    )
}

/// A query time: `n/d` seconds, with denominators of sample rates, frame rates and odd values.
fn query_time() -> impl Strategy<Value = (i64, i64)> {
    prop_oneof![
        (-96_000_i64..9_600_000, Just(48_000_i64)),
        (-44_100_i64..8_820_000, Just(44_100)),
        (-3_i64..2_000, Just(10)),
        // Starts of video frames at 30000/1001 fps: k·1001/30000 s.
        (-30_i64..6_000).prop_map(|k| (k * 1001, 30_000_i64)),
        (0_i64..600_000, Just(3_001)),
    ]
}

/// Builds a map from random edits.
fn build(edits: &[Edit]) -> TempoMap {
    let mut map = TempoMap::new(tempi()[0]).unwrap();
    for e in edits {
        apply(&mut map, e);
    }
    map
}

proptest! {
    // Each case builds a map with up to 24 edits and runs 64 queries: 256 cases are plenty.
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Position → time agrees with the oracle, and is non-decreasing.
    #[test]
    fn times_match_oracle(
        edits in prop::collection::vec(edit(), 0..24),
        mut queries in prop::collection::vec(query_position(), 64),
    ) {
        let map = build(&edits);
        let (segs, _) = oracle_segments(&map);
        queries.sort_by(|a, b| Q::int(a.0.into()).mul(Q::int(b.1.into())).unwrap()
            .cmp(Q::int(b.0.into()).mul(Q::int(a.1.into())).unwrap()).unwrap());
        let mut last = f64::NEG_INFINITY;
        for (n, d) in queries {
            let got = map.time_at(at(n, d)).unwrap();
            prop_assert!(got.to_f64() >= last, "time went back at {n}/{d}");
            last = got.to_f64();
            let Some(want) = oracle_time(&segs, Q::new(n.into(), d.into()).unwrap()) else {
                continue;
            };
            match (got, want) {
                (TimeAt::Exact(g), Ok(w)) => prop_assert_eq!(Q::of(g.as_rational()), w),
                (TimeAt::Approximate(g), Err(w)) => {
                    prop_assert!((g - w).abs() < 1e-9, "{g} vs {w} at {n}/{d}");
                }
                (g, w) => prop_assert!(false, "kind differs at {n}/{d}: {g:?} vs {w:?}"),
            }
        }
    }

    /// Time → position agrees with the oracle exactly, and is non-decreasing.
    #[test]
    fn positions_match_oracle(
        edits in prop::collection::vec(edit(), 0..24),
        queries in prop::collection::vec(query_time(), 64),
    ) {
        let map = build(&edits);
        let (segs, _) = oracle_segments(&map);
        let mut queries: Vec<Q> =
            queries.iter().map(|&(n, d)| Q::new(n.into(), d.into()).unwrap()).collect();
        queries.sort_by(|a, b| a.cmp(*b).unwrap());
        let mut last: Option<Position> = None;
        for t in queries {
            let time = Seconds::from_rational(
                Rational::new(t.n.try_into().unwrap(), t.d.try_into().unwrap()).unwrap(),
            );
            // Times whose denominator divides the grid (samples, frames, decimal seconds) must
            // always succeed; a foreign denominator (3001) may overflow, as ADR-0021 §4 allows.
            let got = match map.position_at(time) {
                Ok(got) => got,
                Err(error) => {
                    prop_assert!(G % t.d != 0, "{error:?} at {t:?}");
                    continue;
                }
            };
            if let Some(last) = last {
                prop_assert!(got >= last, "position went back at {t:?}");
            }
            last = Some(got);
            if let Some(want) = oracle_position(&segs, t) {
                prop_assert_eq!(Q::of(got.as_rational()), want, "at {:?}", t);
            }
        }
    }

    /// Every boundary lies within half a tick per earlier boundary of the exact cumulated time
    /// (ADR-0021 §1), and the boundary times are those of the oracle.
    #[test]
    fn boundary_error_is_bounded(edits in prop::collection::vec(edit(), 0..24)) {
        let map = build(&edits);
        let (segs, exact) = oracle_segments(&map);
        let mut sum = 0.0_f64;
        for (k, seg) in segs.iter().enumerate().skip(1) {
            sum += exact[k - 1].f();
            #[allow(clippy::cast_precision_loss, reason = "k is small")]
            let bound = k as f64 * 0.5 / G as f64 + 1e-12;
            prop_assert!((seg.time.f() - sum).abs() <= bound, "boundary {k}");
            let position = Position::from_rational(
                Rational::new(seg.start.n.try_into().unwrap(), seg.start.d.try_into().unwrap())
                    .unwrap(),
            );
            prop_assert_eq!(map.time_at(position).unwrap(), TimeAt::Exact(Seconds::from_rational(
                Rational::new(seg.time.n.try_into().unwrap(), seg.time.d.try_into().unwrap())
                    .unwrap(),
            )));
        }
    }
}
