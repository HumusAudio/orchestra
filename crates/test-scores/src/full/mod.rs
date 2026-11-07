// SPDX-License-Identifier: GPL-3.0-or-later

//! The **full** preset (ADR-0031 §6; SCALETTA F1.16b): the reference score proper, a plausible
//! piece that uses everything the model can hold.
//!
//! # What it is
//! The specification approved by the owner on 2026-11-07: a movement of 100 bars in five sections
//! (a pickup and a repeated section A with two voltas in 4/4, a section B in 6/8 in the relative
//! minor, a section C starting at the segno, a section D in 3/4 with a free cadenza bar for the
//! flute and a D.S. al Coda, and a coda), written for a real ensemble with realistic densities
//! of tuplets, ties, grace notes, articulations, ornaments, slurs, dynamics and hairpins. The
//! details live where they are made:
//! - [`plan`]: the bars, meters, keys, tempo marks, harmony and form of a movement;
//! - [`ensemble`]: the instruments, their clefs, transpositions, roles and registers;
//! - [`music`]: how each voice writes its notes;
//! - [`marks`]: the dynamics, hairpins, clef changes and the other per-instrument markings.
//!
//! # The two sizes
//! - **normal**: one movement (100 bars) for 10 staves (piano, flute, oboe, clarinet in B♭, horn
//!   in F, violins I and II, viola, violoncello);
//! - **heavy**: the movement ten times (1 000 bars), each in its own key, for an orchestra of 30
//!   staves.
//!
//! # What it deliberately leaves out
//! Three things of the "everything" document of the golden files that no plausible piece has
//! (owner's decision, 2026-11-07): the lettered bar-numbering style, a numbering restart and a
//! last bar shorter than its meter. Everything else of that document appears at least once in the
//! normal size: the coverage test of `tests/full.rs` checks it.
//!
//! # Stability
//! As for the plain preset (ADR-0031 §7), the output is pinned by a fingerprint test: a change to
//! anything here changes the inputs of the benchmarks and must be deliberate.
//!
//! # Complexity
//! Linear in the number of events: 3 396 events and 3 939 noteheads for the normal size, 75 811
//! and 77 225 for the heavy one. That is about half the plain preset's density, on purpose: wind
//! and brass rest for whole phrases, accompaniments have rests and held notes, the piano's second
//! voices play only in some sections, as in a real score (owner's choice A, 2026-11-07). The dense
//! case stays measured by the plain preset.

mod ensemble;
mod marks;
mod music;
mod plan;

use orchestra_core_model::{BarId, Document, DocumentBuilder, Tolerance};

use crate::Size;
use ensemble::Built;
use music::{Counters, Part, Rng};

/// The seed of the full preset's random choices: any non-zero constant (xorshift's only
/// requirement); changing it changes the whole score (ADR-0031 §7).
const SEED: u64 = 0x0F1_16B;

/// The full score of this size (see the module documentation). Re-exported at the crate root as
/// [`full`](crate::full).
///
/// # Order of construction
/// Bars first (so that every voice gets a chunk per bar), then the instruments, then movement by
/// movement the score-wide marks, each instrument's marks and each voice's notes. The order fixes
/// the IDs, which the fingerprint test pins.
///
/// # Panics
/// Only if the generator itself is wrong (ADR-0031 §5).
#[must_use]
pub fn score(size: Size) -> Document {
    let (movements, ensemble) = match size {
        Size::Normal => (1, ensemble::chamber()),
        Size::Heavy => (10, ensemble::orchestra()),
    };
    let mut b = DocumentBuilder::new(
        plan::meter_map(movements),
        plan::initial_tempo(),
        plan::key(plan::Section::A, 0),
    )
    .unwrap();
    let bars: Vec<BarId> = (0..movements * plan::BARS)
        .map(|_| b.add_bar().unwrap())
        .collect();
    let built: Vec<Built> = ensemble
        .iter()
        .map(|spec| ensemble::add(&mut b, spec))
        .collect();
    let mut rng = Rng(SEED);
    let mut counters = Counters::default();
    for m in 0..movements {
        let from = usize::try_from(m * plan::BARS).unwrap();
        let movement = &bars[from..from + usize::try_from(plan::BARS).unwrap()];
        plan::add_score_marks(&mut b, movement, m);
        for (spec, instrument) in ensemble.iter().zip(&built) {
            marks::add_instrument_marks(&mut b, spec, instrument, movement, m);
            for (k, &(id, voice)) in instrument.voices.iter().enumerate() {
                let part = Part {
                    spec,
                    id,
                    voice,
                    first: k == 0,
                };
                music::write_voice(&mut b, &mut rng, &mut counters, &part, movement, m);
            }
        }
    }
    // The piano keeps its own tolerance for DAW moves (ADR-0028): 25 ms, as a pianist's
    // microtiming is finer than an orchestra's.
    if let Some((_, piano)) = ensemble
        .iter()
        .zip(&built)
        .find(|(spec, _)| spec.name == "Piano")
    {
        b.set_tolerance(Some(piano.track), Some(Tolerance::Millis(25)))
            .unwrap();
    }
    b.build().unwrap()
}
