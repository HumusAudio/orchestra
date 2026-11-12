// SPDX-License-Identifier: GPL-3.0-or-later

//! Measuring and outlines (ADR-0044 TX2, TX6, TX11): known metrics of the bundled faces, the
//! face bounds, and outlines in the interface's y-down coordinates.

#![allow(
    clippy::unwrap_used,
    reason = "test helpers: a failed unwrap fails the test"
)]

use orchestra_text::{FaceId, Faces, GlyphId, OutlineSink, TextError};

/// Records an outline's calls and its extent.
#[derive(Default)]
struct Recorder {
    /// Number of calls of each kind: move, line, quad, cubic, close.
    calls: [usize; 5],
    /// Smallest and largest coordinates seen (x min, y min, x max, y max).
    extent: Option<[f32; 4]>,
}

impl Recorder {
    /// Widens the extent with a point.
    fn see(&mut self, x: f32, y: f32) {
        let e = self.extent.get_or_insert([x, y, x, y]);
        *e = [e[0].min(x), e[1].min(y), e[2].max(x), e[3].max(y)];
    }
}

impl OutlineSink for Recorder {
    fn move_to(&mut self, x: f32, y: f32) {
        self.calls[0] += 1;
        self.see(x, y);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.calls[1] += 1;
        self.see(x, y);
    }
    fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
        self.calls[2] += 1;
        self.see(cx, cy);
        self.see(x, y);
    }
    fn cubic_to(&mut self, c1x: f32, c1y: f32, c2x: f32, c2y: f32, x: f32, y: f32) {
        self.calls[3] += 1;
        self.see(c1x, c1y);
        self.see(c2x, c2y);
        self.see(x, y);
    }
    fn close(&mut self) {
        self.calls[4] += 1;
    }
}

/// Inter's typographic metrics (`USE_TYPO_METRICS` set): 1984 / −494 / 0 on 2048 units.
#[test]
fn inter_line_metrics_come_from_the_typographic_table() {
    let faces = Faces::bundled().unwrap();
    let size = 13.0;
    let metrics = faces.line_metrics(FaceId::InterRegular, size).unwrap();
    assert!((metrics.ascent - 1984.0 / 2048.0 * size).abs() < 1e-4);
    assert!((metrics.descent - 494.0 / 2048.0 * size).abs() < 1e-4);
    assert_eq!(metrics.line_gap, 0.0);
    // A line of Inter is 1.21 em.
    assert!((metrics.line_height() / size - 2478.0 / 2048.0).abs() < 1e-5);
    // The three weights share their line metrics, so labels of mixed weight line up.
    for face in [FaceId::InterSemiBold, FaceId::InterBold] {
        assert_eq!(faces.line_metrics(face, size).unwrap(), metrics);
    }
}

/// JetBrains Mono (`USE_TYPO_METRICS` set): 1020 / −300 / 0 on 1000 units.
#[test]
fn jetbrains_mono_line_metrics() {
    let faces = Faces::bundled().unwrap();
    let metrics = faces.line_metrics(FaceId::JetBrainsMono, 10.0).unwrap();
    assert!((metrics.ascent - 10.2).abs() < 1e-4);
    assert!((metrics.descent - 3.0).abs() < 1e-4);
}

/// Metrics scale linearly with the size, and bad sizes are refused.
#[test]
fn metrics_scale_with_the_size() {
    let faces = Faces::bundled().unwrap();
    for face in FaceId::ALL {
        let small = faces.line_metrics(face, 10.0).unwrap();
        let large = faces.line_metrics(face, 40.0).unwrap();
        assert!((large.line_height() - 4.0 * small.line_height()).abs() < 1e-3);
        let bounds = faces.bounds(face, 10.0).unwrap();
        // y down: the top of the box is above the baseline, the bottom below it.
        assert!(
            bounds.top < 0.0 && bounds.bottom > 0.0,
            "{face:?} {bounds:?}"
        );
        assert!(bounds.left < bounds.right);
    }
    assert_eq!(
        faces.line_metrics(FaceId::Bravura, 0.0),
        Err(TextError::InvalidSize)
    );
    assert_eq!(
        faces.bounds(FaceId::Bravura, f32::NAN),
        Err(TextError::InvalidSize)
    );
}

