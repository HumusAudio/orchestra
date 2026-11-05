# ADR-0026 — Commands, one undo history, fine-grained change notifications

- **Status:** Accepted (owner's decision, 2026-11-04; SCALETTA F1.11, decisions D08 and D21).
  The owner approved every recommendation of §10 (U1–U11) as written, and the split of the step
  into F1.11b and F1.11c (Consequences).
  The precisions of §11 (V1–V9, W1–W6) were approved on 2026-11-04, while implementing F1.11b
  part 2a; X1–X10 on 2026-11-05, before part 2b was written, and X11 the same day, when the
  property test of part 2b found the case.
- **Date:** 2026-11-04
- **Refines:** ADR-0003 §4–§6 (every modification is a reversible command, one history, caches
  invalidated by fine-grained notifications), ADR-0022 §1 and §8 (undo re-inserts objects with
  their old IDs; notifications name `BarId`s and `EventId`s; insertion and deletion of bars in the
  meter map), ADR-0024 §4, §7, §11 (ties and slurs cleaned up with their ends; the tempo map
  rebuilt after edits), ADR-0025 §9 (deleting a bar deletes its form markings, the form stays
  valid).
- **Amends:** nothing.

## Context

Since F1.08 a document is created with the `DocumentBuilder` and is then read-only: ADR-0003 §4
says that every later change — from the score, from the DAW view, from an import, a script or a
recorded automation — is a **command**, reversible, in **one undo history** for the whole
document. ADR-0003 §5–§6 adds that derived data (layout, the unrolled performance, the tempo map,
the indices of ADR-0022 §1) are caches invalidated by **fine-grained notifications**, so that
every consumer updates incrementally (ADR-0007: re-layout after one note ≤ 16 ms).

Constraints already fixed by Accepted ADRs:

- IDs are never reused; undoing a deletion brings the object back **with the same ID**
  (ADR-0022 §1). The ID counter only grows, so it is not part of what undo restores.
- One transaction contains everything a user action implies: the meter map edit, the event
  changes, the rebuild of the derived tempo map (ADR-0022 §8, ADR-0024 §11).
- A note entered across a barline becomes a tie chain; notes entered past the end of a free bar
  lengthen it in the same step (ADR-0022 §4).
- Deleting an event deletes the slurs that end on it (ADR-0024 §7); deleting a bar deletes its form
  markings, and every command leaves the form valid (ADR-0025 §9).
- The default behaviour of a **meter change** (D21) is open: keep the bars' contents, or rewrite
  the music across the new barlines (ADR-0022 §8). Both are cheap with the storage of ADR-0022
  (measured: 0 and 1.2 ms on the heavy score).
- The size of the history and the merging of continuous edits (D08) are open.

### Sources checked (2026-11-04)

- **Dorico** rebars the music when a time signature is entered, **only up to the next existing
  time signature** (or the end of the flow); with Insert mode on, the following music is pushed
  or pulled instead
  ([help](https://steinberg.help/dorico/v4/en/dorico/topics/write_mode/write_mode_notations_input/write_mode_time_signatures_inputting_panel_t.html),
  [forum](https://forums.steinberg.net/t/time-signature-change-in-dorico/722062)).
- **MuseScore** rewrites the measures up to the next time signature when one is dropped on a
  measure, and asks whether to rewrite when the whole score's signature changes
  ([forum](https://musescore.org/en/node/375460), [forum](https://musescore.org/en/node/7322)).
- **Sibelius** makes "rhythmic adjustments automatically" when time signatures are changed after
  notes are entered (course summary,
  [LinkedIn Learning](https://www.lynda.com/Sibelius-tutorials/Rhythms-time-signatures/2814065/2247904-4.html)).
- **Logic Pro** keeps the recorded notes where they are and moves the barlines when a signature
  changes (user reports, [Apple Community](https://discussions.apple.com/thread/7772028)): the
  DAW view of the same choice.
- **Undo limits:** Cubase and Nuendo expose a "Maximum Undo Steps" preference, explained as a way
  to save memory
  ([Steinberg help](https://steinberg.help/nuendo/v10/en/cubase_nuendo/topics/project_window/project_window_edit_history_maximum_undo_steps_number_setting_t.html));
  Adobe Animate defaults to 100 levels (2–300). No public limit was found for Dorico.

What the sources show: notation programs and DAWs agree that a meter change **keeps the music
where it is in time and moves the barlines**, up to the next meter change; "keep the bars and
shift everything after" is the special mode (Dorico's Insert mode). A limit on undo exists to
bound memory, and is expressed in steps only because steps are what the user sees.

## Decision

New module(s) in `orchestra-core-model`: `edit` (the primitive edits), `command` (what the user
asks for), `history` (undo/redo), `change` (notifications). `orchestra-core-time` gains the bar
insertion/deletion and the bulk edit of its maps (ADR-0022 §8).

### 1. Two levels: commands expand into primitive edits that return their inverse (U1)

- A **primitive edit** (`Edit`) is a closed enum of the smallest changes the document knows. Its
  vocabulary is three verbs over one enum of object kinds, plus the bar and meter operations:

  ```text
  Edit = Insert(Object)            // an object with its ID, at its place
       | Remove(ObjectRef)         // by ID
       | Replace(Object)           // same ID, new content (fields, place)
       | InsertBars { at: BarIndex, ids: Vec<BarId> }    // empty bars only
       | RemoveBars { at: BarIndex, count: u32 }         // bars carrying nothing only
       | Meter(MeterEdit)          // the source data of the meter map (ADR-0018/0019)
       | SetInitial(Initial)       // initial tempo, key, a staff's initial clef, numbering style
  Object = Event{voice, bar, offset, event} | Tuplet{voice, …} | Slur | Dynamic | Hairpin
         | TempoMark | ClefChange{staff, …} | KeyChange | KeyOverride | RepeatStart
         | RepeatEnd | Volta | Marker | Jump | BarRepeat | PassRule
  ```

  Tracks, instruments, staves and voices are objects of the same vocabulary (§9 says when they
  are implemented).

- **Applying an edit returns its exact inverse** (an `Insert` returns a `Remove` of the same ID, a
  `Remove` returns an `Insert` of the whole removed object, a `Replace` returns a `Replace` with
  the old content, `core-time`'s setters already return the previous value). Undo applies the
  recorded inverses in reverse order, and **what undo returns is exactly the redo**: one
  mechanism, no second code path to keep in step.
- `RemoveBars` only accepts bars that carry nothing (no event, no mark, no form marking): the
  command that deletes bars first removes or moves what sits on them with ordinary edits (§6).
  So every primitive stays small and its inverse obviously exact.
- A **command** (`Command`) is what the user asks for, as data: "enter this chord here", "delete
  bars 5–8", "change the meter at bar 9 to 3/4", "paste these events". Executing a command checks
  its preconditions, allocates new IDs from the counter, and emits a sequence of edits into the
  open transaction. The invariants of the model (voice without overlaps, ties to the same pitch,
  valid form, tie chains across barlines, lengthened free bars…) are the commands' job; the
  edits only keep the structure consistent (indices, sort orders).
- Commands are data too, so they can be generated by property tests, logged, replayed and later
  scripted; the history stores **edits**, not commands, so redo never re-runs a command (and never
  allocates new IDs: redo brings back the same ones).

### 2. Transactions: all or nothing, one step of undo

- Every command runs inside a **transaction**. If any check or edit fails, the edits already
  applied are reverted in reverse order before the error is returned (a typed `CommandError`,
  English `Display` for developers, Fluent keys for the user later, as for the other errors):
  the document is never left half-changed.
- One transaction is one step of undo, whatever its size (pasting 200 notes, deleting 40 bars).
  Several commands can share one transaction when one user action needs them (a "paste special"
  that also adds a dynamic).
- **Derived data** is updated at the end of the transaction, once: the tempo map is rebuilt from
  the marks (ADR-0024 §11) with the bulk edit, only if a tempo mark or a bar start changed; the
  ID → location, note → event, slur and tie indices are updated per edit (O(1) average each). A
  tempo map that fails to rebuild (overflow) fails the transaction, which is reverted.
- The ID counter is not reverted (ADR-0022 §1): a failed or undone transaction "spends" its IDs.

### 3. The history (U3, U6, U7 — D08 part 1)

- **Linear**: undo and redo walk one list; a new transaction after some undos discards the redo
  steps. No undo tree.
- **Bounded by memory, not by count**: each step records an estimate of the heap its inverses
  hold (events, text, vectors). When the total exceeds the limit, the oldest steps are dropped.
  The most recent step is always kept, even if it alone is larger than the limit. Proposed
  default: **128 MB**, a preference later (F2.12). Typical steps are small: entering a note costs
  one `Remove` (tens of bytes); deleting the whole heavy score (255 776 notes) ≈ 66 MB with the
  257 bytes per note measured in F1.09c.
- **Not saved in the project file.** The file is the score (ADR-0010); undo across sessions is
  rare in notation programs and DAWs and would freeze the edit vocabulary into the file format.
  Crash recovery (F2.14) decides separately whether it journals edits.
- The history lives next to the document in an `Editor` (document + history + open transaction)
  owned by the editing thread (ADR-0022 §7). Selection, scroll and zoom are view state
  (ADR-0003 §2) and are not in the history; after undo/redo the notification (§5) tells the view
  which objects came back, so it can select them.

### 4. Continuous gestures: one open transaction (U4 — D08 part 2)

- A drag (a note moved with the mouse, a velocity painted, a hairpin stretched) is a **gesture**:
  `begin_gesture`, then any number of commands applied live (every view updates through the
  notifications, no view keeps a private copy of the moving data — ADR-0003), then `end_gesture`,
  which closes the transaction as **one step of undo**. `cancel_gesture` (Esc) reverts it.
- **Compaction:** inside a gesture, a `Replace` of an object already replaced in the same gesture
  keeps only the **first** inverse, so a 1 000-step drag stores one old value, not 1 000.
- No other command and no undo run while a gesture is open (the editor refuses them with a typed
  error; the interface ends or cancels the gesture first).
- No coalescing by time ("edits within 500 ms merge"): it would make the history depend on the
  clock, hence not reproducible in tests.

### 5. Notifications: a change set per transaction (U5)

- Applying a command inside a gesture, closing a transaction, undo and redo each **return a
  `ChangeSet`**: what changed, never how to redraw it. Nobody registers callbacks inside the model:
  the editor returns the set, the application (`app`) hands it to layout, playback and the views.
  This keeps `core-model` free of threads and observers, and testable.
- Contents (deduplicated, cheap to merge):
  - the **cells** (`VoiceId`, `BarId`) whose events changed, and the `EventId`s/`NoteId`s
    inserted, replaced or removed;
  - the IDs of the other objects inserted, replaced or removed, by kind;
  - **structural changes**: bars inserted or removed (where, how many), tracks/voices/staves;
  - the **earliest bar index** from which bar starts changed (meter map), and the **earliest
    position** from which the tempo map changed;
  - whether the form changed (the unrolled path of F1.12 must be recomputed).
- Each edit adds its own entries, so a notification can never be forgotten by a command. The set
  is a `#[non_exhaustive]` structure: layout (F2.16) and playback (F1.13) may ask for more detail,
  which is added without changing the edits.
- The document gains a **revision** number (`u64`) incremented by every transaction, undo and
  redo, so a background reader holding a snapshot (ADR-0022 §7) knows whether it is stale.

### 6. Inserting and deleting bars (U10, U11)

Inserting bars before bar *i*:

- the new bars are empty and take the meter (and the clef, the key, the tempo) in force just
  before *i* — at *i* = 0, those of bar 0; a meter change, a mark or a form marking at the start
  of bar *i* stays with bar *i*, which moves after the new bars;
- position-anchored ranges that span the point (hairpins, pass rules, voltas, repeated sections)
  grow to include the new bars, because their ends are `BarId`s;
- inside a bar-repeat (%) range the insertion is **refused** with a typed error (the new bars would
  become repeated bars of a span they do not fit); the interface asks to remove the repeat first.

Deleting bars *i..j*:

- their events are deleted, with the slurs that end on them, the ties that lead into them
  (`Tie::To` → `None`) and the tuplets left without members;
- **marks that set a state** — meter changes, clef and key changes, tempo marks — that are the
  last in force inside the deleted range move to the start of the first bar after it, unless that
  bar already has its own, so the remaining music keeps its meter, clef, key and tempo;
  **dynamics** inside the range are deleted with the music they belong to;
- ranges that only partly overlap the deletion (hairpins, pass rules, voltas, bar repeats) are
  **clipped**; those that become empty are deleted;
- form markings on the deleted bars are deleted (ADR-0025 §9), and so is every marking whose
  validity depended on them (**cascade**): a jump whose segno, Fine, To Coda or coda is gone, the
  rest of a volta group that lost one volta (its repeat barlines stay), a bar repeat whose source
  bars are gone. Everything deleted this way is in the change set, so the interface can say so;
  the whole deletion is one step of undo;
- at least one bar remains: deleting every bar is refused.

### 7. Changing a meter: rewrite the music by default (U8, U9 — D21)

A meter change is one command with a **mode**:

- **`Rewrite`** (the default, as in Dorico, MuseScore, Sibelius and DAWs): the music keeps its
  place in time and the barlines move, from the bar of the change **up to the next meter change**,
  the next free bar (ADR-0018 §3) or the end of the score, whichever comes first. Rules:
  1. Events, marks and form markings of the region are placed by their absolute position into the
     new bars; bars are added or removed at the end of the region as needed (new `BarId`s for new
     bars, existing ones reused in order).
  2. A note that now crosses a barline is split into a **tie chain** (ADR-0022 §4): the first part
     keeps its `EventId`/`NoteId`, the continuation gets new IDs and no performance offsets
     (ADR-0024 §4); a tuplet member is split inside its tuplet (ADR-0024 C4). Existing ties are
     never merged: that would be a "tidy rhythm" command of its own.
  3. If the region does not end on a barline of the new meter, its last bar becomes an
     **irregular bar** (`AlignStart`, ADR-0018 §3) with the remaining length, so that nothing after
     the region moves; at the end of the score the last bar keeps the full length of the new
     meter, with nothing written in its tail.
  4. A **form marking** that would fall inside a bar (a repeat barline no longer on a barline) splits
     that bar into two irregular bars at that point (the way ADR-0025 §1 writes mid-bar repeats),
     so the form and the played path do not change.
  5. Whole-bar rests of the region become rests of the corresponding written values (the silence
     is kept, its notation is new); written values are otherwise not renotated (that is
     engraving and beaming, F2B).
- **`KeepBars`** (Dorico's "Insert mode" counterpart): every bar keeps its contents and offsets,
  only the bar lengths change; bars may become overfull (a valid state, ADR-0022 §4) and the
  music after the change moves in time. O(1) in the voices.

The rules of `Rewrite` are recommended here; their implementation (F1.11c) may make them more
precise, with the owner's approval, as was done for ADR-0025 §8.

### 8. Tests and benchmarks

- **Property tests** (proptest, ADR-0016) on random documents and random command sequences:
  (a) after every transaction `validate` passes; (b) do then undo gives a document **equal to the
  original except the ID counter**, derived indices included; (c) do, undo, redo equals do;
  (d) the incrementally updated indices equal the ones rebuilt from scratch (oracle); (e) a
  transaction that fails leaves the document equal to before.
- Hand-written cases for every rule of §6 and §7, and for the ADR constraints listed in Context.
- **Benchmarks** in batches (ADR-0017), on the normal and heavy scores: entering a note
  end to end (command, edits, indices, change set), undo of a 200-note paste, inserting and
  deleting a bar, a meter change in both modes, a 1 000-update gesture; plus the heap per history
  step. Budget: everything that can be triggered by one keystroke stays well under the 16 ms
  re-layout budget on the heavy score (target ≤ 1 ms for a note).

### 9. Scope of F1.11 (U2)

The **vocabulary** above is decided now, completely. Implemented in F1.11: every edit except the
structure (tracks, instruments, staves, voices), the editor with history, gestures and change
sets, the commands needed by F1.12–F1.18 and named by the ADRs (enter and delete events with tie
chains and free-bar growth, set and remove every mark and form marking, insert and delete bars,
meter changes in both modes). The structural edits and their commands (add an instrument, add a
voice) arrive with the first interface that needs them (F2), using the same mechanism.

### 10. Choices for the owner

| # | Question | Options | Recommended, with its reason |
|---|---|---|---|
| U1 | How a command is represented | **primitive edits that return their inverse, commands as data expanding into them**; trait objects with their own `do`/`undo`; a snapshot of the document per step | **Edits + inverses**: one mechanism for undo and redo, notifications produced by the edits, everything testable and serialisable. Hand-written `undo` per command multiplies the bugs; snapshots cost ≈ 1.4 ms and an O(bars × voices) copy per step on the heavy score, and say nothing about *what* changed |
| U2 | What F1.11 implements | **whole vocabulary decided, everything implemented except tracks/instruments/staves/voices**; everything; only what F1.12–F1.18 need | **The first**: the structure has no user until the interface (no work in reserve, SCALETTA §1.6), the rest is needed by the next steps and by the ADRs' constraints |
| U3 | History limit (D08) | **memory (128 MB, last step always kept)**; number of steps (e.g. 100); unlimited | **Memory**: a step can weigh 50 bytes or 60 MB, so a count bounds nothing. Unlimited breaks the 8 GB reference laptop on long sessions |
| U4 | Continuous edits (D08) | **explicit gestures with compaction**; merging by time window; none (the view previews by itself) | **Gestures**: one undo step per drag, deterministic; a view-side preview would be a private copy of model data (ADR-0003) |
| U5 | Notifications | **a change set returned by each transaction, plus a revision number**; callbacks registered in the model; per-cell version counters polled by consumers | **Change set**: no observers or threads in `core-model`, nothing to forget; counters alone cannot say *what* was removed |
| U6 | History in the project file | **no**; yes | **No**: the file is the score; the edit vocabulary would become file format |
| U7 | Shape of the history | **linear**; tree (branches kept) | **Linear**: what every notation program and DAW offers; a tree needs an interface to browse it |
| U8 | Default of a meter change (D21) | **`Rewrite` up to the next meter change, `KeepBars` as an option**; `KeepBars` by default; always ask | **`Rewrite`**: it is what Dorico, MuseScore and Sibelius do and what a DAW user expects (notes stay where they sound); asking every time slows down entry, the interface can still offer the other mode |
| U9 | Form markings inside a rewritten region | **split the bar there into irregular bars**; stop the region at the first form marking; refuse the change | **Split**: the form and the played path stay exactly as they were, with the representation ADR-0025 §1 already uses |
| U10 | Marks inside deleted bars | **state marks (meter, clef, key, tempo) carried to the next bar, dynamics deleted**; delete all; carry all | **The first**: the remaining music keeps its meaning (pitches, meter, tempo); a dynamic belongs to the deleted music |
| U11 | Form markings made invalid by a deletion | **cascade (delete them too, report them)**; refuse the deletion | **Cascade**: the deletion is one undo step, so nothing is lost; refusing would make the user hunt for the markings first |

### 11. Precisions made while implementing F1.11b (owner, 2026-11-04 and 2026-11-05)

Approved by the owner before the code of F1.11b part 2a (V, W) and part 2b (X1–X10) was written,
as §7 allows for the meter change; X11 when the property test of part 2b found its case. They make
§1, §4, §6 and §9 precise; none of them changes a decision above.

| # | Precision |
|---|---|
| V1 | Commands are a `Command` enum (data), run by `Transaction::execute` inside a transaction (several commands may share one) or by `Editor::execute` in a transaction of their own. They report through the `ChangeSet`, which already names the new IDs. |
| V2 | An event is entered **only into free space**: over another event of the voice it is refused (the local check reports `Violation::Overlap`). Overwriting, as step input does in Dorico and MuseScore, will be a composed command of F2.22 ("delete the range, then enter"). |
| V3 | An event entered across a barline becomes a **tie chain**: it is cut at every barline, each part is written with the longest values first (at most two dots), and the noteheads of a chord are tied pitch by pitch. A rest becomes several rests, not tied. |
| V4 | A tuplet note that would cross a barline is **refused** for now: the split inside a tuplet belongs to the rewrite of §7 (F1.11c) and will be reused here. |
| V5 | An event entered past the end of a **free** bar lengthens the bar (ADR-0022 §4) in the same transaction; in any other bar it is split (V3). |
| V6 | Deleting an event leaves a **gap** (no rest is written: the model allows gaps, and filling them is engraving's job, as Dorico's implicit rests); the slurs on it, the ties leading into it (set to none), the tuplets left without members and the **grace notes that ornament it** go with it. Removing the last notehead of a chord deletes the event. |
| V7 | Changing an event is one command with one aspect per variant (move, value, pitch, add or remove a notehead, tie, articulations, ornament, fermata, arpeggio, performance offsets, staff); a drag sends it inside a gesture. |
| V8 | The musical rules after an event command are checked by a **local check** on the cells (voice, bar) it touched, using a backward tie index (target → sources, §2's "tie indices") and the slur index; never the whole score. The property tests run the full `validate` after every command. |
| V9 | Gestures as in §4, with the per-update rollback: a refused update is reverted alone and the gesture stays open; the tempo map is derived again after each update that needs it. |
| W1 | Moving an event **never splits it**: past the barline the bar becomes overfull, a valid state. Splitting at every step of a drag would create new IDs at every mouse movement and defeat the compaction. |
| W2 | A new pitch on a tied note changes the **whole tie chain**, forward and backward, as MuseScore and Dorico do. |
| W3 | An event that would reach past the last bar is **refused**; step input may add bars and enter the note in one transaction. |
| W4 | Deleting bars **clips** gradual tempo changes like hairpins (the end inside the run moves to the cut point). If the tempo last in force inside the run comes from a gradual change ending inside it, it arrives at the first bar after the run as an **immediate** mark of its target tempo, without text. A gradual change that goes on past the run cannot start the score (ADR-0021): when the run starts the score, the score starts at the tempo the change **arrives at** (the initial mark keeps its text) and the change is deleted. Only the rest of the change plays at the wrong tempo; from its end on the music plays as before, whereas starting at the tempo it started from would leave everything after it wrong up to the next tempo mark (owner's choice). |
| W5 | The cascade of §6 runs the form checks of `validate` (rules 8–11) again and deletes the marking each one reports, until the form is valid. A bar repeat that loses source bars is deleted; one that loses repeated bars keeps a multiple of its span. |
| W6 | "Inside a bar repeat" (§6) means **from the first bar of its source to its end**: inserting there is refused, because it would change what the sign repeats. |
| X1 | Voltas are added **a whole group at a time** (`AddVoltas`), with the end repeats the group lacks (with the group's number of plays); a repeated section with its two barlines (`AddRepeat`). A volta alone is never valid, so adding one at a time could never succeed. |
| X2 | **Removing** a mark or a form marking removes it and then deletes in **cascade** (as W5) the form markings it left invalid: a jump whose segno is gone, the rest of a volta group. |
| X3 | **Adding, moving and changing never delete anything**: a result that breaks a rule is refused with the typed violation of the local check (a hairpin overlapping one of the same scope, a tempo change inside a gradual one, a jump to a missing marker, a duplicate dynamic). |
| X4 | A bar repeat over bars that hold music is **refused**; "empty the bars and put the sign" is a composed command of F2.22, like the overwriting of V2. |
| X5 | Changing a mark is **one command with its whole new description**, keeping its ID and its kind (`ChangeMark`); a drag sends it inside a gesture, and the compaction keeps one old value. Marks are described without IDs (`MarkSpec`): a new one takes its ID from the counter. |
| X6 | Clefs and keys **never change a pitch** (pitches are stored as spelled at concert pitch); respelling after a key change will be a command of its own (transposition or respelling). |
| X7 | A tuplet is **added empty** (incomplete is a valid state; its notes are entered with `EnterEvent`), **changed in place** (ratio, unit, look; its parent kept; refused if its notes would then overlap), and **removed with the events it contains** and the tuplets nested in it; turning its notes into plain values is a renotation of F2B or F2.22. |
| X8 | The values the document has once (initial tempo, key, each staff's clef, numbering style) are **set, never removed**; the initial tempo stays immediate. |
| X9 | The per-bar data that **does not move a barline** (beat grouping, uncounted bar, numbering restart) has its commands in F1.11b; irregular and free bars and meter changes, which do, belong to F1.11c. |
| X10 | The local check extends to the marks: the ends of a slur, the hairpins of its instrument, the tempo marks (one walk), the form (rules 8–11), only when the command touched them; the property test keeps running the full `validate` after every command. |
| X11 | Bars inserted **between two voltas of a group** (the earlier one ending just before the point, the next one starting at it) **join the earlier volta**, and the end repeat that closed it moves to the end of the last new bar (owner's choice, 2026-11-05): otherwise they would fall outside both voltas and the group would be invalid. |

## Alternatives rejected

- **Trait objects with hand-written `do`/`undo`** (classic command pattern): every command carries
  its own inverse logic, so every command is a new chance of an inexact undo; notifications too
  would be hand-written per command.
- **A snapshot per step:** simple and exact, but each step copies O(bars × voices) chunk pointers
  (≈ 1.4 ms on the heavy score) and the derived indices, memory is hard to bound, and the
  "what changed" must be recomputed by diffing two documents.
- **Persistent data structures for the whole model:** already rejected by ADR-0022.
- **Observers inside the model:** callbacks run in the middle of an edit, see half-changed states
  and tie `core-model` to threads.
- **Coalescing by time:** non-deterministic, untestable.

## Consequences

- `core-model` gains `edit`, `command`, `history`, `change`; the builder stays for new documents
  and the file loader (F1.08). `core-time` gains bar insertion/deletion in the meter map and bulk
  edits for both maps.
- Every later feature that changes the document adds edits (if it adds a kind of object) and
  commands; nothing else.
- Proposed split of the step (SCALETTA): **F1.11** this ADR; **F1.11b** the engine (edits,
  transactions, editor, history, gestures, change sets, `core-time` operations) with the commands
  on events, marks, form markings and bars; **F1.11c** the meter change in both modes, whose
  `Rewrite` is the largest single algorithm of the step.
- F1.12 (unrolling) and F1.13 (playback) consume the change sets for their incremental update.
