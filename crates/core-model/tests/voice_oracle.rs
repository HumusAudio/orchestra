// SPDX-License-Identifier: GPL-3.0-or-later

//! Property tests of the voices' content against an independent oracle (ADR-0016 §4,
//! ADR-0024 §1–§3 and §9, SCALETTA F1.09b).
//!
//! # The oracle
//! Written here, sharing no code with the library: its own `i128` fractions (textbook formulas,
//! Euclid's gcd), note value lengths computed as "half the previous value" in a loop rather than
//! with the library's shifts, tuplet ratios multiplied along a chain kept as plain indices, and a
//! validity check that literally sorts each bar's events and compares neighbours.
//!
//! # What is checked
//! - **Durations:** for random voices with random nested tuplets, every event's metric duration
//!   equals the oracle's.
//! - **Validity:** `build()` succeeds exactly when the oracle finds no overlap and no whole-bar
//!   rest in company, and otherwise reports the same first violation, on the same event.
//! - **Clefs:** for random clef changes and random query points, `clef_at` equals a linear scan
//!   for the last change at or before the point.

// Helper functions (not `#[test]` functions) are outside the scope of clippy.toml's
// `allow-unwrap-in-tests`, so the exemption is stated here, for this test file only.
#![allow(
    clippy::unwrap_used,
    reason = "test helpers: a failed unwrap fails the test"
)]

use std::cmp::Ordering;

use orchestra_core_model::{
    Alteration, BaseValue, Clef, DocumentBuilder, DocumentError, EventId, Interval, Key, Mode,
    NoteValue, Pitch, Placement, RestValue, Step, Tuplet, TupletId, Violation,
};
use orchestra_core_time::{Duration, Meter, MeterChange, MeterMap, Rational, Tempo, TempoMap};
use proptest::prelude::*;

// ------------------------------------------------------------------------------------------------
// The oracle's arithmetic
// ------------------------------------------------------------------------------------------------

/// Euclid's greatest common divisor of two non-negative numbers.
fn gcd(a: i128, b: i128) -> i128 {
    if b == 0 { a } else { gcd(b, a % b) }
}

/// A fraction in lowest terms with a positive denominator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct F(i128, i128);

impl F {
    /// `n/d` reduced.
    fn new(n: i128, d: i128) -> Self {
        let g = gcd(n.abs(), d.abs()).max(1);
        let sign = if d < 0 { -1 } else { 1 };
        Self(sign * n / g, sign * d / g)
    }
    /// The sum.
    fn add(self, o: Self) -> Self {
        Self::new(self.0 * o.1 + o.0 * self.1, self.1 * o.1)
    }
    /// The product.
    fn mul(self, o: Self) -> Self {
        Self::new(self.0 * o.0, self.1 * o.1)
    }
    /// Cross-multiplied comparison.
    fn cmp(self, o: Self) -> Ordering {
        (self.0 * o.1).cmp(&(o.0 * self.1))
    }
    /// The library's duration as an oracle fraction.
    fn of(d: Duration) -> Self {
        let r = d.as_rational();
        Self::new(i128::from(r.numerator()), i128::from(r.denominator()))
    }
}

/// The length of a base value with dots, the oracle's way: start from a whole note, halve or
/// double it step by step, then add half of the previous addition once per dot.
fn oracle_length(base: BaseValue, dots: u8) -> F {
    // Position of the base value in the list, the whole note being at 3.
    let position = BaseValue::ALL.iter().position(|&b| b == base).unwrap();
    let mut value = F::new(8, 1); // the maxima
    for _ in 0..position {
        value = value.mul(F::new(1, 2));
    }
    let mut total = value;
    let mut addition = value;
    for _ in 0..dots {
        addition = addition.mul(F::new(1, 2));
        total = total.add(addition);
    }
    total
}

// ------------------------------------------------------------------------------------------------
// The generated cases
// ------------------------------------------------------------------------------------------------

/// A generated tuplet: its terms, its unit, and the index of an earlier tuplet as parent.
#[derive(Debug, Clone)]
struct GenTuplet {
    actual: u8,
    normal: u8,
    unit: BaseValue,
    parent: Option<usize>,
}

/// A generated event.
#[derive(Debug, Clone)]
struct GenEvent {
    /// Bar index, modulo the number of bars.
    bar: usize,
    /// Offset in sixteenths of a whole note (so up to 1 1/16: overfull bars happen).
    offset: i64,
    base: BaseValue,
    dots: u8,
    /// Index of a tuplet, modulo the number of tuplets.
    tuplet: Option<usize>,
    /// Rest or chord.
    rest: bool,
    /// A whole-bar rest (only meaningful with `rest`).
    whole_bar: bool,
}

