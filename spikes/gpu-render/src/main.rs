// SPDX-License-Identifier: GPL-3.0-or-later

//! Spike F2.06: is a GPU renderer worth it, and which one? The reference scene of F2.03 (10 staves
//! × 20 measures + a widget panel, 1920 × 1080) drawn by four renderers, measured the same way:
//!
//! | Backend | Where it rasterises | Build |
//! |---|---|---|
//! | `tiny-skia` through our `CpuRenderer` (segments + damage, ADR-0039) | CPU, 1 thread | always |
//! | `vello_cpu` 0.3 | CPU, 1 thread or several | always |
//! | `vello` 0.11 on `wgpu` 30 | GPU compute shaders | `--features gpu` |
//! | `vello_hybrid` 0.2 on `wgpu` 29 | CPU (strips) + GPU (fine raster) | `--features hybrid` |
//!
//! `vello` and `vello_hybrid` need different `wgpu` majors, so they are built separately.
//!
//! # Commands
//! - `gpu-render bench [BACKEND…] [--frames N] [--out DIR]` — headless, no window and no
//!   display: for each backend, start-up costs, then three cases of N frames each (default 300):
//!   `full` (the whole static frame), `edit` (one note moves back and forth), `scroll` (scroll and
//!   zoom every frame). Prints a Markdown table and writes it, plus one PNG per backend, to DIR
//!   (default `../out/gpu-render`). GPU frames are timed until the GPU has finished (`poll`
//!   waits), so they measure latency, not just submission.
//! - `gpu-render window BACKEND` — opens a 1920 × 1080 window that scrolls and zooms the scene
//!   continuously with vsync, printing frames per second: to judge smoothness by eye.
//!
//! Backends: `tiny-skia`, `vello-cpu`, `vello-cpu-mt`, `vello` (feature `gpu`), `hybrid`
//! (feature `hybrid`). Without names, `bench` runs every backend compiled in.
//!
//! Spike shortcuts (fine here, never in product code): errors panic, wall-clock timing.

mod cpu;
#[cfg(feature = "gpu")]
mod gpu;
#[cfg(feature = "hybrid")]
mod hybrid;
mod scene;
#[cfg(feature = "window")]
mod window;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use scene::{HEIGHT, Scene, View, WIDTH};

/// One renderer under test. Every method draws a complete frame into the backend's own target.
pub trait Backend {
    /// Human-readable name for the report.
    fn name(&self) -> String;
    /// Draws the whole scene under `view`, as when scrolling or zooming.
    fn draw_full(&mut self, view: View);
    /// The steady state of editing: the edited note is lifted (`true`) or back (`false`), at the
    /// identity view. Backends with retained segments redraw only the damage.
    fn draw_edit(&mut self, lift: bool);
    /// The last frame as premultiplied RGBA8 bytes, `WIDTH × HEIGHT × 4` (opaque scene, so
    /// premultiplied equals straight).
    fn read_back(&mut self) -> Vec<u8>;
    /// The CPU backends' frame, premultiplied RGBA8, without a copy (for the window). `None` on
    /// the GPU, whose frame stays in a texture.
    fn cpu_pixels(&self) -> Option<&[u8]> {
        None
    }
}

/// The backends compiled into this binary.
fn available() -> Vec<&'static str> {
    let mut names = vec!["tiny-skia", "vello-cpu", "vello-cpu-mt"];
    if cfg!(feature = "gpu") {
        names.push("vello");
    }
    if cfg!(feature = "hybrid") {
        names.push("hybrid");
    }
    names
}

/// Worker threads for `vello-cpu-mt`: what `vello_cpu` itself would choose (cores − 1, ≤ 8).
fn mt_threads() -> u16 {
    let cores = std::thread::available_parallelism().map_or(1, |n| n.get());
    (cores.saturating_sub(1)).clamp(1, 8) as u16
}

