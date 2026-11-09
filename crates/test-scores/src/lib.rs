// SPDX-License-Identifier: GPL-3.0-or-later

//! # orchestra-test-scores — the reference scores, for tests and benchmarks only
//!
//! ## Purpose
//! Generate, deterministically and from code, the scores that the tests and the benchmarks of
//! every crate above the model measure (ADR-0031, SCALETTA F1.16): the **reference score**
//! (100 bars × 10 staves, the size named by the budgets of ADR-0007) and its **heavy** version
//! (1 000 bars × 30 staves), which shows how a cost grows. The same scores will be used by the
//! layout (Phase 2) and the audio engine (Phase 3), so every phase measures the same music.
//!
//! ## Presets
//! - [`plain`]: notes, rests and chords only, exactly as generated since SCALETTA F1.09c. The
//!   existing benchmarks use it, so its output never changes by accident (ADR-0031 §6–§7).
//! - [`full`]: the reference score proper (SCALETTA F1.16b): a plausible piece in five sections
//!   with repeats, voltas, a D.S. al Coda and a free cadenza, for a real ensemble, using
//!   everything the model can hold.
//!
//! ## How to use it
//! ```
//! use orchestra_test_scores::{Instruments, Size, plain};
//!
//! // The normal plain score, without the empty "editing" instrument at the end.
//! let document = plain(Size::Normal, Instruments::Plain);
//! assert_eq!(document.bars().len(), 100);
//! assert_eq!(document.tracks().len(), 9); // one piano (two staves) + 8 one-staff instruments
//! ```
//!
//! ## What this crate must NOT do
//! - Be used outside `[dev-dependencies]` (ADR-0031 §2, checked by `cargo xtask check`): it is
//!   test machinery and never part of the program.
//! - Be used by the unit tests inside `core-time` or `core-model` (ADR-0031 §3): those are
//!   compiled as a second copy of their crate, whose types would not match the ones this crate
//!   was built against. Integration tests (`tests/`) and benchmarks (`benches/`) are fine.
//! - Depend on anything random or platform-dependent: the same call returns the same document,
//!   IDs included, on every platform and every run.
//!
//! ## Allowed dependencies
//! `orchestra-core-time` and `orchestra-core-model` only (ADR-0031 §1).
//!
//! ## Unsafe code
//! None, and forbidden (ADR-0031 §4).

#![forbid(unsafe_code)]
// ADR-0031 §5: the generators give the model's builders fixed, valid input, so a builder error
// can only mean the generator itself is wrong; stopping loudly is the useful reaction, and a
// `Result` would be unwrapped by every caller anyway. This exception to the library rule
// (CLAUDE.md §4) is acceptable only because the crate never reaches the program (ADR-0031 §2).
#![allow(
    clippy::unwrap_used,
    reason = "test-only generators of valid input: a failure is a bug in the generator (ADR-0031 §5)"
)]

mod full;
mod plain;

/// The full preset (see the [module documentation](self) and [`full::score`]).
pub use full::score as full;
/// Which instruments the plain preset has (see [`plain::score`]).
pub use plain::Instruments;
/// The plain preset (see the [module documentation](self) and [`plain::score`]).
pub use plain::score as plain;

/// The size of a generated score.
///
/// The two sizes of `spikes/model/` (ADR-0022): `Normal` is the reference score of the budgets
/// (ADR-0007: "100 bars × 10 staves"), `Heavy` ten times longer and three times wider, to show
/// whether a cost grows linearly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Size {
    /// 100 bars, 10 staves.
    Normal,
    /// 1 000 bars, 30 staves.
    Heavy,
}
