// SPDX-License-Identifier: GPL-3.0-or-later

//! Filling lines (ADR-0044 TX10): from shaped glyphs and break opportunities to the lines of a
//! [`TextLayout`].
//!
//! # The algorithm: greedy, first fit
//! Walk the glyphs left to right, summing their advances. At every break opportunity (a byte
//! offset given by UAX #14, which lands on a cluster boundary) the line so far is a
//! **candidate**: its content width is the sum minus the spaces at its end (they hang) plus a
//! hyphen if the break is at a soft hyphen. When a candidate no longer fits, the line is cut at
//! the previous candidate and the walk goes on from there. If there is no previous candidate
//! (one word wider than the line), the word is cut at the last cluster that fits (the last
//! resort of TX10). A mandatory break (after `\n`) ends the line whatever its width.
//!
//! This is what browsers and word processors do for interface text. An optimal-fit algorithm
//! (Knuth–Plass, which balances the lines of a paragraph) is meant for justified book text and
//! costs O(n²) in the worst case; dialogs and labels do not need it.
//!
//! # Why the shaping is done once
//! The whole paragraph is shaped before breaking, and each line takes its glyphs as they are.
//! Shaping each line again after breaking would be exact even across the break (kerning between
//! the last letter and a hyphen, or contextual forms), but it would double the cost; at a space
//! or a hyphen, where lines break, fonts have no kerning or ligatures across, so the glyphs do
//! not change (ADR-0044 TX10, TB6).
//!
//! # Complexity
//! O(n) in the glyphs, plus the backward walk over trailing spaces at each candidate (bounded
//! by the spaces between two words) and the emergency cut, which visits each glyph of an
//! over-long word once per line it produces.

use crate::{GlyphId, Line, LineMetrics, PositionedGlyph, TextLayout};

/// A glyph as the shaper produced it, in logical pixels.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ShapedGlyph {
    /// The glyph.
    pub(crate) glyph: GlyphId,
    /// Byte offset of its cluster in the text.
    pub(crate) cluster: usize,
    /// Its advance, logical pixels (0 for hidden glyphs).
    pub(crate) advance: f32,
    /// Its horizontal offset from the pen, logical pixels.
    pub(crate) x_offset: f32,
    /// Its vertical offset from the baseline, logical pixels, y down.
    pub(crate) y_offset: f32,
    /// A control character: neither drawn nor advancing.
    pub(crate) hidden: bool,
    /// White space: hangs past the width at the end of a line.
    pub(crate) space: bool,
}

/// A break opportunity: a line may start at `position`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Break {
    /// Byte offset in the text (never 0).
    pub(crate) position: usize,
    /// The line must break here (after a hard line break character).
    pub(crate) mandatory: bool,
    /// The character before is a soft hyphen (U+00AD).
    pub(crate) soft_hyphen: bool,
}

/// The visible hyphen added at a soft hyphen where a line breaks (TB6).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Hyphen {
    /// Its glyph in the layout's face.
    pub(crate) glyph: GlyphId,
    /// Its advance, logical pixels.
    pub(crate) advance: f32,
}

/// A place where the current line could end.
#[derive(Debug, Clone, Copy)]
struct Candidate {
    /// The glyph the next line would start with.
    glyph: usize,
    /// Its byte offset in the text.
    byte: usize,
    /// The line's content width if it ended here (trailing spaces off, hyphen on).
    content: f32,
    /// The sum of the advances of the line's glyphs up to here, trailing spaces included: what
    /// the next line does not carry.
    consumed: f32,
    /// The break is at a soft hyphen.
    hyphen: bool,
}

/// The state of the walk: where the current line starts and what it holds so far.
struct Walk<'a> {
    /// The shaped glyphs of the whole text.
    shaped: &'a [ShapedGlyph],
    /// The hyphen for soft hyphens, if the face has one.
    hyphen: Option<Hyphen>,
    /// The line metrics, to place the baselines.
    metrics: LineMetrics,
    /// The output.
    layout: &'a mut TextLayout,
    /// The first glyph of the current line.
    start_glyph: usize,
    /// The first byte of the current line.
    start_byte: usize,
    /// The sum of the advances from `start_glyph` to the glyph being looked at.
    width: f32,
}

