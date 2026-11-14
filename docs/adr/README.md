# Architecture Decision Records

Each ADR records one decision: context, decision, rejected alternatives, consequences, status.
Statuses: **Proposed** (under discussion), **Accepted** (binding: never contradict it — propose a
new ADR that supersedes it instead), **Superseded** (replaced; links to its successor).

New ADRs: `docs/adr/NNNN-short-title.md`, next free number, same structure.

| # | Title | Status |
|---|---|---|
| [0001](0001-rust-and-native-dependencies.md) | Rust as the implementation language; native dependencies policy | Accepted |
| [0002](0002-license-and-dependency-licenses.md) | GPL-3.0-or-later; dependency licenses; AI-generated code | Accepted |
| [0003](0003-single-document-two-views.md) | One document, two views, video as a track; commands and undo | Accepted |
| [0004](0004-two-layer-note-model.md) | Two-layer note model; tolerance threshold; unrolled timeline | Accepted |
| [0005](0005-time-representation.md) | Exact rational time, one tempo map, derived samples and frames | Accepted |
| [0006](0006-cross-platform-policy.md) | Cross-platform policy | Accepted |
| [0007](0007-performance-policy.md) | Performance policy: reference hardware, budgets, real-time rules | Accepted |
| [0008](0008-internationalisation.md) | Internationalisation: six languages, Fluent, glossary, parity | Accepted |
| [0009](0009-documentation-system.md) | Documentation system: four documents, parity, trailers, Typst | Accepted |
| [0010](0010-project-file-format.md) | Project file format: versioned with migrations | Accepted (policy; format in ADR-0029) |
| [0011](0011-plugin-hosting.md) | Plugin hosting: own host layer, one process per plugin | Accepted |
| [0012](0012-gui-toolkit.md) | GUI: no toolkit, own interface on low-level building blocks | Accepted |
| [0013](0013-github-hosting-and-workflow.md) | GitHub hosting, protected `main`, PR workflow | Accepted |
| [0014](0014-visual-identity-logo-and-themes.md) | Visual identity: one logo, light and dark themes | Accepted |
| [0015](0015-rational-type.md) | The rational type: own `i64` fraction, exact `i128` intermediates, typed overflow | Accepted |
| [0016](0016-test-and-benchmark-tools.md) | Test and benchmark tools: Gungraun, proptest, own allocation counter | Accepted |
| [0017](0017-benchmark-regression-gate.md) | Benchmark regression gate: same-job comparison with the merge base, +5% instructions | Accepted |
| [0018](0018-meter-map-and-time-domain-types.md) | Meter map and time domain types: `Position`/`Duration`, meters, irregular and free bars | Accepted |
| [0019](0019-beat-grouping.md) | Beat grouping: how a bar is counted, separate from how its meter is written | Accepted |
| [0020](0020-tempo-map.md) | Tempo map: gradual changes linear in time, exact boundaries, exact sample rounding | Accepted |
| [0021](0021-tempo-map-time-grid.md) | Tempo map: boundary times on a fixed grid, rounded positions inside gradual changes | Accepted |
| [0022](0022-model-architecture.md) | Model architecture: stable IDs, bar-relative events in per-bar chunks, voices per instrument | Accepted |
| [0023](0023-pitch-spelling-intervals-keys.md) | Pitch with spelling, intervals, key signatures, transposition | Accepted |
| [0024](0024-voice-content.md) | What a voice contains: events, note values, tuplets, ties, grace notes, markings, offsets | Accepted |
| [0025](0025-form.md) | The form: repeats, voltas, jumps, bar repeats and pass-specific playing | Accepted |
| [0026](0026-commands-undo-notifications.md) | Commands, one undo history, fine-grained change notifications | Accepted |
| [0027](0027-playback-rules-v0.md) | Playback rules v0: from the written intent to the played notes | Accepted |
| [0028](0028-daw-move-tolerance-threshold.md) | Moving a note from the DAW side: tolerance threshold, performance-only edits, pass offsets | Accepted |
| [0029](0029-project-file-format-v1.md) | Project file format v1: plain JSON, a schema of its own, strict loading | Accepted |
| [0030](0030-macos-ci-not-blocking-merges.md) | macOS CI no longer blocks merges; it still decides when work is done | Accepted |
| [0031](0031-test-scores-crate.md) | A test-only crate for the reference scores | Accepted |
| [0032](0032-midi-file-export-v0.md) | MIDI file export v0: Standard MIDI File 1.0, written by us | Accepted (E9 amended by ADR-0033) |
| [0033](0033-midi-half-way-key.md) | One rule for a pitch half way between two MIDI keys: the lower key | Accepted |
| [0034](0034-public-api-policy.md) | Public API of the library crates: one path per item, exhaustive errors | Accepted |
| [0035](0035-release-build-profile.md) | Release build profile: one codegen unit per crate | Accepted |
| [0036](0036-ui-building-blocks.md) | UI building blocks: versions, features, native libraries (amends ADR-0012) | Accepted |
| [0037](0037-themes-tokens-and-customisation.md) | Themes: tokens, ten built-in pairs, user customisation (amends ADR-0014) | Accepted |
| [0038](0038-system-theme-detection.md) | Reading the system light/dark preference: our portal code on Linux, winit elsewhere | Accepted |
| [0039](0039-drawing-abstraction.md) | The drawing abstraction: a segmented display list of themed commands | Accepted |
| [0040](0040-window-and-event-loop.md) | The window and the event loop: on-demand frames, our own input events | Accepted (precision F2.09e) |
| [0041](0041-headless-screenshots.md) | Headless screenshots: a scene catalogue, rendered by the app, never committed (amends ADR-0014 §7, ADR-0037 T10) | Accepted |
| [0042](0042-cpu-renderer-vello-cpu.md) | The interface renderer: `vello_cpu` behind our display list, no GPU for now (amends ADR-0039 D9, ADR-0036, ADR-0012) | Accepted; R1, R3, R5, R6 superseded by ADR-0043 |
| [0043](0043-keep-tiny-skia-cache-shadows.md) | Keep `tiny-skia`, cache shadows; `vello_cpu` is dropped (supersedes part of ADR-0042) | Accepted (precisions F2.09b2, F2.09d) |
| [0044](0044-text-fonts-shaping-glyph-cache.md) | Text: bundled fonts, shaping, measuring and a glyph cache (amends ADR-0036 B1, ADR-0039 D4) | Accepted |
| [0045](0045-widget-architecture.md) | Widget architecture: a retained tree, our own layout, routed events, AccessKit per widget | Accepted (precisions F2.09a–e) |
| [0046](0046-value-control-conventions.md) | Value-control conventions: wheel, double click to the default, fine adjustment | Accepted |
| [0047](0047-text-field.md) | The text field: byte offsets on graphemes, platform editing keys, a local undo, IME, the clipboard | Accepted (precisions F2.10b1) |
