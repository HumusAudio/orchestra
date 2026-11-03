// SPDX-License-Identifier: GPL-3.0-or-later

//! Property tests of the form, second part (markers, jumps, bar repeats, pass rules: rules 10
//! and 11 of `validate`), against an oracle that shares no code with the library (ADR-0016 §4;
//! ADR-0025 §4, §6–§8; SCALETTA F1.10b).
//!
//! # The generator
//! A score of 6–14 bars gets a valid set of repeated sections without voltas (rules 8 and 9 are
//! tested by `form_oracle.rs`), then random markers, jumps between them, bar repeats on the
//! piano's lower staff, pass rules on the trumpet and whole notes in the piano's left hand. Every
//! piece is built so that the builder accepts it; whether the document is valid is what the
//! oracle predicts.
//!
//! # The oracle
//! It states rules 10 and 11 on plain integers: points of the score as (bar, side) pairs, the
//! sections as bar ranges with their plays, the left hand's notes as a set of bars. The library
//! looks the sections up by merging the repeat barlines and compares points with its own keys.

// Helper functions (not `#[test]` functions) are outside the scope of clippy.toml's
// `allow-unwrap-in-tests`, so the exemption is stated here, for this test file only.
#![allow(
    clippy::unwrap_used,
    reason = "test helpers: a failed unwrap fails the test"
)]
// The oracle indexes its own small vectors with indices it generated itself.
#![allow(clippy::indexing_slicing, reason = "test oracle over its own vectors")]

use orchestra_core_model::{
    Alteration, BarId, BarRepeatSpan, BaseValue, Clef, DocumentBuilder, DocumentError,
    DynamicScope, Interval, JumpFires, JumpSpec, JumpTarget, Key, MarkerId, MarkerKind, Metronome,
    Mode, NoteValue, PassSet, Pitch, Placement, RepeatsAfterJump, ScorePoint, Step, TempoMark,
};
use orchestra_core_time::{Duration, Meter, MeterChange, MeterMap, Rational, Tempo};
use proptest::prelude::*;

/// A repeated section: first and last bar, plays.
type Section = (usize, usize, u8);

/// A generated jump: bar, target (`None` = start, else a marker index), until and then (marker
/// indices), and `OnPass(n)` if `Some(n)`.
#[derive(Debug, Clone, Copy)]
struct GJump {
    /// Bar.
    bar: usize,
    /// Target marker index, or the start.
    to: Option<usize>,
    /// Until marker index.
    until: Option<usize>,
    /// Then marker index.
    then: Option<usize>,
    /// `OnPass(n)`.
    on_pass: Option<u8>,
}

/// A generated case.
#[derive(Debug, Clone)]
struct Case {
    /// Number of bars.
    bars: usize,
    /// The repeated sections (valid by construction).
    sections: Vec<Section>,
    /// Markers: (bar, kind), at most one per side of a bar.
    markers: Vec<(usize, MarkerKind)>,
    /// Jumps, at most one per bar.
    jumps: Vec<GJump>,
    /// Bar repeats on the lower staff: (first, count, span).
    bar_repeats: Vec<(usize, u32, BarRepeatSpan)>,
    /// Pass rules on the trumpet: (start bar, end bar, voice scope?, pass mask).
    pass_rules: Vec<(usize, usize, bool, u64)>,
    /// Bars where the left hand has a note.
    left_hand: Vec<usize>,
}

/// The kinds in a fixed order, for the generator.
const KINDS: [MarkerKind; 4] = [
    MarkerKind::Segno,
    MarkerKind::Coda,
    MarkerKind::Fine,
    MarkerKind::ToCoda,
];

/// The spans in a fixed order, for the generator.
const SPANS: [BarRepeatSpan; 3] = [BarRepeatSpan::One, BarRepeatSpan::Two, BarRepeatSpan::Four];