/// Fills `layout`'s lines from the shaped glyphs (see the module documentation). `layout` must
/// have been cleared, with its face, size and metrics set.
pub(crate) fn wrap(
    text_len: usize,
    shaped: &[ShapedGlyph],
    breaks: &[Break],
    max_width: Option<f32>,
    hyphen: Option<Hyphen>,
    layout: &mut TextLayout,
) {
    let metrics = layout.metrics;
    let mut walk = Walk {
        shaped,
        hyphen,
        metrics,
        layout,
        start_glyph: 0,
        start_byte: 0,
        width: 0.0,
    };
    // Without a width every line fits: only mandatory breaks end lines.
    let fits = |content: f32| max_width.is_none_or(|max| content <= max);
    let mut candidate: Option<Candidate> = None;
    let mut next_break = breaks.iter().peekable();
    let count = shaped.len();
    // The text ends with a hard line break (e.g. "abc\n"): an empty last line follows it, where
    // a caret would stand after the newline.
    let mut ends_with_hard_break = false;

    for index in 0..=count {
        // The byte offset where glyph `index` starts (the end of the text after the last one).
        let byte = shaped.get(index).map_or(text_len, |glyph| glyph.cluster);
        // A break may only fall where a new cluster begins: between the glyphs of one cluster
        // (a letter and its mark) there is no place to cut.
        let at_boundary = index == 0
            || index == count
            || shaped
                .get(index.wrapping_sub(1))
                .is_none_or(|previous| previous.cluster != byte);

        // Take every break opportunity up to this byte; only one exactly here is usable.
        while let Some(found) = next_break.next_if(|found| found.position <= byte) {
            if found.position != byte || !at_boundary || found.position <= walk.start_byte {
                continue;
            }
            if index == count {
                // The end of the text: the last line is settled after the loop; remember a
                // final hard break for the empty line it implies.
                ends_with_hard_break = found.mandatory;
                break;
            }
            // Settle this opportunity: as long as the line does not fit, cut it earlier.
            loop {
                let hyphen_width = if found.soft_hyphen {
                    walk.hyphen.map_or(0.0, |hyphen| hyphen.advance)
                } else {
                    0.0
                };
                let content = walk.width - walk.trailing_spaces(index) + hyphen_width;
                if fits(content) || walk.single_cluster(index) {
                    if found.mandatory {
                        // The line ends here whatever its width; no hyphen at a hard break.
                        let content = walk.width - walk.trailing_spaces(index);
                        walk.emit(index, byte, content, false);
                        walk.restart(index, byte, walk.width);
                        candidate = None;
                    } else {
                        candidate = Some(Candidate {
                            glyph: index,
                            byte,
                            content,
                            consumed: walk.width,
                            hyphen: found.soft_hyphen && walk.hyphen.is_some(),
                        });
                    }
                    break;
                }
                // Too wide. Cut at the previous candidate, if there is one, and try again.
                if let Some(previous) = candidate.take() {
                    walk.emit(
                        previous.glyph,
                        previous.byte,
                        previous.content,
                        previous.hyphen,
                    );
                    walk.restart(previous.glyph, previous.byte, previous.consumed);
                    continue;
                }
                // No earlier opportunity: one word wider than the line. Cut it between clusters
                // (the last resort of TX10) and try again with the rest.
                // Either way the loop runs again: after a cut, the rest is settled as a new
                // line; when nothing could be cut, the line holds a single cluster, and
                // `single_cluster` makes the next round accept it (so the loop always ends).
                walk.emergency_cut(index, max_width.unwrap_or(f32::INFINITY));
            }
        }

        let Some(glyph) = shaped.get(index) else {
            break;
        };
        walk.width += glyph.advance;
    }

    // The last line: the rest of the text, cut as long as it does not fit, like any line.
    loop {
        let content = walk.width - walk.trailing_spaces(count);
        if fits(content) || walk.single_cluster(count) {
            walk.emit(count, text_len, content, false);
            break;
        }
        if let Some(previous) = candidate.take() {
            walk.emit(
                previous.glyph,
                previous.byte,
                previous.content,
                previous.hyphen,
            );
            walk.restart(previous.glyph, previous.byte, previous.consumed);
            continue;
        }
        if !walk.emergency_cut(count, max_width.unwrap_or(f32::INFINITY)) {
            walk.emit(count, text_len, content, false);
            break;
        }
    }
    if ends_with_hard_break {
        walk.restart(count, text_len, walk.width);
        walk.emit(count, text_len, 0.0, false);
    }
}

