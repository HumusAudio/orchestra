# ADR-0015 — The rational type: own `i64` fraction, exact `i128` intermediates, typed overflow

- **Status:** Accepted (owner's decision, 2026-10-30; SCALETTA F1.01, decision D02)
- **Date:** 2026-10-30
- **Completes:** ADR-0005 §1 and §6 (representation and overflow policy of musical time)

## Context

ADR-0005 makes musical time an exact rational number of whole notes, kept in lowest terms, and
postpones the overflow policy to Phase 1: "64-bit numerator/denominator first, checked
operations, typed error on overflow, big integers only if measurements show the need". This ADR
fixes the concrete type that every other crate will use for positions, durations and offsets.
Changing it later would touch the whole code base and the file format, so it is decided before
any code depends on it.

Constraints that the type must satisfy, all coming from Accepted ADRs and CLAUDE.md:

- **Exactness** (ADR-0005): no rounding anywhere in the model; equality and ordering are exact.
- **Real time** (ADR-0007): the audio thread converts positions to samples, so arithmetic and
  comparisons must **never allocate**, never lock and run in bounded time.
- **No panics in library code** (CLAUDE.md §4), and `core-time` denies
  `clippy::arithmetic_side_effects` and `clippy::indexing_slicing`: every overflow must be
  detected and reported as a typed error, never wrapped silently nor turned into a panic.
- **Dependencies** (CLAUDE.md §5, §10): GPL-compatible, maintained, small; each one is weighed
  against binary size and compile time.

### How big do numbers get in real music?

Positions are fractions of a whole note. A note's denominator is a power of two times the product
of the enclosing tuplet ratios: a 7:4 tuplet inside a 5:4 inside a 3:2 inside sixteenths gives a
denominator of at most 16·3·5·7 = 1680. Sums of positions keep denominators bounded by the **least
common multiple** of all denominators in the piece: even a piece using every tuplet from 3 to 13
plus 1/1024 durations has an LCM of 1024·9·5·7·11·13 ≈ 4.6·10⁷. Numerators are bounded by the
length of the piece times that LCM: one hour of music at 60 BPM in 4/4 is 900 whole notes, so
900·4.6·10⁷ ≈ 4.2·10¹⁰. A signed 64-bit integer holds 9.2·10¹⁸: about **eight orders of magnitude
of headroom** for extreme-but-plausible scores. Products of two such values (cross
multiplication, intermediate sums) need up to ~126 bits, which fits in `i128`.

So `i64` storage is enough for musical time, provided that **intermediate** results are computed
in `i128` and only the final, reduced result has to fit in `i64`.

## Candidates examined (checked on crates.io and in the sources on 2026-10-30)

| Crate | Version / last release | License | Storage | Notes |
|---|---|---|---|---|
| `num-rational` | 0.4.2 / 2024-05-08 | MIT OR Apache-2.0 | generic `Ratio<T>`, e.g. `i64` | Small (19 KB, deps `num-integer`, `num-traits`). `new` and `reduce` **panic** on a zero denominator; `Add`/`Mul` operators overflow (panic in debug, wrap in release); `checked_add` returns `Option` and fails whenever an **intermediate** `i64` product overflows, even if the reduced result would fit; negating a numerator of `i64::MIN` overflows; `Ord` is a recursive continued-fraction comparison (panic-free but slower than one `i128` cross product). Low activity (repository pushed 2026-07, 41 open issues). |
| `fraction` | 0.17.0 / 2026-08-22 | MIT OR Apache-2.0 | `GenericFraction<T>` with sign, NaN and ±infinity | Larger (118 KB) and depends on the whole `num` umbrella crate. NaN/infinity semantics are wrong for musical time: an invalid position must be an error, not a value that propagates. |
| `malachite-q` | 0.14.0 / 2026-10-13 | LGPL-3.0-only | arbitrary precision (heap) | Excellent and maintained, but every value may allocate: unusable on the audio thread. Large (627 KB + `malachite-nz`, `malachite-base`). |
| `dashu-ratio` | 0.6.1 / 2026-09-25 | MIT OR Apache-2.0 | arbitrary precision (heap) | Same problem: heap allocation. 73 KB plus `dashu-int`, `dashu-base`. |
| `rug` | — | LGPL | GMP (C) | Excluded a priori: native C dependency (ADR-0001) and allocation. |

None of them offers the combination we need: fixed size, no allocation, exact `i128`
intermediates, typed errors, no panic path. The arbitrary-precision crates remain a possible
**test oracle** (dev-dependency only, decision D03 in F1.02), never a runtime dependency.

## Decision

1. **Own type.** `orchestra-core-time` defines `Rational`, a small value type written by us. No
   external dependency is added for it.

2. **Representation.** Two private fields `num: i64` and `den: i64` (16 bytes, `Copy`), with these
   invariants established by every constructor and every operation:
   - `den ≥ 1` (the sign lives in the numerator);
   - `gcd(|num|, den) = 1` (lowest terms); zero is exactly `0/1`;
   - `num ≠ i64::MIN` (so `|num| ≤ i64::MAX`): negation and absolute value can never overflow,
     and `Neg` is infallible.

   Because the representation is canonical, `Eq` and `Hash` are the derived ones (field by
   field) and are correct. Fields are private: a non-reduced value cannot be built from outside.

3. **Signed.** The base type is signed because offsets (microtiming, ADR-0004) and differences
   between positions can be negative. Domain types with fewer allowed operations (e.g. a position
   minus a position is a duration; a position plus a position does not compile) are thin newtypes
   over `Rational`, designed in F1.03/F1.04; this ADR only fixes the arithmetic base.

4. **Exact arithmetic with `i128` intermediates.** Every operation computes in `i128`, reduces,
   and only then narrows to `i64`:
   - addition/subtraction: `a/b ± c/d` with `g = gcd(b, d)`:
     `(a·(d/g) ± c·(b/g)) / ((b/g)·d)`, then reduce by the gcd of the result;
   - multiplication: cross-reduce first (`gcd(a, d)`, `gcd(c, b)`), then multiply;
   - division: multiplication by the reciprocal; dividing by zero is an error.

   An operation fails **only if the reduced result does not fit** in the invariants above. This is
   strictly more permissive than `num-rational`, which fails on intermediate overflow.

5. **Comparisons are exact and infallible.** `a/b` vs `c/d` compares `a·d` with `c·b` in `i128`:
   each product is below 2¹²⁶ in absolute value, so it cannot overflow. `Ord` is O(1), with a
   fast path for equal denominators.

6. **Overflow policy: typed error, never silent.** Fallible operations are methods
   `checked_add`, `checked_sub`, `checked_mul`, `checked_div` (and constructors such as `new`)
   returning `Result<Rational, RationalError>`, where `RationalError` distinguishes at least
   `Overflow` and `DivisionByZero` (zero denominator). `std::ops::{Add, Sub, Mul, Div}` are
   **not** implemented, because they could only panic, wrap or saturate. Wrapping and saturating
   are rejected outright: a saturated position is a silently wrong position. Infallible
   operations (`Neg`, `abs`, `signum`, `is_zero`, comparisons, `floor`/`ceil` towards integers)
   are plain methods or traits.

7. **No big integers for now.** The analysis above gives about eight orders of magnitude of
   headroom. If a real file ever overflows, the user gets a clear error (the editing command
   fails and is not applied; ADR-0003 commands are atomic), not corruption. A fallback to
   arbitrary precision would need a new ADR backed by such cases; the private representation
   keeps that door open without changing the public API.

8. **Leaving exact arithmetic.** Conversion to seconds, samples and frames is F1.05's job (with
   `i128` intermediates and the documented rounding rule of ADR-0005 §3). An approximate `to_f64`
   exists only for display and drawing; nothing converted to a float ever flows back into the
   model.

