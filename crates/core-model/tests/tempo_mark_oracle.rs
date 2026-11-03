// SPDX-License-Identifier: GPL-3.0-or-later

//! Property tests of the tempo marks and the hairpins against an oracle that shares no code with
//! the library (ADR-0016 §4; SCALETTA F1.09c, second part).
//!
//! # The oracle
//! Every meter here has a denominator of 4 or 8 and every offset is a whole number of eighths, so
//! the oracle counts in **eighths**, plain `i64`: bar lengths, bar starts (a running sum, no meter
//! map), points, positions. It knows the rules from ADR-0024 §8 and §11 and from the owner's
//! decisions of 2026-11-03, written out again:
//! - tempo marks: one per point, never at the very start; in score order a gradual change ends
//!   after it starts and no change starts strictly inside it; positions are clamped to the
//!   barline; at one position the later mark wins; a gradual change shrunk to nothing is
//!   immediate;
//! - hairpins: each ends after it starts; in the order of their starts (creation order at one
//!   start), two of one scope never overlap.
//!
//! The library is fed random edits; after each one the oracle predicts whether it is accepted
//! (and with which error), the builder must be unchanged after a refusal, and the derived tempo
//! map must equal the oracle's list of changes. For hairpins the oracle predicts the outcome of
//! `build`.

// Helper functions (not `#[test]` functions) are outside the scope of clippy.toml's
// `allow-unwrap-in-tests`, so the exemption is stated here, for this test file only.
#![allow(
    clippy::unwrap_used,
    reason = "test helpers: a failed unwrap fails the test"
)]
// The oracle indexes its own small vectors with indices it generated itself.
#![allow(clippy::indexing_slicing, reason = "test oracle over its own vectors")]

use orchestra_core_model::{
    BarId, Clef, DocumentBuilder, DocumentError, DynamicPlacement, GradualMark, HairpinKind,
    HairpinShape, HairpinStyle, Interval, Key, Metronome, Mode, ScorePoint, TempoMark,
    TempoMarkChange, Violation,
};
use orchestra_core_time::{
    BarIndex, Duration, Meter, MeterChange, MeterMap, Position, Rational, Tempo, TempoChange,
};
use proptest::prelude::*;

/// The tempi the marks choose from, as quarter-note BPM.
const TEMPI: [i64; 4] = [40, 60, 90, 132];

/// ♩ = `TEMPI[i]`.
fn tempo(i: usize) -> Tempo {
    Tempo::new(
        Duration::from_rational(Rational::new(1, 4).unwrap()),
        Rational::from_integer(TEMPI[i]).unwrap(),
    )
    .unwrap()
}

/// `n` eighths as a library duration.
fn eighths(n: i64) -> Duration {
    Duration::from_rational(Rational::new(n, 8).unwrap())
}

/// A library position as a whole number of eighths (every position here is one).
fn to_eighths(p: Position) -> i64 {
    let r = p.as_rational();
    assert_eq!(
        8 % r.denominator(),
        0,
        "position {p:?} is not on the eighth grid"
    );
    r.numerator() * (8 / r.denominator())
}

// ------------------------------------------------------------------------------------------------
// The score: meters in eighths
// ------------------------------------------------------------------------------------------------

/// A meter of the generated scores: `beats`/`unit`, with `unit` 4 or 8.
#[derive(Debug, Clone, Copy)]
struct M {
    /// Beats per bar.
    beats: u8,
    /// The beat unit: 4 or 8.
    unit: u16,
}

impl M {
    /// The bar length in eighths.
    fn eighths(self) -> i64 {
        i64::from(self.beats) * (8 / i64::from(self.unit))
    }
}

/// A meter strategy.
fn meter() -> impl Strategy<Value = M> {
    (1_u8..=7, prop_oneof![Just(4_u16), Just(8)]).prop_map(|(beats, unit)| M { beats, unit })
}

