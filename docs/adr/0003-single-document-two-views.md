# ADR-0003 — One document, two views (notation/DAW), video as a track type; commands and undo

- **Status:** Accepted
- **Date:** 2026-10-28

## Context

Existing tools treat notation and sequencing as separate programs, or as one program with two
data models synchronised by import/export or bridges. Synchronisation is where they break: edits
get lost, MIDI-imported scores are unreadable, written articulations do not affect playback. The
owner wants a single program where the score and the arrangement are the same music.

## Decision

1. There is **one project document**, owned by `orchestra-core-model`. It is the only source of
   truth for every musical and structural datum.
2. **Notation and DAW are projections** of that document: they read the same tracks and the same
   notes and present them differently. No view stores its own copy of musical data; view state is
   limited to presentation (zoom, scroll, selection, collapsed lanes…).
3. **Video is a third kind of track** on the same timeline, not a separate project.
4. **Every modification is a command** applied to the model: from the notation view, the DAW view,
   an import, a script, a plugin automation recording. Commands are reversible; there is **one
   undo/redo history** for the whole document, independent of which view issued the command.
5. Derived data (layout, unrolled performance, waveforms, video proxies) are **caches** keyed by
   model identifiers and invalidated by model change notifications. They can always be thrown
   away and recomputed.
6. Model change notifications are fine-grained (which elements changed), so that every consumer
   updates **incrementally** (ADR-0007).

## Alternatives rejected

- **Two models with a synchronisation layer** (e.g. score model + MIDI clips, bridged): the
  classic approach and the classic failure mode; every bidirectional mapping becomes a source of
  divergence.
- **MIDI as the single model, notation derived from it:** loses the written intent (spelling,
  voices, tuplets, articulations as written), which cannot be reconstructed reliably.
- **Notation as the single model, DAW as an export:** makes free performance editing (microtiming,
  velocity, audio) a second-class citizen. ADR-0004 solves this with two layers per note instead.

## Consequences

- The model must express everything both views need — this is why each note has two layers
  (ADR-0004) and why time is rational with one tempo map (ADR-0005).
- Undo semantics are uniform and testable at the model level, without a GUI.
- Views are thinner but must handle "the music changed under me" through notifications.
