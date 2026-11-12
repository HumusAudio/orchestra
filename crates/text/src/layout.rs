// SPDX-License-Identifier: GPL-3.0-or-later

//! The result of shaping and wrapping a text: positioned glyphs grouped into lines (ADR-0044
//! TX9).
//!
//! A [`TextLayout`] is built by [`Shaper::layout`](crate::Shaper::layout) and owned by the
//! caller (a widget, a region of the score), which keeps it as long as the text and the style
//! do not change and records it into its display-list segment as often as needed. Rebuilding a
//! layout reuses its vectors, so a layout of the same size does not allocate (ADR-0044 TX9).

use std::ops::Range;

use crate::{FaceId, LineMetrics};

/// A glyph of a face: the index of an outline in the font file.
///
/// Our own type, so that `skrifa`'s and `harfrust`'s glyph types never appear in the interfaces
/// of the crates above (ADR-0044 TX2). Glyph 0 is `.notdef`, the box drawn for a character the
/// face does not have (TX4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GlyphId(pub u32);

impl GlyphId {
    /// The `.notdef` glyph (TX4).
    pub const NOTDEF: Self = Self(0);
}

/// One glyph placed by the layout.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PositionedGlyph {
    /// The glyph.
    pub glyph: GlyphId,
    /// Its origin's x, logical pixels from the left edge of the layout. Not rounded: drawing
    /// rounds to a quarter of a device pixel (TX7).
    pub x: f32,
    /// Its origin's y, logical pixels from the top of the layout: the baseline of its line,
    /// moved by the glyph's own vertical offset (a mark placed above a letter, for example).
    pub y: f32,
    /// The byte offset, in the laid-out text, of the cluster this glyph belongs to (TB4): the
    /// first byte of the characters it draws. Several glyphs can share a cluster (a letter and
    /// a mark drawn separately), and a cluster can cover several characters (a ligature).
    pub cluster: u32,
}

/// One line of a layout.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    /// The line's glyphs, a range of [`TextLayout::glyphs`].
    pub glyphs: Range<usize>,
    /// The bytes of the text the line covers, including its trailing spaces and the line break
    /// that ends it (so the ranges of all lines, in order, cover the whole text exactly once).
    pub text: Range<usize>,
    /// The y of the line's baseline, logical pixels from the top of the layout.
    pub baseline: f32,
    /// The width of the line's content, logical pixels: from the left edge to the end of its
    /// last visible glyph's advance. Spaces at the end of the line hang past it and are not
    /// counted (TX10); a hyphen added at a soft hyphen is.
    pub width: f32,
    /// `true` when the line ends at a soft hyphen (U+00AD) where it was broken, so a visible
    /// hyphen was added as its last glyph (TB6).
    pub hyphenated: bool,
}

/// Shaped, wrapped text: its glyphs, its lines and its extent.
///
/// Created empty with [`TextLayout::new`] and filled by
/// [`Shaper::layout`](crate::Shaper::layout); an empty text still has one (empty) line, so a
/// caret has somewhere to stand (F2.10).
#[derive(Debug, Clone, PartialEq)]
pub struct TextLayout {
    /// Every glyph, line after line, in visual (= logical, left to right, TX5) order.
    pub(crate) glyphs: Vec<PositionedGlyph>,
    /// The lines, top to bottom.
    pub(crate) lines: Vec<Line>,
    /// The face the glyphs belong to (one run per style, TX9).
    pub(crate) face: FaceId,
    /// The size the glyphs were placed at, logical pixels per em.
    pub(crate) size: f32,
    /// The face's line metrics at that size.
    pub(crate) metrics: LineMetrics,
    /// The widest line's [`Line::width`].
    pub(crate) width: f32,
}

impl Default for TextLayout {
    fn default() -> Self {
        Self::new()
    }
}

impl TextLayout {
    /// An empty layout, ready to be filled by [`Shaper::layout`](crate::Shaper::layout).
    ///
    /// Allocates nothing: its vectors grow on the first layout and are reused afterwards.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            glyphs: Vec::new(),
            lines: Vec::new(),
            face: FaceId::InterRegular,
            size: 0.0,
            metrics: LineMetrics {
                ascent: 0.0,
                descent: 0.0,
                line_gap: 0.0,
            },
            width: 0.0,
        }
    }

    /// Every glyph of the layout, line after line.
    #[must_use]
    pub fn glyphs(&self) -> &[PositionedGlyph] {
        &self.glyphs
    }

    /// The lines, top to bottom (at least one after a layout).
    #[must_use]
    pub fn lines(&self) -> &[Line] {
        &self.lines
    }

    /// The face of every glyph.
    #[must_use]
    pub const fn face(&self) -> FaceId {
        self.face
    }

    /// The size the layout was made at, logical pixels per em.
    #[must_use]
    pub const fn size(&self) -> f32 {
        self.size
    }

    /// The face's line metrics at the layout's size.
    #[must_use]
    pub const fn line_metrics(&self) -> LineMetrics {
        self.metrics
    }

    /// The width of the widest line, logical pixels.
    #[must_use]
    pub const fn width(&self) -> f32 {
        self.width
    }

    /// The height of the layout, logical pixels: one line height per line.
    #[must_use]
    pub fn height(&self) -> f32 {
        // The number of lines is small (a paragraph), far below f32's exact integer range.
        #[allow(
            clippy::cast_precision_loss,
            reason = "a line count is far below 2^24, where f32 is exact"
        )]
        let lines = self.lines.len() as f32;
        lines * self.metrics.line_height()
    }

    /// Empties the layout, keeping the capacity of its vectors.
    pub(crate) fn clear(&mut self) {
        self.glyphs.clear();
        self.lines.clear();
        self.width = 0.0;
    }
}
