// SPDX-License-Identifier: GPL-3.0-or-later

//! Line metrics and the face's bounding box, in logical pixels (ADR-0044 TX9, TX11).
//!
//! # Coordinates
//! Fonts measure upwards from the baseline (y up); the interface measures downwards (y down,
//! ADR-0039 D2). Every value here is already turned around: an ascent is a positive distance
//! *above* the baseline, a descent a positive distance *below* it, and a bounding box has its
//! `top` negative (above the baseline) and its `bottom` positive.

use skrifa::font::metrics::LineBox;

use crate::{FaceId, Faces, TextError};

/// The vertical metrics of one line of text in a face at a size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LineMetrics {
    /// Distance from the top of the line to the baseline, logical pixels (positive).
    pub ascent: f32,
    /// Distance from the baseline to the bottom of the line, logical pixels (positive).
    pub descent: f32,
    /// Extra space the font asks for between one line and the next, logical pixels (≥ 0).
    pub line_gap: f32,
}

impl LineMetrics {
    /// The distance between the baselines of two consecutive lines: ascent + descent + gap.
    #[must_use]
    pub fn line_height(&self) -> f32 {
        self.ascent + self.descent + self.line_gap
    }
}

/// The box enclosing every glyph of a face, relative to a glyph's origin on the baseline, in
/// logical pixels, y down.
///
/// The bounds of a glyph run (ADR-0044 TX9 point 3) are its advance extent widened by this box,
/// so a partial redraw never cuts an overhanging glyph (an italic `f`, an accent above a
/// capital, a Bravura symbol drawn far above its baseline).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FaceBounds {
    /// Leftmost extent, relative to the glyph origin (usually ≤ 0).
    pub left: f32,
    /// Topmost extent, relative to the baseline (negative: above it).
    pub top: f32,
    /// Rightmost extent, relative to the glyph origin.
    pub right: f32,
    /// Bottom extent, relative to the baseline (positive: below it).
    pub bottom: f32,
}

impl Faces {
    /// The line metrics of `face` at `size` pixels per em.
    ///
    /// # Which table
    /// From the `OS/2` typographic metrics when the face sets `USE_TYPO_METRICS` (Inter and
    /// JetBrains Mono do, checked on 2026-11-12), otherwise from `hhea`, otherwise from the
    /// typographic metrics anyway, otherwise from the face's bounding box (ADR-0044 TX11). This
    /// is the order browsers and FreeType-based toolkits use, so a line of Inter has the same
    /// height here as in a web page: 1.21 em.
    ///
    /// # Errors
    /// [`TextError::InvalidSize`] if `size` is not finite and positive.
    pub fn line_metrics(&self, face: FaceId, size: f32) -> Result<LineMetrics, TextError> {
        let scale = crate::style::checked_scale(self, face, size)?;
        let metrics = self.font(face).metrics();
        // Pick the line box in the documented order; `typo_line` alone (without the flag) is
        // the third choice because `hhea` is what most platforms use when the flag is off.
        let line = if metrics.use_typo_metrics {
            metrics.typo_line.or(metrics.hhea_line)
        } else {
            metrics.hhea_line.or(metrics.typo_line)
        };
        let (ascender, descender, gap) = match line {
            Some(LineBox {
                ascender,
                descender,
                line_gap,
            }) => (
                to_f32(ascender.to_f64()),
                to_f32(descender.to_f64()),
                to_f32(line_gap.to_f64()),
            ),
            // No line metrics at all: the bounding box is the only vertical measure left.
            None => (
                to_f32(metrics.bounds.y_max.to_f64()),
                to_f32(metrics.bounds.y_min.to_f64()),
                0.0,
            ),
        };
        Ok(LineMetrics {
            // The descender is negative in the font (below a y-up baseline); our descent is a
            // positive distance. A malformed font could give a positive descender or a negative
            // gap: clamp both, so a line height is never smaller than its ascent.
            ascent: (ascender * scale).max(0.0),
            descent: (-descender * scale).max(0.0),
            line_gap: (gap * scale).max(0.0),
        })
    }

    /// The bounding box of every glyph of `face` at `size` pixels per em (the `head` table's
    /// box), y down.
    ///
    /// # Errors
    /// [`TextError::InvalidSize`] if `size` is not finite and positive.
    pub fn bounds(&self, face: FaceId, size: f32) -> Result<FaceBounds, TextError> {
        let scale = crate::style::checked_scale(self, face, size)?;
        let bounds = self.font(face).metrics().bounds;
        Ok(FaceBounds {
            left: to_f32(bounds.x_min.to_f64()) * scale,
            // y up → y down: the top of the box is its largest y, negated.
            top: -to_f32(bounds.y_max.to_f64()) * scale,
            right: to_f32(bounds.x_max.to_f64()) * scale,
            bottom: -to_f32(bounds.y_min.to_f64()) * scale,
        })
    }
}

/// A font measurement (a 48.16 fixed-point number of font units, read as `f64`) as `f32`.
///
/// Font units are integers below 2^15 in practice (an em is at most 16 384 units), so the
/// narrowing loses nothing that matters.
#[allow(
    clippy::cast_possible_truncation,
    reason = "font measurements are far below f32's range and need no more than f32's precision"
)]
const fn to_f32(value: f64) -> f32 {
    value as f32
}
