// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of [`Seconds`], [`SampleRate`], [`FrameRate`] and [`Tempo`] (ADR-0020 §2, §5, §7).
//!
//! # What is checked
//! - the rounding rules **by their definition**, with proptest: `sample_at(t) = n` exactly when
//!   `(n − 1/2)/rate ≤ t < (n + 1/2)/rate`, and `frame_at(t) = k` exactly when
//!   `k/fps ≤ t < (k + 1)/fps`; and that the inverses land on their own index;
//! - constant-tempo conversions in both directions, and the equivalence of tempi written with
//!   different units;
//! - hand-written cases: ties, negative times, NTSC frames, errors, `Debug`.

// Helper functions (not `#[test]` functions) are outside the scope of clippy.toml's
// `allow-unwrap-in-tests`, so the exemption is stated here, for this test file only: a helper that
// unwraps fails the calling test with a clear message, which is exactly what a test wants.
#![allow(
    clippy::unwrap_used,
    reason = "test helpers: a failed unwrap fails the test"
)]

use orchestra_core_time::{
    ClockError, Duration, FrameRate, Rational, SampleRate, Seconds, Tempo, TempoError,
};
use proptest::prelude::*;

/// Shorthand: the rational `n/d`.
fn r(n: i64, d: i64) -> Rational {
    Rational::new(n, d).unwrap()
}

/// Shorthand: `n/d` seconds.
fn s(n: i64, d: i64) -> Seconds {
    Seconds::from_rational(r(n, d))
}

/// Shorthand: a tempo of `bpm` beats of `unit_n/unit_d` per minute.
fn tempo(unit_n: i64, unit_d: i64, bpm: Rational) -> Tempo {
    Tempo::new(Duration::from_rational(r(unit_n, unit_d)), bpm).unwrap()
}

/// The common sample rates.
fn sample_rate() -> impl Strategy<Value = SampleRate> {
    prop::sample::select(vec![
        8_000_u32, 22_050, 44_100, 48_000, 88_200, 96_000, 192_000,
    ])
    .prop_map(|hz| SampleRate::new(hz).unwrap())
}

/// The common frame rates, NTSC ones included.
fn frame_rate() -> impl Strategy<Value = FrameRate> {
    prop::sample::select(vec![
        (24, 1),
        (25, 1),
        (30, 1),
        (50, 1),
        (60, 1),
        (24_000, 1001),
        (30_000, 1001),
    ])
    .prop_map(|(n, d)| FrameRate::new(r(n, d)).unwrap())
}

/// A time within about ±3 hours, with musical and awkward denominators.
fn time() -> impl Strategy<Value = Seconds> {
    (
        -10_000_000_i64..=10_000_000,
        prop::sample::select(vec![1_i64, 3, 7, 1000, 1001, 96_000, 30_000]),
    )
        .prop_map(|(n, d)| s(n, d))
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1024))]

    /// The nearest-sample rule holds by its definition, ties to the later sample.
    #[test]
    fn sample_rounding_by_definition(rate in sample_rate(), t in time()) {
        let n = rate.sample_at(t).unwrap();
        let half = r(1, 2);
        let n_r = Rational::from_integer(n).unwrap();
        let hz = Rational::from_integer(i64::from(rate.hz())).unwrap();
        let low = n_r.checked_sub(half).unwrap().checked_div(hz).unwrap();
        let high = n_r.checked_add(half).unwrap().checked_div(hz).unwrap();
        prop_assert!(low <= t.as_rational() && t.as_rational() < high, "t = {:?}, n = {}", t, n);
    }

    /// The containing-frame rule holds by its definition.
    #[test]
    fn frame_rounding_by_definition(fps in frame_rate(), t in time()) {
        let k = fps.frame_at(t).unwrap();
        prop_assert!(fps.frame_start(k).unwrap() <= t);
        prop_assert!(t < fps.frame_start(k + 1).unwrap());
    }

    /// The inverses are exact: the time of sample n rounds to n, and frame k starts in frame k.
    #[test]
    fn inverses_land_on_their_index(rate in sample_rate(), fps in frame_rate(), n in -1_000_000_000_i64..=1_000_000_000) {
        prop_assert_eq!(rate.sample_at(rate.time_of_sample(n).unwrap()).unwrap(), n);
        prop_assert_eq!(fps.frame_at(fps.frame_start(n).unwrap()).unwrap(), n);
    }

    /// At a constant tempo, duration → seconds → duration is the identity, and ♩ = x equals
    /// ♩. = 2x/3 in speed.
    #[test]
    fn constant_tempo_round_trip(bpm_n in 1_i64..=600, bpm_d in 1_i64..=4, num in 0_i64..=100_000, den in 1_i64..=1680) {
        let t = tempo(1, 4, r(bpm_n, bpm_d));
        let duration = Duration::from_rational(r(num, den));
        let seconds = t.seconds_for(duration).unwrap();
        prop_assert_eq!(t.duration_in(seconds).unwrap(), duration);
        let dotted = tempo(3, 8, r(bpm_n * 2, bpm_d * 3));
        prop_assert_eq!(dotted.speed().unwrap(), t.speed().unwrap());
    }
}

