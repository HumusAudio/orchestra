// SPDX-License-Identifier: GPL-3.0-or-later

//! Glyph outlines, delivered to a sink of our own (ADR-0044 TX2).
//!
//! # Why our own sink
//! `skrifa` draws into its `PathSink` trait; `tiny-skia` (in `orchestra-ui`) builds paths with
//! its own builder. A trait of this crate in between keeps `skrifa` out of `ui` and `tiny-skia`
//! out of this crate, and turns the font's y-up coordinates into the interface's y-down ones in
//! one place.

use skrifa::{DrawMemory, PathSink, ScalerFont};

use crate::{FaceId, Faces, GlyphId, TextError};

/// Receives the outline of a glyph, in logical pixels, **y down**, relative to the glyph's
/// origin on the baseline (so a capital letter has negative y values).
///
/// The calls describe closed contours as the font stores them: a `move_to`, segments, a
/// `close`. Quadratic segments come from TrueType outlines (JetBrains Mono), cubic ones from CFF
/// outlines (Inter, Bravura).
pub trait OutlineSink {
    /// Starts a new contour at `(x, y)`.
    fn move_to(&mut self, x: f32, y: f32);
    /// A straight segment to `(x, y)`.
    fn line_to(&mut self, x: f32, y: f32);
    /// A quadratic Bézier with control point `(cx, cy)`, ending at `(x, y)`.
    fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32);
    /// A cubic Bézier with control points `(c1x, c1y)` and `(c2x, c2y)`, ending at `(x, y)`.
    fn cubic_to(&mut self, c1x: f32, c1y: f32, c2x: f32, c2y: f32, x: f32, y: f32);
    /// Closes the current contour.
    fn close(&mut self);
}

/// Adapts an [`OutlineSink`] to `skrifa`'s `PathSink`, flipping y.
struct Flip<'a, S: OutlineSink> {
    /// The caller's sink.
    sink: &'a mut S,
}

impl<S: OutlineSink> PathSink for Flip<'_, S> {
    fn move_to(&mut self, x: f32, y: f32) {
        self.sink.move_to(x, -y);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.sink.line_to(x, -y);
    }
    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        self.sink.quad_to(cx0, -cy0, x, -y);
    }
    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.sink.cubic_to(cx0, -cy0, cx1, -cy1, x, -y);
    }
    fn close(&mut self) {
        self.sink.close();
    }
}

impl Faces {
    /// Draws the outline of `glyph` of `face` at `size` pixels per em into `sink`.
    ///
    /// Returns `false`, without calling the sink, when the font has no outline for the glyph
    /// (an ID beyond the font), and `true` otherwise, even when the outline is empty (a space
    /// draws no contour: the sink is simply not called; tested). Outlines are **unhinted**
    /// (ADR-0036 B2, ADR-0044 TX6): the same shape at every size on every platform.
    ///
    /// # Cost
    /// Proportional to the glyph's segments; it may allocate (`skrifa`'s scratch memory for the
    /// CFF interpreter). It is called when a glyph is rasterised into the cache (step F2.07b part
    /// 2), never per frame.
    ///
    /// # Errors
    /// [`TextError::InvalidSize`] if `size` is not finite and positive; [`TextError::Outline`]
    /// if the glyph's program in the font is corrupt.
    pub fn draw_outline<S: OutlineSink>(
        &self,
        face: FaceId,
        glyph: GlyphId,
        size: f32,
        sink: &mut S,
    ) -> Result<bool, TextError> {
        // Validate the size the same way as everywhere else (the scale itself is not needed:
        // skrifa scales the outline to `size` pixels per em).
        crate::style::checked_scale(self, face, size)?;
        // `unhinted` with a ppem gives outlines already scaled to pixels. Cloning the font is
        // a reference-count increment.
        let scaler = ScalerFont::unhinted(self.font(face).clone(), Some(size));
        let Some(outline) = scaler.outline(skrifa::GlyphId::new(glyph.0)) else {
            return Ok(false);
        };
        let mut flip = Flip { sink };
        outline
            .draw(DrawMemory::Scratch, &mut flip)
            .map_err(|_| TextError::Outline(face, glyph))?;
        Ok(true)
    }
}
