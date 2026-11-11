# ADR-0042 — The interface renderer: `vello_cpu` behind our display list, no GPU for now

- **Status:** Accepted (owner, 2026-11-11; option B with points 1–6). **R1, R3, R5 and R6
  superseded by ADR-0043** (2026-11-11): the spike's comparison was confounded by a per-frame
  shadow; `tiny-skia` stays, with cached shadows. R2 (no GPU for now) and R4 stand.
- **Date:** 2026-11-11
- **Based on:** the spike `spikes/gpu-render` (SCALETTA F2.06), figures in its `RESULTS.md`.
- **Amends:** ADR-0039 D9 (the backend behind the `Renderer` trait), ADR-0036 B1 and B10 (a new
  building block; the GPU path decided here), ADR-0012 (the "vello (GPU) evaluated in Phase 2"
  row of its block table).

## Context

ADR-0012 chose our own UI on `tiny-skia` and left the GPU path (`wgpu` + `vello`) to be measured
in Phase 2; ADR-0039 D9 put the CPU backend behind a `Renderer` trait so a GPU backend could be
added without changing the display list. F2.03 already showed that our `tiny-skia` backend
misses the full-frame budget of the reference scene (84.6 M instructions against 33 M).

The spike drew the reference scene of F2.03 (4216 operations, 1920 × 1080 and 3840 × 2160) with
four renderers on the development machine, an Intel N150 with its integrated GPU (weaker than the
reference hardware of CLAUDE.md §5), and the owner judged each one in a window by eye.
Median frame times in ms (full / one note edited / scroll + zoom):

| Renderer | 1920 × 1080 | 3840 × 2160 | Cold start | RSS |
|---|---|---|---|---|
| our `CpuRenderer` on `tiny-skia` (segments + damage) | 21.4 / **0.1** / 19.4 | 68.2 / **0.1** / 64.3 | 27 ms | 13–37 MiB |
| `vello_cpu` 0.3, 3 worker threads | **7.1** / 7.3 / **6.8** | 15.7 / 16.2 / 15.0 | 11 ms | 20–51 MiB |
| `vello` 0.11, `wgpu` 30, Vulkan | 9.7 / 12.3 / 14.4 | 19.4 / 19.0 / 19.7 | 1.4 s | 74–139 MiB |
| `vello` 0.11, `wgpu` 30, GL | 7.2 / 9.5 / 13.5 | **11.4** / 12.8 / 15.0 | 4.4 s | 84–180 MiB |
| `vello_hybrid` 0.2, `wgpu` 29, Vulkan | 11.5 / 11.8 / 12.7 | 21.0 / 29.4 / 25.0 | 0.28 s | 67–76 MiB |