/// The meter of each bar (one entry per bar: the generator changes the meter at any bar).
fn meters() -> impl Strategy<Value = Vec<M>> {
    prop::collection::vec(meter(), 1..8)
}

/// The library builder for these bars, and the bar IDs.
fn builder(bars: &[M]) -> (DocumentBuilder, Vec<BarId>) {
    let first = bars[0];
    let mut map = MeterMap::new(MeterChange::new(
        Meter::new(&[first.beats], first.unit).unwrap(),
    ))
    .unwrap();
    for (i, m) in bars.iter().enumerate().skip(1) {
        map.set_meter_change(
            BarIndex::new(u32::try_from(i).unwrap()),
            MeterChange::new(Meter::new(&[m.beats], m.unit).unwrap()),
        )
        .unwrap();
    }
    let mut b = DocumentBuilder::new(
        map,
        TempoMark::immediate(None, Metronome::shown(tempo(1))).unwrap(),
        Key::traditional(0, Mode::Major).unwrap(),
    )
    .unwrap();
    let ids = bars.iter().map(|_| b.add_bar().unwrap()).collect();
    (b, ids)
}

/// The start of every bar in eighths: a running sum, the oracle's own meter map.
fn starts(bars: &[M]) -> Vec<i64> {
    let mut total = 0;
    bars.iter()
        .map(|m| {
            let start = total;
            total += m.eighths();
            start
        })
        .collect()
}

// ------------------------------------------------------------------------------------------------
// Tempo marks
// ------------------------------------------------------------------------------------------------

/// What a generated mark does.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Kind {
    /// Text only.
    Text,
    /// ♩ = `TEMPI[i]` at once.
    Now(usize),
    /// Gradually to `TEMPI[to]` up to (bar, offset in eighths), from `TEMPI[from]` if given.
    Gradual {
        /// The starting tempo, if any.
        from: Option<usize>,
        /// The target tempo.
        to: usize,
        /// The end: bar index (taken modulo the bars) and offset in eighths.
        end: (usize, i64),
    },
}

/// A generated mark: (bar index modulo the bars, offset in eighths, kind). Offsets go up to 12
/// eighths, past the end of most bars, so that overfull points and clamping are common.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Gen {
    /// Bar index, modulo the number of bars.
    bar: usize,
    /// Offset in eighths.
    offset: i64,
    /// What it does.
    kind: Kind,
}

/// A mark strategy.
fn gen_mark() -> impl Strategy<Value = Gen> {
    let kind = prop_oneof![
        Just(Kind::Text),
        (0_usize..4).prop_map(Kind::Now),
        (
            prop::option::of(0_usize..4),
            0_usize..4,
            0_usize..8,
            0_i64..=12
        )
            .prop_map(|(from, to, bar, offset)| Kind::Gradual {
                from,
                to,
                end: (bar, offset)
            }),
    ];
    (0_usize..8, 0_i64..=12, kind).prop_map(|(bar, offset, kind)| Gen { bar, offset, kind })
}

/// The library mark for a generated one.
fn library_mark(g: &Gen, ids: &[BarId]) -> TempoMark {
    let change = match g.kind {
        Kind::Text => None,
        Kind::Now(t) => Some(TempoMarkChange::Immediate(Metronome::shown(tempo(t)))),
        Kind::Gradual { from, to, end } => Some(TempoMarkChange::Gradual(GradualMark {
            from: from.map(|f| Metronome::shown(tempo(f))),
            to: tempo(to),
            end: ScorePoint::new(ids[end.0 % ids.len()], eighths(end.1)),
        })),
    };
    let text = matches!(g.kind, Kind::Text).then_some("text");
    TempoMark::new(text, change).unwrap()
}

/// The oracle's change of the tempo map: (position in eighths, tempo index, gradual end in
/// eighths and starting tempo, if gradual).
type OChange = (i64, usize, Option<(i64, Option<usize>)>);

