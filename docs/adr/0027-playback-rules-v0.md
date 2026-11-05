# ADR-0027 — Playback rules v0: from the written intent to the played notes

- **Status:** Proposed (draft for the owner, 2026-11-05; SCALETTA F1.13, decision D22). Nothing
  here is binding until the owner decides the choices of §12.
- **Date:** 2026-11-05
- **Refines:** ADR-0004 (two layers: written intent and performance, derived plus explicit
  offsets), ADR-0024 §4–§10 (ties, grace notes, articulations, ornaments, fermatas, arpeggios,
  dynamics, hairpins, performance offsets), ADR-0025 §12 (the path of F1.12: pass index, section
  play, bar-repeat sources, pass rules, the time shift).
- **Amends:** nothing.

## Context

F1.12 gives the **path**: the bars in the order they are played. F1.13 turns the written notes
along that path into **played notes**: when each one starts and ends on the unrolled timeline,
at which pitch, how loud. It is where "written intent + explicit offsets" (ADR-0004) becomes
concrete, and its output is what the MIDI export (F1.18) and the audio engine (F3) will receive:
`audio-engine` never sees `core-model` (docs/ARCHITECTURE.md).

Already fixed by Accepted ADRs:

- A note has its written intent (pitch with spelling, rational value, voice, articulations,
  dynamics) and its **performance offsets** (ADR-0024 §10): `onset` and `length` as signed deltas
  in **musical time**, `velocity` as a signed delta on the **16-bit scale of MIDI 2.0**. They are
  stored without limits (F1.09c), so the rules must bound the results.
- Pitches are stored spelled at **concert pitch** with a rational alteration in semitones
  (ADR-0023); 12-TET is the reference of ADR-0023 §2–§3, and the intonation (temperaments, just
  intonation, comma signs) is open (**D22**, to be decided by F1.13).
- Ties are forward links between noteheads (ADR-0024 §4); grace notes have a kind (acciaccatura,
  appoggiatura) and attach before or after their main event (ADR-0024 §5).
- Dynamics apply to an instrument or to one voice (ADR-0024 §8); which one wins for a voice is a
  playback rule, left to F1.13 (SCALETTA F1.09c note).
- Pedal and glissando come with the rules that use them (SCALETTA F1.13); the pass-specific
  offsets table comes with F1.14 (ADR-0025 §12 P6).
- Derived data is a cache, updated incrementally (ADR-0003 §6; ADR-0007: re-layout ≤ 16 ms, the
  same spirit for playback).

The model has, today: articulations `Accent`, `Marcato`, `Staccato`, `Staccatissimo`, `Spiccato`,
`Tenuto`, `Portato`, `Stress`, `Unstress`, `SoftAccent`; ornaments `Trill`, `Turn`,
`InvertedTurn`, `Mordent`, `UpperMordent`, `Tremolo(strokes)`; fermatas `Short`, `Normal`, `Long`,
`VeryLong`; arpeggios `Plain`, `Up`, `Down`, `NonArpeggio`; dynamic levels from *pppppp* to
*ffffff* and the marks `sf`, `sfz`, `sffz`, `fz`, `rf`, `rfz`, `fp`, `sfp`, `sfpp`, `pf`, `n` and
free text with an optional level.

### Sources checked (2026-11-05)

Most public information on the defaults of notation programs comes from their forums, not from
their manuals: the numbers below are what users and support staff report, to be confirmed where
marked.

**Notation programs**

- **Dorico** (Steinberg forum threads; steinberg.help): staccato plays at **50%** of the written
  length and staccatissimo at **25%**, both editable in Playback Options › Timing › Note
  Durations; the shortening applies to the **last note of a tie chain**; Dorico 4.3 added a
  maximum duration for staccato playback. Dynamics become velocity and controller data through a
  project-wide **dynamic curve** (a power, 1 = straight line); MIDI export uses the same
  velocities as playback. Beat stress and humanising exist as playback options. Grace notes
  always precede the next rhythmic note. **Fermatas** lengthen the notes ("hold duration %") and
  insert a gap after them ("gap duration %"), per fermata type in Playback Options › Timing ›
  Pauses and per fermata in the Properties panel; they have no effect inside a gradual tempo
  change. No built-in ornament playback reported. Microtones: through VST3 note expression, or
  since Dorico 5 through **MIDI pitch bend** (the note snapped to the nearest 12-EDO pitch plus a
  bend), which is the only method that survives MIDI export.
