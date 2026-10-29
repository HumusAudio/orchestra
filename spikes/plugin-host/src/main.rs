// SPDX-License-Identifier: GPL-3.0-or-later

//! Spike B: out-of-process plugin hosting (ADR-0011).
//!
//! - `plugin-host scan` — scans installed CLAP, VST3 and LV2 plugins with truce-rack, timing each
//!   scanner.
//! - `plugin-host host <clap|vst3|lv2> <name substring>` — the HOST: starts a BRIDGE child process
//!   (this same executable, `bridge` subcommand) that loads the plugin; audio blocks travel through
//!   a shared-memory file; each block is triggered by one byte on the child's stdin and completed
//!   by one byte on its stdout. It measures the round trip per block against the plugin's own
//!   processing time (the difference is the cost of being out of process), then KILLS the bridge in
//!   the middle of processing, checks that the host survives (silence instead of a crash),
//!   restarts the bridge and continues.
//!
//! Spike shortcuts: pipes for signalling (a real engine would use futex/eventfd/semaphores with a
//! bounded wait, so a HUNG plugin can be detected too, not only a dead one), errors panic,
//! Linux-first paths for the shared-memory file.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::time::{Duration, Instant};

use truce_rack::core::buffer::BusRange;
use truce_rack::core::{
    AudioBuffer, BusLayout, EventList, Plugin, PluginInfo, PluginScanner, ProcessContext,
};

/// Frames per block (the reference buffer size of ADR-0007).
const FRAMES: usize = 256;
/// Sample rate.
const RATE: f64 = 48_000.0;
/// Shared memory layout, in f32 slots: in L, in R, out L, out R; then a u64 with the plugin's
/// processing time in ns (written by the bridge).
const SHM_BYTES: usize = 4 * FRAMES * 4 + 8;

fn scan(format: &str) -> Vec<PluginInfo> {
    let result = match format {
        "clap" => truce_rack::clap::ClapScanner::new().scan(),
        "vst3" => truce_rack::vst3::Vst3Scanner::new().scan(),
        "lv2" => truce_rack::lv2::Lv2Scanner::new().scan(),
        other => panic!("unknown format {other}"),
    };
    result.unwrap_or_else(|e| panic!("{format} scan failed: {e}"))
}

fn cmd_scan() {
    for format in ["clap", "vst3", "lv2"] {
        let start = Instant::now();
        let plugins = scan(format);
        println!(
            "SCAN {format}: {} plugins in {:.0} ms; e.g. {}",
            plugins.len(),
            start.elapsed().as_secs_f64() * 1e3,
            plugins
                .iter()
                .take(4)
                .map(|p| p.name.as_str())
                .collect::<Vec<_>>()
                .join(" | ")
        );
    }
}

fn find(format: &str, needle: &str) -> PluginInfo {
    scan(format)
        .into_iter()
        .find(|p| p.name.contains(needle))
        .unwrap_or_else(|| panic!("no {format} plugin matching {needle:?}"))
}

/// Shared memory as f32/u64 views over a mapped file.
struct Shm(memmap2::MmapMut);

impl Shm {
    fn open(path: &Path, create: bool) -> Self {
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(create)
            .truncate(create)
            .open(path)
            .unwrap();
        if create {
            file.set_len(SHM_BYTES as u64).unwrap();
        }
        // SAFETY (spike): the file is private to host + bridge, which never access the same block
        // concurrently (strict request/response on the pipes).
        Shm(unsafe { memmap2::MmapMut::map_mut(&file).unwrap() })
    }
    fn channels(&mut self) -> &mut [f32] {
        let bytes = &mut self.0[..4 * FRAMES * 4];
        bytemuck_cast(bytes)
    }
    fn set_nanos(&mut self, ns: u64) {
        self.0[4 * FRAMES * 4..].copy_from_slice(&ns.to_ne_bytes());
    }
    fn nanos(&self) -> u64 {
        u64::from_ne_bytes(self.0[4 * FRAMES * 4..].try_into().unwrap())
    }
}

