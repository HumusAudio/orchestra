# ADR-0028 — Moving a note from the DAW side: tolerance threshold, performance-only edits, pass-specific offsets

- **Status:** Accepted (owner's decision, 2026-11-06; SCALETTA F1.14, decision D09). The owner
  approved the recommendations Q1–Q10 and asked that, beside the default rule, the user can
  **consciously** choose to change only the performance, with offsets of any size that never touch
  the score (§4), and that every DAW edit has such a pair of behaviours (§7). For velocity the
  owner chose the **reverse**: by default a DAW edit stays in the performance, and the conscious
  choice makes it reach the written dynamics.
- **Date:** 2026-11-06
- **Refines:** ADR-0004 (tolerance threshold; pass-specific offsets), ADR-0024 §10 (performance
  offsets; the pass-specific table), ADR-0025 §12 P6 (the table's key), ADR-0027 §8 (where the
  offsets are added).
- **Amends:** nothing.

## Context

ADR-0004 fixes the rule: a note moved in the DAW view within **±tolerance** of its written
position keeps the score unchanged and becomes a performance offset; beyond it, the written
position changes, quantised to the active grid, and the residual becomes the new offset. The
tolerance is configurable per project and per track, in rational musical time (proposed default:
a 1/32 note) and optionally in milliseconds; "the same rule applies to duration changes".
ADR-0004 also says that a note edited in one pass of a repeat edits the written note, unless the
user explicitly creates a pass-specific offset.

What already exists:

- `PerformanceOffsets { onset, length, velocity }` inline on every notehead (ADR-0024 §10), with
  the command `EventChange::Offsets`; offsets are stored unbounded and bounded by the playback
  rules (ADR-0027 §8).
- `EventChange::Move`, which moves an event inside its voice and never splits it (ADR-0026 W1:
  past the barline the bar becomes overfull); an event moved over another one is refused by the
  local check (V2).
- The path of F1.12, with the pass index of every played bar (ADR-0025 §12).

What is missing is the threshold as data, the table of pass-specific offsets and the command that
decides between "offset" and "written move".

### Sources checked (2026-11-06)

No notation program or DAW stores the score and the performance as one model with a threshold
between them, so there is no direct precedent. The closest concept is the "safe zone" of DAW
quantisation, which leaves alone notes close to the grid to keep the human feel:

- **Logic Pro, Q-Range** ([Mac guide 10.4](https://help.apple.com/logicpro/mac/10.4/en.lproj/lgcp35029a2b.html),
  [iPad guide](https://support.apple.com/guide/logicpro-ipad/lpip70c8d20d/ipados)): at 0 every
  note is quantised; with a range, notes near a grid position are not moved (the two guides
  disagree on the sign of the value).
- **Cubase/Nuendo, Non Quantize** ([Steinberg help](https://steinberg.help/nuendo/v8/en/cubase_nuendo/topics/quantizing_midi_audio/quantizing_quantizing_panel_options_musical_grid_r.html),
  [Sound On Sound](https://www.soundonsound.com/techniques/introduction-cubase-part-3-quantisation-master-track-synchronisation)):
  a distance in ticks around each grid position inside which events are not quantised, "so slight
  variations are kept"; the help's example is 120 ticks, a 1/16 note.

Both apply the zone around the **grid**; ours is around the **written position**, which is
already on a grid of the score. Both express it in musical time, as ADR-0004 does.

## Decision

### 1. The threshold as data (Q2, Q3)

```text
Tolerance = Musical(Duration)   // > 0, a fraction of a whole note
          | Millis(u32)         // > 0, converted through the tempo map at the note's written start
ToleranceSettings { project: Tolerance, per_track: [(TrackId, Tolerance)] }   // sorted by TrackId
```

- **Default: `Musical(1/32)`** for a new project (D09), no per-track override.
- The settings are **in the document**, saved in the file (F1.15 reserves them), and changed by an
  undoable command `SetTolerance { track: Option<TrackId>, tolerance: Option<Tolerance> }`
  (`None` track = the project value, which cannot be removed; `None` tolerance on a track removes
  its override). Deleting a track removes its override.
- `Millis` converts at the written start of the note: `Musical` equivalent = the musical length
  that lasts that many milliseconds from there, through the one tempo map (ADR-0005). The
  comparison is exact: the milliseconds become an exact time, the note's offset is converted to
  time and compared, never rounded to floats.

### 2. What the threshold measures (Q1)

The command receives the **onset offset the user wants** for the note (`onset: Duration`, from
its written start: the DAW side computes it from the played timeline, removing what the playback
rules add, such as arpeggios and acciaccaturas). `core-model` never needs `playback`.

- `|onset| ≤ tolerance` (inclusive): the new value becomes the onset offset of the note; the score
  does not change.
- Beyond: the written position changes (§3), and the residual becomes the new onset offset.

### 3. The written move, beyond the threshold (Q4–Q7, Q9)

- The **grid** is a positive `Duration` passed by the caller (the DAW's active grid), counted
  **from the start of the bar** in which the new position falls; there is no "no grid".
- New written position = the grid point nearest to `written start + onset` (ties away from the
  written position, so a move is never undone by rounding toward it); residual = `written start +
  onset − new written position`. If the grid brings the note back to its own position, only the
  residual is stored (no special case).
- It moves the **whole event** (every notehead of a chord; its grace notes follow, as with
  `Move`). A move asked for **one notehead of a chord** with more than one notehead is always an
  offset, of any size (Q5): splitting a chord is an editorial choice the command does not make.
- **Tie chains** move as a whole (every event of the chain by the same amount), only if every
  piece stays inside its own bar (Q7).
- The move is **refused** with a typed error, and nothing changes, when the new position
  overlaps another event of the voice (V2), when a piece would cross a barline or reach past the
  last bar (Q6: unlike `Move`, a DAW move never makes a bar overfull), or when the note is a grace
  note (Q9: grace notes are moved only by offsets, their place belongs to the main event). The
  interface (F3) will offer "keep it as an offset" or "move anyway" (the latter is a future
  composed command that splits into ties, F1.14c).

### 4. Performance-only edits: the conscious choice (owner, 2026-11-06)

Beside the default rule, the user can choose to change **only the performance**: then the move is
an offset of any size and the score never changes. It is an explicit choice, never a silent
fallback. The command carries the mode:

```text
MovePlayed { note: NoteId, pass: Option<u32>, onset: Duration, grid: Duration, mode: MoveMode }
MoveMode = ByThreshold      // the default rule of §2–§3
         | PerformanceOnly  // always an offset, no threshold
```

The DAW view will expose it as a separate command (and a modifier during a drag, F3); how it is
shown (key, menu entry, a "free performance" toggle per track) is for the interface steps. The
bounds of ADR-0027 §8 still apply to what is played.

### 5. Pass-specific offsets (Q8)

```text
PassOffsets: per track, sorted map (NoteId, pass: u32) → PerformanceOffsets
```

- Written by `MovePlayed` with `pass: Some(p)`: **always performance-only** (a written change would
  change every pass), whatever the mode. The key is the **first notehead of the tie chain**, as
  for the inline offsets (ADR-0024 §10); the played note's offsets are inline + pass entry,
  **added** before the bounds of ADR-0027 §8. With bar repeats (ADR-0027 §7) the key is the source
  note with the pass of the played bar.
- A primitive edit sets or clears one entry and returns its exact inverse; an entry equal to zero
  is removed, so the table holds only real deviations.
- **Deleting a note deletes its entries** (and the undo puts them back). When the form changes and
  a pass no longer exists, the entry **stays** and is ignored: deleting it would lose data on the
  undo of the form change.
- `ChangeSet` reports the notes whose pass offsets changed, so `Performance::update` recomputes
  only their played notes.

### 6. Durations (split, Q10)

The same rule for the length ("ADR-0004: the same rule applies to duration changes") needs written
values that follow the grid, often a tie chain (5/16 is not one value), with the rewriting rules
of F1.11c. It is **F1.14c**, with its own precisions; until then a length change from the DAW side
is a performance-only `length` offset.

### 7. The same pair for intonation and velocity (owner, 2026-11-06)

Every DAW edit of a note will have a **default rule** and a **conscious performance-only
choice**, never a silent mix:

- **Pitch / intonation:** by default a DAW pitch edit changes the written pitch, spelled from the
  key (ADR-0004, enharmonic row); the performance-only choice will be a pitch offset (detune,
  microtonal deviation) that leaves the written pitch alone. It needs a new offset field and comes
  with intonation (D22, after F3.04, ADR-0027 §10).
- **Velocity (reversed, owner):** by default a DAW velocity edit is a velocity offset (ADR-0024
  §10) and never touches the written dynamics, as today. The conscious choice will make it change
  the score too (the written dynamics, with rules to be designed with the DAW velocity editor,
  F3).

### 8. Tests and benchmarks

Hand-written cases for every row of §2–§5, a property test (random documents and random
`MovePlayed`: after each, `validate` passes; within the threshold the score is unchanged; the sum
"written start + offset" equals the asked position; undo restores the document), injected bugs,
and a benchmark of one `MovePlayed` inside a gesture on the heavy score (the drag of F3).

### 9. Choices approved by the owner (2026-11-06)

| ID | Question | Choice |
|---|---|---|
| Q1 | What the threshold measures | The onset offset that would be stored, given by the caller |
| Q2 | Default and units (D09) | 1/32 in musical time; the milliseconds variant in the type from the start |
| Q3 | Where it is stored | In the document: project value + per-track overrides, undoable command |
| Q4 | Grid | Passed by the caller, from the start of the bar, never absent; residual as the new offset |
| Q5 | One notehead of a chord | Always an offset |
| Q6 | Occupied space, barline, last bar | Refused with a typed error |
| Q7 | Tie chains | Move as a whole, only if every piece stays in its bar |
| Q8 | Pass-specific offsets | Always performance-only, added to the inline ones, deleted with the note, kept when a pass disappears |
| Q9 | Grace notes | Offsets only |
| Q10 | Split of the step | F1.14 (this ADR), F1.14b (onset moves, settings, pass table), F1.14c (durations) |
| Q11 | Velocity from the DAW (§7) | Default: performance only; conscious choice: the written dynamics change too |

## Alternatives rejected

- **Threshold on the distance from the written position** instead of the stored offset: counts
  the shift that arpeggios and acciaccaturas already add, so a rolled chord would cross the
  threshold by itself.
- **Silent fallback to an offset** when a written move is impossible: the score would stop
  following the user's gesture without saying so.
- **Splitting into ties at every step of a drag:** new IDs at every mouse movement, against W1.
- **Overfull bars from a DAW move:** valid in the model, but a DAW user never means it.
- **Deleting pass entries when the form changes:** the undo of the form change would lose them.
- **Threshold only in seconds:** the feel would not follow tempo edits (ADR-0024 C8).

## Consequences

- New stored data: the tolerance settings and the pass-offset table, both reserved by F1.15.
- `Performance::update` learns one more kind of change (pass offsets).
- The interface steps (F3) must expose the performance-only choice and the "move anyway" option.
- F1.14c will need a composed command that rewrites lengths and moves across barlines into ties.
