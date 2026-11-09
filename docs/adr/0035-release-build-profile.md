# ADR-0035 — Release build profile: one codegen unit per crate

- **Status:** Accepted (owner, 2026-11-09, as written; the owner asked for this ADR after review 3,
  point C1).
- **Date:** 2026-11-09
- **Amends:** nothing. It fills a gap: until now the workspace used Cargo's default `release`
  profile, never decided. ADR-0017 (the benchmark gate) is unchanged; how its first run after
  this change behaves is described under Consequences.

## Context

Splitting `document.rs` into a module directory (review 2, P3.2, PR #49) made `to_parts` 6%
slower (1 231 868 → 1 308 207 instructions on the normal score) with no change to the code or
to the work done. Review 3 (C1) found the reason: all of it is in `[PlacedEvent]::to_vec`, the
derived `Clone` of a bar's events. LLVM now compiles it into a different loop (192 → 210
instructions in the function), because the split changed how the crate is cut into **codegen
units**. The owner accepted the regression (`Bench-regression: accepted`, ADR-0017).

Codegen units are the pieces a crate is split into so that LLVM can optimise them in parallel.
The default `release` profile uses 16 units and no link-time optimisation (LTO). A function in
one unit is inlined into another unit only if it is generic or marked `#[inline]`, and what ends
up in which unit depends on how the source is laid out. So **an ordinary refactoring can change
the generated code**, and the benchmarks see it as a regression or an improvement that has
nothing to do with the change. This will keep happening as the crates grow and get reorganised.

There is no source-level fix that lasts: `#[inline]` hints were tried in #49 and made things
worse as often as better. The lever that works is the build profile. With fewer codegen units,
or with LTO, the optimiser sees more of the program at once and the layout of the files stops
mattering. That costs build time and changes binary size, which CLAUDE.md §5 asks us to weigh, so
this needs an ADR.

### What only uses this profile

`release` is used by `cargo build --release` (the program we ship) and, through `bench`, which
inherits from it, by `cargo xtask bench` and the CI `bench` job. The `dev` and `test` profiles
(`cargo xtask check`, the CI `check` jobs, everyday work) are **not** affected by anything here.

### Measurements (2026-11-09)

Commit 59a5ce3 (`main`), Rust 1.99.1, measured on the owner's machine (Intel N150, 4 cores).
Every configuration built from an empty target directory. The settings were passed as
`CARGO_PROFILE_{RELEASE,BENCH}_{CODEGEN_UNITS,LTO}` environment variables, so the
manifest was not changed. Instruction counts come from Gungraun (ADR-0016) and do not depend on
the machine; build times are a single run on a slow machine, so read them as orders of magnitude.

| | default (16 units, no LTO) | `codegen-units = 1` | thin LTO | fat LTO + 1 unit |
|---|---|---|---|---|
| `to_parts`, normal score | 1 308 209 | **−5.5%** (1 236 653) | −2.8% (1 271 592) | −8.0% (1 203 513) |
| `add_dynamic`, normal | 296 153 | −4.0% | −2.1% | −6.9% |
| `save_bytes` (project file) | 42 309 313 | −9.3% | −2.1% | −10.7% |
| `load_bytes` (project file) | 74 676 170 | −4.9% | −2.9% | −8.1% |
| `perform::full`, heavy | 1 485 031 057 | −0.8% | −4.2% | −4.8% |
| mean over the 97 benchmarks | — | −2.6% | −4.1% | −7.0% |
| best / worst benchmark | — | −14.5% / **+2.0%** | −63.6% / −0.9% | −63.6% / **+6.2%** |
| cold build of all benchmarks | 139 s | 121 s | 396 s | 324 s |
| rebuild after touching `core-model` | 86 s | 84 s | 262 s | 222 s |
| `cargo build --release` of the `m1` example | 16 s | 34 s | 28 s | 50 s |
| size of `m1` | 2.74 MB | **1.91 MB (−30%)** | 2.74 MB | 1.73 MB (−37%) |

Notes on the table:

- With `codegen-units = 1`, `to_parts` goes back to where it was before the split (1 236 653
  against 1 231 868, +0.4%). The two benchmarks that get slower, `perform::update_dynamic` (+2.0%)
  and `perform::update_note` (+1.1%), stay well under the 5% gate.
