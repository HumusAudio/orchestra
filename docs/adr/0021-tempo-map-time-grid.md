# ADR-0021 — Tempo map: boundary times on a fixed grid, rounded positions inside gradual changes

- **Status:** Accepted (owner's decision, 2026-11-01; SCALETTA F1.05b). The content was approved
  before the text was written.
- **Date:** 2026-11-01
- **Amends:** ADR-0020 §3 (the source data of a gradual change), §4 (where exactness ends) and §5
  (how the exact sample index is computed). Everything else in ADR-0020 stays as it is, in
  particular D05: a gradual change is linear in time.

## Context

ADR-0020 §4 promised that every boundary time of the tempo map (every tempo change, the start and
end of every gradual change) is an exact rational, cumulated in a table, and that inside a gradual
change the direction seconds → position is exact (a polynomial). ADR-0015 fixed the rational type
as an `i64` fraction with typed overflow and no big integers.

Before implementing the map, the two promises were checked against realistic numbers (a Python
simulation with exact fractions, 2026-11-01): random sequences of metronome marks between 40 and
208 (quarter-note unit), segments of 1 to 16 bars, 40% of the changes gradual.

- **Cumulated boundary times overflow.** A gradual change lasts `2L/(s₀+s₁)`; the denominators of
  different changes multiply, so the denominator of the cumulated time grows without bound. With
  an `i64` fraction, every one of 200 random scores overflowed after **8–13 gradual changes**
  (median 13); with an `i128` fraction, after **31–53**. With constant tempi only, 40 changes never
  overflowed, but the growth is the same in kind (any metronome mark with a new prime factor
  multiplies the denominator). In practice adding a *rit.* to a long score would fail with an
  overflow error: unacceptable.
- **Exact positions inside gradual changes are not representable.** For a gradual change starting
  at a non-trivial time, the exact position of a sample instant `n/48000` (`x = s₀τ + aτ²/2`)
  fitted in an `i64` fraction in only **25%** of 2000 random cases: its denominator grows like the
  square of the time's denominator.

Exactness of every boundary is therefore impossible with any fixed-width fraction. What we really
need from it is weaker and still achievable: **no drift** (the error must not grow with the
length of the score in any audible or visible way), **determinism** (the same numbers on every
platform), and **one table** shared by every track, so tracks can never disagree.

## Decision

### 1. Boundary times lie on a fixed grid of 1/705 600 000 s

The tempo map's time grid is `G = 705 600 000` ticks per second (the "flick", see the
alternatives; `G = 2⁹ · 3² · 5⁵ · 7²`). It divides every common sample rate (8, 11.025, 16,
22.05, 24, 32, 44.1, 48, 88.2, 96, 176.4, 192 kHz) and every common integer frame rate (24, 25,
30, 48, 50, 60, 90, 100, 120, 240 fps).

The start time of the first segment is 0. The start time of every following segment is
`t_{k+1} = round_G(t_k + D_k)`: the previous start plus the **exact** rational duration `D_k` of
the previous segment (`L/s` for a constant segment, `2L/(s₀+s₁)` for a gradual one), rounded to the
**nearest** grid point, ties going to the later point (the same rule as samples, ADR-0020 §5).

- Every boundary time has a denominator dividing `G`: denominators no longer grow, and an `i64`
  numerator holds about 3.6 million hours.
- The error is at most half a tick per boundary and grows at most linearly: below **1 µs after
  1000 tempo changes**, against 10.4 µs for half a sample at 48 kHz. It is deterministic and the
  same on every platform (exact integer arithmetic, no floating point).
- A segment whose exact duration is already a multiple of `1/G` adds no error at all; for example
  every constant segment at an integer metronome mark of a quarter note whose prime factors are
  2, 3, 5 and 7 only (60, 72, 84, 96, 100, 120, 126, 144…), over any length in sixteenths or
  triplet eighths. The rule is nevertheless applied uniformly to every segment, so that the
  behaviour of a boundary never depends on whether an earlier one happened to fit.

### 2. Inside a segment, times are exact relative to its start

Every conversion inside a segment works with the local time `τ = t − t_k` and the local position
`x = p − p_k`, both exact. A segment's time span is `[t_k, t_{k+1})`; because of the rounding of
§1 it can be up to half a tick longer or shorter than the exact `D_k`:

- if the next start comes **after** the exact end, the times in between map to the segment's end
  position (clamped), so time → position stays non-decreasing;
- if it comes **before**, the last half tick of the segment is never reached: those times belong
  to the next segment.

