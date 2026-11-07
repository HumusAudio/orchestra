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
//! The length and the FNV-1a hash of the saved bytes of each of the four plain documents (two
//! sizes, with and without the editing instrument): see `common/mod.rs`.
//!
//! # Where the values come from
//! They were computed on 2026-11-07 twice: with this crate, and with the generator as it was on
//! `main` before the move (`crates/core-model/tests/common/score.rs`, commit 02d7ec8), giving the
//! same four results. That is the proof that the move of SCALETTA F1.16 changed nothing.
//!
//! # Cost
//! Linear in the size of the documents: the heavy ones take most of the time (about 7 seconds in
//! a debug build on the development machine, generation and saving together).

mod common;

use orchestra_test_scores::{Size, plain};

/// Asserts the fingerprint of the plain score of `size` (with the editing instrument if
/// `editing`).
fn assert_fingerprint(size: Size, editing: bool, len: usize, hash: u64) {
    let name = format!("plain({size:?}, {editing})");
    common::assert_fingerprint(&name, &plain(size, editing), len, hash);
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
