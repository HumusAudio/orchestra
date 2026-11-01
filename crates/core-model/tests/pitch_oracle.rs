// SPDX-License-Identifier: GPL-3.0-or-later

//! Property tests of the pitch module (ADR-0023) against an **independent oracle**.
//!
//! # The oracle
//! It shares no code and no formula with the library:
//! - the semitones of a natural note come from **walking the major scale** (2 2 1 2 2 2 1) from C,
//!   not from a table;
//! - the line-of-fifths place of a spelling comes from the **order of sharps** written out as a
//!   list (F C G D A E B), not from a formula;
//! - the alterations of a key come from **applying sharps or flats one at a time** in the order
//!   of sharps (or its reverse), as a musician reads a signature;
//! - the quality of an interval comes from a **textbook table** of names by number and size for
//!   simple intervals, up to doubly augmented and doubly diminished.
//! - the tonic of a mode is the **degree of the major scale** of the key that the mode names
//!   (Ionian 1st … Locrian 7th), the major tonic being as many fifths above C as the key has
//!   sharps.
//!
//! # The properties
//! Sounding pitch, MIDI key and bend, transposition (forwards, backwards, and its agreement with
//! `interval_to`), quality round trips, key alterations (exhaustively), spelling of every MIDI key
//! in every key (exhaustively), and the agreement between transposing a key and transposing its
//! tonic.

// Helper functions (not `#[test]` functions) are outside the scope of clippy.toml's
// `allow-unwrap-in-tests`, so the exemption is stated here, for this test file only.
#![allow(
    clippy::unwrap_used,
    reason = "test helpers: a failed unwrap fails the test"
)]

use orchestra_core_model::pitch::{
    Alteration, Interval, Key, Mode, Pitch, PitchError, Quality, Step, TraditionalKey,
};
use orchestra_core_time::Rational;
use proptest::prelude::*;

/// Shorthand: the rational `n/d`.
fn r(n: i64, d: i64) -> Rational {
    Rational::new(n, d).unwrap()
}

// ------------------------------------------------------------------------------------------------
// The oracle
// ------------------------------------------------------------------------------------------------

/// The steps in scale order, written out.
const SCALE: [Step; 7] = [
    Step::C,
    Step::D,
    Step::E,
    Step::F,
    Step::G,
    Step::A,
    Step::B,
];

/// The order of sharps, written out.
const ORDER_OF_SHARPS: [Step; 7] = [
    Step::F,
    Step::C,
    Step::G,
    Step::D,
    Step::A,
    Step::E,
    Step::B,
];

/// Semitones of the natural `step` above C, by walking the major scale from C.
fn oracle_natural(step: Step) -> i64 {
    let gaps = [2, 2, 1, 2, 2, 2, 1];
    // Add the gaps of the steps below `step`: the walk stops at `step` itself.
    SCALE
        .iter()
        .zip(gaps)
        .take_while(|(s, _)| **s != step)
        .map(|(_, gap)| gap)
        .sum()
}

/// Scale position of `step` (C = 0), by searching the written scale.
fn oracle_index(step: Step) -> i64 {
    i64::try_from(SCALE.iter().position(|s| *s == step).unwrap()).unwrap()
}

/// The exact sounding pitch in semitones, MIDI scale.
fn oracle_sounding(p: Pitch) -> Rational {
    let base = 12 * (i64::from(p.octave()) + 1) + oracle_natural(p.step());
    Rational::from_integer(base)
        .unwrap()
        .checked_add(p.alteration().semitones())
        .unwrap()
}

/// The line-of-fifths place of a chromatic spelling: the natural's place in the order of sharps,
/// minus one (F is −1), plus seven per sharp.
fn oracle_fifths(step: Step, sharps: i64) -> i64 {
    let natural = i64::try_from(ORDER_OF_SHARPS.iter().position(|s| *s == step).unwrap()).unwrap();
    natural - 1 + 7 * sharps
}

