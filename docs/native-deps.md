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
