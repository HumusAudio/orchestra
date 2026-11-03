// SPDX-License-Identifier: GPL-3.0-or-later

//! Hand-written cases for the form, first part (ADR-0025 §2–§3, SCALETTA F1.10b): pass sets,
//! repeats, voltas, rules 8 and 9 of `validate`, the builder's checks, and the bar numbers of the
//! three styles with the examples of ADR-0025 §3. The cases of ADR-0025 §10 that belong to this
//! part are named after it.
//!
//! Bars are written here by their index in the score from 0, and the displayed numbers start at 1
//! (no pickup), so bar index 7 is "bar 8".

// Helper functions (not `#[test]` functions) are outside the scope of clippy.toml's
// `allow-unwrap-in-tests`, so the exemption is stated here, for this test file only.
#![allow(
    clippy::unwrap_used,
    reason = "test helpers: a failed unwrap fails the test"
)]
// Same reasoning for the helper that fails the test when `build` does not report a violation.
#![allow(clippy::panic, reason = "test helpers: a panic fails the test")]

use orchestra_core_model::{
    BarId, BarLabel, BarNumbering, DocumentBuilder, DocumentError, Key, MAX_PLAYS, Metronome, Mode,
    PassSet, TempoMark, Violation,
};
use orchestra_core_time::{BarIndex, Duration, Meter, MeterChange, MeterMap, Rational, Tempo};

/// A builder in 4/4 with `bars` bars, the meter map changed by `edit` first.
fn builder_with(bars: usize, edit: impl FnOnce(&mut MeterMap)) -> (DocumentBuilder, Vec<BarId>) {
    let mut meters = MeterMap::new(MeterChange::new(Meter::new(&[4], 4).unwrap())).unwrap();
    edit(&mut meters);
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
    let ids = (0..bars).map(|_| b.add_bar().unwrap()).collect();
    (b, ids)
}

/// A builder in 4/4 with `bars` bars.
fn builder(bars: usize) -> (DocumentBuilder, Vec<BarId>) {
    builder_with(bars, |_| {})
}

/// The passes `first..=last`.
fn passes(first: u8, last: u8) -> PassSet {
    PassSet::range(first, last).unwrap()
}

/// The labels of every bar, as text ("8", "8a", "–" for an uncounted bar).
fn labels(b: DocumentBuilder, bars: &[BarId]) -> Vec<String> {
    let document = b.build().unwrap();
    bars.iter()
        .map(|&bar| match document.bar_label(bar).unwrap() {
            None => "–".to_owned(),
            Some(label) => format!(
                "{}{}",
                label.number,
                label.letter().map(String::from).unwrap_or_default()
            ),
        })
        .collect()
}

/// The violation `build` reports.
fn violation(b: DocumentBuilder) -> Violation {
    match b.build() {
        Err(DocumentError::Invalid(v)) => v,
        other => panic!("expected a violation, got {other:?}"),
    }
}

/// Pass sets: one bit per pass, ranges, the bounds, the prefix test.
#[test]
fn pass_sets() {
    let first_three = passes(1, 3);
    assert_eq!(first_three, PassSet::of(&[1, 2, 3]).unwrap());
    assert!(first_three.contains(2) && !first_three.contains(4) && !first_three.contains(0));
    assert_eq!(
        (first_three.first(), first_three.last()),
        (Some(1), Some(3))
    );
    assert_eq!(first_three.len(), 3);
    assert!(first_three.is_prefix());
    assert!(!PassSet::of(&[1, 3]).unwrap().is_prefix());
    assert!(!PassSet::of(&[2]).unwrap().is_prefix());
    assert!(!PassSet::EMPTY.is_prefix());
    let top = passes(MAX_PLAYS, MAX_PLAYS);
    assert_eq!((top.first(), top.last()), (Some(64), Some(64)));
    assert_eq!(passes(1, MAX_PLAYS).bits(), u64::MAX);
    assert!(passes(1, 2).is_disjoint(passes(3, 4)));
    assert!(passes(4, 2).is_empty());
    assert_eq!(PassSet::of(&[0]), Err(DocumentError::PassOutOfRange(0)));
    assert_eq!(
        PassSet::range(1, 65),
        Err(DocumentError::PassOutOfRange(65))
    );
}

