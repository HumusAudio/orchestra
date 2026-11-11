# ADR-0036 — UI building blocks: versions, features, native libraries

- **Status:** Accepted (owner, 2026-11-09, after trying the spike's window mode on their desktop:
  window, text and file dialog work; B6 completed with the plan for our own title bar).
  **B10 answered by ADR-0042** (2026-11-11): no GPU path for now. (Its addition of `vello_cpu`
  to B1 was reverted by ADR-0043: the blocks are unchanged.)
- **Date:** 2026-11-09
- **Amends:** ADR-0012, the table "Building block (to be confirmed with an ADR amendment when
  first added)". The decision of ADR-0012 (no toolkit, our own interface on low-level blocks,
  each behind our own traits) is unchanged. This ADR confirms each block, fixes its version and
  features, and removes `ttf-parser`.

## Context

ADR-0012 named the building blocks of `orchestra-ui` but left their versions, their Cargo
features and their native dependencies to be checked "when first added". Phase 2A starts adding
them (F2.02–F2.12), so F2.01 checks them first, all together, before any of them enters the
workspace.

Everything was checked on crates.io on 2026-11-09 and measured in `spikes/gui-blocks` (numbers
and method in `spikes/gui-blocks/RESULTS.md`): one binary that links every block, renders shaped
text and SMuFL glyphs headless, and in window mode publishes an AccessKit tree and opens the
native file dialog. Measurement machine: Intel N150, 4 cores; `release` profile with one codegen
unit (ADR-0035); stripped binaries.

| | binary | crates Linux / Windows / macOS | clean build |
|---|---|---|---|
| all blocks | 9.64 MB | 163 / 70 / 82 | 213 s |
| without AccessKit | 6.43 MB | 94 / 52 / 76 | 137 s |
| window + drawing + text only | 6.34 MB | 92 / 51 / 73 | — |

1.50 MB of each binary is the two embedded fonts (Inter, Bravura), which the product embeds
anyway (ADR-0006).

## Decision

### B1 — The blocks and their versions

| Concern | Crate | Version | License | Published | Notes |
|---|---|---|---|---|---|
| Windows, input, IME | `winit` | 0.30.13 | Apache-2.0 | 2026-03-02 | 0.31 is in beta since 2025-11 (beta 4, 2026-10-24) and `accesskit_winit` does not support it yet |
| Presenting pixels (CPU) | `softbuffer` | 0.4.8 | MIT OR Apache-2.0 | 2025-12-13 | |
| 2D drawing (CPU) | `tiny-skia` | 0.12.0 | BSD-3-Clause | 2026-02-02 | the version measured in ADR-0012 |
| Font reading, outlines | `skrifa` | 0.49.0 | MIT OR Apache-2.0 | 2026-10-21 | with `read-fonts` 0.46 |
| Text shaping | `harfrust` | 0.15.0 | MIT | 2026-10-22 | port of HarfBuzz 15.0.0, from the HarfBuzz organisation; uses `read-fonts` 0.46 |
| Accessibility | `accesskit` + `accesskit_winit` | 0.25.2 + 0.34.2 | MIT OR Apache-2.0; Apache-2.0 | 2026-10-25 | platform adapters chosen inside by target |
| Native file dialogs | `rfd` | 0.17.2 | MIT | 2026-01-12 | 0.18.0 came out on 2026-11-07: too new, re-evaluate at F2.12 |
| macOS global menu | `muda` | 0.21.2 | Apache-2.0 OR MIT | 2026-10-08 | macOS only (B5) |

All licenses are accepted by our `deny.toml` (`cargo deny check licenses` on the spike: ok). All
crates are actively maintained (a release in the last 11 months, most in the last month). The
MSRVs (≤ 1.90) are below ours (1.99).

Versions go in `[workspace.dependencies]` as caret requirements on the verified version, like
the other external crates. Two groups are **updated together, deliberately, never one alone**:
`skrifa` + `harfrust` (they must share the same `read-fonts`; both publish a breaking minor
version every 2–4 weeks), and `accesskit` + `accesskit_winit` (+ `winit`).