/// The oracle's verdict on a list of marks (bar index already reduced), in score order: the
/// expected changes, or which rule fails first.
#[derive(Debug, PartialEq)]
enum Verdict {
    /// The changes of the map.
    Ok(Vec<OChange>),
    /// A gradual change does not go forwards.
    NotForward,
    /// A change starts inside a gradual one.
    Inside,
}

/// The oracle: walks the marks in score order with the rules of the module documentation.
fn verdict(bars: &[M], marks: &[Gen]) -> Verdict {
    let starts = starts(bars);
    let position = |bar: usize, offset: i64| starts[bar] + offset.min(bars[bar].eighths());
    let mut sorted: Vec<&Gen> = marks.iter().collect();
    sorted.sort_by_key(|g| (g.bar, g.offset));
    let mut out: Vec<OChange> = vec![(0, 1, None)];
    let mut open: Option<(usize, i64)> = None;
    for g in sorted {
        let item = match g.kind {
            Kind::Text => continue,
            Kind::Now(t) => (t, None),
            Kind::Gradual { from, to, end } => (to, Some((from, (end.0 % bars.len(), end.1)))),
        };
        let key = (g.bar, g.offset);
        if let Some(end) = open.take()
            && key < end
        {
            return Verdict::Inside;
        }
        let at = position(g.bar, g.offset);
        let change = match item {
            (t, None) => (at, t, None),
            (to, Some((from, end))) => {
                if end <= key {
                    return Verdict::NotForward;
                }
                open = Some(end);
                let end_at = position(end.0, end.1);
                if end_at <= at {
                    (at, to, None)
                } else {
                    (at, to, Some((end_at, from)))
                }
            }
        };
        if out.last().is_some_and(|last| last.0 == at) {
            *out.last_mut().unwrap() = change;
        } else {
            out.push(change);
        }
    }
    Verdict::Ok(out)
}

/// The library's tempo map in the oracle's terms.
fn library_changes(b: &DocumentBuilder) -> Vec<OChange> {
    let index = |t: Tempo| (0..4).find(|&i| tempo(i) == t).unwrap();
    b.document()
        .tempo_map()
        .changes()
        .iter()
        .map(|&(p, c)| match c {
            TempoChange::Immediate(t) => (to_eighths(p), index(t), None),
            TempoChange::Gradual(g) => (
                to_eighths(p),
                index(g.to),
                Some((to_eighths(g.end), g.from.map(index))),
            ),
        })
        .collect()
}

// ------------------------------------------------------------------------------------------------
// Hairpins
// ------------------------------------------------------------------------------------------------

/// A generated hairpin: scope (0 = instrument, 1–2 = a voice), start and end as (bar index modulo
/// the bars, offset in eighths).
#[derive(Debug, Clone, Copy)]
struct HGen {
    /// 0 for the whole instrument, 1 or 2 for one voice.
    scope: usize,
    /// Start point.
    start: (usize, i64),
    /// End point (excluded).
    end: (usize, i64),
}

