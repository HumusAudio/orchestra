# ADR-0019 — Beat grouping: how a bar is counted, separate from how its meter is written

- **Status:** Proposed (SCALETTA F1.04b; content approved by the owner on 2026-10-31 before the
  text was written: separate grouping with its own unit, per section and per bar, up to 16
  groups, default rule unchanged)
- **Date:** 2026-10-31
- **Amends:** ADR-0018 §2 ("beats are derived, not stored"); the rest of ADR-0018 is unchanged

## Context

ADR-0018 §2 derives the beats of a bar from its meter with a fixed rule (additive → one beat per
group; single numerator multiple of 3 and greater than 3 → compound; otherwise simple) and left
"how it is displayed" versus "how it is counted" to a later field. Reviewing F1.04b, the owner
asked for that field now: a 9/8 is usually counted 3+3+3, but may be thought of as 3+2+2+2
without being *written* as an additive meter, and the user must be able to change the grouping
of single bars or of the whole piece.

ADR-0018 also left open how beats are counted in an irregular bar **longer** than its meter; the
owner chose (2026-10-31) that the beat pattern starts again and the beat numbers keep going
(option "a"): a 5/4 bar in 4/4 has a fifth quarter-note beat.

## Decision

1. **Written meter and beat grouping are distinct.** The `Meter` of ADR-0018 §2 stays what is
   *written* (9/8 is `[9]/8`; the written mixed meter is `3+2+2+2/8`). A **beat grouping** is an
   optional list of beat lengths: 9/8 written "9/8" and counted 3+2+2+2 is the meter `[9]/8` with
   the grouping `[3, 2, 2, 2]/8`. It changes the counting (beats, metronome in F3, default
   beaming in F2B), never the time signature that is drawn.
2. **The grouping has its own unit**, not necessarily the meter's denominator: `[3, 3]/8` counts a
   3/4 bar in two dotted quarters (hemiola), which cannot be expressed in quarter notes. Its only
   constraint is that its total, `(sum of groups) / unit`, equals the **nominal** length of the
   bar's meter exactly; otherwise the edit fails with a typed error.
3. **Where it applies.**
   - **To a section:** a grouping can be attached to a meter change and is valid until the next
     meter change. "The whole piece" is the meter change at bar 0. Several sections with the same
     meter each carry their own grouping; an "apply to every 9/8" action is a matter for the
     interface (a command that edits several meter changes at once).
   - **To one bar:** a bar can have its own grouping, which wins over the section's. It is a
     further kind of source data of the meter map, found with a binary search (O(log n)).
4. **Irregular bars** follow ADR-0018 §3 with the grouping in force instead of the default one:
   - `AlignStart` longer than its meter: the pattern starts again and beat numbers continue
     (owner's option "a"); the last beat is cut short by the barline if it does not fit;
   - `AlignEnd`: counted from the virtual start of the nominal bar, as for a pickup;
   - `Free`: no beats; a per-bar grouping on a free bar is rejected with a typed error.
5. **Up to 16 groups** in a grouping (owner's choice), against the 8 of a written meter, so that a
   12/8 can be counted "in twelve". Still a fixed array: `Copy`, no allocation.
6. **The default rule is unchanged** (owner's choice, 2026-10-31): without an explicit grouping, a
   single-numerator meter that is not compound is **simple**, so `[7]/8` counts seven eighths and
   `[5]/8` five. Other defaults were considered (groups of 2 with the 3 last, 2+2+3; or first,
   3+2+2) and rejected, because no convention is universal; the interface asks instead (see
   Consequences). Internally the default needs no array: simple and compound meters are a
   uniform pattern (count and beat length), so a 17/16 with 17 beats does not hit the limit of 16
   groups.

## Alternatives rejected

- **Writing the counting into the meter only** (users who want 3+2+2+2 write an additive meter):
  it changes the drawn time signature, which the user did not ask for.
- **Grouping in units of the meter's denominator:** cannot express hemiola (3/4 as 3+3 eighths).
- **One grouping per meter value for the whole piece** ("every 9/8 counts 3+2+2+2"): convenient,
  but a hidden global rule that would surprise in a piece where two 9/8 sections are counted
  differently; the same effect is a command over several meter changes.
- **A smarter default rule for odd meters** (5/8 → 2+3, 7/8 → 2+2+3): no universal convention;
  asking the user once is better than guessing silently.

## Consequences

- The meter map gains two kinds of source data (the grouping on a meter change, the per-bar
  grouping), validated on every edit together with the rest: an edit that would leave any bar
  inconsistent (e.g. a meter change that makes an existing per-bar grouping the wrong length) is
  rejected as a whole, and the map is unchanged.
- **Interface requirement (owner, 2026-10-31), for the phase that builds the meter dialog:** when
  the user enters a mixed meter (an odd single numerator that is not compound, such as 5/8, 7/8,
  11/8), the interface must **ask at once** which grouping to use, offering the options
  explicitly — for 5/8: 2+3, 3+2 or keep 5; for 7/8: 2+2+3, 3+2+2, 2+3+2 or keep 7; and so on —
  and remind the user that the grouping can later be changed bar by bar.
- The project file format (F1.15) stores both groupings.
