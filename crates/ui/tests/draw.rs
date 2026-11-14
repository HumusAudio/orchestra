// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of the drawing abstraction (ADR-0039 D10): pixels drawn headless, damage, culling,
//! theme tokens, clips, transforms, shadows, images and errors.
//!
//! Pixels are checked with **exact probes** where the result is exact by design (snapped edges,
//! opaque fills, the cleared surface) and with a tolerance where anti-aliasing is involved. The
//! strongest invariant is "drawing only the damage gives the same picture as drawing everything":
//! it is checked on a scene with every kind of command.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test helpers: a failed unwrap or index fails the test"
)]
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "test loop counters and grid sizes, all small and non-negative, turned into \
              coordinates and back"
)]

use orchestra_ui::{
    Appearance, BuiltInTheme, ColorToken, CpuRenderer, Damage, DisplayList, DrawError, FillRule,
    Image, LineCap, Paint, Point, Rect, Renderer, RoundedRect, SegmentId, StrokeStyle, Theme,
    Transform,
};

/// The theme of most tests: Studio, light (the default pair, ADR-0037 T1; the guides' screenshots
/// use its dark half since ADR-0041).
fn light() -> Theme {
    *BuiltInTheme::Studio.pair().instance(Appearance::Light)
}

/// Studio, dark.
fn dark() -> Theme {
    *BuiltInTheme::Studio.pair().instance(Appearance::Dark)
}

/// The premultiplied pixel an opaque token gives (the test fails if the token is not opaque,
/// because then the expected pixel would depend on what is under it).
fn opaque(theme: &Theme, token: ColorToken) -> [u8; 4] {
    let color = theme.color(token);
    assert_eq!(color.a, 255, "{token:?} is not opaque in this theme");
    [color.r, color.g, color.b, 255]
}

/// Draws the whole list into a new image.
fn draw(list: &DisplayList, theme: &Theme, width: u32, height: u32, scale: f32) -> Image {
    CpuRenderer::new(orchestra_text::Faces::bundled().unwrap())
        .render_to_image(list, theme, width, height, scale)
        .unwrap()
}

/// The pixel at `(x, y)`.
fn px(image: &Image, x: u32, y: u32) -> [u8; 4] {
    image.pixel(x, y).unwrap()
}

/// The largest difference of any channel of any pixel between two images of the same size.
fn max_difference(a: &Image, b: &Image) -> u8 {
    assert_eq!((a.width(), a.height()), (b.width(), b.height()));
    a.data()
        .iter()
        .zip(b.data())
        .map(|(x, y)| x.abs_diff(*y))
        .max()
        .unwrap_or(0)
}

#[test]
fn an_empty_list_is_the_surface() {
    let theme = light();
    let image = draw(&DisplayList::new(), &theme, 8, 4, 1.0);
    let surface = opaque(&theme, ColorToken::Surface);
    for y in 0..4 {
        for x in 0..8 {
            assert_eq!(px(&image, x, y), surface);
        }
    }
}

#[test]
fn a_rectangle_is_snapped_to_whole_pixels() {
    // Edges at fractional positions round to the nearest pixel boundary, on both axes with a case
    // where rounding down would differ: x 10.6 → 11, 20.4 → 20; y 10.3 → 10, 20.6 → 21. Inside
    // and on the edge the colour is exact (no anti-aliasing).
    let theme = light();
    let mut list = DisplayList::new();
    list.record(SegmentId(1), |r| {
        r.fill_rect(Rect::new(10.6, 10.3, 20.4, 20.6), ColorToken::Accent);
    });
    let image = draw(&list, &theme, 32, 32, 1.0);
    let accent = opaque(&theme, ColorToken::Accent);
    let surface = opaque(&theme, ColorToken::Surface);
    assert_eq!(px(&image, 11, 10), accent, "top-left pixel");
    assert_eq!(px(&image, 19, 20), accent, "bottom-right pixel");
    assert_eq!(px(&image, 10, 15), surface, "left of the edge");
    assert_eq!(px(&image, 20, 15), surface, "right of the edge");
    assert_eq!(px(&image, 15, 9), surface, "above the edge");
    assert_eq!(px(&image, 15, 21), surface, "below the edge");
}

#[test]
fn a_hairline_never_vanishes() {
    // A rectangle 0.2 px tall keeps one pixel, the one containing its centre.
    let theme = light();
    let mut list = DisplayList::new();
    list.record(SegmentId(1), |r| {
        r.fill_rect(Rect::new(2.0, 5.3, 6.0, 5.5), ColorToken::Text);
    });
    let image = draw(&list, &theme, 8, 8, 1.0);
    let text = opaque(&theme, ColorToken::Text);
    assert_eq!(px(&image, 3, 5), text);
    assert_eq!(px(&image, 3, 4), opaque(&theme, ColorToken::Surface));
    assert_eq!(px(&image, 3, 6), opaque(&theme, ColorToken::Surface));
}

#[test]
fn horizontal_and_vertical_lines_are_crisp() {
    // A 1 px line centred on y = 4.0 (a pixel boundary) or 4.5 (a pixel centre) fills exactly
    // row 4: no half-transparent second row. Same for a vertical line and columns.
    let theme = light();
    let text = opaque(&theme, ColorToken::Text);
    let surface = opaque(&theme, ColorToken::Surface);
    for centre in [4.0, 4.5, 4.9] {
        let mut list = DisplayList::new();
        list.record(SegmentId(1), |r| {
            r.line(
                Point::new(1.0, centre),
                Point::new(9.0, centre),
                StrokeStyle::new(1.0),
                ColorToken::Text,
            );
            r.line(
                Point::new(centre + 10.0, 1.0),
                Point::new(centre + 10.0, 9.0),
                StrokeStyle::new(1.0),
                ColorToken::Text,
            );
        });
        let image = draw(&list, &theme, 20, 10, 1.0);
        assert_eq!(px(&image, 5, 4), text, "row 4 at centre {centre}");
        assert_eq!(px(&image, 5, 3), surface, "row 3 at centre {centre}");
        assert_eq!(px(&image, 5, 5), surface, "row 5 at centre {centre}");
        assert_eq!(px(&image, 14, 5), text, "column 14 at centre {centre}");
        assert_eq!(px(&image, 13, 5), surface);
        assert_eq!(px(&image, 15, 5), surface);
    }
}

