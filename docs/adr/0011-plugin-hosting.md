# ADR-0011 — Plugin hosting

- **Status:** Proposed (to be Accepted after the Phase 0, Step 13 spike with measured data)
- **Date:** 2026-10-28

## Context

The DAW must host external instruments and effects. Plugins are third-party native code: they can
crash, hang, leak or corrupt memory. A crash of a plugin must never take down the project.
Plugin GUIs embedded in a host window cause trouble on Linux (XEmbed under Wayland/XWayland,
already met with pluginval).

## Decision (proposed)

1. **Our own `PluginHost` interface** (`orchestra-plugin-host`): the rest of the program sees only
   our types; format-specific code stays behind it, so formats and libraries can change.
2. **Supported formats:** CLAP, VST3, LV2, and AU on macOS. **No VST2** (deprecated, licensing).
3. **Every plugin in a separate process**, with audio exchanged through shared memory and a
   real-time-safe signalling mechanism; the host detects a dead or hung plugin process, mutes it
   and offers to restart it.
4. **Plugin GUIs in separate top-level windows**, never embedded in our window.
5. Concrete libraries (to be measured in the spike): existing hosting crates (candidates named in
   the Phase 0 brief — `truce-rack`, `maolan-plugin-host`, `rack` — to be checked for existence,
   maturity, license and maintenance), raw bindings (`clap-sys`, VST3 bindings, `lilv-sys` for LV2),
   or a thin layer of our own on top of the bindings.

## Spike criteria

Scan installed CLAP/VST3/LV2 plugins; load one per format in a child process; process a buffer
through shared memory and measure the per-buffer overhead; kill the plugin process mid-processing
and verify the host survives; open the plugin GUI in a separate window under Wayland/XWayland.

## Consequences

- Out-of-process hosting adds latency/overhead per buffer: the spike must quantify it against the
  ADR-0007 budget.
- Native SDKs (VST3, LV2 world libraries) will need entries in `docs/native-deps.md`.
