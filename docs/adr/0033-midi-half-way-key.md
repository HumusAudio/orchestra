# ADR-0033 — One rule for a pitch half way between two MIDI keys: the lower key

- **Status:** Accepted (owner's decision, 2026-11-08, after the codebase review of the same day,
  point E1). The content was approved before the text was written.
- **Date:** 2026-11-08
- **Amends:** ADR-0032 E9 (the key and bend of a note, and the key of a note in a bend conflict).
  Everything else in ADR-0032 stays as it is.

## Context

Two Accepted ADRs gave two different answers to the same question: which MIDI key plays a pitch
lying **exactly half way** between two keys (a quarter tone, C4 + ½ = 60½)?

- ADR-0023 §3 (`Pitch::midi` in `core-model`): the **lower** key bent up by a half (C4 + ½), so
  the bend lies in `(−½, +½]`.
- ADR-0032 E9 (the MIDI file export in `io`): the nearest semitone, **halves up** (C♯4 − ½), so
  the bend lies in `[−½, +½)`.

The same note therefore had one key in the model and another in the exported file, and the export
computed key and bend with its own formula instead of reusing the model's.

### What changes if the export follows ADR-0023 (checked 2026-11-08)

- **Bend range.** The largest bend becomes +½ semitone = `8 192 + 2 048 = 10 240`, well inside
  0 ..= 16 383 and inside the ±2 semitones that every channel sets with RPN 0. Nothing is clamped
  that was not clamped before.
- **The ends of the key range move by half a step, as in `Pitch::midi`.** 127½ becomes playable
  (key 127 bent up), −½ becomes out of range (it would need key −1). Both are reported exactly as
  `Pitch::midi` reports them, so model and file agree on what is playable.
- **Only exact halves change.** Every other pitch keeps its key and its bend, chromatic notes
  (no bend) included. The reference scores of `test-scores` have no microtones, so the M1 file is
  unchanged; the project files (and their golden copies) do not store keys at all.
- **Tests that change:** the unit tests of the encoder's key and bend, and two cases of
  `io/tests/midi_cases.rs` (the bend of a quarter-tone sharp C4, and the key of E4 + ½ in a bend
  conflict). Nothing else reads the rule.

## Decision

1. **A pitch exactly half way between two keys takes the lower key with a bend of +½**, everywhere:
   in the model (`Pitch::midi`, unchanged) and in the MIDI file export. The bend of a note lies in
   `(−½, +½]` semitone, written as `8 192 + round(bend × 4 096)`, bounded to 0 ..= 16 383.
2. **One implementation.** The key and the bend of an exact sounding pitch (a `Rational` in
   semitones, C4 = 60) are computed by one public function of `core-model`, used both by
   `Pitch::midi` and by the export; the export only converts the bend to 14 bits.
3. **Bend conflicts follow the same rule.** When a note must be played under another bend already
   sounding on its channel (`BendRounded`, ADR-0032 E9), it takes the key whose bent sound is
   nearest to its pitch; a tie takes the **lower** key. (ADR-0032 E9 described this case as
   "rounded to the semitone, played without bend": precisely, it is played under the channel's
   current bend, which is no bend only when the channel is at the centre.)

## Rejected alternatives

- **Halves up everywhere** (change `Pitch::midi` instead): it would amend ADR-0023, whose rule is
  older and is the one the owner chose; nothing favours the upper key.
- **Keeping the two rules:** two answers to one question, the inconsistency this ADR removes.

## Consequences

- A quarter-tone sharp C4 is exported as key 60 with bend 10 240 (it was key 61 with bend 6 144).
- ADR-0032's status line records this amendment; its E9 text stays as written, for the history.
- `DOCUMENTAZIONE.md` / `DOCUMENTATION.md` describe the encoder with the new rule.