#[test]
fn an_even_line_is_centred_on_a_pixel_boundary() {
    // 2 px centred on y = 5.2: the nearest boundary is 5, so rows 4 and 5.
    let theme = light();
    let mut list = DisplayList::new();
    list.record(SegmentId(1), |r| {
        r.line(
            Point::new(0.0, 5.2),
            Point::new(8.0, 5.2),
            StrokeStyle::new(2.0),
            ColorToken::Text,
        );
    });
    let image = draw(&list, &theme, 8, 10, 1.0);
    let text = opaque(&theme, ColorToken::Text);
    let surface = opaque(&theme, ColorToken::Surface);
    assert_eq!(px(&image, 3, 4), text);
    assert_eq!(px(&image, 3, 5), text);
    assert_eq!(px(&image, 3, 3), surface);
    assert_eq!(px(&image, 3, 6), surface);
}

#[test]
fn square_caps_extend_and_butt_caps_do_not() {
    let theme = light();
    let text = opaque(&theme, ColorToken::Text);
    let surface = opaque(&theme, ColorToken::Surface);
    let mut list = DisplayList::new();
    list.record(SegmentId(1), |r| {
        r.line(
            Point::new(4.0, 2.5),
            Point::new(8.0, 2.5),
            StrokeStyle::new(2.0),
            ColorToken::Text,
        );
        let mut square = StrokeStyle::new(2.0);
        square.cap = LineCap::Square;
        r.line(
            Point::new(4.0, 7.5),
            Point::new(8.0, 7.5),
            square,
            ColorToken::Text,
        );
    });
    let image = draw(&list, &theme, 12, 10, 1.0);
    // Butt: columns 4..8.
    assert_eq!(px(&image, 3, 2), surface);
    assert_eq!(px(&image, 4, 2), text);
    assert_eq!(px(&image, 7, 2), text);
    assert_eq!(px(&image, 8, 2), surface);
    // Square: one more pixel (half the width) at each end, columns 3..9.
    assert_eq!(px(&image, 3, 7), text);
    assert_eq!(px(&image, 8, 7), text);
    assert_eq!(px(&image, 2, 7), surface);
    assert_eq!(px(&image, 9, 7), surface);
}

#[test]
fn a_stroked_rectangle_is_four_crisp_bands() {
    // A 1 px outline centred on the edges of 2.5..9.5: the outline spans 2..10, with a hole.
    let theme = light();
    let mut list = DisplayList::new();
    list.record(SegmentId(1), |r| {
        r.stroke_rect(
            Rect::new(2.5, 2.5, 9.5, 9.5),
            StrokeStyle::new(1.0),
            ColorToken::ControlBorder,
        );
    });
    let image = draw(&list, &theme, 12, 12, 1.0);
    let border = theme.color(ColorToken::ControlBorder);
    let surface = opaque(&theme, ColorToken::Surface);
    for (x, y) in [(2, 2), (9, 2), (2, 9), (9, 9), (5, 2), (2, 5)] {
        assert_ne!(px(&image, x, y), surface, "outline at ({x}, {y})");
    }
    assert_eq!(px(&image, 5, 5), surface, "the hole");
    assert_eq!(px(&image, 1, 5), surface, "outside");
    if border.a == 255 {
        assert_eq!(px(&image, 2, 5), [border.r, border.g, border.b, 255]);
    }
}

#[test]
fn the_scale_factor_multiplies_logical_pixels() {
    // At scale 2, the logical rectangle 1..3 covers device pixels 2..6.
    let theme = light();
    let mut list = DisplayList::new();
    list.record(SegmentId(1), |r| {
        r.fill_rect(Rect::new(1.0, 1.0, 3.0, 3.0), ColorToken::Accent);
    });
    let image = draw(&list, &theme, 8, 8, 2.0);
    let accent = opaque(&theme, ColorToken::Accent);
    let surface = opaque(&theme, ColorToken::Surface);
    assert_eq!(px(&image, 2, 2), accent);
    assert_eq!(px(&image, 5, 5), accent);
    assert_eq!(px(&image, 1, 1), surface);
    assert_eq!(px(&image, 6, 6), surface);
}

#[test]
fn tokens_resolve_with_the_theme_given_at_draw_time() {
    // The same list in two themes gives each theme's colours (ADR-0039 D3).
    let mut list = DisplayList::new();
    list.record(SegmentId(1), |r| {
        r.fill_rect(Rect::new(0.0, 0.0, 4.0, 4.0), ColorToken::Accent);
    });
    for theme in [light(), dark()] {
        let image = draw(&list, &theme, 8, 8, 1.0);
        assert_eq!(px(&image, 1, 1), opaque(&theme, ColorToken::Accent));
        assert_eq!(px(&image, 6, 6), opaque(&theme, ColorToken::Surface));
    }
}

#[test]
fn opacity_multiplies_the_tokens_alpha() {
    let theme = light();
    let mut list = DisplayList::new();
    list.record(SegmentId(1), |r| {
        r.fill_rect(
            Rect::new(0.0, 0.0, 4.0, 4.0),
            Paint::with_opacity(ColorToken::Accent, 0),
        );
        r.fill_rect(
            Rect::new(4.0, 0.0, 8.0, 4.0),
            Paint::with_opacity(ColorToken::Accent, 128),
        );
    });
    let image = draw(&list, &theme, 8, 4, 1.0);
    let surface = opaque(&theme, ColorToken::Surface);
    let accent = opaque(&theme, ColorToken::Accent);
    assert_eq!(px(&image, 1, 1), surface, "opacity 0 draws nothing");
    // Half opacity: each channel about half way between surface and accent.
    let half = px(&image, 6, 1);
    for channel in 0..3 {
        let expected = u16::midpoint(u16::from(surface[channel]), u16::from(accent[channel]));
        assert!(
            u16::from(half[channel]).abs_diff(expected) <= 2,
            "channel {channel}: {half:?}"
        );
    }
}

