// SPDX-License-Identifier: GPL-3.0-or-later

//! Property tests of the form, first part (repeats, voltas, bar numbers), against an oracle that
//! shares no code with the library (ADR-0016 §4; ADR-0025 §2–§3, §8; SCALETTA F1.10b).
//!
//! # The generator
//! Random scores of 4–24 bars get a sequence of sections, each with an optional start repeat, a
//! body, an end repeat or a group of 2–3 voltas (with combined voltas such as "1.–2."), and random
//! `times`; then, with some probability, a mutation that may break a rule (a dropped or extra
//! barline, a moved or removed volta, other passes, another number of plays). Random uncounted
//! bars, an optional numbering restart and a random style complete the case.
//!
//! # The oracle
//! It states the rules of ADR-0025 again in its own terms, on plain bar indices and `u64` pass
//! masks: voltas sorted and disjoint, grouped from the one holding pass 1, consecutive, ascending,
//! covering 1..=n, at least two per group, every volta but the last ending with an end repeat whose
//! `times` is 2 or n, no start repeat inside a group; repeats paired in score order, an end without
//! a start allowed only as the first end or as a further end of a group's voltas. For bar numbers
//! it **walks the bars one by one** with a counter, the way a copyist numbers a part: plain
//! numbers with restarts and uncounted bars, and in the "same as first" styles the counter goes
//! back to the first volta's number at the start of each later volta, and after the group to that
//! number plus the longest volta; a restart inside a group ends the group's special numbering.
//! The library computes the same numbers per bar, with shifts: the two methods share nothing.

// Helper functions (not `#[test]` functions) are outside the scope of clippy.toml's
// `allow-unwrap-in-tests`, so the exemption is stated here, for this test file only.
#![allow(
    clippy::unwrap_used,
    reason = "test helpers: a failed unwrap fails the test"
)]
// The oracle indexes its own small vectors with indices it generated itself.
#![allow(clippy::indexing_slicing, reason = "test oracle over its own vectors")]

use orchestra_core_model::{
    BarId, BarNumbering, DocumentBuilder, DocumentError, Key, Metronome, Mode, PassSet, TempoMark,
};
use orchestra_core_time::{BarIndex, Duration, Meter, MeterChange, MeterMap, Rational, Tempo};
use proptest::prelude::*;

/// A volta of the generated case: first and last bar index, pass mask.
#[derive(Debug, Clone, Copy, PartialEq)]
struct V {
    /// First bar index.
    first: usize,
    /// Last bar index.
    last: usize,
    /// Bit `k − 1` for pass `k`.
    mask: u64,
}

/// A generated case.
#[derive(Debug, Clone)]
struct Case {
    /// Number of bars.
    bars: usize,
    /// Start repeats (bar indices, distinct).
    starts: Vec<usize>,
    /// End repeats (bar index, times), distinct bars.
    ends: Vec<(usize, u8)>,
    /// Voltas.
    voltas: Vec<V>,
    /// Uncounted bars.
    uncounted: Vec<usize>,
    /// A numbering restart (bar index, number).
    restart: Option<(usize, u32)>,
    /// The style: 0 continue, 1 lettered, 2 plain same-as-first.
    style: u8,
}