/// A hairpin strategy.
fn gen_hairpin() -> impl Strategy<Value = HGen> {
    (0_usize..3, 0_usize..8, 0_i64..=8, 0_usize..8, 0_i64..=8).prop_map(
        |(scope, sb, so, eb, eo)| HGen {
            scope,
            start: (sb, so),
            end: (eb, eo),
        },
    )
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// Every tempo mark edit is accepted or refused as the oracle says, a refusal changes nothing,
    /// and the derived tempo map always equals the oracle's.
    #[test]
    fn tempo_marks_match_the_oracle(
        bars in meters(),
        edits in prop::collection::vec(gen_mark(), 0..16),
    ) {
        let (mut b, ids) = builder(&bars);
        let mut accepted: Vec<Gen> = Vec::new();
        for raw in edits {
            let g = Gen { bar: raw.bar % bars.len(), ..raw };
            let duplicate = (g.bar, g.offset) == (0, 0)
                || accepted.iter().any(|a| (a.bar, a.offset) == (g.bar, g.offset));
            let mut candidate = accepted.clone();
            candidate.push(g);
            let expected = if duplicate { None } else { Some(verdict(&bars, &candidate)) };
            let before = b.document().clone();
            let result = b.add_tempo_mark(
                ScorePoint::new(ids[g.bar], eighths(g.offset)),
                library_mark(&g, &ids),
            );
            match (expected, result) {
                (None, Err(DocumentError::DuplicateMark))
                | (Some(Verdict::NotForward),
                   Err(DocumentError::Invalid(Violation::GradualTempoNotForward(_))))
                | (Some(Verdict::Inside),
                   Err(DocumentError::Invalid(Violation::TempoChangeInsideGradual(_)))) => {
                    prop_assert_eq!(b.document(), &before, "a refused mark changed the builder");
                }
                (Some(Verdict::Ok(want)), Ok(_)) => {
                    accepted.push(g);
                    prop_assert_eq!(library_changes(&b), want);
                }
                (want, got) => prop_assert!(false, "{g:?}: expected {want:?}, got {got:?}"),
            }
        }
        prop_assert!(b.build().is_ok());
    }

    /// Hairpins: `add_hairpin` refuses those that do not go forwards, and `build` reports the
    /// first overlap in the order of the starts, as the oracle says.
    #[test]
    fn hairpins_match_the_oracle(
        bars in meters(),
        edits in prop::collection::vec(gen_hairpin(), 0..12),
    ) {
        let (mut b, ids) = builder(&bars);
        let unison = Interval::new(0, Rational::ZERO);
        let track = b.add_instrument_track("Piano", unison, &[Clef::TREBLE, Clef::BASS]).unwrap();
        let (piano, staff, voices) = {
            let instrument = b.document().track(track).unwrap().instrument();
            let voices = [instrument.voices()[0].id(), instrument.voices()[1].id()];
            (instrument.id(), instrument.staves()[0].id(), voices)
        };
        // (start key, end key, scope, ID) of the accepted hairpins, in creation order.
        let mut accepted = Vec::new();
        for h in edits {
            let start = (h.start.0 % bars.len(), h.start.1);
            let end = (h.end.0 % bars.len(), h.end.1);
            let placement = match h.scope {
                0 => DynamicPlacement::instrument(staff),
                v => DynamicPlacement::voice(voices[v - 1], staff),
            };
            let shape = HairpinShape {
                kind: if h.scope == 1 { HairpinKind::Diminuendo } else { HairpinKind::Crescendo },
                style: HairpinStyle::Wedge,
                niente: false,
            };
            let result = b.add_hairpin(
                piano,
                ScorePoint::new(ids[start.0], eighths(start.1)),
                ScorePoint::new(ids[end.0], eighths(end.1)),
                placement,
                shape,
            );
            if end <= start {
                prop_assert_eq!(result, Err(DocumentError::HairpinNotForward));
            } else {
                accepted.push((start, end, h.scope, result.unwrap()));
            }
        }
        // The oracle: sort by start (stable, so creation order at one start), remember the last
        // end per scope.
        let mut sorted = accepted.clone();
        sorted.sort_by_key(|&(start, _, _, _)| start);
        let mut last_end: [Option<(usize, i64)>; 3] = [None; 3];
        let mut first_overlap = None;
        for &(start, end, scope, id) in &sorted {
            if last_end[scope].is_some_and(|e| start < e) {
                first_overlap = Some(id);
                break;
            }
            last_end[scope] = Some(end);
        }
        match first_overlap {
            None => prop_assert!(b.build().is_ok()),
            Some(id) => prop_assert_eq!(
                b.build(),
                Err(DocumentError::Invalid(Violation::HairpinOverlap(id)))
            ),
        }
    }
}
