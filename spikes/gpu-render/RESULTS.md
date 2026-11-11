# Spike F2.06 — a GPU renderer? Four renderers on the reference scene

Run on 2026-11-11 on the development machine: **Intel N150** (4 cores, Alder Lake-N, integrated
GPU "Intel Graphics (ADL-N)", 24 EU), 16 GB, CachyOS, Mesa 26.2.6, Rust 1.99.1, `release`
profile. The N150 is **weaker** than the reference hardware of CLAUDE.md §5 (i5-1135G7 with
Iris Xe, 80 EU): what holds here holds there. Versions checked on crates.io the same day:
`vello` 0.11.0 (2026-10-02), `vello_cpu` 0.3.0 (2026-10-02), `vello_hybrid` 0.2.0 (2026-08-07),
`wgpu` 30.0.1 (vello) and 29.0.4 (vello_hybrid), all Apache-2.0 OR MIT.

```sh
cd spikes
./gpu-render/run.sh 300          # builds 3 variants, measures every backend at scale 1 and 2, appends to out/gpu-render/results.md
out/gpu-render/bin/gpu-render-gpu window vello        # by eye: tiny-skia | vello-cpu | vello-cpu-mt | vello
out/gpu-render/bin/gpu-render-hybrid window hybrid
```

The GPU numbers must be taken **outside the Claude Code sandbox**: inside it there is no access to
the GPU and wgpu silently falls back to a software Vulkan driver (lavapipe: 44 ms a frame,
meaningless). The report prints the adapter, so a wrong run is visible.

## The scene and the cases

The reference scene of F2.03 (`crates/ui/benches/draw.rs`): 10 staves × 20 measures at
1920 × 1080, 2000 noteheads as filled ellipses (four cubics each: glyphs come in F2.07), stems,
staff lines and barlines as 1 px rectangles, plus a widget panel (blurred shadow, rounded surface,
six rounded controls with borders, a focus ring): **4216 operations**, described once in
`src/scene.rs` and drawn identically by every backend. The PNGs of the four renderers differ only
on anti-aliased edges (at most 39–42 of 255 on notehead borders), as expected from different
coverage algorithms.

- **full**: the whole static frame, drawn again (a window repaint).
- **edit**: one note in the middle of the frame moves back and forth. `tiny-skia` goes through our
  `CpuRenderer` exactly as the product does: the measure's segment is re-recorded and only the
  damage is drawn (ADR-0039). The vello family has no retained segments: every edit is a full
  re-encode and a full frame.
- **scroll**: every frame scrolls 4 px and zooms between 0.8 and 1.25 (the whole scene re-encoded
  under a new transform; our `CpuRenderer` re-records every segment).

300 frames per case after 10 warm-up frames; median / 95th percentile / worst, in ms. GPU frames
are timed until the GPU has finished (`poll` waits), so they are the latency of one frame with
encoding, submission and execution in series; a window pipelines CPU and GPU work, so its
throughput can be better (see the window mode).

## Frame times (ms, median / p95 / worst)

| Backend | full frame | edit one note | scroll + zoom |
|---|---|---|---|
| `tiny-skia` (our `CpuRenderer`, 1 thread) | 21.4 / 22.5 / 25.2 | **0.1** / 0.1 / 0.1 | 19.4 / 26.4 / 29.4 |
| `vello_cpu` 1 thread | 10.4 / 10.8 / 11.4 | 10.4 / 11.4 / 12.1 | 9.4 / 11.4 / 13.1 |
| `vello_cpu` 3 worker threads | **7.1** / 8.5 / 9.7 | 7.3 / 8.8 / 11.1 | **6.8** / 8.5 / 9.5 |
| `vello` (Vulkan) | 9.7 / 10.7 / 12.3 | 12.3 / 13.4 / 14.5 | 14.4 / 16.2 / 25.2 |
| `vello` (GL 4.6) | 7.2 / 8.4 / 14.5 | 9.5 / 11.0 / 12.4 | 13.5 / 14.5 / 15.1 |
| `vello_hybrid` (Vulkan) | 11.5 / 16.0 / 17.2 | 11.8 / 16.3 / 18.2 | 12.7 / 16.3 / 17.4 |
| `vello_hybrid` (GL 4.6) | 12.0 / 15.6 / 17.9 | 10.9 / 15.7 / 16.5 | 10.9 / 15.5 / 16.8 |

