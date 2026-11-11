// SPDX-License-Identifier: GPL-3.0-or-later

//! `gpu-render window BACKEND`: a window that scrolls and zooms the reference scene continuously,
//! presented with vsync, printing once a second the frames per second and the time our frame
//! takes (draw + present). It exists to judge smoothness by eye on the owner's screen; the
//! numbers that go into the ADR come from `bench`.
//!
//! - CPU backends (`tiny-skia`, `vello-cpu`, `vello-cpu-mt`) present with `softbuffer`, as the
//!   product does today (ADR-0040).
//! - `vello` / `hybrid` render into their 1920 × 1080 texture and blit it (stretched to the
//!   window) onto the `wgpu` surface with `TextureBlitter`.
//!
//! The first line printed is the time from process start to the first presented frame: the
//! start-up cost a user would see (opening the GPU, compiling shaders, the first frame).

use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::{Duration, Instant};

use winit::application::ApplicationHandler;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

use crate::Backend;
use crate::scene::{HEIGHT, Scene, View, WIDTH};

/// How the frame reaches the screen.
enum Presenter {
    /// A CPU backend and the softbuffer surface it is copied into.
    Cpu {
        backend: Box<dyn Backend>,
        surface: softbuffer::Surface<Arc<Window>, Arc<Window>>,
    },
    #[cfg(feature = "gpu")]
    Vello {
        backend: crate::gpu::Vello,
        surface: vello::wgpu::Surface<'static>,
        config: vello::wgpu::SurfaceConfiguration,
        blitter: vello::wgpu::util::TextureBlitter,
    },
    #[cfg(feature = "hybrid")]
    Hybrid {
        backend: crate::hybrid::Hybrid,
        surface: wgpu29::Surface<'static>,
        config: wgpu29::SurfaceConfiguration,
        blitter: wgpu29::util::TextureBlitter,
    },
}

/// The application state.
struct App {
    name: String,
    window: Option<Arc<Window>>,
    presenter: Option<Presenter>,
    started: Instant,
    frame: usize,
    first_presented: bool,
    /// Frames and summed work time since the last report.
    window_frames: u32,
    work: Duration,
    worst: Duration,
    last_report: Instant,
}

/// Opens the window and runs until it is closed or Esc is pressed.
pub fn run(name: &str) {
    let started = Instant::now();
    let event_loop = EventLoop::new().expect("event loop");
    let mut app = App {
        name: name.to_owned(),
        window: None,
        presenter: None,
        started,
        frame: 0,
        first_presented: false,
        window_frames: 0,
        work: Duration::ZERO,
        worst: Duration::ZERO,
        last_report: Instant::now(),
    };
    event_loop.run_app(&mut app).expect("run");
}

/// Picks a non-sRGB surface format when there is one: the renderers write sRGB-encoded values
/// already, and an sRGB surface would encode them a second time (washed-out colours).
macro_rules! surface_config {
    ($wgpu:ident, $surface:expr, $gpu:expr, $size:expr) => {{
        let caps = $surface.get_capabilities(&$gpu.adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| !f.is_srgb())
            .unwrap_or(caps.formats[0]);
        let mut config = $surface
            .get_default_config(&$gpu.adapter, $size.width.max(1), $size.height.max(1))
            .expect("surface supported by the adapter");
        config.format = format;
        config.present_mode = $wgpu::PresentMode::AutoVsync;
        config
    }};
}

/// Blits `source` onto the surface and presents it.
macro_rules! blit_and_present {
    ($wgpu:ident, $device:expr, $queue:expr, $surface:expr, $config:expr, $blitter:expr, $source:expr, $present:expr) => {{
        let texture = match $surface.get_current_texture() {
            $wgpu::CurrentSurfaceTexture::Success(t)
            | $wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            _ => {
                $surface.configure($device, $config);
                return Duration::ZERO;
            }
        };
        let view = texture
            .texture
            .create_view(&$wgpu::TextureViewDescriptor::default());
        let mut encoder =
            $device.create_command_encoder(&$wgpu::CommandEncoderDescriptor::default());
        $blitter.copy($device, &mut encoder, $source, &view);
        $queue.submit([encoder.finish()]);
        $present(texture);
    }};
}

impl App {
    fn create_presenter(&self, window: &Arc<Window>) -> Presenter {
        let size = window.inner_size();
        match self.name.as_str() {
            #[cfg(feature = "gpu")]
            "vello" => {
                use vello::wgpu;
                let instance = wgpu::Instance::new(
                    wgpu::InstanceDescriptor::new_without_display_handle_from_env(),
                );
                let surface = instance.create_surface(window.clone()).expect("surface");
                let (backend, _, _) =
                    crate::gpu::Vello::build(Scene::new(), instance, Some(&surface));
                let config = surface_config!(wgpu, surface, backend.gpu, size);
                surface.configure(&backend.gpu.device, &config);
                let blitter = wgpu::util::TextureBlitter::new(&backend.gpu.device, config.format);
                eprintln!("adapter: {}", backend.gpu.describe());
                Presenter::Vello {
                    backend,
                    surface,
                    config,
                    blitter,
                }
            }
            #[cfg(feature = "hybrid")]
            "hybrid" => {
                use wgpu29 as wgpu;
                let instance = wgpu::Instance::new(
                    wgpu::InstanceDescriptor::new_without_display_handle_from_env(),
                );
                let surface = instance.create_surface(window.clone()).expect("surface");
                let (backend, _, _) =
                    crate::hybrid::Hybrid::build(Scene::new(), instance, Some(&surface));
                let config = surface_config!(wgpu, surface, backend.gpu, size);
                surface.configure(&backend.gpu.device, &config);
                let blitter = wgpu::util::TextureBlitter::new(&backend.gpu.device, config.format);
                eprintln!("adapter: {}", backend.gpu.describe());
                Presenter::Hybrid {
                    backend,
                    surface,
                    config,
                    blitter,
                }
            }
            cpu => {
                let (backend, _, _) = crate::create(cpu);
                let context = softbuffer::Context::new(window.clone()).expect("softbuffer context");
                let surface = softbuffer::Surface::new(&context, window.clone()).expect("surface");
                Presenter::Cpu { backend, surface }
            }
        }
    }