/// ADR-0025 §10, "Repeat ×3 without voltas" and "End repeat without start": both valid, and the
/// end keeps its number of plays.
#[test]
fn simple_repeats() {
    let (mut b, bars) = builder(8);
    // An end without start: back to the start of the score.
    b.add_repeat_end(bars[1], 2).unwrap();
    b.add_repeat_start(bars[2]).unwrap();
    let thrice = b.add_repeat_end(bars[3], 3).unwrap();
    // A one-bar section, and a start without end at the end (inert while being written).
    b.add_repeat_start(bars[5]).unwrap();
    b.add_repeat_end(bars[5], 2).unwrap();
    b.add_repeat_start(bars[7]).unwrap();
    let document = b.build().unwrap();
    let ends = document.form().repeat_ends();
    assert_eq!(ends.len(), 3);
    assert_eq!(ends[1].id(), thrice);
    assert_eq!(ends[1].times(), 3);
    assert_eq!(document.form().repeat_starts().len(), 3);
}

/// ADR-0025 §10, "Nested repeats": nesting and a second end without start are refused.
#[test]
fn repeats_do_not_nest() {
    let (mut b, bars) = builder(6);
    b.add_repeat_start(bars[0]).unwrap();
    let inner = b.add_repeat_start(bars[1]).unwrap();
    b.add_repeat_end(bars[2], 2).unwrap();
    b.add_repeat_end(bars[3], 2).unwrap();
    assert_eq!(violation(b), Violation::RepeatsOverlap(inner));

    // "‖: A :‖ B :‖": the second end would go back to the start of the score, over the first.
    let (mut b, bars) = builder(6);
    b.add_repeat_start(bars[0]).unwrap();
    b.add_repeat_end(bars[1], 2).unwrap();
    let second = b.add_repeat_end(bars[3], 2).unwrap();
    assert_eq!(violation(b), Violation::RepeatsOverlap(second));
}

/// ADR-0025 §10, "First and second volta, open second", and "1.–3." with "4.".
#[test]
fn voltas() {
    let (mut b, bars) = builder(10);
    b.add_repeat_start(bars[0]).unwrap();
    let first = b
        .add_volta(bars[3], bars[3], passes(1, 1), true, None)
        .unwrap();
    b.add_repeat_end(bars[3], 2).unwrap();
    let second = b
        .add_volta(bars[4], bars[4], passes(2, 2), false, None)
        .unwrap();
    // A second section with "1.–3." and "4.": `times` may be 2 (the default) or 4.
    b.add_repeat_start(bars[5]).unwrap();
    b.add_volta(bars[7], bars[7], passes(1, 3), true, Some("1.–3."))
        .unwrap();
    b.add_repeat_end(bars[7], 4).unwrap();
    b.add_volta(bars[8], bars[9], passes(4, 4), false, None)
        .unwrap();
    let document = b.build().unwrap();
    let voltas = document.form().voltas();
    assert_eq!(voltas.len(), 4);
    assert_eq!((voltas[0].id(), voltas[1].id()), (first, second));
    assert!(voltas[0].closed() && !voltas[1].closed());
    assert_eq!(voltas[2].text(), Some("1.–3."));
    assert_eq!(voltas[3].last(), bars[9]);
}

/// "1. :‖ 2. :‖ 3." (ADR-0025 §3 as corrected on 2026-11-03): every volta but the last ends with
/// an end repeat, the second one without a start of its own.
#[test]
fn every_volta_but_the_last_ends_with_a_repeat() {
    let (mut b, bars) = builder(6);
    b.add_repeat_start(bars[0]).unwrap();
    b.add_volta(bars[2], bars[2], passes(1, 1), true, None)
        .unwrap();
    b.add_repeat_end(bars[2], 3).unwrap();
    b.add_volta(bars[3], bars[3], passes(2, 2), true, None)
        .unwrap();
    b.add_repeat_end(bars[3], 3).unwrap();
    b.add_volta(bars[4], bars[5], passes(3, 3), false, None)
        .unwrap();
    assert!(b.clone().build().is_ok());

    // Without the second end, the second volta could not go back.
    let (mut b, bars) = builder(6);
    b.add_repeat_start(bars[0]).unwrap();
    b.add_volta(bars[2], bars[2], passes(1, 1), true, None)
        .unwrap();
    b.add_repeat_end(bars[2], 3).unwrap();
    let lonely = b
        .add_volta(bars[3], bars[3], passes(2, 2), true, None)
        .unwrap();
    b.add_volta(bars[4], bars[4], passes(3, 3), false, None)
        .unwrap();
    assert_eq!(violation(b), Violation::VoltaWithoutRepeat(lonely));
}

