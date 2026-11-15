// SPDX-License-Identifier: GPL-3.0-or-later

//! Eliding (step F2.09f; precision of ADR-0044 and ADR-0045 WJ1–WJ7): a one-line text that does
//! not fit a width is shortened to its start followed by an ellipsis, "Reverb send le…".
//!
//! # The rule
//! - The text is cut **at its end**, on a **grapheme boundary** (UAX #29: never inside a letter
//!   with its accents, an emoji sequence or a flag), and the white space left at the end of the
//!   kept part is dropped, so the ellipsis touches the last visible character ("Reverb…", never
//!   "Reverb …").
//! - The kept part is the **longest** one that fits: adding the next grapheme would not.
//! - When not even one grapheme fits before the ellipsis, the ellipsis stands alone; when the
//!   ellipsis itself does not fit, nothing is drawn. A text therefore never draws wider than the
//!   width it was given.
//!
//! # Why the shortened text is shaped again
//! Cutting the glyphs of the full layout and appending the ellipsis glyph would be cheaper, but
//! wrong at the cut: a ligature ("fi" drawn as one glyph) cannot be cut in half, kerning between
//! the last letter and what followed no longer applies, and kerning between the last letter and
//! the ellipsis is missing. Shaping "start + ellipsis" as one run gives exactly what the font
//! intends (rejected alternative, WJ2).
//!
//! # Limits
//! Left to right only, like the whole crate (ADR-0044 TX5): the ellipsis always follows the
//! start of the text. A text with a mandatory line break (several lines without any wrapping
//! width) is left whole: eliding is for one-line texts.

use crate::boundary::{grapheme_boundaries, next_grapheme, prev_grapheme};
use crate::{FaceId, Faces, Shaper, TextError, TextLayout, TextStyle};

/// The ellipsis as one character, U+2026 HORIZONTAL ELLIPSIS (WJ3): the typographic sign, drawn
/// by the face as one glyph with its own spacing. A typographic constant, the same in every
/// language of the interface, not a translatable text.
const ELLIPSIS: &str = "\u{2026}";

/// Three full stops, used only when the face has no U+2026 glyph. None of the interface faces
/// needs it (Inter and JetBrains Mono both have the ellipsis); it exists so that a face which
/// has the full stop but not the ellipsis still shows three dots rather than a `.notdef` box.
const ELLIPSIS_FALLBACK: &str = "...";

/// What [`Shaper::elide`] did with a text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Elision {
    /// The whole text fits (or it has several lines): draw the full layout. The output layout
    /// was not touched.
    Whole,
    /// The text was cut: the output layout holds the first `kept` bytes of the text followed by
    /// the ellipsis. `kept` is 0 when the ellipsis stands alone.
    Cut {
        /// How many bytes of the text come before the ellipsis (a character boundary; trailing
        /// white space already dropped).
        kept: usize,
    },
    /// Not even the ellipsis fits: the output layout is the layout of an empty text (one empty
    /// line, so the height is still a line's).
    Hidden,
}