#[test]
fn later_segments_draw_on_top_and_re_recording_keeps_the_place() {
    let theme = light();
    let mut list = DisplayList::new();
    list.record(SegmentId(1), |r| {
        r.fill_rect(Rect::new(0.0, 0.0, 4.0, 4.0), ColorToken::Accent);
    });
    list.record(SegmentId(2), |r| {
        r.fill_rect(Rect::new(0.0, 0.0, 4.0, 4.0), ColorToken::Text);
    });
    assert_eq!(
        px(&draw(&list, &theme, 4, 4, 1.0), 1, 1),
        opaque(&theme, ColorToken::Text)
    );
    // Re-recording segment 1 does not move it above segment 2.
    list.record(SegmentId(1), |r| {
        r.fill_rect(Rect::new(0.0, 0.0, 4.0, 4.0), ColorToken::Accent);
    });
    assert_eq!(
        px(&draw(&list, &theme, 4, 4, 1.0), 1, 1),
        opaque(&theme, ColorToken::Text)
    );
    assert_eq!(list.segment_count(), 2);
    // Removing segment 2 uncovers segment 1.
    list.remove(SegmentId(2));
    assert_eq!(list.segment_count(), 1);
    assert_eq!(
        px(&draw(&list, &theme, 4, 4, 1.0), 1, 1),
        opaque(&theme, ColorToken::Accent)
    );
}

#[test]
fn re_recording_damages_the_old_and_the_new_area_only() {
    let mut list = DisplayList::new();
    list.record(SegmentId(1), |r| {
        r.fill_rect(Rect::new(0.0, 0.0, 10.0, 10.0), ColorToken::Accent);
    });
    list.record(SegmentId(2), |r| {
        r.fill_rect(Rect::new(50.0, 50.0, 60.0, 60.0), ColorToken::Accent);
    });
    let _ = list.take_damage();
    assert!(list.damage().is_empty());
    // Segment 2 moves: its old and new places are damaged; segment 1 is not.
    list.record(SegmentId(2), |r| {
        r.fill_rect(Rect::new(80.0, 50.0, 90.0, 60.0), ColorToken::Accent);
    });
    let damage = list.take_damage();
    assert_eq!(
        damage.rects(),
        &[
            Rect::new(50.0, 50.0, 60.0, 60.0),
            Rect::new(80.0, 50.0, 90.0, 60.0)
        ]
    );
    // Removing damages the removed area; a theme change damages everything.
    list.remove(SegmentId(1));
    assert_eq!(
        list.take_damage().rects(),
        &[Rect::new(0.0, 0.0, 10.0, 10.0)]
    );
    list.damage_all();
    assert!(list.take_damage().is_all());
}

#[test]
fn only_the_damage_is_redrawn() {
    // Draw a frame, then paint a marker pixel by hand outside the next damage: a partial redraw
    // must leave it, a full one must clear it.
    let theme = light();
    let mut list = DisplayList::new();
    list.record(SegmentId(1), |r| {
        r.fill_rect(Rect::new(0.0, 0.0, 8.0, 8.0), ColorToken::Accent);
    });
    list.record(SegmentId(2), |r| {
        r.fill_rect(Rect::new(40.0, 40.0, 48.0, 48.0), ColorToken::Accent);
    });
    let mut renderer = CpuRenderer::new(orchestra_text::Faces::bundled().unwrap());
    let mut image = Image::new(64, 64).unwrap();
    let damage = list.take_damage();
    renderer
        .render(&list, &theme, &damage, 1.0, &mut image)
        .unwrap();
    // A marker in a corner far from segment 2, by rebuilding the image with one changed pixel.
    let mut data = image.data().to_vec();
    let marker = 63 * 64 * 4;
    data[marker..marker + 4].copy_from_slice(&[1, 2, 3, 255]);
    let mut image = Image::from_premultiplied_rgba(64, 64, data).unwrap();
    // Segment 2 changes colour: only its area is damaged and redrawn.
    list.record(SegmentId(2), |r| {
        r.fill_rect(Rect::new(40.0, 40.0, 48.0, 48.0), ColorToken::Text);
    });
    let damage = list.take_damage();
    renderer
        .render(&list, &theme, &damage, 1.0, &mut image)
        .unwrap();
    assert_eq!(
        px(&image, 44, 44),
        opaque(&theme, ColorToken::Text),
        "redrawn"
    );
    assert_eq!(
        px(&image, 0, 63),
        [1, 2, 3, 255],
        "outside the damage: untouched"
    );
    assert_eq!(
        px(&image, 2, 2),
        opaque(&theme, ColorToken::Accent),
        "segment 1 kept"
    );
    // More than half the frame damaged: everything is redrawn, the marker goes.
    let mut big = Damage::none();
    big.add(Rect::new(0.0, 0.0, 64.0, 40.0));
    renderer
        .render(&list, &theme, &big, 1.0, &mut image)
        .unwrap();
    assert_eq!(
        px(&image, 0, 63),
        opaque(&theme, ColorToken::Surface),
        "full redraw"
    );
}

/// A scene with every kind of command, for the partial-versus-full test.
fn every_command(list: &mut DisplayList) {
    let photo = list
        .add_image(
            Image::from_premultiplied_rgba(
                2,
                2,
                vec![
                    255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 128, 128, 128, 255,
                ],
            )
            .unwrap(),
        )
        .unwrap();
    list.record(SegmentId(1), |r| {
        r.shadow(
            RoundedRect::new(Rect::new(20.0, 24.0, 90.0, 64.0), 8.0),
            6.0,
            ColorToken::Shadow,
        );
        r.fill_rounded_rect(
            RoundedRect::new(Rect::new(20.0, 20.0, 90.0, 60.0), 8.0),
            ColorToken::SurfaceRaised,
        );
        r.stroke_rounded_rect(
            RoundedRect::new(Rect::new(20.0, 20.0, 90.0, 60.0), 8.0),
            StrokeStyle::new(1.5),
            ColorToken::Border,
        );
    });
    list.record(SegmentId(2), |r| {
        r.push_transform(Transform {
            scale: 1.5,
            offset: Point::new(5.0, 70.0),
        });
        r.push_clip(RoundedRect::new(Rect::new(0.0, 0.0, 40.0, 20.0), 5.0));
        r.fill_path(FillRule::NonZero, ColorToken::Accent, |p| {
            p.move_to(0.0, 10.0);
            p.cubic_to(10.0, -5.0, 30.0, 25.0, 45.0, 5.0);
            p.line_to(45.0, 25.0);
            p.close();
        });
        r.pop_clip();
        r.stroke_path(StrokeStyle::new(1.0), ColorToken::Text, |p| {
            p.move_to(0.0, 0.0);
            p.quad_to(20.0, 30.0, 40.0, 0.0);
        });
        r.pop_transform();
        r.line(
            Point::new(100.0, 10.0),
            Point::new(120.0, 90.0),
            StrokeStyle::new(2.0),
            ColorToken::Error,
        );
        r.image(photo, Rect::new(100.0, 70.0, 116.0, 86.0));
    });
}

