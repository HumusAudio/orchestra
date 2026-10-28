# ADR-0012 — GUI toolkit

- **Status:** Proposed (to be Accepted after the Phase 0, Step 13 spike with measured data)
- **Date:** 2026-10-28

## Context

Orchestra needs mostly *custom-drawn* content (notation, timelines, waveforms, video thumbnails)
plus conventional chrome (menus, dialogs, inspectors). Requirements from other ADRs: same look on
every platform with bundled fonts (ADR-0006), macOS global menu, native file dialogs, IME and
accessibility, **software rendering fallback** and strict frame budgets (ADR-0007), and
**headless rendering to PNG** for the guides' screenshots (ADR-0009). License compatible with
GPL-3.0-or-later (ADR-0002).

## Candidates (state to be verified in the spike, 2026)

- `iced` — retained-mode, Elm architecture, wgpu with a tiny-skia software fallback.
- `egui` — immediate mode, very fast to iterate; custom painting is easy.
- `Slint` — declarative UI language; check licensing terms for GPL projects carefully.
- **Own renderer** on `wgpu` / `vello` (GPU vector rendering) / `tiny-skia` (CPU), with a
  lightweight toolkit only for menus and windows.

## Spike scene (identical for every candidate)

1. Window with a menu bar and a custom drawing area with 2000 scrollable/zoomable musical
   shapes/glyphs.
2. Text in the six languages with a bundled font.
3. **Headless screenshot to PNG without a window** (mandatory).
4. Software rendering without a GPU.
5. Measurements: frame time, CPU, RAM, compile time.
6. Status of accessibility, IME/text input, macOS global menu, native dialogs.
7. License compatibility; build verified on the three-OS CI.

## Decision

Pending the spike results; the comparison table and the recommendation will be recorded here.
