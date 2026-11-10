# ADR-0039 — The drawing abstraction: a segmented display list of themed commands

- **Status:** Accepted (owner, 2026-11-10; D1, D3, D6 and D8 chosen by the owner, option (a) each)
- **Date:** 2026-11-10
- **Implements:** ADR-0012 (our own renderer, headless rendering for free), ADR-0007 rule 4
  (incremental rendering) and rule 5 (software fallback), ADR-0014 §2 (no literal colour in
  widgets or renderers), ADR-0036 (`tiny-skia` 0.12). Leaves room for the GPU renderer of
  SCALETTA F2.06 and the text of F2.07 without building either.

## Context

Step F2.03 (SCALETTA) builds the layer between "what to draw" and "pixels": everything the
interface and the views will draw (widgets, the score, the DAW timeline) is first described as
**commands**, then a **backend** turns them into pixels. This is one of the structures that are
hard to change later (SCALETTA §1.3), so it is fixed here before the code.

Facts checked on 2026-11-10:

- `tiny-skia` 0.12.0 (2026-02-02, BSD-3-Clause) is still the latest release, the version pinned
  by ADR-0036. It fills and strokes paths with anti-aliasing, clips with masks, draws pixmaps,
  and has **no blur**.
- `vello` 0.11.0 (2026-10-02, Apache-2.0 OR MIT), the GPU candidate of F2.06, records a `Scene`
  that is itself a **flat encoded list** of fills, strokes, clip layers, images and glyph runs,
  and has `draw_blurred_rounded_rect` natively. A flat list of ours maps onto it almost one to
  one.
- The Phase 0 spike scene (`spikes/scene`: 10 staves × 20 measures, 2000 symbols, 1920 × 1080)
  is a ready-made reference for the benchmark.

On 2026-11-10 the owner chose the four open points (D1, D3, D6, D8 below, all option (a)).

## Decision

### D1 — A flat display list, divided into segments (owner's choice)

The `DisplayList` is a sequence of **segments**, drawn in order (later segments on top). Each
segment is a flat list of commands recorded by one piece of the interface (a widget, a region of
the score, a track of the timeline) and identified by an opaque `SegmentId` chosen by the caller.

- When a piece changes, the caller **re-records only its segment**. The list compares the
  segment's bounds before and after and adds both to the damage (D5). Segments that did not
  change are neither re-recorded nor redrawn outside the damage.
- `SegmentId` is opaque on purpose: whether widgets are retained or immediate (F2.08, D12 of
  SCALETTA) does not change this layer.
- Each segment keeps its own command vector and its own **path arena** (all the path points of
  the segment in one vector, commands refer to ranges of it), so recording a path does not
  allocate per path, and re-recording reuses the capacity of the previous recording. After the
  first frame, recording a segment of the same size **does not allocate** (tested, ADR-0016).
- A segment's bounds are computed from its commands (stroke half-widths, the anti-aliasing
  fringe and a shadow's blur extent included), never declared by hand.

Rejected: a retained tree of nodes with cached bounds (a compositor's model), more complex, not
what vello consumes, and it would anticipate the widget architecture of F2.08.

### D2 — Logical coordinates, the scale applied by the backend

Commands are in **logical pixels** (`f32`), y down, origin top left. The backend multiplies by
the window's scale factor (DPI, F2.04) when it draws, so a list recorded once is valid at any
scale and in the headless renderer. Transforms in the list are limited to **translate and
uniform scale**, pushed and popped (enough for scrolling and zooming a score); rotation and skew
are not needed by the interface and stay out.

### D3 — Commands carry theme tokens, not colours (owner's choice)

Every paint is a `ColorToken` of the active theme (ADR-0037), optionally with an extra opacity
(`u8`, multiplied with the token's own alpha) for fades and overlays. The backend resolves tokens
**at draw time** with the `Theme` it is given. Consequences:

- changing theme or mode only **redraws** (full damage), it never re-records;
- no literal colour can enter the drawing: the structure enforces ADR-0014 §2, not discipline.

The only pixels that do not come from tokens are **images** (D4), which are content.

### D4 — The command set

- fill a rectangle; fill a rounded rectangle (one radius for all corners);
- stroke a rectangle or a rounded rectangle; a straight line;
- fill a path (non-zero or even-odd rule); stroke a path (width, caps, joins); paths are lines,
  quadratic and cubic Béziers, closed or open;
- a **blurred rounded rectangle** (D6);
- an **image**: an RGBA picture registered once in the list's image store and drawn into a
  destination rectangle (video thumbnails, waveforms rendered offline, icons later);
- push and pop a **clip** (rectangle or rounded rectangle); push and pop a **transform** (D2).

Not now (SCALETTA §1.6, no work "for later"): **text and glyph runs** (F2.07 adds them as a new
command, with the glyph cache it decides), gradients, dashed lines, blend modes. The command enum
is internal to the crate, so adding a variant breaks no public caller.

### D5 — Damage: up to eight rectangles, else everything

The damage of a frame is a list of at most **8 rectangles**, kept in logical pixels by the list
(which does not know the scale) and rounded outwards to whole device pixels, plus one pixel of
anti-aliasing fringe, by the backend when it draws. A new rectangle that overlaps or touches one already there is merged with it; when a ninth
would be needed, the two whose union adds the least area are merged. When the damaged area
exceeds **half the frame**, the frame is redrawn whole (one rectangle), because clipping many
regions then costs more than it saves.

Redrawing a damaged rectangle means: clip to it, clear it with the theme's `surface`, and replay
**only the commands whose bounds intersect it** (culling by the precomputed bounds of D1). The
damage list is fixed-capacity: computing it never allocates.

### D6 — Shadows: a blurred rounded rectangle, analytic (owner's choice)

ADR-0037 has the shadow tokens (`shadow` colour, `shadow-blur`, `shadow-offset`). The command is
a rounded rectangle, a blur radius (the standard deviation, in logical pixels) and a token. The
`tiny-skia` backend computes it **analytically**, like vello's native command (whose formula
differs): along x the integral of the Gaussian over the shape's row is exact (two `erf`, with
the row's width shortened by the corner's circle); along y the integral is approximated with
four samples weighted by the Gaussian over ±3σ. This is the well-known method for blurred
rounded boxes; for a plain rectangle it is exact along x and very close along y. Only the
pixels inside the blurred bounds (the rectangle grown by 3σ) are touched, and no mask buffer is
allocated per shadow. A blur of 0 is an ordinary filled rounded rectangle.

