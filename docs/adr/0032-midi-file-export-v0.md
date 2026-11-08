# ADR-0032 — MIDI file export v0: Standard MIDI File 1.0, written by us

- **Status:** Accepted (owner, 2026-11-08; SCALETTA F1.18). Precisions P1–P3 approved by the owner
  the same day, after the implementation (§ "Precisions made while implementing").
- **Date:** 2026-11-08
- **Amends:** `docs/ARCHITECTURE.md`, the table of allowed internal dependencies (`io` may depend
  on `playback`). It leaves ADR-0027 unchanged: the played notes stay as they are; this ADR only
  says how they are written into a file.

## Context

SCALETTA F1.18 asks for an export of the **played notes** of ADR-0027 into a MIDI file, so that
the owner can *listen* to the model with any player at the end of Phase 1, long before the audio
engine (F3). It is also the first moment to **tune by ear** the provisional times and levels of
ADR-0027 §14 (accents, *sf*, *fp*, *pf*).

What the export starts from (ADR-0027 §1, §14):

- `Performance`: per instrument, the played notes sorted by start. Each has an exact `start` and
  `end` on the unrolled timeline (`Position`, whole notes), an exact sounding `pitch` in
  semitones (`Rational`, C4 = 60, microtones kept), a 16-bit attack `velocity` and an attack
  profile (`hold`, `settle_in` in seconds, `settle_to` on 16 bits).
- `Unrolled`: the path; the time in seconds of a point of a played bar is the tempo map's time
  of the written point plus the bar's **shift** (F1.12), exact in constant-tempo segments and
  approximate (`TimeAt::Approximate`, an `f64`) inside gradual changes (ADR-0021).
- The instrument has a user-given name and nothing that says how it sounds.

**The internal representation is not MIDI 1.0, and must never become it** (owner, 2026-11-08).
The played notes already carry at least the resolution of MIDI 2.0: 16-bit velocity, exact
microtonal pitch, a per-note volume profile. MIDI 1.0 appears **only** in the byte encoder of
this file export, because today nearly every player and sequencer reads only that. The owner's
requirement for the finished program is **MIDI 2.0 by default internally** (between playback and
the instruments, and towards external MIDI devices), which SCALETTA records under F3.01.

### Facts checked (2026-11-08)

- Standard MIDI File 1.0 (MIDI Manufacturers Association): header `MThd` (format, number of
  tracks, division), tracks `MTrk`, delta times as variable-length quantities. Format 1 = several
  tracks played together; by convention the first holds the tempo and time-signature map.
- Tempo meta event `FF 51 03`: microseconds per quarter note, 24 bits (1 ..= 16 777 215, i.e.
  down to ≈ 3.6 quarter notes per minute). Time signature `FF 58 04 nn dd cc bb`: the denominator
  is a power of two (`dd` is its exponent). Marker `FF 06`, track name `FF 03`, end of track
  `FF 2F 00`.
- Note-on with velocity 0 means note-off for most readers: a 7-bit velocity of 0 must never be
  written for a sounding note.
- Pitch bend: 14 bits, centre 8 192; its range is set by RPN 0 (CC 101/100 = 0, CC 6 = semitones,
  CC 38 = cents), default ±2 semitones. CC 11 (expression) scales a channel's volume.
- `midly` 0.5.3 (crates.io, 2026-11-08): Unlicense (allowed by `deny.toml`), last release
  2023-01-01, ≈ 585 000 downloads; a complete reader and writer of SMF.
- A MIDI 2.0 file format exists, the MIDI Association's "MIDI Clip File" (2023), still read by
  few programs; to be verified again when it is taken up (see Consequences).

## Decision

A new module `midi` in `orchestra-io`, next to `project`.

### E1. Format: Standard MIDI File 1.0, format 1

- **Track 0, the conductor:** tempo changes (E4), time signatures and markers (E5), a track name
  (the document's title if it has one, else none).
- **One track per instrument**, in score order, named with the instrument's name (UTF-8 bytes in
  a track-name meta event; readers that expect Latin-1 show odd characters for non-ASCII names,
  which is harmless).
- **MIDI 1.0 only in this file.** Every reduction to 7 bits (velocity, CC 11) or to a key number
  plus bend happens in the encoder, never earlier; nothing in `playback` or `core-model` changes.

### E2. Our own writer, `midly` only to check it

- The writer is ours (≈ 200 lines: variable-length quantities, chunks, events): no runtime
  dependency, bytes fully under our control and **deterministic** (same document, same bytes, on
  every platform), easy to test byte by byte.
