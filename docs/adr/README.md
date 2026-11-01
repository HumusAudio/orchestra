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
| [0010](0010-project-file-format.md) | Project file format: versioned with migrations | Accepted (policy) / Proposed (format) |
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