Each block enters the workspace in the SCALETTA step that first uses it (`tiny-skia` in F2.03,
`winit`/`softbuffer` in F2.04, `skrifa`/`harfrust` in F2.07, AccessKit with the first widget in
F2.08–F2.09, `rfd` and `muda` in F2.11–F2.12), with the version of this table unless a newer one
has been checked again at that moment.

### B2 — Features

Only what Orchestra needs, because features decide size, crates and native libraries:

```toml
winit = { version = "0.30.13", default-features = false, features = ["rwh_06", "x11", "wayland", "wayland-dlopen", "wayland-csd-adwaita-notitle"] }
softbuffer = { version = "0.4.8", default-features = false, features = ["x11", "x11-dlopen", "wayland", "wayland-dlopen"] }
tiny-skia = { version = "0.12.0", default-features = false, features = ["std", "simd", "png-format"] }
skrifa = { version = "0.49.0", default-features = false, features = ["std"] }
harfrust = { version = "0.15.0", default-features = false, features = ["std"] }
accesskit = "0.25.2"
accesskit_winit = { version = "0.34.2", default-features = false, features = ["accesskit_unix", "async-io", "rwh_06"] }
rfd = { version = "0.17.2", default-features = false, features = ["xdg-portal"] }
# [target.'cfg(target_os = "macos")'.dependencies]
muda = { version = "0.21.2", default-features = false }
```

- `winit`: X11 and Wayland both loaded with dlopen; Wayland decorations without a title font
  (`-notitle`), so that winit does not bring a second font stack (`ab_glyph`). See B6.
- `softbuffer`: no `kms` (drawing without a compositor is never needed).
- `tiny-skia`: `png-format` is needed for the headless screenshots (ADR-0009).
- `skrifa`: no `autohint_shaping`. Autohinting is not used: we draw unhinted outlines, the same
  on every platform (ADR-0006).
- `accesskit_winit`: the `async-io` executor, not `tokio`.
- `rfd`: the XDG portal only, never GTK.

### B3 — `skrifa` only, `ttf-parser` removed

ADR-0012 said "`skrifa`/`ttf-parser` outlines". We keep only `skrifa`. `harfrust` is built on
`read-fonts`, the same parser as `skrifa`, and since `skrifa` 0.49 both take the same font
object (`skrifa::Font`), so one parsed font serves both shaping and drawing. `ttf-parser` would
be a second parser of the same files. The ADR-0012 spike used `ttf-parser` only because it had
no shaping at all.

### B4 — `rfd` 0.17, which loads libdbus-1, rather than 0.16 with `ashpd`

Since 0.17, `rfd` reaches the XDG desktop portal through `libdbus-1` loaded with dlopen. Up to
0.16 it used `ashpd` on `zbus`, which is pure Rust. We measured both. 0.17 costs +0.07 MB and
+2 crates. 0.16 costs +0.9 MB and +29 more crates, even though `zbus` is already in the tree for
AccessKit. So we choose 0.17. libdbus-1 is on every Linux desktop with a portal: the portal itself
is a D-Bus service. If libdbus-1 is missing, rfd falls back to running `zenity` when it is
installed, otherwise the dialog does not open. That case must turn into an error message of
ours, not a silent failure (F2.12). The library is registered in `docs/native-deps.md` (B8).

### B5 — `muda` only on macOS

On Linux `muda` needs GTK. There, and on Windows, ADR-0012 already has us draw the menu bar
ourselves. `muda` is a `[target.'cfg(target_os = "macos")'.dependencies]` entry, so it is never
compiled on Linux or Windows.

### B6 — Wayland window decorations: winit's own for now

GNOME on Wayland does not draw window decorations (title bar, close button). The application
draws them. With `wayland-csd-adwaita-notitle`, winit draws simple Adwaita-like ones without a
title. They cost +0.3 MB and a second `tiny-skia` (0.11, from `sctk-adwaita`). Without them, on
GNOME/Wayland the window would have no title bar and could not be moved or closed with the
mouse. We keep them **for now**. The plan approved by the owner is to replace them later with
**our own title bar**, drawn with our widgets, in SCALETTA step F2.11b (after the basic widgets
and the menus). That step starts with its own ADR, which decides where our title bar is used
(only on Linux, or everywhere with the native window buttons on macOS, as CLAUDE.md §6 asks us to
respect platform conventions). The switch is local to the window layer: drop the
`wayland-csd-adwaita-notitle` feature, create the window without decorations, draw the bar, and
use winit's `drag_window`/`drag_resize_window`. It also removes the second `tiny-skia` (B7).