/// The meters a generated document uses: numerator, denominator.
const METERS: [(u8, u8); 3] = [(4, 4), (3, 4), (5, 8)];
/// The base values events and units are drawn from.
const BASES: [BaseValue; 5] = [
    BaseValue::Half,
    BaseValue::Quarter,
    BaseValue::Eighth,
    BaseValue::Sixteenth,
    BaseValue::ThirtySecond,
];

/// A strategy for tuplets: each may nest in any earlier one (index modulo its position).
fn tuplets() -> impl Strategy<Value = Vec<GenTuplet>> {
    prop::collection::vec(
        (
            2_u8..8,
            1_u8..7,
            0..BASES.len(),
            prop::option::of(0_usize..8),
        ),
        0..4,
    )
    .prop_map(|raw| {
        raw.into_iter()
            .enumerate()
            .map(|(i, (actual, normal, unit, parent))| GenTuplet {
                actual,
                normal,
                unit: BASES[unit],
                // The parent must be an earlier tuplet; the first has none.
                parent: parent.and_then(|p| (i > 0).then(|| p % i)),
            })
            .collect()
    })
}

/// A strategy for events.
fn events() -> impl Strategy<Value = Vec<GenEvent>> {
    prop::collection::vec(
        (
            0_usize..3,
            0_i64..17,
            0..BASES.len(),
            0_u8..3,
            prop::option::of(0_usize..4),
            any::<bool>(),
            prop::bool::weighted(0.1),
        ),
        0..14,
    )
    .prop_map(|raw| {
        raw.into_iter()
            .map(
                |(bar, offset, base, dots, tuplet, rest, whole_bar)| GenEvent {
                    bar,
                    offset,
                    base: BASES[base],
                    dots,
                    tuplet,
                    rest,
                    whole_bar,
                },
            )
            .collect()
    })
}