/// Builds a case from random choices: `plan` drives the sections, `mutation` the corruption.
fn case(plan: &[u8], mutation: (u8, u8), extras: (u32, u8, u8)) -> Case {
    let mut c = Case {
        bars: 0,
        starts: Vec::new(),
        ends: Vec::new(),
        voltas: Vec::new(),
        uncounted: Vec::new(),
        restart: None,
        style: extras.2 % 3,
    };
    let mut bar = 0_usize;
    for (k, &p) in plan.iter().enumerate() {
        // Start repeat, except sometimes for the very first section (implicit start).
        if !(k == 0 && p % 2 == 0) {
            c.starts.push(bar);
        }
        let body = 1 + usize::from(p % 3);
        bar += body;
        let times = [2_u8, 2, 3, 4][usize::from(p / 3 % 4)];
        if p % 5 < 2 {
            // No voltas: the end closes the body.
            c.ends.push((bar - 1, times));
            continue;
        }
        // A volta group: 2 or 3 voltas, lengths 1–2, passes 1..=n split in order.
        let count = 2 + usize::from(p / 5 % 2);
        let combined = p / 10 % 2 == 1;
        let mut next_pass = 1_u32;
        for v in 0..count {
            let len = 1 + usize::from((p >> v) % 2);
            let width = if combined && v == 0 { 2 } else { 1 };
            let mask = ((1_u64 << width) - 1) << (next_pass - 1);
            next_pass += width;
            c.voltas.push(V {
                first: bar,
                last: bar + len - 1,
                mask,
            });
            if v + 1 < count {
                let n =
                    u8::try_from(next_pass - 1 + u32::try_from(count - v - 1).unwrap()).unwrap();
                let t = if p % 7 == 0 { n } else { 2 };
                c.ends.push((bar + len - 1, t));
            }
            bar += len;
        }
    }
    bar += 1;
    c.bars = bar;
    mutate(&mut c, mutation);
    // Uncounted bars and a restart.
    let (bits, restart_at, _) = extras;
    for b in 0..c.bars.min(32) {
        if bits >> b & 1 == 1 && b % 5 == 4 {
            c.uncounted.push(b);
        }
    }
    if restart_at % 3 == 0 {
        c.restart = Some((usize::from(restart_at) % c.bars, 30));
    }
    c.starts.sort_unstable();
    c.ends.sort_unstable();
    c.voltas.sort_by_key(|v| v.first);
    c
}

/// Applies one mutation that may break a rule: drop an end or a start, add a stray start or end,
/// change or remove a volta, change a number of plays. `kind` picks it, `which` the target.
fn mutate(c: &mut Case, (kind, which): (u8, u8)) {
    let w = usize::from(which);
    match kind % 8 {
        0 if !c.ends.is_empty() => {
            let i = w % c.ends.len();
            c.ends.remove(i);
        }
        1 if !c.starts.is_empty() => {
            let i = w % c.starts.len();
            c.starts.remove(i);
        }
        2 => {
            let at = w % c.bars;
            if !c.starts.contains(&at) {
                c.starts.push(at);
            }
        }
        3 => {
            let at = w % c.bars;
            if !c.ends.iter().any(|e| e.0 == at) {
                c.ends.push((at, 2));
            }
        }
        4 if !c.voltas.is_empty() => {
            let i = w % c.voltas.len();
            c.voltas[i].mask <<= 1;
        }
        5 if !c.voltas.is_empty() => {
            let i = w % c.voltas.len();
            if c.voltas[i].last + 1 < c.bars {
                c.voltas[i].first += 1;
                c.voltas[i].last += 1;
            }
        }
        6 if !c.voltas.is_empty() => {
            // A volta removed: a group may be left with one volta, or with a gap.
            let i = w % c.voltas.len();
            c.voltas.remove(i);
        }
        7 if !c.ends.is_empty() => {
            // Another number of plays: it may contradict the voltas of its section.
            let i = w % c.ends.len();
            c.ends[i].1 = 2 + which % 4;
        }
        _ => {}
    }
}

/// A case strategy.
fn cases() -> impl Strategy<Value = Case> {
    (
        prop::collection::vec(0_u8..=255, 1..5),
        (0_u8..16, 0_u8..=255),
        (0_u32.., 0_u8..=255, 0_u8..3),
    )
        .prop_map(|(plan, mutation, extras)| case(&plan, mutation, extras))
}

// ------------------------------------------------------------------------------------------------
// The oracle: validity
// ------------------------------------------------------------------------------------------------

