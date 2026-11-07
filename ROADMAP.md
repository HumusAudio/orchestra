# Roadmap

Read this first at the start of every session (CLAUDE.md §0). It records **where we are**, **what
comes next** and **which decisions are still open**. Decisions themselves live in `docs/adr/`.

## Current status — Phase 0 complete and merged; visual identity added

Phase 0 was merged into `main` (PR #1, 2026-10-29). Then, on `feature/logo-e-temi`: the owner's
logo became the only project logo (outlined SVG in `assets/logo/`, used on the guide covers) and
ADR-0014 fixed light and dark themes from the start. On 2026-10-30 the owner approved the
detailed work plan **`SCALETTA.md`** (Italian, owner's choice): phases, steps with stable IDs,
milestones and the index of open decisions live there. Steps F0.15 and F0.16 are done
(`fix/raccordo-fase-0`, 2026-10-30): every CI run on `main` is green on all three platforms, a
weekly scheduled run was added, and the lint selection was approved. Phase 1 has started: step
F1.01 (`feature/f1-01-adr-razionale`, 2026-10-30) produced **ADR-0015, the rational type**,
accepted by the owner (decision D02 closed). Step F1.02 (`feature/f1-02-bench-test`,
2026-10-31) produced **ADR-0016, test and benchmark tools** (decision D03 closed) and implemented
it: the test-only crate `orchestra-test-support` (per-thread allocation-counting global allocator,
`[dev-dependencies]` only, enforced by `cargo xtask check`), `proptest` for property tests, and
`cargo xtask bench` with Gungraun (instruction counts under Valgrind; CI job on Linux, compile-only
on Windows and macOS). Step F1.03 (`feature/f1-03-rational`, 2026-10-31) implemented
**`Rational`** in `core-time` as ADR-0015 prescribes, with the API choices approved by the owner
that day: base type only (the position/duration newtypes are designed in F1.04), `Debug` only (no
`Display`/`FromStr` until F1.15), English developer-facing `Display` on `RationalError` (the UI will
map variants to Fluent keys), `const fn` constructors and arithmetic. It is covered by property
tests against an independent `i128` oracle (textbook formulas + Euclid), hand-written edge cases
and allocation-free tests; two deliberately injected bugs were caught by the oracle tests. Step
F1.03b (`feature/f1-03b-rational-bench`, 2026-10-31) added the Gungraun benchmarks of `Rational`
(batches of 64, `const` inputs, a case beyond 64 bits) and the **regression gate** of ADR-0017
(Accepted by the owner, 2026-10-31): `cargo xtask bench --compare <rev>` measures the merge
base and the working tree in the same run and fails above +5% instructions, unless the HEAD
commit has the trailer `Bench-regression: accepted`; CI runs it on every branch except `main`. A
deliberately injected regression was caught. The owner added `bench (instruction counts)` to the
required checks of the `main` ruleset (verified with `gh api`, 2026-10-31). The first CI run found
that sharing `target/` between the base export and the current tree made cargo run the base's
benchmark binaries for both; the base now builds in `target/bench-base-target/`. Step F1.04
(`feature/f1-04-meter-map`, started 2026-10-31) is split in two parts. Choices approved by the
owner on 2026-10-31: meters with non-power-of-two denominators are allowed in the type (D04
closed: their engraving is F2B's problem, their MIDI export F1.18's), `Duration` is signed, an
additive meter has at most 8 groups, and basic bar-numbering exceptions are done now (those tied to
voltas go with the form). Reviewing the draft, the owner asked for **free cadenzas** with the
meter suspended: added as `Free` irregular bars (no beats, the meter resumes afterwards), with an
explicit length and counted by default. **ADR-0018 Accepted** (2026-10-31, SCALETTA v0.8). Step
F1.04 (done): the ADR and the types `Position`/`Duration` in `core-time` (`src/position.rs`), with
forwarding/algebra property tests, `compile_fail` doctests for `position + position`, and
allocation-free tests; an injected bug was caught (PR #10, merged). Step **F1.04b**
(`feature/f1-04b-meter-map`, 2026-10-31): before starting, the owner chose that a bar longer than
its meter repeats the beat pattern with the numbers continuing, and asked that the beat grouping
be changeable per bar or per section independently of the written meter (9/8 counted 3+2+2+2):
**ADR-0019** (Accepted by the owner, 2026-10-31) amends ADR-0018 §2 with an optional
`BeatGrouping` (own unit, so hemiola works; up to 16 groups) on a meter change or on one bar; the
default rule stays as it was (7/8 counts seven eighths), and the interface must ask for the
grouping when a mixed meter is entered. Implemented in `core-time`: `Meter`, `BeatGrouping`
(`src/meter.rs`) and `MeterMap` (`src/meter_map.rs`) with all-or-nothing edits that return the
previous value, O(log n) allocation-free lookups (bar ⇄ position, beat ⇄ position, beat range,
displayed number), free bars without beats. Tests: 20 hand-written cases, a property test against a
bar-by-bar naive oracle (two injected bugs caught), an allocation-free test; benchmarks in batches
(≈ 100–1140 instructions per lookup, ≈ 97 000 per edit on a 150-segment map). PR #11; SCALETTA
v0.9 adds step F2.22b (meter entry in the interface) with the mixed-meter requirement (PR #11,
merged). Step **F1.05** (`feature/f1-05-tempo-map`, 2026-10-31): decision **D05** taken by the
owner — gradual tempo changes are linear in **time** (constant acceleration), which makes their
durations rational, so every boundary time stays exact; checked against Friberg & Sundberg 1999
(q = 2 is "linear in time" and, with q = 3, was the best rated; Claude had first misattributed
the runners' model to q = 2 and corrected it). **ADR-0020** (Accepted by the owner,
2026-10-31): tempo as beat unit + rational BPM, tempo map on written positions (the unrolled clock
belongs to `playback`), nearest-sample and containing-frame rounding, exact sample indices even
inside gradual changes via the exact inverse (f64 fallback on overflow, to be measured).
Implemented so far: `Tempo` (`src/tempo.rs`), `Seconds`, `SampleRate`, `FrameRate` with the
rounding rules (`src/clock.rs`), property tests by definition, allocation-free test; an injected
rounding bug was caught. PR #12; SCALETTA v0.10. Step **F1.05b** (`feature/f1-05b-tempo-map`,
started 2026-11-01), split in two parts. Before implementing, a simulation with realistic
metronome marks showed that ADR-0020 §4 could not hold: exact cumulated boundary times overflow
an `i64` fraction after 8–13 gradual changes (an `i128` one after 31–53), and the exact position of
a sample instant inside a gradual change fits an `i64` fraction only in about 25% of cases.
**ADR-0021** (Accepted by the owner, 2026-11-01) amends ADR-0020: boundary times on a grid of
1/705 600 000 s (the "flick"; ≤ half a tick per boundary, deterministic), positions inside gradual
changes rounded to 1/705 600 000 of a whole note, `i128` exact checks for the sample indices, an
optional starting tempo on a gradual change ("Meno mosso, poco a poco accel."), the first change
immediate at 0, an immediate change allowed at the end of a gradual one, back-to-back gradual
changes allowed, `time_at` returning `TimeAt::Exact`/`Approximate`. **First part done**
(2026-11-01): `TempoMap` in `src/tempo_map.rs` with all-or-nothing edits returning the previous
value, segments with start times as integer grid ticks, clamping at segment ends so that both
conversions are non-decreasing, O(log n) allocation-free lookups. Tests: 12 hand-computed cases,
a property test against an oracle sharing no code with the library (own `i128` fractions, linear
scans, trapezoid form) that also bounds the boundary error, an allocation-free test; three
injected bugs caught. The oracle found that times with a denominator foreign to the grid
(`n/3001` s) could overflow the grid rounding: a gcd-reduction fallback now rescues most of them
(1.3% still overflow, with a typed error; never for sample, frame or decimal times). The owner
then approved stating in ADR-0021 §2 that the clamp applies in both directions (SCALETTA v0.11).
**Second part done** (2026-11-01): `sample_at`/`frame_at` return `IndexAt::Exact`/`Estimated`
(owner's choice: the type never hides an estimate), with an `f64` guess checked exactly in `i128`
against the positions where the index's interval begins and ends, at most four corrections, capped
at the next segment's start for consistency with the clamp; `position_of_sample`/`position_of_frame`
go back. Measurement: a 2 h 16 min score with 500 gradual changes and unfriendly marks needed **no
estimate** at 44.1–384 kHz and 24/25/29.97 fps. Index tests: 5 cases, a by-definition property test,
round trip and monotonicity; five injected bugs caught (two only after adding targeted cases).
Benchmarks (`benches/tempo_map.rs`, 250 segments, batches of 64): ≈ 900 instructions per `time_at`,
2000 per `position_at`, 2300 per `sample_at` at constant tempo, 4500 inside a gradual change, 3400
per `frame_at`, 573 000 per edit; the gate against `main` shows no regression. PR #13 (CI green
on the three platforms, merged); SCALETTA v0.12 closes F1.05b and with it F1-A. Step **F1.06**
(`feature/f1-06-model-architecture`, 2026-11-01) in three stages, each approved by the owner:
an analysis of the alternatives (`spikes/model/ANALYSIS.md`), in which the owner chose **voices
per instrument** with a staff per event; a prototype in `spikes/model/` comparing three storages
with the real `core-time` types on generated scores (normal: 100 bars, 10 staves, 8 690 notes;
heavy: 1 000 bars, 30 staves, 255 776 notes), with a test that the three hold the same music after
every operation. On the heavy score, absolute positions in one sorted vector per voice need
≈ 50 ms (≈ 170 M instructions) to insert a bar, lengthen a free bar or change a meter keeping the
bars, while bar-relative offsets in one chunk per bar keyed by stable bar IDs keep every measured
edit under 1.3 ms. **ADR-0022** (Accepted by the owner, 2026-11-01, D06 closed): 64-bit IDs from
one counter, never reused, saved in the file; voices per instrument; bar lengths only in the meter
map plus a list of `BarId`s; events as (bar, offset) in copy-on-write chunks; overfull bars a valid
state; notes split into tie chains at barlines; commands shift the later tempo changes; snapshots
for background readers, the audio thread never reads the model. SCALETTA v0.13 closes F1.06, adds
decision **D21** (default behaviour of a meter change, for F1.11) and writes ADR-0022's
constraints into F1.08, F1.09, F1.11 and F1.16 (PR #14, merged). Step **F1.07**
(`feature/f1-07-pitch`, 2026-11-01): after a search of MusicXML, LilyPond, MuseScore and SMuFL, the
owner separated microtones in notation from tuning and chose step + alteration + octave with a
**rational alteration in semitones** (D07 closed), traditional keys up to ±14 fifths with seven
modes and "none", and **deferred non-traditional key signatures, to be remembered** (MusicXML
thread). **ADR-0023** (Accepted, 2026-11-01, with the shifted spelling window for minor):
scientific octaves, exact sounding pitch and MIDI key with a bend (lower key on a tie, typed error
outside 0–127), intervals as steps + semitones with derived quality, keys on the line of fifths,
spelling of MIDI keys by a window of twelve fifths, transposing instruments as written-to-sounding
intervals. Implemented in `core-model` (`src/pitch.rs`, its first module): 14 hand-written cases,
property and exhaustive tests against an oracle that shares no formula with the library (five
injected bugs caught, one only after adding a tonic oracle), an allocation-free test; benchmarks
≈ 155 instructions per MIDI conversion (after a chromatic fast path, from ≈ 650), 560 per
transposition, 800 per interval and quality, 100 per spelling. SCALETTA v0.14 closes F1.07 and
adds decision **D22** (in which step tuning is done). Errata: ADR-0022, ADR-0023 and a comment
called the MusicXML import "F1.17", which is the phase close; the references now point to the
MusicXML and MIDI import threads of SCALETTA §5. Step **F1.08**
(`feature/f1-08-document-skeleton`, 2026-11-02, PR #16): owner's decisions that day — documents are
created with a public **`DocumentBuilder`** (an existing document will change only through F1.11's
commands, ADR-0003 §4), clef and key signature go to F1.09, a new instrument gets **one voice per
staff**, at most **16 staves and 256 voices per instrument** (technical limits against corrupt
files, not musical ones). Implemented in `core-model`: `src/id.rs` (typed `NonZeroU64` IDs from one
counter, typed error on exhaustion, multiplicative ID hasher) and `src/document.rs` (`Document` with
the meter and tempo maps, `BarList` of `BarId`s with an inverse index, tracks → instrument → staves
and voices with a home staff). Tests: 12 cases, exhaustion unit tests, a property test against a
naive oracle (four injected bugs caught), allocation-free lookups; benchmarks ≈ 42 instructions per
`BarId` → index, 510 per bar start, 280 per voice lookup. SCALETTA v0.15 closes F1.08 and moves clef
and key signature to F1.09. Step **F1.09** (`feature/f1-09-voice-content`, started 2026-11-02),
split as proposed in the briefing into the ADR (F1.09), the events and tuplets (F1.09b) and the
markings and offsets (F1.09c). With the owner's scope (fermatas and ornaments in, lyrics in a
later step of their own) and after a search of MusicXML 4.0, MEI, Dorico and MuseScore, the draft
**ADR-0024** (Proposed) covers event kinds, note values with derived durations, tuplets, ties,
grace notes, markings, slurs, dynamics and hairpins, clefs and keys, tempo marks and the
performance offsets. The owner approved every recommendation (C1–C9) and **ADR-0024 was
Accepted** (2026-11-02). It amends ADR-0022 in two places: tuplet membership is recorded on the
events (§6), and tempo changes become tempo marks at (bar, offset) from which the tempo map is
derived, so they follow inserted bars by construction (§5). SCALETTA v0.16 records the split.
Step **F1.09b** (same branch, 2026-11-02). Before starting, the owner approved four implementation
choices: the initial clef of a staff and the initial key of the document are fields of their owners
(only the changes are marks at (bar, offset)), so `DocumentBuilder::new` takes the initial key and
`add_instrument_track` one clef per staff; `build()` runs `Document::validate` and returns a
`Result`; an instrument's key override is a signature, "no signature" (horns) or "follow the
score"; a tie's target must start strictly later, in the same instrument, with the same sound.
Implemented in `core-model`: `src/event.rs` (note values, rests with the whole-bar rest, tuplets,
ties, notes, chords sorted by sound, events), `src/content.rs` (one `Arc` chunk per bar per voice,
the tuplet table, the derived metric duration), `src/marks.rs` (clefs, key signatures and
overrides, sorted mark lists with an O(log c) lookup), `src/validate.rs` (no overlap within a voice
and bar, whole-bar rests alone, ties), five new ID kinds and the derived index from event and note
IDs to their place. Every caller of the builder was updated. Tests: 16 hand-written cases (ADR-0024
§13), an oracle with its own `i128` fractions for durations, validity and clef lookups (six
injected bugs caught, one only by the oracle), allocation-free lookups and a single allocation in
`validate`. Benchmarks (`benches/voice.rs`, 10 000 events): ≈ 320 instructions per duration while
walking a system, 625 by event ID, 200 per event lookup, 470 per `clef_at`, 555 per
`written_key_at`, ≈ 5.5 M for `validate` of the whole score. Step **F1.09c** (same branch,
2026-11-02) is done in two sessions, as proposed in the briefing. Before starting, the owner
approved the implementation choices E1–E7 (initial tempo as a non-removable mark at bar 0;
text-only tempo marks do not change the tempo map; explicit end of a gradual change; the grace
rules of `validate`; `add_grace_chord` and setters mirroring the future commands; the slur index
built now; a memory test with a ceiling per note), and during the work the **score order**
(position, then slot, then place in the chunk) for ties and slurs, so that an acciaccatura can be
slurred or tied to its main note. **First part (session A, done):** `src/marking.rs` (grace chords
with zero metric duration and the After < Before < main slot order in the chunks, the
articulation bit set, ornaments with 1–4 tremolo strokes, fermatas on chords and rests,
arpeggios, the performance offsets inline in every notehead), `src/slur.rs` (slurs per
instrument sorted by ID, the derived index of the slurs at an event with up to two IDs inline),
`Document::comes_before`, rules 4 (graces have their main event) and 5 (slurs) of `validate`, and
the builder methods. Tests: 9 hand-written cases, an oracle counting in sixteenths (chunk order,
validity, score order on every pair, the index; four injected bugs caught), allocation-free
lookups. Benchmarks (`benches/marking.rs`): ≈ 48 instructions per `slurs_at`, 640 per slur by ID,
1 670 per `comes_before`, ≈ 13.8 M for `validate` of 12 000 events and 4 500 slurs. A first
version of rule 4 in a pass of its own doubled `validate` (+94%); folded into the pass of rule 1
it costs +5% (5.49 → 5.76 M) on the score without graces, and every lookup stayed within ±0.5%,
so no `Bench-regression` trailer was needed. **Second part (session B, done, 2026-11-03).** Before
starting, the owner approved seven implementation choices: the initial tempo mark is a field of the
document with its own ID (no bar exists yet when the builder is created); a mark's offset past the
end of an overfull bar is **clamped to the barline** when the tempo map is derived, and at one
position the later mark in score order wins (a gradual change shrunk to nothing is immediate); the
map is derived again at every `add_tempo_mark`, all or nothing (the ID is peeked and taken only on
success); at most one dynamic per scope at a point, overlapping hairpins of one scope refused by
`validate`; free texts as non-empty `Box<str>`; a `bytes_deallocated` counter in `test-support`
for the memory measurement, with a ceiling of the measure + ≈ 10%. Implemented: `src/dynamic.rs`
(levels, marks, scope and staff, dynamics and hairpins per instrument), `src/tempo_mark.rs` (marks,
the walk that checks them and derives the map), `ScorePoint` in `src/marks.rs`, three new ID kinds,
rules 6 (hairpins) and 7 (tempo marks) of `validate`, `TempoMap::from_changes` in `core-time`
(`TempoMapError::Unsorted`), `AllocationStats::net_bytes` in `test-support`.
`DocumentBuilder::new(meter_map, initial_tempo_mark, key)` now returns a `Result`; every caller was
updated and the initial mark takes ID 1. Tests: 18 hand-written cases, an oracle counting in eighths
for marks and hairpins (seven injected bugs caught, six by the oracle), a bulk-vs-edits property
test in `core-time`, allocation-free reads and `validate` with at most two allocations. **Memory:
257 bytes per note** on a heavy-shaped score (221 169 noteheads, 56.9 MB), ceiling 285.
Benchmarks (`benches/dynamic.rs`): ≈ 985 000 instructions to add a tempo mark among 250 (≈ 0.3 ms),
≈ 620 for the mark in force at a point, ≈ 3.3 M for `validate` with 4 000 dynamics and hairpins;
every existing benchmark within ±0.9%, but the counter's own benchmarks in `test-support` grew
(+5% to +22%, one more field), accepted with the trailer `Bench-regression: accepted`, which must
stay on the **head commit** of the PR. PR #17 (F1.09, F1.09b, F1.09c) merged with CI green on the
three platforms; SCALETTA v0.18. Step **F1.10** (`feature/f1-10-form`, started 2026-11-03), split
as proposed in the briefing into the ADR (F1.10) and the implementation (F1.10b). The owner set the
scope: the core (repeat barlines, voltas, segno, coda, Fine, D.C./D.S.) plus bar repeats (%),
music played only on some passes and jumps written as free text. After a search of MusicXML 4.0,
MEI, Dorico and MuseScore, the draft **ADR-0025** (Proposed) puts the form markings on barlines in
the document, jumps as "to / until / then" with IDs instead of labels, an explicit choice about
repeats after a jump and the pass on which a jump inside a repeat is taken, bar repeats per staff
that store no events, pass rules as ranges, and the unrolling rules F1.12 will follow. After
examples of the numbering styles, the owner chose **one bar-numbering style per document**
(`Continue` by default, `SameAsFirstLettered` 8a/8b, `SameAsFirst` 8/8; after the voltas the count
continues from the longest one) and approved every other recommendation: **ADR-0025 Accepted**
(2026-11-03). SCALETTA v0.19 records it and adds step F1.10b. Step **F1.10b** (same branch, 2026-11-03) in
two sessions. Before starting, the owner approved: later voltas longer than the first continue
the numbering (8a | 8b 9b) and the count after the group resumes from the longest volta; the
style applies on top of `core-time`'s numbering, counting only counted bars, and a restart inside
a group wins; `set_bar_numbering` on the builder (default `Continue`); with voltas `times` must be
2 or n; duplicates are `DuplicateMark`. While writing rule 9 a gap in ADR-0025 §3 emerged
("1. :‖ 2. :‖ 3." could not be written): the owner chose that **every volta but the last ends with
an end repeat** of its section, and ADR-0025 was corrected. **First part (session A, done):**
`src/form.rs` (`PassSet` in one `u64`, `MAX_PLAYS` = 64, `RepeatStart`/`RepeatEnd`, `Volta`,
`BarNumbering`, `BarLabel`, `Form`), rules 8 and 9 of `validate` without allocations,
`Document::bar_label`, the builder methods, two new ID kinds. Tests: 12 hand-written cases, an
oracle that numbers the bars one by one (it found a first volta with only uncounted bars; nine
injected bugs caught, two only after adding the mutations that produce them), allocation-free
reads. Benchmarks (`benches/form.rs`): `bar_label` ≈ 112 instructions with `Continue`, ≈ 13 800
with the lettered style after 25–50 volta groups (O(v log u), ≈ 275 per group; a derived index
could make it O(log g) if F2B's layout benchmarks ever need it), `validate` of 150 voltas
≈ 178 000. **Second part (session B, done, 2026-11-03).** Before starting, the owner approved:
the "until" marker of a jump lies between its target and the jump; one start marker, one end
marker and one jump per bar; `OnPass(n)` only inside a repeated section; a bar repeat covers a
multiple of its span and no form marking lies between its source and its end; pass rules within
their section's plays and not overlapping in a scope; after a D.C. without repeats a section is
pass 1 (for F1.12). These precisions are recorded in ADR-0025 §8. Implemented: `src/jump.rs`
(`Marker`, `MarkerKind`, `Jump` with `JumpSpec` — to / until / then by ID, `RepeatsAfterJump`,
`JumpFires` — and rule 10), `src/bar_repeat.rs` (`BarRepeat` per staff, `PassRule` per instrument
or voice, rule 11), `Document::section_at` (the repeated section of a bar, stopping early and
computing the plays once), four new ID kinds, the builder methods. Tests: 11 hand-written cases, an
oracle for rules 10–11 (the first generator gave 109 valid cases out of 1 024; biased, about a
third; ten injected bugs caught, eight by the oracle), allocation-free reads. Benchmarks:
`validate` with 50 jumps ≈ 933 000 instructions (≈ 15 000 per jump on an extreme score, from
≈ 1.96 M before the lookup was improved); every existing benchmark within ±0.5% of `main`.
PR #18 (F1.10, F1.10b) with SCALETTA v0.20; merged on 2026-11-04, after a re-run of the macOS
job (three runs in a row had been cancelled after 15 minutes without ever getting a runner: a
GitHub queue problem, not the code). Step **F1.11** (`feature/f1-11-commands`, 2026-11-04):
**ADR-0026** (Accepted by the owner, every recommendation U1–U11 approved, D08 and D21 closed):
primitive edits (`Insert`/`Remove`/`Replace` over one enum of object kinds, plus bar and meter
edits) that return their exact inverse, commands as data expanding into them, all-or-nothing
transactions with the derived data (tempo map, indices) updated once at the end, a **linear
history bounded by memory** (128 MB by default, the last step always kept, not saved in the file),
explicit **gestures** with compaction for drags, a **change set** returned by every transaction,
undo and redo plus a document revision number; rules for inserting bars (new bars take what is in
force before them, refused inside a bar repeat) and deleting them (state marks carried to the next
bar, dynamics deleted, ranges clipped, invalid form markings deleted in cascade, at least one bar
left); a **meter change rewrites the music by default** up to the next meter change (music stays
in time, barlines move, notes split into tie chains, an irregular last bar so nothing after moves,
form markings inside a bar split it), with `KeepBars` as an explicit mode. Sources: Dorico, MuseScore,
Sibelius (a course summary only), Logic, Cubase/Nuendo. SCALETTA v0.21 splits the step into F1.11b (the engine
and the commands on events, marks, form markings and bars) and F1.11c (the meter change in both
modes). **F1.11b part 1 done** (2026-11-04, same branch, not yet in a PR): in `core-time`,
`MeterMap::insert_bars`/`remove_bars` with `BarSpan`, exact inverses of each other (oracle property
tests, two injected bugs caught, ≈ 95 000 instructions for four bars); in `core-model`, the modules
`edit` (primitive edits `Insert`/`Remove`/`Replace` over `Object`/`ObjectRef`, `InsertBars`/
`RemoveBars`, `MeterEdit`, `Initial`, each returning its exact inverse, with structural checks only
and the rule "an insertion checks only what no other edit can take away"), `change` (`ChangeSet`),
`editor` (`Editor::transact`, linear history bounded by memory, undo/redo, revision number,
`CommandError`), plus `Document::same_content` and `Document::indices_are_consistent`; history-free
orders (dynamics, hairpins, voltas, bar repeats, pass rules by (point, ID); slur index by ID; events
remember their place in the chunk). Tests in `src/editor_tests.rs` (nine cases and a snapshot-model
property test over random transactions, undos and redos; five injected bugs caught). The tempo-map
"bulk edit" of ADR-0022 §8 already exists (the map is derived from all marks at once); the meter-map
bulk edit is needed only by the rewrite of F1.11c and comes there. Part 2 was split in two by the
owner (2026-11-04). **F1.11b part 2a done** (2026-11-04, same branch, not yet in a PR): before
coding, the owner approved the precisions V1–V9 and W1–W6, now ADR-0026 §11 (events entered only
into free space; tie chains across barlines written with the longest values first; tuplet notes
across a barline refused until F1.11c; free bars grow; a deletion leaves a gap and takes along the
grace notes, slurs and empty tuplets; a move never splits; a new pitch follows the tie chain;
nothing entered past the last bar; deleting bars clips gradual tempo changes and carries the tempo
as an immediate mark (at the start of the score, a gradual change cut in the middle gives its
target tempo: owner's choice after a first draft used its starting tempo); a cascade by the form checks; no insertion from a bar repeat's source to its
end). Implemented in `core-model`: the module `command` (`Command`: `EnterEvent`, `DeleteEvent`,
`ChangeEvent` with `EventChange`, `InsertBars`, `DeleteBars`), `Transaction::execute` and
`Editor::execute` with atomic commands, the gestures (`begin_gesture`/`update_gesture`/
`end_gesture`/`cancel_gesture`, compaction of the `Replace` inverses, tempo map derived live,
`CommandError::GestureOpen`/`NoGesture`), the **local check** `Document::check_cells` (the rules an
event change can break, on the touched cells only; `validate` split into `check_chunk`), the derived
**backward tie index** (`tie_index`, `Document::ties_into`) on the generic `IdRefs<T>` (the former
`SlurRefs`), the internal `ChangeSet::notes_gone`, new typed errors (`PastLastBar`,
`TupletAcrossBarline`, `StartsPastBarline`, `NotWritable`, `InsideBarRepeat`, `NoBarsLeft`). Tests
in `src/command_tests.rs`: 19 hand-written cases and a property test with the full `validate` after
every random command (seven injected bugs caught). **Known limit (W4):** deleting the first bars in
the middle of a gradual tempo change starts the score at the tempo the change arrives at, and the
change is deleted (a gradual change cannot open the score, ADR-0021). **F1.11b part 2b done**
(2026-11-05, same branch): before coding, the owner approved the precisions X1–X10, now ADR-0026
§11 (voltas added a whole group at a time and repeats in pairs; removing cascades, adding, moving
and changing never delete; a bar repeat over music refused; a change is the whole new description
with the same ID and kind; clefs and keys change no pitch; tuplets added empty, changed in place,
removed with their notes; initial values set, never removed; per-bar data that does not move a
barline here, the rest in F1.11c; a local check of the marks). The property test then found that
`InsertBars` put new bars between two voltas of a group, outside both: the owner chose **X11**,
the new bars join the earlier volta and its end repeat moves with them. Implemented in
`core-model`: the module `mark_command` (`MarkSpec`, `NewVolta`) with the commands `AddMark`,
`ChangeMark`, `RemoveMark`, `AddRepeat`, `AddVoltas`, `AddTuplet`, `ChangeTuplet`, `RemoveTuplet`,
`SetInitial`, `SetBarGrouping`, `SetUncounted`, `SetNumberingRestart`; `command::check` with the
new `Document::check_objects`; the in-place replacement of a tuplet in `edit.rs`; new typed errors
`MarkKindMismatch` and `NotAMark`; history steps shrunk to their length (`Step::undoing`). Tests:
16 hand-written cases in `src/mark_command_tests.rs`, the property test drawing from 21 kinds of
command (12 000 sequences passed), seven injected bugs caught; `tests/history_memory.rs` (the
history estimate is exact to the byte but for texts; it fails below 90%); the normal and heavy
scores generated by the shared `tests/common/score.rs`. Benchmarks `benches/editor.rs`, heavy
score, instructions: a note ≈ 8 700, a dynamic ≈ 5 200, undoing a 200-note paste ≈ 385 000,
inserting a bar ≈ 132 000, deleting one ≈ 978 000, one gesture update ≈ 14 500: far under 1 ms
and 16 ms. F1.11b was merged with PR #19 (SCALETTA v0.24). **F1.11c part 1 done** (2026-11-05,
`feature/f1-11c-meter-change`, not yet in a PR): before coding, the owner approved Y1–Y6, now
ADR-0026 §11 (`ChangeMeter` and `SetIrregularBar` with a `MeterMode`; `KeepBars` keeps every bar's
contents; stale per-bar groupings removed; contradictions refused; no change inside a bar repeat;
the bulk edit of the meter map moved to part 2); the property test then found that a shortened bar
could leave a tie or slur going backwards in score order, and the owner chose **Y7**, refuse it;
and that `InsertBars` (part 2a) let a pass rule ending exactly at the insertion point grow out of
its section: **Y8**, ends exactly at the point stay before the new bars (hairpins, pass rules,
gradual tempo changes).
Implemented in `core-model`: the module `meter_command` (`MeterMode::KeepBars`, `check_overfull`),
eight hand-written cases, two more kinds of command in the property test, six injected bugs
caught; benchmarks: a 3/4 at the middle of the heavy score ≈ 5.7 million instructions, a bar made
irregular ≈ 122 000. F1.11c part 2 was split in two by the owner (2026-11-05). **F1.11c part 2a done** (2026-11-05, same
branch): before coding, the owner approved Z1–Z5, now ADR-0026 §11 (the bulk edit of the meter map
`MeterMap::replace_span` with the primitive edit `MeterEdit::Span`; none for the tempo map; tuplet
notes across a barline split inside their tuplet in its own values, which lifts V4 for `EnterEvent`;
one splitting function, `command::split_values`, for `EnterEvent` and `Rewrite`). The error
`TupletAcrossBarline` is gone; `BarSpan` gained its setters. Tests: two cases and an oracle property
test for `replace_span`, three cases for tuplet splits, the command property test now entering
notes inside tuplets; three injected bugs caught (one only by a case written for it). Benchmark:
`replace_span` of 60 bars ≈ 77 000 instructions. **F1.11c part 2b done** (2026-11-05, same branch): before
coding, the owner approved R1–R11, now ADR-0026 §11 (the region up to the next meter change or free
bar; a grid with an irregular last bar so nothing after moves; pickups and free bars kept; the same
algorithm for `SetIrregularBar` and removals; events and marks placed by position, tie chains inside
tuplets, ties and slur ends moved to the last part; whole-bar rests written out; bars split under
form markings; bar repeats refuse). Implemented in `core-model`: the module `rewrite`
(`MeterMode::Rewrite`), `command::replace_event_at`; `ChangeSet::cell` drops recent duplicates and
the editor deduplicates the cells before the local check. Tests: nine hand-written cases, `Rewrite`
in the property test (half the cases without the bar repeat), seven injected bugs caught (five only
by cases written for them). Benchmarks: a rewrite of 60 bars × 41 voices of the heavy score ≈ 118
million instructions (200 million before two fixes found with the profile), 10 bars of the normal
score ≈ 5.3 million; history 2.9 MB. **Open (owner's choice, 2026-11-05):** that rewrite may exceed
the 16 ms of ADR-0026 §8 on the reference laptop: accepted for now, to be measured in real time with
the interface (F2.23) and optimised then if needed (for example with a bulk move of a run of events).
F1.11c was merged with PR #20 (SCALETTA v0.27); phase F1-B is complete.
**F1.12 done** (2026-11-05, `feature/f1-12-unroll`, not yet in a PR): before coding, the owner
approved P1–P8, now ADR-0025 §12 (the path in `orchestra-playback`, derived data; pass index,
section play, exact start and a time shift per played bar; bar-repeat sources per staff; the pass
rules as a query; where each bar is played; the pass-specific offsets table moved to F1.14; whole
rebuild on form, bars or maps; finite by construction). Implemented: the module `unroll`
(`Unrolled`, `PlayedBar`, `StaffContent`, `UnrollError`); tests: sixteen cases (the hard cases of
ADR-0025 §10) and a property test on random forms (40 000 cases passed); eight injected bugs
caught; benchmark ≈ 2.5–3.9 million instructions for 900–1 450 played bars. F1.12 was merged with PR #21 (SCALETTA v0.28).
**F1.13 in progress** (2026-11-05, `feature/f1-13-adr-playback`): research (Dorico, MuseScore,
Sibelius, MusicXML 4.0, Standard MIDI File, MIDI 2.0, MIDI Tuning Standard; the owner asked for the
exchange formats too) and the draft **ADR-0027** (Proposed), with the choices G1–G14 for the owner
(played notes in exact musical time with exact pitch and 16-bit velocity; ties as one note;
articulation and dynamic defaults; grace notes on the beat; ornaments and fermatas deferred;
offsets and bounds; the cell as the incremental unit; intonation D22 after F3.04; the split into
F1.13b and F1.13c). **ADR-0027 Accepted** the same day: the owner changed G4 (dynamics bounded to
*pppp*–*ffff*, more *p*s or *f*s played as the extremes, ten levels spread evenly), G7
(acciaccaturas before the beat, taking time from the previous note; appoggiaturas on the beat) and
G9 (fermatas deferred, then a longer sound, never a slower tempo), and approved the rest.
Decision D22 closed: 12-TET with exact microtones now, tunings after F3.04. **F1.13b done** (2026-11-05,
same branch): before coding, the owner approved N1–N10, now ADR-0027 §13 (ties followed only where
their target sounds next; one note per notehead; shortening on the last notehead, accents on the
first; tenuto with staccato as portato; acciaccaturas from the previous note along the path, after
its `After` graces; arpeggios; chain offsets; *mf* until the dynamics; a whole `Performance`).
Implemented: the module `perform` of `orchestra-playback` (`Performance`, `PlayedNote`,
`PerformError`, `PROVISIONAL_VELOCITY`); fourteen cases and a property test; ten injected bugs
caught. **F1.13c part 1 done** (2026-11-05, same branch): before coding, the owner approved the
precisions H1–H13 and, asking that an accent be louder at the attack only and then go back to its
dynamic at a fixed speed (more slowly after *sf* and the like), the **amendment H14–H17** of
ADR-0027 (§14): every played note has an attack profile (`velocity`, `hold`, `settle_in` in seconds,
`settle_to`), articulation accents are fixed steps instead of percentages, *fp* is *f* held 0.3 s
then *p* in 0.1 s and *pf* the reverse. Implemented: the module `loudness` of `orchestra-playback`
(per-voice timelines along the path, hairpin plans, dynamic accents, the profile, the exact
up-scaling; `INITIAL_VELOCITY` replaces `PROVISIONAL_VELOCITY`); sixteen cases in
`tests/loudness_cases.rs`, the property test with random dynamics, accents and hairpins and an
oracle of the level along the path; nine injected bugs caught. **F1.13c part 2 done** (2026-11-05,
same branch): `ChangeSet::dynamics_changed` in `core-model` (recorded by every insertion and removal
of a dynamic or hairpin, so a removed one still names its instrument); `Performance::update(doc,
path, changes)` in the new module `perform::update`, identical to a full computation: velocities
only, in the stretch where a voice's timeline really changed (`Timeline::changed`), after a change
of the dynamics; a window of played bars around every changed cell, computed by the same
`perform_range` and spliced in place into the sorted lists (now in a total order: start, pitch,
voice, notehead, played bar); everything on a change of the form or the bars, nothing on a tempo
change. `PlayedBar::written_start` added to the path. Tests: six cases with the real editor and a
property test of random command sequences with undo and redo against a full computation; thirteen
mutations, ten real bugs caught, three equivalent; one mutation revealed that a window's bounds could
chase each other for ever (fixed: they only move outwards). Benchmarks `benches/perform.rs` on the
heavy score: the whole performance ≈ 1.67 billion instructions (≈ 0.5 s natively), a note's update
≈ 1.24 million (natively ≈ 0.6 ms mean, 0.96 ms worst, on the development machine), a dynamic's
≈ 2.11 million, after three fixes found with the profile (integer up-scaling, a per-bar summary of
the notes, `Vec::splice` instead of a new list). **To measure on the reference laptop** with the
interface (F2.23), as for the meter rewrite. The editor's `add_dynamic` benchmark grew by 8–9% (≈ 440
instructions per dynamic, the new list of the change set): accepted by the owner with
`Bench-regression: accepted`. SCALETTA v0.32 closes F1.13c and F1.13. **PR #22** (F1.13: ADR-0027,
parts b and c): after four cancelled macOS jobs that never got a runner, the re-run of 2026-11-06 got
one and every check is green (head `6dd728e`, state CLEAN). **Not merged yet:** the merge command was
refused by the session's permission classifier; the owner merges it, or allows the merge.

**F1.14 done** (2026-11-06, branch `feature/f1-14-soglia-tolleranza`, from PR #22's head): **ADR-0028
Accepted**, the DAW move and the tolerance threshold (decision D09 closed: 1/32 in musical time, with a
milliseconds variant). The owner approved Q1–Q10 and added the conscious **performance-only** choice
(offsets of any size, never the score), and the same pair for every DAW edit; for velocity the
reverse (by default only the performance, by choice the written dynamics too, F3). SCALETTA v0.33:
F1.14 split into F1.14 (the ADR), F1.14b and F1.14c, one PR for the whole step. **F1.14b done**
(2026-11-06, same branch): before coding the owner approved T1–T7 (ADR-0028 §10). In `core-model`:
the modules `tolerance` (`Tolerance`, `ToleranceSettings` in the document, `Command::SetTolerance`),
`pass_offsets` (`PassOffsets` per instrument, `Edit::SetPassOffsets`) and `daw_command`
(`Command::MovePlayed` with `MoveMode::ByThreshold`/`PerformanceOnly`: within the threshold an
offset, beyond it a move to the grid with the residual, refusals typed; tie chains moved whole); the
editor clears the pass entries of deleted notes after every command (T5) and compacts them in
gestures; `validate` rule 12; `ChangeSet::tolerance_changed`. In `playback`: pass entries added to the
played notes (T3, T4). Tests: twelve cases and a property test in `src/daw_command_tests.rs`, three
cases in `tests/perform_cases.rs`, DAW moves in `tests/update_props.rs`; eleven injected bugs caught,
one equivalent. Benchmarks: a DAW drag ≈ 23 000 instructions per update on the heavy score; the plain
gesture +2.4%; the whole performance +1.0% and a note's update +1.9% (after a shortcut that skips zero
additions: +7% without it). PR #22 merged on 2026-11-06 (owner's ok), its branch deleted.

**F1.14c done** (2026-11-06, same branch): before coding the owner approved U1–U9 (ADR-0028 §11).
`Command::ResizePlayed` (`length` offset wanted, the rules' `scale`, grid, mode): within the
threshold a length offset, beyond it the written end on the grid and the residual `length − Δ ×
scale`, so the played end lands exactly; `MoveMode::SplitAtBarlines` ("move anyway"); both
rewrite the written tie chain (`rewrite_chain`: cut at barlines, longest values first, the pieces'
IDs reused in order, surplus deleted, slurs and `After` graces following the end). New error
`ScaleNotPositive`. Tests: seven more cases, resizes and splits in both property tests; nine
injected bugs caught (three after cases written for them). A resize drag ≈ 74 000 instructions per
update on the heavy score. **F1.14 complete**, merged with PR #23 (SCALETTA v0.35).

**F1.15 in progress** (2026-11-06, `feature/f1-15-file-format`), split as approved in the briefing
into the ADR (F1.15), the implementation (F1.15b) and the golden files with the migrations
(F1.15c). A spike (`spikes/fileformat/`, `RESULTS.md`) encoded the real normal and heavy scores:
every candidate reads 100 bars in ≤ 30 ms natively; JSON with compact values reads the heavy score
in ≈ 100 ms, RON is 3–5× slower and its generic tree cannot read enums back, postcard is fastest
but not self-describing. Draft **ADR-0029** with the choices S1–S14 for the owner:
JSON with `serde`/`serde_json` only in `io`, one plain uncompressed file in version 1 (a zip as the
alternative; media later, beside the file), deterministic one-record-per-line layout, a file schema
separate from the model with `Document::from_parts`, the version as the first member, canonical
rational strings, compact pitches and note values, no floats, strict loading (unknown fields
refused), limits (1 024 tracks, 1 GiB), rule 13 against meter changes past the last bar, migrations
on `serde_json::Value`, golden and round-trip tests, atomic saving, the extension `.orchestra`.
**ADR-0029 Accepted** the same day (D10 closed): the owner approved S1–S14 as recommended
(one plain JSON file in version 1, compression possible later as a new version; a 1 GiB limit,
since audio and video will live beside the file, never inside), and added two precisions: the
version-1 reader already recognises zip and gzip signatures and answers "made with a newer
Orchestra" (S5); only structural absences are omitted, user settings such as the tolerance are
always written, so a revised program default never changes a saved project (S6). SCALETTA v0.36
adds F1.15b and F1.15c, one PR for the whole step. **F1.15b part 1 done** (2026-11-06, same
branch; split proposed in the briefing and approved): before coding, the owner approved L1–L8, now
in ADR-0029 ("Precisions L1–L8"). In `core-time`: `Display` and a strict `FromStr` for `Rational`
(canonical text only, `ParseRationalError`; the oracle found an error-precedence bug, fixed). In
`core-model`: `from_raw` on every ID type, public constructors of the leaf values with their IDs,
the module `parts` (`DocumentParts` and its containers, `Document::to_parts`,
`Document::from_parts`, which **replays the description through the builder** with the counter in
a script mode handing out the saved IDs, checks IDs unique and issued, runs `validate` and
compares the rebuilt description to refuse lists out of order), `MAX_TRACKS = 1024` in the
builder, rule 13 of `validate` (no meter-map entry past the last bar), new errors `ZeroId`,
`DuplicateId`, `IdNotIssued`, `TooManyTracks`, `NotCanonical`. Tests: the round trip after every
step of every property test of edits and commands (and the refusal of every invalid document they
meet), `src/parts_tests.rs` (nine cases), `tests/rational_text.rs` (cases, round trip, oracle);
seventeen injected bugs caught. Benchmarks `benches/parts.rs`: `to_parts` ≈ 1.2 M / 35.7 M
instructions (normal / heavy), `from_parts` ≈ 29 M / 882 M. **F1.15b part 2 done**
(2026-11-06, same branch): before coding, the owner approved I1–I10, now in ADR-0029 ("Precisions
I1–I10", with three details settled while implementing: `to_bytes` returns a `Result`, the
`Schema` error's line is optional for the conversion's errors, which name the object by ID, and
an omitted default list written explicitly still loads). In `orchestra-io`, the module `project`
(`codec`: the strict text forms and the named enumerations; `schema`: the serde structs of
version 1 with `deny_unknown_fields`; `convert`: schema ⇄ `DocumentParts`; `writer`: the
one-record-per-line formatter; `header`: the version before the rest, with the zip and gzip
signatures), the API `save`/`load`/`to_bytes`/`from_bytes`, `LoadError`/`SaveError`, the safe save
(temporary file in the same directory, sync, rename, directory sync in `src/platform/` behind the
`FileSystem` trait, the crate's first platform code). `serde` 1.0.229 and `serde_json` 1.0.154 in
the workspace (only `io` uses them). Tests: `tests/project_cases.rs` (round trips of a document with
every kind of object, `tests/common/everything.rs`, and of the normal and heavy scores, byte for
byte; layout; BOM and CRLF; every refusal; saving over a file), `tests/project_props.rs` (every
truncation refused, corruption never panicking, random commands round-tripping); seven injected
bugs caught, three after cases written for them. Benchmarks `benches/project.rs`: saving ≈ 42 M / 1.23 G instructions (normal / heavy), loading ≈ 74 M / 2.2 G; natively 5.5 / 168 ms to save and 11 / 396 ms to load, files of 0.84 / 25.4 MB (the heavy load to be measured on the reference laptop, F2.23).
Binary: about +0.6 MB stripped (1.18 MB against 0.55 MB for a probe using only the model). F1.15b
complete (SCALETTA v0.38, which also adds to F2.23 the measurement of the heavy score's opening on
the reference laptop). **F1.15c done** (2026-11-06, same branch): fifteen golden files of version 1
in `crates/io/tests/golden/v1/` (one per kind of saved object, a document with an exhausted
counter, and `everything`), built by `tests/common/golden_docs.rs`, written once by an ignored test
that never overwrites; `tests/golden.rs` compares each with its document (IDs included) and its
bytes, loads every file of every version, refuses every truncation, and feeds random JSON values
into the `everything` file. The migration machinery `src/project/migrate.rs` (the chain on the
generic JSON tree, empty for now, tested with a test-only two-step chain; new
`LoadError::Migration`), wired into `from_bytes` for older versions. `.gitattributes` states LF for
`*.orchestra`. The owner reviewed the golden files by eye on 2026-11-07, as ADR-0029 S12 asks
("hand-reviewed" files): done. F1.15 was merged with PR #25 (2026-11-06), together with ADR-0030
(PR #26: macOS no longer blocks merges).

**Step F1.16** (`feature/f1-16-test-scores`, 2026-11-07). The owner first chose to put the
generator in `test-support`; the analysis found that ADR-0016 §6 forbids that crate to depend on any
workspace crate, so the owner chose option B: **ADR-0031 (Accepted, 2026-11-07)**, a second
test-only crate **`orchestra-test-scores`** (`crates/test-scores`, depending on `core-time` and
`core-model` only, `[dev-dependencies]` only, enforced by the same `cargo xtask check` step; panics
allowed because its input is valid by construction; no `unsafe`). The step was split (SCALETTA
v0.40): **F1.16** moves `crates/core-model/tests/common/score.rs` into the crate as the **plain**
preset, unchanged in output, and migrates its seven includers (`core-model`: `tests/memory.rs`,
`tests/history_memory.rs`, `benches/editor.rs`, `benches/parts.rs`; `io`: `tests/project_cases.rs`,
`benches/project.rs`; `playback`: `benches/perform.rs`) plus `spikes/fileformat`.
`tests/plain_fingerprint.rs` pins the four plain documents (length + FNV-1a of the saved bytes),
values computed both with the new crate and with the old file on `main` (identical). Merged with
PR #27 (2026-11-07), CI green on the three platforms, benchmarks without regressions; SCALETTA
v0.41.

**Step F1.16b** (`feature/f1-16b-full-score`, 2026-11-07). SCALETTA v0.42 split the old F1.16b
into F1.16b (the full preset) and F1.16c (the phase benchmarks). The owner approved a written
specification first: a **plausible** score rather than a dense catalogue (option A), a real
ensemble, and three exceptions (no lettered numbering, no numbering restart, no short last bar).
The **full** preset is in `crates/test-scores/src/full/` (`plan.rs`, `ensemble.rs`, `music.rs`,
`marks.rs`). Each movement has 100 bars: a pickup and A repeated with two voltas, B in 6/8 with a
hemiola, C from the segno, D in 3/4 with a free cadenza and a D.S. al Coda, then a coda. The normal
size is one movement for 10 staves; the heavy size is ten movements in rotating keys for 30
staves. The D.S. uses `RepeatsAfterJump::Play`, so the later movements keep their repeats (see
the open item below). `tests/full.rs` checks coverage (every object kind of
`everything.rs`, every articulation and ornament), the unrolled length (133 / 1 330 bars), the
performance and the save/load round trip. `tests/full_fingerprint.rs` pins the output, and a unit
test checks that every rhythm cell fills its bar. **Density:** 3 939 noteheads in the normal size
and 77 225 in the heavy one, about half of plain. My specification had wrongly estimated it as
similar to plain; the owner kept the realistic density (option A, 2026-11-07), and plain still
covers the dense case. Merged with PR #29 (2026-11-07), CI green on the three platforms; SCALETTA
v0.43.

**Step F1.16c** (`feature/f1-16c-phase-benchmarks`, 2026-11-07). The owner chose (a) and (b), the
size table, and a rule to turn instructions into time. The phase benchmarks are in
`crates/test-scores/benches/phase.rs`, on the full preset, counted in instructions:
- `open`, normal and heavy: bytes → document → unrolled path → played notes;
- `edit`: (a) a note goes up a step from the notation, (b) the same note is dragged from the DAW
  within the threshold. Each is measured as command + path update + played-notes update, on a
  violin I note in the repeat of A, normal and heavy;
- `unroll` and `save`, normal only.

The rule for time is **1 instruction per nanosecond** on the reference laptop, deliberately
pessimistic. With it: open 100 bars 86.6 M ≈ 87 ms (budget 1 s); heavy 2.03 G ≈ 2 s; an edit
0.39–0.49 M ≈ 0.5 ms (budget 50 ms); unroll 0.38 M; save 24 M. A piano note costs about 1.6 M,
because its update window is larger. The CI job grows by about 2.5 minutes. Benchmarks run
outside `cargo xtask bench` need `GUNGRAUN_ENVS=TMPDIR`, or Valgrind fails to start in the
sandbox; `xtask` already does this. **Next:** F1.18 (MIDI export), then F1.17 (phase close, with
the test command of milestone M1).

| Step | What | Status |
|---|---|---|
| 1 | Environment check (CachyOS, Rust, tools) | ✅ |
| 2 | Project name: **Orchestra** | ✅ |
| 3 | Toolchain & system packages (`github-cli`, `cargo-deny`, `cargo-nextest`; Rust 1.99.1) | ✅ |
| 4 | Sandbox: network allowlist; `git push/fetch/pull/ls-remote`, `gh`, `cargo fetch` excluded | ✅ |
| 5 | GitHub repo `HumusAudio/orchestra`, `main` pushed first and protected by a ruleset | ✅ |
| 6 | Workspace skeleton, lints, `deny.toml`, `.gitattributes`, ARCHITECTURE, native-deps | ✅ |
| 7 | CLAUDE.md, ROADMAP, ADR 0001–0013 | ✅ |
| 8 | Hooks: rustfmt (PostToolUse), docs check (UserPromptSubmit + Stop, turn-scoped), commit-msg | ✅ |
| 9 | i18n: Fluent catalogs ×6, glossary (45 researched terms), parity tests | ✅ |
| 10 | Docs: DOCUMENTAZIONE/DOCUMENTATION, Typst guides + template, OFL fonts, `cargo xtask` | ✅ |
| 11 | Every automation verified for real (found and fixed a flaw in the Stop hook) | ✅ |
| 12 | CI on Linux/Windows/macOS + guide PDFs as artifacts | ✅ (all `main` runs green on the 3 OS; weekly run since 2026-10-30) |
| 13 | Spikes → ADR-0012 *no GUI toolkit* and ADR-0011 *own plugin host layer*, both Accepted | ✅ |
| 14 | Closing: docs, roadmap, PR, green CI, merge | ✅ merged (PR #1) |

## Next phases

Summary only; the detailed plan (steps, estimates, acceptance criteria) is `SCALETTA.md`.

1. **Phase 1 — `core-time` and `core-model`:** rational time type with overflow policy, tempo and
   meter maps with O(log n) lookup, conversions to seconds/samples/frames (benchmarks: first
   `cargo xtask bench`); the document model with two-layer notes (ADR-0004), form (repeats,
   voltas, jumps), commands and unified undo/redo; concrete project file format with version 1
   and its first golden file (ADR-0010); MIDI file export of the performed events (SCALETTA
   F1.18, approved 2026-10-30) so the owner can listen to the model before the audio engine.
2. **Phase 2 — UI foundation and basic notation** (SCALETTA: F2A UI foundation first, then F2B
   notation; order confirmed by the owner 2026-10-30): because ADR-0012 chose no toolkit, this phase
   starts with our own UI layer, and that starts with the **theme system** of ADR-0014 (semantic
   colour tokens, light and dark themes, WCAG contrast test, system light/dark detection): window + input (winit), renderer (tiny-skia, then a measured GPU
   path with wgpu/vello), text shaping and glyph cache, widgets with **AccessKit nodes from day
   one**, focus, text fields with IME, menus (our menu bar on Linux/Windows, muda on macOS),
   native dialogs (rfd), headless rendering → `cargo xtask screenshots`. Then incremental
   engraving of single- and multi-voice staves with Bravura, and the notation view.
3. **Phase 3 — audio engine and timeline:** null/offline backend, real device backends behind
   traits, the reference project and its benchmarks, the unrolled DAW timeline.
4. **Phase 4 — plugin hosting (ADR-0011):** our own hosts on `clap-sys`, `vst3`, `lilv-sys`
   (+ AU via objc2), one bridge process per plugin, out-of-process scanning, shared-memory audio
   with RT signalling and bounded waits, editors in separate windows owned by the bridge.
5. **Phase 5 — video:** video tracks with proxies on the shared timeline.

**Fix of the meter rewrite after a free bar** (2026-11-06, PR #24, merged the same day, from
`main`, while F1.15b was in progress on its own branch): the command property test of F1.15b
found that a `ChangeMeter` in `Rewrite` mode whose region ends at a free bar let the new meter reach
the bars after the free bar, which kept their old music (overfull bars, and a tie pointing
backwards). The owner chose **R12** (ADR-0026 §11): the old meter is set again after the free bars.
Implemented in `rewrite.rs` (`meter_after_free_bars`, step 8), two cases in
`meter_command_tests.rs`, the seed in the proptest regressions; the property test passed 4 000
cases in release.

**CI rule changed (2026-11-06, ADR-0030 Accepted, branch `chore/ci-macos-non-bloccante`):**
GitHub's macOS runners were often unavailable (11 of the last 30 macOS jobs never got a runner),
which stopped every finished pull request. macOS no longer blocks merges when it is only waiting
for a runner (a macOS job that ran and failed still blocks); after a merge, the macOS job of
`main` is followed until green, and a step is complete only then. The owner removes
`check (macos-latest)` from the required checks of the `main` ruleset.

## Open decisions and open items

- **Repeats after a jump, in documents with several movements (seen in F1.16b, owner asked to
  record it on 2026-11-07):** after a jump with `RepeatsAfterJump::Skip` (the default), the
  unrolling skips **every** later repeat to the end of the document, even after the coda
  (`crates/playback/src/unroll.rs`: `skip_repeats` is never reset; ADR-0025 §8 "every section is
  played once"). That is right for one piece. With several movements in one document (a suite or
  a concert in the DAW view) it may not be: a possible rule is that repeats play again once the
  jump's path ends, for example at the coda. **To remember:** the model already lets the user ask
  for the repeats to be **played again after a jump** (`RepeatsAfterJump::Play`, ADR-0025 F4); the
  full preset uses it. Revisit with a new ADR when multi-movement documents arrive (no change
  now).
- **Meter rewrite after a free bar (F1.11c): fixed** by PR #24 (R12, merged 2026-11-06, see above);
  the property test's seed is in `crates/core-model/proptest-regressions/command_tests.txt`.
- **Tolerance threshold default** (ADR-0028, D09 closed): 1/32 note; still to validate with users
  once the DAW view exists.
- **From F1.14b, to keep in mind:** the interface (F3) must expose `MoveMode::PerformanceOnly` (a
  command, a modifier while dragging) and offer it when a written move is refused; the DAW side
  computes the onset from the note's **current** written start, without what arpeggios and
  acciaccaturas add. Pass-offset tables are sorted vectors: fine while per-pass edits are rare, to
  measure if a recording ever fills them; the file format (F1.15) must save the tolerance settings
  and the tables. Velocity from the DAW: default performance only, the choice that reaches the
  written dynamics comes with the velocity editor (F3), intonation offsets with D22 (after F3.04).
- **REMIND THE OWNER (asked on 2026-11-05):** every time and level of ADR-0027 §14 (accent
  attacks, the hold and the settling times of accents, *sf*-like marks, *fp* and *pf*) is a
  provisional value to be **tuned by ear** with the owner. Bring it up as soon as the owner can
  listen: at F1.18 (MIDI export, first listening) and again with the internal instruments of F3;
  they become playback options in F2.12.
- **Possible `Rational` optimisations seen in the first numbers (F1.03b), not urgent:** the
  equal-denominator fast path of the sum saves little because the final gcd dominates; the gcd
  could short-cut when one operand is 1 (integer values, the overflow path at ~455 instructions
  per operation); the 128-bit gcd costs ~5× the 64-bit one. Any change is judged by the gate.
- No `rem` was needed on `Rational` (`checked_ratio` + `floor` in the meter map).
- **Interface requirement (owner, 2026-10-31, ADR-0019):** when a mixed meter (5/8, 7/8, 11/8…)
  is entered, ask at once which grouping to use, listing the options (5/8: 2+3, 3+2, keep 5;
  7/8: 2+2+3, 3+2+2, 2+3+2, keep 7; …), and remind that it can be changed bar by bar later. Written
  into SCALETTA step F2.22b.
- **Non-traditional key signatures** (Bartók, klezmer and Balkan modes, maqam and makam): deferred
  by the owner on 2026-11-01, **to be remembered**; a second variant of `Key` with its own spelling
  rule, at the MusicXML import or at the first concrete need (ADR-0023 §5, SCALETTA §5).
- **"No mode" spelling:** ADR-0023 §6 spells MIDI keys in a key without mode with the C major
  window, whatever its signature; to revisit if open signatures with accidentals turn out common.
- **From F1.11b part 1, to keep in mind:** while a transaction is open the tempo map is the one
  from before (it is derived once, at the end); gestures derive it after each live update (done in
  part 2a); the meter-map bulk edit of ADR-0022 §8 is left to F1.11c (the only
  user); interface choices confirmed by the owner (2026-11-04, SCALETTA v0.22): `Edit`,
  `Object` and `Transaction::apply` are public (raw edits keep only the structure, the musical
  invariants are the caller's), `Transaction::new_id` returns a raw `NonZeroU64` because the typed ID
  constructors stay crate-private, the editor's tests live inside the crate (`src/editor_tests.rs`).
- **Possible model optimisations, not urgent (F1.06 numbers):** a whole-score iteration with
  absolute positions costs ≈ 17 ms on the heavy score (one rational addition per event); a snapshot
  ≈ 1.4 ms (one reference count per non-empty voice-bar). See ADR-0022, Consequences.
- **Concrete project file format**: ADR-0029 Accepted; implementation in F1.15b, golden files and
  migrations in F1.15c.
- **From F1.10b, to keep in mind:** the number of a bar in the lettered styles walks the volta
  groups before it (≈ 275 instructions each), and finding the section of a jump or a pass rule
  walks the repeat barlines: fine for real scores, and a derived index of the sections (rebuilt
  with the commands of F1.11) would make both O(log) if F2B's layout or F1.12 ever need it; the
  commands of F1.11 must keep the form valid when bars are inserted or deleted (a deleted bar takes
  its form markings with it).
- **From F1.09c (session B), to keep in mind:** which dynamic wins when an instrument dynamic and
  a voice dynamic (or hairpin) apply at once is a playback rule for F1.13; the commands of F1.11
  must derive the tempo map again after every edit of the tempo marks or of where bars start
  (inserting a bar, a meter change), and the clamping means an overfull bar can silently move a
  tempo change to its barline, which the interface should show; ID lookups of dynamics, hairpins
  and tempo marks (today only the sorted lists) come with the first command that needs them.
- **From F1.09c (session A), to keep in mind:** one ornament per chord (a set if the repertoire
  ever needs two); an arpeggio spanning the two staves of a piano is one sign per chord today (a
  cross-staff arpeggio line is an engraving question for F2B, or a spanner if it must be one
  object); performance offsets are stored unclamped, so the playback rules of F1.13 must clamp the
  results (start ≥ 0, a minimum length, the velocity scale).
- **`cargo xtask bench --compare` bug (found 2026-11-02, not fixed):** the base commit is exported
  with the commit's file times and built in the reused `target/bench-base-target/`, so after a
  comparison with a *newer* base (e.g. `main`), comparing with an *older* one (e.g. `HEAD` of a
  branch) makes cargo think the exported sources are up to date and reuse the other base's
  library: the base build fails or measures the wrong code. Workaround: `rm -rf
  target/bench-base-target` before switching base. Fix: touch the exported files, or key the
  target directory by the base commit. CI is not affected (one comparison per fresh runner).
- **For F1.11/F1.15 (from F1.08):** whether the model rejects meter map entries and tempo changes
  beyond the last bar (today nothing checks it: the meter map covers infinitely many bars), and a
  cap on the number of tracks when a file is loaded (staves and voices already have one).
- **Possible tempo map optimisations, not urgent (F1.05b numbers):** `sample_at` at constant tempo
  (≈ 2300 instructions) costs more than `time_at` (≈ 900 on mixed points) because the exact time has
  large denominators (boundaries off the grid) and is then multiplied by the rate; computing in
  grid ticks would avoid most gcds. Any change is judged by the benchmark gate.
- **Tempo queries for the interface and the metronome** (the tempo in force at a position, the
  speed at a time inside a gradual change): the segments already keep the written tempi; the API
  comes when a caller needs it.
- **"Region" or "clip"** as the name of a block of material on a track (glossary entry `region`).
- **Glossary entries with `review = true`** (grand staff, clef fr, key signature de, slur fr/es,
  hairpin, part es, grace note, volta, region, send, plug-in, velocity, mute, menu Edit fr): to
  be confirmed by native-speaking musicians.
- **GPU rendering not measured yet** (the development sandbox has no GPU): first item of Phase 2.
- **Spike findings to keep in mind for Phase 4:** LSP Plugins' CLAP and VST3 crashed when loaded
  through truce-rack; VST3 output was silent through truce-rack (cause not investigated); plugin
  round trip p99 1–2 ms with pipes and normal priority must be brought down with RT signalling.
- **Language of project docs:** ADRs, ARCHITECTURE and this ROADMAP are in English;
  `DOCUMENTAZIONE.md` is the Italian counterpart of the developer documentation. Change if the
  owner prefers Italian.

## Notes for future sessions

- `sandbox.excludedCommands` only applies to a command run **on its own**: `cd … && git push`
  and `gh … | grep` stay sandboxed (and `gh` then has no token: anonymous rate limit, no job
  logs). Run `git push`, `gh …` and `cargo fetch` alone; the cwd is already the repository.
  A `gh` command spanning several lines (e.g. `--body "…"` with newlines) is not recognised as
  excluded either and fails with HTTP 401: write the text to a file and use `--body-file`.
- `.git/config`, `~/.cargo` and `~/.rustup` are read-only inside the sandbox: new dependencies
  are downloaded with `cargo fetch` (then builds work offline), toolchain updates are done by the
  owner, `cargo xtask setup` must run outside the sandbox.
- The sandbox has no GPU and no display: windows (GUI tests, plugin editors) must run outside it
  with the owner's permission, and the owner must look at the screen.
- Commit trailers (`User-visible`, `Co-Authored-By`) must be in the **same last paragraph**.
- The weekly CI run (Monday 06:00 UTC) emails failures to whoever last edited its `cron` line,
  and GitHub disables scheduled workflows after 60 days without repository activity.
- `core-time` denies `clippy::arithmetic_side_effects` and `clippy::indexing_slicing`,
  `core-model` denies `clippy::indexing_slicing` (crate-root attributes): plain `+ - *` and `v[i]`
  do not compile there.
- Benchmarks need Valgrind and `gungraun-runner` of exactly the `gungraun` version in Cargo.toml
  (both installed on the development machine on 2026-10-31). Inside the sandbox `/tmp` is not
  writable: `cargo xtask bench` forwards `TMPDIR` so that Valgrind can create its temporary files.
  A plain `cargo bench --bench <name>` does not (Gungraun clears the environment): prefix it with
  `GUNGRAUN_ENV_CLEAR=no`.
- In `core-time`, clippy's `arithmetic_side_effects` also rejects `wrapping_div` (it may panic on a
  zero divisor) but accepts `wrapping_div_euclid`, `wrapping_neg/abs/shr/shl` and all `checked_*`.
  Helper functions in integration tests are not covered by clippy.toml's `allow-unwrap-in-tests`:
  those files lift `unwrap_used` with a file-level `#![allow(..., reason = ...)]`.
- Every Gungraun runner option has an environment variable (`GUNGRAUN_BASELINE`,
  `GUNGRAUN_SAVE_BASELINE`, `GUNGRAUN_CALLGRIND_LIMITS`, `GUNGRAUN_HOME`, …): xtask passes them
  that way through `cargo bench`. Baselines are keyed by benchmark **name**: rename a benchmark
  whose meaning changes. Small benchmarks drift by an instruction when unrelated code in the same
  binary changes: always benchmark batches (ADR-0017 §4).
- Gungraun's `#[library_benchmark]` rejects doc comments on benchmark functions: use `//` comments.
- `rustfmt` on a single file fails when a `mod` it declares has no file yet: write module files
  first, or run `cargo fmt --all` afterwards.
- The PostToolUse rustfmt hook formats only the edited file; a later script edit can still leave
  `cargo fmt --all --check` failing: run `cargo fmt --all` before `cargo xtask check`.
- `spikes/` strips release binaries; its `[profile.bench]` keeps symbols because Callgrind finds
  the measured function by name (otherwise every count is 0). Run its benchmarks with
  `GUNGRAUN_ENVS=TMPDIR` (Valgrind needs a writable temporary directory inside the sandbox).
  Passing a name filter after `--` filtered out every benchmark there; the whole run takes a few
  minutes. One case (`iterate` on P2) recorded no work under Callgrind while the native timing
  showed it did: always cross-check a suspicious count with a native measurement.
