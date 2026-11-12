// SPDX-License-Identifier: GPL-3.0-or-later

//! Glyph runs drawn headless (ADR-0044 TX6–TX9, TC2–TC9): ink where the text is and nowhere
//! else, the glyph cache giving the same bytes as fresh drawing, quarter-pixel positions, whole
//! pixel baselines, partial redraws equal to full frames, clips, themes, glyphs too large for
//! the cache, and the scale factor.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test helpers: a failed unwrap or index fails the test"
)]
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "test loop counters and sizes, small and non-negative, turned into coordinates"
)]

use orchestra_text::{FaceId, Faces, Shaper, TextLayout, TextStyle};
use orchestra_ui::{
    Appearance, BuiltInTheme, ColorToken, CpuRenderer, Damage, DisplayList, Image, Point, Rect,
    Renderer, SegmentId, Theme,
};

/// Studio, light.
fn light() -> Theme {
    *BuiltInTheme::Studio.pair().instance(Appearance::Light)
}

/// Studio, dark.
fn dark() -> Theme {
    *BuiltInTheme::Studio.pair().instance(Appearance::Dark)
}

/// `text` laid out in Inter at `size`, wrapped at `width` if given.
fn layout(faces: &Faces, text: &str, size: f32, width: Option<f32>) -> TextLayout {
    let mut layout = TextLayout::new();
    Shaper::new()
        .layout(
            faces,
            text,
            TextStyle::new(FaceId::InterRegular, size),
            width,
            &mut layout,
        )
        .unwrap();
    layout
}

/// Draws the whole list with a fresh renderer.
fn draw(list: &DisplayList, theme: &Theme, width: u32, height: u32, scale: f32) -> Image {
    CpuRenderer::new(Faces::bundled().unwrap())
        .render_to_image(list, theme, width, height, scale)
        .unwrap()
}

/// The pixels of `image` that differ from the theme's surface: (x, y) pairs.
fn inked(image: &Image, theme: &Theme) -> Vec<(u32, u32)> {
    let surface = theme.color(ColorToken::Surface);
    let surface = [surface.r, surface.g, surface.b, 255];
    let mut out = Vec::new();
    for y in 0..image.height() {
        for x in 0..image.width() {
            if image.pixel(x, y).unwrap() != surface {
                out.push((x, y));
            }
        }
    }
    out
}

/// `true` when two images differ in at least one byte (without printing them on failure: a
/// test image is tens of kilobytes).
fn differ(a: &Image, b: &Image) -> bool {
    a.data() != b.data()
}

/// A list with one text segment at `origin`.
fn text_list(layout: &TextLayout, origin: Point) -> DisplayList {
    let mut list = DisplayList::new();
    list.record(SegmentId(1), |r| r.text(layout, origin, ColorToken::Text));
    list
}

#[test]
fn text_inks_inside_its_bounds_only() {
    let faces = Faces::bundled().unwrap();
    let text = layout(&faces, "Hello, Orchestra", 13.0, None);
    let origin = Point::new(10.0, 10.0);
    let list = text_list(&text, origin);
    let image = draw(&list, &light(), 160, 40, 1.0);
    let ink = inked(&image, &light());
    assert!(ink.len() > 100, "only {} pixels inked", ink.len());
    let bounds = text.bounds().unwrap();
    for (x, y) in ink {
        let (x, y) = (x as f32, y as f32);
        assert!(x + 1.0 >= origin.x + bounds.left && x <= origin.x + bounds.right);
        assert!(y + 1.0 >= origin.y + bounds.top && y <= origin.y + bounds.bottom);
    }
    // The darkest pixel is the text colour itself: stems of letters are fully covered.
    let text_color = light().color(ColorToken::Text);
    let darkest = (0..image.height())
        .flat_map(|y| (0..image.width()).map(move |x| (x, y)))
        .map(|(x, y)| image.pixel(x, y).unwrap())
        .min_by_key(|p| u32::from(p[0]) + u32::from(p[1]) + u32::from(p[2]))
        .unwrap();
    assert_eq!(darkest, [text_color.r, text_color.g, text_color.b, 255]);
}