/// A case strategy.
///
/// Each case holds only some of the four kinds of objects (`features`, one bit each), and the
/// choices lean towards valid objects (jumps to the start or without "until", pass rules born
/// inside a section, bar repeats after their source), so that both sides of the oracle are
/// exercised: about half of the cases are valid.
fn cases() -> impl Strategy<Value = Case> {
    (
        (6_usize..15, 0_u8..16),
        prop::collection::vec((0_usize..16, 1_usize..4, 2_u8..4), 1..3),
        prop::collection::vec((0_usize..16, 0_usize..4), 0..4),
        prop::collection::vec(
            (0_usize..16, 0_usize..8, 0_usize..8, 0_usize..8, 0_u8..8),
            1..3,
        ),
        prop::collection::vec((0_usize..16, 1_u32..3, 0_usize..3), 1..3),
        prop::collection::vec((0_usize..8, 0_usize..4, any::<bool>(), 1_u64..8), 1..3),
        prop::collection::vec(0_usize..16, 0..3),
    )
        .prop_map(
            |((bars, features), secs, marks, jumps, reps, rules, notes)| {
                // Sections: sorted, disjoint, inside the score.
                let mut sections: Vec<Section> = Vec::new();
                let mut next = 0;
                let mut starts: Vec<(usize, usize, u8)> = secs
                    .into_iter()
                    .map(|(s, len, plays)| (s % bars, len, plays))
                    .collect();
                starts.sort_unstable();
                for (s, len, plays) in starts {
                    let first = s.max(next);
                    let last = first + len - 1;
                    if last >= bars {
                        break;
                    }
                    sections.push((first, last, plays));
                    next = last + 1;
                }
                // Markers: one per side of a bar.
                let mut markers: Vec<(usize, MarkerKind)> = Vec::new();
                if features & 1 != 0 {
                    for (b, k) in marks {
                        let (bar, kind) = (b % bars, KINDS[k]);
                        if !markers
                            .iter()
                            .any(|&(m, mk)| m == bar && mk.at_start() == kind.at_start())
                        {
                            markers.push((bar, kind));
                        }
                    }
                }
                // Jumps: one per bar; a marker index beyond the markers means "start" or "none",
                // which is the common case (indices up to 8 for at most 4 markers).
                let pick = |i: usize| (i < markers.len()).then_some(i);
                let mut out_jumps: Vec<GJump> = Vec::new();
                if features & 2 != 0 {
                    for (b, to, until, then, pass) in jumps {
                        let bar = b % bars;
                        if out_jumps.iter().any(|j| j.bar == bar) {
                            continue;
                        }
                        out_jumps.push(GJump {
                            bar,
                            to: pick(to),
                            until: pick(until),
                            then: pick(then),
                            // Mostly "after the repeats"; sometimes a pass, possibly invalid.
                            on_pass: pass.checked_sub(5),
                        });
                    }
                }
                // Bar repeats, after their source most of the time.
                let mut bar_repeats = Vec::new();
                if features & 4 != 0 {
                    for (f, count, s) in reps {
                        let span = SPANS[s];
                        let first = (f % bars)
                            .max(usize::try_from(span.bars()).unwrap() * usize::from(f % 5 != 0));
                        let count = count * span.bars();
                        if first + usize::try_from(count).unwrap() <= bars {
                            bar_repeats.push((first, count, span));
                        }
                    }
                }
                // Pass rules born inside a section (by index), ending inside or at its end.
                let mut pass_rules = Vec::new();
                if features & 8 != 0 && !sections.is_empty() {
                    for (k, len, voice, mask) in rules {
                        let (first, last, _) = sections[k % sections.len()];
                        let start = first + len % (last - first + 1);
                        let end = (start + 1 + len % 2).min(bars - 1).max(start + 1);
                        if end < bars {
                            pass_rules.push((start, end, voice, mask));
                        }
                    }
                }
                let mut left_hand: Vec<usize> = notes.into_iter().map(|n| n % bars).collect();
                left_hand.sort_unstable();
                left_hand.dedup();
                Case {
                    bars,
                    sections,
                    markers,
                    jumps: out_jumps,
                    bar_repeats,
                    pass_rules,
                    left_hand,
                }
            },
        )
}

