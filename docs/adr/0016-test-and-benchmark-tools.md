# ADR-0016 — Test and benchmark tools: Gungraun, proptest, own allocation counter

- **Status:** Accepted (owner's decision, 2026-10-31; SCALETTA F1.02, decision D03)
- **Date:** 2026-10-31
- **Completes:** ADR-0007 rules 1 and 3 (allocation test on the audio thread, deterministic
  benchmarks run by `cargo xtask bench`)

## Context

ADR-0007 requires that every hot path has a benchmark, that **deterministic measures**
(instruction counts, allocation counts) are preferred to wall-clock times, that benchmarks run in
CI with regressions visible in pull requests, and that "a test fails if the audio thread
allocates (via an allocation-counting global allocator in test builds)". ADR-0015 adds that every
`Rational` operation must be covered by property tests and proven allocation-free. Step F1.02
chooses the tools before the first hot path (the rational type, F1.03) is written.

Constraints: GPL-compatible licenses (ADR-0002), active maintenance, dependency weight and
compile time (CLAUDE.md §5, §10), CI on Linux, Windows and macOS (ADR-0006), `unsafe` only in
designated crates (CLAUDE.md §4). All tools below are **dev-dependencies**: none of them ends up
in the shipped program.

## Candidates examined (crates.io and GitHub checked on 2026-10-31)

Dependency counts are the unique crates in `cargo tree -e normal`; build times are clean
`--release` builds of an empty crate depending only on the candidate, on the development machine
(4 threads). They are only meant to compare the candidates with each other.

### Benchmarks

| Crate | Version / last release | License | Deps | Build | Notes |
|---|---|---|---|---|---|
| `gungraun` (formerly `iai-callgrind`) | 0.20.0 / 2026-09-26 | Apache-2.0 OR MIT | 33 | 16 s | Runs each benchmark under Valgrind's Callgrind: **instruction counts**, cache and branch estimates, deterministic run to run. Linux only in practice. Needs Valgrind and the `gungraun-runner` binary of the **same version**. Repository pushed 2026-10-23. |
| `criterion` | 0.8.2 / 2026-02-04 | Apache-2.0 OR MIT | 46 | 33 s | The classic statistical wall-clock framework, all platforms. Heaviest tree; wall-clock numbers on shared CI runners are noisy. |
| `divan` | 0.1.21 / 2025-04-10 | MIT OR Apache-2.0 | 21 | 12 s | Lighter wall-clock framework, all platforms. Repository still active (2026-07) but no release for 18 months. |
| `tango-bench` | 0.9.0 / 2026-10-16 | MIT | — | — | Paired comparison against a baseline build, which reduces noise. Young; not evaluated further. |

### Property tests

| Crate | Version / last release | License | Deps | Build | Notes |
|---|---|---|---|---|---|
| `proptest` | 1.12.0 / 2026-10-10 | MIT OR Apache-2.0 | 15 | 11 s | Strategy-based generation, strong shrinking to a minimal counter-example, persisted regression seeds. Very active. |
| `quickcheck` | 1.1.0 / 2026-02-10 | Unlicense OR MIT | 7 | 3 s | Smaller and faster to build; type-based generation, weaker shrinking. |

### Allocation counting

| Crate | Last release | License | Notes |
|---|---|---|---|
| `stats_alloc` | 0.1.10 / 2022-03-30 | MIT | Unmaintained; global (not per-thread) counters. |
| `assert_no_alloc` | 1.1.2 / 2021-08-03 | BSD-1-Clause | Unmaintained; license not in our allow list (`deny.toml`). |
| `allocation-counter` | 0.8.1 / 2023-09-21 | MIT OR Apache-2.0 | Small, but inactive for three years. |
| `dhat` | 0.3.3 / 2024-02-04 | MIT OR Apache-2.0 | A heap profiler: far more than we need, and global. |

## Decision

1. **Benchmarks: Gungraun only.** `cargo xtask bench` runs the Gungraun benchmarks of the
   workspace and reports instruction counts. No wall-clock framework is added for now; one is
   added (by a new ADR) only when a measurement genuinely needs wall-clock time on Windows or
   macOS. The audio callback budget of ADR-0007 is measured in Phase 3 inside the engine itself,
   not by a benchmark framework.
2. **Benchmarks in CI.** A dedicated job on `ubuntu-latest` installs Valgrind and the matching
   `gungraun-runner`, runs `cargo xtask bench` and publishes the numbers (job summary and
   artifact). On Windows and macOS the benchmarks are **compiled only** (`cargo bench --no-run`),
   so that they never rot on a platform where they cannot run.
3. **Regression comparison deferred to F1.03.** Comparing a pull request against `main` and
   failing above a threshold needs real benchmarks to calibrate; it is designed in F1.03 together
   with the first benchmarks of the rational type. Until then the numbers are visible but not
   enforced.
4. **Property tests: `proptest`.** Failing cases are written by proptest to
   `proptest-regressions/` next to the test file; those files are **committed** and become
   regression tests. The exact oracle for the rational type (ADR-0015 mentioned an
   arbitrary-precision crate as an option) is chosen in F1.03; no oracle crate is added now.
5. **Allocation counter: our own, in a new test-only crate.** `crates/test-support` (package
   `orchestra-test-support`) provides a global allocator that wraps the system allocator and
   counts allocations, reallocations and deallocations **per thread** (const-initialised
   thread-local counters with no destructor, so reading them never allocates and parallel tests
   do not disturb each other), plus helpers to count the allocations of a closure and to assert
   that a closure does not allocate. Each test binary that needs it registers it with
   `#[global_allocator]`.
6. **That crate is designated for `unsafe`.** Implementing `GlobalAlloc` is `unsafe` by
   definition; the crate joins the FFI/audio crates that may use `unsafe`, with a `// SAFETY:`
   comment on every block. It sits at the **bottom** of the dependency graph (it depends on no
   workspace crate), so every crate, `core-time` included, can use it without breaking the
   "only downwards" rule. It is allowed **only in `[dev-dependencies]`**: `cargo xtask check`
   verifies that no crate depends on it as a normal or build dependency.
7. **Valgrind is a development tool, not a native dependency** in the sense of ADR-0001: it is
   never linked into the program. It is nevertheless listed in `docs/native-deps.md` under the
   tools used by development and CI, for transparency.

## Alternatives rejected

- **`criterion` or `divan` as the main tool:** wall-clock times are exactly what ADR-0007 asks
  us to avoid as the primary measure on shared CI runners. `criterion` is also the heaviest tree
  of all candidates; `divan` has had no release for 18 months.
- **Gungraun plus a wall-clock framework from day one:** two benchmark harnesses to maintain
  before any measurement needs the second one.
- **`quickcheck`:** lighter, but its shrinking is weaker, and minimal counter-examples matter
  for arithmetic edge cases (`i64::MAX`, values that fit only after reduction).
- **Existing allocation counters:** unmaintained, global counters (wrong under parallel tests)
  or a license we do not allow.
- **Allocator inside `audio-engine`** (already designated for `unsafe`): `core-time` and
  `core-model` would then depend on a crate above them, against the dependency rule.

## Consequences

- Benchmarks run locally only on Linux, with Valgrind and the exact `gungraun-runner` version
  installed. When the `gungraun` version in `Cargo.toml` changes, the runner must be reinstalled
  with the same version, locally and in CI.
- The workspace gains a twelfth crate; `docs/ARCHITECTURE.md`, CLAUDE.md §2 (crate list) and
  the developer documentation are updated with it.
- The CI gains a benchmark job on Linux and a compile-only benchmark step on Windows and macOS.
- The same allocation counter will back the audio-thread test of Phase 3 (ADR-0007 rule 1).
