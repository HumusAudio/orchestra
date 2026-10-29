// SPDX-License-Identifier: GPL-3.0-or-later

//! The tiny-skia score renderer of the "own renderer" spike, shared with the Slint spike (which
//! has no immediate-mode canvas and displays this output as an image).

use std::collections::HashMap;

use scene::{Scene, View};
use tiny_skia::{Color, FillRule, Paint, PathBuilder, Pixmap, Rect, Stroke, Transform};
use ttf_parser::{Face, OutlineBuilder};

/// Collects a glyph outline from ttf-parser into a tiny-skia path (font units, y up).
struct Outline(PathBuilder);

impl OutlineBuilder for Outline {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.move_to(x, y);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.0.line_to(x, y);
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.0.quad_to(x1, y1, x, y);
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.0.cubic_to(x1, y1, x2, y2, x, y);
    }
    fn close(&mut self) {
        self.0.close();
    }
}

/// A font with a per-character cache of outlines and advances. Outlines are built once and then
/// only transformed, so a frame costs one rasterisation per visible glyph and no parsing.
struct CachedFont<'a> {
    face: Face<'a>,
    glyphs: HashMap<char, (Option<tiny_skia::Path>, f32)>,
}

impl<'a> CachedFont<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            face: Face::parse(data, 0).expect("valid font"),
            glyphs: HashMap::new(),
        }
    }

    fn units_per_em(&self) -> f32 {
        f32::from(self.face.units_per_em())
    }

    /// Outline (font units) and advance (font units) of `ch`.
    fn glyph(&mut self, ch: char) -> &(Option<tiny_skia::Path>, f32) {
        let face = &self.face;
        self.glyphs
            .entry(ch)
            .or_insert_with(|| match face.glyph_index(ch) {
                Some(id) => {
                    let mut outline = Outline(PathBuilder::new());
                    let path = face
                        .outline_glyph(id, &mut outline)
                        .and_then(|_| outline.0.finish());
                    let advance = f32::from(face.glyph_hor_advance(id).unwrap_or(0));
                    (path, advance)
                }
                None => (None, 0.0),
            })
    }
}

/// The renderer: fonts + reusable paints.
pub struct Renderer<'a> {
    /// Draw the menu bar and the sample texts (false when the toolkit draws them, as in Slint).
    pub header: bool,
    music: CachedFont<'a>,
    text: CachedFont<'a>,
}

impl<'a> Renderer<'a> {
    pub fn new() -> Self {
        Self {
            header: true,
            music: CachedFont::new(scene::MUSIC_FONT),
            text: CachedFont::new(scene::TEXT_FONT),
        }
    }

    /// Draws a string with its baseline at (x, y) px, size in px.
    fn draw_text(
        &mut self,
        pixmap: &mut Pixmap,
        s: &str,
        mut x: f32,
        y: f32,
        size: f32,
        paint: &Paint,
    ) {
        let scale = size / self.text.units_per_em();
        for ch in s.chars() {
            let (path, advance) = self.text.glyph(ch).clone();
            if let Some(path) = path {
                let t = Transform::from_row(scale, 0.0, 0.0, -scale, x, y);
                pixmap.fill_path(&path, paint, FillRule::Winding, t, None);
            }
            x += advance * scale;
        }
    }

    /// Renders one full frame: score (culled to the visible area), then the header on top.
    pub fn render(&mut self, pixmap: &mut Pixmap, scene: &Scene, view: View) {
        let (w, h) = (pixmap.width() as f32, pixmap.height() as f32);
        pixmap.fill(Color::WHITE);
        let mut ink = Paint::default();
        ink.set_color_rgba8(24, 26, 40, 255);
        ink.anti_alias = true;
        let pps = view.px_per_sp();

        // Lines: staff lines, barlines, stems. Culled against the window.
        for line in &scene.lines {
            let (x0, y0) = view.to_px(line.x0, line.y0);
            let (x1, y1) = view.to_px(line.x1, line.y1);
            if x0.max(x1) < 0.0 || x0.min(x1) > w || y0.max(y1) < scene::SCORE_TOP || y0.min(y1) > h
            {
                continue;
            }
            let mut pb = PathBuilder::new();
            pb.move_to(x0, y0);
            pb.line_to(x1, y1);
            if let Some(path) = pb.finish() {
                let stroke = Stroke {
                    width: (line.width * pps).max(1.0),
                    ..Stroke::default()
                };
                pixmap.stroke_path(&path, &ink, &stroke, Transform::identity(), None);
            }
        }

        // Music glyphs: 1 em = 4 sp (SMuFL), origin on the staff position.
        let scale = 4.0 * pps / self.music.units_per_em();
        let margin = 4.0 * pps;
        for glyph in &scene.glyphs {
            let (x, y) = view.to_px(glyph.x, glyph.y);
            if x < -margin || x > w + margin || y < scene::SCORE_TOP - margin || y > h + margin {
                continue;
            }
            if let (Some(path), _) = self.music.glyph(glyph.ch) {
                let t = Transform::from_row(scale, 0.0, 0.0, -scale, x, y);
                pixmap.fill_path(path, &ink, FillRule::Winding, t, None);
            }
        }

        if !self.header {
            return;
        }
        // Header (drawn last, over any score content scrolled under it).
        let mut bg = Paint::default();
        bg.set_color_rgba8(255, 255, 255, 255);
        pixmap.fill_rect(
            Rect::from_xywh(0.0, 0.0, w, scene::SCORE_TOP).unwrap(),
            &bg,
            Transform::identity(),
            None,
        );
        bg.set_color_rgba8(236, 236, 242, 255);
        pixmap.fill_rect(
            Rect::from_xywh(0.0, 0.0, w, scene::MENU_HEIGHT).unwrap(),
            &bg,
            Transform::identity(),
            None,
        );
        for (i, item) in scene::MENU_ITEMS.iter().enumerate() {
            self.draw_text(pixmap, item, 14.0 + i as f32 * 64.0, 19.0, 14.0, &ink);
        }
        for ((_, text), (x, y)) in scene::TEXTS.iter().zip(scene::text_positions()) {
            self.draw_text(pixmap, text, x, y, scene::TEXT_SIZE, &ink);
        }
    }
}
