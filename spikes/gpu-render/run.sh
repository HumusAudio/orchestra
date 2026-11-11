#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Spike F2.06: builds the three variants of gpu-render and measures every backend.
#
# The `gpu` (vello, wgpu 30) and `hybrid` (vello_hybrid, wgpu 29) features cannot share one binary
# usefully, and cargo writes every variant to the same target/release/gpu-render, so each build is
# copied to out/gpu-render/bin/ under its own name before the next one overwrites it.
#
# Usage (from spikes/):   ./gpu-render/run.sh [FRAMES]
# Output: out/gpu-render/results.md (appended) and one PNG per backend, to compare by eye.
# Needs the real GPU: run it on the desktop, NOT inside a sandbox (there wgpu falls back to a
# software Vulkan driver and the numbers mean nothing; the report prints the adapter name).
set -euo pipefail
cd "$(dirname "$0")/.."
frames="${1:-300}"
out=out/gpu-render
mkdir -p "$out/bin"

for variant in cpu gpu hybrid; do
    case "$variant" in
        cpu) features=() ;;
        gpu) features=(--features gpu) ;;
        hybrid) features=(--features hybrid) ;;
    esac
    cargo build --release -p gpu-render "${features[@]}"
    cp target/release/gpu-render "$out/bin/gpu-render-$variant"
done

# Each invocation measures its backends one per fresh process (see `bench` in main.rs).
"$out/bin/gpu-render-cpu" bench tiny-skia vello-cpu vello-cpu-mt --frames "$frames" --out "$out"
"$out/bin/gpu-render-gpu" bench vello --frames "$frames" --out "$out"
"$out/bin/gpu-render-hybrid" bench hybrid --frames "$frames" --out "$out"
# The GL backend: vello needs compute shaders, vello_hybrid should not. A failure is a result too.
WGPU_BACKEND=gl "$out/bin/gpu-render-hybrid" bench hybrid --frames "$frames" --out "$out" || true
WGPU_BACKEND=gl "$out/bin/gpu-render-gpu" bench vello --frames "$frames" --out "$out" || true

echo
echo "Window mode, to judge by eye (Esc closes):"
echo "  $out/bin/gpu-render-gpu window tiny-skia | vello-cpu | vello-cpu-mt | vello"
echo "  $out/bin/gpu-render-hybrid window hybrid"
