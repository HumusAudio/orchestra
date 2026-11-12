// SPDX-License-Identifier: GPL-3.0-or-later

//! How a piece of text is set: its face, its size and its OpenType features (ADR-0044 TX9).
//!
//! The text tokens of the theme (TX11, step F2.07c) resolve to a [`TextStyle`]; this crate does
//! not know about tokens, so that `notation-layout` can use it without the theme.

use crate::{FaceId, Faces, TextError};

/// Optional OpenType features applied when shaping.
///
/// Only what the interface needs today: tabular figures for the `numeric` family (counters, bar
/// numbers, times, where digits must not shift as the value changes). The font's default
/// features (kerning, standard ligatures, contextual alternates) are always on, as `harfrust`
/// applies them by default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Features {
    /// `tnum`: every digit has the same advance.
    pub tabular_figures: bool,
}

/// A face, a size and the features: everything the shaper needs besides the text.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextStyle {
    /// The face.
    pub face: FaceId,
    /// The size, in logical pixels per em (the interface's "13 px" body text means 13 here).
    pub size: f32,
    /// The optional features.
    pub features: Features,
}

impl TextStyle {
    /// A style with the face and size and no optional feature.
    #[must_use]
    pub const fn new(face: FaceId, size: f32) -> Self {
        Self {
            face,
            size,
            features: Features {
                tabular_figures: false,
            },
        }
    }

    /// The same style with tabular figures on or off.
    #[must_use]
    pub const fn with_tabular_figures(mut self, on: bool) -> Self {
        self.features.tabular_figures = on;
        self
    }
}

/// The factor from font units of `face` to logical pixels at `size` pixels per em, after
/// checking that the size is usable.
///
/// # Why checked here
/// A zero, negative, infinite or NaN size would give empty, mirrored or NaN positions that
/// travel silently to the renderer. Refusing it at the entrance keeps every later computation
/// finite.
///
/// # Errors
/// [`TextError::InvalidSize`] if `size` is not finite and strictly positive.
pub(crate) fn checked_scale(faces: &Faces, face: FaceId, size: f32) -> Result<f32, TextError> {
    if !size.is_finite() || size <= 0.0 {
        return Err(TextError::InvalidSize);
    }
    // `units_per_em` is between 16 and 16384 by skrifa's own validation, so the division is
    // safe and the factor is finite.
    Ok(size / f32::from(faces.units_per_em(face)))
}