### D7 — Crisp lines: snapping to device pixels

Axis-aligned rectangles, rectangle strokes and horizontal or vertical lines are **snapped** to
the device pixel grid by default: edges are rounded to whole device pixels, and a line of odd
device width is centred on a pixel centre. Staff lines, barlines, stems and widget borders then
stay sharp instead of spreading over two half-transparent rows. Paths and rounded corners are
not snapped (their anti-aliasing is what makes them smooth).

### D8 — Headless: an image in memory; PNG comes with the screenshots (owner's choice)

The headless renderer draws a list into an in-memory RGBA image of a given size and scale, with
no window and no display. Tests compare pixels in memory; **no PNG is written in F2.03**. PNG
encoding arrives with `cargo xtask screenshots` (F2.05), together with `tiny-skia`'s
`png-format` feature and the `png`/`miniz_oxide` duplicate ADR-0036 B7 already lists.

### D9 — Backends behind one trait

A `Renderer` trait takes the list, the theme, the damage and the target. F2.03 implements it with
`tiny-skia` (features `std` and `simd`, no `png-format`), for the window (F2.04, through
`softbuffer`) and for the headless image. The GPU backend (`vello`) implements the same trait in
F2.06 if that step adopts it; the list does not change.

### D10 — Tests and benchmark

- **Pixels:** small scenes drawn headless, checked with exact probes on chosen pixels (inside,
  outside, on the snapped edge) and against a reference rendering with a **tolerance** of one
  unit per channel, because `tiny-skia`'s SIMD paths may differ by one unit between x86-64 and
  ARM. CI on the three platforms measures the actual difference; the tolerance is tightened if it
  is zero.
- **Damage:** merging, the eight-rectangle limit, the half-frame rule, culling (a command outside
  the damage is not replayed), old and new bounds of a re-recorded segment.
- **Tokens:** the same list drawn with two themes gives each theme's colours; a theme change is
  full damage without re-recording.
- **Allocations:** re-recording a segment of the same size and computing the damage never
  allocate after the first frame.
- **Benchmark** (Gungraun, instruction counts, ADR-0016): record and draw the reference scene at
  1920 × 1080 (staff lines, barlines and stems as lines, noteheads and accidentals as paths,
  since glyphs come in F2.07): a whole frame, and the frame after one note changes (one segment
  re-recorded, small damage). Budget: ≥ 30 fps in software for scrolling, so ≤ 33 ms per frame;
  by the project's rule (one instruction per nanosecond, set in F1.16c) that is ≤ 33 M
  instructions per whole frame, to be confirmed in real time in F2.23.

## Alternatives rejected

- **A retained tree** (see D1).
- **Resolved colours in commands** (rejected by the owner): every theme change would re-record
  everything, and nothing in the structure would stop a literal colour.
- **Shadows in F2.09** (rejected by the owner): the command set would be incomplete when the
  first widgets need it.
- **PNG reference files in the repository now** (rejected by the owner): they would bring the
  `png` crate and a `miniz_oxide` duplicate before F2.05 needs them.
- **`kurbo` as our geometry type** (it is vello's): one more dependency now for a handful of
  plain structs; converting our points to kurbo in F2.06 is trivial.

## Consequences

- `tiny-skia` 0.12.0 enters the workspace in `orchestra-ui` (pinned, ADR-0036 features minus
  `png-format`). Its dependencies are measured in this step and must keep `cargo deny` green.
- The module `draw` of `orchestra-ui` is the only place that knows about `tiny-skia`; the rest of
  the interface records commands.
- F2.04 presents the image drawn here through `softbuffer`, F2.05 encodes it as PNG, F2.06 adds
  (or not) a second backend, F2.07 adds glyph runs, F2.08 decides who owns the segments.
