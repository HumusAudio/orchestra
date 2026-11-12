// SPDX-License-Identifier: GPL-3.0-or-later

//! The crate's error type.
//!
//! # Why one enum
//! Every operation of the crate can fail for few, simple reasons (a bad number from the caller,
//! a font or a shaper refusing its input), and the interface will map each variant to a Fluent
//! key: one exhaustive enum (ADR-0034 D2) makes a forgotten variant a compile error there.
//! `Display` is English and meant for developers (logs, test failures), as in the other crates.

use crate::{FaceId, GlyphId};

/// Why a text operation failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextError {
    /// A bundled face could not be parsed. Cannot happen with the files in the repository (a
    /// test parses every face); kept as an error because library code never panics.
    FontUnreadable(FaceId),
    /// A font size was not a finite, positive number of pixels per em.
    InvalidSize,
    /// A maximum line width was negative, or not a finite number.
    InvalidWidth,
    /// The text is longer than 4 GiB: the shaper's clusters are 32-bit byte offsets.
    TextTooLong,
    /// The shaper refused the text (an internal limit of `harfrust`, such as its operation
    /// budget against malicious fonts, was reached).
    Shaping,
    /// The outline of a glyph could not be drawn (a corrupt glyph program in the font).
    Outline(FaceId, GlyphId),
}

impl std::fmt::Display for TextError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FontUnreadable(face) => write!(f, "the bundled face {face:?} cannot be read"),
            Self::InvalidSize => f.write_str("the font size must be a finite, positive number"),
            Self::InvalidWidth => {
                f.write_str("the maximum line width must be a finite, non-negative number")
            }
            Self::TextTooLong => f.write_str("the text is longer than 4 GiB"),
            Self::Shaping => f.write_str("the shaper refused the text"),
            Self::Outline(face, glyph) => {
                write!(
                    f,
                    "the outline of glyph {} in {face:?} cannot be drawn",
                    glyph.0
                )
            }
        }
    }
}

impl std::error::Error for TextError {}