9. **Performance.** No allocation in any operation (verified by the allocation counter test of
   F1.02). Greatest common divisor with the binary (Stein) algorithm, O(log n) in the magnitude of
   the operands; fast paths for equal denominators and integer values. These choices are
   **reasoned, not yet measured**: F1.03 adds benchmarks (instruction counts) for add, mul, cmp and
   reduction, and if `i128` gcd proves too slow, the first remedy is an `i64` fast path when the
   intermediate fits, not a change of representation.

10. **Text form.** The canonical text form (`"3/8"`, `"-1/4"`, `"2"` when `den = 1`) and its use in
    the project file are fixed in F1.15 (ADR-0010).

## Alternatives rejected

- **`num-rational`:** the closest candidate, but panics on construction, operators that overflow,
  `Option` instead of a typed error, failures on intermediate overflow and the `i64::MIN` edge
  case would all need a wrapper around it, at which point the crate brings little beyond `gcd`.
- **`fraction`:** NaN and infinity are not valid musical times; bigger dependency tree.
- **Arbitrary precision (`malachite-q`, `dashu-ratio`, `rug`):** heap allocation forbids them on
  the audio thread; size and compile time; `rug` is a C dependency.
- **`i128` storage:** 32 bytes per value, slower everywhere, and it only postpones the same
  overflow question; `i64` storage with `i128` intermediates gives the same safety where it matters.
- **Unsigned denominator (`u64`):** one extra bit of range, at the cost of mixed-sign arithmetic in
  every operation; not worth it given the headroom.
- **Operators returning `Result`** (`impl Add<Output = Result<…>>`): unusual, chains poorly
  (`(a + b)? + c`), and surprising to readers.
- **Saturating or wrapping arithmetic:** silently wrong results.
- **Ticks** were already rejected by ADR-0005.

## Consequences

- We own and maintain a few hundred lines of arithmetic; they must be covered by property tests
  (against an exact oracle) and by the edge cases above (`i64::MAX`, zero, negative values,
  results that fit only after reduction).
- Code that does arithmetic on time is more verbose (`a.checked_add(b)?`); domain newtypes and
  helper methods on cumulative tables keep the hot paths readable.
- Overflow becomes a normal, testable error path of every command that edits time.
- Out of scope, noted for F1.05: integrating a gradual tempo change generally yields a logarithm,
  so seconds inside such segments are **not** rational; exactness there ends at the analytic
  formula, before the final rounding to samples. This refines, and does not contradict, ADR-0005.
