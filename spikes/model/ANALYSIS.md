# F1.06 — Model architecture: analysis of the alternatives (stage 1 of 3)

Status: stage 1 (this analysis) reviewed by the owner on 2026-11-01, who chose **F2** for the
voices (§6) and approved the prototype plan of §7; **stage 2 results in §8**. Stage 3 is the
draft ADR.

The binding context: one document, views as projections, every edit a command, one undo history,
derived data as caches keyed by model identifiers, fine-grained notifications (ADR-0003);
two-layer notes, offsets as deltas, ties stored as relations, played note = (written note id,
pass index) (ADR-0004); exact rational time, one meter map and one tempo map (ADR-0005,
ADR-0018…0021); the performance budgets of ADR-0007 (re-layout after one note ≤ 16 ms, opening
100 bars ≤ 1 s, idle RAM ≤ 250 MB).

## 0. A fact from the code that shapes everything

The two maps that already exist are keyed **differently**:

- `MeterMap` (ADR-0018) keys meter changes, irregular/free bars, groupings and numbering
  exceptions by **`BarIndex`**. Bar starts are *derived* by summing bar lengths. The map has no end
  (bars go on forever) and no "insert a bar" operation.
- `TempoMap` (ADR-0020/0021) keys tempo changes by **`Position`** (absolute whole notes from 0).

So when a bar gets longer (a meter change, a free cadenza that grows, a bar inserted), everything
keyed by `BarIndex` after it moves **with** the bars, and everything keyed by `Position` stays
where it was and **slides relative to the bars**. Whatever we choose for notes, the editing
commands must keep the two families consistent. This is the core of question 4 below.

## 1. Object identity (IDs)

Requirements: an ID survives undo/redo (undoing a delete brings back *the same* note, so that a
later redo, a selection, a layout cache or a pass-specific offset still refers to it), survives
save/load (diffable files, stable references inside the file: ties, slurs, offsets), never refers
to the wrong object after a delete (no ABA), cheap to copy and hash.

| | A1. Generational index (slotmap-style: slot + generation) | A2. Monotonic 64-bit ID, never reused | A3. No IDs: addresses (track, voice, bar, offset) |
|---|---|---|---|
| Lookup by ID | O(1), one array access | O(1) average via a hash map ID → location | O(log n) search, but the address changes on every edit |
| Survives undo of a delete | Only if the undo re-inserts into the **same slot** with the same generation; standard slotmaps cannot do that, an own one can | Yes, trivially: the undo record keeps the ID and re-inserts it | No |
| Survives save/load | Only if slots are saved as-is (holes included) or remapped (then IDs change at every load) | Yes, the ID is just written; the counter is saved too | No |
| Copy/paste, import, merge of documents | New IDs | New IDs from the counter | — |
| Memory | 8 bytes per ID + free list | 8 bytes per ID + ≈ 16–24 bytes per entry in the hash index | 0 |

**Recommendation: A2.** A1 is faster on paper, but stability across undo and save is exactly
what A1 trades away, and the lookup by ID is not the hot path (scans by position are: layout,
playback). A3 is rejected by ADR-0003 §5 (caches keyed by identifiers). Constraint passed on to
F1.11 and F1.15: IDs are never reused within a document; the counter is part of the file.

Open sub-question for the prototype: is the ID → location index **authoritative data** or a
**cache** rebuilt at load? It holds nothing that is not derivable, so ADR-0003 says cache.

## 2. How events are stored in a voice

"Event" = a chord/note, a rest, or something else that occupies a rhythmic position in a voice
(a chord is one rhythmic event with one or more noteheads sharing the written duration; each
notehead has its own ID because ties, offsets and spelling are per note).

Operations that matter, with the counts of the reference scores (§7): insert/delete one event
(note entry), find the events of a range of bars on all staves (layout of the visible page,
playback compile, ADR-0007 "incremental"), shift everything after a point (insert a bar, lengthen
a free bar), iterate one voice in order (export, playback).