#[test]
fn drawing_the_damage_gives_the_same_picture_as_drawing_everything() {
    let theme = light();
    let mut list = DisplayList::new();
    every_command(&mut list);
    let mut renderer = CpuRenderer::new(orchestra_text::Faces::bundled().unwrap());
    let mut partial = Image::new(128, 100).unwrap();
    renderer
        .render(&list, &theme, &Damage::all(), 1.0, &mut partial)
        .unwrap();
    // Re-record each segment identically and redraw only its damage, one at a time.
    let _ = list.take_damage();
    every_command(&mut list);
    for region in list.take_damage().rects() {
        let mut one = Damage::none();
        one.add(*region);
        renderer
            .render(&list, &theme, &one, 1.0, &mut partial)
            .unwrap();
    }
    let full = draw(&list, &theme, 128, 100, 1.0);
    // Exactly equal is expected; one unit allowed for anti-aliasing computed in a shifted pixmap.
    assert!(
        max_difference(&partial, &full) <= 1,
        "{}",
        max_difference(&partial, &full)
    );
}

#[test]
fn a_moving_shape_over_paths_redraws_them_exactly() {
    // Found by the owner in F2.04a: a disc moving over noteheads left their edge pixels a few
    // units off (up to 28 of 255), because tiny-skia anti-aliases a path cut by the edge of the
    // pixmap differently. A large rounded panel, noteheads (paths) inside a rounded clip, and a
    // disc moved in small steps across them: after every partial redraw the frame must equal a
    // full drawing of the same list.
    let theme = light();
    let mut list = DisplayList::new();
    list.record(SegmentId(1), |r| {
        r.fill_rounded_rect(
            RoundedRect::new(Rect::new(10.0, 10.0, 230.0, 110.0), 12.0),
            ColorToken::SurfaceRaised,
        );
        r.push_clip(RoundedRect::new(Rect::new(20.0, 20.0, 220.0, 100.0), 16.0));
        for note in 0..10_u8 {
            let x = 30.0 + f32::from(note) * 20.0;
            let y = 40.0 + f32::from(note % 5) * 9.0;
            r.fill_path(FillRule::NonZero, ColorToken::Text, |p| {
                p.move_to(x + 6.0, y);
                p.cubic_to(x + 6.0, y + 2.5, x + 3.3, y + 4.5, x, y + 4.5);
                p.cubic_to(x - 3.3, y + 4.5, x - 6.0, y + 2.5, x - 6.0, y);
                p.cubic_to(x - 6.0, y - 2.5, x - 3.3, y - 4.5, x, y - 4.5);
                p.cubic_to(x + 3.3, y - 4.5, x + 6.0, y - 2.5, x + 6.0, y);
                p.close();
            });
        }
        r.pop_clip();
    });
    let mut renderer = CpuRenderer::new(orchestra_text::Faces::bundled().unwrap());
    let mut frame = Image::new(240, 120).unwrap();
    let damage = list.take_damage();
    renderer
        .render(&list, &theme, &damage, 1.0, &mut frame)
        .unwrap();
    for step in 0..60_u8 {
        let x = 15.0 + f32::from(step) * 3.5;
        let y = 30.0 + f32::from(step % 7) * 8.0;
        list.record(SegmentId(2), |r| {
            r.fill_rounded_rect(
                RoundedRect::new(Rect::new(x - 7.0, y - 7.0, x + 7.0, y + 7.0), 7.0),
                Paint::with_opacity(ColorToken::Text, 160),
            );
        });
        let damage = list.take_damage();
        renderer
            .render(&list, &theme, &damage, 1.0, &mut frame)
            .unwrap();
        let full = draw(&list, &theme, 240, 120, 1.0);
        let difference = max_difference(&frame, &full);
        assert!(
            difference <= 1,
            "step {step}: partial and full differ by {difference}"
        );
    }
}

#[test]
fn clips_limit_drawing_and_nest() {
    let theme = light();
    let mut list = DisplayList::new();
    list.record(SegmentId(1), |r| {
        r.push_clip_rect(Rect::new(2.0, 2.0, 10.0, 10.0));
        r.push_clip_rect(Rect::new(6.0, 0.0, 20.0, 20.0));
        r.fill_rect(Rect::new(0.0, 0.0, 20.0, 20.0), ColorToken::Accent);
        r.pop_clip();
        r.pop_clip();
    });
    let image = draw(&list, &theme, 20, 20, 1.0);
    let accent = opaque(&theme, ColorToken::Accent);
    let surface = opaque(&theme, ColorToken::Surface);
    // Inside both clips: columns 6..10, rows 2..10.
    assert_eq!(px(&image, 6, 2), accent);
    assert_eq!(px(&image, 9, 9), accent);
    assert_eq!(px(&image, 5, 5), surface, "outside the inner clip");
    assert_eq!(px(&image, 10, 5), surface, "outside the outer clip");
    assert_eq!(px(&image, 7, 10), surface);
    // The segment's bounds are the clipped area, so the damage is too.
    assert_eq!(list.bounds(), Rect::new(6.0, 2.0, 10.0, 10.0));
}

#[test]
fn a_shape_clipped_away_is_not_recorded() {
    let mut list = DisplayList::new();
    list.record(SegmentId(1), |r| {
        r.push_clip_rect(Rect::new(0.0, 0.0, 10.0, 10.0));
        r.fill_rect(Rect::new(20.0, 20.0, 30.0, 30.0), ColorToken::Accent);
        r.pop_clip();
    });
    assert!(list.bounds().is_empty());
    assert!(list.take_damage().is_empty());
}

#[test]
fn transforms_translate_and_scale_and_are_closed_automatically() {
    let theme = light();
    let mut list = DisplayList::new();
    list.record(SegmentId(1), |r| {
        r.push_transform(Transform::translate(10.0, 0.0));
        r.push_transform(Transform::scale(2.0));
        r.fill_rect(Rect::new(1.0, 1.0, 2.0, 2.0), ColorToken::Accent);
        // No pops: the recorder closes both. An invalid transform counts as the identity.
        r.push_transform(Transform::scale(f32::NAN));
    });
    list.record(SegmentId(2), |r| {
        // Segment 2 starts with no transform at all.
        r.fill_rect(Rect::new(0.0, 0.0, 1.0, 1.0), ColorToken::Text);
        r.pop_transform();
    });
    let image = draw(&list, &theme, 16, 8, 1.0);
    // (1..2) × 2 + 10 = 12..14 horizontally, 2..4 vertically.
    assert_eq!(px(&image, 12, 2), opaque(&theme, ColorToken::Accent));
    assert_eq!(px(&image, 13, 3), opaque(&theme, ColorToken::Accent));
    assert_eq!(px(&image, 11, 2), opaque(&theme, ColorToken::Surface));
    assert_eq!(px(&image, 0, 0), opaque(&theme, ColorToken::Text));
    assert_eq!(list.bounds(), Rect::new(0.0, 0.0, 14.0, 4.0));
}