/// A builder with `bars` bars in this meter and one flute; returns its voice and bars too.
fn flute(
    meter: (u8, u8),
    bars: usize,
) -> (
    DocumentBuilder,
    Vec<orchestra_core_model::BarId>,
    orchestra_core_model::VoiceId,
    orchestra_core_model::StaffId,
) {
    let meters = MeterMap::new(MeterChange::new(
        Meter::new(&[meter.0], u16::from(meter.1)).unwrap(),
    ))
    .unwrap();
    let tempo = Tempo::new(
        Duration::from_rational(Rational::new(1, 4).unwrap()),
        Rational::from_integer(60).unwrap(),
    )
    .unwrap();
    let mut b = DocumentBuilder::new(
        meters,
        TempoMap::new(tempo).unwrap(),
        Key::traditional(0, Mode::Major).unwrap(),
    );
    let ids = (0..bars).map(|_| b.add_bar().unwrap()).collect();
    let track = b
        .add_instrument_track("Flute", Interval::new(0, Rational::ZERO), &[Clef::TREBLE])
        .unwrap();
    let instrument = b.document().track(track).unwrap().instrument();
    let (voice, staff) = (instrument.voices()[0].id(), instrument.staves()[0].id());
    (b, ids, voice, staff)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// Durations and validity against the oracle.
    #[test]
    fn durations_and_validity(
        meter in 0..METERS.len(),
        bars in 1_usize..4,
        gen_tuplets in tuplets(),
        gen_events in events(),
    ) {
        let meter = METERS[meter];
        let bar_length = F::new(i128::from(meter.0), i128::from(meter.1));
        let (mut b, bar_ids, voice, _) = flute(meter, bars);

        // Tuplets, with the oracle's chain of factors (innermost first) for each.
        let mut tuplet_ids: Vec<TupletId> = Vec::new();
        let mut chains: Vec<Vec<F>> = Vec::new();
        for t in &gen_tuplets {
            let tuplet = Tuplet::new(t.actual, t.normal, NoteValue::plain(t.unit)).unwrap();
            let parent = t.parent.map(|p| tuplet_ids[p]);
            let id = b.add_tuplet(voice, tuplet, parent).unwrap();
            let mut chain = vec![F::new(i128::from(t.normal), i128::from(t.actual))];
            if let Some(p) = t.parent {
                chain.extend(chains[p].iter().copied());
            }
            tuplet_ids.push(id);
            chains.push(chain);
        }

        // Events: what the builder accepted, with the oracle's offset and duration, per bar.
        let mut accepted: Vec<Vec<(F, F, EventId, bool)>> = vec![Vec::new(); bars];
        for e in &gen_events {
            let bar = e.bar % bars;
            let tuplet = if tuplet_ids.is_empty() {
                None
            } else {
                e.tuplet.map(|t| t % tuplet_ids.len())
            };
            let offset = Duration::from_rational(Rational::new(e.offset, 16).unwrap());
            let mut at = Placement::new(voice, bar_ids[bar], offset);
            if let Some(t) = tuplet {
                at = at.in_tuplet(tuplet_ids[t]);
            }
            let value = NoteValue::new(e.base, e.dots).unwrap();
            let whole_bar = e.rest && e.whole_bar;
            let result = if !e.rest {
                let pitch = Pitch::new(Step::A, Alteration::NATURAL, 4);
                b.add_chord(at, value, &[pitch]).map(|ids| ids.event)
            } else if whole_bar {
                b.add_rest(at, RestValue::WholeBar)
            } else {
                b.add_rest(at, RestValue::Value(value))
            };
            if whole_bar && tuplet.is_some() {
                prop_assert_eq!(result, Err(DocumentError::WholeBarRestInTuplet));
                continue;
            }
            let id = result.unwrap();
            let duration = if whole_bar {
                bar_length
            } else {
                let mut d = oracle_length(e.base, e.dots);
                for &factor in tuplet.map_or(&[][..], |t| &chains[t]) {
                    d = d.mul(factor);
                }
                d
            };
            // The library's duration, read before `build` (which may refuse the document).
            let got = F::of(b.document().metric_duration(id).unwrap());
            prop_assert_eq!(got, duration);
            accepted[bar].push((F::new(i128::from(e.offset), 16), duration, id, whole_bar));
        }

        // The oracle's first violation, in score order.
        let mut expected: Option<Violation> = None;
        'bars: for bar in &mut accepted {
            // A stable sort keeps the insertion order of events at the same offset, as the
            // chunks do.
            bar.sort_by(|a, b| a.0.cmp(b.0));
            let count = bar.len();
            let mut end: Option<F> = None;
            for &(offset, duration, id, whole_bar) in bar.iter() {
                if whole_bar && count > 1 {
                    expected = Some(Violation::WholeBarRestNotAlone(id));
                    break 'bars;
                }
                if end.is_some_and(|e| offset.cmp(e) == Ordering::Less) {
                    expected = Some(Violation::Overlap(id));
                    break 'bars;
                }
                end = Some(offset.add(duration));
            }
        }
        match expected {
            None => prop_assert!(b.build().is_ok()),
            Some(v) => prop_assert_eq!(b.build().err(), Some(DocumentError::Invalid(v))),
        }
    }

    /// `clef_at` against a linear scan.
    #[test]
    fn clef_lookup(
        changes in prop::collection::vec((0_usize..4, 0_i64..8, 0_usize..4), 0..12),
        queries in prop::collection::vec((0_usize..4, 0_i64..8), 1..20),
    ) {
        const CLEFS: [Clef; 4] = [Clef::TREBLE, Clef::BASS, Clef::ALTO, Clef::TENOR];
        let (mut b, bars, _, staff) = flute((4, 4), 4);
        // The oracle: the accepted changes as (bar, eighths, clef).
        let mut oracle: Vec<(usize, i64, Clef)> = Vec::new();
        for &(bar, eighths, clef) in &changes {
            let offset = Duration::from_rational(Rational::new(eighths, 8).unwrap());
            let result = b.add_clef_change(staff, bars[bar], offset, CLEFS[clef]);
            let taken = oracle.iter().any(|&(b2, e2, _)| (b2, e2) == (bar, eighths));
            if (bar, eighths) == (0, 0) || taken {
                prop_assert_eq!(result, Err(DocumentError::DuplicateMark));
            } else {
                prop_assert!(result.is_ok());
                oracle.push((bar, eighths, CLEFS[clef]));
            }
        }
        let document = b.build().unwrap();
        for &(bar, eighths) in &queries {
            let offset = Duration::from_rational(Rational::new(eighths, 8).unwrap());
            let expected = oracle
                .iter()
                .filter(|&&(b2, e2, _)| (b2, e2) <= (bar, eighths))
                .max_by_key(|&&(b2, e2, _)| (b2, e2))
                .map_or(Clef::TREBLE, |&(_, _, c)| c);
            prop_assert_eq!(document.clef_at(staff, bars[bar], offset).unwrap(), expected);
        }
    }
}