| | B1. One sorted `Vec` per voice | B2. One ordered tree (`BTreeMap`) per voice | B3. One chunk per bar per voice (small sorted `Vec` in each) |
|---|---|---|---|
| Insert/delete one event | O(log n) search + O(n) memmove of the tail (n = events of that voice: ≈ 1–10 thousand, a few µs) | O(log n) | O(log B) or O(1) to find the bar + O(k) inside it (k = events in that bar, ≈ 1–30) |
| Range of bars | O(log n + r), contiguous memory, best for the cache | O(log n + r), pointer chasing | O(r), contiguous inside each bar |
| Shift everything after a point | O(n) rewrite of every later position, in **every** voice | O(n) as well (keys change) | **O(B)** pointer moves if offsets are bar-relative (question 4); the events are untouched |
| Natural snapshot / sharing (§5) | Whole voice copied | Whole tree copied | Per-bar chunks behind `Arc`: clone = O(B) pointer copies, write = copy one bar |
| Allocations | 1 per voice | 1 per node (≈ every 11 entries) | 1 per non-empty bar per voice |

B2 brings no advantage over B1 at these sizes (a tree wins over a `Vec` only for much longer
sequences) and scans worse; I propose **not** to prototype it. B1 and B3 are the real contest.
B3 is close to what most notation programs do internally (measures containing segments).

## 3. Bars: stored or derived?

The *structure* of the bars (where every barline is) is already source data **in the meter map**,
and must stay there only (ADR-0018 chose explicit lengths, also for free bars). So bar objects
carrying their own length would duplicate it: excluded by ADR-0003.

What is not decided is whether bars have an **identity** in the model:

| | C1. Bars are only indices (`BarIndex`), purely derived | C2. A document-level ordered list of stable `BarId`s, lengths still from the meter map |
|---|---|---|
| What it stores | Nothing; plus the document length (last bar), which is needed anyway | One ID per bar (≈ 8 bytes × bars) + the document length implicitly |
| Things attached to a bar (B3 chunks, rehearsal marks, repeat barlines, voltas) | Keyed by `BarIndex`: inserting a bar shifts every later key, in every voice and every list | Keyed by `BarId`: inserting a bar inserts one ID into one list; nothing else changes, and the ID → location index (§1) stays valid |
| Meter map | Unchanged | Unchanged too (it keeps `BarIndex` keys; the insert-bar command shifts its few entries, O(meter changes)) |
| Risk | Simplest | Two ways to name a bar; the list must always have exactly "document length" entries |

The bar list is not a duplicate: it says *which* bar is which, not *how long* it is. Note that
with B1 (absolute positions) C2 is pointless; it only matters with B3.

## 4. Absolute or bar-relative positions (ADR-0004's "position in its measure", ADR-0018 §open)

Separating two things that the question mixes:

- **Storage:** what number the model keeps for an event.
- **Behaviour:** what the user sees when a meter changes (e.g. 4/4 → 3/4 at bar 10). Every major
  program offers some variant of (i) *keep the bars' contents* (bar 10 becomes overfull, later bars
  unchanged) or (ii) *rewrite the music* until the next meter change (notes flow across the new
  barlines and get re-tied). The behaviour is a **command**; both storages can implement both.
  What the storage decides is which behaviour is **free** and which costs a rewrite.

| | D1. Absolute `Position` | D2. (bar, offset in bar) |
|---|---|---|
| Meter change, behaviour (ii) "rewrite" | Free for the positions; ties across the new barlines still have to be recomputed (ties are stored, ADR-0004) | O(events until the next meter change) rewrite |
| Meter change, behaviour (i) "keep bars" | O(all later events) rewrite, in every voice | Free (only bar 10 can overflow) |
| Lengthen a free bar while entering a cadenza (ADR-0018) | **O(all later events)** shifted in every voice, plus all later tempo changes | Free: only the meter map entry changes |
| Insert/delete a bar in the middle | O(all later events) | Free with C2; O(B) key shifts with C1 |
| Keeps aligned with what is keyed by `BarIndex` (§0) | Must be fixed by the command | Automatically |
| Keeps aligned with the tempo map (keyed by `Position`) | Automatically | Must be fixed by the command: O(later tempo changes), a handful |
| DAW side: absolute position of a note | Read directly | One O(log m) meter map lookup + one addition (m = meter changes: tens) |
| DAW side: drag a note across a barline | Change one number | Move the event to the other bar's chunk |
| Rational costs | Larger numerators late in long scores | Small offsets (< one bar), cheaper gcds |

