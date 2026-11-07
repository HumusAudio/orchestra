// SPDX-License-Identifier: GPL-3.0-or-later

//! The fingerprint of a document, shared by the fingerprint tests of the two presets
//! (`plain_fingerprint.rs`, `full_fingerprint.rs`; ADR-0031 §7).
//!
//! # What is compared
//! The bytes of the project file (`orchestra_io::to_bytes`): the saved form is the most complete
//! description of a document we have (every object, every ID, every ID counter), and it is
//! deterministic by construction (ADR-0029: no maps with random iteration order, LF line ends,
//! fixed field order). The check is the length plus a 64-bit FNV-1a hash of the bytes: a hash
//! keeps the expected values small enough to live in the test files (a heavy file is 12–25 MB),
//! and FNV-1a is a few lines, needs no dependency and is the same on every platform.

// Helper functions (not `#[test]` functions) are outside the scope of clippy.toml's
// `allow-unwrap-in-tests`, so the exemption is stated here, for this module only.
#![allow(
    clippy::unwrap_used,
    reason = "test helpers: a failed unwrap fails the test"
)]

use orchestra_core_model::Document;

/// The FNV-1a 64-bit hash of `bytes`.
///
/// # Why FNV-1a?
/// A fingerprint, not a security measure: we only need two different byte strings to give
/// different values with overwhelming probability, deterministically on every platform. FNV-1a is
/// the shortest well-known function with that property; `std`'s `DefaultHasher` is explicitly not
/// guaranteed to stay the same between Rust releases, so it would make the tests fail on a
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

/// Asserts that `doc` (described as `name` in the failure message) saves to exactly `len` bytes
/// whose FNV-1a hash is `hash`.
pub(crate) fn assert_fingerprint(name: &str, doc: &Document, len: usize, hash: u64) {
    let bytes = orchestra_io::to_bytes(doc).unwrap();
    // The length first: when it differs, it says at once that something was added or removed,
    // which is easier to read than two unrelated hashes.
    assert_eq!(
        bytes.len(),
        len,
        "{name} changed size: a change to a preset must be deliberate (ADR-0031 §7)"
    );
    assert_eq!(
        fnv1a(&bytes),
        hash,
        "{name} changed content: a change to a preset must be deliberate (ADR-0031 §7)"
    );
}
