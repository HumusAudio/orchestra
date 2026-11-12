// SPDX-License-Identifier: GPL-3.0-or-later

//! Shaping (ADR-0044 TX9): from a string and a style to glyphs with advances, plus the line
//! break opportunities of the string (TX10); `wrap` then turns both into lines.
//!
//! # Why a `Shaper` object
//! Shaping needs working memory: `harfrust`'s buffer, the compiled shape plans, the list of
//! shaped glyphs, the list of break opportunities. Keeping them in one object that the caller
//! reuses means that, after the first texts, shaping a text of a size already seen allocates
//! nothing (TX9; tested in `tests/no_alloc.rs`). One `Shaper` per thread that lays out text
//! (the interface thread) is enough; it holds no reference to the faces, so it can live
//! anywhere.

use harfrust::{Buffer, Direction, Feature, ShapeOptions, ShapePlan, ShapePlanKey, ShaperFont};
use icu_segmenter::LineSegmenter;
use icu_segmenter::LineSegmenterBorrowed;
use icu_segmenter::options::LineBreakOptions;

use crate::wrap::{Break, Hyphen, ShapedGlyph, wrap};
use crate::{FaceId, Faces, GlyphId, TextError, TextLayout, TextStyle};

/// How many compiled shape plans the shaper keeps.
///
/// A plan depends on the face, the script and the features. The interface uses five faces, one
/// or two scripts (Latin; Greek or Cyrillic in a user's text) and two feature sets, so a few
/// dozen plans cover everything; 32 keeps the linear search short (each comparison is a handful
/// of field comparisons). When full, the oldest plan is replaced (round robin): compiling a plan
/// again costs microseconds, so a smarter policy would not pay for itself.
const MAX_PLANS: usize = 32;

/// A compiled plan and the face it was compiled for (a `ShapePlan` does not remember it).
struct CachedPlan {
    /// The face.
    face: FaceId,
    /// The plan.
    plan: ShapePlan,
}

/// Shapes and wraps text into [`TextLayout`]s, reusing its working memory (see the module
/// documentation).
pub struct Shaper {
    /// `harfrust`'s buffer: characters in, glyphs out, cleared and reused for every text.
    buffer: Buffer,
    /// The compiled plans (at most [`MAX_PLANS`]).
    plans: Vec<CachedPlan>,
    /// The plan to replace next when `plans` is full.
    next_plan: usize,
    /// The feature list for tabular figures (`tnum` on the whole text), built once.
    tabular: [Feature; 1],
    /// The UAX #14 line segmenter, with its rules compiled into the binary (TX10).
    segmenter: LineSegmenterBorrowed<'static>,
    /// Working memory: the shaped glyphs of the current text.
    shaped: Vec<ShapedGlyph>,
    /// Working memory: the break opportunities of the current text.
    breaks: Vec<Break>,
}

impl std::fmt::Debug for Shaper {
    /// `harfrust`'s buffer and plans have no `Debug`; the sizes are what a test failure needs.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Shaper")
            .field("plans", &self.plans.len())
            .field("shaped_capacity", &self.shaped.capacity())
            .field("breaks_capacity", &self.breaks.capacity())
            .finish_non_exhaustive()
    }
}

impl Default for Shaper {
    fn default() -> Self {
        Self::new()
    }
}

impl Shaper {
    /// A shaper with empty working memory.
    #[must_use]
    pub fn new() -> Self {
        Self {
            buffer: Buffer::new(),
            plans: Vec::new(),
            next_plan: 0,
            tabular: [Feature::new(harfrust::Tag::new(b"tnum"), 1, ..)],
            // Without the dictionaries and the LSTM model (TX10): Thai, Lao, Khmer and Burmese
            // break only where UAX #14's rules alone allow it. Default options: strict
            // line-breaking, as for any text whose language is not known.
            segmenter: LineSegmenter::new_for_non_complex_scripts(LineBreakOptions::default()),
            shaped: Vec::new(),
            breaks: Vec::new(),
        }
    }