In the window (vsync, owner's desktop): `tiny-skia` 37–50 fps, `vello_cpu` 99–108 fps and the
smoothest, `vello` 68–95 fps, `vello_hybrid` 61–98 fps and "a little jerky". The GPU variants
add about 40 crates, 2 minutes of clean build and 8–9 MB of binary; Windows (DX12) and macOS
(Metal) could not be measured (D18). The vello family has no retained segments, so every edit
costs a full frame; our design redraws one edited note in 0.1 ms.

The owner chose option B on 2026-11-11 (A: keep `tiny-skia`; C: GPU by default with a CPU
fallback; D: `vello_hybrid`), with points 1–6 below.

## Decision

### R1 — `vello_cpu` rasterises, our display list stays

The interface keeps everything ADR-0039 built: the segmented display list, themed commands, the
damage, pixel snapping (D7), the `Renderer` trait. A new backend implements that trait with
**`vello_cpu`** (multi-threaded), replacing `tiny-skia` as the rasteriser of the window and of the
headless screenshots (ADR-0041). Partial redraw is kept: the backend rasterises only the damaged
regions, so an edit stays in the order of the 0.1 ms measured today. No GPU code enters the
product.

### R2 — When the GPU comes back

The GPU path is re-evaluated, by re-running `spikes/gpu-render` (kept in the repository) on the
then current versions, when either happens:

- a measured case (the phase benchmarks of F2.23, HiDPI screens, larger views) misses its budget
  **with `vello_cpu`** on the reference hardware;
- or `vello`'s costs drop markedly: cold start (shader compilation), binary size and crates, and
  DX12/Metal can be measured (D18).

### R3 — Worker threads and the audio thread

`vello_cpu` renders with worker threads. Their number is **configurable**, by default the number
of cores minus one, and never all of them: at least one core stays free for the audio thread
(4 cores → 3 workers today). The audio thread will run at real-time priority, so it should
preempt rendering, but this is **measured in F3** with the reference project (zero xruns in
10 minutes while the score scrolls) before the default is final.

### R4 — The pixels we keep

- ADR-0039 D7 pixel snapping stays ours (in the recorder/backend, not in the library).
- The analytic shadow of ADR-0039 D6 is replaced by `vello_cpu`'s `fill_blurred_rounded_rect` if
  it meets the same accuracy test; otherwise ours stays.
- The pixel tests of `crates/ui/tests/draw.rs` become the acceptance test of the new backend;
  where the result is exact by design they stay exact, elsewhere the tolerance is re-measured.

### R5 — How it enters: step F2.06b

A new step **F2.06b** implements the backend: the `Renderer` with `vello_cpu`, partial redraw,
the thread setting, benchmarks (instruction counts on one thread through Gungraun, wall-clock for
the multi-threaded case as a local measurement), and the comparison with `tiny-skia` on the
reference scene. `tiny-skia` leaves the workspace only when the new backend passes every test;
PNG encoding then moves to the `png` crate already in the tree (ADR-0041 S6 keeps its meaning).

### R6 — A young dependency

`vello_cpu` is at 0.3.0 (2026-10-02, Apache-2.0 OR MIT) and its API changes between minor
versions. It is pinned with a caret requirement on the verified version, as ADR-0036 B1 does for
the other blocks, and updated in a step of its own. Its `text` feature is decided in F2.07 (glyphs
may come from our own cache instead).

### Amendments

- **ADR-0039 D9:** the backend is `vello_cpu` (R1) from F2.06b on; `tiny-skia` until then.
- **ADR-0036 B1:** `vello_cpu` 0.3 joins the blocks (R6). **B10:** the GPU path is decided here:
  not now (R2).
- **ADR-0012:** "`vello` (GPU) evaluated in Phase 2" is answered: measured and postponed (R2); the
  software renderer is `vello_cpu` behind our list.

## Alternatives rejected

- **A, keep `tiny-skia`:** 21 ms full frames at 1080p and 68 ms at 4K (below 30 fps), with no
  headroom for text, which is not drawn yet.
- **C, `vello` on the GPU by default with a CPU fallback:** not faster at 1080p on the measured
  hardware, faster at 4K only on GL (whose cold start is 4.4 s, beyond the 3 s budget alone),
  +8–9 MB, +40 crates, +50–150 MiB of RAM, and two renderers to keep pixel-compatible; Windows and
  macOS unmeasured.
- **D, `vello_hybrid`:** version 0.2, visible jitter in the owner's test, a different `wgpu` major
  from `vello`.
- **`vello_cpu` without our display list** (re-encoding the whole scene every frame): loses the
  0.1 ms edits that segments and damage give.

## Consequences

- A rewrite of the backend of ADR-0039 (F2.06b) before text (F2.07): the glyph cache of F2.07 is
  then built for `vello_cpu`.
- About 25 small crates more (`vello_common`, `fearless_simd`, `peniko`, `kurbo`, `rayon`,
  `crossbeam`…), some of which F2.07 would bring anyway; `tiny-skia` and its crates leave later.
- The UI uses several threads to draw; F3 must show it does not disturb audio (R3).
- No GPU driver, shader cache or `wgpu` in the product: start-up, memory and the three platforms
  stay simple, at the price of re-checking R2 as screens and scores grow.
