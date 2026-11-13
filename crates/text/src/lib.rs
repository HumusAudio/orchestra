// SPDX-License-Identifier: GPL-3.0-or-later

//! # orchestra-text — bundled fonts, shaping, line breaking, measuring, outlines
//!
//! The crate every other crate goes through to put letters and musical symbols on the screen
//! (ADR-0044). It sits at the bottom of the workspace with no internal dependency, so that both
//! the interface (`orchestra-ui`) and the engraving (`orchestra-notation-layout`, phase F2B) can
//! use the same fonts and the same measurements.
//!
//! ## What it does
//! - Holds the **bundled faces** ([`FaceId`], [`Faces`]): Inter Regular, SemiBold and Bold,
//!   Bravura and JetBrains Mono, embedded in the binary (ADR-0044 TX3).
//! - **Shapes** text with `harfrust` (kerning, ligatures, marks) and **wraps** it into lines at
//!   the break opportunities of the Unicode line breaking algorithm, UAX #14, from
//!   `icu_segmenter` ([`Shaper`], [`TextLayout`]; TX9, TX10).
//! - **Measures**: line metrics, face bounds, single advances ([`LineMetrics`],
//!   [`GlyphBox`], [`Faces::advance`]).
//! - Delivers **glyph outlines** to an [`OutlineSink`], unhinted, y down (TX2, TX6).
//! - Finds a **SMuFL glyph** by its code point without shaping ([`Faces::glyph_for_char`]).
//! - Finds **grapheme and word boundaries** for the caret and the selection of the text field
//!   ([`next_grapheme`], [`prev_word_start`], [`word_at`]…; ADR-0047 TF1), and the caret's
//!   place in a layout ([`TextLayout::caret_x`], [`TextLayout::hit_test`]).
//!
//! ```
//! use orchestra_text::{FaceId, Faces, Shaper, TextLayout, TextStyle};
//!
//! let faces = Faces::bundled()?;
//! let mut shaper = Shaper::new();
//! let mut layout = TextLayout::new();
//! let style = TextStyle::new(FaceId::InterRegular, 13.0);
//! shaper.layout(&faces, "Quarter note · Crotchet · Viertelnote", style, Some(120.0), &mut layout)?;
//! assert!(layout.lines().len() > 1);
//! assert!(layout.width() <= 120.0);
//! # Ok::<(), orchestra_text::TextError>(())
//! ```
//!
//! ## What it must NOT do
//! - Rasterise or cache glyph images: that is the renderer's job in `orchestra-ui`, the only
//!   crate that knows `tiny-skia` (ADR-0044 TX2, TX8).
//! - Know about theme tokens: the text tokens (TX11) resolve to a [`TextStyle`] in the theme.
//! - Read fonts from the system (ADR-0006): only the bundled faces exist.
//! - Run on the audio thread: shaping allocates on its first texts and has no time bound.
//!
//! ## Recorded limits (ADR-0044)
//! No fallback font: a character the face lacks is drawn as `.notdef` (TX4). Left to right
//! only (TX5). Control characters, tab included, are neither drawn nor advancing.
//!
//! ## Allowed dependencies
//! None inside the workspace. External: `skrifa`, `harfrust` (ADR-0036 B1, ADR-0044 TX1),
//! `icu_segmenter` (ADR-0044 TX10).

#![forbid(unsafe_code)]

mod boundary;
mod error;
mod face;
mod layout;
mod metrics;
mod outline;
mod shape;
mod style;
mod wrap;

pub use boundary::{
    ceil_grapheme, floor_grapheme, grapheme_boundaries, is_grapheme_boundary, next_grapheme,
    next_word_end, next_word_start, prev_grapheme, prev_word_start, word_at,
};
pub use error::TextError;
pub use face::{FaceId, Faces};
pub use layout::{CaretStop, GlyphId, Line, PositionedGlyph, TextLayout};
pub use metrics::{GlyphBox, LineMetrics};
pub use outline::OutlineSink;
pub use shape::Shaper;
pub use style::{Features, TextStyle};
