# ADR-0043 — Keep `tiny-skia`, cache shadows; `vello_cpu` is dropped

- **Status:** Accepted (owner, 2026-11-11; points 1–5 as proposed)
- **Date:** 2026-11-11
- **Supersedes:** ADR-0042 R1, R3, R5 and R6 (`vello_cpu` as the rasteriser, its worker threads,
  the step that introduced it, the dependency). ADR-0042's other decisions stand: **no GPU for
  now** and the conditions to measure it again (R2).
- **Amends:** ADR-0039 D6 (shadow coverage is cached between frames).

## Context

ADR-0042 chose `vello_cpu` over our `tiny-skia` backend because the F2.06 spike measured it about
three times faster (7 against 21 ms a 1080p frame, 16 against 68 ms at 4K). Implementing it in
step F2.06b showed that comparison was **confounded**: in the spike, `tiny-skia` was our
`CpuRenderer`, which computed the panel's shadow pixel by pixel with our analytic formula at every
frame, while `vello_cpu` used its own cheap blur. That blur turned out to be 28–40 steps of 255 off
our formula, so the product had to keep ours anyway (ADR-0042 R4).

Measured again in the product, on the spike's scene (staves, 2000 noteheads as paths, a panel),
full frames in ms on the development machine (Intel N150):

| | 1080p, with shadow | 1080p, without | 4K, with shadow | 4K, without |
|---|---|---|---|---|
| `tiny-skia` (shadow recomputed every frame) | 25.8 | **11.1** | 83.5 | **23.8** |
| `vello_cpu`, curves flattened by us at 0.05 px, 3 threads | 10.7 | 11.7 | 24.9 | 24.6 |
| `vello_cpu`, its own 0.25 px flattening, 3 threads | 7.3 | 6.8 | 19.4 | 21.0 |

Without the shadow's cost, `vello_cpu` with the accuracy the owner chose (curves flattened by us,
because its fixed 0.25 px tolerance shrank noteheads by 4.4 % at zoom 1 and 9.8 % at zoom 0.5)
is **as fast as `tiny-skia`**: our flattening runs on one thread and eats the workers' gain. Only
its own coarse flattening is faster, by 10–40 %, at the cost the owner had refused. `tiny-skia`
meets the budgets without it (11 ms at 1080p, under 16.7 ms; 24 ms at 4K, under 33 ms), on one
thread, with more accurate edges and fewer dependencies.

The owner approved this ADR on 2026-11-11 with points 1–5 below.

## Decision

### K1 — `tiny-skia` stays the rasteriser

`CpuRenderer` stays on `tiny-skia` (ADR-0039 D9 as written). `vello_cpu` leaves the workspace
before it was ever merged: no new dependency, no drawing threads. With it go
`default_render_threads`, `WindowOptions::render_threads`, and the F3.04 measurement of xruns with
drawing threads (ADR-0042 R3).

### K2 — Shadow coverage is cached

A shadow's coverage (our formula, ADR-0039 D6) depends only on its shape, radius and blur in the
frame's device pixels, not on colour, theme or the region being redrawn. The renderer keeps it
between frames (up to 32 shadows, least recently used evicted) and each frame only blends it, with
the same single floating-point rounding as before (within one step of the formula, tested on the
first and the cached frame). This removes the cost that made the spike's comparison misleading.

### K3 — What F2.06b keeps

The pixel-snapping rules move to their own module (`draw/snap.rs`), ready for any future backend;
the tests written for the new backend stay and now guard ours: partial redraw of a busy scene,
repeated renders give the same bytes, small noteheads keep their area (a coarse flattening, like
`vello_cpu`'s, fails it), shadow accuracy (computed and cached). A new instruction-count benchmark,
`draw_shadowed_panel_frame`, measures a repaint with a cached shadow. The spike's `RESULTS.md`
gains a correction section.

### K4 — A rule for comparing renderers

A measurement that compares renderers first separates costs that do not belong to the rasteriser
(a shadow computed by our own code in one candidate only, a different blur, a different curve
tolerance): each candidate is measured with and without them, and the comparison uses the like
for like figures. ADR-0042 R2's re-measurement of the GPU follows this rule.

### K5 — The plan

Step F2.06b becomes "shadow cache and the corrected measurement"; SCALETTA is updated with the
owner's approval at the end of the step.

## Alternatives rejected

- **Keep `vello_cpu` with curves flattened by us (ADR-0042 as amended in F2.06b):** no faster than
  `tiny-skia`, plus 25 crates, worker threads near the audio thread, 8-bit compositing (shadows 2
  steps off instead of 0.6, alpha 254 to repair).
- **Keep `vello_cpu` with its own flattening:** 10–40 % faster, with noteheads 4–10 % smaller; the
  owner had refused that accuracy. Noteheads become cached glyphs in F2.07, but the UI's small
  round shapes would stay affected.

## Consequences

- F2.07 builds the glyph cache for `tiny-skia`, as planned before ADR-0042.
- The GPU question stays open on ADR-0042 R2's conditions, measured with K4's rule.
- A day of implementation was spent on a backend that is not kept; the measuring rule K4 is the
  lesson recorded for next time.

## Precision of F2.09b2 (owner, 2026-11-12): rounded rectangles are cached too

Measuring the first real buttons (ADR-0045, F2.09b) showed a redraw of a button cost ≈ 0.9 M
instructions, almost all of it `tiny-skia` filling and stroking its anti-aliased rounded rectangle,
above ADR-0045's 0.5 M per event. The owner chose to extend K2's idea to shapes: the coverage of
rounded rectangles (filled, or stroked through `tiny-skia`'s stroke-to-outline) is kept in a mask
cache of 4 MB, keyed by size, radius, stroke width, join and sub-pixel phase in device pixels (exact
bits) and rasterised from the key alone, so pixels never depend on which shape filled the entry.
Shapes over 512 device pixels a side are still filled as paths. The glyph cache's code (ADR-0044
TX8) became generic (`MaskCache<K>`) and serves both. Result: a button's redraw ≈ 0.37 M, the
shadowed panel frame −2%, no other benchmark changed; cached shapes match `tiny-skia`'s direct
drawing within 3 steps of 255, but for a few corner pixels of curved strokes (≤ 16 steps).


## Precision of F2.09d (owner, 2026-11-13): a rectangular clip's mask is reused

A clip costs `tiny-skia` a mask the size of the pixmap being drawn (one byte a pixel), built from
the clip's path. The scroll area of F2.09d clips the widgets that cross its viewport's edge, all to
the same rectangle: with a mask built for each of them, one wheel notch over 200 real widgets cost
38.3 M instructions (24 M of them masks), above the 33 M of a frame. The owner chose to **keep the
mask of the last outermost rectangular clip** of the region being drawn and reuse it for the next
segment whose clip is the same snapped device rectangle (exact bits). It is dropped at the start of
every region (another pixmap); nested and rounded clips are built as before, and a mask is never
attached to another clip's key. Result: the notch at 17.7 M (14.4 M with no masks at all); no other
benchmark changed. B (rectangular clips without masks: rectangles and glyphs cut directly, a mask
only for paths crossing the edge) stays available if a measure ever asks for it.
