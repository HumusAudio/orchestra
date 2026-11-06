// SPDX-License-Identifier: GPL-3.0-or-later

//! The canonical text form of [`Rational`] (ADR-0029 S6): `Display` writes it, `FromStr` reads it
//! back and refuses everything else.
//!
//! # What is tested
//! - hand-written cases for every accepted shape and every error variant;
//! - a round trip `x.to_string().parse() == Ok(x)` on random values, with the extremes;
//! - an **oracle** on random texts built from the grammar's pieces (signs, digits with and without
//!   leading zeros, slashes, stray characters): the oracle classifies each text with its own code,
//!   in `i128` and by Euclid's gcd, sharing nothing with the library's reader, and the two must
//!   agree on the value or on the error.

// Helper functions are outside the scope of clippy.toml's `allow-unwrap-in-tests`.
#![allow(
    clippy::unwrap_used,
    reason = "test helpers: a failed unwrap fails the test"
)]

use orchestra_core_time::{ParseRationalError, Rational};
use proptest::prelude::*;

/// Shorthand: builds `n/d`, which every caller here knows to be valid.
fn r(n: i64, d: i64) -> Rational {
    Rational::new(n, d).unwrap()
}

/// Shorthand: parses a text.
fn parse(text: &str) -> Result<Rational, ParseRationalError> {
    text.parse()
}

/// `Display` writes the canonical form: integers without a denominator, the sign on the
/// numerator.
#[test]
fn display_is_canonical() {
    assert_eq!(r(3, 16).to_string(), "3/16");
    assert_eq!(r(-1, 8).to_string(), "-1/8");
    assert_eq!(r(6, 8).to_string(), "3/4");
    assert_eq!(r(1, -2).to_string(), "-1/2");
    assert_eq!(Rational::ZERO.to_string(), "0");
    assert_eq!(r(4, 2).to_string(), "2");
    assert_eq!(r(-5, 1).to_string(), "-5");
    assert_eq!(
        r(i64::MAX, 1).to_string(),
        "9223372036854775807",
        "the largest numerator"
    );
    assert_eq!(
        r(-1, i64::MAX).to_string(),
        "-1/9223372036854775807",
        "the largest denominator"
    );
}

/// Every canonical shape is read back.
#[test]
fn canonical_texts_are_read() {
    assert_eq!(parse("3/16"), Ok(r(3, 16)));
    assert_eq!(parse("-1/8"), Ok(r(-1, 8)));
    assert_eq!(parse("0"), Ok(Rational::ZERO));
    assert_eq!(parse("2"), Ok(r(2, 1)));
    assert_eq!(parse("-5"), Ok(r(-5, 1)));
    assert_eq!(parse("9223372036854775807"), Ok(r(i64::MAX, 1)));
    assert_eq!(parse("-9223372036854775807"), Ok(r(-i64::MAX, 1)));
    assert_eq!(parse("1/9223372036854775807"), Ok(r(1, i64::MAX)));
}

/// Each error variant, with the texts that produce it.
#[test]
fn every_error_variant() {
    assert_eq!(parse(""), Err(ParseRationalError::Empty));
    for text in [
        " 1/2", "1/2 ", "1 /2", "+1/2", "1.5", "1/2/3", "/2", "1/", "-", "--1", "1/-2", "a", "1e3",
        "٣", "½", "-/2",
    ] {
        assert_eq!(parse(text), Err(ParseRationalError::Malformed), "{text:?}");
    }
    for text in [
        "2/4", "0/5", "3/1", "-0", "-0/5", "03/8", "3/08", "00", "-01",
    ] {
        assert_eq!(
            parse(text),
            Err(ParseRationalError::NotCanonical),
            "{text:?}"
        );
    }
    assert_eq!(parse("3/0"), Err(ParseRationalError::DivisionByZero));
    assert_eq!(parse("0/0"), Err(ParseRationalError::DivisionByZero));
    for text in [
        "9223372036854775808",
        "-9223372036854775808",
        "1/9223372036854775808",
        "99999999999999999999999",
    ] {
        assert_eq!(parse(text), Err(ParseRationalError::Overflow), "{text:?}");
    }
}

// ------------------------------------------------------------------------------------------------
// The oracle
// ------------------------------------------------------------------------------------------------