/// The volta groups of a case, as index ranges into `voltas`, or `None` if the voltas cannot be
/// grouped (overlap, a group not starting with pass 1, not consecutive, passes not ascending).
fn groups(c: &Case) -> Option<Vec<(usize, usize)>> {
    let mut out: Vec<(usize, usize)> = Vec::new();
    for (i, v) in c.voltas.iter().enumerate() {
        if i > 0 && v.first <= c.voltas[i - 1].last {
            return None;
        }
        if v.mask & 1 == 1 {
            out.push((i, i + 1));
            continue;
        }
        let g = out.last_mut()?;
        let prev = c.voltas[i - 1];
        // Ascending: every pass of `v` above every pass of the previous voltas of the group.
        let below: u64 = c.voltas[g.0..i].iter().fold(0, |m, x| m | x.mask);
        let highest_below = 64 - below.leading_zeros();
        let lowest_here = v.mask.trailing_zeros() + 1;
        if v.first != prev.last + 1 || v.mask == 0 || lowest_here <= highest_below {
            return None;
        }
        g.1 = i + 1;
    }
    Some(out)
}

/// Whether the case is valid by the rules of ADR-0025 §8, rules 8 and 9.
fn valid(c: &Case) -> bool {
    let Some(groups) = groups(c) else {
        return false;
    };
    let end_at = |bar: usize| c.ends.iter().find(|e| e.0 == bar).copied();
    let mut continuation_bars: Vec<usize> = Vec::new();
    for &(a, z) in &groups {
        let vs = &c.voltas[a..z];
        if vs.len() < 2 {
            return false;
        }
        let union = vs.iter().fold(0_u64, |m, v| m | v.mask);
        // Exactly 1..=n: a run of ones from bit 0.
        if union & (union + 1) != 0 {
            return false;
        }
        let n = u8::try_from(64 - union.leading_zeros()).unwrap();
        for (k, v) in vs[..vs.len() - 1].iter().enumerate() {
            let Some((_, times)) = end_at(v.last) else {
                return false;
            };
            if times != 2 && times != n {
                return false;
            }
            if k > 0 {
                continuation_bars.push(v.last);
            }
        }
        let (from, to) = (vs[0].first, vs[vs.len() - 1].last);
        if c.starts.iter().any(|&s| s > from && s <= to) {
            return false;
        }
    }
    // Repeats in score order: (bar, 0) for a start, (bar, 1) for an end.
    let mut marks: Vec<(usize, u8)> = c.starts.iter().map(|&s| (s, 0)).collect();
    marks.extend(c.ends.iter().map(|e| (e.0, 1)));
    marks.sort_unstable();
    let (mut open, mut seen_end) = (false, false);
    for (bar, kind) in marks {
        if kind == 0 {
            if open {
                return false;
            }
            open = true;
        } else {
            if !open && seen_end && !continuation_bars.contains(&bar) {
                return false;
            }
            open = false;
            seen_end = true;
        }
    }
    true
}

// ------------------------------------------------------------------------------------------------
// The oracle: bar numbers, bar by bar
// ------------------------------------------------------------------------------------------------

