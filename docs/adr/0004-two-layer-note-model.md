# ADR-0004 — Two-layer note model; tolerance threshold; unrolled timeline linked to the score

- **Status:** Accepted (the concrete Rust types are designed in Phase 1 and must respect this ADR)
- **Date:** 2026-10-28

## Context

A score says *what is meant*; a performance says *what is played*. A quarter note with a staccato
dot is written once but sounds shorter; a repeat is written once but played twice; a B-flat
clarinet's written D sounds as C. ADR-0003 requires one model serving both views, so the model must
hold both the intent and the deviations from it, without either one being a copy of the other.

## Decision

### The two layers

Every note has:

1. **Written intent** (authoritative, edited mainly from the notation view):
   pitch *with its spelling* (step, alteration, octave — not a MIDI number), rational written
   duration (ADR-0005), position in its measure and voice, tuplet membership, articulations,
   ornaments, dynamics and other markings that apply to it, tie/slur relations.
2. **Performance** = *derived* from the intent by interpretation rules (`orchestra-playback`)
   **plus explicit offsets** stored in the model: onset offset (microtiming), duration change,
   velocity change, per-note controller data. Offsets are deltas, not absolute copies, so that a
   change of intent (e.g. a new dynamic) still propagates to the performance.

The derived part is never stored as editable data; only the explicit offsets are.

### Tolerance threshold

When a note is moved in the DAW view:

- if the new onset is within **±tolerance** of its written position, the move is recorded as a
  **performance offset**: the score does not change;
- beyond the tolerance, the written position changes (quantised to the active grid) and the
  residual becomes the new offset.

The tolerance is **configurable** (per project, overridable per track), expressed in rational
musical time (default proposal: a 1/32 note, to be validated with users) and optionally in
milliseconds for tempo-independent behaviour. The same rule applies to duration changes.

### Unrolled timeline

The DAW view always shows the music **as played**: repeats, voltas, D.S./D.C., codas expanded into
a linear sequence of *passes*. Each pass segment keeps a link to the written range it comes from.
Editing a note in pass 2 of a repeat edits the written note (both passes change) unless the user
explicitly creates a pass-specific performance offset, which is stored with the pass index.

### Hard cases and their representation

| Case | Representation |
|---|---|
| **Enharmonic spelling** | Pitch stored as spelled (e.g. F♯4 vs G♭4). The DAW shows the sounding pitch; a DAW edit of pitch chooses spelling from key context, never overwriting a deliberate spelling without user action. |
| **Legato / staccato and other articulations** | Stored as written markings. Playback rules map them to duration ratio/velocity/key-switch; explicit per-note offsets override. Changing the rule changes every affected note at once. |
| **Repeats and jumps** | Stored once as form markings (repeat barlines, voltas, segno/coda, D.S./D.C.). `playback` unrolls them into passes; each played note = (written note id, pass index). |
| **Transposing instruments** | Pitch stored at **concert pitch with spelling**; the instrument's transposition (interval + octave, with spelling) is applied when displaying a transposed part. The concert/transposed toggle is a view setting. Hence no duplicate data between score and parts. |
| **Tuplets** | Durations are exact rationals (a triplet eighth is 1/12 of a whole note); tuplet groups store their ratio and bracket/number display separately. No rounding anywhere. |
| **Grace notes and ornaments** | Grace notes have written duration but zero *metric* duration; playback assigns them time (stealing from the previous or main note per rule). Trills, mordents, turns are markings expanded by playback rules into performed notes, which can receive offsets but are not written notes. |
| **Dynamics → velocity/curves** | Dynamic markings and hairpins are written objects with a range. Playback maps them to velocity (attack) and/or a continuous controller curve (e.g. CC1/CC11 or plugin parameter) per the instrument's settings. Explicit velocity offsets are deltas on top. |
| **Tied notes** | Written as separate notes with a tie relation (as notation requires). Playback merges a tie chain into **one sounding note**; offsets of the chain are stored on the first note. |

## Alternatives rejected

- **Storing the performance as a full second copy** (absolute onsets/velocities): any intent
  change would need manual re-sync — exactly what ADR-0003 forbids.
- **Only quantised positions, no offsets:** loses human feel; unacceptable for a DAW.
- **No threshold (every DAW move changes the score):** makes the score unreadable after any
  performance tweaking.

## Consequences

- `orchestra-playback` is a core component, not an afterthought, and needs extensive tests for
  each hard case above (to be written in Phase 1–3).
- MIDI import must infer intent (quantisation, spelling, voices) and store the rest as offsets.
