// SPDX-License-Identifier: GPL-3.0-or-later

//! The reference scene of F2.03 (`crates/ui/benches/draw.rs`), described ONCE as plain drawing
//! operations, so that every renderer draws exactly the same thing.
//!
//! - 10 staves × 20 measures at 1920 × 1080, 8 px per staff space: every symbol is visible, the
//!   worst case for a full frame. Staff lines, barlines and stems are 1 px snapped rectangles
//!   (what our renderer's snapping turns a 1 px line into); the 2000 noteheads are filled ellipses
//!   of four cubic Béziers (glyphs come in F2.07, so paths are the honest stand-in).
//! - One **segment** per measure of each staff (200), as a score view would record it; an edit
//!   replaces one segment.
//! - A side panel with a blurred shadow, a rounded surface, six rounded controls with borders:
//!   the shapes of widgets, which the score alone does not exercise.
//!
//! Colours are theme tokens resolved with Studio light, so every backend paints the same values.

use orchestra_ui::{Appearance, BuiltInTheme, ColorToken, Theme};

/// Staves in the scene.
pub const STAVES: usize = 10;
/// Measures per staff.
pub const MEASURES: usize = 20;
/// Notes per measure.
const NOTES_PER_MEASURE: usize = 10;
/// Pixels per staff space at zoom 1.
const PX_PER_SP: f64 = 8.0;
/// Distance between the top lines of two staves.
const STAFF_DISTANCE: f64 = 12.0 * PX_PER_SP;
/// Width of one measure.
const MEASURE_WIDTH: f64 = 10.0 * PX_PER_SP;
/// Room left for a clef.
const CLEF_WIDTH: f64 = 4.0 * PX_PER_SP;
/// Margin around the score.
const MARGIN: f64 = 16.0;
/// Frame width in device pixels (scale 1, the reference 1080p screen).
pub const WIDTH: u32 = 1920;
/// Frame height.
pub const HEIGHT: u32 = 1080;
/// The staff and measure of the edited note (the middle of the frame, as in F2.03's benchmark).
pub const EDITED: (usize, usize) = (4, 9);

/// An axis-aligned rectangle, device pixels, `x0 < x1`, `y0 < y1`.
#[derive(Debug, Clone, Copy)]
pub struct Rect {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
}

/// One drawing operation. Every backend implements these five and nothing else.
#[derive(Debug, Clone, Copy)]
pub enum Op {
    /// A filled rectangle (lines are 1 px rectangles on whole pixels: crisp everywhere).
    Rect(Rect, ColorToken),
    /// A filled rounded rectangle.
    RoundRect(Rect, f64, ColorToken),
    /// A stroked rounded rectangle, stroke width in pixels.
    StrokeRoundRect(Rect, f64, f64, ColorToken),
    /// A filled ellipse (centre, half axes), as four cubic Béziers.
    Ellipse {
        cx: f64,
        cy: f64,
        a: f64,
        b: f64,
        color: ColorToken,
    },
    /// A Gaussian-blurred rounded rectangle (rect, corner radius, standard deviation).
    Shadow(Rect, f64, f64, ColorToken),
}

/// The scene: the ops of every segment, plus the lifted variant of the edited segment.
pub struct Scene {
    /// `segments[staff * MEASURES + measure]`, then the panel as the last segment.
    pub segments: Vec<Vec<Op>>,
    /// The edited segment with its first note lifted by one staff position.
    pub edited_lifted: Vec<Op>,
    /// The theme every backend resolves tokens with.
    pub theme: Theme,
}

impl Scene {
    /// Builds the whole scene once (outside any measurement).
    pub fn new() -> Self {
        let mut segments = Vec::with_capacity(STAVES * MEASURES + 1);
        for staff in 0..STAVES {
            for measure in 0..MEASURES {
                segments.push(measure_ops(staff, measure, 0));
            }
        }
        segments.push(panel_ops());
        Self {
            segments,
            edited_lifted: measure_ops(EDITED.0, EDITED.1, 1),
            theme: *BuiltInTheme::Studio.pair().instance(Appearance::Light),
        }
    }

    /// Index of the edited segment in `segments`.
    pub fn edited_index() -> usize {
        EDITED.0 * MEASURES + EDITED.1
    }

    /// Number of ops in the whole scene (reported with the results).
    pub fn op_count(&self) -> usize {
        self.segments.iter().map(Vec::len).sum()
    }

    /// The colour of `token` as straight-alpha RGBA bytes.
    pub fn rgba(&self, token: ColorToken) -> [u8; 4] {
        let c = self.theme.color(token);
        [c.r, c.g, c.b, c.a]
    }
}