#[test]
fn bounds_include_the_stroke_and_the_shadow_reach() {
    let mut list = DisplayList::new();
    list.record(SegmentId(1), |r| {
        let mut stroke = StrokeStyle::new(4.0);
        stroke.join = orchestra_ui::LineJoin::Round;
        r.stroke_rect(Rect::new(10.0, 10.0, 20.0, 20.0), stroke, ColorToken::Text);
    });
    assert_eq!(list.bounds(), Rect::new(8.0, 8.0, 22.0, 22.0));
    list.record(SegmentId(1), |r| {
        r.shadow(
            RoundedRect::new(Rect::new(10.0, 10.0, 20.0, 20.0), 2.0),
            2.0,
            ColorToken::Shadow,
        );
    });
    // 3σ = 6 beyond the shape.
    assert_eq!(list.bounds(), Rect::new(4.0, 4.0, 26.0, 26.0));
}

#[test]
fn a_shadow_is_dark_inside_fades_out_and_is_symmetric() {
    let theme = light();
    let shadow = theme.color(ColorToken::Shadow);
    let mut list = DisplayList::new();
    list.record(SegmentId(1), |r| {
        r.shadow(
            RoundedRect::new(Rect::new(20.0, 20.0, 60.0, 60.0), 6.0),
            4.0,
            ColorToken::Shadow,
        );
    });
    let image = draw(&list, &theme, 80, 80, 1.0);
    let surface = opaque(&theme, ColorToken::Surface);
    let centre = px(&image, 40, 40);
    if shadow.a > 0 {
        assert_ne!(centre, surface, "the inside is shaded");
    }
    // Beyond 3σ there is nothing.
    assert_eq!(px(&image, 5, 40), surface);
    assert_eq!(px(&image, 40, 75), surface);
    // Symmetric about the centre (the shape is centred on pixel boundary 40).
    for d in 0..30 {
        assert_eq!(
            px(&image, 40 - 1 - d, 40),
            px(&image, 40 + d, 40),
            "row, offset {d}"
        );
        assert_eq!(
            px(&image, 40, 40 - 1 - d),
            px(&image, 40, 40 + d),
            "column, offset {d}"
        );
    }
    // Monotonic fade from the inside outwards along the middle row.
    let darkness = |x: u32| u32::from(255 - px(&image, x, 40)[0]);
    let mut previous = darkness(40);
    for x in 41..64 {
        let now = darkness(x);
        assert!(now <= previous + 1, "x = {x}: {now} after {previous}");
        previous = now;
    }
}

#[test]
fn an_image_is_drawn_into_its_rectangle() {
    let theme = light();
    let mut list = DisplayList::new();
    let pixels = vec![
        255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
    ];
    let id = list
        .add_image(Image::from_premultiplied_rgba(2, 2, pixels).unwrap())
        .unwrap();
    list.record(SegmentId(1), |r| r.image(id, Rect::new(4.0, 4.0, 6.0, 6.0)));
    let image = draw(&list, &theme, 10, 10, 1.0);
    // At scale 1 and whole-pixel placement each source pixel lands on one target pixel.
    assert_eq!(px(&image, 4, 4), [255, 0, 0, 255]);
    assert_eq!(px(&image, 5, 4), [0, 255, 0, 255]);
    assert_eq!(px(&image, 4, 5), [0, 0, 255, 255]);
    assert_eq!(px(&image, 5, 5), [255, 255, 255, 255]);
    // A removed image draws nothing.
    list.remove_image(id);
    let image = draw(&list, &theme, 10, 10, 1.0);
    assert_eq!(px(&image, 4, 4), opaque(&theme, ColorToken::Surface));
}

#[test]
fn an_anti_aliased_circle_has_the_right_area() {
    // The sum of the coverage of a filled circle of radius 20 is π·20² within 0.5 %, an
    // independent check of the anti-aliased path filling.
    let theme = light();
    let mut list = DisplayList::new();
    let radius = 20.0;
    list.record(SegmentId(1), |r| {
        r.fill_rounded_rect(
            RoundedRect::new(Rect::new(5.0, 5.0, 45.0, 45.0), radius),
            ColorToken::Text,
        );
    });
    let image = draw(&list, &theme, 50, 50, 1.0);
    let text = opaque(&theme, ColorToken::Text);
    let surface = opaque(&theme, ColorToken::Surface);
    // Coverage of a pixel from its red channel, between the surface's and the text's.
    let span = f64::from(surface[0]) - f64::from(text[0]);
    assert!(
        span.abs() > 50.0,
        "surface and text must differ enough to measure"
    );
    let mut area = 0.0;
    for y in 0..50 {
        for x in 0..50 {
            area += (f64::from(surface[0]) - f64::from(px(&image, x, y)[0])) / span;
        }
    }
    let expected = std::f64::consts::PI * 400.0;
    assert!(
        (area - expected).abs() / expected < 0.005,
        "{area} vs {expected}"
    );
}

#[test]
fn damage_merges_touching_rectangles_and_keeps_at_most_eight() {
    let mut damage = Damage::none();
    damage.add(Rect::new(0.0, 0.0, 10.0, 10.0));
    damage.add(Rect::new(10.0, 0.0, 20.0, 10.0));
    assert_eq!(
        damage.rects(),
        &[Rect::new(0.0, 0.0, 20.0, 10.0)],
        "touching edges merge"
    );
    damage.add(Rect::EMPTY);
    damage.add(Rect::new(5.0, 5.0, 5.0, 9.0));
    assert_eq!(damage.rects().len(), 1, "empty rectangles are ignored");
    // Ten far-apart squares: never more than eight rectangles, and their union is covered.
    let mut damage = Damage::none();
    let mut hull = Rect::EMPTY;
    for i in 0..10 {
        let x = i as f32 * 100.0;
        let square = Rect::new(x, 0.0, x + 10.0, 10.0);
        hull = hull.union(&square);
        damage.add(square);
        assert!(damage.rects().len() <= 8);
    }
    let covered = damage
        .rects()
        .iter()
        .fold(Rect::EMPTY, |acc, r| acc.union(r));
    assert_eq!(covered, hull);
    // Everything absorbs the rest.
    damage.add_all();
    assert!(damage.is_all());
    assert_eq!(damage.rects(), &[]);
}

