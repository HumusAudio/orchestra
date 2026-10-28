# ADR-0005 — Time representation: exact fractions, one tempo map, derived samples and frames

- **Status:** Accepted
- **Date:** 2026-10-28

## Context

Musical positions must line up exactly: a triplet of quintuplets must end precisely on the beat,
and two tracks must not drift apart after an hour. Floating-point positions accumulate rounding
errors and make equality tests unreliable. Audio, however, runs on integer sample indices and
video on frame numbers with rational frame rates (e.g. 30000/1001).

## Decision

1. **Musical time is an exact rational number** of whole notes (positions and durations), with
   integer numerator and denominator kept in lowest terms. No floats in the model.
2. There is **one tempo map** per project (and one meter map), owned by `orchestra-core-time`.
   Every conversion goes through it; no other component keeps its own clock.
3. **Seconds, sample indices and frame numbers are derived**, never stored as primary data,
   leaving exact arithmetic only at the last step:
   - musical time → seconds through the tempo map (constant segments exact; gradual tempo changes
     integrated analytically per segment);
   - seconds → sample index = `round(seconds × sample_rate)` with a documented rounding rule;
   - seconds → video frame through a **rational frame rate** (e.g. 24/1, 25/1, 30000/1001), never
     a float like 29.97.
4. Lookup in the tempo map is **O(log n)** in the number of tempo changes (binary search on a
   cumulative table), and the conversions used on the audio thread do not allocate.
5. Audio regions and video clips can be anchored either in musical time (they follow tempo
   changes) or in absolute time (they do not); the anchor kind is part of the model.
6. Overflow policy (64-bit numerator/denominator first, checked operations, typed error on
   overflow, big integers only if measurements show the need) is designed in Phase 1.

## Alternatives rejected

- **Ticks (PPQ integers, e.g. 960 per quarter):** cannot represent every tuplet exactly (a 7-tuplet
  inside a 5-tuplet needs a PPQ divisible by 35…); the choice of PPQ becomes a permanent
  compromise.
- **f64 seconds or beats:** rounding drift and fragile equality, exactly what we want to avoid.
- **Separate clocks for audio and video:** guarantees drift between them.

## Consequences

- Rational arithmetic costs more than floats; hot paths must be benchmarked (ADR-0007) and use
  cumulative tables to avoid repeated work.
- Equality and ordering of positions are exact, which simplifies layout and tests.
