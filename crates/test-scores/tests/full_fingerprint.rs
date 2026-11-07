// SPDX-License-Identifier: GPL-3.0-or-later

//! The full preset never changes by accident (ADR-0031 §7).
//!
//! # Why this test exists
//! The phase benchmarks (SCALETTA F1.16c) will measure the full scores, and the regression gate
//! (ADR-0017) compares their instruction counts with the merge base: if the generator changed what
//! it builds, the gate would compare two different inputs. The exact output is therefore pinned
//! here, like the plain preset's; a change must be deliberate, in its own commit, with the two
//! constants below updated and the reason stated.
//!
//! # What is compared
//! The length and the FNV-1a hash of the saved bytes of the two full documents: see
//! `common/mod.rs`.
//!
//! # Where the values come from
//! Computed on 2026-11-07 from the first version of the preset (SCALETTA F1.16b), twice, with the
//! same results: 3 939 noteheads in the normal score, 77 225 in the heavy one (the density the
//! owner chose that day, option A).
//!
//! # Cost
//! Dominated by the heavy score: generation and saving take a few seconds in a debug build.

mod common;

use orchestra_test_scores::{Size, full};

/// The normal full score.
#[test]
fn the_normal_full_score_is_unchanged() {
    common::assert_fingerprint(
        "full(Normal)",
        &full(Size::Normal),
        509_739,
        0x94a0_473a_ae4d_a774,
    );
}

/// The heavy full score.
#[test]
fn the_heavy_full_score_is_unchanged() {
    common::assert_fingerprint(
        "full(Heavy)",
        &full(Size::Heavy),
        11_997_030,
        0xdd40_1214_e95e_fdc3,
    );
}
