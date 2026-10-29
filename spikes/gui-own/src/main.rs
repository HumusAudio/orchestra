// SPDX-License-Identifier: GPL-3.0-or-later

//! Spike A, candidate "own renderer": everything is drawn by us with `tiny-skia` (CPU), glyph
//! outlines come from the bundled fonts through `ttf-parser`, the window is `winit` + `softbuffer`.
//!
//! Modes:
//! - `gui-own headless <out.png>` — renders the overview to a PNG without any window or display;
//! - `gui-own bench` — renders the 120-frame benchmark path off-screen and prints frame times;
//! - `gui-own window` — interactive window (wheel scrolls, Ctrl+wheel zooms), prints frame times.
//!
//! Spike shortcuts (acceptable here, NOT in product code): text uses the cmap + advances only
//! (no shaping or kerning; the real thing would use harfrust/parley), the menu bar is drawn but
//! not interactive, errors panic.

use std::num::NonZeroU32;
use std::rc::Rc;
use std::time::Instant;

mod render;

use render::Renderer;
use scene::{Scene, View};
use tiny_skia::Pixmap;

fn headless(out: &str) {
    let scene = scene::build();
    let mut renderer = Renderer::new();
    let mut pixmap = Pixmap::new(scene::WIDTH, scene::HEIGHT).unwrap();
    let start = Instant::now();
    renderer.render(&mut pixmap, &scene, View::OVERVIEW);
    println!(
        "first frame (cold glyph cache): {:.2} ms",
        start.elapsed().as_secs_f64() * 1e3
    );
    pixmap.save_png(out).unwrap();
    println!("wrote {out}");
}

fn bench() {
    let scene = scene::build();
    let mut renderer = Renderer::new();
    let mut pixmap = Pixmap::new(scene::WIDTH, scene::HEIGHT).unwrap();
    renderer.render(&mut pixmap, &scene, View::OVERVIEW); // warm the glyph cache
    let times = scene::bench_views()
        .into_iter()
        .map(|view| {
            let start = Instant::now();
            renderer.render(&mut pixmap, &scene, view);
            start.elapsed()
        })
        .collect();
    scene::report("own/tiny-skia (CPU, 1920x1080)", times);
}

mod window {
    //! Interactive mode: winit window, softbuffer presents the tiny-skia pixmap (CPU only).

    use super::*;
    use winit::application::ApplicationHandler;
    use winit::event::{MouseScrollDelta, WindowEvent};
    use winit::event_loop::{ActiveEventLoop, EventLoop};
    use winit::keyboard::ModifiersState;
    use winit::window::{Window, WindowId};

    struct App {
        scene: Scene,
        renderer: Renderer<'static>,
        view: View,
        modifiers: ModifiersState,
        window: Option<Rc<Window>>,
        surface: Option<softbuffer::Surface<Rc<Window>, Rc<Window>>>,
        times: Vec<std::time::Duration>,
    }

    impl ApplicationHandler for App {
        fn resumed(&mut self, el: &ActiveEventLoop) {
            let attrs = Window::default_attributes()
                .with_title("Orchestra spike — own renderer")
                .with_inner_size(winit::dpi::PhysicalSize::new(scene::WIDTH, scene::HEIGHT));
            let window = Rc::new(el.create_window(attrs).unwrap());
            let context = softbuffer::Context::new(window.clone()).unwrap();
            self.surface = Some(softbuffer::Surface::new(&context, window.clone()).unwrap());
            self.window = Some(window);
        }

        fn window_event(&mut self, el: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
            match event {
                WindowEvent::CloseRequested => {
                    if !self.times.is_empty() {
                        scene::report(
                            "own/tiny-skia window (CPU, softbuffer)",
                            std::mem::take(&mut self.times),
                        );
                    }
                    el.exit();
                }
                WindowEvent::ModifiersChanged(m) => self.modifiers = m.state(),
                WindowEvent::MouseWheel { delta, .. } => {
                    let dy = match delta {
                        MouseScrollDelta::LineDelta(_, y) => y,
                        MouseScrollDelta::PixelDelta(p) => p.y as f32 / 40.0,
                    };
                    if self.modifiers.control_key() {
                        self.view.zoom = (self.view.zoom * (1.0 + dy * 0.1)).clamp(0.3, 6.0);
                    } else {
                        self.view.scroll_y -= dy * 3.0;
                    }
                    self.window.as_ref().unwrap().request_redraw();
                }
                WindowEvent::RedrawRequested => {
                    let start = Instant::now();
                    let size = self.window.as_ref().unwrap().inner_size();
                    let (Some(w), Some(h)) =
                        (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
                    else {
                        return;
                    };
                    let mut pixmap = Pixmap::new(w.get(), h.get()).unwrap();
                    self.renderer.render(&mut pixmap, &self.scene, self.view);
                    let surface = self.surface.as_mut().unwrap();
                    surface.resize(w, h).unwrap();
                    let mut buffer = surface.buffer_mut().unwrap();
                    for (dst, px) in buffer.iter_mut().zip(pixmap.pixels()) {
                        *dst = (u32::from(px.red()) << 16)
                            | (u32::from(px.green()) << 8)
                            | u32::from(px.blue());
                    }
                    buffer.present().unwrap();
                    self.times.push(start.elapsed());
                }
                _ => {}
            }
        }
    }

    pub fn run() {
        let event_loop = EventLoop::new().unwrap();
        let mut app = App {
            scene: scene::build(),
            renderer: Renderer::new(),
            view: View::OVERVIEW,
            modifiers: ModifiersState::empty(),
            window: None,
            surface: None,
            times: Vec::new(),
        };
        event_loop.run_app(&mut app).unwrap();
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("headless") => headless(args.get(2).map_or("own.png", String::as_str)),
        Some("bench") => bench(),
        Some("window") => window::run(),
        _ => eprintln!("usage: gui-own headless <out.png> | bench | window"),
    }
}