#[test]
fn errors_are_reported() {
    assert_eq!(
        Image::new(0, 5).unwrap_err(),
        DrawError::InvalidSize {
            width: 0,
            height: 5
        }
    );
    assert_eq!(
        Image::from_premultiplied_rgba(2, 2, vec![0; 15]).unwrap_err(),
        DrawError::WrongDataLength {
            expected: 16,
            actual: 15
        }
    );
    let list = DisplayList::new();
    let mut image = Image::new(4, 4).unwrap();
    for scale in [0.0, -1.0, f32::INFINITY] {
        assert_eq!(
            CpuRenderer::new(orchestra_text::Faces::bundled().unwrap())
                .render(&list, &light(), &Damage::all(), scale, &mut image)
                .unwrap_err(),
            DrawError::InvalidScale { scale }
        );
    }
    assert!(
        CpuRenderer::new(orchestra_text::Faces::bundled().unwrap())
            .render(&list, &light(), &Damage::all(), f32::NAN, &mut image)
            .is_err()
    );
    for error in [
        DrawError::InvalidSize {
            width: 0,
            height: 1,
        },
        DrawError::TooManyImages,
        DrawError::PngEncoding {
            message: "x".to_owned(),
        },
    ] {
        assert_ne!(error.to_string(), "");
    }
}

/// PNG encoding (ADR-0041): an opaque image survives the round trip byte for byte, and a
/// translucent pixel comes back demultiplied and re-premultiplied within one unit.
///
/// Decoding uses `tiny-skia`'s own decoder (a dependency of the crate, so visible to its tests):
/// it premultiplies again, which is what makes the comparison with our premultiplied bytes direct.
#[test]
fn png_encoding_round_trips() {
    // An opaque scene with anti-aliased edges, so the image is not a single colour.
    let mut list = DisplayList::new();
    list.record(SegmentId(1), |r| {
        r.fill_rect(Rect::new(0.0, 0.0, 40.0, 30.0), ColorToken::Surface);
        r.fill_rounded_rect(
            RoundedRect::new(Rect::new(5.0, 5.0, 35.0, 25.0), 6.0),
            ColorToken::Accent,
        );
    });
    let image = draw(&list, &dark(), 40, 30, 1.0);
    let bytes = image.encode_png().unwrap();
    // The PNG signature: the file really is a PNG.
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
    let decoded = tiny_skia::Pixmap::decode_png(&bytes).unwrap();
    assert_eq!((decoded.width(), decoded.height()), (40, 30));
    assert_eq!(decoded.data(), image.data());

    // A translucent pixel: premultiplied [64, 32, 0, 128] is straight [127.5…, 63.7…, 0] — the
    // division rounds, so the round trip may be one unit off, never more.
    let translucent = Image::from_premultiplied_rgba(1, 1, vec![64, 32, 0, 128]).unwrap();
    let decoded = tiny_skia::Pixmap::decode_png(&translucent.encode_png().unwrap()).unwrap();
    for (a, b) in decoded.data().iter().zip(translucent.data()) {
        assert!(
            a.abs_diff(*b) <= 1,
            "{:?} vs {:?}",
            decoded.data(),
            translucent.data()
        );
    }
}

/// A notehead, the shape the score is made of: an ellipse of 5 × 4 px (zoom 1) as four cubics.
fn notehead(r: &mut orchestra_ui::Recorder<'_>, cx: f32, cy: f32, a: f32, b: f32) {
    let (ka, kb) = (a * 0.552_284_8, b * 0.552_284_8);
    r.fill_path(FillRule::NonZero, ColorToken::Text, |p| {
        p.move_to(cx + a, cy);
        p.cubic_to(cx + a, cy + kb, cx + ka, cy + b, cx, cy + b);
        p.cubic_to(cx - ka, cy + b, cx - a, cy + kb, cx - a, cy);
        p.cubic_to(cx - a, cy - kb, cx - ka, cy - b, cx, cy - b);
        p.cubic_to(cx + ka, cy - b, cx + a, cy - kb, cx + a, cy);
        p.close();
    });
}

/// Small round shapes keep their area: a notehead at zoom 1 (5 × 4 px) and at zoom 0.5
/// (2.5 × 2 px) lose at most 1.5 % and 3 % of the exact ellipse area (measured in F2.06b: 0.8 %
/// and 2.8 %). A renderer that flattens curves coarsely fails it: `vello_cpu` 0.3, with its fixed
/// 0.25 px tolerance, lost 4.4 % and 9.8 % (ADR-0043). The centre is off the pixel grid on purpose.
#[test]
fn small_noteheads_keep_their_area() {
    let theme = light();
    let text = opaque(&theme, ColorToken::Text);
    let surface = opaque(&theme, ColorToken::Surface);
    let span = f64::from(surface[0]) - f64::from(text[0]);
    for (a, b, limit) in [(5.0_f32, 4.0_f32, 0.015), (2.5, 2.0, 0.03)] {
        let mut list = DisplayList::new();
        list.record(SegmentId(1), |r| notehead(r, 25.3, 25.6, a, b));
        let image = draw(&list, &theme, 50, 50, 1.0);
        let mut area = 0.0;
        for y in 0..50 {
            for x in 0..50 {
                area += (f64::from(surface[0]) - f64::from(px(&image, x, y)[0])) / span;
            }
        }
        let expected = std::f64::consts::PI * f64::from(a) * f64::from(b);
        let loss = 1.0 - area / expected;
        assert!(
            loss.abs() <= limit,
            "{a} × {b}: {:.2} % of the area lost",
            loss * 100.0
        );
    }
}