    /// Shapes `text` with `style` and wraps it at `max_width` logical pixels (or only at the
    /// mandatory line breaks, such as `\n`, when `max_width` is `None`), into `layout`, which is
    /// cleared first and reuses its capacity.
    ///
    /// # What happens
    /// 1. The whole text is shaped once with `harfrust`, as one run in one face, left to right
    ///    (TX5, TX9): kerning, ligatures, marks, Fluent's isolation marks hidden.
    /// 2. UAX #14 gives the break opportunities (TX10).
    /// 3. The lines are filled greedily (`wrap`): spaces at the end hang, soft hyphens become
    ///    visible only where a line breaks, a word longer than the width is cut between
    ///    clusters.
    ///
    /// # Complexity
    /// Shaping is roughly linear in the text's length (with the face's lookups); the break
    /// opportunities are linear; wrapping is linear but for the backward walk over trailing
    /// spaces at each candidate break, which visits each space a bounded number of times.
    ///
    /// # Errors
    /// [`TextError::InvalidSize`], [`TextError::InvalidWidth`], [`TextError::TextTooLong`],
    /// [`TextError::Shaping`].
    pub fn layout(
        &mut self,
        faces: &Faces,
        text: &str,
        style: TextStyle,
        max_width: Option<f32>,
        layout: &mut TextLayout,
    ) -> Result<(), TextError> {
        // --- Check the inputs before touching anything ------------------------------------
        let scale = crate::style::checked_scale(faces, style.face, style.size)?;
        if let Some(width) = max_width
            && (!width.is_finite() || width < 0.0)
        {
            return Err(TextError::InvalidWidth);
        }
        // Clusters are `u32` byte offsets: a longer text could not be mapped back.
        if u32::try_from(text.len()).is_err() {
            return Err(TextError::TextTooLong);
        }
        let metrics = faces.line_metrics(style.face, style.size)?;
        let face_box = faces.bounds(style.face, style.size)?;

        // --- 1. Shape --------------------------------------------------------------------
        self.shape(faces, text, style, scale)?;

        // --- 2. Break opportunities ------------------------------------------------------
        self.find_breaks(text);

        // --- 3. Lines --------------------------------------------------------------------
        // The hyphen for soft hyphens is looked up only when the text has one: most texts do
        // not, and the lookup is a `cmap` search plus a metrics read.
        let hyphen = if text.contains('\u{AD}') {
            hyphen(faces, style.face, scale)
        } else {
            None
        };
        layout.clear();
        layout.face = style.face;
        layout.size = style.size;
        layout.metrics = metrics;
        wrap(
            text.len(),
            &self.shaped,
            &self.breaks,
            max_width,
            hyphen,
            layout,
        );
        layout.compute_bounds(face_box);
        Ok(())
    }

    /// Shapes `text` into `self.shaped`, with advances and offsets already in logical pixels.
    fn shape(
        &mut self,
        faces: &Faces,
        text: &str,
        style: TextStyle,
        scale: f32,
    ) -> Result<(), TextError> {
        let font = faces.font(style.face);
        let features: &[Feature] = if style.features.tabular_figures {
            &self.tabular
        } else {
            &[]
        };

        // Fill the buffer. The direction is set BEFORE guessing the other properties, because
        // `guess_segment_properties` only fills what is unset: a Hebrew text would otherwise
        // turn the buffer right to left, and this layout is left to right only (TX5).
        self.buffer.clear();
        self.buffer.push_str(text);
        self.buffer.set_direction(Direction::LeftToRight);
        self.buffer.guess_segment_properties();
        let script = self.buffer.script();

        // Find a compiled plan for (face, script, features), or compile one. The key compares
        // everything a plan depends on, as `harfrust` defines it.
        let key = ShapePlanKey::new(font, script, Direction::LeftToRight).features(features);
        let index = if let Some(index) = self
            .plans
            .iter()
            .position(|cached| cached.face == style.face && key.matches(&cached.plan))
        {
            index
        } else {
            let plan = ShapePlan::new(font, Direction::LeftToRight, script, None, features);
            let cached = CachedPlan {
                face: style.face,
                plan,
            };
            if self.plans.len() < MAX_PLANS {
                self.plans.push(cached);
                self.plans.len() - 1
            } else {
                let index = self.next_plan;
                if let Some(slot) = self.plans.get_mut(index) {
                    *slot = cached;
                }
                self.next_plan = (index + 1) % MAX_PLANS;
                index
            }
        };
        let plan = self.plans.get(index).map(|cached| &cached.plan);

        // Shape. `ShaperFont::new` is cheap: it borrows the font's layout tables, which the
        // font caches after the first use.
        let shaper_font = ShaperFont::new(font);
        harfrust::shape(
            &shaper_font,
            &mut self.buffer,
            ShapeOptions::new().plan(plan).features(features),
        )
        .map_err(|_| TextError::Shaping)?;

        // Copy the result out in our own form, in pixels.
        self.shaped.clear();
        for (info, position) in self
            .buffer
            .glyph_infos()
            .iter()
            .zip(self.buffer.glyph_positions())
        {
            // `cluster` is a byte offset of `text`: the text is shorter than 4 GiB (checked by
            // the caller), and `u32` always fits in `usize` on the platforms we build for.
            let cluster = info.cluster as usize;
            // The first character of the cluster decides whether the glyph is a space (it hangs
            // at the end of a line) or a control character (not drawn, no advance).
            let first = text.get(cluster..).and_then(|rest| rest.chars().next());
            let hidden = first.is_some_and(is_control);
            let space = first.is_some_and(char::is_whitespace);
            self.shaped.push(ShapedGlyph {
                glyph: GlyphId(info.glyph_id),
                cluster,
                advance: if hidden {
                    0.0
                } else {
                    units(position.x_advance) * scale
                },
                x_offset: units(position.x_offset) * scale,
                // Font offsets are y up; ours are y down.
                y_offset: -units(position.y_offset) * scale,
                hidden,
                space,
            });
        }
        Ok(())
    }

