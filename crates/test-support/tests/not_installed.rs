// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of the protection against vacuous passes: this binary deliberately does **not**
//! register [`orchestra_test_support::CountingAllocator`], so every count would be zero.
//!
//! Without the protection, `assert_no_allocations` would happily "prove" that allocating code is
//! allocation-free. These tests check that the helpers refuse to measure instead.

use orchestra_test_support::{
    NotInstalledError, assert_no_allocations, count_allocations, is_installed,
};

/// The probe reports that the counting allocator is absent.
#[test]
fn absent_allocator_is_detected() {
    assert!(!is_installed());
}

/// `count_allocations` returns the typed error and does not run the closure.
#[test]
fn count_allocations_refuses_to_measure() {
    let mut ran = false;
    let outcome = count_allocations(|| ran = true);
    assert_eq!(outcome, Err(NotInstalledError));
    assert!(
        !ran,
        "the closure must not run when nothing can be measured"
    );
}

/// `assert_no_allocations` fails loudly instead of passing vacuously.
#[test]
#[should_panic(expected = "CountingAllocator is not the global allocator")]
fn assert_no_allocations_fails_without_allocator() {
    assert_no_allocations(|| drop(Box::new(1_u64)));
}
