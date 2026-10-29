# ADR-0007 — Performance policy: reference hardware, budgets, real-time rules

- **Status:** Accepted (budgets are initial targets; changing them requires a new ADR)
- **Date:** 2026-10-28

## Context

Efficiency is the number-one priority (CLAUDE.md §5). Musicians often work on modest laptops, and
real-time audio has hard deadlines: a missed buffer is an audible click.

## Decision

### Reference hardware

A mid-range laptop of about five years ago: 4 cores / 8 threads (Core i5-1135G7 or Ryzen 5 5500U
class), 8 GB RAM, integrated GPU, SSD, 1080p screen. Every feature must run well there.

### Initial budgets (to be calibrated after the first measurements)

| Area | Budget |
|---|---|
| Audio callback | ≤ 50 % of the buffer period (256 frames @ 48 kHz ≈ 5.3 ms → ≤ 2.6 ms) with the *reference project* (16 audio + 16 MIDI tracks, internal instruments only); **zero xruns in 10 minutes** |
| Input → frame | ≤ 50 ms |
| Scroll/zoom 100 bars × 10 staves | 60 fps with GPU, ≥ 30 fps software rendering |
| Re-layout after editing one note | ≤ 16 ms |
| Startup to interactive | ≤ 3 s |
| Open a 100-bar score | ≤ 1 s |
| RAM at rest, empty project | ≤ 250 MB |

### Rules

1. **Audio thread:** no allocations, no locks, no I/O, no unbounded waits. Communication only
   through fixed-capacity lock-free structures. A test fails if the audio thread allocates (via an
   allocation-counting global allocator in test builds).
2. **Choose the best algorithm and data structure from the start** and document the complexity in
   the comment; then measure. Never optimise at random.
3. **Every hot path has a benchmark** (`cargo xtask bench`). Prefer deterministic measures
   (instruction counts, allocation counts) to wall-clock times, which are noisy on shared CI
   runners.
4. **Incremental layout and rendering:** never recompute the whole score for one note.
5. **Software rendering fallback** when no GPU is available.
6. **Video through low-resolution proxy files** for editing; full resolution only for export.
7. **Every new dependency** is also evaluated for binary size and compile time.

## Alternatives rejected

- **"Make it work, optimise later":** in a real-time, incremental architecture, the data
  structures *are* the performance; changing them later means rewriting.

## Consequences

- The reference project and the measuring harness must be built early (Phase 3 for audio,
  Phase 2 for layout).
- Benchmarks run in CI; regressions are visible in PRs.
