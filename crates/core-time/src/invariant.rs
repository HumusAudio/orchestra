// SPDX-License-Identifier: GPL-3.0-or-later

//! Conversions that **cannot fail** while the program is correct, made loud in debug builds.
//!
//! # The problem
//! Library code never panics (CLAUDE.md §4), so a conversion such as `u32::try_from(index)` whose
//! failure would mean a bug still needs a value for the impossible case. Writing
//! `.unwrap_or(0)` gives one, but silently: if the invariant ever breaks, a plausible but wrong
//! value (an octave 0, a tick 0) flows on and nobody notices.
//!
//! # The rule
//! [`convert_or`] keeps the fallback for release builds (no panic for the user) and adds a
//! `debug_assert!`, so every debug build, and therefore every test run, stops at the very
//! conversion that broke, with the value and the caller's location (`#[track_caller]`).
//!
//! # When NOT to use it
//! Only where a failure is a **bug**. Where it is a legitimate input (an absurd hand-built
//! interval that saturates on purpose, a capacity hint, an index turned into a lookup that fails
//! with `None` on its own), keep the plain `unwrap_or`, with a comment saying so.

use core::fmt::Debug;

/// Converts `value` with `TryFrom`; if that fails, which must never happen while the program is
/// correct, returns `fallback` in release builds and fails a `debug_assert!` in debug builds.
///
/// `#[track_caller]` makes the assertion message point at the caller's line, not at this
/// function, so a failing test names the broken conversion directly.
///
/// # Complexity
/// O(1) plus the cost of the conversion; the assertion is compiled out of release builds.
#[track_caller]
#[inline]
pub fn convert_or<T, U>(value: T, fallback: U) -> U
where
    T: Copy + Debug,
    U: TryFrom<T>,
{
    let converted = U::try_from(value);
    debug_assert!(
        converted.is_ok(),
        "a conversion that cannot fail did: {value:?} (an invariant is broken)"
    );
    converted.unwrap_or(fallback)
}

/// Returns `value` when `holds` is true; otherwise, which must never happen while the program is
/// correct, returns `fallback` in release builds and fails a `debug_assert!` in debug builds. The
/// same rule as [`convert_or`] for an `Option` produced by a constructor that may refuse its input
/// (for example an alteration outside the supported range).
///
/// # Complexity
/// O(1).
#[track_caller]
#[inline]
pub fn some_or<T>(value: Option<T>, fallback: T) -> T {
    debug_assert!(
        value.is_some(),
        "a value that is always present was missing (an invariant is broken)"
    );
    value.unwrap_or(fallback)
}

#[cfg(test)]
mod tests {
    //! The fallback is returned only in release builds; in debug builds a failure panics.

    use super::*;

    /// A successful conversion returns the converted value.
    #[test]
    fn a_valid_conversion_converts() {
        assert_eq!(convert_or(300_i64, 0_u16), 300);
        assert_eq!(some_or(Some(3), 0), 3);
    }

    /// A failing conversion is caught in debug builds (the tests run in debug).
    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "an invariant is broken")]
    fn a_failing_conversion_is_caught() {
        let _ = convert_or(-1_i64, 0_u16);
    }

    /// A missing value is caught in debug builds.
    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "an invariant is broken")]
    fn a_missing_value_is_caught() {
        let _ = some_or(None, 0_u8);
    }
}