/// Reinterprets an aligned byte slice as f32 (mmap pages are page-aligned).
fn bytemuck_cast(bytes: &mut [u8]) -> &mut [f32] {
    assert_eq!(bytes.as_ptr() as usize % 4, 0);
    // SAFETY (spike): alignment checked above, every bit pattern is a valid f32, length divisible.
    unsafe { std::slice::from_raw_parts_mut(bytes.as_mut_ptr().cast::<f32>(), bytes.len() / 4) }
}

/// Bridge process: loads the plugin and serves blocks until stdin closes.
fn cmd_bridge(format: &str, needle: &str, shm_path: &Path) {
    // Plugins print to stdout (clap-wrapper logs, debug output…), which would corrupt our protocol:
    // keep a private copy of the stdout pipe for the protocol and point fd 1 at stderr.
    // SAFETY (spike): plain fd duplication at start-up, before any other thread exists.
    unsafe {
        let proto = libc::dup(1);
        libc::dup2(2, 1);
        PROTO_FD.store(proto, std::sync::atomic::Ordering::SeqCst);
    }
    eprintln!("[bridge] stage: scan");
    let info = find(format, needle);
    eprintln!("[bridge] stage: load {:?} ({})", info.name, info.path.display());
    match format {
        "clap" => serve(
            truce_rack::clap::ClapScanner::new().load(&info).unwrap(),
            shm_path,
        ),
        "vst3" => serve(
            truce_rack::vst3::Vst3Scanner::new().load(&info).unwrap(),
            shm_path,
        ),
        "lv2" => serve(
            truce_rack::lv2::Lv2Scanner::new().load(&info).unwrap(),
            shm_path,
        ),
        _ => unreachable!(),
    }
}

/// The protocol's copy of the original stdout (see `cmd_bridge`).
static PROTO_FD: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(1);

fn serve<P: Plugin<f32>>(mut plugin: P, shm_path: &Path) {
    eprintln!("[bridge] stage: activate");
    plugin.activate(BusLayout::stereo(), RATE, FRAMES).unwrap();
    eprintln!("[bridge] stage: serving");
    let mut shm = Shm::open(shm_path, false);
    use std::os::fd::FromRawFd;
    let mut input = std::io::stdin().lock();
    // SAFETY (spike): PROTO_FD is a valid fd we own (dup'ed in cmd_bridge).
    let mut output = unsafe { std::fs::File::from_raw_fd(PROTO_FD.load(std::sync::atomic::Ordering::SeqCst)) };
    let mut events = EventList::new();
    let mut out_events = EventList::new();
    let mut byte = [0u8; 1];
    let (mut in_l, mut in_r) = (vec![0.0f32; FRAMES], vec![0.0f32; FRAMES]);
    // Ready signal.
    output.write_all(b"R").unwrap();
    output.flush().unwrap();
    while input.read_exact(&mut byte).is_ok() {
        let ch = shm.channels();
        in_l.copy_from_slice(&ch[0..FRAMES]);
        in_r.copy_from_slice(&ch[FRAMES..2 * FRAMES]);
        let (_, outs) = ch.split_at_mut(2 * FRAMES);
        let (out_l, out_r) = outs.split_at_mut(FRAMES);
        let start = Instant::now();
        {
            let inputs: [&[f32]; 2] = [&in_l, &in_r];
            let mut outputs: [&mut [f32]; 2] = [out_l, out_r];
            let ranges = [BusRange::new(0, 2)];
            let mut buffer = AudioBuffer::new(&inputs, &mut outputs, FRAMES, &ranges, &ranges);
            let mut context = ProcessContext {
                sample_rate: RATE,
                max_block_size: FRAMES,
                transport: None,
                output_events: &mut out_events,
            };
            plugin.process(&mut buffer, &events, &mut context).unwrap();
        }
        let ns = start.elapsed().as_nanos() as u64;
        shm.set_nanos(ns);
        events.clear();
        output.write_all(b"D").unwrap();
        output.flush().unwrap();
    }
}

/// One running bridge, as seen by the host.
struct Bridge {
    child: Child,
    to: ChildStdin,
    from: ChildStdout,
}