    /// Collects the break opportunities of `text` into `self.breaks`.
    fn find_breaks(&mut self, text: &str) {
        self.breaks.clear();
        for position in self.segmenter.segment_str(text) {
            // UAX #14 always reports the start of the text, where no line can break.
            if position == 0 {
                continue;
            }
            let before = text.get(..position).unwrap_or_default();
            self.breaks.push(Break {
                position,
                // `icu_segmenter` does not say which breaks are mandatory in its stable API
                // (`is_mandatory` needs its `unstable` feature): UAX #14 LB4–LB5 put them right
                // after a hard line break character, so the character before decides.
                mandatory: before.chars().next_back().is_some_and(is_hard_break),
                // A break right after U+00AD: the hyphen becomes visible if the line breaks
                // here (TB6).
                soft_hyphen: before.ends_with('\u{AD}'),
            });
        }
    }
}

/// The glyph and advance of the hyphen drawn at a soft hyphen (TB6): U+2010 HYPHEN if the face
/// has it, otherwise U+002D HYPHEN-MINUS; `None` if neither exists (then nothing is added).
fn hyphen(faces: &Faces, face: FaceId, scale: f32) -> Option<Hyphen> {
    let glyph = faces
        .glyph_for_char(face, '\u{2010}')
        .or_else(|| faces.glyph_for_char(face, '-'))?;
    let advance = faces
        .font(face)
        .glyph_metrics()
        .h_advance(skrifa::GlyphId::new(glyph.0));
    Some(Hyphen {
        glyph,
        advance: advance * scale,
    })
}

/// `true` for characters that are never drawn: control characters (tab, line feed, carriage
/// return…) and the Unicode line and paragraph separators. Fonts usually have no glyph for
/// them, so drawing them would show `.notdef` boxes; they get no advance either (there are no
/// tab stops in interface text).
const fn is_control(c: char) -> bool {
    c.is_control() || matches!(c, '\u{2028}' | '\u{2029}')
}

/// `true` for the characters after which UAX #14 requires a line break (classes BK, CR, LF,
/// NL): line feed, vertical tab, form feed, carriage return, next line, line separator and
/// paragraph separator. (UAX #14 never breaks between CR and LF, so a CR followed by LF gives
/// one break, after the LF.)
const fn is_hard_break(c: char) -> bool {
    matches!(
        c,
        '\n' | '\u{0B}' | '\u{0C}' | '\r' | '\u{85}' | '\u{2028}' | '\u{2029}'
    )
}

/// A shaper value in font units as `f32`.
#[allow(
    clippy::cast_precision_loss,
    reason = "shaper values are font units, far below 2^24 where f32 is exact"
)]
const fn units(value: i32) -> f32 {
    value as f32
}