impl Walk<'_> {
    /// The advance of the white space at the end of the current line, if it ended before glyph
    /// `end`: it hangs past the width and is not counted (TX10).
    fn trailing_spaces(&self, end: usize) -> f32 {
        self.spaces_before(self.start_glyph, end).1
    }

    /// How many glyphs of white space end the glyphs `from..end`, and their total advance.
    fn spaces_before(&self, from: usize, end: usize) -> (usize, f32) {
        self.shaped
            .get(from..end)
            .unwrap_or_default()
            .iter()
            .rev()
            .take_while(|glyph| glyph.space)
            .fold((0, 0.0), |(count, width), glyph| {
                (count + 1, width + glyph.advance)
            })
    }

    /// The glyphs of the current line's **content**, if it ended before glyph `end`: without
    /// the white space at its end, which hangs and can never be what makes a line too wide.
    fn content(&self, end: usize) -> &[ShapedGlyph] {
        let (spaces, _) = self.spaces_before(self.start_glyph, end);
        self.shaped
            .get(self.start_glyph..end.saturating_sub(spaces).max(self.start_glyph))
            .unwrap_or_default()
    }

    /// `true` if the content of the current line, ending before glyph `end`, holds at most
    /// one cluster: then it cannot be cut and is accepted even if too wide.
    fn single_cluster(&self, end: usize) -> bool {
        let glyphs = self.content(end);
        glyphs
            .first()
            .is_none_or(|first| glyphs.iter().all(|glyph| glyph.cluster == first.cluster))
    }

    /// Cuts the current line (which would end before glyph `end`) after the longest run of
    /// whole clusters of its content that fits in `max`, or after its first cluster if even
    /// that does not fit. Returns `false`, changing nothing, when no cut is possible.
    fn emergency_cut(&mut self, end: usize, max: f32) -> bool {
        let glyphs = self.content(end);
        // (index of the first glyph after the cut, width up to it)
        let mut best: Option<(usize, f32)> = None;
        let mut sum = 0.0;
        let mut position = 0;
        while let Some(first) = glyphs.get(position) {
            // One whole cluster: the glyphs sharing the first one's byte offset.
            let length = glyphs
                .get(position..)
                .unwrap_or_default()
                .iter()
                .take_while(|glyph| glyph.cluster == first.cluster)
                .count();
            let cluster = glyphs.get(position..position + length).unwrap_or_default();
            let cluster_width: f32 = cluster.iter().map(|glyph| glyph.advance).sum();
            // A cluster of white space (a no-break space inside the word) always fits: at the
            // end of the piece it hangs, like any space (found by the property test).
            let hangs = cluster.iter().all(|glyph| glyph.space);
            let after = position + length;
            if after >= glyphs.len() {
                // Cutting at the end of the line is not a cut.
                break;
            }
            if best.is_some() && !hangs && sum + cluster_width > max {
                break;
            }
            sum += cluster_width;
            best = Some((after, sum));
            position = after;
            if !hangs && sum > max {
                // The first cluster alone is too wide: it gets a line of its own.
                break;
            }
        }
        let Some((cut, consumed)) = best else {
            return false;
        };
        let glyph = self.start_glyph + cut;
        let byte = self
            .shaped
            .get(glyph)
            .map_or(self.start_byte, |glyph| glyph.cluster);
        // A no-break space inside the cut word may end the piece: it hangs like any space.
        let content = consumed - self.spaces_before(self.start_glyph, glyph).1;
        self.emit(glyph, byte, content, false);
        self.restart(glyph, byte, consumed);
        true
    }

    /// Starts a new line at glyph `glyph` (byte `byte`), after a line whose glyphs' advances
    /// summed to `consumed`.
    fn restart(&mut self, glyph: usize, byte: usize, consumed: f32) {
        self.start_glyph = glyph;
        self.start_byte = byte;
        self.width -= consumed;
        // Subtracting the same additions in a different order can leave a tiny residue; a
        // new line that starts at its last glyph has nothing in it.
        if glyph >= self.shaped.len() || self.width < 0.0 {
            self.width = self.width.max(0.0);
        }
    }

    /// Writes the current line, from its start to glyph `end` (byte `end_byte`), into the
    /// layout, with its content width and, if `hyphen`, the visible hyphen at its end.
    fn emit(&mut self, end: usize, end_byte: usize, content: f32, hyphen: bool) {
        // The baseline of line k is the ascent plus k line heights below the top.
        #[allow(
            clippy::cast_precision_loss,
            reason = "a line count is far below 2^24, where f32 is exact"
        )]
        let row = self.layout.lines.len() as f32;
        let baseline = self.metrics.ascent + row * self.metrics.line_height();
        let first_output = self.layout.glyphs.len();
        let mut pen = 0.0;
        for glyph in self.shaped.get(self.start_glyph..end).unwrap_or_default() {
            if !glyph.hidden {
                self.layout.glyphs.push(PositionedGlyph {
                    glyph: glyph.glyph,
                    x: pen + glyph.x_offset,
                    y: baseline + glyph.y_offset,
                    // Byte offsets fit in u32: the caller refused texts of 4 GiB or more.
                    cluster: u32::try_from(glyph.cluster).unwrap_or(u32::MAX),
                });
            }
            pen += glyph.advance;
        }
        if hyphen && let Some(added) = self.hyphen {
            // The soft hyphen is the last character of the line (two bytes in UTF-8), so its
            // cluster is two bytes before the line's end.
            self.layout.glyphs.push(PositionedGlyph {
                glyph: added.glyph,
                x: content - added.advance,
                y: baseline,
                cluster: u32::try_from(end_byte.saturating_sub(2)).unwrap_or(u32::MAX),
            });
        }
        self.layout.width = self.layout.width.max(content);
        self.layout.lines.push(Line {
            glyphs: first_output..self.layout.glyphs.len(),
            text: self.start_byte..end_byte,
            baseline,
            width: content,
            hyphenated: hyphen,
        });
    }
}
