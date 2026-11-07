// SPDX-License-Identifier: GPL-3.0-or-later

//! The plain preset never changes by accident (ADR-0031 §6–§7).
//!
//! # Why this test exists
//! The benchmarks of `core-model`, `io` and `playback` measure the plain scores, and the regression
//! gate (ADR-0017) compares their instruction counts with the merge base. If the generator changed
//! what it builds, the gate would compare two different inputs and its verdict would mean nothing,
//! while the memory ceilings of `core-model`'s tests would silently refer to another score. So the
//! exact output is pinned here: a change to it must be deliberate, made in its own commit, with
//! the four constants below updated and the reason stated (ADR-0031 §7).
//!
//! # What is compared
//! The bytes of the project file (`orchestra_io::to_bytes`) of each of the four plain documents
//! (two sizes, with and without the editing instrument): the saved form is the most complete
//! description of a document we have (every object, every ID, every ID counter), and it is
//! deterministic by construction (ADR-0029: no maps with random iteration order, LF line ends,
//! fixed field order). The check is the length plus a 64-bit FNV-1a hash of the bytes: a hash
//! keeps the expected values small enough to live in this file (the heavy file is about 25 MB),
//! and FNV-1a is a few lines, needs no dependency and is the same on every platform.
//!
//! # Where the values come from
//! They were computed on 2026-11-07 twice: with this crate, and with the generator as it was on
//! `main` before the move (`crates/core-model/tests/common/score.rs`, commit 02d7ec8), giving the
//! same four results. That is the proof that the move of SCALETTA F1.16 changed nothing.
//!
//! # Cost
//! Linear in the size of the documents: the heavy ones take most of the time (about 7 seconds in
//! a debug build on the development machine, generation and saving together).

// Helper functions (not `#[test]` functions) are outside the scope of clippy.toml's
// `allow-unwrap-in-tests`, so the exemption is stated here, for this test file only.
#![allow(
    clippy::unwrap_used,
    reason = "test helpers: a failed unwrap fails the test"
)]

use orchestra_test_scores::{Size, plain};

/// The FNV-1a 64-bit hash of `bytes`.
///
/// # Why FNV-1a?
/// A fingerprint, not a security measure: we only need two different byte strings to give
/// different values with overwhelming probability, deterministically on every platform. FNV-1a is
/// the shortest well-known function with that property; `std`'s `DefaultHasher` is explicitly not
/// guaranteed to stay the same between Rust releases, so it would make this test fail on a
/// toolchain bump.
///
/// # Complexity
/// O(n) in the number of bytes, one xor and one multiplication each.
fn fnv1a(bytes: &[u8]) -> u64 {
    // The standard 64-bit offset basis and prime of FNV (Fowler–Noll–Vo).
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Asserts that the plain score of `size` (with the editing instrument if `editing`) saves to
/// exactly `len` bytes whose FNV-1a hash is `hash`.
fn assert_fingerprint(size: Size, editing: bool, len: usize, hash: u64) {
    let bytes = orchestra_io::to_bytes(&plain(size, editing)).unwrap();
    // The length first: when it differs, it says at once that something was added or removed,
    // which is easier to read than two unrelated hashes.
    assert_eq!(
        bytes.len(),
        len,
        "plain({size:?}, {editing}) changed size: a change to the plain preset must be deliberate \
         (ADR-0031 §7)"
    );
    assert_eq!(
        fnv1a(&bytes),
        hash,
        "plain({size:?}, {editing}) changed content: a change to the plain preset must be \
         deliberate (ADR-0031 §7)"
    );
}

/// The normal score, without and with the editing instrument.
#[test]
fn the_normal_plain_score_is_unchanged() {
    assert_fingerprint(Size::Normal, false, 836_643, 0xfd09_bbc1_911d_e245);
    assert_fingerprint(Size::Normal, true, 836_998, 0x5969_b934_d135_b9d9);
}

/// The heavy score, without and with the editing instrument.
#[test]
fn the_heavy_plain_score_is_unchanged() {
    assert_fingerprint(Size::Heavy, false, 25_361_550, 0xf9ce_11b2_dd09_4128);
    assert_fingerprint(Size::Heavy, true, 25_361_910, 0x6175_3e54_a2cb_4dc0);
}

/// The generator is deterministic within one run too: two calls give equal documents. (Equality
/// of documents is the model's own `PartialEq`, IDs included.)
#[test]
fn two_calls_give_the_same_document() {
    assert_eq!(plain(Size::Normal, true), plain(Size::Normal, true));
}