/// The labels of every bar of a valid case: (number, letter place), `None` for an uncounted bar.
fn numbers(c: &Case) -> Vec<Option<(u32, Option<u8>)>> {
    let groups = groups(c).unwrap();
    // Where each bar is: (group index, volta place) if inside a group.
    let place = |bar: usize| {
        groups.iter().enumerate().find_map(|(g, &(a, z))| {
            c.voltas[a..z]
                .iter()
                .position(|v| v.first <= bar && bar <= v.last)
                .map(|p| (g, p))
        })
    };
    let mut out = Vec::new();
    let mut counter = 1_u32;
    // The state of the group being numbered: (group, first volta's number, longest so far,
    // counted bars in the current volta, current place), or abandoned after a restart.
    let mut group: Option<(usize, u32, u32, u32, usize)> = None;
    let mut abandoned: Option<usize> = None;
    for bar in 0..c.bars {
        if let Some((at, n)) = c.restart
            && at == bar
        {
            counter = n;
            if let Some(g) = group.take() {
                abandoned = Some(g.0);
            } else if let Some((g, _)) = place(bar)
                && c.voltas[groups[g].0].first != bar
            {
                // A restart inside a group ends its special numbering; one exactly at the group's
                // first bar only sets the number the voltas start from.
                abandoned = Some(g);
            }
        }
        let here = place(bar);
        let style_on = c.style != 0;
        // Leaving a group: continue after its longest volta.
        if let Some((g, first, longest, current, _)) = group
            && here.map(|h| h.0) != Some(g)
        {
            counter = first + longest.max(current);
            group = None;
        }
        if style_on
            && let Some((g, p)) = here
            && abandoned != Some(g)
        {
            match group.as_mut() {
                None => group = Some((g, counter, 0, 0, p)),
                Some(state) if state.4 != p => {
                    // A new volta of the group: back to the first volta's number.
                    state.2 = state.2.max(state.3);
                    state.3 = 0;
                    state.4 = p;
                    counter = state.1;
                }
                Some(_) => {}
            }
        }
        if c.uncounted.contains(&bar) {
            out.push(None);
            continue;
        }
        let letter = (c.style == 1)
            .then(|| group.map(|g| u8::try_from(g.4 + 1).unwrap()))
            .flatten();
        out.push(Some((counter, letter)));
        counter += 1;
        if let Some(state) = group.as_mut() {
            state.3 += 1;
        }
    }
    out
}

// ------------------------------------------------------------------------------------------------
// The library side
// ------------------------------------------------------------------------------------------------

/// The library builder for a case, and its bar IDs.
fn build(c: &Case) -> (DocumentBuilder, Vec<BarId>) {
    let mut meters = MeterMap::new(MeterChange::new(Meter::new(&[3], 4).unwrap())).unwrap();
    for &u in &c.uncounted {
        meters.set_uncounted(BarIndex::new(u32::try_from(u).unwrap()), true);
    }
    if let Some((at, n)) = c.restart {
        meters.set_numbering_restart(BarIndex::new(u32::try_from(at).unwrap()), Some(n));
    }
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
    for &s in &c.starts {
        b.add_repeat_start(ids[s]).unwrap();
    }
    for &(e, times) in &c.ends {
        b.add_repeat_end(ids[e], times).unwrap();
    }
    for v in &c.voltas {
        let passes: Vec<u8> = (1..=64_u8).filter(|p| v.mask >> (p - 1) & 1 == 1).collect();
        b.add_volta(
            ids[v.first],
            ids[v.last],
            PassSet::of(&passes).unwrap(),
            true,
            None,
        )
        .unwrap();
    }
    b.set_bar_numbering(match c.style {
        0 => BarNumbering::Continue,
        1 => BarNumbering::SameAsFirstLettered,
        _ => BarNumbering::SameAsFirst,
    });
    (b, ids)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1024))]

    /// `build` accepts exactly the valid cases, and the bar numbers of every valid case match the
    /// oracle's bar-by-bar count.
    #[test]
    fn form_matches_the_oracle(c in cases()) {
        // Voltas with an empty mask (shifted past bit 63) cannot even be built: skip them.
        prop_assume!(c.voltas.iter().all(|v| v.mask != 0));
        let (b, ids) = build(&c);
        let expected = valid(&c);
        match b.build() {
            Ok(document) => {
                prop_assert!(expected, "accepted an invalid form: {c:?}");
                let want = numbers(&c);
                for (bar, &id) in ids.iter().enumerate() {
                    let got = document
                        .bar_label(id)
                        .unwrap()
                        .map(|l| (l.number, l.volta));
                    prop_assert_eq!(got, want[bar], "bar {} of {:?}", bar, c);
                }
            }
            Err(DocumentError::Invalid(v)) => {
                prop_assert!(!expected, "refused a valid form with {v:?}: {c:?}");
            }
            Err(other) => prop_assert!(false, "unexpected error {other:?}: {c:?}"),
        }
    }
}