fn spawn(format: &str, needle: &str, shm_path: &Path) -> (Bridge, Duration) {
    let start = Instant::now();
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["bridge", format, needle])
        .arg(shm_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let to = child.stdin.take().unwrap();
    let mut from = child.stdout.take().unwrap();
    let mut ready = [0u8; 1];
    from.read_exact(&mut ready).expect("bridge failed to start");
    (Bridge { child, to, from }, start.elapsed())
}

impl Bridge {
    /// Processes one block; `None` when the bridge is dead.
    fn process(&mut self) -> Option<()> {
        self.to.write_all(b"P").ok()?;
        self.to.flush().ok()?;
        let mut done = [0u8; 1];
        self.from.read_exact(&mut done).ok()
    }
}

fn cmd_host(format: &str, needle: &str) {
    let shm_path = PathBuf::from(format!("/dev/shm/orchestra-spike-{}", std::process::id()));
    let mut shm = Shm::open(&shm_path, true);
    let (mut bridge, startup) = spawn(format, needle, &shm_path);
    println!(
        "HOST {format} {needle:?}: bridge started (scan + load + activate) in {:.0} ms",
        startup.as_secs_f64() * 1e3
    );

    let mut phase = 0.0f32;
    let mut fill_input = |shm: &mut Shm| {
        let ch = shm.channels();
        for i in 0..FRAMES {
            let s = 0.25 * (phase * std::f32::consts::TAU).sin();
            phase = (phase + 440.0 / RATE as f32) % 1.0;
            ch[i] = s;
            ch[FRAMES + i] = s;
        }
    };

    // 1) Round trips (2000 blocks ≈ 10.7 s of audio at 48 kHz/256).
    let (mut round, mut plugin_ns, mut energy) = (Vec::new(), Vec::new(), 0.0f64);
    for _ in 0..2000 {
        fill_input(&mut shm);
        let start = Instant::now();
        bridge.process().expect("bridge died unexpectedly");
        round.push(start.elapsed());
        plugin_ns.push(shm.nanos());
        energy += shm.channels()[2 * FRAMES..]
            .iter()
            .map(|s| f64::from(s * s))
            .sum::<f64>();
    }
    round.sort();
    plugin_ns.sort_unstable();
    let us = |d: Duration| d.as_secs_f64() * 1e6;
    let p = |q: f64| round[((round.len() - 1) as f64 * q) as usize];
    let pn = |q: f64| plugin_ns[((plugin_ns.len() - 1) as f64 * q) as usize] as f64 / 1e3;
    let budget = FRAMES as f64 / RATE * 1e6;
    println!(
        "RESULT {format}: round trip p50={:.1}µs p99={:.1}µs max={:.1}µs | plugin p50={:.1}µs p99={:.1}µs | IPC overhead p50≈{:.1}µs ({:.2}% of the {:.0}µs block) | output energy {:.1} (non-zero = audio processed)",
        us(p(0.5)),
        us(p(0.99)),
        us(p(1.0)),
        pn(0.5),
        pn(0.99),
        us(p(0.5)) - pn(0.5),
        (us(p(0.5)) - pn(0.5)) / budget * 100.0,
        budget,
        energy
    );

    // 2) Kill the bridge while blocks are flowing; the host must survive and output silence.
    let mut dead_blocks = 0;
    let mut detected_after = None;
    for block in 0..200 {
        if block == 50 {
            bridge.child.kill().unwrap();
        }
        fill_input(&mut shm);
        let start = Instant::now();
        if bridge.process().is_none() {
            if detected_after.is_none() {
                detected_after = Some((block, start.elapsed()));
            }
            dead_blocks += 1;
            shm.channels()[2 * FRAMES..].fill(0.0); // silence instead of garbage
        }
    }
    let _ = bridge.child.wait();
    let (block, latency) = detected_after.expect("kill was not detected");
    println!(
        "KILL {format}: bridge killed at block 50, detected at block {block} in {:.1}µs; host survived, {dead_blocks} blocks rendered as silence",
        us(latency)
    );

    // 3) Restart and check processing resumes.
    let (mut bridge, restart) = spawn(format, needle, &shm_path);
    fill_input(&mut shm);
    bridge.process().expect("restarted bridge works");
    println!(
        "RESTART {format}: new bridge in {:.0} ms, processing resumed",
        restart.as_secs_f64() * 1e3
    );
    drop(bridge.to);
    let _ = bridge.child.wait();
    let _ = std::fs::remove_file(&shm_path);
}