/// Ties go to the later sample; negative times round the same way.
#[test]
fn sample_ties_and_negative_times() {
    let rate = SampleRate::new(48_000).unwrap();
    assert_eq!(rate.hz(), 48_000);
    // Exactly half a sample: the later one.
    assert_eq!(rate.sample_at(s(1, 96_000)).unwrap(), 1);
    // Just below half a sample: the earlier one.
    assert_eq!(rate.sample_at(s(1, 96_001)).unwrap(), 0);
    // −1/2 sample is a tie between −1 and 0: the later, 0.
    assert_eq!(rate.sample_at(s(-1, 96_000)).unwrap(), 0);
    assert_eq!(rate.sample_at(s(-1, 1)).unwrap(), -48_000);
    assert_eq!(rate.time_of_sample(24_000).unwrap(), s(1, 2));
    // One hour at 48 kHz.
    assert_eq!(rate.sample_at(s(3600, 1)).unwrap(), 172_800_000);
}

/// NTSC frames: 30000/1001 fps, so one second holds 29.97 frames.
#[test]
fn ntsc_frames() {
    let ntsc = FrameRate::new(r(30_000, 1001)).unwrap();
    assert_eq!(ntsc.fps(), r(30_000, 1001));
    assert_eq!(ntsc.frame_at(s(1, 1)).unwrap(), 29);
    assert_eq!(ntsc.frame_start(30).unwrap(), s(1001, 1000));
    // Just before frame 30 starts, still frame 29.
    assert_eq!(ntsc.frame_at(s(1001 * 30 - 1, 30_000)).unwrap(), 29);
    assert_eq!(ntsc.frame_at(s(1001, 1000)).unwrap(), 30);
    // One hour of NTSC: 107 892.107… frames, so the frame containing 3600 s is 107 892.
    assert_eq!(ntsc.frame_at(s(3600, 1)).unwrap(), 107_892);
    assert_eq!(ntsc.frame_at(s(-1, 1_000_000)).unwrap(), -1);
}

/// Constant-tempo conversions on musical values.
#[test]
fn constant_tempo_values() {
    let t = tempo(1, 4, Rational::from_integer(120).unwrap());
    assert_eq!(t.unit(), Duration::from_rational(r(1, 4)));
    assert_eq!(t.bpm(), Rational::from_integer(120).unwrap());
    assert_eq!(t.speed().unwrap(), r(1, 2));
    // A 4/4 bar at ♩ = 120 lasts 2 s; a triplet eighth 1/6 s.
    assert_eq!(
        t.seconds_for(Duration::from_rational(Rational::ONE))
            .unwrap(),
        s(2, 1)
    );
    assert_eq!(
        t.seconds_for(Duration::from_rational(r(1, 12))).unwrap(),
        s(1, 6)
    );
    // ♩ = 92.5: a quarter lasts 120/185 = 24/37 s.
    let t = tempo(1, 4, r(185, 2));
    assert_eq!(
        t.seconds_for(Duration::from_rational(r(1, 4))).unwrap(),
        s(24, 37)
    );
    assert_eq!(format!("{t:?}"), "Tempo(1/4 = 185/2)");
    assert_eq!(format!("{:?}", s(1, 3)), "Seconds(1/3)");
    assert_eq!(format!("{:?}", Seconds::ZERO), "Seconds(0)");
}

/// Invalid values are typed errors.
#[test]
fn errors() {
    assert_eq!(SampleRate::new(0), Err(ClockError::ZeroSampleRate));
    assert_eq!(
        FrameRate::new(Rational::ZERO),
        Err(ClockError::NonPositiveFrameRate)
    );
    assert_eq!(
        FrameRate::new(r(-25, 1)),
        Err(ClockError::NonPositiveFrameRate)
    );
    let quarter = Duration::from_rational(r(1, 4));
    assert_eq!(
        Tempo::new(Duration::ZERO, r(60, 1)),
        Err(TempoError::NonPositiveUnit)
    );
    assert_eq!(
        Tempo::new(quarter, Rational::ZERO),
        Err(TempoError::NonPositiveBpm)
    );
    assert_eq!(
        Tempo::new(quarter, r(-60, 1)),
        Err(TempoError::NonPositiveBpm)
    );
    assert_eq!(s(1, 2).checked_add(s(1, 3)).unwrap(), s(5, 6));
    assert_eq!(s(1, 2).checked_sub(s(1, 3)).unwrap(), s(1, 6));
}
