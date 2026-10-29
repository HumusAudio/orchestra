# ADR-0006 — Cross-platform policy

- **Status:** Accepted
- **Date:** 2026-10-28

## Context

Development is Linux first (the owner uses Arch-based CachyOS), but Windows and macOS are planned.
Retrofitting portability is expensive: assumptions about paths, line endings, audio APIs or window
systems spread through a codebase quickly.

## Decision

1. **Every choice is evaluated first for Linux/Windows/macOS compatibility**, even when only
   Linux is implemented.
2. **Platform-specific code only in `platform/` modules behind traits**, inside the crate that
   needs it (e.g. `audio-engine/src/platform/{linux,windows,macos}.rs`). The rest of the code
   depends only on the trait.
3. **CI on three systems** (GitHub-hosted standard runners: `ubuntu-latest`, `windows-latest`,
   `macos-latest`). A red platform means the work is not done.
4. **Same visual and behavioural identity everywhere**: our own drawn interface and fonts bundled
   in the program (no dependence on system fonts or native widgets for the main UI) —
   **while respecting system conventions**: an abstract *primary modifier* (Ctrl on Linux/Windows,
   Cmd on macOS), the global menu bar on macOS, native file dialogs, platform shortcuts for
   quitting/preferences, standard config/data directories.
5. **Portable code rules:** paths only through `std::path` (never concatenated strings); no
   assumptions about `\n` vs `\r\n` (the repository is LF via `.gitattributes`, but files we read
   may be either); no assumptions about case sensitivity of file systems, path separators or the
   locale's encoding.
6. **Native libraries** only per ADR-0001, each with its per-platform story in
   `docs/native-deps.md`.
7. **Plugins:** CLAP, VST3, LV2 (+ AU on macOS), each in a separate process, GUIs in separate
   windows (ADR-0011).

## Alternatives rejected

- **Native widgets per platform:** maximal integration, but three UIs to maintain and inconsistent
  behaviour of a complex editor (notation needs custom drawing anyway).
- **Linux-only for now, port later:** postpones the cost and multiplies it.

## Consequences

- Some platform conventions need explicit work (macOS global menu with a custom-drawn UI); the
  GUI toolkit evaluation (ADR-0012) scores this.
- CI minutes on three systems are free only for public repositories on standard runners
  (ADR-0013).
