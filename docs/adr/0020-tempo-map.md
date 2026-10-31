# ADR-0020 — Tempo map: gradual changes linear in time, exact boundaries, exact sample rounding

- **Status:** Proposed (SCALETTA F1.05, decision D05; content approved by the owner on 2026-10-31
  before the text was written)
- **Date:** 2026-10-31
- **Completes:** ADR-0005 §2–§4 (the tempo map and the derived seconds, samples and frames) and
  the note of ADR-0015 "Consequences" on where exactness ends

## Context

ADR-0005 gives `orchestra-core-time` the single tempo map of a project and makes seconds, sample
indices and video frames **derived** values, computed from exact rational positions only at the
last step, with documented rounding and O(log n) lookups that do not allocate. ADR-0015 noted
that integrating a gradual tempo change generally gives a logarithm, so F1.05 must state where
exactness ends. ADR-0018 fixed `Position` and `Duration` (whole notes, written timeline).

Decision D05 asks in which quantity a gradual change (accelerando, ritardando) is linear. The
three candidates, on the example of a *rit.* from ♩ = 120 to ♩ = 60 over 16 quarter notes (8 s at a
steady 120, 16 s at a steady 60):

| | Linear in | Shape | Duration | Exact? |
|---|---|---|---|---|
| A | tempo vs **score position** | the same slowing on every beat | 16·ln 2 ≈ 11.09 s | no: a logarithm, so everything after it is inexact |
| B | tempo vs **time** (constant acceleration) | the slowing concentrates towards the end | 32/3 ≈ 10.67 s | yes: the total duration is rational |
| C | beat **period** vs score position | the slowing is strongest at the start | 12 s | yes |

Perception: Friberg and Sundberg (1999) measured final ritardandi and modelled the tempo as
`v(x) = [1 + (v_end^q − 1)·x]^(1/q)`, with `x` the normalised score position. They state that
q = 1 is a linear function of `x` (our A) and q = 2 "a square-root function of x (i.e., a linear
function of t)" (our B); q = 3 is the constant braking power measured on stopping runners. In
their listening experiment "the two curvatures that received the highest ratings originated from
the model with q=2 and q=3"; lower and higher q values, and a quadratic inter-onset-interval
function, were rated lower (A. Friberg, J. Sundberg, "Does music performance allude to
locomotion? A model of final ritardandi derived from measurements of stopping runners", J.
Acoust. Soc. Am. 105(3), 1999, pp. 1469–1484, abstract and §III; checked in the text on
2026-10-31). B is therefore one of the two preferred shapes, and the only one of the two whose
durations are rational.

## Decision

### 1. D05: a gradual change is linear in **time** (option B)

Inside a gradual change the **speed** `s` (whole notes per second, see §2) varies linearly with
time: constant acceleration `a`. With the change spanning `L` whole notes from speed `s₀` to `s₁`
(both > 0):

- its duration is `D = 2L / (s₀ + s₁)` — **rational**;
- `a = (s₁ − s₀) / D` — rational;
- time → position, `τ` seconds after its start: `x(τ) = s₀·τ + a·τ²/2` — a polynomial, **exact**
  for a rational `τ`;
- position → time: `τ(x) = 2x / (s₀ + √(s₀² + 2a·x))` — a square root, **not rational** in
  general (this form avoids the cancellation of `(−s₀ + √…)/a` when `a` is small).

The speed stays positive throughout (it moves linearly between two positive values), so both
functions are strictly increasing. The owner chose B on 2026-10-31.

### 2. Tempo values: a beat unit and a rational number of beats per minute

A `Tempo` is a beat unit (a positive `Duration`: ♩ = 1/4, ♩. = 3/8) and a positive `Rational`
number of beats per minute (120, 80, or 185/2 for 92.5). Its **speed** is
`unit × bpm / 60` whole notes per second: ♩ = 120 and ♩. = 80 are both 1/2. Linearity (§1) is in
speed, so a change between tempi written with different units is well defined. A metric
modulation (♩ = ♩.) is simply a new tempo whose speed is computed from the old one by the
interface or the model.

### 3. The tempo map works on **written** positions

Like the meter map, the tempo map describes the score as written. Its source data are tempo
changes at `Position`s, sorted, the first at position 0:

- **immediate**: from position `p` the tempo is `T`;
- **gradual**: from `p₀` to `p₁` the speed goes linearly in time from the tempo in force at `p₀` to
  `T`, which holds from `p₁` on.

No change may start strictly inside a gradual change (typed error, all-or-nothing edits returning
the previous value, as in ADR-0018 §4). Positions before 0 (notes played early, pre-roll) use the
first tempo extended backwards.