/// Creates a backend by name. Returns it with the time its creation took, split into "device"
/// (opening the GPU: instance, adapter, device; zero on the CPU) and "renderer" (pipelines and
/// shaders, buffers).
pub fn create(name: &str) -> (Box<dyn Backend>, Duration, Duration) {
    let scene = Scene::new();
    let start = Instant::now();
    match name {
        "tiny-skia" => (Box::new(cpu::OrchestraUi::new(scene)), Duration::ZERO, start.elapsed()),
        "vello-cpu" => (Box::new(cpu::VelloCpu::new(scene, 0)), Duration::ZERO, start.elapsed()),
        "vello-cpu-mt" => (
            Box::new(cpu::VelloCpu::new(scene, mt_threads())),
            Duration::ZERO,
            start.elapsed(),
        ),
        #[cfg(feature = "gpu")]
        "vello" => {
            let (backend, device, renderer) = gpu::Vello::headless(scene);
            (Box::new(backend), device, renderer)
        }
        #[cfg(feature = "hybrid")]
        "hybrid" => {
            let (backend, device, renderer) = hybrid::Hybrid::headless(scene);
            (Box::new(backend), device, renderer)
        }
        other => panic!("unknown or not compiled backend `{other}`; available: {:?}", available()),
    }
}

/// Median, 95th percentile and worst of a set of frame times, in milliseconds.
struct Stats {
    median: f64,
    p95: f64,
    max: f64,
}

impl Stats {
    fn of(mut samples: Vec<Duration>) -> Self {
        samples.sort();
        let ms = |d: Duration| d.as_secs_f64() * 1000.0;
        let at = |q: f64| ms(samples[((samples.len() - 1) as f64 * q).round() as usize]);
        Self { median: at(0.5), p95: at(0.95), max: ms(*samples.last().expect("frames > 0")) }
    }
}

/// Times `frames` calls of `draw(i)` after 10 warm-up calls (caches, lazily built pipelines).
fn time_frames(frames: usize, mut draw: impl FnMut(usize)) -> Stats {
    for i in 0..10 {
        draw(i);
    }
    let mut samples = Vec::with_capacity(frames);
    for i in 0..frames {
        let start = Instant::now();
        draw(i);
        samples.push(start.elapsed());
    }
    Stats::of(samples)
}

/// Resident memory of this process (`VmRSS`) and its peak (`VmHWM`), in MiB. Linux only; other
/// systems report `None` (the measurements run on the owner's Linux machine).
fn memory_mib() -> Option<(f64, f64)> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let field = |key: &str| -> Option<f64> {
        let line = status.lines().find(|l| l.starts_with(key))?;
        let kib: f64 = line.split_whitespace().nth(1)?.parse().ok()?;
        Some(kib / 1024.0)
    };
    Some((field("VmRSS:")?, field("VmHWM:")?))
}

/// Writes premultiplied, opaque RGBA8 pixels as a PNG (through tiny-skia's encoder).
fn write_png(path: &Path, rgba: Vec<u8>) {
    let size = tiny_skia::IntSize::from_wh(WIDTH, HEIGHT).expect("size");
    let pixmap = tiny_skia::Pixmap::from_vec(rgba, size).expect("pixel count");
    pixmap.save_png(path).expect("write png");
}

/// One backend's results, as a Markdown table row plus a details line.
fn bench_one(name: &str, frames: usize, out: &Path) -> (String, String) {
    let process_start = Instant::now();
    let (mut backend, device, renderer) = create(name);
    let first = Instant::now();
    backend.draw_full(View::IDENTITY);
    let first_frame = first.elapsed();
    let to_first_frame = process_start.elapsed();
    write_png(&out.join(format!("{name}.png")), backend.read_back());

    let full = time_frames(frames, |_| backend.draw_full(View::IDENTITY));
    let edit = time_frames(frames, |i| backend.draw_edit(i % 2 == 0));
    let scroll = time_frames(frames, |i| backend.draw_full(View::animated(i)));
    let memory = memory_mib();

    let ms = |d: Duration| d.as_secs_f64() * 1000.0;
    let row = format!(
        "| {} | {:.1} / {:.1} / {:.1} | {:.1} / {:.1} / {:.1} | {:.1} / {:.1} / {:.1} |",
        backend.name(),
        full.median,
        full.p95,
        full.max,
        edit.median,
        edit.p95,
        edit.max,
        scroll.median,
        scroll.p95,
        scroll.max,
    );
    let details = format!(
        "- **{}**: device {:.0} ms, renderer {:.0} ms, first frame {:.0} ms (from start {:.0} ms); \
         memory {}",
        backend.name(),
        ms(device),
        ms(renderer),
        ms(first_frame),
        ms(to_first_frame),
        memory.map_or("n/a".to_owned(), |(rss, peak)| format!("{rss:.0} MiB now, {peak:.0} MiB peak")),
    );
    (row, details)
}