#[test]
fn drawing_from_the_cache_gives_the_same_bytes_as_drawing_fresh() {
    let faces = Faces::bundled().unwrap();
    // 200 labels, as an interface frame might have (TC9).
    let labels: Vec<TextLayout> = (0..200)
        .map(|i| layout(&faces, &format!("Label {i}: Viertelnote"), 12.0, None))
        .collect();
    let mut list = DisplayList::new();
    list.record(SegmentId(1), |r| {
        for (i, label) in labels.iter().enumerate() {
            let at = Point::new(4.0 + (i % 4) as f32 * 150.25, 4.0 + (i / 4) as f32 * 15.5);
            r.text(label, at, ColorToken::Text);
        }
    });
    let theme = light();
    let mut renderer = CpuRenderer::new(faces.clone());
    let first = renderer
        .render_to_image(&list, &theme, 620, 800, 1.0)
        .unwrap();
    for _ in 0..3 {
        let again = renderer
            .render_to_image(&list, &theme, 620, 800, 1.0)
            .unwrap();
        assert_eq!(again.data(), first.data(), "a cached frame differs");
    }
    assert_eq!(draw(&list, &theme, 620, 800, 1.0).data(), first.data());
}

#[test]
fn positions_are_quarter_pixels_across_and_whole_pixels_down() {
    let faces = Faces::bundled().unwrap();
    let text = layout(&faces, "Ill", 13.0, None);
    let at = |x: f32, y: f32| draw(&text_list(&text, Point::new(x, y)), &light(), 40, 30, 1.0);
    let base = at(5.0, 5.0);
    // Below an eighth of a pixel the quarter does not change: the same bytes.
    assert_eq!(at(5.1, 5.0).data(), base.data());
    // A quarter of a pixel later: another mask, other bytes.
    assert!(differ(&at(5.25, 5.0), &base));
    // A whole pixel later: the same picture shifted by one column.
    let shifted = at(6.0, 5.0);
    for y in 0..30 {
        for x in 1..40 {
            assert_eq!(shifted.pixel(x, y), base.pixel(x - 1, y));
        }
    }
    // Vertically the BASELINE (origin + ascent) is rounded to a whole row. Start from an
    // origin that puts it exactly on row 18: up to 0.4 px lower it stays there, 0.6 px lower it
    // moves to row 19.
    let ascent = text.lines()[0].baseline;
    let on_row = at(5.0, 18.0 - ascent);
    assert_eq!(at(5.0, 18.4 - ascent).data(), on_row.data());
    assert!(differ(&at(5.0, 18.6 - ascent), &on_row));
}

#[test]
fn drawing_the_damage_gives_the_same_picture_as_drawing_everything() {
    let faces = Faces::bundled().unwrap();
    let paragraph = layout(
        &faces,
        "The project was saved by a newer version of Orchestra and cannot be opened here.",
        13.0,
        Some(150.0),
    );
    let big = faces.bounds(FaceId::Bravura, 300.0).unwrap();
    let clef = faces.glyph_for_char(FaceId::Bravura, '\u{E050}').unwrap();
    let scene = |list: &mut DisplayList, marker: f32| {
        list.record(SegmentId(1), |r| {
            r.text(&paragraph, Point::new(7.3, 5.6), ColorToken::Text);
        });
        // A G clef at 300 px: too large for the cache, filled as a path (TC7).
        list.record(SegmentId(2), |r| {
            r.glyphs(
                FaceId::Bravura,
                300.0,
                big,
                [(clef, Point::new(170.5, 220.0))],
                ColorToken::TextMuted,
            );
        });
        // A small marker moving over the text and the clef: its damage cuts through both.
        list.record(SegmentId(3), |r| {
            r.fill_rect(
                Rect::new(marker, 30.0, marker + 6.0, 140.0),
                ColorToken::Accent,
            );
        });
    };
    let theme = dark();
    let mut list = DisplayList::new();
    scene(&mut list, 20.0);
    let mut renderer = CpuRenderer::new(faces.clone());
    let mut partial = Image::new(300, 260).unwrap();
    renderer
        .render(&list, &theme, &Damage::all(), 1.0, &mut partial)
        .unwrap();
    for marker in [40.0, 75.5, 120.0, 190.0, 230.0] {
        let _ = list.take_damage();
        scene(&mut list, marker);
        let damage = list.take_damage();
        renderer
            .render(&list, &theme, &damage, 1.0, &mut partial)
            .unwrap();
        let full = draw(&list, &theme, 300, 260, 1.0);
        assert_eq!(partial.data(), full.data(), "marker at {marker}");
    }
}