**Leaning: D2 with B3 and C2.** The operations that D1 makes expensive (lengthen a free bar,
insert a bar, keep-bars meter changes) are ordinary notation edits that happen while the user is
typing, and with D1 each one rewrites the whole rest of the score. That is exactly what CLAUDE.md
§5 forbids. D2 costs one cheap lookup per note on the DAW side, which the playback compile does
once anyway. The tempo map stays keyed by `Position`. The commands that change bar lengths also
shift the tempo changes after the edit point, in the same atomic step. That is a handful of
entries and contradicts nothing in ADR-0020, which left this question open.

If the owner prefers D1, ADR-0004's phrase reads as "the note *has* a derivable position in its
bar"; with D2 it is literal. The new ADR states which one explicitly.

Consequences to record whatever is chosen:

- An event may be **longer than its bar** (a DAW-entered whole note across a barline)? ADR-0004
  says ties are stored as separate notes, so the entering command splits it into a tie chain;
  playback merges the chain back into one sounding note. Constraint for F1.09/F1.11, no new
  decision.
- An **overfull bar** (behaviour (i), or a meter that shrinks) is a valid model state, shown as an
  error in the score until the user fixes it, never silently "repaired".

## 5. Who reads the model while it changes

The audio thread never touches the model (ADR-0007: no locks there). Layout and playback compile
read it; later, autosave and a background layout worker may want to read it **while the UI keeps
editing**.

| | E1. Mutable model on the UI thread only; consumers read it there and send compiled results | E2. Persistent (immutable) structures (e.g. the `im`/`rpds` crates): every version is a snapshot | E3. Mutable model whose per-bar chunks sit behind `Arc` (copy-on-write) |
|---|---|---|---|
| Snapshot for a worker thread | Not possible: copy the whole document or do the work on the UI thread | O(1) | O(B) pointer copies per voice (≈ 30 000 for the heavy score: tens of µs) |
| Cost of an edit | Lowest | Higher: path copying, more indirections everywhere | One bar's chunk copied if a snapshot holds it, otherwise in place (`Arc::make_mut`) |
| Undo | By commands (ADR-0003) | Could keep old versions, but ADR-0003 still wants commands and notifications | By commands |
| New dependency | No | Yes (both MPL-2.0, to verify) | No |

**Leaning: E3**, which comes almost for free with B3. E1 is the fallback if the prototype shows
the `Arc` costing too much; E2 changes the cost of every access for a benefit E3 already gives.

## 6. Tracks, instruments, staves, voices; spanners

### Hierarchy

Project → **tracks** (kinds: instrument track; audio and video tracks later, each with its own
anchoring per ADR-0005 §5) → an instrument track has one **instrument** (name, transposition per
ADR-0004, number of staves: piano 2, organ 3) → **staves** → **voices**.

The open choice is where voices belong:

