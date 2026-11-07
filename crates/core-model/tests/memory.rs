// SPDX-License-Identifier: GPL-3.0-or-later

//! The memory the document keeps per note on the heavy score (ADR-0007's RAM budget, ADR-0024
//! Consequences; SCALETTA F1.09c, owner's choice E7: a test with a ceiling per note).
//!
//! # The score
//! The shape of the heavy score of `spikes/model/` (ADR-0022): 1 000 bars of 4/4 with a passage of
//! 6/8 from bar 60 to 69 of every hundred; two pianos (two staves, two voices per staff) and 26
//! one-staff instruments, every fourth with a second voice: 30 staves, 41 voices. Every voice fills
//! every bar with a rhythmic unit chosen per voice every 8 bars, 15% rests, chords of 1–4 notes.
//! It is the plain preset of `orchestra-test-scores` (ADR-0031), generated with the real builder and
//! shared with the editor's benchmarks. Unlike the spike, there are no triplets and quintuplets (they would
//! need tuplets, which only add a small table per voice), so the note count is close to, but not
//! exactly, the spike's 255 776.
//!
//! # What is measured
//! The growth of the live heap while the document is built and kept
//! ([`AllocationStats::net_bytes`](orchestra_test_support::AllocationStats::net_bytes)): every
//! chunk, the derived indices, the tables; temporary buffers freed before the end cancel out. It
//! is divided by the number of noteheads, so it is "bytes of document per note", events and their
//! overhead included. It counts what the program requests, not the allocator's rounding: a lower
//! bound of the resident memory, the same on every platform.

// Helper functions (not `#[test]` functions) are outside the scope of clippy.toml's
// `allow-unwrap-in-tests`, so the exemption is stated here, for this test file only.
#![allow(
    clippy::unwrap_used,
    reason = "test helpers: a failed unwrap fails the test"
)]

use orchestra_core_model::{Document, EventBody};
use orchestra_core_time::BarIndex;

// The plain preset of the reference scores (ADR-0031): `plain(size, editing)`.
use orchestra_test_scores::{Size, plain};

orchestra_test_support::install_counting_allocator!();

/// The ceiling per note, in bytes: what this test measured when it was written (2026-11-03: 257
/// bytes per note, 221 169 noteheads, 56.9 MB), plus about 10%, rounded up to a multiple of 5
/// (owner's decision of the same day). A change of the data structures that crosses it must be a
/// deliberate decision: raise the ceiling in the same commit and say why.
const CEILING_BYTES_PER_NOTE: i128 = 285;

/// The number of noteheads of a document.
fn noteheads(document: &Document) -> usize {
    let bars = u32::try_from(document.bars().len()).unwrap();
    document
        .tracks()
        .iter()
        .flat_map(|t| t.instrument().voices())
        .flat_map(|v| (0..bars).flat_map(move |i| v.events_at(BarIndex::new(i))))
        .map(|placed| match placed.event().body() {
            EventBody::Chord(chord) => chord.notes().count(),
            EventBody::Rest(_) => 0,
        })
        .sum()
}

/// The document keeps at most [`CEILING_BYTES_PER_NOTE`] bytes per notehead on the heavy score.
#[test]
fn memory_per_note_on_the_heavy_score() {
    let (document, stats) =
        orchestra_test_support::count_allocations(|| plain(Size::Heavy, false)).unwrap();
    let notes = noteheads(&document);
    let per_note = stats.net_bytes() / i128::try_from(notes).unwrap();
    // Printed so that `cargo test -- --nocapture` shows the numbers the ceiling is based on.
    println!(
        "heavy score: {notes} noteheads, {} bytes kept, {per_note} bytes per note",
        stats.net_bytes()
    );
    assert!(
        per_note <= CEILING_BYTES_PER_NOTE,
        "{per_note} bytes per note, above the ceiling of {CEILING_BYTES_PER_NOTE}"
    );
}