/// The ops of one measure: its five staff-line pieces, its barline, ten notes, five stems.
/// `lift` raises the first note by that many staff positions (the edit).
fn measure_ops(staff: usize, measure: usize, lift: usize) -> Vec<Op> {
    let top = MARGIN + staff as f64 * STAFF_DISTANCE;
    let left = MARGIN + CLEF_WIDTH + measure as f64 * MEASURE_WIDTH;
    let mut ops = Vec::with_capacity(21);
    for i in 0..5 {
        let y = top + i as f64 * PX_PER_SP;
        ops.push(Op::Rect(
            Rect {
                x0: left,
                y0: y,
                x1: left + MEASURE_WIDTH,
                y1: y + 1.0,
            },
            ColorToken::Text,
        ));
    }
    let x = left + MEASURE_WIDTH;
    ops.push(Op::Rect(
        Rect {
            x0: x,
            y0: top,
            x1: x + 1.0,
            y1: top + 4.0 * PX_PER_SP + 1.0,
        },
        ColorToken::Text,
    ));
    for note in 0..NOTES_PER_MEASURE {
        let position = (note * 3 + staff + measure) % 9 + if note == 0 { lift } else { 0 };
        let cx = left + 6.0 + note as f64 * 7.0;
        let cy = top + 4.0 * PX_PER_SP - position as f64 * PX_PER_SP * 0.5 + 0.5;
        ops.push(Op::Ellipse {
            cx,
            cy,
            a: 5.0,
            b: 4.0,
            color: ColorToken::Text,
        });
        if note % 2 == 0 {
            let sx = (cx + 4.0).floor();
            ops.push(Op::Rect(
                Rect {
                    x0: sx,
                    y0: (cy - 3.5 * PX_PER_SP).floor(),
                    x1: sx + 1.0,
                    y1: cy.floor(),
                },
                ColorToken::Text,
            ));
        }
    }
    ops
}

/// A side panel like a properties pane: shadow, raised surface, border, six controls (the
/// second one focused, the third one in the accent colour).
fn panel_ops() -> Vec<Op> {
    let panel = Rect {
        x0: 1680.0,
        y0: 16.0,
        x1: 1904.0,
        y1: 600.0,
    };
    let shadow = Rect {
        x0: panel.x0,
        y0: panel.y0 + 4.0,
        x1: panel.x1,
        y1: panel.y1 + 4.0,
    };
    let mut ops = vec![
        Op::Shadow(shadow, 10.0, 8.0, ColorToken::Shadow),
        Op::RoundRect(panel, 10.0, ColorToken::SurfaceRaised),
        Op::StrokeRoundRect(panel, 10.0, 1.0, ColorToken::Border),
    ];
    for i in 0..6 {
        let y = panel.y0 + 20.0 + i as f64 * 48.0;
        let control = Rect {
            x0: panel.x0 + 16.0,
            y0: y,
            x1: panel.x1 - 16.0,
            y1: y + 32.0,
        };
        let token = if i == 2 {
            ColorToken::Accent
        } else {
            ColorToken::Control
        };
        ops.push(Op::RoundRect(control, 6.0, token));
        ops.push(Op::StrokeRoundRect(
            control,
            6.0,
            1.0,
            ColorToken::ControlBorder,
        ));
        if i == 1 {
            let ring = Rect {
                x0: control.x0 - 3.0,
                y0: control.y0 - 3.0,
                x1: control.x1 + 3.0,
                y1: control.y1 + 3.0,
            };
            ops.push(Op::StrokeRoundRect(ring, 9.0, 2.0, ColorToken::Focus));
        }
    }
    ops
}

/// The four cubic Béziers of an ellipse, as `(start, [(c1, c2, end); 4])`, the same
/// approximation our renderer and the F2.03 benchmark use (kappa = 0.5522848).
pub fn ellipse_cubics(cx: f64, cy: f64, a: f64, b: f64) -> ((f64, f64), [[(f64, f64); 3]; 4]) {
    let (ka, kb) = (a * 0.552_284_8, b * 0.552_284_8);
    (
        (cx + a, cy),
        [
            [(cx + a, cy + kb), (cx + ka, cy + b), (cx, cy + b)],
            [(cx - ka, cy + b), (cx - a, cy + kb), (cx - a, cy)],
            [(cx - a, cy - kb), (cx - ka, cy - b), (cx, cy - b)],
            [(cx + ka, cy - b), (cx + a, cy - kb), (cx + a, cy)],
        ],
    )
}

/// The view of a frame: a horizontal scroll and a zoom about the origin, applied to the whole
/// scene as one transform (how a score view scrolls and zooms).
#[derive(Debug, Clone, Copy)]
pub struct View {
    /// Horizontal offset in device pixels (negative scrolls the music left).
    pub dx: f64,
    /// Scale factor (1 = the reference zoom).
    pub zoom: f64,
}

impl View {
    /// No scroll, zoom 1.
    pub const IDENTITY: Self = Self { dx: 0.0, zoom: 1.0 };

    /// The view of frame `i` of the scroll-and-zoom case: 4 px of scroll per frame over a
    /// 400 px range, and a zoom that breathes between 0.8 and 1.25 over 120 frames.
    pub fn animated(i: usize) -> Self {
        let t = i as f64;
        let dx = -((t * 4.0) % 400.0);
        let phase = (t / 120.0) * std::f64::consts::TAU;
        let zoom = 1.025 + 0.225 * phase.sin();
        Self { dx, zoom }
    }
}