/// The alteration (in semitones) of `step` in a key of `fifths`, by applying the sharps (or flats)
/// one at a time in the order of sharps (or its reverse).
fn oracle_key_alteration(fifths: i64, step: Step) -> i64 {
    let mut alteration = 0;
    for i in 0..fifths.unsigned_abs() {
        let i = usize::try_from(i).unwrap() % 7;
        let s = if fifths > 0 {
            ORDER_OF_SHARPS[i]
        } else {
            ORDER_OF_SHARPS[6 - i]
        };
        if s == step {
            alteration += fifths.signum();
        }
    }
    alteration
}

/// The textbook name of a simple interval (number 1–7) of `size` semitones measured upwards, for
/// deviations up to two from the major or perfect size; `None` outside the table.
fn oracle_quality(number: i64, size: i64) -> Option<Quality> {
    use Quality::{Augmented as A, Diminished as D, Major as M, Minor as Mi, Perfect as P};
    // (number, [(size, quality)…]) written out from a theory textbook.
    let table: [(i64, &[(i64, Quality)]); 7] = [
        (1, &[(-2, D(2)), (-1, D(1)), (0, P), (1, A(1)), (2, A(2))]),
        (
            2,
            &[(-1, D(2)), (0, D(1)), (1, Mi), (2, M), (3, A(1)), (4, A(2))],
        ),
        (
            3,
            &[(1, D(2)), (2, D(1)), (3, Mi), (4, M), (5, A(1)), (6, A(2))],
        ),
        (4, &[(3, D(2)), (4, D(1)), (5, P), (6, A(1)), (7, A(2))]),
        (5, &[(5, D(2)), (6, D(1)), (7, P), (8, A(1)), (9, A(2))]),
        (
            6,
            &[
                (6, D(2)),
                (7, D(1)),
                (8, Mi),
                (9, M),
                (10, A(1)),
                (11, A(2)),
            ],
        ),
        (
            7,
            &[
                (8, D(2)),
                (9, D(1)),
                (10, Mi),
                (11, M),
                (12, A(1)),
                (13, A(2)),
            ],
        ),
    ];
    let (_, sizes) = table.iter().find(|(n, _)| *n == number)?;
    sizes.iter().find(|(s, _)| *s == size).map(|(_, q)| *q)
}

// ------------------------------------------------------------------------------------------------
// Generators
// ------------------------------------------------------------------------------------------------

/// Any step.
fn step() -> impl Strategy<Value = Step> {
    (0_i64..7).prop_map(Step::from_index)
}

/// Chromatic alterations from double flat to double sharp, or microtonal ones (halves, thirds,
/// quarters, sixths of a semitone) up to two semitones.
fn alteration() -> impl Strategy<Value = Alteration> {
    prop_oneof![
        3 => (-2_i64..=2).prop_map(|a| Alteration::chromatic(a).unwrap()),
        1 => (prop::sample::select(vec![2_i64, 3, 4, 6]), -12_i64..=12).prop_map(|(d, n)| {
            let a = r(n, d);
            Alteration::new(if a.abs() > r(2, 1) { Rational::ZERO } else { a }).unwrap()
        }),
    ]
}

/// A pitch over the musical range (octaves −1 to 9).
fn pitch() -> impl Strategy<Value = Pitch> {
    (step(), alteration(), -1_i8..=9).prop_map(|(s, a, o)| Pitch::new(s, a, o))
}

/// A chromatic pitch (for keys and spelling).
fn chromatic_pitch() -> impl Strategy<Value = Pitch> {
    (step(), -2_i64..=2, 0_i8..=8)
        .prop_map(|(s, a, o)| Pitch::new(s, Alteration::chromatic(a).unwrap(), o))
}

/// An interval named by a valid quality, number 1–15, either direction.
fn named_interval() -> impl Strategy<Value = (Quality, u32, bool)> {
    let quality = prop_oneof![
        Just(Quality::Perfect),
        Just(Quality::Major),
        Just(Quality::Minor),
        (1_u32..=2).prop_map(Quality::Augmented),
        (1_u32..=2).prop_map(Quality::Diminished),
    ];
    (quality, 1_u32..=15, any::<bool>()).prop_filter("quality valid for the number", |(q, n, _)| {
        let perfect = matches!((n - 1) % 7, 0 | 3 | 4);
        match q {
            Quality::Perfect => perfect,
            Quality::Major | Quality::Minor => !perfect,
            _ => true,
        }
    })
}