### B7 — Duplicate versions: listed in `deny.toml`, each with its exit

`deny.toml` denies two versions of the same crate (`multiple-versions = "deny"`). These blocks
bring some that we cannot remove from our side. They are added to `[bans] skip` **in the step
that first brings each one**, each with a comment naming what pulls it and what removes it:

| Duplicate | Pulled by | Removed when |
|---|---|---|
| `rustix` 0.38 / 1, `linux-raw-sys` 0.4 / 0.12 (Linux) | `winit` 0.30 | `winit` 0.31 |
| `windows-sys` 0.52 / 0.61 (Windows) | `winit` 0.30 | `winit` 0.31 |
| `objc2` 0.5 / 0.6 and its family, `bitflags` 1 / 2 (macOS) | `winit` 0.30, `accesskit_macos` 0.27 | `winit` 0.31 and the matching `accesskit_winit` |
| `tiny-skia` 0.11 / 0.12 (Linux) | `sctk-adwaita` (B6) | our own decorations, or a new `sctk-adwaita` |
| `miniz_oxide` 0.8 / 0.9 | `png` 0.18 (directly and through `flate2`) | a new `png` |
| `syn` 2 / 3 (build only) | proc macros of `jni`, `num_enum`, `phf`, `enumflags2` | their move to syn 3 |

We considered the alternative of `winit` 0.31 beta, which removes most of them, and rejected
it: it is a beta, its API changes between betas, and `accesskit_winit` does not support it. When
`winit` 0.31 is released and `accesskit_winit` follows, the update is a deliberate step of its
own: the winit API changes a lot, but only our `platform`/window layer sees it (ADR-0012).

### B8 — Native libraries

Nothing is linked at build time except the C runtime (`ldd`: `libc`, `libm`, `libgcc_s`). On
Linux the system libraries are **loaded at run time with dlopen**, only when needed, so the same
binary starts on X11-only and on Wayland-only systems:

- `winit`, X11: `libX11`, `libX11-xcb`, `libXcursor`, `libXi`, `libxcb`, `libxkbcommon-x11`;
- `winit` and `softbuffer`, Wayland: `libwayland-client`; both sessions: `libxkbcommon`;
- `rfd`: `libdbus-1` (B4).

They are system libraries with no Rust replacement: they are the windowing system's own
interface. They are recorded in `docs/native-deps.md` as one entry per group. On Windows and
macOS only system APIs are used (`windows-sys`/`windows`, `objc2` bindings to AppKit, Core
Graphics, Quartz Core, NSAccessibility, UI Automation). They are listed there too, as the registry
asks for platform APIs. AccessKit on Linux is pure Rust (`zbus`).

### B9 — A rule from AccessKit for the window code

`accesskit_winit::Adapter` panics if it is created after the window has been shown for the first
time. Our window code always creates the window invisible, attaches the adapter, and only then
shows it. This is checked by a test in F2.04.

### B10 — Not decided here

Clipboard (F2.10), system theme detection (F2.02) and the GPU path (`wgpu` + `vello`, F2.06)
are decided in their own steps, as SCALETTA plans. *The GPU path: ADR-0042 (not now); the CPU
rasteriser stays `tiny-skia` (ADR-0043).*

## Consequences

- The size we pay for the interface is known before writing it. All blocks: about 8 MB of code
  (9.6 MB with the fonts). Accessibility is the biggest single cost on Linux: +3.2 MB, +69
  crates, +76 s of clean build. It is required (ADR-0012), so we accept it.
- Clean release builds get longer: 213 s for the blocks alone on the development machine.
  Incremental `dev` builds are not affected after the first build.
- `deny.toml` gains `skip` entries with a reason and an exit each; `winit` 0.31 removes most of
  them.
- `docs/native-deps.md` gets its first entries.
- Windows and macOS binary sizes are measured by the CI from F2.04, when the window code exists.
- The window mode of the spike could not be tried in the development sandbox (no compositor).
  The owner tried it on their desktop on 2026-11-09: the window, the text and the file dialog
  work.