- Thin LTO's largest gains (−64%, −54%, −29%) are the counting allocator's own benchmarks
  (`counting::*`, test-support), which measure a handful of instructions. They say little about
  the program.
- Fat LTO is the fastest on average, but three benchmarks get slower by more than the 5% gate:
  `form::validate_with_jumps` (+6.2%), `perform::update_dynamic` (+5.6%) and
  `voice::event_lookup` (+5.1%). It also has the slowest builds.
- The cold benchmark build is faster with one unit than with 16. Building all the benchmark
  binaries already keeps the 4 cores busy, so the parallelism inside a crate adds nothing, while
  16 units cost more total work. Building a single binary (`m1`) is where one unit is slower,
  about twice as slow.

## Options

1. **Keep the default.** Nothing to pay. Splitting or merging files can keep moving the
   benchmarks by a few percent at random, each time with a trailer and an explanation.
2. **`codegen-units = 1`** in `[profile.release]`. Fixes C1, everything a little faster, the
   binary 30% smaller, benchmark builds no slower, one release binary about twice as slow to
   build.
3. **Thin LTO.** Never slower than the default, but builds about three times as long (the bench
   job and every `cargo xtask bench`), and the binary is no smaller.
4. **Fat LTO plus one unit.** Fastest and smallest, but the slowest builds, and three benchmarks
   regress beyond the gate today.

## Decision

**Option 2.** Add to the workspace `Cargo.toml`:

```toml
# ADR-0035: one codegen unit per crate, so LLVM sees the whole crate when it optimises and the
# generated code no longer depends on how the source files are split. `bench` inherits it.
[profile.release]
codegen-units = 1
```

Nothing else changes: opt-level stays 3, no LTO, no change to `dev` or `test`, no change to
`panic` (the library code does not panic anyway, CLAUDE.md §4).

Why not LTO: thin LTO triples every benchmark build for gains that, outside the allocator
benchmarks, are about the size of option 2's. Fat LTO adds roughly another 4% on average, but it
costs the slowest builds and today fails the gate on three benchmarks. LTO can come back as its
own decision once there is a real binary (the `app` crate) to measure. At that point a
`[profile.dist]` for shipped builds, separate from `release`, would also be an option.

## Consequences

- **C1 is resolved:** `to_parts` is back to its value before the split. The accepted regression
  of #49 is no longer there.
- **Benchmarks stop following the file layout** within a crate. Splitting or merging modules
  should not need a `Bench-regression` trailer any more. Changes *across* crates still depend on
  inlining across crates (`#[inline]` or generics), as before.
- **Build time:** the CI `bench` job builds twice (the merge base and the tree, ADR-0017), and its
  build gets no slower and probably a little faster. A single `cargo build --release` takes about
  twice as long (16 s → 34 s for `m1` here). Everyday work (`dev`, `test`, `cargo xtask check`)
  is unaffected.
- **Binary size:** about 30% smaller for the same code (2.74 → 1.91 MB for `m1`).
- **The first gate run after the change:** on the pull request that adds the profile, the merge
  base is still built with the old profile, because the comparison builds `main` as it is. So the
  gate compares 16 units against 1 unit. The measurements say no benchmark gets slower by more
  than 2.0%, so the gate passes without a trailer. Any later pull request compares one unit
  against one unit.
- **What to watch:** if a future benchmark turns out slower with one unit by more than the gate,
  the gate will not show it on the PR that adds it (the base would have the same profile). This
  ADR records the measured state. Measuring the default again is one environment variable away
  (`CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16`).

## How to measure again

Use an empty target directory for each configuration (for example
`CARGO_TARGET_DIR=$TMPDIR/prof/target-<cfg>`). Then run, with `GUNGRAUN_ENVS=TMPDIR` under the
sandbox:

```sh
CARGO_PROFILE_RELEASE_CODEGEN_UNITS=<n> CARGO_PROFILE_BENCH_CODEGEN_UNITS=<n> \
CARGO_PROFILE_RELEASE_LTO=<false|thin|fat> CARGO_PROFILE_BENCH_LTO=<false|thin|fat> \
  cargo bench --workspace --locked --bench '*'
```
