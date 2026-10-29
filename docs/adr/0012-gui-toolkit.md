# ADR-0012 — GUI: no toolkit, our own interface on low-level building blocks

- **Status:** Accepted (2026-10-29, owner's decision after the Phase 0, Step 13 spike)
- **Date:** 2026-10-28 (proposed) · 2026-10-29 (accepted)

## Context

Orchestra needs mostly *custom-drawn* content (notation, timelines, waveforms, video thumbnails)
plus conventional chrome (menus, dialogs, inspectors, text fields). Requirements from other ADRs:
same look on every platform with bundled fonts (ADR-0006), macOS global menu, native file
dialogs, IME and accessibility, **software rendering fallback** and strict frame budgets
(ADR-0007), **headless rendering to PNG** for the guides' screenshots (ADR-0009), license
compatible with GPL-3.0-or-later (ADR-0002).

## The spike (Phase 0, Step 13)

Code: `spikes/` (separate workspace). Every candidate drew the same scene (`spikes/scene`):
menu bar, the six interface languages in the bundled Inter font, 10 staves × 20 measures with
exactly 2000 Bravura (SMuFL) glyphs plus staff lines, barlines and stems; then a deterministic
120-frame scroll/zoom path at 1920 × 1080. Machine: Intel Alder Lake-N, 4 cores, 16 GB (weaker
CPU than the reference hardware of ADR-0007). CPU rendering only: the sandbox of the development
session has no GPU access, so **GPU rendering was not measured** (Phase 2 will).

| | Own renderer (tiny-skia 0.12) | iced 0.14 | egui 0.34 | Slint 1.18 |
|---|---|---|---|---|
| Frame time mean / p95 (CPU) | 21.5 / 29.6 ms | 28.8 / 39.1 ms ¹ | 11.7 / 14.2 ms ² | 29.5 / 39.0 ms ³ |
| Peak RSS | 14 MB | 35 MB | 47 MB | 48 MB |
| Clean release build ⁴ | 98 s | 578 s | 172 s | 500 s |
| Binary / crates in tree | 5.3 MB / 91 | 18.5 MB / 255 | 6.4 MB / 122 | 25.1 MB / 335 |
| Headless PNG | trivial (render to pixmap) | `iced_test` (scale factor fixed at 2) | needs wgpu, or the experimental `egui_software_backend` 0.0.3 (egui 0.34 only, current 0.37) | custom `Platform` + software renderer |
| Menu bar | to be drawn | no widget in core (`iced_aw`) | `MenuBar` | `MenuBar`, native on macOS via muda |
| Accessibility / IME | to be built (AccessKit) | in progress / partial | AccessKit, IME | yes |
| License | MIT / BSD-3 | MIT | MIT / Apache-2.0 | GPL-3.0-only or royalty-free (attribution) or commercial |

¹ 243 ms when the glyphs are drawn as canvas text (shaping every frame); 28.8 ms with cached
outline paths. ² Glyphs through egui's text atlas: epaint only fills **convex** paths, so music
glyphs and filled slurs/ties cannot be drawn as vector paths. ³ Slint has no immediate-mode
canvas: the score is our tiny-skia image (≈ 21 ms) and Slint adds ≈ 8 ms of composition.
⁴ Wall time on 4 cores; iced and Slint were partly measured while another build was running, so
their numbers are somewhat inflated (the ranking is not affected).

**Main finding:** no toolkit draws a score well with its standard tools (iced's canvas text is
slow, egui cannot fill concave shapes, Slint has no canvas). Score and timeline must be drawn by
a renderer of ours in every option; a toolkit would only provide the chrome.

## Decision

Orchestra uses **no GUI toolkit**. We write our own interface layer in `orchestra-ui`: widgets,
layout, focus, text editing, menus, theming, all drawn by our renderer. We build it on
**low-level building blocks** that are not reasonable to rewrite, each behind our own traits so it
can be replaced:

| Concern | Building block (to be confirmed with an ADR amendment when first added) |
|---|---|
| Windows, input events, IME events | `winit` (Apache-2.0) |
| Presenting pixels | `softbuffer` (CPU) now; `wgpu` (GPU) when the GPU renderer arrives |
| 2D drawing | `tiny-skia` (CPU, measured above); `vello` (GPU) evaluated in Phase 2 |
| Fonts and text shaping | bundled fonts, `skrifa`/`ttf-parser` outlines, `harfrust` shaping, our own glyph cache |
| Accessibility | `accesskit` + its platform adapters (the same library egui and Slint use) |
| Native file dialogs | `rfd` (xdg-desktop-portal on Linux, native on Windows/macOS) |
| macOS global menu | `muda` on macOS only; on Linux/Windows the menu bar is drawn by us |

Headless screenshots (ADR-0009) come for free: the same renderer draws into an off-screen pixmap.

## Alternatives considered (all measured above)

- **egui + own renderer** (recommended by the spike): fastest on CPU and light; rejected by the
  owner in favour of full control of look, behaviour and rendering pipeline, and to avoid the
  experimental headless backend and immediate-mode constraints in a large editor.
- **Slint + own renderer:** native macOS menu and mature accessibility, but 25 MB binary,
  500 s builds and a **GPL-3.0-only** license that would turn our "or later" into "only" for
  distributed binaries.
- **iced + own renderer:** elegant architecture, acceptable speed with outline paths, but slow
  builds, accessibility still in progress and no menu widget in core.

## Consequences

- Full control of rendering, incremental redraw (ADR-0007) and identical look everywhere
  (ADR-0006); smallest binary and fastest builds of the options measured.
- **Significant extra work** before user-facing features: text fields with IME, selection and
  clipboard; focus and keyboard navigation; menus and context menus; scrolling, dialogs;
  accessibility tree. The roadmap gets an explicit "UI foundation" milestone (Phase 2).
- Accessibility is our responsibility: every widget must publish an AccessKit node from the start;
  retrofitting it is not acceptable.
- The CPU path already meets the ≥ 30 fps software budget on weaker-than-reference hardware
  (21.5 ms mean); the GPU path must still be measured.
