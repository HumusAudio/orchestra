// SPDX-License-Identifier: GPL-3.0-or-later

//! Spike A, candidate "egui" 0.34 (immediate mode). Headless CPU rendering through the
//! experimental `egui_software_backend` 0.0.3, which only supports egui 0.34 (current: 0.37).
//!
//! Modes: `gui-egui headless <out.png>`, `gui-egui bench`.
//! (Interactive and GPU measurements need eframe/wgpu and run outside the sandbox.)
//!
//! Notes for the evaluation:
//! - epaint only fills CONVEX paths, so music glyph outlines (concave, with holes) cannot be drawn
//!   as vector paths: glyphs must go through egui's text system, i.e. a glyph texture atlas
//!   rasterised per pixel size. Continuous zoom therefore keeps adding atlas entries.
//! - Text has no baseline anchor; as in iced, we offset by the font ascent (ttf-parser).

use std::sync::Arc;
use std::time::Instant;

use egui::{Align2, Color32, FontData, FontDefinitions, FontFamily, FontId, Pos2, RawInput, Rect, Stroke, Vec2};
use egui_software_backend::{BufferMutRef, ColorFieldOrder, EguiSoftwareRender};
use scene::{Scene, View};

const INK: Color32 = Color32::from_rgb(24, 26, 40);

fn fonts() -> FontDefinitions {
    let mut defs = FontDefinitions::empty();
    defs.font_data.insert("Inter".into(), Arc::new(FontData::from_static(scene::TEXT_FONT)));
    defs.font_data.insert("Bravura".into(), Arc::new(FontData::from_static(scene::MUSIC_FONT)));
    defs.families.insert(FontFamily::Proportional, vec!["Inter".into()]);
    defs.families.insert(FontFamily::Monospace, vec!["Inter".into()]);
    defs.families.insert(FontFamily::Name("Bravura".into()), vec!["Bravura".into()]);
    defs
}

/// Bravura ascent in em (for the baseline workaround).
fn ascent() -> f32 {
    let face = ttf_parser::Face::parse(scene::MUSIC_FONT, 0).unwrap();
    f32::from(face.ascender()) / f32::from(face.units_per_em())
}

/// Builds and paints one frame of the UI.
fn ui(root: &mut egui::Ui, scene: &Scene, view: View, ascent: f32) {
    egui::Panel::top("header").exact_size(scene::SCORE_TOP).show_inside(root, |ui| {
        egui::MenuBar::new().ui(ui, |ui| {
            for item in scene::MENU_ITEMS {
                ui.menu_button(item, |ui| {
                    let _ = ui.button("…");
                });
            }
        });
        ui.columns(2, |cols| {
            for (i, (_, t)) in scene::TEXTS.iter().enumerate() {
                cols[i / 3].label(egui::RichText::new(*t).size(scene::TEXT_SIZE));
            }
        });
    });
    egui::CentralPanel::default()
        .frame(egui::Frame::NONE.fill(Color32::WHITE))
        .show_inside(root, |ui| {
            let painter = ui.painter();
            let top = scene::SCORE_TOP;
            let (w, h) = (scene::WIDTH as f32, scene::HEIGHT as f32);
            let pps = view.px_per_sp();
            for line in &scene.lines {
                let (x0, y0) = view.to_px(line.x0, line.y0);
                let (x1, y1) = view.to_px(line.x1, line.y1);
                if x0.max(x1) < 0.0 || x0.min(x1) > w || y0.max(y1) < top || y0.min(y1) > h {
                    continue;
                }
                painter.line_segment([Pos2::new(x0, y0), Pos2::new(x1, y1)], Stroke::new((line.width * pps).max(1.0), INK));
            }
            let size = 4.0 * pps;
            let font = FontId::new(size, FontFamily::Name("Bravura".into()));
            for g in &scene.glyphs {
                let (x, y) = view.to_px(g.x, g.y);
                if x < -size || x > w + size || y < top - size || y > h + size {
                    continue;
                }
                painter.text(Pos2::new(x, y - ascent * size), Align2::LEFT_TOP, g.ch, font.clone(), INK);
            }
        });
}

/// Headless renderer state: egui context + CPU rasteriser + RGBA buffer.
struct Headless {
    ctx: egui::Context,
    sw: EguiSoftwareRender,
    buffer: Vec<[u8; 4]>,
    scene: Scene,
    ascent: f32,
}

impl Headless {
    fn new() -> Self {
        let ctx = egui::Context::default();
        ctx.set_fonts(fonts());
        ctx.set_visuals(egui::Visuals::light());
        Self {
            ctx,
            sw: EguiSoftwareRender::new(ColorFieldOrder::Rgba),
            buffer: vec![[255; 4]; (scene::WIDTH * scene::HEIGHT) as usize],
            scene: scene::build(),
            ascent: ascent(),
        }
    }

    fn frame(&mut self, view: View) {
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(scene::WIDTH as f32, scene::HEIGHT as f32))),
            ..RawInput::default()
        };
        let (scene, ascent) = (&self.scene, self.ascent);
        let out = self.ctx.run_ui(input, |root| ui(root, scene, view, ascent));
        let primitives = self.ctx.tessellate(out.shapes, out.pixels_per_point);
        self.buffer.fill([255; 4]);
        let mut target = BufferMutRef::new(&mut self.buffer, scene::WIDTH as usize, scene::HEIGHT as usize);
        self.sw.render(&mut target, &primitives, &out.textures_delta, out.pixels_per_point);
    }
}

fn headless(out: &str) {
    let mut h = Headless::new();
    let start = Instant::now();
    h.frame(View::OVERVIEW);
    // egui needs a second pass for layout that depends on the previous frame (panels, menus).
    h.frame(View::OVERVIEW);
    println!("first two frames: {:.2} ms", start.elapsed().as_secs_f64() * 1e3);
    let file = std::fs::File::create(out).unwrap();
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), scene::WIDTH, scene::HEIGHT);
    enc.set_color(png::ColorType::Rgba);
    let mut w = enc.write_header().unwrap();
    w.write_image_data(h.buffer.as_flattened()).unwrap();
    println!("wrote {out}");
}

fn bench() {
    let mut h = Headless::new();
    h.frame(View::OVERVIEW);
    h.frame(View::OVERVIEW);
    let times = scene::bench_views()
        .into_iter()
        .map(|v| {
            let start = Instant::now();
            h.frame(v);
            start.elapsed()
        })
        .collect();
    scene::report("egui 0.34 + egui_software_backend (CPU, 1920x1080)", times);
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("headless") => headless(args.get(2).map_or("egui.png", String::as_str)),
        Some("bench") => bench(),
        _ => eprintln!("usage: gui-egui headless <out.png> | bench"),
    }
}
