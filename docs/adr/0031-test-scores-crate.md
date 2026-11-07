# ADR-0031 — A test-only crate for the reference scores

- **Status:** Accepted (owner's decision, 2026-11-07; SCALETTA F1.16, option B)
- **Date:** 2026-11-07
- **Leaves unchanged:** ADR-0016 §6 (`test-support` depends on no workspace crate and is used only
  in `[dev-dependencies]`). This ADR adds a second test-only crate next to it rather than
  amending it.

## Context

SCALETTA F1.16 asks for a reusable, deterministic generator of the **reference score**
(100 bars × 10 staves) and of its **heavy** version (1 000 bars × 30 staves), to be used by the
benchmarks of Phase 1 and again by the layout (F2) and the audio engine (F3). Today the
generator is `crates/core-model/tests/common/score.rs`, included with `#[path]` into seven test
and benchmark files of three crates (`core-model`, `io`, `playback`). That works but has three
defects: every includer compiles its own copy, a crate reaches into another crate's `tests/`
directory, and the file can only hold what one crate's tests need.

A generator builds a `Document`, so it depends on `core-model` (and `core-time`). The natural
home looked like `test-support`, but ADR-0016 §6 keeps that crate at the **bottom** of the
dependency graph, depending on no workspace crate, precisely so that `core-time` and `core-model`
can use its allocation counter. Making it depend on `core-model` would break that invariant,
make every test build of `core-time` compile `core-model` as well, and give the unit tests inside
`core-model` (`#[cfg(test)]`) two incompatible copies of the model's types.

## Decision

1. **A new workspace crate `orchestra-test-scores`** in `crates/test-scores`, holding the
   deterministic generators of the scores used by tests and benchmarks. Its normal dependencies
   are `orchestra-core-time` and `orchestra-core-model` only, and nothing else (no external
   crate; the random numbers come from a small xorshift written in the crate, as today, so the
   output is identical on every platform and every Rust version).
2. **Test-only, like `test-support`:** any crate may use it, but **only in
   `[dev-dependencies]`**. `cargo xtask check` enforces this with the same check that guards
   `test-support` (the list of test-only crates gains this one), so it can never end up in the
   program.
3. **Where it can be used:** integration tests (`tests/`) and benchmarks (`benches/`). The unit
   tests inside `core-model` and `core-time` cannot use it (they would see a second copy of the
   types); they do not need it today. A crate above `core-model` (e.g. `io`, `playback`, later
   `notation-layout` and `audio-engine`) can use it anywhere in its tests.
4. **No `unsafe`:** the crate is not designated for `unsafe` (CLAUDE.md §4); it uses
   `#![forbid(unsafe_code)]`.
5. **Failures panic, by exception to the library rule.** The generators feed fixed,
   known-valid input to the model's builders; a builder error means the generator itself is
   wrong, and the only useful reaction is to stop the test or benchmark loudly. The crate
   therefore allows `unwrap`/`expect` with a stated `reason` (as `score.rs` does today), instead
   of returning errors that every caller would unwrap anyway. This is acceptable only because of
   point 2: the crate is never part of the program.
6. **Two presets** (owner's choice, 2026-11-07):
   - **plain**: the current `score.rs` moved unchanged, producing **identical** documents, so the
     existing benchmarks keep their numbers and the regression gate (ADR-0017) keeps comparing
     like with like;
   - **full**: the reference score proper, using everything the model can hold (form with repeats,
     voltas and jumps, tuplets, ties, grace notes, articulations, slurs, dynamics and hairpins,
     tempo marks including gradual changes, key and clef changes), in the normal and heavy sizes.
     It passes `validate` and survives a save/load round trip unchanged. It is defined in F1.16b.
7. **Stable output:** a change to what a preset generates changes the inputs of the benchmarks,
   so it is made deliberately, in its own commit, with the trailer `Bench-regression: accepted`
   when the gate fires (ADR-0017).

## Alternatives rejected

- **Put the generator in `test-support`** (amending ADR-0016 §6): breaks the "bottom of the
  graph" invariant that lets `core-time` use the allocator, makes `core-time`'s test builds slower,
  and makes the generator unusable from `core-model`'s unit tests in a confusing way.
- **Keep the `#[path]` inclusion:** one copy compiled per includer, a crate reaching into another
  crate's `tests/` directory, and no natural place for code shared by F2 and F3.
- **A feature of `core-model`** (`reference-score`): the generator would live in the shipped crate,
  behind a flag that features unification can turn on unexpectedly; a test-only crate is enforced
  by the existing `xtask` check instead.
- **Return `Result` from the generators:** every caller would unwrap it; the errors would carry no
  information the panic message does not.

## Consequences

- The workspace has two test-only crates; CLAUDE.md §2 and `docs/ARCHITECTURE.md` list the new
  one, and `xtask`'s `TEST_ONLY_CRATES` gains it.
- `crates/core-model/tests/common/score.rs` is deleted; its seven includers depend on the new
  crate instead (F1.16).
- `spikes/model/` stays as it is: ADR-0022 cites its measurements.
- The small purpose-built `score(n)` helpers of `playback`'s case tests stay where they are: they
  are targeted cases, not reference scores.
