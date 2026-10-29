// SPDX-License-Identifier: GPL-3.0-or-later

//! Spike A, candidate "Slint" 1.18: the interface (menu bar, texts) is declared in the Slint
//! language; the score, which needs immediate-mode drawing Slint does not offer, is rendered by
//! our tiny-skia renderer (shared with the "own" spike) and shown as an `Image`.
//!
//! Modes: `gui-slint headless <out.png>` and `gui-slint bench` use a custom headless platform
//! (`MinimalSoftwareWindow`, Slint's software renderer, no display); `gui-slint window` uses the
//! default winit backend.

#[path = "../../gui-own/src/render.rs"]
mod render;

use std::rc::Rc;
use std::time::{Duration, Instant};

use render::Renderer;
use scene::View;
use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::platform::{Platform, WindowAdapter};
use slint::{Image, PhysicalSize, Rgba8Pixel, SharedPixelBuffer};

slint::slint! {
    import "../../../assets/fonts/inter/Inter-Regular.otf";

    export component Ui inherits Window {
        width: 1920px;
        height: 1080px;
        default-font-family: "Inter";
        background: white;
        in property <image> score;
        in property <[string]> texts;

        MenuBar {
            Menu { title: "File"; MenuItem { title: "…"; } }
            Menu { title: "Edit"; MenuItem { title: "…"; } }
            Menu { title: "View"; MenuItem { title: "…"; } }
        }

        Image { x: 0; y: 0; width: 1920px; height: 1080px; source: root.score; }

        // Header band over the score (the score image is drawn under it, as in the other spikes).
        Rectangle {
            x: 0; y: 0; width: 1920px; height: 64px; background: white;
            for t[i] in root.texts: Text {
                x: i < 3 ? 16px : 976px;
                y: 6px + mod(i, 3) * 20px;
                text: t;
                font-size: 15px;
                color: #181a28;
            }
        }
    }
}

/// Headless platform: every window is the same software-rendered window.
struct Headless {
    window: Rc<MinimalSoftwareWindow>,
    start: Instant,
}

impl Platform for Headless {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        Ok(self.window.clone())
    }
    fn duration_since_start(&self) -> Duration {
        self.start.elapsed()
    }
}

/// Renders the score with tiny-skia into a Slint image (header left to Slint).
fn score_image(renderer: &mut Renderer, scene: &scene::Scene, view: View) -> Image {
    let mut pixmap = tiny_skia::Pixmap::new(scene::WIDTH, scene::HEIGHT).unwrap();
    renderer.render(&mut pixmap, scene, view);
    let buffer = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(
        pixmap.data(),
        scene::WIDTH,
        scene::HEIGHT,
    );
    Image::from_rgba8_premultiplied(buffer)
}

struct Harness {
    window: Rc<MinimalSoftwareWindow>,
    ui: Ui,
    renderer: Renderer<'static>,
    scene: scene::Scene,
    pixels: Vec<slint::Rgb8Pixel>,
}

impl Harness {
    fn new() -> Self {
        let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
        slint::platform::set_platform(Box::new(Headless {
            window: window.clone(),
            start: Instant::now(),
        }))
        .unwrap();
        window.set_size(PhysicalSize::new(scene::WIDTH, scene::HEIGHT));
        let ui = Ui::new().unwrap();
        let texts: Vec<slint::SharedString> =
            scene::TEXTS.iter().map(|(_, t)| (*t).into()).collect();
        ui.set_texts(Rc::new(slint::VecModel::from(texts)).into());
        ui.show().unwrap();
        Self {
            window,
            ui,
            renderer: {
                let mut r = Renderer::new();
                r.header = false;
                r
            },
            scene: scene::build(),
            pixels: vec![Default::default(); (scene::WIDTH * scene::HEIGHT) as usize],
        }
    }

    /// Returns (score render time, Slint composition time).
    fn frame(&mut self, view: View) -> (Duration, Duration) {
        let t0 = Instant::now();
        let image = score_image(&mut self.renderer, &self.scene, view);
        let t1 = Instant::now();
        self.ui.set_score(image);
        slint::platform::update_timers_and_animations();
        let pixels = &mut self.pixels;
        self.window.draw_if_needed(|r| {
            r.render(pixels, scene::WIDTH as usize);
        });
        (t1 - t0, t1.elapsed())
    }
}

fn headless(out: &str) {
    let mut h = Harness::new();
    let start = Instant::now();
    let (score, compose) = h.frame(View::OVERVIEW);
    println!(
        "first frame: {:.2} ms (score {:.2} ms + slint {:.2} ms)",
        start.elapsed().as_secs_f64() * 1e3,
        score.as_secs_f64() * 1e3,
        compose.as_secs_f64() * 1e3
    );
    let file = std::fs::File::create(out).unwrap();
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), scene::WIDTH, scene::HEIGHT);
    enc.set_color(png::ColorType::Rgb);
    let mut w = enc.write_header().unwrap();
    let bytes: Vec<u8> = h.pixels.iter().flat_map(|p| [p.r, p.g, p.b]).collect();
    w.write_image_data(&bytes).unwrap();
    println!("wrote {out}");
}

fn bench() {
    let mut h = Harness::new();
    h.frame(View::OVERVIEW);
    let mut slint_only = Vec::new();
    let times = scene::bench_views()
        .into_iter()
        .map(|v| {
            let start = Instant::now();
            let (_, compose) = h.frame(v);
            slint_only.push(compose);
            start.elapsed()
        })
        .collect();
    scene::report(
        "slint software renderer + tiny-skia score (CPU, 1920x1080)",
        times,
    );
    scene::report("  of which Slint composition only", slint_only);
}

fn window() {
    let ui = Ui::new().unwrap();
    let texts: Vec<slint::SharedString> = scene::TEXTS.iter().map(|(_, t)| (*t).into()).collect();
    ui.set_texts(Rc::new(slint::VecModel::from(texts)).into());
    let mut renderer = Renderer::new();
    renderer.header = false;
    ui.set_score(score_image(&mut renderer, &scene::build(), View::OVERVIEW));
    ui.run().unwrap();
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("headless") => headless(args.get(2).map_or("slint.png", String::as_str)),
        Some("bench") => bench(),
        Some("window") => window(),
        _ => eprintln!("usage: gui-slint headless <out.png> | bench | window"),
    }
}