- `midly` enters **only in `[dev-dependencies]`** of `io`, as an **independent reader**: the tests
  parse every file we write with it and compare the events with what we meant to write. An
  oracle that is not our own code catches the errors that our own reader would share.
- No running status (every event writes its status byte): a few percent larger, but simpler and
  read correctly by every program.

### E3. Resolution: 960 ticks per quarter note

- `division = 960`. A position `p` (whole notes) is at tick `round(p × 3 840)`, rounded to the
  nearest, halves away from zero. 960 = 2⁶ · 3 · 5: every binary value down to the 256th,
  triplets and quintuplets fall on exact ticks.
- **Absolute positions are rounded, never durations**, so errors do not accumulate: a septuplet
  is off by at most half a tick (≈ 0.26 ms at ♩ = 120) at each note and never drifts.
- A note whose rounded end is not after its rounded start lasts **one tick** (very short grace
  notes).

### E4. Tempo: a tempo step for every stretch, the average tempo of the stretch

MIDI time changes only in steps, while ours can change continuously (gradual changes) and jump
(the shift at a repeat). The export rebuilds the time from the seconds, not from the tempo marks:

1. **Breakpoints** on the unrolled timeline: the start of the path, every tempo change of the
   tempo map (each time it is played), the start of every played bar whose shift differs from the
   previous bar's, and, **only inside gradual changes**, a grid of one 32nd note (120 ticks) plus
   every note start and end there.
2. Each breakpoint gets its time in seconds from the path (`Unrolled::time_at`). Inside gradual
   changes this is the `f64` approximation of ADR-0021, acceptable here because a file export is
   an **output**: nothing flows back into the model (ADR-0015 §8).
3. Each stretch between two breakpoints gets one tempo event, its **average tempo**: the
   microseconds per quarter that make the stretch last exactly what it lasts. To keep rounding
   from accumulating, the microseconds are taken from the **cumulative** time rounded to whole
   microseconds (`round(t_end) − round(t_start)`), divided by the stretch's quarters. A tempo event
   is written only where the value changes.
4. Consequence: in constant-tempo music there is one tempo event per tempo mark; every note start
   inside a gradual change, and every breakpoint, falls on its exact time to within one
   microsecond; between breakpoints the error is bounded by the 32nd grid.
5. A tempo outside 1 ..= 16 777 215 µs per quarter is clamped and reported (E6 warnings): about
   3.6 to 60 000 000 quarter notes per minute, far beyond what the model's tempo marks produce.

### E5. Time signatures: the real length of each played bar

- For every played bar, a time-signature event with **the bar's real length**, written only where
  it differs from the previous one: the meter itself when the bar is regular; otherwise a fraction
  of the same length with a power-of-two denominator (a 5/8 irregular bar, an anacrusis of 1/4, a
  free bar of explicit length). The sequencer's bar lines then match ours, repeats and jumps
  included.
- When the length cannot be written with a power-of-two denominator (a 4/3 meter, a free bar of
  1/3), **no time signature**: a **marker** with the meter as text (for example `4/3`) and a
  warning. The sequencer's bar lines drift from there on, the sound does not.
- `cc` (MIDI clocks per metronome click) = 24, `bb` (32nds per quarter) = 8: the standard values.
- **Lead-in:** if a note starts before position 0 (an acciaccatura before the very first note,
  ADR-0027 §5), the whole file is delayed by the smallest whole number of quarter notes that
  covers it, as a first bar of that many quarters (`k/4`) at the first tempo, with a marker. No
  note is cut.

### E6. Channels: one per instrument, shared beyond 15, with warnings

- Instrument *i* (score order) gets channel `CH[i mod 15]`, where `CH` lists the 16 channels
  without channel 10 (General MIDI drums). Beyond 15 instruments the channels are **shared** in
  that round-robin order; each shared channel is reported.
- **Warnings** are returned with the bytes, never printed: `ChannelShared`, `BendRounded`,
  `PitchOutOfRange`, `MeterNotWritable`, `TempoClamped`, `LeadIn`. Each names the instrument, the
  note or the played bar concerned, so the interface (later) and the example (now) can show them.
  The export never fails for a musical reason; it fails only for an arithmetic error or a
  document the path cannot read (typed errors, as elsewhere).

### E7. Sound: an optional General MIDI program per instrument

