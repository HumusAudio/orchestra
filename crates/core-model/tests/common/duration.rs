// SPDX-License-Identifier: GPL-3.0-or-later

//! The shorthand for durations shared by the integration tests and the benchmarks of
//! `orchestra-core-model` (third codebase review, A6: fourteen files had the same copy).
//!
//! Included with `#[path = "common/duration.rs"] mod duration;` (from `benches/`:
//! `#[path = "../tests/common/duration.rs"]`): every test and benchmark target is its own crate,
//! so a shared file is the way to share a helper without a library of test code. Only the helper
//! that was **identical** everywhere lives here; helpers with the same name but another meaning
//! in one file (`tempo_mark_cases.rs`, `voice_cases.rs`) stay in that file.

use orchestra_core_time::{Duration, Rational};

/// The duration `n/den` of a whole note. Panics on an invalid fraction: in a test that is the
/// clearest failure.
pub(crate) fn d(n: i64, den: i64) -> Duration {
    Duration::from_rational(Rational::new(n, den).unwrap())
}
