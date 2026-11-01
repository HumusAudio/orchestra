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
MusicXML and MIDI import threads of SCALETTA §5. Next: **F1.08** (document skeleton).

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

## Open decisions and open items

- **Tolerance threshold default** (ADR-0004): proposed 1/32 note; to validate with users.
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
- **D22, for F1.13:** tuning (temperaments, just intonation, comma accidentals); the MIDI
  conversion uses 12-TET as the reference (ADR-0023 §3).
- **"No mode" spelling:** ADR-0023 §6 spells MIDI keys in a key without mode with the C major
  window, whatever its signature; to revisit if open signatures with accidentals turn out common.
- **D21, for F1.11:** the default behaviour of a meter change (keep the bars' contents, or rewrite
  the music up to the next meter change); both are cheap with ADR-0022's storage.
- **`core-time` work required by ADR-0022 §8, with the first command that needs it:** inserting and
  deleting bars in the meter map, and a bulk edit on both maps (shifting t tempo changes is now t
  rebuilds, O(t²)).
- **Possible model optimisations, not urgent (F1.06 numbers):** a whole-score iteration with
  absolute positions costs ≈ 17 ms on the heavy score (one rational addition per event); a snapshot
  ≈ 1.4 ms (one reference count per non-empty voice-bar). See ADR-0022, Consequences.
- **Concrete project file format** (ADR-0010, Phase 1).
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