- `MidiOptions` maps an instrument to an optional General MIDI program (0 ..= 127). Without one,
  no program change is written and the player chooses (usually a piano).
- The model gets **no** sound field now: what an instrument sounds like is F3's business (internal
  instruments and plugins). The listening example (E11) fills the map by hand for the full preset.

### E8. The attack profile: velocity plus a CC 11 curve on the channel

For a played note with attack level `v`, settling level `s` (16-bit), `hold` and `settle_in`:

- **Note-on velocity** = `max(v, s) >> 9`, then **at least 1** (E1 facts: 0 would be a note-off;
  *niente* is 16-bit 1, which would otherwise become 0).
- **CC 11** follows the profile relative to that peak: `round(127 × level(t) / max(v, s))`, where
  `level(t)` is `v` until `hold`, then linear to `s` in `settle_in`, then `s`. A note without a
  profile (`hold = settle_in = 0`, `s = v`) needs only CC 11 = 127 at its start. Using the ratio of
  levels matches General MIDI players, whose gain grows about as the square of both the velocity
  and CC 11.
- For *pf* (`s > v`) this means the note-on is at the *f* velocity with CC 11 starting low and
  rising: the level is right, the attack's timbre is brighter than a real *p*. Accepted for v0.
- The curve is written in steps of **10 ms** (the times converted with E4's map) and only where the
  7-bit value changes, with an event exactly at the start and at the end of the ramp.
- **The latest attack owns the channel's CC 11:** at every note-on the curve restarts from that
  note's profile, which also changes the level of the notes still sounding on the channel (chords,
  other voices, shared channels). This is the unavoidable limit of MIDI 1.0's per-channel
  controllers; per-note volume is F3's (CLAP and VST3 note expressions, MIDI 2.0).

### E9. Microtones: pitch bend on the instrument's channel

- Each used channel starts with RPN 0 = ±2 semitones (CC 101/100 = 0, CC 6 = 2, CC 38 = 0, then
  RPN null 127/127), CC 11 = 127, pitch bend at the centre.
- A note's key and bend come from its exact pitch: key = the nearest semitone (halves up), bend =
  the remainder (−½ ..< ½ semitone) as `8 192 + round(bend × 4 096)`, bounded to 0 ..= 16 383. A
  bend event is written before a note-on only where the channel's current bend differs.
- **Conflict:** if a note needs a bend different from the one of a note still sounding on the same
  channel, it is **rounded to the semitone** (played without bend) and reported as `BendRounded`.
  A key outside 0 ..= 127 is skipped and reported as `PitchOutOfRange`. One channel per note
  (MPE-like) is left to F3.

### E10. Unisons on one channel

- When a note-on finds its key already sounding on the channel (two voices in unison, a shared
  channel), the encoder writes a note-off and a new note-on (the attack is heard again), and keeps
  a count per key: the **final** note-off is written only when the last note on that key ends. One
  voice never cuts another short.
- At the same tick the order is fixed: note-offs, then controllers, bend and programs, then
  note-ons; within each group by channel and key. On the conductor track: tempo, time signature,
  marker. The bytes are therefore deterministic.

### E11. API and how to run it

- `orchestra_io::midi::export(doc, unrolled, performance, options) -> Result<MidiExport,
  MidiError>`, where `MidiExport` holds the bytes and the warnings; `midi::save` writes them with
  the **same safe write** as the project file (temporary file, sync, rename, sync of the
  directory: ADR-0029), moved from `project` into a module shared by both.
- **Dependency:** `io` may depend on `playback` (it goes downwards and makes no cycle:
  `playback` depends on `core-time` and `core-model` only). `docs/ARCHITECTURE.md` is updated.
- **To listen now:** an example in `io`, `cargo run -p orchestra-io --example export_midi --
  <preset> <size> <out.mid>`, using `test-scores` (allowed: examples see dev-dependencies) and
  printing the warnings. The real command of milestone M1 stays in F1.17.
- Complexity: O(n log n) for n played notes (one sort of the events of each track), memory O(n).

### E12. Tuning by ear

The example also writes a small **listening file** (built in the example with the
`DocumentBuilder`): the same phrase with no accent, the articulation accents (stress, accent,
marcato), *rf*, *sf*, *sffz*, *fp*, *sfp*, *pf*, at ♩ = 60 and ♩ = 120, on long and short notes.
The owner listens and decides changes to ADR-0027 §14, which become an **amendment** to ADR-0027
(approved separately); this ADR changes no value.