impl Shaper {
    /// Shortens the one-line `text` to `max_width` logical pixels, into `out` (see the module
    /// documentation for the rule).
    ///
    /// `full` must be the layout of the same `text` in the same `style` without a wrapping width
    /// ([`Shaper::layout`] with `None`): the caller, a widget, already has it because it measured
    /// its text with it, and its caret stops give the first guess of where to cut without shaping
    /// anything.
    ///
    /// The clusters and caret stops of `out` refer to the **shortened** text, not to `text`.
    ///
    /// # How the cut is found
    /// 1. If `full` fits, nothing is done ([`Elision::Whole`]).
    /// 2. The first guess is the last grapheme boundary whose pen position in `full`, plus the
    ///    ellipsis glyph's advance, still fits. It is almost always right: it only ignores what
    ///    changes at the cut (kerning with the ellipsis, a ligature broken by the cut, the white
    ///    space dropped).
    /// 3. The guess is shaped with its ellipsis and measured. If it fits, the next boundaries are
    ///    tried while they fit too; if it does not, the previous ones until one does. The usual
    ///    cost is therefore **two** shapings of a text shorter than the original: the guess and
    ///    the one-grapheme-longer candidate that proves it is the longest.
    ///
    /// # Complexity
    /// O(n) in the text's bytes for the guess (one walk of the grapheme boundaries up to the
    /// cut, each placed by a binary search on the caret stops), plus one shaping, itself roughly
    /// linear, per candidate tried: two in the usual case, at most one per grapheme in a
    /// pathological one (a guess far from the truth, which needs a font whose kerning with the
    /// ellipsis is wider than whole letters). No allocation once the shaper's working memory and
    /// `out` have grown to the text's size (tested).
    ///
    /// # Errors
    /// [`TextError::InvalidWidth`] if `max_width` is NaN or negative; otherwise as
    /// [`Shaper::layout`].
    pub fn elide(
        &mut self,
        faces: &Faces,
        text: &str,
        style: TextStyle,
        max_width: f32,
        full: &TextLayout,
        out: &mut TextLayout,
    ) -> Result<Elision, TextError> {
        // NaN fails both comparisons, so it is named explicitly. An infinite width is allowed: it
        // means "no limit", and the full text then fits by definition.
        if max_width.is_nan() || max_width < 0.0 {
            return Err(TextError::InvalidWidth);
        }
        let scale = crate::style::checked_scale(faces, style.face, style.size)?;

        // --- 1. Nothing to do ---------------------------------------------------------------
        // Several lines mean mandatory breaks in a text laid out without a width: not a
        // one-line text (see the module's limits).
        if full.width() <= max_width || full.lines().len() > 1 {
            return Ok(Elision::Whole);
        }

        // --- 2. The first guess -------------------------------------------------------------
        let (ellipsis, ellipsis_advance) = ellipsis(faces, style.face, scale);
        // What is left for the start of the text once the ellipsis has its place. Negative when
        // the ellipsis alone is too wide: no boundary passes the test below, the guess stays 0.
        let budget = max_width - ellipsis_advance;
        let mut cut = 0;
        // The pen positions grow with the offset (left to right, non-negative advances), so the
        // walk stops at the first boundary that is too far. The text's end is not a candidate:
        // the whole text is already known not to fit.
        for boundary in grapheme_boundaries(text).take_while(|&boundary| boundary < text.len()) {
            if full.caret_x(text, boundary) > budget {
                break;
            }
            cut = boundary;
        }

        // --- 3. Check the guess, then lengthen or shorten it --------------------------------
        self.lay_out_cut(faces, text, style, cut, ellipsis, out)?;
        if out.width() <= max_width {
            // The guess fits: try one grapheme more, as long as that fits too. The candidate is
            // laid out into the scratch layout, so `out` keeps the best one found so far; when
            // the candidate fits, the two are swapped (no copy, no allocation).
            let mut scratch = std::mem::take(&mut self.scratch);
            let outcome = loop {
                let next = next_grapheme(text, cut);
                if next >= text.len() {
                    break Ok(());
                }
                if let Err(error) =
                    self.lay_out_cut(faces, text, style, next, ellipsis, &mut scratch)
                {
                    break Err(error);
                }
                if scratch.width() > max_width {
                    break Ok(());
                }
                std::mem::swap(out, &mut scratch);
                cut = next;
            };
            // Give the working memory back before reporting an error, so its capacity is kept.
            self.scratch = scratch;
            outcome?;
        } else {
            // The guess is too wide: step back one grapheme at a time until something fits.
            loop {
                if cut == 0 {
                    // Even the ellipsis alone is too wide (WJ5): draw nothing. The layout of an
                    // empty text has one empty line, so the caller's height does not change.
                    self.layout(faces, "", style, None, out)?;
                    return Ok(Elision::Hidden);
                }
                cut = prev_grapheme(text, cut);
                self.lay_out_cut(faces, text, style, cut, ellipsis, out)?;
                if out.width() <= max_width {
                    break;
                }
            }
        }
        Ok(Elision::Cut {
            kept: kept_part(text, cut).len(),
        })
    }

    /// Lays out the first `cut` bytes of `text`, without their trailing white space, followed by
    /// `ellipsis`, as one line into `out`.
    ///
    /// The shortened text is built in the shaper's own string, taken out for the duration of the
    /// call because [`Shaper::layout`] borrows the whole shaper; taking and putting back a
    /// `String` moves three words and allocates nothing.
    fn lay_out_cut(
        &mut self,
        faces: &Faces,
        text: &str,
        style: TextStyle,
        cut: usize,
        ellipsis: &str,
        out: &mut TextLayout,
    ) -> Result<(), TextError> {
        let mut shortened = std::mem::take(&mut self.elided);
        shortened.clear();
        shortened.push_str(kept_part(text, cut));
        shortened.push_str(ellipsis);
        let result = self.layout(faces, &shortened, style, None, out);
        self.elided = shortened;
        result
    }
}

/// The part of `text` drawn before the ellipsis for a cut at byte `cut` (a grapheme boundary):
/// the first `cut` bytes without the white space at their end (WJ4).
///
/// `cut` always comes from the grapheme boundaries of `text`, so it is a character boundary; if
/// it ever were not, `get` answers `None` and nothing is kept, rather than panicking.
fn kept_part(text: &str, cut: usize) -> &str {
    text.get(..cut).unwrap_or_default().trim_end()
}

/// The ellipsis to append in `face` and its advance in logical pixels (`scale` turns the face's
/// units into pixels): U+2026 when the face has it, three full stops otherwise.
///
/// The advance is the glyph's own, without the kerning the shaper may add next to a letter: it
/// only serves the first guess of [`Shaper::elide`], which is then checked by shaping.
fn ellipsis(faces: &Faces, face: FaceId, scale: f32) -> (&'static str, f32) {
    let advance_of = |c: char| {
        faces.glyph_for_char(face, c).map(|glyph| {
            faces
                .font(face)
                .glyph_metrics()
                .h_advance(skrifa::GlyphId::new(glyph.0))
                * scale
        })
    };
    advance_of('\u{2026}').map_or_else(
        // No ellipsis glyph: three full stops, three advances (zero if the face has no full stop
        // either, a symbol font; the check by shaping still decides).
        || (ELLIPSIS_FALLBACK, 3.0 * advance_of('.').unwrap_or(0.0)),
        |advance| (ELLIPSIS, advance),
    )
}
