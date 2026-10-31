# ADR-0017 — Benchmark regression gate: same-job comparison with the merge base, +5% instructions

- **Status:** Proposed (SCALETTA F1.03b; options approved by the owner on 2026-10-31)
- **Date:** 2026-10-31
- **Completes:** ADR-0016 §3 (regression comparison against `main` and its thresholds)

## Context

ADR-0007 wants performance regressions to be visible in pull requests; ADR-0016 chose Gungraun
(instruction counts under Callgrind) and deferred the comparison against `main` and its
thresholds until real benchmarks existed to calibrate them. Step F1.03b added the first ones
(the rational type of ADR-0015) and measured them.

What Gungraun 0.20 offers (checked in its sources): a run can save its numbers as a named
**baseline** (`--save-baseline`), compare against a saved baseline (`--baseline`), and fail with
exit code 3 when a metric exceeds a **limit** relative to the baseline (`--callgrind-limits`,
e.g. `ir=5%`). Every option can also be given as an environment variable (`GUNGRAUN_…`).

Facts that shape the decision:

- Instruction counts are deterministic **for a given binary**: two runs of the same build gave
  identical counts, and the base commit built in another directory gave the same counts as the
  same code built in place. They do change with the compiler, Valgrind and the CPU model the
  runner image reports.
- Small benchmarks move by a few instructions when **unrelated** code in the same binary
  changes (inlining and layout): in a test, `measure_empty_closure` went from 163 to 164
  instructions although only another function of the file had changed. On a 15-instruction
  benchmark one instruction is already 7%.
- CI runs on `push` events (every branch), not on `pull_request` events, so pull-request labels
  are not available to the job.
- `.git/` is read-only inside the development sandbox.

## Decision

1. **Where the base numbers come from: measured in the same job.** On every branch except
   `main`, the `bench` job measures the **merge base** of the pushed commit and `origin/main`,
   then the pushed commit, on the same runner with the same tools. `cargo xtask bench --compare
   <rev>` implements it (CLAUDE.md §3: automation goes through xtask), so it also works locally.
   The base commit is exported with `git archive | tar -x` into `target/bench-base/` (no
   `.git/` writes, nothing to clean up in git) and built in its own target directory,
   `target/bench-base-target/`. Sharing `target/` was tried and is wrong: cargo names the
   artifacts of workspace members from paths relative to the workspace root, so both trees produce
   the same file names, and its modification-time freshness check let the base commit's benchmark
   binaries run for the current tree (seen in this step's first CI run). The merge base, not the tip of `main`, so that a branch is judged
   on its own changes only.
2. **On `main`:** the numbers are published, not compared.
3. **What is checked: instructions only, at most +5%.** The limit is `ir=5%` against the base;
   improvements never fail. Cache and cycle figures are Callgrind estimates and stay informative.
   The value is calibrated on the first measurements: the batched benchmarks (rule 4) count
   between about 1,100 and 77,000 instructions, so a drift of one or two instructions like the
   one observed is at most 0.2%, while a real slowdown of a hot path (an extra gcd, a lost fast
   path) moves a count by tens of percent. 5% sits well between the two. The limit lives in one constant,
   `INSTRUCTION_LIMIT` in `xtask/src/bench.rs`; changing it needs a new ADR (as for the budgets of
   ADR-0007).
4. **Every benchmark runs a batch.** A benchmark repeats its operation on a batch of inputs (64
   for the rational type and the allocation counter) so that its count is in the thousands and a
   drift of one or two instructions stays far below the limit. A benchmark whose meaning changes
   (for example a single operation becoming a batch) gets a **new name**: Gungraun keys
   baselines by name, so the new benchmark starts without a baseline instead of reporting a
   meaningless jump.
5. **New benchmarks pass.** A benchmark with no baseline in the base commit is recorded, not
   compared (Gungraun's behaviour); it becomes enforced from the next branch on.
6. **Accepting a deliberate regression: a commit trailer.** If the message of the pushed (HEAD)
   commit has the trailer `Bench-regression: accepted`, the comparison runs and is shown, but no
   limit is applied. The commit message must say why the slowdown is accepted. A trailer because
   labels are unavailable on `push` events, and because the project already uses trailers
   (`User-visible`, ADR-0009). Any other value leaves the gate on.
7. **The gate is enforced by branch protection.** The `bench (instruction counts)` job is a
   required status check of the `main` ruleset (added by the owner on 2026-10-31), so a branch
   that regresses cannot be merged without the trailer.
8. **Visibility.** The job summary shows the second half of the comparison (the pushed commit
   against the base, with the percentage differences); the full output and every `summary.json`
   are uploaded as the `benchmarks` artifact.

## Alternatives rejected

- **Baseline cached from `main` runs (GitHub Actions cache):** cheaper (one run instead of two),
  but caches are evicted after 7 days without use, may come from an older commit than the merge
  base, and may have been produced by a different runner image, compiler or Valgrind, reporting
  differences nobody made.
- **Numbers committed to the repository:** reviewable, but every deliberate change and every
  toolchain bump would need a manual update of the numbers, and they would still depend on the
  machine that produced them.
- **Pull-request labels to accept a regression:** not visible to `push`-triggered jobs; switching
  CI to `pull_request` events would change how every other job is triggered.
- **Absolute limits (`ir=10000`):** they bound the cost of an operation, not its change; they
  would need updating with every legitimate change and say nothing about regressions.
- **A tighter limit (1–2%):** within reach of the drift of small benchmarks; the batches make it
  possible later, if the measurements of Phase 2 and 3 show the need.
- **A third-party comparison service or action:** a dependency and an external service for what
  Gungraun and a few lines of xtask already do.

## Consequences

- The `bench` job does two builds and two benchmark runs and clones the whole history, so it
  takes longer on branches than on `main` (measured in this step's CI runs, see the developer
  documentation).
- The comparison only runs on Linux (Valgrind); `cargo xtask bench --compare` on Windows and
  macOS only compiles the benchmarks, like plain `bench`.
- Locally, `cargo xtask bench --compare main` (or `--compare HEAD` for uncommitted changes) shows
  what CI will say before pushing.
- If the base commit cannot be benchmarked with the current tools (for example after a toolchain
  bump), the comparison fails; with the trailer, xtask measures the pushed commit alone and says
  why, so the bump's own branch can pass while explaining it in its message.