/// ADR-0025 §10, "gaps in volta numbers" and the other rule-9 violations.
#[test]
fn volta_rules() {
    // A gap: {1} then {3}.
    let (mut b, bars) = builder(6);
    let first = b
        .add_volta(bars[2], bars[2], passes(1, 1), true, None)
        .unwrap();
    b.add_repeat_end(bars[2], 2).unwrap();
    b.add_volta(bars[3], bars[3], passes(3, 3), false, None)
        .unwrap();
    assert_eq!(violation(b), Violation::VoltaPassesIncomplete(first));

    // A group that does not start with pass 1.
    let (mut b, bars) = builder(6);
    let orphan = b
        .add_volta(bars[2], bars[2], passes(2, 2), true, None)
        .unwrap();
    assert_eq!(violation(b), Violation::VoltaOrder(orphan));

    // A later volta that does not follow directly.
    let (mut b, bars) = builder(6);
    b.add_volta(bars[2], bars[2], passes(1, 1), true, None)
        .unwrap();
    b.add_repeat_end(bars[2], 2).unwrap();
    let apart = b
        .add_volta(bars[4], bars[4], passes(2, 2), false, None)
        .unwrap();
    assert_eq!(violation(b), Violation::VoltaOrder(apart));

    // A single volta.
    let (mut b, bars) = builder(6);
    let single = b
        .add_volta(bars[2], bars[2], passes(1, 1), true, None)
        .unwrap();
    b.add_repeat_end(bars[2], 2).unwrap();
    assert_eq!(violation(b), Violation::VoltaWithoutRepeat(single));

    // Overlapping voltas.
    let (mut b, bars) = builder(6);
    b.add_volta(bars[2], bars[3], passes(1, 1), true, None)
        .unwrap();
    let over = b
        .add_volta(bars[3], bars[4], passes(2, 2), false, None)
        .unwrap();
    assert_eq!(violation(b), Violation::VoltaOverlap(over));

    // A start repeat inside the group.
    let (mut b, bars) = builder(6);
    b.add_volta(bars[2], bars[2], passes(1, 1), true, None)
        .unwrap();
    b.add_repeat_end(bars[2], 2).unwrap();
    b.add_volta(bars[3], bars[4], passes(2, 2), false, None)
        .unwrap();
    let inside = b.add_repeat_start(bars[4]).unwrap();
    assert_eq!(violation(b), Violation::RepeatsOverlap(inside));

    // `times` contradicting the voltas: three plays written, two voltas.
    let (mut b, bars) = builder(6);
    b.add_volta(bars[2], bars[2], passes(1, 1), true, None)
        .unwrap();
    let three = b.add_repeat_end(bars[2], 3).unwrap();
    b.add_volta(bars[3], bars[3], passes(2, 2), false, None)
        .unwrap();
    assert_eq!(violation(b), Violation::RepeatTimesMismatch(three));
}

/// The builder's own checks, all or nothing.
#[test]
fn builder_checks() {
    let (mut b, bars) = builder(3);
    let (_, foreign) = builder(9);
    let before = b.document().clone();
    assert_eq!(
        b.add_repeat_end(bars[0], 1),
        Err(DocumentError::PlaysOutOfRange(1))
    );
    assert_eq!(
        b.add_repeat_end(bars[0], MAX_PLAYS + 1),
        Err(DocumentError::PlaysOutOfRange(65))
    );
    assert_eq!(
        b.add_repeat_start(foreign[8]),
        Err(DocumentError::UnknownBar(foreign[8]))
    );
    assert_eq!(
        b.add_volta(bars[2], bars[1], passes(1, 1), true, None),
        Err(DocumentError::VoltaNotForward)
    );
    assert_eq!(
        b.add_volta(bars[1], bars[1], PassSet::EMPTY, true, None),
        Err(DocumentError::EmptyPassSet)
    );
    assert_eq!(
        b.add_volta(bars[1], bars[1], passes(1, 1), true, Some(" ")),
        Err(DocumentError::EmptyText)
    );
    assert_eq!(b.document(), &before);
    b.add_repeat_start(bars[1]).unwrap();
    assert_eq!(
        b.add_repeat_start(bars[1]),
        Err(DocumentError::DuplicateMark)
    );
    b.add_repeat_end(bars[1], 2).unwrap();
    assert_eq!(
        b.add_repeat_end(bars[1], 3),
        Err(DocumentError::DuplicateMark)
    );
}

/// A repeat of bars 1–8 (indices 0–7) with one-bar voltas at bars 8 and 9, then two more bars: the
/// first example of ADR-0025 §3, in the three styles.
fn example_one(style: BarNumbering) -> Vec<String> {
    let (mut b, bars) = builder(11);
    b.add_volta(bars[7], bars[7], passes(1, 1), true, None)
        .unwrap();
    b.add_repeat_end(bars[7], 2).unwrap();
    b.add_volta(bars[8], bars[8], passes(2, 2), false, None)
        .unwrap();
    b.set_bar_numbering(style);
    labels(b, &bars[6..])
}