    /// Draws and presents frame `self.frame`; returns how long our part took.
    fn draw(&mut self) -> Duration {
        let start = Instant::now();
        let view = View::animated(self.frame);
        let Some(window) = self.window.clone() else {
            return Duration::ZERO;
        };
        let size = window.inner_size();
        match self.presenter.as_mut().expect("presenter") {
            Presenter::Cpu { backend, surface } => {
                backend.draw_full(view);
                let (Some(w), Some(h)) =
                    (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
                else {
                    return start.elapsed();
                };
                surface.resize(w, h).expect("resize");
                let mut buffer = surface.buffer_mut().expect("buffer");
                let pixels = backend.cpu_pixels().expect("cpu backend");
                // Copy the top-left part that fits, converting RGBA to softbuffer's 0RGB.
                let (cw, ch) = (
                    size.width.min(WIDTH) as usize,
                    size.height.min(HEIGHT) as usize,
                );
                for y in 0..ch {
                    let src = &pixels[y * WIDTH as usize * 4..][..cw * 4];
                    let dst = &mut buffer[y * size.width as usize..][..cw];
                    for (d, s) in dst.iter_mut().zip(src.chunks_exact(4)) {
                        *d = (u32::from(s[0]) << 16) | (u32::from(s[1]) << 8) | u32::from(s[2]);
                    }
                }
                buffer.present().expect("present");
            }
            #[cfg(feature = "gpu")]
            Presenter::Vello {
                backend,
                surface,
                config,
                blitter,
            } => {
                use vello::wgpu;
                backend.render(view, false);
                let device = &backend.gpu.device;
                blit_and_present!(
                    wgpu,
                    device,
                    &backend.gpu.queue,
                    surface,
                    config,
                    blitter,
                    &backend.view,
                    |t| backend.gpu.queue.present(t)
                );
            }
            #[cfg(feature = "hybrid")]
            Presenter::Hybrid {
                backend,
                surface,
                config,
                blitter,
            } => {
                use wgpu29 as wgpu;
                backend.render(view, false);
                let device = &backend.gpu.device;
                blit_and_present!(
                    wgpu,
                    device,
                    &backend.gpu.queue,
                    surface,
                    config,
                    blitter,
                    &backend.view,
                    |t: wgpu29::SurfaceTexture| t.present()
                );
            }
        }
        start.elapsed()
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attributes = Window::default_attributes()
            .with_title(format!("gpu-render — {}", self.name))
            .with_inner_size(winit::dpi::PhysicalSize::new(WIDTH, HEIGHT));
        let window = Arc::new(event_loop.create_window(attributes).expect("window"));
        self.presenter = Some(self.create_presenter(&window));
        self.window = Some(window.clone());
        window.request_redraw();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested
            | WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        logical_key: Key::Named(NamedKey::Escape),
                        state: ElementState::Pressed,
                        ..
                    },
                ..
            } => event_loop.exit(),
            WindowEvent::Resized(size) => match self.presenter.as_mut() {
                #[cfg(feature = "gpu")]
                Some(Presenter::Vello {
                    backend,
                    surface,
                    config,
                    ..
                }) => {
                    config.width = size.width.max(1);
                    config.height = size.height.max(1);
                    surface.configure(&backend.gpu.device, config);
                }
                #[cfg(feature = "hybrid")]
                Some(Presenter::Hybrid {
                    backend,
                    surface,
                    config,
                    ..
                }) => {
                    config.width = size.width.max(1);
                    config.height = size.height.max(1);
                    surface.configure(&backend.gpu.device, config);
                }
                _ => {}
            },
            WindowEvent::RedrawRequested => {
                let work = self.draw();
                if !self.first_presented {
                    self.first_presented = true;
                    println!(
                        "{}: first frame presented {:.0} ms after start",
                        self.name,
                        self.started.elapsed().as_secs_f64() * 1000.0
                    );
                } else {
                    self.window_frames += 1;
                    self.work += work;
                    self.worst = self.worst.max(work);
                }
                self.frame += 1;
                if self.last_report.elapsed() >= Duration::from_secs(1) && self.window_frames > 0 {
                    let secs = self.last_report.elapsed().as_secs_f64();
                    println!(
                        "{}: {:.0} fps, our work {:.1} ms average, {:.1} ms worst{}",
                        self.name,
                        f64::from(self.window_frames) / secs,
                        self.work.as_secs_f64() * 1000.0 / f64::from(self.window_frames),
                        self.worst.as_secs_f64() * 1000.0,
                        memory_suffix(),
                    );
                    self.window_frames = 0;
                    self.work = Duration::ZERO;
                    self.worst = Duration::ZERO;
                    self.last_report = Instant::now();
                }
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            _ => {}
        }
    }
}

/// ", RSS N MiB" on Linux.
fn memory_suffix() -> String {
    crate::memory_mib().map_or(String::new(), |(rss, _)| format!(", RSS {rss:.0} MiB"))
}