/// The seven modes and "none".
fn mode() -> impl Strategy<Value = Mode> {
    prop::sample::select(vec![
        Mode::Major,
        Mode::Dorian,
        Mode::Phrygian,
        Mode::Lydian,
        Mode::Mixolydian,
        Mode::Minor,
        Mode::Locrian,
        Mode::None,
    ])
}

// ------------------------------------------------------------------------------------------------
// Properties
// ------------------------------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(4096))]

    /// The sounding pitch and the diatonic index agree with the oracle.
    #[test]
    fn sounding_and_index(p in pitch()) {
        prop_assert_eq!(p.sounding().unwrap(), oracle_sounding(p));
        prop_assert_eq!(p.diatonic_index(), 7 * i64::from(p.octave()) + oracle_index(p.step()));
    }

    /// The MIDI key is the nearest one (the lower on a tie) and the bend reaches the exact pitch.
    #[test]
    fn midi_key_and_bend(p in pitch()) {
        let exact = oracle_sounding(p);
        let floor = exact.floor();
        // Nearest key, lower on a tie: round up only if strictly above half way.
        let frac = exact.checked_sub(Rational::from_integer(floor).unwrap()).unwrap();
        let key = if frac > r(1, 2) { floor + 1 } else { floor };
        match p.midi() {
            Ok(m) => {
                prop_assert_eq!(i64::from(m.key), key);
                prop_assert_eq!(Rational::from_integer(key).unwrap().checked_add(m.bend).unwrap(), exact);
                prop_assert!(m.bend > r(-1, 2) && m.bend <= r(1, 2));
            }
            Err(e) => {
                prop_assert_eq!(e, PitchError::MidiOutOfRange);
                prop_assert!(!(0..=127).contains(&key));
            }
        }
    }

    /// Transposing moves the index by the steps and the sound by the semitones; transposing back
    /// returns the same spelling; `interval_to` gives back the interval.
    #[test]
    fn transposition_round_trip(p in pitch(), (q, n, down) in named_interval()) {
        let i = Interval::from_quality(q, n, down).unwrap();
        match p.transposed(i) {
            Ok(t) => {
                prop_assert_eq!(t.diatonic_index(), p.diatonic_index() + i64::from(i.steps()));
                prop_assert_eq!(
                    oracle_sounding(t),
                    oracle_sounding(p).checked_add(i.semitones()).unwrap()
                );
                prop_assert_eq!(p.interval_to(t).unwrap(), i);
                prop_assert_eq!(t.transposed(i.reversed()).unwrap(), p);
            }
            Err(e) => {
                // Only a too-large alteration can fail in the musical range.
                prop_assert_eq!(e, PitchError::AlterationOutOfRange);
                let target = oracle_sounding(p).checked_add(i.semitones()).unwrap();
                let index = p.diatonic_index() + i64::from(i.steps());
                let natural = 12 * (index.div_euclid(7) + 1)
                    + oracle_natural(Step::from_index(index));
                let needed = target.checked_sub(Rational::from_integer(natural).unwrap()).unwrap();
                prop_assert!(needed.abs() > r(3, 1));
            }
        }
    }

    /// Naming an interval and reading its quality back gives the same name and number.
    #[test]
    fn quality_round_trip((q, n, down) in named_interval()) {
        let i = Interval::from_quality(q, n, down).unwrap();
        prop_assert_eq!(i.quality().unwrap(), Some(q));
        prop_assert_eq!(i.number(), n);
        prop_assert_eq!(i.is_descending(), down && n > 1);
    }

    /// The quality of the interval between two chromatic pitches agrees with the textbook table
    /// (for simple ascending intervals inside the table).
    #[test]
    fn quality_against_table(a in chromatic_pitch(), b in chromatic_pitch()) {
        let i = a.interval_to(b).unwrap();
        let steps = i64::from(i.steps());
        let size = i.semitones().floor();
        if (0..7).contains(&steps)
            && let Some(expected) = oracle_quality(steps + 1, size)
        {
            prop_assert_eq!(i.quality().unwrap(), Some(expected));
        }
    }

    /// Transposing a key agrees with transposing its tonic: the new tonic is the old one moved by
    /// the interval (whenever both stay in range).
    #[test]
    fn key_and_tonic_transposition(fifths in -12_i8..=12, mode in mode(), (q, n, down) in named_interval()) {
        let k = TraditionalKey::new(fifths, mode).unwrap();
        let i = Interval::from_quality(q, n, down).unwrap();
        let (Some((step, alteration)), Ok(moved)) = (k.tonic(), k.transposed(i)) else {
            return Ok(());
        };
        let tonic = Pitch::new(step, alteration, 4);
        if let Ok(t) = tonic.transposed(i) {
            prop_assert_eq!(moved.tonic(), Some((t.step(), t.alteration())));
        }
    }
}