Repeats are not the tempo map's business: when the music jumps back, the written tempo at that
point applies again, which is what musicians expect. The **absolute clock of the unrolled DAW
timeline** (ADR-0004) is built by `orchestra-playback` as a cache, summing passes through the
tempo map. Fermatas, caesuras, swing and rubato are execution offsets (ADR-0004) handled there,
not tempo changes. Tempo changes are anchored to positions, so a meter change earlier in the score
does not move them with the bars: the same open question as for notes, noted for F1.06.

### 4. Where exactness ends

- **Positions** are always exact.
- **Times of segment boundaries** (every tempo change, the start and end of every gradual change)
  are exact rationals, cumulated in a table: no drift, however many gradual changes come before.
- **Constant-tempo segments**: position ↔ seconds is exact in both directions.
- **Gradual changes**: seconds → position is exact (the polynomial); position → seconds is
  irrational in general and is offered only as an approximate `f64`, for display.
- **Sample and frame indices are exact everywhere**, including inside gradual changes (§5).

### 5. Rounding rules, and how they are exact inside gradual changes

- **Samples** are instants: an event at time `t` sounds at the **nearest** sample,
  `n = ⌊t·rate + 1/2⌋`, ties going to the later sample. Maximum error half a sample (±10.4 µs at
  48 kHz). Sample rates are positive integers (Hz).
- **Frames** are intervals: time `t` belongs to the frame that contains it, `⌊t·fps⌋`. Frame rates
  are positive rationals (24/1, 25/1, 30000/1001), never floats (ADR-0005 §3).
- Indices are `i64` (negative before the start of the timeline).
- The inverse is exact: sample `n` is at `n/rate` seconds, frame `k` starts at `k/fps`, and their
  positions follow from the exact time → position direction.

Inside a gradual change `t(x)` is irrational, but the rounded index can still be computed
**exactly**, because the inverse direction is exact and increasing: compute an estimate `n` with
`f64`, then compare `x` exactly with the positions of the sample boundaries `(n ± 1/2)/rate` and
move `n` until `x(n − 1/2) ≤ x < x(n + 1/2)` (one or two steps in practice). If an intermediate of
this check overflows the `i64` rational (very long changes at very high sample rates, where the
denominators grow with `τ²`), the `f64` estimate is used instead and the case is counted by the
tests; its error is far below half a sample for any realistic score. F1.05b measures how often it
happens.

### 6. Structure and costs

The map derives **segments** (constant or gradual) with their start position and their start time
(exact, cumulated). Position → time and time → position are a binary search on either column
followed by O(1) arithmetic: **O(log n)** in the number of tempo changes. Lookups take `&self` and
never allocate (allocation-counter test); edits rebuild the table in O(n log n) and allocate,
editing path only.

### 7. Types introduced

`Tempo`, `SampleRate` (non-zero `u32`), `FrameRate` (positive `Rational`), and `Seconds`, a
newtype over `Rational` for **exact** times (an approximate `f64` is only ever an output). The
rounding functions of §5 for constant segments (seconds ⇄ sample, seconds ⇄ frame) belong to these
types; the tempo map adds the gradual case.

## Alternatives rejected

- **A, tempo linear in score position:** the most common intuition, but its durations are
  logarithms, so every time after the first *rit.* would be inexact and drift would have to be
  managed; it was rated lower in the experiment above.
- **C, beat period linear in score position:** exact, but the slowing is front-loaded, the opposite
  of the preferred shapes.
- **q = 3 (constant braking power):** as well rated as B, but its durations involve cube roots,
  so nothing after it would be exact. A curvature parameter could be added later as an option of
  a gradual change, with a new ADR, if users ask for it.
- **Tempo map on the unrolled timeline:** would duplicate tempo data for every pass of a repeat
  (ADR-0003 forbids duplicated data) and would make `core-time` know about the form.
- **Seconds as `f64` everywhere:** drift and fragile equality (ADR-0005).
- **Rounding samples down (`⌊t·rate⌋`):** a systematic delay of half a sample on average; rounding
  to nearest is unbiased.

## Consequences

- Every time on a tempo-change boundary is exact, so two tracks never drift apart, even after an
  hour full of ritardandi.
- The sample index of a note is exact and deterministic on every platform (no dependence on the
  floating-point unit), except in the overflow fallback of §5, which is measured and documented.
- Recording and the DAW view can convert a time back to a position exactly.
- `orchestra-playback` owns the unrolled clock and the execution offsets (fermatas, swing,
  rubato).
- The tempo map's source data become part of the project file format (F1.15).
