# Native (C/C++) dependencies

Orchestra is written in Rust. A C or C++ library is allowed **only when no reasonable Rust
alternative exists** (CLAUDE.md §6, ADR-0001). Every such exception is recorded here, *before* it
is merged, with its justification, license and a replacement plan.

"Native dependency" includes libraries linked through `-sys` crates, libraries loaded at run time
(`dlopen`), and system frameworks we bind to directly. Platform APIs reached through the operating
system's own interface (e.g. ALSA, WASAPI, CoreAudio) are listed too, because they constrain
portability, even though they cannot be "replaced".

## Registry

*No entries yet.*

## Development tools (never linked into the program)

Native programs that the **development workflow and CI** need, but that are never linked into,
loaded by or shipped with Orchestra. They are not exceptions to ADR-0001; they are listed for
transparency, because a contributor has to install them.

### Valgrind

- **Used by:** `cargo xtask bench` (Gungraun runs each benchmark under Valgrind's Callgrind and
  reports instruction counts; ADR-0016).
- **License:** GPL-2.0-or-later. Only executed as a separate program, never linked.
- **Platforms:** Linux only (Arch/CachyOS: `pacman -S valgrind`; Ubuntu CI: `apt-get install
  valgrind`). On Windows and macOS benchmarks are compiled but not run (ADR-0016 §2).
- **Companion tool:** `gungraun-runner` (Rust, Apache-2.0 OR MIT), installed with
  `cargo install gungraun-runner --version <same as gungraun in Cargo.toml> --locked`.
- **Added:** 2026-10-31, ADR-0016.

## Entry template

Copy this block for every new native dependency:

```markdown
### <library name> (via `<crate-name>`)

- **Used by:** <workspace crate(s)>
- **What for:** <one paragraph>
- **Why no Rust alternative:** <the alternatives we evaluated and why they are not enough>
- **License:** <SPDX identifier> — compatible with GPL-3.0-or-later because <reason/source>
- **Platforms:** Linux <how it is obtained: pacman package / vendored / system> ·
  Windows <…> · macOS <…>
- **Build requirements:** <headers, pkg-config names, toolchains>
- **Binary size / compile time impact:** <measured numbers>
- **Replacement plan:** <what would let us drop it, and when we re-evaluate>
- **Added:** <YYYY-MM-DD>, ADR-<NNNN> (if any)
```