/// A capital letter's outline is closed contours of cubics (Inter is CFF), above the baseline
/// (negative y), inside the face's bounds.
#[test]
fn a_capital_outline_is_above_the_baseline() {
    let faces = Faces::bundled().unwrap();
    let size = 100.0;
    let a = faces.glyph_for_char(FaceId::InterRegular, 'A').unwrap();
    let mut recorder = Recorder::default();
    assert!(
        faces
            .draw_outline(FaceId::InterRegular, a, size, &mut recorder)
            .unwrap()
    );
    let [moves, _, quads, _, closes] = recorder.calls;
    assert!(moves >= 2, "an A has an outer and an inner contour");
    assert_eq!(moves, closes, "every contour is closed");
    assert_eq!(quads, 0, "CFF outlines have no quadratic segments");
    let [x_min, y_min, x_max, y_max] = recorder.extent.unwrap();
    // The cap height of Inter is 1490 units: the top of the A is about 72.8 px above.
    assert!((y_min + 1490.0 / 2048.0 * size).abs() < 1.0, "top {y_min}");
    assert!(y_max.abs() < 0.5, "the A sits on the baseline: {y_max}");
    let bounds = faces.bounds(FaceId::InterRegular, size).unwrap();
    assert!(x_min >= bounds.left && x_max <= bounds.right);
    assert!(y_min >= bounds.top && y_max <= bounds.bottom);
}

/// JetBrains Mono is TrueType: its outlines are quadratic.
#[test]
fn truetype_outlines_are_quadratic() {
    let faces = Faces::bundled().unwrap();
    let o = faces.glyph_for_char(FaceId::JetBrainsMono, 'o').unwrap();
    let mut recorder = Recorder::default();
    faces
        .draw_outline(FaceId::JetBrainsMono, o, 20.0, &mut recorder)
        .unwrap();
    assert!(recorder.calls[2] > 0 && recorder.calls[3] == 0);
}

/// A space has an outline with nothing in it; an ID beyond the font has none at all.
#[test]
fn empty_and_missing_outlines() {
    let faces = Faces::bundled().unwrap();
    let space = faces.glyph_for_char(FaceId::InterRegular, ' ').unwrap();
    let mut recorder = Recorder::default();
    faces
        .draw_outline(FaceId::InterRegular, space, 13.0, &mut recorder)
        .unwrap();
    assert_eq!(recorder.calls[0], 0, "a space draws no contour");
    let mut recorder = Recorder::default();
    assert!(
        !faces
            .draw_outline(FaceId::InterRegular, GlyphId(60_000), 13.0, &mut recorder)
            .unwrap()
    );
    assert_eq!(recorder.calls, [0; 5]);
}

/// A Bravura notehead sits on the baseline, its centre on it (SMuFL: noteheads are centred on
/// the staff line their origin is on), half a staff space above and below.
#[test]
fn a_bravura_notehead_is_centred_on_the_baseline() {
    let faces = Faces::bundled().unwrap();
    // 32 px per em: a staff space of 8 px, the zoom of the reference scene.
    let size = 32.0;
    let notehead = faces.glyph_for_char(FaceId::Bravura, '\u{E0A4}').unwrap();
    let mut recorder = Recorder::default();
    faces
        .draw_outline(FaceId::Bravura, notehead, size, &mut recorder)
        .unwrap();
    let [x_min, y_min, x_max, y_max] = recorder.extent.unwrap();
    assert!(
        (y_min + 4.0).abs() < 0.5 && (y_max - 4.0).abs() < 0.5,
        "{y_min} {y_max}"
    );
    assert!(
        x_min > -0.5 && (x_max - 9.44).abs() < 0.5,
        "{x_min} {x_max}"
    );
}