mod gui {
    //! Opens the plugin editor inside a top-level X11 window owned by the bridge process: a window
    //! SEPARATE from the host's (ADR-0011). Under Wayland this runs through XWayland, because
    //! truce-rack (like most hosts) only supports X11 parents on Linux.

    use super::*;
    use truce_rack::core::{PluginCore, WindowHandle};
    use winit::application::ApplicationHandler;
    use winit::event::WindowEvent;
    use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
    use winit::platform::x11::EventLoopBuilderExtX11;
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use winit::window::{Window, WindowId};

    struct App<P: PluginCore> {
        plugin: P,
        window: Option<Window>,
        opened: Option<Instant>,
        seconds: u64,
    }

    impl<P: PluginCore> ApplicationHandler for App<P> {
        fn resumed(&mut self, el: &ActiveEventLoop) {
            let attrs = Window::default_attributes()
                .with_title(format!("{} — Orchestra plugin window (spike)", self.plugin.info().name))
                .with_inner_size(winit::dpi::PhysicalSize::new(800, 600));
            let window = el.create_window(attrs).unwrap();
            let RawWindowHandle::Xlib(h) = window.window_handle().unwrap().as_raw() else {
                panic!("not an X11 window: run under X11 or XWayland")
            };
            let start = Instant::now();
            let editor = self.plugin.editor().expect("plugin has no editor");
            editor.open(WindowHandle::X11(h.window as u64), 1.0).unwrap();
            editor.show();
            if let Some((w, hh)) = editor.size() {
                let _ = window.request_inner_size(winit::dpi::PhysicalSize::new(w, hh));
            }
            println!("GUI: editor opened in {:.0} ms, size {:?}", start.elapsed().as_secs_f64() * 1e3, editor.size());
            self.opened = Some(Instant::now());
            self.window = Some(window);
            el.set_control_flow(ControlFlow::Poll);
        }
        fn window_event(&mut self, el: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
            if let WindowEvent::CloseRequested = event {
                el.exit();
            }
        }
        fn about_to_wait(&mut self, el: &ActiveEventLoop) {
            if let Some(editor) = self.plugin.editor() {
                editor.on_idle();
            }
            std::thread::sleep(Duration::from_millis(16));
            if self.opened.is_some_and(|t| t.elapsed() > Duration::from_secs(self.seconds)) {
                if let Some(editor) = self.plugin.editor() {
                    editor.close();
                }
                println!("GUI: closed after {} s, host process still alive", self.seconds);
                el.exit();
            }
        }
    }

    pub fn run<P: PluginCore>(plugin: P, seconds: u64) {
        let event_loop = EventLoop::builder().with_x11().build().unwrap();
        let mut app = App { plugin, window: None, opened: None, seconds };
        event_loop.run_app(&mut app).unwrap();
    }
}

fn cmd_gui(format: &str, needle: &str, seconds: u64) {
    let info = find(format, needle);
    println!("GUI: loading {:?} ({format}), has_editor={}", info.name, info.has_editor);
    match format {
        "clap" => gui::run(truce_rack::clap::ClapScanner::new().load(&info).unwrap(), seconds),
        "vst3" => gui::run(truce_rack::vst3::Vst3Scanner::new().load(&info).unwrap(), seconds),
        "lv2" => gui::run(truce_rack::lv2::Lv2Scanner::new().load(&info).unwrap(), seconds),
        _ => unreachable!(),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("gui") => cmd_gui(&args[2], &args[3], args.get(4).map_or(15, |s| s.parse().unwrap())),
        Some("scan") => cmd_scan(),
        Some("host") => cmd_host(&args[2], &args[3]),
        Some("bridge") => cmd_bridge(&args[2], &args[3], Path::new(&args[4])),
        _ => eprintln!("usage: plugin-host scan | host <clap|vst3|lv2> <name substring>"),
    }
}