#[test]
fn a_clip_limits_the_text() {
    let faces = Faces::bundled().unwrap();
    let text = layout(&faces, "MMMMMMMMMM", 20.0, None);
    let mut list = DisplayList::new();
    list.record(SegmentId(1), |r| {
        r.push_clip_rect(Rect::new(0.0, 0.0, 50.0, 40.0));
        r.text(&text, Point::new(5.0, 5.0), ColorToken::Text);
        r.pop_clip();
    });
    let image = draw(&list, &light(), 200, 40, 1.0);
    let ink = inked(&image, &light());
    assert_ne!(ink.len(), 0);
    assert!(ink.iter().all(|&(x, _)| x < 50), "ink beyond the clip");
}

#[test]
fn a_theme_change_recolours_the_same_coverage() {
    let faces = Faces::bundled().unwrap();
    let text = layout(&faces, "Theme", 16.0, None);
    let list = text_list(&text, Point::new(3.0, 3.0));
    let mut renderer = CpuRenderer::new(faces.clone());
    let light_image = renderer
        .render_to_image(&list, &light(), 70, 30, 1.0)
        .unwrap();
    let dark_image = renderer
        .render_to_image(&list, &dark(), 70, 30, 1.0)
        .unwrap();
    // The same pixels carry ink in both themes (one coverage mask, two colours).
    assert_eq!(inked(&light_image, &light()), inked(&dark_image, &dark()));
    assert_ne!(light_image.data(), dark_image.data());
}

#[test]
fn the_scale_factor_draws_glyphs_at_device_size() {
    let faces = Faces::bundled().unwrap();
    let text = layout(&faces, "Scale", 13.0, None);
    let list = text_list(&text, Point::new(2.0, 2.0));
    let one = inked(&draw(&list, &light(), 60, 24, 1.0), &light()).len();
    let two = inked(&draw(&list, &light(), 120, 48, 2.0), &light()).len();
    // Twice the size in each direction: about four times the inked pixels (edges add a little).
    let ratio = two as f32 / one as f32;
    assert!((3.0..4.6).contains(&ratio), "ratio {ratio}");
}

#[test]
fn glyphs_placed_one_by_one_are_drawn_like_text() {
    let faces = Faces::bundled().unwrap();
    // "A" as a text and "A" as a free glyph at the same origin: the same pixels.
    let text = layout(&faces, "A", 20.0, None);
    let first = text.glyphs()[0];
    let origin = Point::new(4.25, 3.0);
    let from_text = draw(&text_list(&text, origin), &light(), 30, 30, 1.0);
    let mut list = DisplayList::new();
    let face_box = faces.bounds(FaceId::InterRegular, 20.0).unwrap();
    list.record(SegmentId(1), |r| {
        r.glyphs(
            FaceId::InterRegular,
            20.0,
            face_box,
            [(
                first.glyph,
                Point::new(origin.x + first.x, origin.y + first.y),
            )],
            ColorToken::Text,
        );
    });
    assert_eq!(draw(&list, &light(), 30, 30, 1.0).data(), from_text.data());
}

#[test]
fn empty_text_and_bad_sizes_record_nothing() {
    let faces = Faces::bundled().unwrap();
    let empty = layout(&faces, "", 13.0, None);
    let mut list = DisplayList::new();
    let face_box = faces.bounds(FaceId::Bravura, 20.0).unwrap();
    let notehead = faces.glyph_for_char(FaceId::Bravura, '\u{E0A4}').unwrap();
    list.record(SegmentId(1), |r| {
        r.text(&empty, Point::new(0.0, 0.0), ColorToken::Text);
        for size in [0.0, -3.0, f32::NAN] {
            r.glyphs(
                FaceId::Bravura,
                size,
                face_box,
                [(notehead, Point::new(5.0, 5.0))],
                ColorToken::Text,
            );
        }
        r.glyphs(
            FaceId::Bravura,
            20.0,
            face_box,
            std::iter::empty(),
            ColorToken::Text,
        );
    });
    assert_eq!(list.bounds(), Rect::EMPTY);
}