/// Every step in every key from −14 to +14 fifths, against the oracle that applies the signature
/// one accidental at a time.
#[test]
fn key_alterations_exhaustive() {
    for fifths in -14_i8..=14 {
        let key = Key::traditional(fifths, Mode::Major).unwrap();
        for step in SCALE {
            let expected = oracle_key_alteration(i64::from(fifths), step);
            assert_eq!(
                key.alteration_of(step).semitones(),
                Rational::from_integer(expected).unwrap(),
                "{fifths} fifths, {step:?}"
            );
        }
    }
}

/// Every MIDI key in every key and mode: the spelling sounds exactly the key, is chromatic, and its
/// line-of-fifths place lies in the window of ADR-0023 §6.
#[test]
fn spelling_exhaustive() {
    let modes = [
        Mode::Major,
        Mode::Dorian,
        Mode::Phrygian,
        Mode::Lydian,
        Mode::Mixolydian,
        Mode::Minor,
        Mode::Locrian,
        Mode::None,
    ];
    for fifths in -14_i8..=14 {
        for mode in modes {
            let key = Key::traditional(fifths, mode).unwrap();
            let start = match mode {
                Mode::Minor => i64::from(fifths) - 3,
                Mode::None => -4,
                _ => i64::from(fifths) - 4,
            };
            for midi in 0_u8..=127 {
                let p = Pitch::spell_midi(midi, key);
                assert_eq!(
                    oracle_sounding(p),
                    Rational::from_integer(i64::from(midi)).unwrap(),
                    "{midi} in {fifths} {mode:?}: {p:?}"
                );
                assert!(p.alteration().is_chromatic());
                let place = oracle_fifths(p.step(), p.alteration().semitones().floor());
                assert!(
                    (start..start + 12).contains(&place),
                    "{midi} in {fifths} {mode:?}: place {place}"
                );
            }
        }
    }
}

/// The tonic of every mode in every key, against the oracle: the major tonic of `k` fifths is `k`
/// perfect fifths above C, each fifth four steps up the scale; a mode's tonic is the degree of that
/// major scale named by the mode (Ionian 1st … Locrian 7th), with the alteration the signature
/// gives it (applied one accidental at a time).
#[test]
fn tonics_exhaustive() {
    let degrees = [
        (Mode::Major, 0),
        (Mode::Dorian, 1),
        (Mode::Phrygian, 2),
        (Mode::Lydian, 3),
        (Mode::Mixolydian, 4),
        (Mode::Minor, 5),
        (Mode::Locrian, 6),
    ];
    for fifths in -14_i8..=14 {
        for (mode, degree) in degrees {
            let step =
                SCALE[usize::try_from((4 * i64::from(fifths) + degree).rem_euclid(7)).unwrap()];
            let alteration = oracle_key_alteration(i64::from(fifths), step);
            let key = TraditionalKey::new(fifths, mode).unwrap();
            assert_eq!(
                key.tonic(),
                Some((step, Alteration::chromatic(alteration).unwrap())),
                "{fifths} fifths, {mode:?}"
            );
        }
        assert_eq!(
            TraditionalKey::new(fifths, Mode::None).unwrap().tonic(),
            None
        );
    }
}
