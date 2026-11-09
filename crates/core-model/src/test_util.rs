// SPDX-License-Identifier: GPL-3.0-or-later

//! Shorthands shared by the unit tests of the crate (`*_tests.rs`; third codebase review, A6:
//! six of them had the same copy). Compiled only for tests (`#[cfg(test)]` in `lib.rs`).

use orchestra_core_time::{Duration, Rational};

/// The duration `n/den` of a whole note. Panics on an invalid fraction: in a test that is the
/// clearest failure.
pub(crate) fn d(n: i64, den: i64) -> Duration {
    Duration::from_rational(Rational::new(n, den).unwrap())
}
