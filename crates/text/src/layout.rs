// SPDX-License-Identifier: GPL-3.0-or-later

//! The result of shaping and wrapping a text: positioned glyphs grouped into lines (ADR-0044
//! TX9).
//!
//! A [`TextLayout`] is built by [`Shaper::layout`](crate::Shaper::layout) and owned by the
//! caller (a widget, a region of the score), which keeps it as long as the text and the style
//! do not change and records it into its display-list segment as often as needed. Rebuilding a
//! layout reuses its vectors, so a layout of the same size does not allocate (ADR-0044 TX9).

use std::ops::Range;

use crate::boundary::grapheme_boundaries;
use crate::{FaceId, GlyphBox, LineMetrics};

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

/// A place where a caret can stand in a line (ADR-0047 TF1): the start of a cluster, with the
/// pen's position there.
///
/// # Why the pen and not the glyph's x
/// A glyph's x includes its own horizontal offset (a mark moved over its letter), so the
/// boundary between two clusters is where the **pen** is: the sum of the advances before the
/// cluster. Hanging spaces at the end of a line are included, so a caret typed after a space moves
/// past it, although the line's content width does not count it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CaretStop {
    /// The cluster's first byte in the laid-out text. The last stop of each line has the line's
    /// end (after its last cluster).
    pub offset: u32,
    /// The pen there, logical pixels from the left edge of the layout.
    pub x: f32,
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
    /// The line's caret stops, a range of [`TextLayout::caret_stops`]: one per cluster in text
    /// order (control characters included: they have a place, if no width), then one for the
    /// line's end. An empty line has only the end, at x 0.
    pub carets: Range<usize>,
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
    /// The caret stops of every line, line after line (ADR-0047 TF1).
    pub(crate) carets: Vec<CaretStop>,
    /// The face the glyphs belong to (one run per style, TX9).
    pub(crate) face: FaceId,
    /// The size the glyphs were placed at, logical pixels per em.
    pub(crate) size: f32,
    /// The face's line metrics at that size.
    pub(crate) metrics: LineMetrics,
    /// The widest line's [`Line::width`].
    pub(crate) width: f32,
    /// The box enclosing the glyphs, `None` when there are none (TC3).
    pub(crate) bounds: Option<GlyphBox>,
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
            carets: Vec::new(),
            face: FaceId::InterRegular,
            size: 0.0,
            metrics: LineMetrics {
                ascent: 0.0,
                descent: 0.0,
                line_gap: 0.0,
            },
            width: 0.0,
            bounds: None,
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

    /// Every caret stop of the layout, line after line ([`Line::carets`] gives each line's).
    #[must_use]
    pub fn caret_stops(&self) -> &[CaretStop] {
        &self.carets
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

    /// The box enclosing every glyph of the layout, logical pixels from its top-left corner:
    /// each glyph's origin widened by the face's bounding box ([`GlyphBox`] explains why the
    /// face's box). `None` when the layout has no glyph (an empty text, or only line breaks).
    ///
    /// This is what a glyph run records as its bounds in the display list (ADR-0044 TX9).
    #[must_use]
    pub const fn bounds(&self) -> Option<GlyphBox> {
        self.bounds
    }

    /// The line a caret at `offset` stands on: the last line starting at or before it. An offset
    /// at a line's end that is also the next line's start (after a hard break, or where a line
    /// was wrapped) belongs to the next line, where typing would continue.
    ///
    /// O(log lines) (a binary search on the lines' starts).
    #[must_use]
    pub fn line_of(&self, offset: usize) -> usize {
        self.lines
            .partition_point(|line| line.text.start <= offset)
            .saturating_sub(1)
    }

    /// The x of a caret at `offset` in `text` (the text this layout was made from), logical pixels
    /// from the layout's left edge (ADR-0047 TF1). The caret's line is [`TextLayout::line_of`].
    ///
    /// `offset` is meant to be a grapheme boundary (the text field's always are). At the start of
    /// a cluster the answer is the pen there; **inside a cluster** that covers several graphemes
    /// (a ligature such as "fi", drawn as one glyph), the cluster's advance is split evenly among
    /// its graphemes, which is what browsers do: the font gives no finer positions (OpenType's
    /// ligature caret positions are rare in interface fonts and Inter has none). An offset inside
    /// a grapheme gets the x of that grapheme's start.
    ///
    /// # Complexity
    /// O(log stops) (a binary search) when `offset` starts a cluster, the usual case; inside a
    /// cluster, also O(n) in the text's bytes (its grapheme boundaries are walked from the
    /// start). No allocation.
    #[must_use]
    pub fn caret_x(&self, text: &str, offset: usize) -> f32 {
        let offset = offset.min(text.len());
        let Some(line) = self.lines.get(self.line_of(offset)) else {
            return 0.0;
        };
        let stops = self.carets.get(line.carets.clone()).unwrap_or_default();
        // The last stop at or before the offset; its successor (if any) ends that cluster.
        let index = stops
            .partition_point(|stop| stop.offset as usize <= offset)
            .saturating_sub(1);
        let Some(&stop) = stops.get(index) else {
            return 0.0;
        };
        match stops.get(index + 1) {
            Some(&next) if stop.offset as usize != offset => interpolate(text, stop, next, offset),
            _ => stop.x,
        }
    }

    /// The caret offset nearest to the point (`x`, `y`), logical pixels from the layout's
    /// top-left corner (ADR-0047 TF4): where a click puts the caret. Always a grapheme boundary.
    ///
    /// The line is the one whose band contains `y` (above the first line: the first; below the
    /// last: the last); in it, the nearest grapheme boundary, with ties going to the earlier one.
    /// On every line but the last, the line's end is left out: it is also the next line's start,
    /// where [`TextLayout::caret_x`] would draw it, so a click past the end of a wrapped line puts
    /// the caret before its last grapheme (a precise "end of this line" needs a caret *affinity*,
    /// left for the multi-line field, TF2).
    ///
    /// # Why the boundaries and not the stops
    /// The shaper groups characters into clusters by its own approximation of graphemes, which
    /// can differ from UAX #29 (the two halves of a flag may be two clusters): the caret must
    /// only stand where the text field's offsets can be, so the candidates are the UAX #29
    /// boundaries, each placed with the stops.
    ///
    /// # Complexity
    /// One walk of the text's grapheme boundaries, advancing through the line's stops alongside
    /// (both are sorted): O(n + stops). A boundary strictly inside a cluster (a ligature, rare)
    /// costs one more O(n) walk. No allocation.
    #[must_use]
    pub fn hit_test(&self, text: &str, x: f32, y: f32) -> usize {
        let line_height = self.metrics.line_height();
        let last = self.lines.len().saturating_sub(1);
        let row = if line_height > 0.0 && y > 0.0 {
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "a positive quotient, floored, then clamped to the line count"
            )]
            let row = (y / line_height).floor() as usize;
            row.min(last)
        } else {
            0
        };
        let Some(line) = self.lines.get(row) else {
            return 0;
        };
        let stops = self.carets.get(line.carets.clone()).unwrap_or_default();
        // The line's end is a candidate only on the last line (see above).
        let end = if row == last {
            line.text.end
        } else {
            line.text.end.saturating_sub(1)
        };
        let mut best: Option<(f32, usize)> = None;
        let mut index = 0;
        for boundary in grapheme_boundaries(text)
            .skip_while(|&b| b < line.text.start)
            .take_while(|&b| b <= end)
        {
            // The cluster holding this boundary: the last stop at or before it.
            while stops
                .get(index + 1)
                .is_some_and(|next| next.offset as usize <= boundary)
            {
                index += 1;
            }
            let Some(&stop) = stops.get(index) else {
                break;
            };
            let place = match stops.get(index + 1) {
                Some(&next) if stop.offset as usize != boundary => {
                    interpolate(text, stop, next, boundary)
                }
                _ => stop.x,
            };
            let distance = (place - x).abs();
            if best.is_none_or(|(nearest, _)| distance < nearest) {
                best = Some((distance, boundary));
            }
        }
        best.map_or(line.text.start, |(_, offset)| offset)
    }

    /// Computes [`TextLayout::bounds`] from the glyphs and the face's box, in one pass.
    pub(crate) fn compute_bounds(&mut self, face_box: GlyphBox) {
        // The extremes of the glyph origins: O(glyphs), no allocation.
        self.bounds = self
            .glyphs
            .iter()
            .fold(None, |extent, glyph| {
                let [x0, y0, x1, y1] = extent.unwrap_or([glyph.x, glyph.y, glyph.x, glyph.y]);
                Some([
                    x0.min(glyph.x),
                    y0.min(glyph.y),
                    x1.max(glyph.x),
                    y1.max(glyph.y),
                ])
            })
            .map(|[x0, y0, x1, y1]| GlyphBox {
                left: x0 + face_box.left,
                top: y0 + face_box.top,
                right: x1 + face_box.right,
                bottom: y1 + face_box.bottom,
            });
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
        self.carets.clear();
        self.width = 0.0;
        self.bounds = None;
    }
}

/// The x of the grapheme boundary `offset` inside the cluster that starts at `stop` and ends at
/// `next`: the cluster's advance split evenly among the graphemes it holds (see
/// [`TextLayout::caret_x`]). O(n) in the text's bytes, no allocation.
fn interpolate(text: &str, stop: CaretStop, next: CaretStop, offset: usize) -> f32 {
    let (start, end) = (stop.offset as usize, next.offset as usize);
    // The boundaries strictly inside the cluster split it into `inside + 1` graphemes; `before`
    // of them end at or before `offset`.
    let (mut inside, mut before) = (0_u32, 0_u32);
    for boundary in grapheme_boundaries(text).filter(|&b| b > start && b < end) {
        inside += 1;
        if boundary <= offset {
            before += 1;
        }
    }
    #[allow(
        clippy::cast_precision_loss,
        reason = "grapheme counts of one cluster are tiny, exact in f32"
    )]
    let fraction = before as f32 / (inside + 1) as f32;
    stop.x + (next.x - stop.x) * fraction
}