### Precisions made while implementing (owner, 2026-11-08)

Found while writing and testing the code, approved by the owner before the step was closed. They
make E4, E5 and E10 precise; the rest of the decision is unchanged.

| # | Precision |
|---|---|
| P1 | **Simultaneous unisons (E10).** Two attacks of one key **at the same tick** on one channel (two voices in unison, two instruments sharing a channel) are written as **one** attack: the second note only counts as sounding, and the key is turned off when the last of them ends. A re-attack needs a note-off at that tick, which the order of E10 (note-offs first) would place before the note-on it must close, leaving the note open. A unison starting **later** is re-attacked as E10 says. The note-off of a re-attack, and the final one, go to the track that holds the key (the latest note-on), so that every track closes the notes it opens even when two tracks share a channel. In the full preset 236 of 5 248 played notes are such unisons. |
| P2 | **Exactness of the tempo (E4).** Every breakpoint falls on its time to within half a unit of 1/960 µs, as E4 says. **Between** breakpoints, in a constant-tempo stretch whose microseconds per quarter note are not an integer (♩ = 72 is 833 333.3 µs), the two tempo steps of the stretch make a note deviate from the exact time by at most Δ/4 units for a stretch of Δ ticks: 21 µs at worst on the whole full preset, far below anything audible. E4's "within one microsecond" holds for breakpoints, among which every note start inside a gradual change. |
| P3 | **The lead-in marker (E5).** Its text is given by the caller (`MidiOptions::set_lead_in_marker`), and by default there is none: the library writes no text of its own in any language (CLAUDE.md §7); the interface will pass its translated text. The lead-in's time signature (`k/4`) and the `LeadIn` warning are always written. |

### Tests and benchmarks

- Byte-exact tests on small hand-built cases (one note, a chord, a tempo change, an irregular bar,
  a microtone, a unison, an accent): the expected bytes written out by hand in the test.
- Every file written in the tests is read back with `midly` (E2) and its events compared with the
  played notes: keys, bends, velocity, start and end ticks, tempo and time signatures.
- Property tests: on random documents, every note start converted back from ticks and tempos to
  seconds is within one microsecond of the path's time (constant-tempo segments) or within the
  32nd grid's bound (gradual changes); the 7-bit velocity is never 0; every note-on has its
  note-off; the file is identical when exported twice.
- Benchmark (Gungraun, ADR-0016): export of the full preset, normal and heavy. Proposed budget:
  the normal size within 100 M instructions (≈ 0.1 s by the rule of F1.16c), the heavy within
  2 G (≈ 2 s, like its opening); to confirm with the first measure.

## Alternatives rejected

- **MIDI 2.0 file (MIDI Clip File) now:** few players read it, and the point of F1.18 is that the
  owner can listen with any player. It comes later as an option (Consequences).
- **`midly` as the writer:** a runtime dependency for ≈ 200 lines, last released in 2023, and its
  writer would make our tests compare `midly` with `midly`.
- **One fixed tempo with ticks in real time:** every note would be exact, but the sequencer's bar
  lines would not match the score, which defeats reading the file in a sequencer.
- **Tempo steps only at the tempo marks:** gradual changes and the shifts at repeats would be
  wrong, by whole beats over a long accelerando.
- **Velocity only, ignoring the profile:** the attack and the settling level could not both be
  heard, and the tuning of §14 would be impossible.
- **One channel per note (MPE-like):** exact bends and per-note volume, but 15 channels run out
  at once with chords and an orchestra; it is the internal path's question (F3), not the file's.
- **Guessing a General MIDI program from the instrument's name:** fragile and dependent on the
  language the user wrote the name in.

## Consequences

- `orchestra-io` gains `midi` and depends on `orchestra-playback`; `docs/ARCHITECTURE.md`
  changes; the safe write moves to a module shared by `project` and `midi`.
- `midly` enters `[dev-dependencies]` of `io` (checked with `cargo deny`; binary size unchanged).
- SCALETTA: F3.01 records the owner's requirement **MIDI 2.0 by default internally** (playback →
  instruments, external devices through UMP with an automatic fallback to MIDI 1.0); a new entry
  for the **MIDI 2.0 file export** (MIDI Clip File) as an option, tied to F3.04, keeping MIDI 1.0
  as the default while few programs read the new format.
- The listening session of E12 may produce an amendment to ADR-0027 §14.
- Key signatures, lyrics, rehearsal marks and per-staff tracks are not exported in v0.
