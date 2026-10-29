// SPDX-License-Identifier: GPL-3.0-or-later

//! Spike A, candidate "iced" 0.14: the score is a `Canvas` program; text widgets show the six
//! languages; a row of buttons stands in for the menu bar (iced has no menu-bar widget in its
//! core; `iced_aw` provides one). Headless rendering through `iced_test::Simulator`, whose backend
//! is chosen with `ICED_TEST_BACKEND=tiny-skia` (CPU) or `wgpu` (GPU).
//!
//! Modes: `gui-iced headless <out.png>`, `gui-iced bench`, `gui-iced window`.
//!
//! Notes for the evaluation:
//! - `iced_test` always renders at scale factor 2, so the UI is laid out at 960 × 540 logical px
//!   to obtain 1920 × 1080 physical px, the size every candidate is measured at.
//! - Canvas text has no "baseline" vertical alignment, but SMuFL glyphs are positioned by their
//!   baseline: we compensate with the font ascent read through ttf-parser (and an exact line
//!   height), a workaround a real implementation would also need.

use std::borrow::Cow;
use std::cell::Cell;
use std::rc::Rc;
use std::time::Instant;

use iced::alignment;
use iced::widget::canvas::{self, Canvas, Frame, Geometry, Path, Stroke, Text};
use iced::widget::{button, column, container, row, text};
use iced::{Color, Element, Font, Length, Pixels, Point, Rectangle, Renderer, Theme, mouse};
use scene::{Scene, View};

/// Logical size (the snapshot is rendered at 2×).
const LOGICAL: (f32, f32) = (scene::WIDTH as f32 / 2.0, scene::HEIGHT as f32 / 2.0);

const INK: Color = Color::from_rgb(24.0 / 255.0, 26.0 / 255.0, 40.0 / 255.0);

/// Bravura's ascent and descent in em, for the baseline workaround.
fn music_metrics() -> (f32, f32) {
    let face = ttf_parser::Face::parse(scene::MUSIC_FONT, 0).unwrap();
    let em = f32::from(face.units_per_em());
    (f32::from(face.ascender()) / em, -f32::from(face.descender()) / em)
}

/// Collects a ttf-parser outline into an iced canvas path (font units, y up).
struct IcedOutline(canvas::path::Builder);

impl ttf_parser::OutlineBuilder for IcedOutline {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.move_to(Point::new(x, y));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.0.line_to(Point::new(x, y));
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.0.quadratic_curve_to(Point::new(x1, y1), Point::new(x, y));
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.0.bezier_curve_to(Point::new(x1, y1), Point::new(x2, y2), Point::new(x, y));
    }
    fn close(&mut self) {
        self.0.close();
    }
}

/// Outlines of every SMuFL code point used by the scene, in font units, plus units per em.
fn glyph_paths(scene: &Scene) -> (std::collections::HashMap<char, Path>, f32) {
    let face = ttf_parser::Face::parse(scene::MUSIC_FONT, 0).unwrap();
    let mut map = std::collections::HashMap::new();
    for g in &scene.glyphs {
        map.entry(g.ch).or_insert_with(|| {
            let mut b = IcedOutline(canvas::path::Builder::new());
            face.outline_glyph(face.glyph_index(g.ch).unwrap(), &mut b);
            b.0.build()
        });
    }
    (map, f32::from(face.units_per_em()))
}

/// The score canvas. The view lives in a shared cell so the benchmark can move it between
/// snapshots without rebuilding the interface.
struct Score {
    scene: Scene,
    view: Rc<Cell<View>>,
    metrics: (f32, f32),
    /// `Some` when glyphs are drawn as cached outlines (ICED_GLYPHS=paths) instead of text.
    paths: Option<(std::collections::HashMap<char, Path>, f32)>,
}

impl<Message> canvas::Program<Message> for Score {
    type State = ();

    fn draw(&self, _: &(), renderer: &Renderer, _: &Theme, bounds: Rectangle, _: mouse::Cursor) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        // Draw in "physical 1920 × 1080 space", scaled to the logical canvas.
        let k = bounds.width / scene::WIDTH as f32;
        frame.scale(k);
        let view = self.view.get();
        let pps = view.px_per_sp();
        // The canvas starts below the header (a separate widget), so subtract SCORE_TOP.
        let top = scene::SCORE_TOP;
        for line in &self.scene.lines {
            let (x0, y0) = view.to_px(line.x0, line.y0);
            let (x1, y1) = view.to_px(line.x1, line.y1);
            if x0.max(x1) < 0.0 || x0.min(x1) > scene::WIDTH as f32 || y0.max(y1) < top || y0.min(y1) > scene::HEIGHT as f32 {
                continue;
            }
            frame.stroke(
                &Path::line(Point::new(x0, y0 - top), Point::new(x1, y1 - top)),
                Stroke::default().with_width((line.width * pps).max(1.0)).with_color(INK),
            );
        }
        let size = 4.0 * pps; // SMuFL: 1 em = 4 sp
        let (ascent, descent) = self.metrics;
        for glyph in &self.scene.glyphs {
            let (x, y) = view.to_px(glyph.x, glyph.y);
            if x < -size || x > scene::WIDTH as f32 + size || y < top - size || y > scene::HEIGHT as f32 + size {
                continue;
            }
            if let Some((paths, upem)) = &self.paths {
                let s = size / upem;
                frame.with_save(|f| {
                    f.translate(iced::Vector::new(x, y - top));
                    f.scale_nonuniform(iced::Vector::new(s, -s));
                    f.fill(&paths[&glyph.ch], INK);
                });
                continue;
            }
            frame.fill_text(Text {
                content: glyph.ch.to_string(),
                position: Point::new(x, y - top - ascent * size),
                color: INK,
                size: Pixels(size),
                line_height: iced::widget::text::LineHeight::Absolute(Pixels((ascent + descent) * size)),
                font: Font::with_name("Bravura"),
                align_y: alignment::Vertical::Top,
                ..Text::default()
            });
        }
        vec![frame.into_geometry()]
    }
}