/// Greatest common divisor by Euclid's algorithm (the library uses Stein's).
fn euclid(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        let t = a % b;
        a = b;
        b = t;
    }
    a
}

/// Reads one part of digits for the oracle, in `u128` so that overflow is a plain comparison:
/// `Err(Malformed)` for an empty part or a non-digit; the value otherwise (possibly beyond
/// `i64::MAX`), and whether it has a leading zero.
fn oracle_part(text: &str) -> Result<(u128, bool), ParseRationalError> {
    if text.is_empty() || !text.chars().all(|c| c.is_ascii_digit()) {
        return Err(ParseRationalError::Malformed);
    }
    // Saturate instead of overflowing u128 for absurdly long parts: anything that large is an
    // overflow anyway.
    let value = text.bytes().fold(0_u128, |v, b| {
        v.saturating_mul(10).saturating_add(u128::from(b - b'0'))
    });
    Ok((value, text.len() > 1 && text.starts_with('0')))
}

/// What the reader must return for `text`, decided independently of the library. The order of
/// the checks follows the precedence the library documents: shape first (Malformed), then the
/// sizes (Overflow), then the zero denominator, then the canonical form.
fn oracle(text: &str) -> Result<(i64, i64), ParseRationalError> {
    if text.is_empty() {
        return Err(ParseRationalError::Empty);
    }
    let (negative, rest) = text.strip_prefix('-').map_or((false, text), |r| (true, r));
    let mut pieces = rest.splitn(2, '/');
    let num_text = pieces.next().unwrap_or("");
    let den_text = pieces.next();
    let (num, num_zero_lead) = oracle_part(num_text)?;
    let (den, den_zero_lead) = match den_text {
        Some(d) => oracle_part(d)?,
        None => (1, false),
    };
    let max = u128::try_from(i64::MAX).unwrap();
    if num > max || den > max {
        return Err(ParseRationalError::Overflow);
    }
    if den == 0 {
        return Err(ParseRationalError::DivisionByZero);
    }
    let written_one = den_text.is_some() && den == 1;
    let reduced = euclid(num, den) == 1;
    if num_zero_lead || den_zero_lead || written_one || (negative && num == 0) || !reduced {
        return Err(ParseRationalError::NotCanonical);
    }
    let n = i64::try_from(num).unwrap();
    Ok((if negative { -n } else { n }, i64::try_from(den).unwrap()))
}

/// The pieces texts are built from: mostly grammar, some leading zeros, some noise.
fn piece() -> impl Strategy<Value = String> {
    prop_oneof![
        4 => (1_u64..1000).prop_map(|n| n.to_string()),
        1 => Just("0".to_string()),
        1 => (1_u64..100).prop_map(|n| format!("0{n}")),
        1 => any::<u64>().prop_map(|n| n.to_string()),
        1 => Just("9223372036854775808".to_string()),
        1 => Just("9223372036854775807".to_string()),
        1 => prop_oneof![
            Just(String::new()),
            Just(" ".to_string()),
            Just("+".to_string()),
            Just(".".to_string()),
            Just("x".to_string()),
        ],
    ]
}

/// A random text: optional sign, a numerator piece, optionally a slash and a denominator piece,
/// occasionally a second slash.
fn text() -> impl Strategy<Value = String> {
    (
        any::<bool>(),
        piece(),
        proptest::option::of(piece()),
        0_u8..20,
    )
        .prop_map(|(neg, n, d, extra)| {
            let mut t = String::new();
            if neg {
                t.push('-');
            }
            t.push_str(&n);
            if let Some(d) = d {
                t.push('/');
                t.push_str(&d);
            }
            if extra == 0 {
                t.push_str("/2");
            }
            t
        })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(20_000))]

    /// Writing then reading gives the same value, for any valid rational.
    #[test]
    fn round_trip(n in any::<i64>().prop_filter("not i64::MIN", |&n| n != i64::MIN),
                  d in 1_i64..=i64::MAX) {
        let x = r(n, d);
        prop_assert_eq!(x.to_string().parse::<Rational>(), Ok(x));
    }

    /// The reader agrees with the independent oracle on every generated text.
    #[test]
    fn reader_matches_oracle(t in text()) {
        let got = parse(&t).map(|x| (x.numerator(), x.denominator()));
        prop_assert_eq!(got, oracle(&t), "text {:?}", t);
    }
}