Budgets (CLAUDE.md §5): scrolling ≥ 30 fps in software (33 ms) and 60 fps with the GPU (16.7 ms);
input to frame ≤ 50 ms. A second full run gave the same figures within ±1 ms.

### At 3840 × 2160 (`--scale 2`: a 4K screen, or 1080p at 200 %)

The same scene, four times the pixels (the renderers draw at scale 2, so lines stay 2 px crisp).

| Backend | full frame | edit one note | scroll + zoom |
|---|---|---|---|
| `tiny-skia` (our `CpuRenderer`) | 68.2 / 78.0 / 78.8 | **0.1** / 0.1 / 0.2 | 64.3 / 94.7 / 105.9 |
| `vello_cpu` 1 thread | 26.0 / 37.7 / 44.9 | 26.0 / 27.9 / 29.6 | 23.5 / 28.6 / 32.0 |
| `vello_cpu` 3 worker threads | 15.7 / 18.1 / 26.9 | 16.2 / 18.3 / 20.8 | 15.0 / 17.8 / 20.4 |
| `vello` (Vulkan) | 19.4 / 21.6 / 24.2 | 19.0 / 19.8 / 23.4 | 19.7 / 21.0 / 22.4 |
| `vello` (GL 4.6) | **11.4** / 12.3 / 14.5 | 12.8 / 14.0 / 16.7 | 15.0 / 16.3 / 17.3 |
| `vello_hybrid` (Vulkan) | 21.0 / 27.9 / 31.0 | 29.4 / 31.8 / 32.8 | 25.0 / 29.7 / 32.5 |
| `vello_hybrid` (GL 4.6) | 25.6 / 28.8 / 32.2 | 26.0 / 29.7 / 34.9 | 24.9 / 30.3 / 34.6 |

At 4K our `tiny-skia` path no longer reaches 30 fps on a full frame (68 ms) while editing stays at
0.1 ms; `vello_cpu` on 3 threads stays near 60 fps (15–16 ms); only `vello` on GL is clearly
faster than it (11–15 ms). Peak memory roughly doubles for the CPU paths (≈ 100 MiB: the frame
alone is 32 MiB) and grows by 50–60 MiB for the GPU ones.

## Start-up and memory

"Renderer" is building the pipelines, i.e. compiling shaders. Mesa keeps a shader cache on disk,
so the first start (or the first after a driver update) is **cold**; cold runs below use
`MESA_SHADER_CACHE_DISABLE=true`. "From start" is process start to the first finished frame.

| Backend | device | renderer (cold / warm) | first frame | from start (cold / warm) | RSS (now / peak) |
|---|---|---|---|---|---|
| `tiny-skia` | — | 0 | 26 ms | 27 ms | 13 / 29 MiB |
| `vello_cpu` 1 thread | — | 3 ms | 11 ms | 15 ms | 16 / 31 MiB |
| `vello_cpu` 3 threads | — | 4 ms | 6 ms | 11 ms | 20 / 32 MiB |
| `vello` (Vulkan) | 35 ms | **1120 ms** / 30 ms | 160–220 ms | **1.4 s** / 0.34 s | 74–139 / 89–149 MiB |
| `vello` (GL) | 8–15 ms | **4200 ms** / 105 ms | 135 ms | **4.4 s** / 0.27 s | 84–180 / 100–235 MiB |
| `vello_hybrid` (Vulkan) | 35 ms | 165 ms / 14 ms | 50 ms | 0.28 s / 0.12 s | 67–76 / 82–91 MiB |
| `vello_hybrid` (GL) | 8–14 ms | 335 ms / 44 ms | 20 ms | 0.39 s / 0.09 s | 74–104 / 90–119 MiB |

Cold, `vello` on GL needs 4.2 s of pipelines, beyond the 3 s start-up budget on its own; on Vulkan
1.1 s, more than a third of it. Warm, every GPU variant starts in 0.1–0.35 s. (A first reading of
"3.2 s warm" on GL was wrong: it ran right after the cold runs, which do not fill the cache.)

## Size, crates, build time

Clean `release` builds of the spike binary on the N150 (stripped, `opt-level 3`, default codegen
units), one at a time with nothing else running. The CPU variant already contains `orchestra-ui`
(winit, softbuffer, zbus, tiny-skia) **and** `vello_cpu`, so the GPU rows show what `wgpu` and the
GPU renderer add on top.

