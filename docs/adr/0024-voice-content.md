# ADR-0024 — What a voice contains: events, written durations, tuplets, ties, grace notes, markings, performance offsets

- **Status:** Proposed (draft for the owner's review; SCALETTA F1.09). The choices marked
  **Choice C1…C9** in §12 are the owner's to make; the text around them follows Claude's
  recommendations and changes with the answers.
- **Date:** 2026-11-02
- **Refines:** ADR-0004 (the two layers, the hard cases), ADR-0022 §4 and §6 (events, chords,
  spanners: "the details of each kind belong to F1.09"). If the owner accepts the recommendations
  of C2 and C5, this ADR also **amends** ADR-0022 §6 (how tuplets are referenced) and §5 (how
  tempo changes follow the bars); without them it changes no Accepted decision.

## Context

ADR-0022 fixed the container: a voice has one chunk per bar, a chunk is a vector of
`(offset, event)` sorted by offset, a chord is one event with one `NoteId` per notehead, a rest is
an event without noteheads, events never cross a barline, overfull bars are valid, spanners are
either note-anchored (by `EventId`/`NoteId`) or position-anchored (by (bar, offset)). ADR-0004
listed what the written intent holds and how each hard case is represented. ADR-0023 gave the
pitch. F1.08 built the skeleton (tracks → instrument → staves and voices, bar list, the two time
maps). SCALETTA moved the clef and the key signature into this step.

This ADR decides what is **inside** the chunks and next to them: the event kinds, how a written
duration is stored, tuplets, ties, grace notes, articulations, ornaments, fermatas, slurs,
dynamics, hairpins, clefs, key signatures, tempo indications and the performance offsets. It
does not decide the form (F1.10), the commands (F1.11), the playback rules (F1.13) or the
engraving (F2B).

### Sources checked (2026-11-02)

- **MusicXML 4.0.** A note's sounding length is `<duration>`, its written form `<type>` +
  `<dot>`; inside tuplets `<time-modification>` gives the cumulative ratio (actual / normal notes,
  optional normal type) while `<tuplet>` describes the display, with a `number` attribute to tell
  nested tuplets apart
  ([tuplet](https://w3c.github.io/musicxml/musicxml-reference/elements/tuplet),
  [time-modification](https://usermanuals.musicxml.com/MusicXML/Content/EL-MusicXML-time-modification.htm),
  [normal-type](https://w3c.github.io/musicxml/musicxml-reference/elements/normal-type)).
  `<grace>` has `slash`, `steal-time-previous`, `steal-time-following` (percentages) and
  `make-time` ([grace](https://w3c.github.io/musicxml/musicxml-reference/elements/grace/)).
  `<rest measure="yes">` is a whole-bar rest, and readers disagree on whether its `<duration>` or
  the time signature wins ([w3c/musicxml#478](https://github.com/w3c/musicxml/issues/478)).
  `attack`/`release` shift a note's performed start and end in divisions, `dynamics` gives its
  velocity as a percentage of a default forte
  ([note](https://www.w3.org/2021/06/musicxml40/musicxml-reference/elements/note)). Dynamics go
  from `pppppp` to `ffffff` plus `fp`, `sfz`, … and `other-dynamics` as free text
  ([dynamics](https://www.w3.org/2021/06/musicxml40/musicxml-reference/elements/dynamics)).
  Articulations: accent, strong-accent, staccato, tenuto, detached-legato, staccatissimo,
  spiccato, … ([articulations](https://www.w3.org/2021/06/musicxml40/musicxml-reference/elements/articulations));
  ornaments: trill-mark, turn, mordent, inverted-mordent, tremolo (single, or start/stop for a
  two-note tremolo) ([ornaments](https://w3c.github.io/musicxml/musicxml-reference/elements/ornaments)).
  The `number` of `<key>` names a staff; without it the key applies to **all staves** of the
  part; `<clef>` always belongs to one staff
  ([attributes](https://usermanuals.musicxml.com/MusicXML/Content/EL-MusicXML-attributes.htm),
  [clef](https://usermanuals.musicxml.com/MusicXML/Content/CT-MusicXML-clef.htm)). A tempo is
  printed with `<metronome>`/`<words>` and played from a separate `<sound tempo>`, which the
  schema calls necessary for reliable playback: two pieces of data that can disagree
  ([sound](https://repository.data2type.de/MusicXML/v_2.0/html/co.sound.html)).
- **MEI.** A `graceGrp` is attached `pre` (to the preceding event: the *Nachschlag*) or `post`
  (to the following one); `@grace.time` records the time stolen
  ([graceGrp](https://music-encoding.org/guidelines/v4/attribute-classes/att.gracegrp.log.html),
  [att.graced](https://music-encoding.org/guidelines/v4/attribute-classes/att.graced.html)).
- **Dorico** splits a tuplet at a barline by default and offers a *Spans barline* property for
  Renaissance and contemporary music
  ([Steinberg help](https://steinberg.help/dorico/v4/en/dorico/topics/notation_reference/notation_reference_tuplets/notation_reference_tuplets_span_barline_allow_disallow_t.html)).
  **MuseScore** refuses tuplets across barlines ("Tuplet cannot cross barlines"), and its users
  report the message also firing because of rounding in tuplet arithmetic
  ([forum](https://musescore.org/en/node/302309), [forum](https://musescore.org/en/node/119541)),
  a problem exact rationals do not have.

## Decision

All types live in `orchestra-core-model`. Every object something can refer to has its own ID
from the document's counter (ADR-0022 §1): `EventId`, `NoteId`, `TupletId`, `SlurId`,
`DynamicId`, `HairpinId`, `ClefId`, `KeyChangeId`, `TempoMarkId`. Lists that are few per score
(hundreds) are vectors sorted by start; per-event data is inline in the chunks, so that the
copy-on-write of ADR-0022 §7 covers it.

### 1. Events

```text
Event { id: EventId, staff: StaffId, body: Chord | Rest }
Chord { value: NoteValue, tuplet: Option<TupletId>, grace: Option<Grace>,
        notes: one or more Note, articulations: ArticulationSet,
        ornament: Option<Ornament>, fermata: Option<Fermata>, arpeggio: Option<ArpeggioDirection> }
Rest  { value: RestValue, tuplet: Option<TupletId>, fermata: Option<Fermata> }
Note  { id: NoteId, pitch: Pitch, tie: Tie, offsets: PerformanceOffsets }
```

- `staff` is the staff the event is drawn on (ADR-0022 §2); by default the voice's home staff.
- A chord's noteheads are kept sorted by sounding pitch, lowest first (ties broken by step), so
  that two equal chords compare equal and layout never sorts. A chord with no notes is not a
  chord: the type makes it impossible (one inline note plus an optional boxed tail, so a single
  note does not allocate).
- **Articulations, ornament, fermata and arpeggio belong to the chord**, not to a notehead: they
  are drawn once per stem and played on the whole chord. Ties and performance offsets belong to
  the notehead (ADR-0004).
- **Voice invariant:** within one voice the non-grace events do not overlap: each one starts at
  or after the end of the previous one. Gaps are allowed (a second voice that appears for two
  beats). The commands of F1.11 keep the invariant; a `validate` function checks it for the file
  loader (F1.15) and for the property tests. Overfull bars (ADR-0022 §4) remain valid: the
  invariant is about overlap, not about the bar's end.

### 2. Written durations: stored as note values, never as numbers

```text
NoteValue { base: BaseValue, dots: u8 }      // dots 0..=4
BaseValue = Maxima | Longa | Breve | Whole | Half | Quarter | Eighth | … | 1024th
RestValue = Value(NoteValue) | WholeBar
```

- The **metric duration** (the `Duration` an event occupies in its bar) is **derived**:
  `base × (2 − 2^−dots) × the ratios of every tuplet that contains the event`. It is never stored,
  because storing both the figure and the number would be two copies of one fact (ADR-0003) that
  a single edit could let disagree. Computing it is a few rational multiplications: O(depth of
  tuplet nesting), ≤ 4 (C3).
- A **whole-bar rest** (`RestValue::WholeBar`) lasts its bar's length from the meter map,
  whatever that is. It stays right when the meter changes and in a pickup bar, which is the
  ambiguity MusicXML readers disagree on. It is valid only alone in its voice and bar
  (checked by `validate`). Multi-bar rests are a display of consecutive whole-bar rests (F2B);
  they are not stored.
- An empty chunk means "nothing written in this voice here". Whether the score shows an implied
  rest for it is an engraving rule (F2B), not data.
- Values shorter than a 1024th or longer than a maxima are rejected by the type. A note value
  of zero cannot be expressed.

### 3. Tuplets

```text
Tuplet { id: TupletId, actual: u8, normal: u8, unit: NoteValue,
         parent: Option<TupletId>, display: TupletDisplay }
```

- `actual` notes of `unit` in the time of `normal` notes of `unit` (3 eighths in the time of 2:
  `3:2, unit = Eighth`). The ratio applied to the members is `normal / actual`. The unit is stored
  because the display ("3:2♪") and the check "is the tuplet complete" need it; the ratio alone
  would lose it.
- **Membership is recorded on the events** (C2): every chord or rest stores the innermost tuplet
  that contains it; nesting is the `parent` chain. Tuplets live in **one table per voice**, outside
  the chunks, keyed by `TupletId`. The metric duration of an event is then computed from the
  event alone and the small table, with no index lookup.
- Nesting depth at most **4**, `actual` and `normal` from 1 to 255 (C3). With those bounds the
  product of the ratios, times the longest value with four dots, always fits a `Rational`; the
  arithmetic stays checked anyway (ADR-0015).
- A tuplet whose members do not add up to its total is a valid, **incomplete** state (like an
  overfull bar): it occurs while typing. The score shows it as an error (F2B); playback plays the
  members with their derived durations.
- **Tuplets across a barline** (C4, recommended: allowed). Events still never cross a barline
  (ADR-0022 §4): a note of the tuplet that would straddle the barline is split into tied notes,
  both members of the same tuplet. Nothing in the representation ties a tuplet to one bar, so
  this costs no extra data; inserting or deleting bars elsewhere does not touch the tuplet table.

### 4. Ties

```text
Tie = None | To(NoteId) | LetRing
```

- A tie is a **forward link** from a notehead to the notehead it is tied to (note-anchored,
  ADR-0022 §6). The backward direction ("which note ties into this one") is a derived index.
- The target must sound at the same pitch (`Pitch::sounds_like`, ADR-0023; F♯ tied to G♭ is
  legal), come later, and belong to the **same instrument** (it may be in another voice: piano
  writing does it). The commands check this; `validate` checks it on load.
- `LetRing` is a laissez-vibrer tie with no target.
- A tie chain is played as **one sounding note**, and the performance offsets of the chain are the
  ones on its **first** note (ADR-0004); those on the other notes of the chain are ignored by
  playback and cleared by the commands that create the chain.

### 5. Grace notes

```text
Grace { kind: Acciaccatura | Appoggiatura, attach: Before | After }
```

- A grace chord keeps its written value (the eighth drawn) and has **zero metric duration**: it
  does not move the following events.
- `Before` graces sit at the offset of the event they precede; `After` graces (the
  *Nachschlag*, MEI `pre`) sit at the offset where the event they follow **ends**. At one offset
  the chunk order is therefore: the `After` graces of the previous event, then the `Before`
  graces, then the main event. The sort key in the chunk becomes (offset, slot, insertion order),
  with the slot derived from `grace`: nothing extra is stored.
- How much time a grace takes from its neighbours is a **playback rule** (F1.13), corrected per
  note by the performance offsets. MusicXML's `steal-time-*` attributes are mapped to offsets on
  import (they are a performance, not an intent).
- Graces are not tuplet members.

### 6. Articulations, ornaments, fermatas, arpeggios

- `ArticulationSet` is a bit set (one `u16`) of: accent, marcato (strong accent), staccato,
  staccatissimo, spiccato (stroke), tenuto, portato (detached legato, its own symbol), stress,
  unstress, soft accent. Several can be combined, as written music does.
- `Ornament`: trill (with an optional alteration of the auxiliary note), turn, inverted turn,
  mordent and upper mordent (named after SMuFL's `ornamentMordent` / `ornamentShortTrill`, because
  MusicXML's "mordent"/"inverted-mordent" are known to be confused by importers), single-note
  tremolo with 1–4 strokes. Two-note tremolos change the written durations and are deferred (§11).
- `Fermata`: short, normal, long, very long; on a chord or a rest. Fermatas over barlines belong to
  the form (F1.10).
- `ArpeggioDirection`: none drawn as a plain wavy line, up, down, non-arpeggio bracket.
- Playback turns all of these into durations, velocities and extra notes (F1.13); ornaments
  expanded into notes create **performed** notes that are not written notes (ADR-0004).

### 7. Slurs

```text
Slur { id: SlurId, start: EventId, end: EventId, placement: Auto | Above | Below }
```

- Note-anchored on **events** (chords), one table per instrument (a slur may join two voices of
  the instrument). Slurs may nest (a phrase mark over shorter slurs). A derived index answers
  "which slurs touch this event" (ADR-0022 §6).
- Deleting an end event deletes the slur in the same command (F1.11); `validate` rejects dangling
  ends.

### 8. Dynamics and hairpins

```text
Dynamic { id: DynamicId, at: (BarId, offset), scope: Instrument | Voice(VoiceId),
          staff: StaffId, mark: DynamicMark }
Hairpin { id: HairpinId, start: (BarId, offset), end: (BarId, offset), scope, staff,
          kind: Crescendo | Diminuendo, style: Wedge | Text, niente: bool }
```

- Position-anchored, as ADR-0022 §6 says, in one list per instrument sorted by start. `scope` says
  what they apply to: the whole instrument (the usual case, including a piano's dynamic between
  its staves) or one voice (choral divisi, two players on one staff). `staff` is only where it is
  drawn.
- `DynamicMark`: the levels `pppppp`…`ffffff` (with `mp`, `mf`), the accents `sf`, `sfz`, `sffz`,
  `fz`, `rf`, `rfz`, the combinations `fp`, `sfp`, `sfpp`, `pf`, `n` (niente), and `Other(text)`
  for anything else (*subito p*, *più f*), with an optional level for playback. The standard marks
  are Italian by convention in every language and are not translated; free text is user content.
- A hairpin's end is exclusive: a hairpin up to (bar 5, 0) stops at the barline.
- Playback (F1.13) turns them into velocities and controller curves; the per-note velocity
  offsets are deltas on top (ADR-0004), so changing a dynamic still changes the notes.

### 9. Clefs and key signatures: per staff and per document, not in the voices

A staff carries several voices and a voice may move between staves, so neither clefs nor keys can
live in a voice's chunks.

- **Clefs:** one list **per staff**, sorted by (bar, offset), whose first entry is at (bar 0, 0)
  and cannot be removed. `Clef { sign: G | F | C | Percussion, line, octave: −2..=2 }` (the
  treble clef with an 8 below is G, line 2, octave −1). A clef at (bar, offset) applies to the
  events of that staff from that offset on, grace notes at the same offset included.
- **Key signatures (C6, recommended: K1):** one list in the **document**, sorted by (bar, offset),
  with the first entry at (bar 0, 0), holding the **concert** key (`Key`, ADR-0023 §5), plus a
  rare per-instrument override list (polytonal writing, an instrument "without key signature" such
  as horns in older scores or unpitched percussion). A transposing instrument's written key is
  derived (ADR-0023 §7), never stored.
- Clef and key changes have IDs (`ClefId`, `KeyChangeId`) like everything else that can be
  selected.

### 10. Performance offsets

```text
PerformanceOffsets { onset: Duration, length: Duration, velocity: i32 }   // all zero by default
```

- Inline on every notehead (C7): no allocation, copy-on-write with the chunk. ≈ 40 bytes more per
  note (≈ 180 bytes per note today, ADR-0022).
- `onset` and `length` are **deltas in musical time** (C8, recommended), signed `Duration`s
  (ADR-0018 §1): a note played 1/64 early has `onset = −1/64`. A tempo change therefore scales
  the human feel with the tempo, and the one tempo map stays the only bridge to seconds
  (ADR-0005). The millisecond variant of the tolerance threshold (ADR-0004) converts through the
  tempo map at the note's position; it does not change what is stored.
- `velocity` is a signed delta on the **16-bit velocity scale of MIDI 2.0** (MIDI 1.0's 7 bits
  are a projection of it), so high-resolution input loses nothing.
- **Pass-specific offsets** (ADR-0004: a note in the second pass of a repeat) need the passes of
  F1.12. They will be a separate table per instrument keyed by (`NoteId`, pass index), empty
  until then. The file format of F1.15 reserves it.
- Per-note controller data (ADR-0004) is deferred to the first instrument that needs it (F3).

### 11. Tempo indications (C5)

Recommended: **T2, tempo marks as the written intent, the tempo map derived from them.**

```text
TempoMark { id: TempoMarkId, at: (BarId, offset), text: Option<String>,
            metronome: Option<Tempo> shown or hidden, change: Immediate | Gradual { to … } }
```

- The written tempo indication ("Allegro ♩ = 132", "poco a poco accel.") is position-anchored like
  a dynamic, at (bar, offset), in one document list. The `TempoMap` of ADR-0020/0021 becomes a
  **derived cache**, rebuilt from the marks with the bulk edit of ADR-0022 §8 (≈ 0.6 ms for 250
  segments, F1.05b numbers) after any command that changes the marks or where bars start.
- Because the marks are bar-relative, a command that inserts a bar no longer has to move the
  tempo changes: they follow the bars by construction. This **replaces ADR-0022 §5** (and its
  property test becomes unnecessary) and is why this needs the owner's approval.
- A tempo change without printed text (a DAW user's tempo automation) is a mark with no text and
  a hidden metronome.

### 12. Choices for the owner

| # | Question | Options | Recommendation |
|---|---|---|---|
| C1 | Scope of this step | (a) as §1–§11; (b) also ottava lines, pedal, glissandi | **(a)**. Ottavas are display only, because pitches are stored at concert pitch (ADR-0004): they belong to F2B. Pedal and glissandi go with the playback rules that use them (F1.13 or later) |
| C2 | Tuplet membership | **T-A** as ADR-0022 §6 says: the tuplet stores its first and last event, members are derived (the events of the voice between them); **T-B** each event stores its innermost tuplet | **T-B**: the metric duration of every event (read by layout and playback for every event) depends on it, and a stored link avoids a derived "event → tuplet" index and the ambiguity of "between" with grace notes and other voices. Amends ADR-0022 §6 for tuplets only |
| C3 | Tuplet nesting and ratio limits | depth 2 / **4** / 8; terms up to 255 | **4** and **255**: nesting deeper than three levels is rare even in complex contemporary notation; with these bounds no product overflows. Raising them is a constant and a test, not a format change |
| C4 | Tuplets across barlines | **X1** forbidden (MuseScore); **X2** allowed, straddling notes split into tied notes inside the tuplet (Dorico's "spans barline"); **X3** allowed by the types, refused by the commands for now | **X2**: it costs no extra data with T-B, and Renaissance and contemporary music need it. X3 would be "work in reserve" the other way round |
| C5 | Tempo text and tempo map | **T1** the text is an annotation that points to a tempo map entry (two lists kept in step by the commands); **T2** marks at (bar, offset) are the source, the tempo map is derived (§11); **T3** free text, unrelated to the tempo map (MusicXML's `<words>` + `<sound>`) | **T2**: one source of truth for what is printed and what is played, and the inserted bars no longer need ADR-0022 §5's shift. Cost: a rebuild of the tempo map per tempo-affecting edit (≈ 0.6 ms). T3 lets the score say 120 and play 90 |
| C6 | Key signatures | **K1** a document list + per-instrument overrides; **K2** a list per instrument only (a global key change = one entry per instrument); **K3** per staff (MusicXML's `number`) | **K1**: a key change is one fact for the whole score; overrides are rare. Per-staff keys (a different signature for each hand of a piano) can be added to the overrides later |
| C7 | Where offsets live | **inline** on each notehead; a **side table** per voice keyed by `NoteId` | **Inline**: no allocation, copied with the chunk by copy-on-write; recorded MIDI gives offsets to almost every note |
| C8 | Unit of the onset and length offsets | **musical time** (`Duration`); **seconds** | **Musical time**: one tempo map, the feel follows tempo edits; the millisecond tolerance converts at the note's position |
| C9 | Two-note tremolos, lyrics, chord symbols, fingerings, technique texts | now / later | **Later**, each with its step: lyrics need syllables, verses and alignment; two-note tremolos change written durations |

### 13. Hard cases → tests (F1.09b, F1.09c)

| Case (ADR-0004, ADR-0022) | Representation here | Test |
|---|---|---|
| Enharmonic spelling | `Note.pitch` with spelling, one `NoteId` per head | A chord holding F♯4 and G♭4 keeps two distinct noteheads, sorted, with their spellings |
| Articulations | `ArticulationSet` on the chord | Set/clear round trips; portato is not staccato + tenuto |
| Transposing instruments | concert pitch and concert key only | B♭ clarinet: written pitch and written key derived, nothing stored twice |
| Tuplets | `NoteValue` + `TupletId` + table | A triplet eighth lasts 1/12; 3:2 inside 5:4 gives 1/30 per member; incomplete tuplet is valid; tuplet across a barline with a tied split (if X2) |
| Grace notes | zero metric duration, slot order | Before/After graces do not move the following events; chunk order After < Before < main at one offset |
| Dynamics | position-anchored, scope | Changing a dynamic never touches a note's stored offsets |
| Tied notes | forward links, offsets on the first | Chains across barlines and voices; F♯ tied to G♭ allowed; tie to another pitch rejected by `validate` |
| Overfull bar (ADR-0022) | offsets kept | A meter change to a shorter meter leaves every event and its derived duration unchanged |
| Whole-bar rest | `RestValue::WholeBar` | Its duration follows a meter change and a pickup bar |
| Clef change mid-bar | per-staff list | Lookup of the clef at an event, grace notes at the same offset included |
| Voice invariant | `validate` | Property test: random valid voices pass, any injected overlap fails |

Every test runs against an oracle that shares no code with the library (ADR-0016 §4), lookups are
allocation-free, and the derived duration and the clef/key lookups get batch benchmarks
(ADR-0017 §4).

## Alternatives rejected

- **Storing the metric duration next to the note value** (or only the number, MusicXML's
  `<duration>` without `<type>`): two copies of one fact, or a written form to be guessed.
- **A tuplet as a container of events** (MEI's `<tuplet>` element as a parent): incompatible with
  ADR-0022's flat chunks per bar, and with tuplets across barlines.
- **Grace notes in a separate list per main note:** a second ordered structure to keep in step;
  the slot order in the chunk is enough.
- **Ties as a separate spanner table:** a tie has exactly one start and one end and belongs to the
  starting notehead; a field is smaller and cannot dangle on the start side.
- **Clefs and keys as events in a voice:** a staff has several voices and a voice moves between
  staves; the clef in force would depend on which voice is asked.
- **Absolute offsets** (a copy of the performed onset): forbidden by ADR-0004.

## Consequences

- F1.09 splits as proposed in the briefing: **F1.09b** events, note values, tuplets, ties, clefs
  and keys, `validate`; **F1.09c** grace notes, articulations, ornaments, fermatas, slurs,
  dynamics, hairpins, tempo marks (if T2), offsets, benchmarks.
- If T2 is chosen, F1.08's `DocumentBuilder::new(meter_map, tempo_map)` changes: the builder
  takes tempo marks and derives the map (an API change with every caller updated in the same
  commit, CLAUDE.md §9).
- The derived indices grow: ID → location (ADR-0022), tie targets backwards, slurs per event. All
  are rebuilt at load and updated by the commands; none is saved (ADR-0022 §8).
- Memory per note grows by the inline offsets (≈ 40 bytes); measured in F1.09c against ADR-0007's
  RAM budget on the heavy score of `spikes/model/`.
- Deferred, recorded in SCALETTA: two-note tremolos, lyrics, chord symbols, fingerings and
  techniques, ottavas (F2B), pedal and glissandi (with their playback rules), pass-specific
  offsets (F1.12), per-note controller data (F3), per-staff key signatures.
