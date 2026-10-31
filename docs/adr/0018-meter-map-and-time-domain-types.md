# ADR-0018 — Meter map and time domain types: `Position`/`Duration`, meters, irregular and free bars

- **Status:** Accepted (owner's decision, 2026-10-31; SCALETTA F1.04, decision D04). The choices
  on D04, signed duration, 8 additive groups and basic bar numbering were approved before the text
  was written; free bars (meter suspension, §3), with an explicit length and counted by default,
  were requested by the owner on reviewing the draft and added before acceptance.
- **Amended by:** ADR-0019 (§2, beat grouping separate from the written meter; beats of a bar
  longer than its meter)
- **Date:** 2026-10-31
- **Completes:** ADR-0005 §2 (the meter map) and ADR-0015 §3 (the domain newtypes)

## Context

ADR-0005 gives `orchestra-core-time` the single tempo map **and the single meter map** of a
project; ADR-0015 fixed the arithmetic base, `Rational`, and left the domain newtypes ("a
position minus a position is a duration; a position plus a position does not compile") to
F1.03/F1.04. F1.03 implemented only the base type, so both questions are open now. Every later
crate (model, playback, layout, file format) will speak in these types, so they are fixed in an
ADR before any code depends on them.

What the meter map has to answer, from the SCALETTA F1.04 cases:

- **bar ⇄ position**: where bar *k* starts; which bar contains a position, and where inside it;
- **beat ⇄ position** (in Italian *movimento*): which beat of the bar, and where inside the beat,
  for display (bar:beat counters), the metronome (F3) and beaming defaults (F2B);
- pickup bars (anacrusis), meter changes, compound meters (6/8, 9/8, 12/8), additive meters
  (3+2+2/8), meters whose denominator is not a power of two (4/3, 3/10: D04), free cadenzas where
  the meter is **suspended** (owner's request on the draft), and bar numbers with exceptions;
- all of this with **O(log n)** lookups in the number of meter changes (ADR-0005 §4, ADR-0007),
  without allocating, because the audio thread will need bar/beat positions for the metronome.

What the meter map must **not** do: know about repeats and jumps. It describes the *written*
score, bar after bar, as printed; unrolling the form into what is played is `orchestra-playback`'s
job (ADR-0004). Nor does it know about notes or staves: the question "does a note store an
absolute position or a position inside its bar?" belongs to the model architecture (F1.06), and
the map is designed to serve either answer.

## Decision

### 1. Two domain types over `Rational`: `Position` and `Duration`

Both are `Copy` newtypes of 16 bytes over `Rational`, in whole notes, defined in `core-time`:

- **`Position`**: a point on the written timeline, measured from the start of the score. Zero is
  the start of bar 0, which is the pickup bar when there is one. The type itself accepts any
  value, negative included: a note played early by a microtiming offset sounds before zero, and
  intermediate results of calculations may lie outside the score. Ranges are checked where they
  matter (the meter map rejects positions before zero, §6).
- **`Duration`**: the distance between two positions, **signed** (owner's choice): `a − b` is
  negative when `b` comes later, and microtiming offsets (ADR-0004) are negative when early. The
  rule "a note lasts more than zero" is a model invariant, checked there, not a property of the
  type.

Allowed operations, all `const fn` and checked (ADR-0015 §6: no operators that could panic,
wrap or saturate):

| Operation | Result |
|---|---|
| `Position::checked_add(Duration)`, `Position::checked_sub(Duration)` | `Position` |
| `Position::checked_duration_since(Position)` (later − earlier, like `Instant::duration_since`) | `Duration` |
| `Duration::checked_add(Duration)`, `Duration::checked_sub(Duration)` | `Duration` |
| `Duration::checked_mul(Rational)`, `Duration::checked_div(Rational)` (scale) | `Duration` |
| `Duration::checked_ratio(Duration)` (how many times one fits in the other) | `Rational` |
| `Neg`, `abs`, `signum`, `is_zero`, `is_positive`, `is_negative` on `Duration` | `Duration` / `bool` |

There is no way to add two positions, nor to multiply a position: those expressions do not
compile (checked by `compile_fail` doctests). The errors are `RationalError`, unchanged. The
conversion to and from `Rational` is explicit (`from_rational`, `as_rational`): it is the escape
hatch for the conversions of F1.05 and for the file format, and its presence in a diff is the
signal that a value leaves the type safety.

`Debug` writes `Position(3/8)` / `Duration(-1/4)`; there is no `Display` until F1.15, like
`Rational`.

### 2. `Meter`: a time signature, additive groups in a fixed array

A `Meter` is a list of 1 to **8** numerator groups (owner's choice) and one denominator:
`4/4` is `[4]/4`, `6/8` is `[6]/8`, `3+2+2/8` is `[3, 2, 2]/8`.

- Numerator groups are `u8` in `1..=255`, the denominator a `u16` in `1..=65535`. Any denominator
  is accepted, **not only powers of two** (decision D04, owner, 2026-10-31): with `Rational`,
  `4/3` costs nothing here. Their engraving (F2B) and their export to MIDI files, whose time
  signature message only encodes powers of two (F1.18), are dealt with in those steps.
- The type is `Copy`, about 12 bytes, never allocates: the 8 groups live in an array plus a
  length. A `Vec` would allocate for every meter and make `Meter` non-`Copy` for no real benefit:
  no realistic additive meter has more than 8 groups.
- The **nominal bar length** is `(sum of groups) / denominator` whole notes; with at most
  8 × 255 = 2040 in the numerator it always fits.
- **Beats** are derived, not stored:
  - additive meter (2 or more groups): one beat per group, of `group/denominator`
    (3+2+2/8 has three beats: 3/8, 2/8, 2/8);
  - single group `n` that is a multiple of 3 and greater than 3 (6, 9, 12, 15…): **compound**,
    `n/3` beats of `3/denominator` (6/8 has two dotted-quarter beats, 6/4 two dotted halves);
  - any other single group: **simple**, `n` beats of `1/denominator` (3/4, 4/4, 3/8, 5/4, 4/3).

  A user who wants 6/8 counted in six can only write it today as the additive
  `[1, 1, 1, 1, 1, 1]/8`, which would also *display* differently. Separating "how it is displayed" from "how it is counted"
  and beaming groups are notation concerns (F2B) and may add a field later, in a migration.

Invalid meters (no groups, more than 8, a zero group, a zero denominator) are a typed
`MeterError`; there is no panic path.

### 3. Bars are indexed from 0; irregular bars have their own length

- **`BarIndex`** (a `u32` newtype) is the internal, contiguous index of a written bar: 0, 1, 2…
  It is never shown to the user; displayed bar numbers are a separate notion (§5). `u32` allows
  about 4·10⁹ bars, far beyond any score.
- Every bar has the meter in force at that bar and, normally, the nominal length of that meter.
  An **irregular bar** overrides the length of one bar with any positive `Duration`: a pickup
  bar, the incomplete bar before a repeat sign, a bar longer than its meter, a free cadenza. One
  mechanism covers all these cases, instead of a special case for the anacrusis.
- An irregular bar also has a **kind**, which decides how its beats are counted:
  - `AlignStart` (default): the bar is the *beginning* of a nominal bar, beat 1 at its start
    (incomplete bar before a repeat; a bar with a few beats more than its meter);
  - `AlignEnd`: the bar is the *end* of a nominal bar, so a quarter-note pickup in 4/4 is
    **beat 4**, as musicians count it. Only allowed when the actual length does not exceed the
    nominal one (otherwise beat 1 would lie before the bar); typically used for bar 0;
  - `Free`: the meter is **suspended** for this bar (a free cadenza, *senza misura*). The bar has
    no beats at all: a beat lookup inside it answers explicitly "no beat" (an `Option`), never an
    invented value. No time signature is in force for display, and the metronome (F3) is silent.
    The meter of the map stays recorded and **resumes** at the next regular bar without a new
    meter change. A free passage may span several consecutive free bars (a long cadenza divided
    by dashed barlines), each with its own length.
- The length of every irregular bar, free ones included, is **explicit source data** of the map
  (owner's choice), not derived from the notes it contains. It is the position of the next
  barline, a fact of its own; deriving it from the content would make the map depend on the model
  and duplicate data (CLAUDE.md §1). When notes are entered beyond the end of a free bar, the
  same editing command lengthens the bar, atomically and with a single undo (ADR-0003); those
  commands belong to the model (F1.06 and following).

### 4. The map: meter changes and irregular bars in, cumulative segments out

The meter map is a value with two kinds of **source data**, the ones the document stores and
commands edit (ADR-0003):

- meter changes `(BarIndex, Meter)`, sorted by bar, the first one at bar 0 (so a meter is always
  in force);
- irregular bars `(BarIndex, Duration, kind)`, sorted by bar.

From them it derives a sorted vector of **segments**: maximal runs of consecutive bars with the
same meter and the same actual length. Each segment stores its first bar, its start `Position`
(cumulated) and its bar length. An irregular bar is a segment of one bar; the last segment
continues **forever**, so the map never "runs out of bars": where the score ends is a model
fact, not a meter fact. The number of segments is at most twice the number of changes and
irregular bars, plus one.

Operations and costs (`n` = number of segments):

| Operation | How | Cost |
|---|---|---|
| start of bar `k` | binary search on the first bar of the segments, then `start + (k − first)·length` | O(log n) |
| position → (bar, offset in bar) | binary search on the start positions, then `floor((pos − start) / length)` | O(log n) |
| (bar, offset) → (beat, offset in beat), and back; "no beat" in a free bar | walk the at most 8 groups of the meter | O(1), bounded |
| editing the source data (add/remove a meter change or an irregular bar) | rebuild the segments and their cumulated starts | O(n) and allocates: commands only, never on the audio thread |

No remainder operation (`rem`) is added to `Rational`: `checked_div` + `floor` + `checked_mul`
already give "which bar and where", and a benchmark will tell if a dedicated operation is worth
it. Lookups never allocate (allocation counter test, ADR-0016) and are benchmarked in batches
(ADR-0017 §4).

The O(n) rebuild is acceptable because `n` is the number of meter changes and irregular bars —
tens, rarely hundreds — not the number of bars or notes. If a measurement ever shows it on the
editing path, the cumulated starts can be patched from the edited segment onwards only.

### 5. Displayed bar numbers

The displayed number of a bar is derived from the `BarIndex` through **numbering rules**, also
source data of the document:

- by default bar 0 is numbered **0 if it is an irregular `AlignEnd` bar** (a pickup), otherwise
  1, and every following bar adds one; **free bars are counted** like any other bar (owner's
  choice: the usual practice for concerto cadenzas), unless marked uncounted;
- a bar can be marked **uncounted**: it has no number, and the following bar continues the
  count (a free cadenza in editions that do not number it, a bar split in two by a line
  break);
- the count can **restart** at a given bar with a given number (a new movement in the same file,
  or editorial numbering).

The lookup is O(log n) in the number of rules: the last restart at or before the bar (binary
search), minus the uncounted bars between the two (`partition_point` on the sorted list).
Numbering tied to the form — the bars of a second volta that repeat the numbers of the first,
"12a" style suffixes — is left to the form (F1.06 and following), which knows about voltas.

### 6. Errors

`MeterError` for invalid meters (§2); `MeterMapError` for the map, with at least: an operation
overflowed (`RationalError` inside), a position before the start of the score, an irregular bar
with a length that is not positive, an `AlignEnd` bar longer than its meter, a source list
without a meter at bar 0. English `Display` for developers, like `RationalError`; the interface
maps variants to Fluent keys.

## Alternatives rejected

- **One newtype `Time` for everything, or plain `Rational` everywhere:** the type checker would
  not catch `position + position`, which is meaningless and a classic source of off-by-a-bar
  bugs.
- **Unsigned `Duration`:** `a − b` would need a `Result` or a separate signed "offset" type, and
  microtiming offsets would need a third type; one signed duration is simpler.
- **A separate `Offset` type for microtiming:** the operations are the same as for a duration;
  the distinction is a meaning, not a different algebra.
- **`Vec` of groups in `Meter`:** allocation and no `Copy`, for meters with more than 8 groups that
  do not exist in practice.
- **Only powers of two as denominators:** a check more, not a cost less; it would close a door
  that contemporary music uses (D04).
- **A special "pickup" flag instead of irregular bars:** the same need (a bar shorter or longer
  than its meter) appears before repeats and in cadenzas; one mechanism is simpler and more
  general.
- **Free bar length derived from its notes:** the map would depend on the content of the model
  (an upward dependency from `core-time`) and the barline position would be duplicated data
  that must be kept in sync; an explicit length, adjusted by the same command that edits the
  cadenza, keeps one source of truth.
- **Storing every bar explicitly (one entry per bar):** O(bars) memory and O(bars) work for a
  meter change at the start of a long score, while segments keep both proportional to the number
  of changes.
- **A balanced tree or a Fenwick tree instead of a sorted vector:** O(log n) edits, but `n` is
  small and edits are rare; a sorted vector is simpler, contiguous in memory and as fast to
  search.

## Consequences

- Code dealing with time states what it means (`Position` vs `Duration`) and the compiler checks
  it; F1.05 (tempo map) and the model use these types from the start.
- One meter map per project (ADR-0005): different meters in different staves *at the same time*
  (polymeter, e.g. 3/4 against 6/8 with shared barlines) can only be a display matter of the
  staff (F2B); truly independent barlines per staff are out of scope and would need a new ADR.
- Free cadenzas and *senza misura* passages are `Free` irregular bars, counted by default.
  Consequences noted for later steps: inside them the tempo map (F1.05) still gives a nominal
  tempo, and the real rubato of a performance is made of execution offsets (ADR-0004); the
  metronome (F3) is silent; whether the time signature is shown again when the meter resumes is
  an engraving choice (F2B).
- The meter map's source data become part of the project file format (F1.15).
- The question of absolute vs bar-relative note positions stays open for F1.06; the map offers
  both directions so that either answer works.