### 3. Position → seconds

- **Constant segment:** exact, `t_k + x/s`, returned as `Seconds`.
- **Gradual segment:** exact at its start; elsewhere the time is irrational in general and is
  returned as an approximate `f64` (ADR-0020 §4, unchanged).

The lookup returns one value that says which of the two it is, an enum with an `Exact(Seconds)`
and an `Approximate(f64)` variant, so a caller can never mistake an approximation for an exact
value.

### 4. Seconds → position

- **Constant segment:** exact, `p_k + s·τ`.
- **Gradual segment:** the exact value `s₀τ + aτ²/2` is computed with `i128` intermediates and
  **rounded to the nearest point of a position grid** of `1/G` whole notes (the same number `G`,
  ties to the later point): about 2.8 ns at ♩ = 120. The grid contains every subdivision a score
  uses in practice (powers of two, triplets, quintuplets, septuplets and their products within
  the factors of `G`). The rounding error is far below anything a user can play or see, and the
  result is always a valid `Position`. If even the `i128` intermediates overflow, the result is a
  typed error, never a panic.

### 5. Exact sample and frame indices inside gradual changes

ADR-0020 §5 stays: an `f64` estimate of the index, then an exact check and correction against
the positions of the sample (or frame) boundaries, moving the index until it is right. The check
compares **exact local positions computed with `i128` intermediates**, not the rounded positions
of §4 (rounding there would make the index depend on the grid instead of the true curve). The
`f64` fallback on overflow of the check stays and is measured in the second part of F1.05b.

### 6. Source data of a gradual change: an optional starting tempo

A gradual change may carry an **optional starting tempo**: "Meno mosso, poco a poco accel." is a
single entry that jumps to the starting tempo at `p₀` and then moves linearly in time to the
target tempo at `p₁`. Without a starting tempo, the change starts from the tempo in force at `p₀`
(ADR-0020 §3). This keeps one entry per position, as in the meter map. In addition:

- the **first** entry of the map is an immediate change at position 0 (the initial tempo);
- an immediate change may sit exactly at the end `p₁` of a gradual change ("rit. … a tempo");
- two gradual changes may follow each other directly (the second starts at the first one's `p₁`);
- a change strictly inside a gradual change remains an error (ADR-0020 §3).

## Alternatives rejected

- **`i128` fractions for the boundary column:** they only postpone the overflow (31–53 gradual
  changes in the simulation).
- **Big-integer fractions:** excluded by ADR-0015 for now; they allocate, and the conversions run
  on the audio thread, which must never allocate (ADR-0007); their cost grows with the score.
- **Exact while it fits, grid rounding after the first overflow:** exact in more cases, but the
  value of a boundary would depend on edits made far earlier in the score, and a boundary could
  change by half a tick when an unrelated earlier tempo is edited. Hard to explain and to test.
- **`f64` boundary times:** excluded by ADR-0005 (fragile equality, rounding that depends on
  the order of operations).
- **A finer or coarser grid:** a coarser grid (e.g. 7 056 000 = lcm(44 100, 48 000)) would make
  the error 100 times larger for no clear gain (whether it makes overflows of the local `i128`
  checks rarer will only be known from the measurements of F1.05b); a finer one would shorten the
  representable span of the `i64` numerator and divide no additional common rate. 705 600 000 is
  an existing convention (the "flick", published by Oculus in 2018) with exactly the divisibility
  we need.
- **Two separate entries (immediate, then gradual) at the same position for "Meno mosso, poco a
  poco accel.":** it would require an ordering rule between entries at the same position and
  break the one-entry-per-position invariant shared with the meter map.

## Consequences

- Adding a gradual change can no longer fail with an overflow in a realistic score.
- Times of boundaries are deterministic and identical on every platform, shared by every track,
  and their error is bounded (≤ half a tick per boundary) and documented; ADR-0020's statement
  "every time on a tempo-change boundary is exact" is replaced by "every boundary is on the grid,
  within half a tick per boundary of the exact value".
- A recorded time converted back to a position inside a gradual change lands on the `1/G` grid;
  quantisation for display and editing happens later, in the model and the interface.
- The tests compare the map with an oracle that uses the same rounding rule (deterministic), and
  separately check that the deviation from exact (big-rational) arithmetic stays within the bound.
- The project file stores only the source data (ADR-0020, Consequences); the grid is a constant of
  the program, so changing it later is a format-independent but behaviour-changing decision that
  needs a new ADR.
