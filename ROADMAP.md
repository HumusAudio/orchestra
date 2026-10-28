# Roadmap

Read this first at the start of every session (CLAUDE.md §0). It records **where we are**, **what
comes next** and **which decisions are still open**. Decisions themselves live in `docs/adr/`.

## Current status — Phase 0 (foundations) in progress

Branch: `feature/fase-0-fondamenta`. Phase 0 prepares rules, automation, decisions and evaluation
prototypes; it contains no program functionality.

| Step | What | Status |
|---|---|---|
| 1 | Environment check (CachyOS, Rust, tools) | ✅ done |
| 2 | Project name: **Orchestra** | ✅ done |
| 3 | Toolchain & system packages (`github-cli`, `cargo-deny`, `cargo-nextest`; Rust updated to 1.99.1) | ✅ done |
| 4 | Sandbox network allowlist + `excludedCommands` for git/gh in `.claude/settings.json` | ✅ done |
| 5 | GitHub repo `HumusAudio/orchestra`, `main` pushed and protected by a ruleset | ✅ done |
| 6 | Workspace skeleton, lints, `deny.toml`, `.gitattributes`, ARCHITECTURE, native-deps | ✅ done |
| 7 | CLAUDE.md, ROADMAP, ADR 0001–0013 | ✅ done |
| 8 | Hooks (rustfmt PostToolUse, docs Stop hook, commit-msg) | ✅ files in place; verification in Step 11 |
| 9 | i18n infrastructure (Fluent catalogs, glossary, parity tests) | ⏳ next |
| 10 | Documentation infrastructure (DOCUMENTAZIONE/DOCUMENTATION, Typst guides, xtask) | ⏳ |
| 11 | Verification of every automation | ⏳ |
| 12 | CI on GitHub Actions (3 OS) + guide artifacts | ⏳ |
| 13 | Spikes: GUI toolkit and plugin hosting → ADR-0011/0012 | ⏳ |
| 14 | Closing: PR to `main`, green CI, final report | ⏳ |

## Next phases (proposal)

1. **Phase 1 — `core-time` and `core-model`:** rational time, tempo/meter map, conversions;
   the document model with two-layer notes, form (repeats/jumps), commands and unified undo/redo;
   concrete project file format and first golden file (ADR-0010).
2. **Phase 2 — basic notation and layout:** incremental engraving of single-voice and
   multi-voice staves with a bundled SMuFL font; notation view in the chosen toolkit.
3. **Phase 3 — audio engine and timeline:** null/offline backend, real device backends behind
   traits, the reference project and its benchmarks, the unrolled DAW timeline.
4. **Phase 4 — plugin hosting:** out-of-process CLAP/VST3/LV2 per ADR-0011.
5. **Phase 5 — video:** video tracks with proxies on the shared timeline.

## Open decisions

- **Pedantic lint selection** (Cargo.toml `[workspace.lints.clippy]`): proposal in place
  (`pedantic` on, `module_name_repetitions`, `must_use_candidate`, `similar_names` silenced);
  awaiting the owner's review.
- **Tolerance threshold default** (ADR-0004): proposed 1/32 note; to validate with users.
- **Concrete project file format** (ADR-0010, Phase 1).
- **GUI toolkit** (ADR-0012) and **plugin hosting libraries** (ADR-0011): after the spikes.
- **Language of project docs:** ADRs, ARCHITECTURE and this ROADMAP are written in English (open
  source audience, same language as the code); `DOCUMENTAZIONE.md` is the Italian counterpart of
  the developer documentation. Change if the owner prefers Italian.

## Notes for future sessions

- `sandbox.excludedCommands` only applies to a **single** command: `cd … && git push` stays
  sandboxed and fails. Run `git push` / `gh …` on their own (cwd is the repository).
- `.git/config` is read-only inside the sandbox: `cargo xtask setup` (which sets
  `core.hooksPath`) must run unsandboxed or by the owner.
- `~/.cargo` and `~/.rustup` are read-only inside the sandbox: toolchain updates are done by the
  owner (`rustup update stable`); `cargo-deny` keeps its advisory DB in `target/advisory-dbs`.
- Commit trailers (`User-visible`, `Co-Authored-By`) must be in the **same last paragraph** of the
  commit message, otherwise git does not parse them and the commit-msg hook rejects the commit.