/// One renderer drawing the same text at two sizes keeps two sets of masks: the second size
/// gives the same bytes as a fresh renderer (the size is part of the cache's key).
#[test]
fn one_renderer_keeps_every_size_apart() {
    let faces = Faces::bundled().unwrap();
    let small = text_list(&layout(&faces, "Size", 13.0, None), Point::new(2.0, 2.0));
    let large = text_list(&layout(&faces, "Size", 20.0, None), Point::new(2.0, 2.0));
    let mut renderer = CpuRenderer::new(faces.clone());
    renderer
        .render_to_image(&small, &light(), 60, 30, 1.0)
        .unwrap();
    let after_small = renderer
        .render_to_image(&large, &light(), 60, 30, 1.0)
        .unwrap();
    assert_eq!(
        after_small.data(),
        draw(&large, &light(), 60, 30, 1.0).data()
    );
}

/// A text inside a scaling transform is drawn at the transformed size: the same pixels as the
/// text laid out at the larger size and placed at the scaled origin (positions and sizes both
/// scale; the glyph shapes come from outlines drawn at the device size).
#[test]
fn a_scaling_transform_scales_the_glyphs() {
    let faces = Faces::bundled().unwrap();
    let text = layout(&faces, "Zoom", 10.0, None);
    let mut list = DisplayList::new();
    list.record(SegmentId(1), |r| {
        r.push_transform(orchestra_ui::Transform::scale(2.0));
        r.text(&text, Point::new(3.0, 2.0), ColorToken::Text);
        r.pop_transform();
    });
    let zoomed = inked(&draw(&list, &light(), 120, 50, 1.0), &light()).len();
    let plain = inked(
        &draw(
            &text_list(&text, Point::new(3.0, 2.0)),
            &light(),
            120,
            50,
            1.0,
        ),
        &light(),
    )
    .len();
    let ratio = zoomed as f32 / plain as f32;
    assert!((3.0..4.6).contains(&ratio), "ratio {ratio}");
}

/// The ink of a glyph lies where its outline is: the mask's offset from the rounded origin is
/// applied (an "H" at 40 px starts about 3 px right of its origin, its top 29 px above the
/// baseline).
#[test]
fn a_glyph_s_ink_matches_its_outline() {
    #[derive(Default)]
    struct Extent(Option<[f32; 4]>);
    impl orchestra_text::OutlineSink for Extent {
        fn move_to(&mut self, x: f32, y: f32) {
            self.see(x, y);
        }
        fn line_to(&mut self, x: f32, y: f32) {
            self.see(x, y);
        }
        fn quad_to(&mut self, _: f32, _: f32, x: f32, y: f32) {
            self.see(x, y);
        }
        fn cubic_to(&mut self, _: f32, _: f32, _: f32, _: f32, x: f32, y: f32) {
            self.see(x, y);
        }
        fn close(&mut self) {}
    }
    impl Extent {
        fn see(&mut self, x: f32, y: f32) {
            let e = self.0.get_or_insert([x, y, x, y]);
            *e = [e[0].min(x), e[1].min(y), e[2].max(x), e[3].max(y)];
        }
    }
    let faces = Faces::bundled().unwrap();
    let text = layout(&faces, "H", 40.0, None);
    let glyph = text.glyphs()[0];
    let mut outline = Extent::default();
    faces
        .draw_outline(FaceId::InterRegular, glyph.glyph, 40.0, &mut outline)
        .unwrap();
    let [x0, y0, x1, y1] = outline.0.unwrap();
    // The origin on whole pixels, so the expected ink is exact up to anti-aliasing.
    let origin = Point::new(10.0, 10.0);
    let image = draw(&text_list(&text, origin), &light(), 60, 60, 1.0);
    let ink = inked(&image, &light());
    let ink_x0 = ink.iter().map(|p| p.0).min().unwrap() as f32;
    let ink_x1 = ink.iter().map(|p| p.0).max().unwrap() as f32 + 1.0;
    let ink_y0 = ink.iter().map(|p| p.1).min().unwrap() as f32;
    let ink_y1 = ink.iter().map(|p| p.1).max().unwrap() as f32 + 1.0;
    let (gx, gy) = (origin.x + glyph.x, origin.y + glyph.y);
    assert!(x0 > 2.0, "the test needs a visible left bearing, got {x0}");
    assert!(
        (ink_x0 - (gx + x0)).abs() <= 1.0,
        "{ink_x0} against {}",
        gx + x0
    );
    assert!(
        (ink_x1 - (gx + x1)).abs() <= 1.0,
        "{ink_x1} against {}",
        gx + x1
    );
    assert!((ink_y0 - (gy.round() + y0)).abs() <= 1.0);
    assert!((ink_y1 - (gy.round() + y1)).abs() <= 1.0);
}