/// A scene of staves and noteheads with a shadowed panel and a rounded clip: what the determinism
/// and partial-redraw tests draw.
fn busy_scene() -> DisplayList {
    let mut list = DisplayList::new();
    for staff in 0..6_u64 {
        for measure in 0..8_u64 {
            list.record(SegmentId(staff * 8 + measure), |r| {
                let top = 20.0 + staff as f32 * 60.0;
                let left = 20.0 + measure as f32 * 70.0;
                for line in 0..5 {
                    let y = top + line as f32 * 8.0;
                    r.line(
                        Point::new(left, y),
                        Point::new(left + 70.0, y),
                        StrokeStyle::new(1.0),
                        ColorToken::Text,
                    );
                }
                for note in 0..8 {
                    let step = (note * 3 + staff as usize + measure as usize) % 9;
                    notehead(
                        r,
                        left + 6.0 + note as f32 * 8.0 + 0.3,
                        top + 32.0 - step as f32 * 4.0,
                        5.0,
                        4.0,
                    );
                }
            });
        }
    }
    list.record(SegmentId(100), |r| {
        let panel = RoundedRect::new(Rect::new(420.0, 60.0, 600.0, 300.0), 10.0);
        r.shadow(
            RoundedRect::new(Rect::new(420.0, 64.0, 600.0, 304.0), 10.0),
            8.0,
            ColorToken::Shadow,
        );
        r.fill_rounded_rect(panel, ColorToken::SurfaceRaised);
        r.push_clip(panel);
        r.fill_rect(Rect::new(400.0, 40.0, 620.0, 100.0), ColorToken::Accent);
        r.pop_clip();
    });
    list
}

/// The same list gives the same bytes every time: twice with one renderer (its shadow cache is
/// then warm) and once with a fresh one, at scale 1 and 2.
#[test]
fn repeated_renders_give_the_same_pixels() {
    let theme = light();
    let list = busy_scene();
    for scale in [1.0, 2.0] {
        let size = |n: f32| (n * scale) as u32;
        let reference = CpuRenderer::new(orchestra_text::Faces::bundled().unwrap())
            .render_to_image(&list, &theme, size(640.0), size(400.0), scale)
            .unwrap();
        let mut renderer = CpuRenderer::new(orchestra_text::Faces::bundled().unwrap());
        for _ in 0..2 {
            let image = renderer
                .render_to_image(&list, &theme, size(640.0), size(400.0), scale)
                .unwrap();
            assert!(
                image == reference,
                "scale {scale}: differs by up to {}",
                max_difference(&image, &reference)
            );
        }
    }
}

/// Partial redraw of a busy scene: after moving notes in a small damaged region and in a large
/// one, and moving the shadowed panel (whose shadow comes from the cache, ADR-0043), the frame
/// equals a full redraw.
#[test]
fn partial_redraw_of_a_busy_scene_equals_a_full_redraw() {
    let theme = light();
    let mut list = busy_scene();
    let mut renderer = CpuRenderer::new(orchestra_text::Faces::bundled().unwrap());
    let mut frame = Image::new(640, 400).unwrap();
    renderer
        .render(&list, &theme, &Damage::all(), 1.0, &mut frame)
        .unwrap();
    let _ = list.take_damage();
    // A small edit (one measure) and a large one (a whole staff and the panel).
    list.record(SegmentId(3), |r| notehead(r, 250.0, 30.0, 5.0, 4.0));
    for measure in 0..8 {
        list.record(SegmentId(16 + measure), |r| {
            notehead(r, 30.0 + measure as f32 * 70.0, 150.0, 5.0, 4.0);
        });
    }
    list.record(SegmentId(100), |r| {
        r.shadow(
            RoundedRect::new(Rect::new(400.0, 80.0, 580.0, 320.0), 10.0),
            8.0,
            ColorToken::Shadow,
        );
    });
    let damage = list.take_damage();
    renderer
        .render(&list, &theme, &damage, 1.0, &mut frame)
        .unwrap();
    let full = CpuRenderer::new(orchestra_text::Faces::bundled().unwrap())
        .render_to_image(&list, &theme, 640, 400, 1.0)
        .unwrap();
    assert!(
        frame == full,
        "partial redraw differs by up to {}",
        max_difference(&frame, &full)
    );
}

/// Rounded rectangles drawn from the shape cache (F2.09b2) match `tiny-skia` filling and
/// stroking the same shape directly, at fractional positions and several scales: the cache's
/// geometry (size, radius, sub-pixel phase, margin, stroke) is the shape's.
#[test]
fn cached_rounded_rectangles_match_direct_drawing() {
    use tiny_skia::{
        Color, FillRule as SkiaFill, Paint as SkiaPaint, PathBuilder, Pixmap, Stroke,
        Transform as SkiaTransform,
    };
    let theme = light();
    let ink = theme.color(ColorToken::Text);
    for &scale in &[1.0_f32, 1.5, 2.0] {
        for &(x, y) in &[(10.0_f32, 10.0_f32), (10.3, 7.7), (12.75, 9.5)] {
            for stroke in [None, Some(1.0_f32), Some(2.5)] {
                let shape = RoundedRect::new(Rect::from_origin_size(x, y, 60.0, 24.0), 6.0);
                let mut list = DisplayList::new();
                list.record(SegmentId(1), |r| match stroke {
                    None => r.fill_rounded_rect(shape, ColorToken::Text),
                    Some(width) => {
                        r.stroke_rounded_rect(shape, StrokeStyle::new(width), ColorToken::Text);
                    }
                });
                let (w, h) = ((90.0 * scale) as u32, (45.0 * scale) as u32);
                let ours = draw(&list, &theme, w, h, scale);
                // The reference: tiny-skia directly. The same rounded rectangle (four sides and
                // four quarter circles as cubics with the KAPPA control distance, as the renderer
                // builds it), scaled to device pixels, then filled, or stroked into an outline and
                // filled, with anti-aliasing.
                let surface = theme.color(ColorToken::Surface);
                let mut reference = Pixmap::new(w, h).unwrap();
                reference.fill(Color::from_rgba8(surface.r, surface.g, surface.b, 255));
                let (x1, y1, rr) = (x + 60.0, y + 24.0, 6.0_f32);
                let k = 0.552_284_8 * rr;
                let mut pb = PathBuilder::new();
                pb.move_to(x + rr, y);
                pb.line_to(x1 - rr, y);
                pb.cubic_to(x1 - rr + k, y, x1, y + rr - k, x1, y + rr);
                pb.line_to(x1, y1 - rr);
                pb.cubic_to(x1, y1 - rr + k, x1 - rr + k, y1, x1 - rr, y1);
                pb.line_to(x + rr, y1);
                pb.cubic_to(x + rr - k, y1, x, y1 - rr + k, x, y1 - rr);
                pb.line_to(x, y + rr);
                pb.cubic_to(x, y + rr - k, x + rr - k, y, x + rr, y);
                pb.close();
                let device = pb
                    .finish()
                    .unwrap()
                    .transform(SkiaTransform::from_scale(scale, scale))
                    .unwrap();
                let filled = match stroke {
                    None => device,
                    Some(width) => device
                        .stroke(
                            &Stroke {
                                width: width * scale,
                                ..Stroke::default()
                            },
                            1.0,
                        )
                        .unwrap(),
                };
                let mut paint = SkiaPaint::default();
                paint.set_color_rgba8(ink.r, ink.g, ink.b, 255);
                paint.anti_alias = true;
                reference.fill_path(
                    &filled,
                    &paint,
                    SkiaFill::Winding,
                    SkiaTransform::identity(),
                    None,
                );
                // Per pixel, the largest channel difference. The anti-aliased edges agree within 3
                // steps of 255; a stroked curve's outline is computed by tiny-skia from absolute
                // coordinates (ours in the mask, the reference in the frame), and its corner
                // subdivision can differ slightly with them: a few corner pixels may be off by more
                // (measured: 2 pixels, 13 steps, at scale 1.5 with a 2.5 px stroke), never by much.
                let differences: Vec<u8> = ours
                    .data()
                    .chunks(4)
                    .zip(reference.data().chunks(4))
                    .map(|(a, b)| a.iter().zip(b).map(|(p, q)| p.abs_diff(*q)).max().unwrap())
                    .collect();
                let worst = differences.iter().copied().max().unwrap();
                let outliers = differences.iter().filter(|&&d| d > 3).count();
                assert!(
                    worst <= 16 && outliers <= 4,
                    "scale {scale}, at ({x}, {y}), stroke {stroke:?}: {outliers} pixels over 3 steps, worst {worst}"
                );
            }
        }
    }
}