#[derive(Debug, Clone)]
enum Message {}

/// The whole interface: menu row, the six texts in two columns, the score canvas.
fn view_ui(score: &Score) -> Element<'_, Message> {
    // Header sizes are given in physical px in `scene`; halve them for logical px.
    let menu = row(scene::MENU_ITEMS.iter().map(|m| button(text(*m).size(7)).padding([2, 6]).style(button::text).into()))
        .spacing(4)
        .height(Length::Fixed(scene::MENU_HEIGHT / 2.0));
    let col = |range: std::ops::Range<usize>| {
        column(scene::TEXTS[range].iter().map(|(_, t)| text(*t).size(scene::TEXT_SIZE / 2.0).into())).spacing(2)
    };
    let texts = row![col(0..3).width(Length::Fixed(480.0)), col(3..6)].padding([2, 8]);
    let header = container(column![menu, texts]).height(Length::Fixed(scene::SCORE_TOP / 2.0));
    column![header, Canvas::new(score).width(Length::Fill).height(Length::Fill)].into()
}

fn settings() -> iced::Settings {
    iced::Settings {
        fonts: vec![Cow::Borrowed(scene::MUSIC_FONT), Cow::Borrowed(scene::TEXT_FONT)],
        default_font: Font::with_name("Inter"),
        ..iced::Settings::default()
    }
}

fn new_score(view: Rc<Cell<View>>) -> Score {
    let scene = scene::build();
    let paths = (std::env::var("ICED_GLYPHS").as_deref() == Ok("paths")).then(|| glyph_paths(&scene));
    Score { scene, view, metrics: music_metrics(), paths }
}

fn headless(out: &str) {
    let score = new_score(Rc::new(Cell::new(View::OVERVIEW)));
    let start = Instant::now();
    let mut ui = iced_test::Simulator::with_size(settings(), LOGICAL, view_ui(&score));
    println!("simulator + renderer setup: {:.2} ms", start.elapsed().as_secs_f64() * 1e3);
    let start = Instant::now();
    let snapshot = ui.snapshot(&Theme::Light).unwrap();
    println!("first frame: {:.2} ms", start.elapsed().as_secs_f64() * 1e3);
    let _ = std::fs::remove_file(out);
    snapshot.matches_image(out).unwrap(); // writes the PNG when the file does not exist
    println!("wrote {out}");
}

fn bench() {
    let view = Rc::new(Cell::new(View::OVERVIEW));
    let score = new_score(view.clone());
    let mut ui = iced_test::Simulator::with_size(settings(), LOGICAL, view_ui(&score));
    let _ = ui.snapshot(&Theme::Light).unwrap(); // warm caches
    let backend = std::env::var("ICED_TEST_BACKEND").unwrap_or_else(|_| "default".into());
    let times = scene::bench_views()
        .into_iter()
        .map(|v| {
            view.set(v);
            let start = Instant::now();
            let _ = ui.snapshot(&Theme::Light).unwrap();
            start.elapsed()
        })
        .collect();
    let glyphs = std::env::var("ICED_GLYPHS").unwrap_or_else(|_| "text".into());
    scene::report(&format!("iced ({backend}, glyphs as {glyphs}, 1920x1080 incl. readback)"), times);
}

/// State of the interactive application.
struct App {
    score: Score,
}

fn app_new() -> App {
    App { score: new_score(Rc::new(Cell::new(View::OVERVIEW))) }
}

fn app_update(_: &mut App, _: Message) {}

fn app_view(app: &App) -> Element<'_, Message> {
    view_ui(&app.score)
}

fn window() -> iced::Result {
    iced::application(app_new, app_update, app_view)
        .settings(settings())
        .window_size(iced::Size::new(LOGICAL.0, LOGICAL.1))
        .theme(|_: &App| Theme::Light)
        .run()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("headless") => headless(args.get(2).map_or("iced.png", String::as_str)),
        Some("bench") => bench(),
        Some("window") => window().unwrap(),
        _ => eprintln!("usage: gui-iced headless <out.png> | bench | window"),
    }
}