/// The bounds a text run records are its layout's bounds moved to its origin: the segment's
/// bounds, which damage and culling use.
#[test]
fn a_text_run_records_its_bounds_at_its_origin() {
    let faces = Faces::bundled().unwrap();
    let text = layout(&faces, "Bounds", 13.0, None);
    let origin = Point::new(100.0, 50.0);
    let list = text_list(&text, origin);
    let b = text.bounds().unwrap();
    assert_eq!(
        list.bounds(),
        Rect::new(
            origin.x + b.left,
            origin.y + b.top,
            origin.x + b.right,
            origin.y + b.bottom
        )
    );
}

/// Glyphs too large for the cache are filled as paths, which `tiny-skia` anti-aliases
/// differently when they are cut by the edge of a small pixmap (F2.04a): a partial redraw must
/// include the whole run (TC7). A disc moves over G clefs drawn at 4× scale (about 680 device
/// pixels tall), and every partial frame must equal a full drawing.
#[test]
fn a_moving_shape_over_large_glyphs_redraws_them_exactly() {
    let faces = Faces::bundled().unwrap();
    let clef = faces.glyph_for_char(FaceId::Bravura, '\u{E050}').unwrap();
    let face_box = faces.bounds(FaceId::Bravura, 100.0).unwrap();
    let theme = light();
    let mut list = DisplayList::new();
    list.record(SegmentId(1), |r| {
        r.glyphs(
            FaceId::Bravura,
            100.0,
            face_box,
            [
                (clef, Point::new(20.3, 140.0)),
                (clef, Point::new(90.7, 150.0)),
            ],
            ColorToken::Text,
        );
    });
    let scale = 4.0;
    let (width, height) = (180, 200);
    let mut renderer = CpuRenderer::new(faces.clone());
    let mut frame = Image::new(width * 4, height * 4).unwrap();
    let damage = list.take_damage();
    renderer
        .render(&list, &theme, &damage, scale, &mut frame)
        .unwrap();
    for step in 0..12_u8 {
        let x = 20.0 + f32::from(step) * 11.5;
        let y = 40.0 + f32::from(step % 5) * 21.0;
        list.record(SegmentId(2), |r| {
            r.fill_rounded_rect(
                orchestra_ui::RoundedRect::new(Rect::new(x - 5.0, y - 5.0, x + 5.0, y + 5.0), 5.0),
                orchestra_ui::Paint::with_opacity(ColorToken::Accent, 160),
            );
        });
        let damage = list.take_damage();
        renderer
            .render(&list, &theme, &damage, scale, &mut frame)
            .unwrap();
        let full = draw(&list, &theme, width * 4, height * 4, scale);
        let difference = frame
            .data()
            .iter()
            .zip(full.data())
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .unwrap_or(0);
        assert!(
            difference <= 1,
            "step {step}: partial and full differ by {difference}"
        );
    }
}

/// A glyph's own box (TD2) encloses the ink it draws: a run recorded with it is never cut by a
/// partial redraw.
#[test]
fn a_glyph_s_own_box_encloses_its_ink() {
    let faces = Faces::bundled().unwrap();
    for (face, symbol) in [
        (FaceId::Bravura, '\u{E0A4}'),
        (FaceId::Bravura, '\u{E050}'),
        (FaceId::InterRegular, 'g'),
    ] {
        let glyph = faces.glyph_for_char(face, symbol).unwrap();
        let own = faces.glyph_box(face, glyph, 32.0).unwrap().unwrap();
        let at = Point::new(30.0, 70.0);
        let mut list = DisplayList::new();
        list.record(SegmentId(1), |r| {
            r.glyphs(face, 32.0, own, [(glyph, at)], ColorToken::Text);
        });
        let image = draw(&list, &light(), 100, 120, 1.0);
        let ink = inked(&image, &light());
        assert_ne!(ink.len(), 0);
        for (x, y) in ink {
            let (x, y) = (x as f32, y as f32);
            assert!(
                x + 1.0 >= at.x + own.left && x <= at.x + own.right,
                "{symbol:?} x {x}"
            );
            assert!(
                y + 1.0 >= at.y + own.top && y <= at.y + own.bottom,
                "{symbol:?} y {y}"
            );
        }
    }
}
