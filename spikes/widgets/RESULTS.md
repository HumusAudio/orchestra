# Spike F2.08 — retained against immediate widgets, taffy, AccessKit

Measured on 2026-11-12 on the development machine (Intel N150), instruction counts with
Gungraun 0.20 (Callgrind), release profile with symbols. Rule of the project: 1 instruction ≈
1 ns on the reference laptop (F1.16c).

## What was built

`src/lib.rs`: a settings panel of 66 rows, each a label (shaped Inter text, 13 px), a checkbox
and a button with a text: 198 widgets, each recording into its own display-list segment, drawn
by the real `CpuRenderer` into a 510 × 1868 image, with an AccessKit node per widget.

- **A, retained**: widgets live between frames with their rectangle, laid-out text and state; an
  event changes one widget, re-records its segment and sends AccessKit its node only.
- **B1, immediate, pure**: the UI function runs again at every event: lays the texts out, places
  the widgets, re-records every segment, builds the whole AccessKit tree.
- **B2, immediate with a text cache**: B1 keeping laid-out texts by content between runs.

Every case starts from the panel already shown (fonts parsed, glyph cache warm).

## Results (instructions)

| Case | A retained | B1 immediate | B2 immediate + text cache |
|---|---|---|---|
| first frame (texts, layout, record, draw, full AccessKit tree) | 40.6 M | — | — |
| click on a checkbox, end to end | **0.17 M** | 33.2 M | 27.2 M |
| pointer onto a button, end to end | **0.25 M** | — | 27.2 M |
| click, UI work only (no drawing) | **0.002 M** | 7.26 M | 1.24 M |

- In the immediate ways every segment is re-recorded, so the whole panel is damaged and redrawn:
  about 26 M of the 27 M. Avoiding that means comparing each widget's new recording with the
  previous one, which is keeping retained state after all.
- B1's own work is mostly shaping 132 texts again (≈ 45 000 instructions each); B2's cache
  removes it, and what is left (1.24 M) is recording 198 segments and building 199 AccessKit
  nodes, every event.
- A's click costs 1 792 instructions of UI work; the rest is drawing the damaged checkbox.
- The first frame (40.6 M) is mostly drawing a 510 × 1868 image and rasterising the glyphs into
  the cache once.

## Layout

| Case | Instructions |
|---|---|
| our column layout, 198 widgets | 3 337 |
| `taffy` 0.15.1, flex column of rows, first layout | 1.61 M |
| `taffy`, after one leaf changed (its cache spares clean subtrees) | 0.31 M |

`taffy` (MIT, maintained, 0.15.1 of 2026-10-25) with `flexbox` and `grid`: +362 KB stripped, 4
crates, ≈ 19 s of clean build. Our column is trivially cheap because it is fixed; a real layout
of our own (stack, row, column, grid with fixed, fill and content sizes) would cost more, but
stays linear.

## AccessKit

`accesskit` 0.25.2, `accesskit_winit` 0.34.2 and `accesskit_unix` 0.24.1 are still the latest
(2026-10-25); `accesskit_unix` still uses zbus 5.19, so ADR-0038 S7's exceptions in `deny.toml`
stay. A tree update carries only the nodes that changed: with retained widgets that is one node
per click; with immediate widgets the natural update is the whole tree (199 nodes) at every
event, unless a diff is kept.

## Clipboard (for F2.10)

`arboard` 3.7.0 (2026-10-15, MIT OR Apache-2.0) is the latest; to be evaluated in F2.10.
