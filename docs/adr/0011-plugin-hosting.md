# ADR-0011 — Plugin hosting: our own host layer, one process per plugin

- **Status:** Accepted (2026-10-29, owner's decision after the Phase 0, Step 13 spike)
- **Date:** 2026-10-28 (proposed) · 2026-10-29 (accepted)

## Context

The DAW must host external instruments and effects. Plugins are third-party native code: they can
crash, hang, leak or corrupt memory. A crash of a plugin must never take down the project. Plugin
GUIs embedded in a host window cause trouble on Linux (XEmbed under Wayland/XWayland, already met
with pluginval).

## The spike (Phase 0, Step 13)

Code: `spikes/plugin-host`. A host process starts one **bridge** process per plugin; audio blocks
(stereo, 256 frames, 48 kHz) travel through a shared-memory file, each block is triggered and
acknowledged by one byte on a pipe. Inside the bridge the plugin was loaded with `truce-rack`
1.1.5 (pure-Rust bindings: `clap-sys`, `vst3`, `lilv-sys`). Plugins: LSP Plugins 1.2.35 in all
three formats (same DSP), plus third-party VST3s (Bertom Vocal Compressor, ZL Equalizer 2,
Delay5).

| | CLAP | VST3 | LV2 |
|---|---|---|---|
| Scan | 198 plugins, 94 ms | 287 plugins, 2.2 s (loads plugin binaries) | 199 plugins, 9.6 s |
| Load LSP | ❌ segfault inside the plugin, called from truce-rack's loader | ❌ segfault | ✅ |
| Load other plugins | — (no other CLAP installed) | ✅ | — |
| Audio through the bridge | — | ⚠️ output always silent | ✅ |
| Out-of-process overhead per block (median) | — | ≈ 27 µs (0.5 % of 5.33 ms) | ≈ 20 µs (0.4 %) |
| Round trip p99 | — | 1.4–2.1 ms | 1.1–1.6 ms |
| Kill the bridge mid-stream | — | detected in the same block; host survives, silence | same |
| Restart | — | 0.3–0.6 s | 11 s (lilv loads the whole world) |
| Editor in a separate top-level window (X11 via XWayland) | — | ✅ complete, correct size, interactive (owner verified) | ✅ interactive; initial window slightly too small (no size reported) |

Other observations: plugins print to stdout (the bridge must keep its protocol off stdout);
truce-rack reported `has_editor = false` for a VST3 that has an editor; plugin GUIs on Linux are
X11-only in practice (Wayland CLAP GUI API is rarely implemented).

## Decision

1. **Our own `PluginHost` interface** (`orchestra-plugin-host`): the rest of the program sees only
   our types.
2. **Our own host implementation for each format**, written directly on the low-level bindings —
   `clap-sys` (CLAP), `vst3` (VST3, MIT-licensed SDK headers), `lilv-sys` (LV2, links the `lilv` C
   library: a `docs/native-deps.md` entry is required when it is added), and AU through `objc2`
   on macOS. **No** dependency on a hosting framework such as truce-rack: the spike showed crashes
   and silent output with common plugins, and the owner prefers to own this critical layer.
3. **Supported formats:** CLAP, VST3, LV2, and AU on macOS. **No VST2.**
4. **One process per plugin** (a "bridge"), confirmed by the measurements: the isolation costs
   ≈ 20–30 µs per block, and every crash in the spike killed only the bridge. **Scanning also runs
   in a separate process**, because scanning VST3 (and CLAP) loads plugin code.
5. **Audio through shared memory**, signalling with a real-time-friendly primitive (futex on
   Linux, events on Windows, semaphores/`os_sync` on macOS) with a **bounded wait**, so that a
   *hung* plugin is detected too, not only a dead one; bridge audio threads run with real-time
   priority. The p99 round trip measured with pipes and normal priority (1–2 ms) is the number
   these measures must bring down; Phase 4 benchmarks it against the ADR-0007 budget.
6. **Plugin GUIs in separate top-level windows owned by the bridge process**, never inside
   Orchestra's own window. On Linux the parent is an X11 window (XWayland under Wayland).
7. The bridge never writes its protocol on stdout/stderr; plugin output is redirected to the log.

## Alternatives considered

- **truce-rack inside the bridge** (recommended by the spike as a starting point): rejected by the
  owner — crashes with LSP CLAP/VST3, silent VST3 output, inaccurate metadata, very young project.
- **In-process hosting:** simplest and fastest, but one bad plugin crashes the whole session.
- **Postpone the decision to Phase 4:** rejected; the architecture is now validated by data.

## Consequences

- More work in Phase 4 (three format hosts plus AU), but no dependency on a young hosting library.
- `lilv` (C) becomes a native dependency for LV2 (ISC license, GPL-compatible), to be recorded in
  `docs/native-deps.md` when introduced; on Windows/macOS it must be built or vendored.
- The bridge protocol, restart policy and RT signalling are designed and benchmarked in Phase 4.