| Variant | Crates (Linux) | Clean build | Binary |
|---|---|---|---|
| CPU (`orchestra-ui` + `vello_cpu`) | 175 | 244 s | 2.6 MB |
| + `vello` + `wgpu` 30 (+ window) | 215 (+40) | 356 s (+112 s) | 12.0 MB (+9.4 MB) |
| + `vello_hybrid` + `wgpu` 29 (+ window) | 219 (+44) | 372 s (+128 s) | 10.6 MB (+8.0 MB) |

`vello_cpu` itself (with the `multithreading` and default `text` features) adds about 25 crates to
`orchestra-ui`'s 149: `vello_common`, `fearless_simd`, `peniko`, `kurbo`, `color`, `rayon` and
`crossbeam`, and `skrifa`/`read-fonts`/`glifo` for text (`skrifa` comes with F2.07 anyway). The
GPU stack brings `naga` (the shader compiler), `ash` (Vulkan), `glow` + `khronos-egl` (GL),
`gpu-allocator`, `wgpu-core`/`wgpu-hal`. Most of the binary growth is `wgpu` + `naga`; the
default `wgpu` features compile every backend (Vulkan, GL, and on other systems DX12 and Metal).

No native library is linked at build time by any variant (`ldd`: only `libc`, `libm`,
`libgcc_s`): `wgpu` loads `libvulkan.so.1` and `libEGL.so.1` (+ `libwayland-egl.so.1`) at run
time, like `winit` and `softbuffer` (ADR-0036 B8).

## Window mode (by eye, owner)

Run by the owner on 2026-11-11 on their KDE Plasma desktop (Wayland), 1920 × 1080 window,
`AutoVsync`, scrolling and zooming continuously (the scroll advances 4 px **per frame**, so a
faster renderer also scrolls faster). Seconds 2 onwards:

| Backend | fps | our work per frame (average / worst) | RSS | first frame presented | Owner's impression |
|---|---|---|---|---|---|
| `tiny-skia` | 37–50 | 21–27 / 25–36 ms | 32 MiB | 49 ms | smooth |
| `vello_cpu` 3 threads | 99–108 | 9.2–10.1 / 13–15 ms | 40 MiB | 60 ms | smooth, clearly the fastest |
| `vello` (Vulkan) | 68–95 | 10.5–14.7 / 13–17 ms | 80 MiB | 308 ms | faster than tiny-skia, slower than vello_cpu |
| `vello_hybrid` (Vulkan) | 61–98 | 9.7–16.3 / 15–18 ms | 70 MiB | 177 ms | a little faster than vello, **a little jerky** |

The first attempt of the two GPU windows panicked: the spike's offscreen targets lacked
`TEXTURE_BINDING`, which the blitter needs (a spike bug, fixed; the bench figures do not use the
blitter and are unchanged).

## What the numbers say

Facts only; the decision is ADR-0042.

1. **On this hardware the GPU does not beat a good CPU renderer at 1080p.** `vello_cpu` on 3
   threads draws the full frame in 7 ms (bench) and runs the window at ~105 fps; `vello` needs
   9–14 ms and `vello_hybrid` 8–16 ms with more jitter.
2. **At 4K the picture shifts only a little.** Our `tiny-skia` path falls to 68 ms per full frame
   (below 30 fps); `vello_cpu` on 3 threads stays at 15–16 ms; only `vello` on GL is clearly faster
   (11–15 ms), and it is the variant with the worst cold start (4.4 s).
3. **Editing is where our current design wins.** Segments + damage redraw one note in 0.1 ms at
   both scales; every vello variant redraws the whole frame (7–30 ms). Any change of rasteriser
   should keep the display list and the damage.
4. **The GPU costs a lot outside the frame:** +40 crates, +2 min of clean build, +8–9 MB of
   binary, +50–150 MiB of RAM, 0.3–4.4 s of start-up depending on the shader cache and the API,
   and Windows/macOS (DX12/Metal) still unmeasured (D18).
5. `vello_cpu` adds ~25 small crates (part of them, `skrifa`/`read-fonts`, come with F2.07 anyway)
   and 3 worker threads: their interplay with the audio thread on a 4-core machine must be looked
   at before it ships (CLAUDE.md §5).
