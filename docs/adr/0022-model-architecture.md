# ADR-0022 — Model architecture: stable IDs, bar-relative events in per-bar chunks, voices per instrument

- **Status:** Accepted (owner's decision, 2026-11-01; SCALETTA F1.06, decision D06). The owner
  chose the candidate P3 and voices per instrument before the text was written, then accepted the
  draft as it is.
- **Date:** 2026-11-01
- **Clarifies:** ADR-0004 ("position in its measure" is taken literally, §4). Does not change any
  Accepted decision.

## Context

ADR-0003 asks for one document read by every view, edited only by commands, with caches keyed by
model identifiers and fine-grained notifications. ADR-0004 gives every note a written intent and
performance offsets. ADR-0005 and ADR-0018…0021 put time in exact rationals, with one meter map
(keyed by **bar index**) and one tempo map (keyed by **absolute position**). ADR-0007 sets the
budgets: re-layout after one note ≤ 16 ms, opening 100 bars ≤ 1 s, idle RAM ≤ 250 MB.

Questions left open for this step: how objects are identified across undo/redo and save; how the
events of a voice are stored; whether bars are stored or derived; whether events keep an absolute
or a bar-relative position (ADR-0018, ADR-0020 §3); who may read the model while it changes; how
tracks, instruments, staves and voices are organised; where spanners live; what happens when notes
are entered past the end of a free bar.

The analysis and the measurements are in `spikes/model/ANALYSIS.md`. Three candidates were built
with the real `core-time` types and measured on two generated scores, **normal** (100 bars,
10 staves, 8 690 notes) and **heavy** (1 000 bars, 30 staves, 255 776 notes). Wall-clock times on
the heavy score, development machine:

| Operation | P1: sorted `Vec`, absolute positions | P2: chunk per bar, bars by index | **P3: chunk per bar, stable bar IDs, `Arc`** |
|---|---:|---:|---:|
| insert one note | 0.16 ms | 0.001 ms | 0.001 ms |
| insert a bar | 47 ms | 8.9 ms | 0.06 ms |
| lengthen a free bar | 51 ms | 0 | 0 |
| meter change, bars keep their contents | 49 ms | 0 | 0 |
| meter change, music rewritten across the new barlines | 0 | 9.7 ms | 1.2 ms |
| snapshot for another thread | 11 ms | 6.8 ms | 1.4 ms |
| one page (bars 40–60, all staves) | 0.14 ms | 0.23 ms | 0.22 ms |
| the whole score in order | 1.5 ms | 10.6 ms | 17 ms |
| memory | 44 MB | 41 MB | 46 MB |

The instruction counts (Gungraun) agree: P1 needs ≈ 170 million instructions to insert a bar on
the heavy score, P3 ≈ 140 000. On the normal score every candidate stays under 1 ms, so the
difference does not show up in small tests.

## Decision

### 1. Identity: 64-bit IDs, never reused

- Every object that something else can refer to has an **ID**: tracks, instruments, staves, voices,
  bars, events, noteheads, spanners. IDs are `u64` values from **one counter per document**, wrapped
  in one newtype per kind (`EventId`, `NoteId`, `BarId`…), so that the compiler rejects an ID of
  the wrong kind.
- An ID is **never reused** within a document, not even after undoing the command that created it:
  the counter only grows. Undoing a deletion re-inserts the object with **the same ID**, so
  selections, caches, pass-specific offsets (ADR-0004) and later redo steps still find it.
- IDs and the counter are **saved in the project file**, so references inside the file (ties,
  slurs, offsets) and diffs between versions stay stable. Paste, import and merge give the new
  objects new IDs.
- The map from an ID to where the object is (`ID → (voice, bar)`) is a **derived cache**
  (ADR-0003 §5), rebuilt at load, with a cheap multiplicative hash (IDs come from the document,
  not from untrusted input).

### 2. Tracks, instruments, staves, voices

Document → **tracks** (instrument tracks now; audio and video tracks later, with their anchoring
of ADR-0005 §5) → one **instrument** per instrument track (its name, its transposition per
ADR-0004, its number of staves) → **staves** and **voices**.

**Voices belong to the instrument**, not to a staff. Every voice has a home staff, and **every
event records the staff it is drawn on** (by default the voice's home staff). A melody passing
from one hand to the other, a cross-staff arpeggio or a beam across the two staves of a piano are
one voice with a different staff on some events. This matches MusicXML (voices per part, a
`<staff>` per note) for F1.17. The interface may still present "voices 1–4 of each staff" if that
reads better for the user.

### 3. Bars: lengths in the meter map, identity in the bar list

- The **length** of every bar stays where ADR-0018 put it, in the meter map, and nowhere else.
- The document keeps an ordered **bar list** of `BarId`s: it says which bar is which and how many
  bars the document has. It holds no length, so it duplicates nothing.
- What is attached to a bar (barline types, repeat barlines, voltas, rehearsal marks, the event
  chunks of §4) is keyed by `BarId`, so inserting or deleting a bar changes the bar list and
  nothing else. The meter map keeps its `BarIndex` keys (ADR-0018 unchanged): the commands that
  insert or delete bars move its few entries in the same step.

### 4. Events: bar-relative offsets in one chunk per bar per voice

- An event's written position is **(bar, offset from the start of the bar)**, literally the
  "position in its measure" of ADR-0004. Its absolute `Position` is derived: one O(log m) meter map
  lookup and one rational addition.
- Every voice has one **chunk** per bar of the bar list: a vector of `(offset, event)` sorted by
  offset (events at the same offset keep the order of insertion). An empty chunk does not allocate.
  A non-empty chunk sits behind an `Arc` (§6).
- A chord is **one event** with one or more noteheads sharing its written duration. Every notehead
  has its own `NoteId`, because ties, spelling and performance offsets are per note (ADR-0004). A
  rest is an event without noteheads.
- **Overfull bars:** when the contents of a bar last longer than the bar (a meter changed to a
  shorter one and the bars kept their contents, or a free bar was shortened), the bar is
  **overfull**. That is a valid state of the model: the events keep their offsets, and the score
  shows the bar as an error until the user fixes it. The model never "repairs" it silently.
- **Events never cross a barline.** A note entered across a barline (for example a long note drawn
  in the DAW view) is split by the entering command into a chain of tied notes. Ties are stored
  relations (ADR-0004), and playback merges the chain into one sounding note. An event that does
  not fit in its bar is exactly the overfull case above.
- **Free bars that grow:** when notes are entered past the end of a free bar (ADR-0018 §3), the
  same command lengthens the bar in the meter map and inserts the notes, in one step of undo.
  Nothing else moves: the later bars keep their offsets.

Complexity (n = events of the whole score, k = events of one bar in one voice, B = bars,
V = voices, m = meter changes, t = tempo changes):

| Operation | Cost |
|---|---|
| insert or delete an event | O(log m + k) |
| find an event by ID | O(1) average + O(k) + O(log m) for its absolute position |
| events of a range of bars with absolute positions | O(bars × log m + r) |
| insert or delete a bar | O(B + V × B) pointer moves, no event touched |
| change a bar's length, keeping the bars' contents | O(1) in the voices (+ the meter map edit) |
| rewrite the music of a region across new barlines | O(events of the region) |

### 5. Things anchored to absolute positions

The tempo map stays keyed by absolute `Position` (ADR-0020). So **every command that changes where
later bars start** (inserting or deleting a bar, changing a bar's length, changing a meter while
keeping the bars' contents) **also moves the tempo changes after the edit point by the same
amount, in the same step of undo**. That way a tempo change stays at the same written place, like
the notes. Rewriting the music across new barlines does not move anything in time, so it leaves
the tempo map alone.

### 6. Spanners

- **Note-anchored** spanners (ties, slurs, glissandi, arpeggios, tuplet groups) refer to the
  `EventId`s or `NoteId`s at their ends. They follow the notes wherever the notes go, even across
  voices and staves. A derived index answers "which spanners touch this event".
- **Position-anchored** spanners (hairpins, ottavas, pedal lines, dynamics and tempo text) store
  their start and end as **(bar, offset)**, like events, plus their staff or voice. They are few
  (hundreds per score), so a list per instrument sorted by start is enough. The details of each
  kind belong to F1.09.

### 7. Readers and threads

- The model belongs to **one editing thread** (the interface thread), which applies the commands.
- A background reader (a layout worker, autosave, the playback compile) receives a **snapshot**:
  a copy of the voices' chunk pointers, whose chunks are shared through `Arc`. Editing a chunk
  that a snapshot still holds copies that chunk only (copy-on-write). On the heavy score a snapshot
  costs ≈ 1.4 ms.
- The audio thread **never reads the model**. It receives compiled events from `playback`
  through fixed-capacity lock-free structures (ADR-0007).

### 8. Constraints passed on to later steps

- **F1.09 (notes):** the definition of overfull bars and the splitting of notes across barlines
  into tie chains (§4); the per-note IDs; position-anchored spanners in (bar, offset) form.
- **F1.11 (commands):** undo re-inserts objects with their old IDs; one transaction includes the
  meter map edit, the event changes and the tempo map shift (§5). The **default behaviour of a
  meter change** stays open there: keep the bars' contents, or rewrite the music up to the next
  meter change. Both are cheap with this storage. Notifications name `BarId`s and `EventId`s.
- **F1.15 (file format):** IDs, the counter, the bar list and the bar-relative offsets are saved
  as they are; the ID → location index and the absolute positions are not saved.
- **`core-time`:** the meter map needs an operation that inserts or deletes bars (shifting its
  entries), and both maps need a **bulk edit**, so that shifting t tempo changes costs one rebuild
  (O(t)), not t rebuilds (O(t²)). These come with the first command that needs them.

## Alternatives rejected

- **Absolute positions in one sorted vector per voice (P1):** the fastest reader, but every edit
  that changes where bars start rewrites every later event of every voice: ≈ 50 ms on the heavy
  score for inserting a bar or lengthening a free cadenza, three times the budget, and growing
  with the score. Single notes cost 0.16 ms there because of the vector's tail move. It also
  cannot represent an overfull bar.
- **Chunks named by bar index (P2):** fixes the bar length edits, but inserting a bar renumbers
  the index entry of every later event (≈ 9 ms on the heavy score, O(n)).
- **An ordered tree per voice (`BTreeMap`):** no advantage over a vector at these sizes, worse for
  scans; not prototyped.
- **Generational indices (slotmap-style):** faster lookup on paper, but an ID survives undo only
  if the object returns to the same slot, and survives save only if the slots are saved with their
  holes; lookup by ID is not the hot path.
- **Addresses instead of IDs** (track, voice, bar, offset): change on every edit; ADR-0003 §5
  needs identifiers for its caches.
- **Bar objects with their own length:** duplicate the meter map (ADR-0003).
- **Persistent (immutable) data structures for the whole model** (`im`, `rpds`): free snapshots,
  but a higher cost on every access and a new dependency, for a benefit the `Arc` chunks already
  give.
- **Voices per staff, with cross-staff notes as exceptions:** the more common design (as far as
  we know, MuseScore and Sibelius); rejected by the owner because a line wandering between staves
  would be an exception on every note, and MusicXML would need a remapping of voice numbers.

## Consequences

- No measured edit grows with the length of the score; a single note costs about 1 µs.
- Reading with absolute positions costs one rational addition per event: ≈ 0.2 ms for a page,
  ≈ 17 ms for the whole heavy score in order. That is acceptable for whole-score work (export, the
  first playback compile), which is not done per keystroke. Possible later optimisations:
  iterating in (bar, offset) form where absolute positions are not needed, a faster addition when
  the bar start is an integer.
- One `Arc` and one vector per non-empty bar of every voice: ≈ 82 000 allocations and ≈ 180 bytes
  per note on the heavy score; load ≈ 57 ms. Possible later optimisations: an `Arc` per voice as
  well (a snapshot would copy one pointer per voice), no slot at all for empty voice-bars.
- The commands of F1.11 carry the burden of consistency between the two key families (§3, §5).
  Property tests there must check that after any sequence of commands every tempo change keeps its
  written place relative to the bars.
- The prototype in `spikes/model/` stays as the evidence for this ADR; it is not product code.