/// ADR-0025 §3, first example: 7 | 8 | 9 | 10 11, 7 | 8a | 8b | 9 10, 7 | 8 | 8 | 9 10.
#[test]
fn bar_numbers_example_one() {
    assert_eq!(
        example_one(BarNumbering::Continue),
        ["7", "8", "9", "10", "11"]
    );
    assert_eq!(
        example_one(BarNumbering::SameAsFirstLettered),
        ["7", "8a", "8b", "9", "10"]
    );
    assert_eq!(
        example_one(BarNumbering::SameAsFirst),
        ["7", "8", "8", "9", "10"]
    );
}

/// ADR-0025 §3, two-bar voltas (7 8 | 9 10 | 11 → 7a 8a | 7b 8b | 9) and voltas of different
/// lengths (7a 8a | 7b | 9), and the owner's rule of 2026-11-03 for a later volta longer than the
/// first (8a | 8b 9b | 10).
#[test]
fn bar_numbers_of_longer_voltas() {
    let lettered = BarNumbering::SameAsFirstLettered;
    let build = |first: (usize, usize), second: (usize, usize), total: usize| {
        let (mut b, bars) = builder(total);
        b.add_volta(bars[first.0], bars[first.1], passes(1, 1), true, None)
            .unwrap();
        b.add_repeat_end(bars[first.1], 2).unwrap();
        b.add_volta(bars[second.0], bars[second.1], passes(2, 2), false, None)
            .unwrap();
        b.set_bar_numbering(lettered);
        labels(b, &bars[6..])
    };
    assert_eq!(build((6, 7), (8, 9), 11), ["7a", "8a", "7b", "8b", "9"]);
    assert_eq!(build((6, 7), (8, 8), 10), ["7a", "8a", "7b", "9"]);
    assert_eq!(build((7, 7), (8, 9), 11), ["7", "8a", "8b", "9b", "10"]);
}

/// Two volta groups: the shift of the first is kept after the second ("same as first" styles).
#[test]
fn bar_numbers_add_up_over_groups() {
    let (mut b, bars) = builder(10);
    for (one, two) in [(2, 3), (6, 7)] {
        b.add_volta(bars[one], bars[one], passes(1, 1), true, None)
            .unwrap();
        b.add_repeat_end(bars[one], 2).unwrap();
        b.add_volta(bars[two], bars[two], passes(2, 2), false, None)
            .unwrap();
        if one == 6 {
            continue;
        }
        b.add_repeat_start(bars[4]).unwrap();
    }
    b.set_bar_numbering(BarNumbering::SameAsFirst);
    assert_eq!(
        labels(b, &bars),
        ["1", "2", "3", "3", "4", "5", "6", "6", "7", "8"]
    );
}

/// The numbering rules of ADR-0018 still apply: an uncounted bar inside a volta has no number and
/// is not counted; a restart inside a group wins over the style.
#[test]
fn bar_numbers_with_uncounted_bars_and_restarts() {
    // The second volta has an uncounted first bar: 7a 8a | – 7b | 9 (after the longest volta).
    let (mut b, bars) = builder_with(11, |m| {
        m.set_uncounted(BarIndex::new(8), true);
    });
    b.add_volta(bars[6], bars[7], passes(1, 1), true, None)
        .unwrap();
    b.add_repeat_end(bars[7], 2).unwrap();
    b.add_volta(bars[8], bars[9], passes(2, 2), false, None)
        .unwrap();
    b.set_bar_numbering(BarNumbering::SameAsFirstLettered);
    assert_eq!(labels(b, &bars[6..]), ["7a", "8a", "–", "7b", "9"]);

    // A restart at the second volta: the group is numbered as written from there.
    let (mut b, bars) = builder_with(11, |m| {
        m.set_numbering_restart(BarIndex::new(8), Some(20));
    });
    b.add_volta(bars[7], bars[7], passes(1, 1), true, None)
        .unwrap();
    b.add_repeat_end(bars[7], 2).unwrap();
    b.add_volta(bars[8], bars[8], passes(2, 2), false, None)
        .unwrap();
    b.set_bar_numbering(BarNumbering::SameAsFirstLettered);
    assert_eq!(labels(b, &bars[6..]), ["7", "8a", "20", "21", "22"]);
}

/// The label type: letters from "a", none past the 26th volta.
#[test]
fn bar_label_letters() {
    let label = |volta| BarLabel { number: 8, volta };
    assert_eq!(label(Some(1)).letter(), Some('a'));
    assert_eq!(label(Some(26)).letter(), Some('z'));
    assert_eq!(label(Some(27)).letter(), None);
    assert_eq!(label(None).letter(), None);
}