| | F1. Voices belong to a staff; an event may be drawn on another staff (cross-staff flag) | F2. Voices belong to the instrument; every event has a staff (default: the voice's home staff) |
|---|---|---|
| Piano cross-staff passages | A per-event override | A per-event field: the same thing, named differently |
| A voice that wanders between staves (a melody passing hands) | Stays in its staff's voice, every note overridden | Natural |
| MusicXML (F1.17) | Needs a remapping of voice numbers | Matches MusicXML directly: voices per part, `<staff>` per note |
| Layout per staff | Direct | One filter per staff (cheap, the staff field is on the event) |

**Leaning: F2**, because it matches the interchange format and makes cross-staff a field rather
than an exception. The difference is small; the owner may prefer the more common F1.

### Spanners (slurs, hairpins, ottavas, pedal, tuplet brackets, glissandi…)

Two kinds, by what they logically attach to:

- **Note-anchored** (tie, slur, glissando, arpeggio, tuplet as a group of events): store the IDs
  of the first and last events; they follow the notes wherever they go. Stored with the voice.
- **Position-anchored** (hairpin, ottava, pedal, dynamics text, tempo text): store a start and an
  end in the **same position form as notes** (question 4) plus their staff or voice. With B3 the
  spanner lives in the chunk of its start bar; a range query looks back at most "longest spanner"
  bars (kept as a per-staff maximum), or uses a small sorted list per staff. They are few
  (hundreds), so a list sorted by start is enough; no interval tree.

## 7. Prototype plan (stage 2), if the owner agrees

Location: `spikes/model/` (the separate workspace of Phase 0; nothing here is product code).
Measurements: instruction counts with Gungraun, as in the main workspace, plus allocation counts
and memory per note.

Reference scores, generated deterministically:

- **Normal:** 100 bars × 10 staves, mixed density (some voices in sixteenths, chords, triplets and
  quintuplets, rests, a few slurs and hairpins): ≈ 8 000–10 000 notes.
- **Heavy:** 1 000 bars × 30 staves, the same mix: ≈ 300 000 notes.

Candidates: **P1** = B1 + D1 (flat sorted `Vec`, absolute positions); **P2** = B3 + D2 + C1 (chunks
per bar, bar-relative, bars as indices); **P3** = B3 + D2 + C2 + E3 (chunks keyed by stable bar
IDs behind `Arc`). All with A2 IDs and F2 voices; the voice and spanner choices do not change the
measurements, so they are not prototyped.

Operations measured on each: insert and delete one note in the middle; find one note by ID;
collect bars 40–60 of every staff with absolute positions (what layout and playback ask for);
insert a bar at bar 10; lengthen a free bar at bar 10 by a quarter; change a meter at bar 10 with
both behaviours (i) and (ii); take a snapshot; full ordered iteration; memory and allocations of
the loaded score. Each operation is compared with the ADR-0007 budget it serves.

Expected time: one session for the prototype and the numbers, then the ADR draft.

## 8. Results (stage 2, 2026-11-01)

Code: `src/p1.rs`, `src/p2.rs`, `src/p3.rs` (the candidates), `src/lib.rs` (event type, score
generator, meter map edits, probes), `tests/agree.rs` (the three candidates hold the same music
after every operation), `benches/model.rs` (instruction counts, Gungraun), `src/bin/report.rs`
(memory, allocations, wall-clock times on the development machine).

Generated scores: **normal** 100 bars, 14 voices on 10 staves, 7 968 events, 8 690 notes;
**heavy** 1 000 bars, 41 voices on 30 staves, 238 552 events, 255 776 notes.

### Instruction counts (Callgrind; insert/delete/find are batches of 64)

| Operation | P1 normal | P2 normal | P3 normal | P1 heavy | P2 heavy | P3 heavy |
|---|---:|---:|---:|---:|---:|---:|
| insert ×64 | 483 328 | 116 112 | 117 862 | 28 721 941 | 107 183 | 108 735 |
| delete ×64 | 2 861 910 | 19 040 | 22 855 | 20 695 907 | 16 253 | 20 104 |
| find ×64 | 3 322 | 48 244 | 50 611 | 3 322 | 51 474 | 53 841 |
| collect bars 40–60 | 19 210 | 456 185 | 457 282 | 60 103 | 1 367 833 | 1 370 372 |
| insert a bar | 4 079 532 | 396 142 | 16 114 | 169 468 248 | 16 288 331 | 137 614 |
| lengthen the free bar | 5 005 594 | 94 | 94 | 172 146 230 | 94 | 94 |
| meter change, keep bars (i) | 4 874 291 | 94 | 94 | 172 963 585 | 94 | 94 |
| meter change, rewrite (ii) | 99 | 1 427 868 | 1 162 839 | 99 | 20 515 262 | 4 801 028 |
| snapshot + 64 inserts | 1 383 180 | 808 309 | 213 864 | 55 472 029 | 19 224 853 | 1 059 976 |
| iterate everything | 64 048 | (invalid) | 2 053 160 | 1 909 071 | (invalid) | 65 385 086 |

94–99 instructions = the empty operation (the benchmark's own overhead). The two P2 `iterate`
counts (120) are a measurement artefact: Callgrind records no work inside the measured function,
while the native timing below shows that P2 does the same work as P3 (same code path). They are
left out rather than explained away.

### Memory and times (native, development machine, median of 31 runs, µs unless stated)

| | memory | allocations | per note | build | insert | delete | find | page 40–60 | insert bar | lengthen | keep | rewrite | snapshot | iterate |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| P1 normal | 1.4 MB | 16 | 165 B | 1.0 ms | 2.80 | 1.83 | 0.008 | 8 | 707 | 874 | 808 | 0 | 49 | 15 |
| P2 normal | 1.3 MB | 1 416 | 152 B | 1.4 ms | 0.43 | 0.12 | 0.139 | 58 | 50 | 0 | 0 | 183 | 144 | 256 |
| P3 normal | 1.5 MB | 2 818 | 172 B | 1.5 ms | 0.42 | 0.19 | 0.160 | 61 | 2 | 0 | 0 | 168 | 25 | 266 |
| P1 heavy | 44.0 MB | 43 | 172 B | 33 ms | 158 | 34 | 0.092 | 135 | **47 458** | **50 814** | **49 291** | 0 | 11 218 | 1 455 |
| P2 heavy | 41.0 MB | 41 043 | 160 B | 55 ms | 1.06 | 0.56 | 0.682 | 230 | 8 887 | 0 | 0 | 9 655 | 6 773 | 10 550 |
| P3 heavy | 46.2 MB | 82 045 | 181 B | 57 ms | 1.15 | 1.00 | 0.911 | 221 | 61 | 0 | 0 | 1 241 | 1 404 | 16 920 |

### What the numbers say

1. **P1 breaks the budget on a large score.** Inserting a bar, lengthening a free bar or a
   keep-bars meter change cost ≈ 170 million instructions and ≈ 50 ms on the heavy score, three
   times the 16 ms of CLAUDE.md §5 for one edit, and they grow with the score. Even a single note
   costs ≈ 0.16 ms there (the `memmove` of a 650 KB voice). P1 also **cannot represent an overfull
   bar** (`p1.rs`). On the normal score it stays under 1 ms, which is why the choice would not
   show up in small tests.
2. **P2 fixes the edits that change bar lengths** (they cost nothing), but naming bars by index
   makes inserting a bar and the rewrite meter change renumber the index of every later event:
   ≈ 9–10 ms on the heavy score, within budget but O(n).
3. **P3 keeps every edit small on the heavy score:** insert a bar 61 µs, rewrite 1.2 ms (only the
   region's events move), and the snapshot for a background reader is 8× cheaper than P1's
   (1.4 ms vs 11 ms), including the copy-on-write cost of the inserts that follow.
4. **The price of bar-relative offsets (P2, P3)** is one rational addition per event read with
   its absolute position: a page of layout costs ≈ 1.4 M instructions instead of 60 k (≈ 0.2 ms,
   far under the 16 ms), find by ID ≈ 0.9 µs instead of 0.1 µs, and a full iteration of the heavy
   score ≈ 17 ms instead of 1.5 ms. The last one matters only for whole-score work (export, the
   first playback compile), which is not per keystroke. It can be lowered later: per-bar
   denominators, or iterating in bar-relative form where absolute positions are not needed.
5. **Memory** is about the same (165–181 bytes per note, 46 MB for the heavy score; ADR-0007's
   250 MB is for an empty project). P3 has one `Arc` and one `Vec` per non-empty voice-bar
   (82 000 allocations on the heavy score), which costs load time (57 ms vs 33 ms, far under
   the 1 s budget for 100 bars).
6. Possible later optimisations for P3, not needed now: share whole voices behind an `Arc` as
   well (the snapshot would copy 41 pointers instead of 41 000); give empty voice-bars no slot at
   all; a cheaper rational add when the bar start is an integer.

**Recommendation for the ADR: P3** (B3 + D2 + C2 + E3, with A2 IDs and F2 voices). It is the
only candidate where no measured edit grows with the length of the score.
