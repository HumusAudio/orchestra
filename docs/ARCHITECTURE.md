# Architecture

This document is the **map of the workspace**: which crates exist, what each one is responsible
for, and which crate may depend on which. The *why* behind the big choices lives in the ADRs
(`docs/adr/`); the detailed, file-by-file explanation lives in `DOCUMENTATION.md` /
`DOCUMENTAZIONE.md`. Keep this file short and current: update it in the same commit that adds,
removes or re-wires a crate.

## Guiding principle

There is **one project document** (`orchestra-core-model`). Notation, DAW and video are views
(projections) of it; every change is a command on the model, with one undo/redo history
(ADR-0003). Musical time is exact and rational, with a single tempo map (ADR-0005).

## Crates

| Directory | Package | Responsibility |
|---|---|---|
| `crates/core-time` | `orchestra-core-time` | Rational musical time, tempo/meter map, conversions to seconds, samples, video frames |
| `crates/core-model` | `orchestra-core-model` | The single document: tracks, two-layer notes, form (repeats/jumps), commands, undo/redo |
| `crates/playback` | `orchestra-playback` | Written intent → performance; unrolled timeline; tolerance-threshold logic |
| `crates/notation-layout` | `orchestra-notation-layout` | Incremental engraving into positioned glyphs (no drawing) |
| `crates/plugin-host` | `orchestra-plugin-host` | Our `PluginHost` interface; out-of-process CLAP/VST3/LV2 (+AU) |
| `crates/audio-engine` | `orchestra-audio-engine` | Real-time graph, mixing, device backends behind traits, null/offline backend |
| `crates/io` | `orchestra-io` | Project file (versioned + migrations), MIDI, MusicXML |
| `crates/i18n` | `orchestra-i18n` | Fluent catalogs for 6 languages, parity tests |
| `crates/ui` | `orchestra-ui` | Notation/DAW/video views, input → commands, headless screenshots |
| `crates/video` | `orchestra-video` | Video tracks (last phase; empty for now) |
| `crates/app` | `orchestra-app` (binary `orchestra`) | Wiring only |
| `xtask` | `xtask` | Automation: `cargo xtask check/guides/screenshots/bench/setup` |
| `spikes/` (separate workspace) | — | Time-boxed evaluation prototypes; never part of release builds |

## Dependency rules ("only downwards")

```
                app
       ┌─────────┼──────────────┬──────────┐
       ui     audio-engine      io     (all others)
   ┌──┬┴──┬───┐   │    └── plugin-host
   │  │   │   i18n│            │
   │  │ notation-layout        │
   │  playback ◄───────────────┘ (audio-engine reads playback output)
   │  │
 video core-model
   │  │
   core-time
```

Allowed internal dependencies, exhaustively (enforced today by the `[dependencies]` sections; a
future `cargo xtask check` step may verify this table automatically):

| Crate | May depend on |
|---|---|
| core-time | — |
| core-model | core-time |
| playback | core-time, core-model |
| notation-layout | core-time, core-model |
| plugin-host | core-time |
| audio-engine | core-time, playback, plugin-host |
| io | core-time, core-model |
| i18n | — |
| video | core-time |
| ui | core-time, core-model, playback, notation-layout, i18n, video |
| app | everything |

Notable consequences:

- `core-model` never depends on views, layout, audio or files: it can be tested in isolation.
- `audio-engine` does not see `core-model`: the audio thread receives pre-computed event buffers
  from `playback`, never live model data.
- `ui` does not see `audio-engine` or `plugin-host`: transport control goes through `app` wiring
  (exact mechanism decided in Phase 3).

## Unsafe code

`#![forbid(unsafe_code)]` in every crate except `audio-engine`, `plugin-host` and `io` (FFI,
lock-free primitives, shared memory). In those three, every `unsafe` block has a `// SAFETY:`
comment (`clippy::undocumented_unsafe_blocks = "deny"`).

## Platform-specific code

Only inside `platform/` modules behind traits, in the crate that needs it (ADR-0006).
