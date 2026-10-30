# Roadmap

Read this first at the start of every session (CLAUDE.md §0). It records **where we are**, **what
comes next** and **which decisions are still open**. Decisions themselves live in `docs/adr/`.

## Current status — Phase 0 complete and merged; visual identity added

Phase 0 was merged into `main` (PR #1, 2026-10-29). Then, on `feature/logo-e-temi`: the owner's
logo became the only project logo (outlined SVG in `assets/logo/`, used on the guide covers) and
ADR-0014 fixed light and dark themes from the start. On 2026-10-30 the owner approved the
detailed work plan **`SCALETTA.md`** (Italian, owner's choice): phases, steps with stable IDs,
milestones and the index of open decisions live there. Next: step F0.15 (macOS CI check), then
Phase 1 on a new branch.

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
| 12 | CI on Linux/Windows/macOS + guide PDFs as artifacts | ✅ (see open items: macOS run) |
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

- **Pedantic lint selection** (`Cargo.toml`): proposal in place, awaiting the owner's review.
- **Tolerance threshold default** (ADR-0004): proposed 1/32 note; to validate with users.
- **Concrete project file format** (ADR-0010, Phase 1).
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
- `.git/config`, `~/.cargo` and `~/.rustup` are read-only inside the sandbox: new dependencies
  are downloaded with `cargo fetch` (then builds work offline), toolchain updates are done by the
  owner, `cargo xtask setup` must run outside the sandbox.
- The sandbox has no GPU and no display: windows (GUI tests, plugin editors) must run outside it
  with the owner's permission, and the owner must look at the screen.
- Commit trailers (`User-visible`, `Co-Authored-By`) must be in the **same last paragraph**.
- `rustfmt` on a single file fails when a `mod` it declares has no file yet: write module files
  first, or run `cargo fmt --all` afterwards.