/// `bench`: every requested backend, each in a **fresh process** so that memory, start-up and
/// caches of one backend never leak into another's numbers.
fn bench(args: &[String]) {
    let mut names = Vec::new();
    let mut frames = 300;
    let mut out = PathBuf::from("../out/gpu-render");
    let mut child = false;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--frames" => frames = it.next().and_then(|n| n.parse().ok()).expect("--frames N"),
            "--out" => out = PathBuf::from(it.next().expect("--out DIR")),
            "--child" => child = true,
            name => names.push(name.to_owned()),
        }
    }
    std::fs::create_dir_all(&out).expect("create output directory");

    if child {
        // Inside the fresh process: one backend, print the two lines for the parent.
        let (row, details) = bench_one(&names[0], frames, &out);
        println!("{row}\n{details}");
        return;
    }

    if names.is_empty() {
        names = available().into_iter().map(str::to_owned).collect();
    }
    let exe = std::env::current_exe().expect("own path");
    let mut rows = Vec::new();
    let mut details = Vec::new();
    for name in &names {
        eprintln!("measuring {name} ({frames} frames per case)…");
        let output = std::process::Command::new(&exe)
            .args(["bench", "--child", "--frames", &frames.to_string(), "--out"])
            .arg(&out)
            .arg(name)
            .output()
            .expect("run child");
        let text = String::from_utf8_lossy(&output.stdout);
        if !output.status.success() {
            eprintln!("{name} failed:\n{}", String::from_utf8_lossy(&output.stderr));
            rows.push(format!("| {name} | failed | | |"));
            continue;
        }
        let mut lines = text.lines();
        rows.push(lines.next().unwrap_or_default().to_owned());
        details.push(lines.next().unwrap_or_default().to_owned());
    }

    let report = format!(
        "## gpu-render bench — {frames} frames per case, {WIDTH} × {HEIGHT}, {ops} ops\n\n\
         {system}\n\n\
         Times in ms: median / 95th percentile / worst.\n\n\
         | Backend | full frame | edit one note | scroll + zoom |\n|---|---|---|---|\n{rows}\n\n{details}\n",
        ops = Scene::new().op_count(),
        system = system_line(),
        rows = rows.join("\n"),
        details = details.join("\n"),
    );
    println!("{report}");
    let path = out.join("results.md");
    let mut all = std::fs::read_to_string(&path).unwrap_or_default();
    all.push_str(&report);
    all.push('\n');
    std::fs::write(&path, all).expect("write results");
    eprintln!("appended to {} (PNGs next to it)", path.display());
}

/// CPU model and kernel, for the report.
fn system_line() -> String {
    let cpu = std::fs::read_to_string("/proc/cpuinfo")
        .ok()
        .and_then(|s| s.lines().find(|l| l.starts_with("model name")).map(|l| l.split(':').nth(1).unwrap_or("").trim().to_owned()))
        .unwrap_or_else(|| "unknown CPU".to_owned());
    format!(
        "System: {cpu}, {} threads, {} {}",
        std::thread::available_parallelism().map_or(1, |n| n.get()),
        std::env::consts::OS,
        std::env::consts::ARCH
    )
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("bench") => bench(&args[1..]),
        #[cfg(feature = "window")]
        Some("window") => window::run(args.get(1).map(String::as_str).expect("window BACKEND")),
        _ => {
            eprintln!(
                "usage: gpu-render bench [BACKEND…] [--frames N] [--out DIR]\n       \
                 gpu-render window BACKEND   (built with --features gpu or hybrid)\n\
                 backends here: {:?}",
                available()
            );
            std::process::exit(2);
        }
    }
}