- **MuseScore** (forum, developer comments, the `gm` R package that documents MuseScore's table):
  dynamic velocities **ppp 16, pp 33, p 49, mp 64, mf 80, f 96, ff 112, fff 126** (MIDI 1.0
  scale; to confirm in the source). Sforzato is treated as a dynamic, not an articulation.
  Grace notes: the written value is ignored; an **acciaccatura** lasts the smaller of half the
  main note and **65 ms**, an **appoggiatura** half the main note (2/3 of a dotted one, 4/7 of a
  double-dotted one), several graces sharing that time; both played **on the beat**, taking time
  from the main note; grace notes after a note are not played. Hairpins: velocity per note, with
  proposals to shape held notes with CC11/CC2. Microtones through per-note pitch bend.
- **Sibelius** (VSL forum, support replies): staccato at **50%**; an accent **20% louder**,
  editable in the playback dictionary; an "espressivo" layer adds its own variation.

**Exchange formats**

- **MusicXML 4.0** (W3C reference): `<sound dynamics="…">` and the `dynamics` attribute of
  `<note>` are a percentage of a default forte of **90 on the MIDI 1.0 scale** (90 = forte);
  `end-dynamics` is the note-off velocity. `attack` and `release` on `<note>` move the start and
  the end of a note from where the durations put it, in divisions: the performance offsets of
  ADR-0024 §10 (a note ending a tie has no attack, one starting a tie no release). `<duration>`
  keeps the written length; interpretation belongs to `attack`/`release`.
- **Standard MIDI File**: time in ticks per quarter note (the header's division), tempo as
  microseconds per quarter note (meta event FF 51 03, 120 bpm if absent), note-off with its own
  velocity 0–127 (a note-on with velocity 0 is also read as a note-off).
- **MIDI 2.0** (MIDI Association): note-on and note-off velocity on **16 bits** (0–65 535); the
  translation to and from 7 bits is normative in the Bit Scaling and Resolution specification
  (M2-115); a common up-scaling is `v << 9 | v << 2 | v >> 5`, the down-scaling keeps the top 7
  bits, and a MIDI 2.0 note-on of velocity 0 becomes 1 in MIDI 1.0 (0 is not a note-off there).
- **MIDI Tuning Standard**: a semitone in 2¹⁴ = 16 384 steps; with the usual ±200-cent bend range
  a pitch-bend step is ≈ 0.024 cents.

What the sources show: the programs agree on the **order of magnitude** of the rules (staccato
about half, accents about a fifth louder, dynamics on a curve of about 16 velocity steps per
level, short grace notes of tens of milliseconds) and all make them **editable** as playback
options. None plays ornaments well by default. Timing effects that change the clock (fermatas,
pauses) are handled through the tempo, not through the notes.

## Decision (proposed)

A new module `perform` in `orchestra-playback`, next to `unroll`.

### 1. What comes out: played notes, in a neutral form

```text
PlayedNote {
    note: NoteId,            // the first notehead of its tie chain: the link back to the score
    event: EventId,          // its event
    instrument, voice, staff,
    played: u32,             // the played bar (index on the path) where it starts
    pass: u32,               // the pass index of its notes (F1.12; the bar-repeat source's pass)
    start: Position,         // on the unrolled timeline, exact
    end: Position,           // exclusive, exact
    pitch: Rational,         // sounding pitch in semitones, 60 = middle C, exact (microtones kept)
    velocity: u16,           // MIDI 2.0 scale, 1..=65 535
}
```

- **Musical time only**, exact: seconds come from the path's `time_at` (the tempo map plus the
  shift of F1.12), computed by whoever needs them (the MIDI export, the audio engine), so the
  rules never depend on the tempo and a tempo edit does not recompute the notes.
- **Pitch as an exact number of semitones**, not a MIDI note: a quarter-tone stays a quarter-tone.
  It is the model's own `Pitch::sounding` (C4 = 60, ADR-0023 §3); turning it into a key number and
  a bend (Dorico's method, already `Pitch::midi` in the model) or into MPE is the job of F1.18
  and F3.
- **No samples, no buffers, no threads**: those are F3.01's (the audio architecture).
- One list per instrument, sorted by start (then pitch), so that consumers read it in order.

### 2. Ties

A tie chain is **one played note**, from the start of its first notehead to the end of its last,
keyed by the first notehead's ID. Articulations that shorten (staccato…) apply to the **last**
notehead of the chain, as in Dorico; accents to the **first**.

### 3. Articulations (defaults, all to become playback options later, F2.12)

| Articulation | Length (of the written value) | Velocity |
|---|---|---|
| none | 100% | — |
| staccato | 50% | — |
| staccatissimo | 25% | — |
| spiccato | 25% | — |
| tenuto | 100% (not shortened by anything else) | — |
| portato | 75% | — |
| accent | — | +20% |
| marcato | 75% | +30% |
| soft accent | — | +10% |
| stress | — | +10% |
| unstress | — | −10% |

Several articulations combine: lengths multiply, velocity changes add. A percentage of velocity
is of the note's dynamic velocity (§4), bounded to the scale.

### 4. Dynamics and hairpins

- **Levels → velocity** on the 16-bit scale, from a table of the 13 levels (*pppppp*…*ffffff*);
  the proposed anchor is MuseScore's table for the 8 common levels (up-scaled from 7 bits), the
  outer ones extrapolated by the same step, bounded to 1..=65 535.
- **Which dynamic is in force for a voice:** the latest one at or before the note's written
  position on the path, of the voice's scope or of the instrument's; at the same point, the
  voice's wins (the more specific).
- **sf, sfz, sffz, fz, rf, rfz:** the note at that point is louder (by one, two or three levels
  above the level in force), the level in force afterwards unchanged; **fp, sfp, sfpp**: the note
  at that point loud, the following ones at *p* (or *pp*); **pf**: the note at *p*, the following
  ones at *f*; **n**: velocity 1 (or silence); free text: its level if it has one, else nothing.
- **Hairpins:** the velocity of each note starting inside a hairpin is interpolated **linearly in
  musical position** between the level in force at its start and the level at its end (the next
  dynamic at or after the end; without one, one level up for a crescendo, one down for a
  diminuendo). Held notes are not swelled in v0: that needs controller curves, F3's business.

### 5. Grace notes

- **Acciaccatura:** the smaller of half the main note and 1/32 of a whole note, played **on the
  beat**, taking that time from the start of the main note (MuseScore's rule, in musical time so
  that it follows the tempo).
- **Appoggiatura:** half the main note (2/3 of a dotted one), on the beat, taken from the main
  note.
- Several graces before one note share that time equally; **after** graces take their time from
  the end of the previous note.

### 6. Ornaments, arpeggios, fermatas

- **Arpeggios:** the notes of the chord start one after the other, 1/64 of a whole note apart
  (upwards, or downwards for `Down`), all ending together; `NonArpeggio` plays them together.
- **Ornaments** (trill, turns, mordents, tremolo) are **played as the written note** in v0:
  realising them needs the auxiliary note from the key and the accidentals above the sign, and a
  speed; they come with a later step.
- **Fermatas** are **played as written** in v0: Dorico shows that a fermata is a change of the
  clock (a hold and a gap), so it belongs to the unrolled timeline of F1.12 (an amendment of
  ADR-0025 §12 P2) rather than to the notes.

### 7. Pass rules and bar repeats

A note is played only where the path's `plays` says its voice plays (ADR-0025 §12 P4); on a staff
with bar repeats the notes come from the source bar given by `content`, with that source's pass.

### 8. Performance offsets and bounds

The offsets of ADR-0024 §10 are added **after** the rules: `start + onset`, `length × rules +
length offset`, `velocity + velocity offset`. Then the results are bounded: a note never starts
before the start of the performance, lasts at least 1/1024 of a whole note, and has a velocity
in 1..=65 535. The pass-specific offsets of F1.14 will be added there, by (note, pass).

### 9. Incremental update

The unit of recomputation is the **cell** (voice, written bar): a change set naming a cell
recomputes the played notes of that cell in every play of the bar (`plays_of`), plus the notes of
tie chains crossing into or out of it. A change of a dynamic, a hairpin or a pass rule recomputes
the instrument from that point to the next dynamic; a change of the path (F1.12 rebuilt) or of
the meter map recomputes everything. A change of the tempo map recomputes nothing (§1: musical
time only). Budget: one note entered on the heavy score ≤ 1 ms, as for the editor (ADR-0026 §8).

### 10. Intonation (D22)

v0 plays **12-TET** with the written microtonal alterations exact (ADR-0023). Temperaments, just
intonation and comma signs (Helmholtz-Ellis, Sagittal) need instruments that can sound them and a
way to send them (pitch bend, MPE, MIDI Tuning Standard, VST3 note expression), which F3 decides:
they come as a playback layer after F3.04, in a step of their own.

### 11. Tests and benchmarks

Hand-written cases for every rule above on small scores; a property test: the played notes are
sorted, never start before 0, last at least the minimum, have velocities in range, and an
incremental update equals a full recomputation; a benchmark of the full computation on the heavy
score and of one note's update.

### 12. Choices for the owner

| # | Question | Options | Recommended, with its reason |
|---|---|---|---|
| G1 | What F1.13 outputs | **played notes in musical time, exact pitch in semitones, 16-bit velocity**; MIDI-like events with seconds and key numbers | **Musical time and exact pitch**: tempo edits do not recompute the notes, microtones survive, F1.18 and F3 choose their own encoding |
| G2 | Ties | **one played note per chain, shortening on the last note, accent on the first**; one note per notehead | **One per chain**: what every program plays; Dorico's placement of staccato |
| G3 | Articulation defaults | **the table of §3** (staccato 50%, staccatissimo 25%, accent +20%…); other values | **§3**: the values Dorico and Sibelius report, editable later (F2.12) |
| G4 | Dynamic levels | **MuseScore's table up-scaled to 16 bits, outer levels extrapolated**; MusicXML's anchor (f = 90 of 127); a power curve like Dorico's | **MuseScore's table**: concrete numbers users know; MusicXML import/export converts through its percentage anyway |
| G5 | Voice vs instrument dynamic | **latest wins, at the same point the voice's**; the voice's always; the instrument's always | **Latest, then the voice's**: what a player reads |
| G6 | Hairpins | **velocity per note, linear in musical position, target the next dynamic or one level**; also swell held notes | **Velocity per note**: swelling needs controller curves (F3) |
| G7 | Grace notes | **on the beat, acciaccatura min(half, 1/32 whole), appoggiatura half**; before the beat | **On the beat**: MuseScore's rule and the classical reading of an appoggiatura; before-the-beat as an option later |
| G8 | Ornaments | **played as written in v0, realised in a later step**; realised now | **Later**: they need the auxiliary note and a speed, a step of their own |
| G9 | Fermatas | **as written in v0, then a hold and a gap on the unrolled clock (amending ADR-0025 §12)**; lengthen the notes only | **On the clock, later**: lengthening notes alone would make the other voices go on; Dorico treats it as timing |
| G10 | Arpeggios | **1/64 of a whole note between notes, ending together**; a fixed time in ms | **Musical time**: follows the tempo like the rest |
| G11 | Offsets and bounds | **added after the rules; start ≥ 0, length ≥ 1/1024, velocity 1..=65 535** | as proposed: ADR-0024 §10 stores them unbounded |
| G12 | Incremental unit | **the cell (voice, written bar) in all its plays; dynamics from the point; everything on a path change** | **The cell**: the same unit as the editor's change sets |
| G13 | Intonation (D22) | **12-TET with exact microtones in v0; tunings as a playback layer after F3.04**; in F1.13 | **After F3.04**: without an instrument that can sound them they cannot be heard or tested |
| G14 | Split of the step | **F1.13 (this ADR), F1.13b (notes, ties, articulations, grace notes, arpeggios, offsets, pass rules), F1.13c (dynamics, hairpins, incremental update, benchmarks)** | Two parts of about the size of F1.11b's |

## Alternatives rejected (proposed)

- **Seconds in the played notes:** every tempo edit would recompute every note; the path already
  gives the seconds of any written point exactly where the tempo map is.
- **MIDI key numbers as the pitch:** loses the microtones of ADR-0023 at the very first step.
- **Swelling held notes with velocity:** velocity is fixed at the attack; a swell is a controller
  curve, for the instruments of F3.
- **Realising ornaments now with a fixed pattern:** a trill on the wrong auxiliary note (the key
  and the accidental above the sign decide it) would be worse than none.

## Consequences

- `orchestra-playback` gains `perform`: the played notes per instrument, derived from the
  document and the path, updated by cell.
- F1.18 (MIDI export) turns them into ticks, key numbers and bends; F3 into whatever the
  instruments take.
- New SCALETTA entries: ornament realisation, fermatas on the clock (amending ADR-0025 §12),
  intonation after F3.04, the playback options of the defaults in the Preferences (F2.12).
