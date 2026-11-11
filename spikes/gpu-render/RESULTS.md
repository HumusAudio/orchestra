# Spike F2.06 — a GPU renderer? Four renderers on the reference scene

Run on 2026-11-11 on the development machine: **Intel N150** (4 cores, Alder Lake-N, integrated
GPU "Intel Graphics (ADL-N)", 24 EU), 16 GB, CachyOS, Mesa 26.2.6, Rust 1.99.1, `release`
profile. The N150 is **weaker** than the reference hardware of CLAUDE.md §5 (i5-1135G7 with
Iris Xe, 80 EU): what holds here holds there. Versions checked on crates.io the same day:
`vello` 0.11.0 (2026-10-02), `vello_cpu` 0.3.0 (2026-10-02), `vello_hybrid` 0.2.0 (2026-08-07),
`wgpu` 30.0.1 (vello) and 29.0.4 (vello_hybrid), all Apache-2.0 OR MIT.

```sh
cd spikes
./gpu-render/run.sh 300          # builds 3 variants, measures every backend, appends to out/gpu-render/results.md
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
input to frame ≤ 50 ms.

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
| `vello` (GL) | 15 ms | **4200 ms / 3200 ms** | 140 ms | **4.4 s / 3.4 s** | 143–180 / 197–235 MiB |
| `vello_hybrid` (Vulkan) | 35 ms | 165 ms / 14 ms | 50 ms | 0.28 s / 0.12 s | 67–76 / 82–91 MiB |
| `vello_hybrid` (GL) | 14 ms | 335 ms | 20 ms | 0.39 s / 0.48 s | 104 / 119 MiB |

`vello` on GL has no working shader cache here: even warm it needs 3.2 s of pipelines, beyond the
3 s start-up budget on its own.

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

OWNER_WINDOW

## What the numbers say

CONCLUSIONS
