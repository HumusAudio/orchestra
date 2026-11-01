# ADR-0023 — Pitch with spelling, intervals, key signatures, transposition

- **Status:** Accepted (owner's decision, 2026-11-01; SCALETTA F1.07, decision D07). The owner
  chose representation R1, microtones M1 and the recommendations on octaves, MIDI, intervals, keys
  and spelling before the text was written, deferred non-traditional key signatures (to be
  remembered), then accepted the draft with the shifted window for the minor mode (§6).
- **Date:** 2026-11-01
- **Refines:** ADR-0004 (pitch stored at concert pitch with its spelling; transposing instruments
  applied for display). Does not change any Accepted decision.

## Context

ADR-0004 stores every note's pitch **as spelled** (F♯4 and G♭4 are different notes) and at
**concert pitch**; a transposing instrument's written pitch is derived for display, so that score
and parts never hold two copies. A pitch edited from the DAW must get its spelling from the key
(ADR-0004, "hard cases"). ADR-0022 gives every notehead its own pitch.

Decision D07 asked whether microtones belong in the pitch type from the start. The owner chose on
2026-11-01 to separate two questions:

- (a) **notation:** which alterations can be *written* (quarter-sharp, three-quarter-flat…);
- (b) **tuning:** what a written note actually *sounds* like (equal temperament, just intonation,
  historical temperaments, non-12 systems).

This ADR decides (a). Tuning (b) is a later rule of `orchestra-playback`; nothing here prevents it.

Sources checked (2026-11-01):

- MusicXML 4.0 spells a pitch as step + `<alter>` + octave. `<alter>` is a **decimal number of
  semitones**: 1 is a sharp, −0.5 a quarter-tone flat
  ([alter](https://www.w3.org/2021/06/musicxml40/musicxml-reference/elements/alter),
  [microtone example](https://www.w3.org/2021/06/musicxml40/musicxml-reference/examples/alter-element-microtones/),
  [semitones](https://w3c.github.io/musicxml/musicxml-reference/data-types/semitones)). A
  transposing instrument is described by **diatonic steps + chromatic semitones + octave change**,
  which keeps enharmonic spelling correct
  ([transpose](https://w3c.github.io/musicxml/musicxml-reference/elements/transpose/)).
- LilyPond stores the alteration as a **rational number** of 200-cent whole tones (a sharp is 1/2),
  according to secondary sources ([x31eq](https://www.x31eq.com/lilypond/),
  [LSR 786](https://wiki.lilypond.community/wiki/LSR_786)).
- MuseScore spells pitches by their **tonal pitch class**, their place on the **line of fifths**,
  with 35 values from F𝄫 to B𝄪 ([MuseScore forum](https://musescore.org/en/node/7542)). The line
  of fifths as a theoretical model is Temperley's (2000), as cited by
  [J. Math. Music 2022](https://informahealthcare.com/doi/full/10.1080/17459737.2022.2044927).
- SMuFL has separate glyph ranges for Stein-Zimmermann quarter tones, Gould arrows, Sagittal and
  Helmholtz-Ellis accidentals ([SMuFL tables](https://w3c-cg.github.io/smufl/latest/tables/)).
  Helmholtz-Ellis and Sagittal mark **just-intonation commas**, which are not fractions of an
  equal-tempered semitone: they belong to tuning (b).
- Existing Rust crates (`tonality`, `tonal`) are small and unmaintained, and `core-model` may
  depend only on `core-time`: the types are our own.

## Decision

The types live in `orchestra-core-model`, module `pitch`. They are small `Copy` values; every
operation is O(1) and allocation-free, and every fallible one returns a typed error.

### 1. Representation: step + alteration + octave (R1)

```text
Pitch { step: Step, alteration: Alteration, octave: i8 }
```

- `Step` is the letter, C D E F G A B, independent of the language of the interface. The names
  shown to the user are localised (Do/Re/Mi in Italian, French and Spanish; C/D/E in English; in
  German **H** is B natural and **B** is B♭); that is the interface's and the glossary's job.
- **Octaves are scientific**: C4 is middle C, as in MusicXML. The octave number changes at C, so
  B♯3 sounds like C4 and C♭4 like B3.
- Two pitches are **equal only if they are spelled the same**. A separate method compares sounding
  pitch (enharmonic equivalence).
- This is how MusicXML and LilyPond store pitch, so import and export are direct. The line of
  fifths is used **inside** the computations (keys, spelling) but is not stored. Storing it (R2)
  makes some arithmetic simpler, but microtonal alterations fit it badly: "three-quarter sharp"
  would be ambiguous between sharp + ¼ and natural + ¾.

### 2. The alteration is a rational number of semitones (M1, decision D07)

- `Alteration` wraps a `Rational` from `core-time`: 1 is a sharp, −1 a flat, 2 a double sharp,
  1/2 a quarter-tone sharp, −3/2 a three-quarter-tone flat, 1/3 a sixth-tone sharp. This covers
  Stein-Zimmermann quarter tones, Gould arrows and the 24-, 36-, 48- and 72-division systems, and
  matches MusicXML's `<alter>`.
- The range is **−3 to +3 semitones** inclusive (triple flat to triple sharp). Anything beyond is a
  typed error. Transposition can produce a triple sharp; nothing produces more.
- Which **glyph** draws an alteration (a Stein-Zimmermann half-sharp or an arrow, for example) is
  an engraving choice (F2B), not part of the pitch.
- **Comma accidentals** (Helmholtz-Ellis, Sagittal) are deferred. They mark just intonation, so
  they will enter as a separate symbolic field together with tuning (b), not as a number of
  semitones.
- Size: a `Pitch` takes 24 bytes (a `Rational` is 16), about 4 MB more than a one-byte alteration
  on the heavy score of ADR-0022: acceptable. A compact encoding can come later behind the same
  type if measurements ask for it.

### 3. Sounding pitch and MIDI (12-TET reference)

- The **exact sounding pitch** is a rational number of semitones on the MIDI scale (C4 = 60):
  `12 × (octave + 1) + natural semitones of the step + alteration`.
- The **MIDI note** is the nearest key plus a bend in semitones. When the pitch lies exactly half
  way between two keys (a quarter tone), the **lower** key is chosen, so the bend is in
  (−1/2, +1/2] and the result is deterministic.
- A key outside 0–127 is a **typed error**, never a value clipped in silence.
- This conversion assumes equal temperament. It is the **reference**: tuning (b) will replace it
  in `playback` without changing the pitch type. How the bend reaches a synthesiser (pitch bend
  per channel, MPE) is a matter for F1.18 and F3.

### 4. Intervals

```text
Interval { steps: i32, semitones: Rational }
```

- `steps` counts diatonic steps (0 = unison, 7 = octave), signed: negative is descending.
  `semitones` is the size, signed the same way. Compound intervals are simply larger values.
- The **quality** is derived. Let `d` be the semitones in the direction of the interval minus
  those of the major or perfect interval with the same number:
  - **perfect class** (unisons, fourths, fifths and their compounds): 0 is perfect, +n is
    n-times augmented, −n is n-times diminished;
  - **major class** (seconds, thirds, sixths, sevenths): 0 is major, −1 minor, +n n-times
    augmented, −(n+1) n-times diminished.
- A unison has no direction, so its sign comes from the semitones: C to C♭ is a diminished unison.
- A microtonal interval has **no standard quality**: the query answers "no quality" and gives the
  deviation in semitones.
- The interval between two pitches is the difference of their diatonic indices
  (`7 × octave + step`) and of their exact sounding pitches.
- **Transposing** a pitch by an interval adds the steps to its diatonic index, then sets the
  alteration so that the sounding pitch moves by exactly the interval's semitones. An alteration
  outside ±3 is a typed error.

### 5. Key signatures (traditional ones only, for now)

```text
Key::Traditional { fifths: i8, mode: Mode }
```

- `fifths` is the number of sharps (positive) or flats (negative), from **−14 to +14**. Keys
  beyond seven (D♯ major, nine sharps) are theoretical but can result from transposition. Whether
  a part shows the simpler enharmonic key is a display policy (F2B).
- `Mode`: the seven diatonic modes (major = Ionian, Dorian, Phrygian, Lydian, Mixolydian,
  minor = Aeolian, Locrian) and **none** (atonal music, empty signature). The mode does not change
  the signature; it names the tonic: major on the line-of-fifths position `fifths`, Dorian
  `fifths + 2`, Phrygian `+4`, Lydian `−1`, Mixolydian `+1`, minor `+3`, Locrian `+5`.
- The alteration a key gives to a step comes from the line of fifths. The natural steps sit at
  F = −1, C = 0, G = 1, D = 2, A = 3, E = 4, B = 5. In a key of `k` fifths, the step at natural
  position `t` takes the spelling `t + 7j` inside the diatonic window `[k − 1, k + 5]`, so its
  alteration is `j` semitones. Beyond seven sharps or flats this gives double sharps or flats.
- `Key` is an **enum with one variant** from the start. **Non-traditional key signatures** (free
  lists of steps and alterations: Bartók, klezmer and Balkan modes, maqam and makam with microtonal
  signatures, as MusicXML allows with `key-step`/`key-alter`/`key-octave`) are **deferred by the
  owner's choice, to be remembered**. They will be a second variant, a trivial file migration.
  The natural moment is F1.17 (MusicXML import, where real files contain them) or the first
  concrete need. They will need their own spelling rule, because §6 relies on the key's place on
  the line of fifths, which a free signature does not have.

### 6. Spelling a MIDI key in a key

For a key of `k` fifths, the spelling of a MIDI key with pitch class `pc` is the line-of-fifths
position `t` with `7t ≡ pc (mod 12)` that lies in a window of twelve consecutive fifths:

- **major, the church modes and "none"** (none uses `k = 0`): `[k − 4, k + 7]`. In C major this
  gives C♯, E♭, F♯, A♭, B♭, the common convention.
- **minor**: `[k − 3, k + 8]`, shifted by one fifth so that the raised leading tone is spelled as
  a sharp. In A minor this gives G♯ rather than A♭ (and E♭ but no longer A♭).

The step and the alteration follow from `t`, and the octave from the key number. The rule is
deliberately simple and documented. Spelling from melodic context (sharps going up, flats going
down) is for MIDI import (F1.17) or F1.14, which may refine it.

### 7. Transposing instruments

- An instrument's transposition is an **interval from written to sounding pitch**, as in MusicXML:
  B♭ clarinet a descending major second, horn in F a descending perfect fifth, piccolo an ascending
  octave, double bass and guitar a descending octave.
- The written pitch is the concert pitch transposed by the inverse interval (§4). The written key
  is the concert key moved along the line of fifths by `7 × semitones − 12 × steps` (+2 for the
  B♭ clarinet). A result beyond ±14 fifths, or a microtonal interval applied to a key, is a typed
  error.
- Whether a part shows concert or transposed pitch stays a view setting (ADR-0004).

### 8. Errors

One error type for the module, with English `Display` texts for developers (the interface maps
variants to Fluent keys, as for `RationalError`):

- an alteration out of range;
- a MIDI key out of range;
- a key out of range;
- a microtonal interval applied to a key;
- arithmetic overflow (`RationalError`).

## Alternatives rejected

- **The line of fifths as the stored form (R2):** simpler arithmetic for keys and intervals, but
  microtonal alterations are ambiguous in it, and every import, export and display needs a
  conversion.
- **No microtones for now (M0):** a one-byte alteration and a later file migration. Rejected by
  the owner: the alteration's form enters intervals, transposition and MIDI, so it is better fixed
  now.
- **Comma accidentals now (M2):** they are tuning (b), not a number of semitones; deferred with it.
- **A MIDI number plus a spelling hint:** loses the written intent that ADR-0004 protects.
- **Clipping out-of-range MIDI keys:** silently wrong playback.
- **Keys limited to seven sharps or flats:** transposition produces theoretical keys that the model
  must be able to hold, even when a part prefers to display the enharmonic one.

## Consequences

- Score and parts hold one pitch per notehead; transposed parts are derived exactly, spelling
  included.
- Tests: property tests against an independent oracle (spelling round trips, transposition up then
  down, interval between two pitches against the transposition, the key's alterations against a
  brute-force table); hand-written cases (B♯3 and C♭4 across the octave, the qualities from doubly
  diminished to doubly augmented, B♭ clarinet, horn in F, piccolo, double bass, double sharps from
  transposition, quarter tones in MIDI with the tie to the lower key); allocation-free tests;
  benchmarks in batches for the MIDI conversion and the transposition, which the playback compile
  calls once per note.
- Recorded for later: non-traditional key signatures (F1.17 or first need), comma accidentals
  together with tuning (b), context-dependent spelling, the enharmonic simplification of transposed
  keys (F2B), localised note names (interface and glossary, with the German H/B).