// ------------------------------------------------------------------------------------------------
// The oracle
// ------------------------------------------------------------------------------------------------

/// The section containing bar `bar`.
fn section(c: &Case, bar: usize) -> Option<Section> {
    c.sections
        .iter()
        .copied()
        .find(|&(f, l, _)| f <= bar && bar <= l)
}

/// Whether rule 10 holds.
fn jumps_valid(c: &Case) -> bool {
    for j in &c.jumps {
        let target = match j.to {
            None => (0, 0),
            Some(i) if c.markers[i].1 == MarkerKind::Segno => (c.markers[i].0, 0),
            Some(_) => return false,
        };
        let here = (j.bar, 1);
        let until = j.until.map(|i| c.markers[i]);
        let then = j.then.map(|i| c.markers[i].1);
        let ok_kinds = matches!(
            (until, then),
            (None | Some((_, MarkerKind::Fine)), None)
                | (Some((_, MarkerKind::ToCoda)), Some(MarkerKind::Coda))
        );
        if !ok_kinds {
            return false;
        }
        if let Some((bar, _)) = until
            && ((bar, 1) < target || (bar, 1) > here)
        {
            return false;
        }
        if let Some(n) = j.on_pass {
            let plays = section(c, j.bar).map_or(0, |s| s.2);
            if n == 0 || n > plays {
                return false;
            }
        }
    }
    true
}

/// Whether a form marking lies on a barline strictly inside bars `from..=to`.
fn marking_inside(c: &Case, from: usize, to: usize) -> bool {
    let start_inside = |k: usize| from < k && k <= to;
    let end_inside = |k: usize| from <= k && k < to;
    c.sections
        .iter()
        .any(|&(f, l, _)| start_inside(f) || end_inside(l))
        || c.markers.iter().any(|&(b, k)| {
            if k.at_start() {
                start_inside(b)
            } else {
                end_inside(b)
            }
        })
        || c.jumps.iter().any(|j| end_inside(j.bar))
}

/// Whether rule 11 holds.
fn bars_and_passes_valid(c: &Case) -> bool {
    // Bar repeats, in the library's order: by first bar, creation order at one bar.
    let mut reps = c.bar_repeats.clone();
    reps.sort_by_key(|r| r.0);
    let mut last_end: Option<usize> = None;
    for (first, count, span) in reps {
        let span = usize::try_from(span.bars()).unwrap();
        let last = first + usize::try_from(count).unwrap() - 1;
        if first < span {
            return false;
        }
        if last_end.is_some_and(|e| first <= e) {
            return false;
        }
        last_end = Some(last);
        if c.left_hand.iter().any(|&b| first <= b && b <= last) {
            return false;
        }
        if marking_inside(c, first - span, last) {
            return false;
        }
    }
    // Pass rules, by start; one end per scope.
    let mut rules = c.pass_rules.clone();
    rules.sort_by_key(|r| r.0);
    let mut ends: [Option<usize>; 2] = [None, None];
    for (start, end, voice, mask) in rules {
        let Some((_, last, plays)) = section(c, start) else {
            return false;
        };
        if end > last + 1 {
            return false;
        }
        let highest = 64 - mask.leading_zeros();
        if highest > u32::from(plays) {
            return false;
        }
        let slot = &mut ends[usize::from(voice)];
        if slot.is_some_and(|e| start < e) {
            return false;
        }
        *slot = Some(end);
    }
    true
}

// ------------------------------------------------------------------------------------------------
// The library side
// ------------------------------------------------------------------------------------------------