/// A rounded rectangle drawn from the shape cache respects the clip in effect (F2.09b2).
#[test]
fn cached_rounded_rectangles_respect_the_clip() {
    let theme = light();
    let mut list = DisplayList::new();
    list.record(SegmentId(1), |r| {
        r.push_clip_rect(Rect::new(0.0, 0.0, 40.0, 60.0));
        r.fill_rounded_rect(
            RoundedRect::new(Rect::new(10.0, 10.0, 90.0, 50.0), 8.0),
            ColorToken::Accent,
        );
        r.pop_clip();
    });
    let image = draw(&list, &theme, 100, 60, 1.0);
    let accent = opaque(&theme, ColorToken::Accent);
    let surface = opaque(&theme, ColorToken::Surface);
    assert_eq!(px(&image, 30, 30), accent, "inside the clip");
    for x in 41..100 {
        assert_eq!(px(&image, x, 30), surface, "outside the clip at x = {x}");
    }
}

/// A small damaged region cutting through cached shapes, a glyph run and a rectangular clip
/// (F2.10b2, precision of ADR-0043): what a text field's blinking caret redraws. Since F2.10b2
/// these no longer grow the redrawn area to their whole shape (they come from the caches, the
/// same pixels in any area), and the rectangular clip's mask is written row by row; a rounded
/// clip, a path, still grows it. The frame must still equal a full redraw, byte for byte, at
/// every scale.
#[test]
fn a_caret_sized_redraw_through_a_field_equals_a_full_redraw() {
    use orchestra_text::{FaceId, Faces, Features, Shaper, TextLayout, TextStyle};
    let theme = light();
    let faces = Faces::bundled().unwrap();
    let mut layout = TextLayout::new();
    Shaper::new()
        .layout(
            &faces,
            "Violin I, solo, con sordino - tutti divisi, sul ponticello mp",
            TextStyle {
                face: FaceId::InterRegular,
                size: 13.0,
                features: Features::default(),
            },
            None,
            &mut layout,
        )
        .unwrap();
    // A field: rounded background, border, focus ring, its text clipped to the padded area.
    let field = Rect::new(20.3, 30.6, 320.3, 58.6);
    let record_field = |list: &mut DisplayList| {
        list.record(SegmentId(1), |r| {
            r.fill_rounded_rect(RoundedRect::new(field, 4.0), ColorToken::SurfaceSunken);
            r.stroke_rounded_rect(
                RoundedRect::new(field.inflate(-0.5), 3.5),
                StrokeStyle::new(1.0),
                ColorToken::ControlBorder,
            );
            r.push_clip_rect(Rect::new(28.3, field.y0, 312.3, field.y1));
            r.text(&layout, Point::new(28.3 - 40.0, 35.6), ColorToken::Text);
            r.pop_clip();
            r.stroke_rounded_rect(
                RoundedRect::new(field.inflate(2.0), 6.0),
                StrokeStyle::new(2.0),
                ColorToken::Focus,
            );
        });
    };
    for scale in [1.0_f32, 1.5, 2.0] {
        let (w, h) = ((360.0 * scale) as u32, (90.0 * scale) as u32);
        let mut list = DisplayList::new();
        record_field(&mut list);
        // A rounded clip across the caret's places (a path: it must still grow the area).
        list.record(SegmentId(3), |r| {
            r.push_clip(RoundedRect::new(Rect::new(140.4, 33.2, 215.9, 56.1), 9.0));
            r.fill_rect(Rect::new(120.0, 20.0, 240.0, 70.0), ColorToken::Accent);
            r.pop_clip();
        });
        let caret = |x: f32| Rect::new(x, 35.6, x + 1.5, 53.6);
        list.record(SegmentId(2), |r| {
            r.fill_rect(caret(150.2), ColorToken::Text);
        });
        let mut renderer = CpuRenderer::new(faces.clone());
        let mut frame = Image::new(w, h).unwrap();
        renderer
            .render(&list, &theme, &Damage::all(), scale, &mut frame)
            .unwrap();
        let _ = list.take_damage();
        // The caret blinks off, then shows again elsewhere: two small redraws.
        list.record(SegmentId(2), |_| {});
        let damage = list.take_damage();
        renderer
            .render(&list, &theme, &damage, scale, &mut frame)
            .unwrap();
        list.record(SegmentId(2), |r| {
            r.fill_rect(caret(201.7), ColorToken::Text);
        });
        let damage = list.take_damage();
        renderer
            .render(&list, &theme, &damage, scale, &mut frame)
            .unwrap();
        let full = CpuRenderer::new(faces.clone())
            .render_to_image(&list, &theme, w, h, scale)
            .unwrap();
        assert!(
            frame == full,
            "scale {scale}: partial redraw differs by up to {}",
            max_difference(&frame, &full)
        );
    }
}