/// Builds the case with the library.
fn build(c: &Case) -> DocumentBuilder {
    let meters = MeterMap::new(MeterChange::new(Meter::new(&[4], 4).unwrap())).unwrap();
    let tempo = Tempo::new(
        Duration::from_rational(Rational::new(1, 4).unwrap()),
        Rational::from_integer(90).unwrap(),
    )
    .unwrap();
    let mut b = DocumentBuilder::new(
        meters,
        TempoMark::immediate(None, Metronome::shown(tempo)).unwrap(),
        Key::traditional(0, Mode::Major).unwrap(),
    )
    .unwrap();
    let ids: Vec<BarId> = (0..c.bars).map(|_| b.add_bar().unwrap()).collect();
    let unison = Interval::new(0, Rational::ZERO);
    let piano = b
        .add_instrument_track("Piano", unison, &[Clef::TREBLE, Clef::BASS])
        .unwrap();
    let trumpet = b
        .add_instrument_track("Trumpet", unison, &[Clef::TREBLE])
        .unwrap();
    let (lower, left, trumpet, trumpet_voice) = {
        let doc = b.document();
        let p = doc.track(piano).unwrap().instrument();
        let t = doc.track(trumpet).unwrap().instrument();
        (
            p.staves()[1].id(),
            p.voices()[1].id(),
            t.id(),
            t.voices()[0].id(),
        )
    };
    for &(first, last, plays) in &c.sections {
        b.add_repeat_start(ids[first]).unwrap();
        b.add_repeat_end(ids[last], plays).unwrap();
    }
    let markers: Vec<MarkerId> = c
        .markers
        .iter()
        .map(|&(bar, kind)| b.add_marker(ids[bar], kind, None).unwrap())
        .collect();
    for j in &c.jumps {
        let spec = JumpSpec {
            to: j
                .to
                .map_or(JumpTarget::Start, |i| JumpTarget::Marker(markers[i])),
            until: j.until.map(|i| markers[i]),
            then: j.then.map(|i| markers[i]),
            repeats: RepeatsAfterJump::Skip,
            fires: j.on_pass.map_or(JumpFires::AfterRepeats, JumpFires::OnPass),
        };
        b.add_jump(ids[j.bar], spec, None).unwrap();
    }
    for &(first, count, span) in &c.bar_repeats {
        b.add_bar_repeat(lower, ids[first], count, span).unwrap();
    }
    for &(start, end, voice, mask) in &c.pass_rules {
        let passes: Vec<u8> = (1..=64_u8).filter(|p| mask >> (p - 1) & 1 == 1).collect();
        let scope = if voice {
            DynamicScope::Voice(trumpet_voice)
        } else {
            DynamicScope::Instrument
        };
        b.add_pass_rule(
            trumpet,
            ScorePoint::start_of(ids[start]),
            ScorePoint::start_of(ids[end]),
            scope,
            PassSet::of(&passes).unwrap(),
            None,
        )
        .unwrap();
    }
    let note = Pitch::new(Step::C, Alteration::NATURAL, 3);
    for &bar in &c.left_hand {
        b.add_chord(
            Placement::new(left, ids[bar], Duration::ZERO),
            NoteValue::plain(BaseValue::Whole),
            &[note],
        )
        .unwrap();
    }
    b
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1024))]

    /// `build` accepts exactly the cases where rules 10 and 11 hold.
    #[test]
    fn jumps_bar_repeats_and_pass_rules_match_the_oracle(c in cases()) {
        let expected = jumps_valid(&c) && bars_and_passes_valid(&c);
        match build(&c).build() {
            Ok(_) => prop_assert!(expected, "accepted an invalid case: {c:?}"),
            Err(DocumentError::Invalid(v)) => {
                prop_assert!(!expected, "refused a valid case with {v:?}: {c:?}");
            }
            Err(other) => prop_assert!(false, "unexpected error {other:?}: {c:?}"),
        }
    }
}
