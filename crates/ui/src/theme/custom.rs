// SPDX-License-Identifier: GPL-3.0-or-later

//! Custom themes: a built-in pair plus the user's overrides (ADR-0037 T7).
//!
//! # Why differences and not a full copy?
//! Tokens will be added in later versions (text in F2.07, density in F2.09, notation and DAW
//! tokens after that). A custom theme stored as a full copy would lack them and need a migration
//! of every saved theme; stored as "base + overrides", it takes every new token from its base
//! automatically. It is the same idea as the performance offsets of a note, stored as deltas
//! from the written intent (ADR-0004).

use crate::theme::builtin::BuiltInTheme;
use crate::theme::color::Color;
use crate::theme::density::Density;
use crate::theme::mode::Appearance;
use crate::theme::pair::ThemePair;
use crate::theme::text::{Family, FamilyToken, SizeToken};
use crate::theme::token::{ColorToken, ShapeToken, Theme};

/// One change a custom theme makes to its base instance.
///
/// Exhaustive (ADR-0034 D2): the text tokens (F2.07c) and the density (F2.09a) added variants, and
/// the settings file and the editor must handle them all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Override {
    /// A colour token gets this colour.
    Color(ColorToken, Color),
    /// A shape token gets this value, in logical pixels.
    Shape(ShapeToken, u8),
    /// A family token gets this bundled family (ADR-0044 TX11).
    Family(FamilyToken, Family),
    /// The theme gets this density (F2.09a).
    Density(Density),
    /// A size token gets this size, in logical pixels (clamped to
    /// [`MIN_TEXT_SIZE`](crate::MIN_TEXT_SIZE)..=[`MAX_TEXT_SIZE`](crate::MAX_TEXT_SIZE)).
    TextSize(SizeToken, u8),
}

impl Override {
    /// Applies this override to `theme`.
    #[must_use]
    fn apply(self, theme: Theme) -> Theme {
        match self {
            Self::Color(token, color) => theme.with_color(token, color),
            Self::Shape(token, value) => theme.with_shape(token, value),
            Self::Family(token, family) => theme.with_family(token, family),
            Self::TextSize(token, size) => theme.with_text_size(token, size),
            Self::Density(density) => theme.with_density(density),
        }
    }
}

/// A theme made by the user: a [`BuiltInTheme`] as its base and, for each half of the pair, a
/// list of [`Override`]s.
///
/// The lists are applied in order, so when one token is overridden twice **the later override
/// wins**; the editor (F2.12b) will keep one entry per token, but a hand-edited settings file may
/// not, and this rule makes the result well defined. A custom theme is never refused for its
/// contrast (ADR-0037 T8): [`Theme::contrast_failures`] on the resolved instances gives the
/// warnings.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CustomTheme {
    /// The built-in pair the theme starts from. Built-in IDs are never removed, so a base always
    /// exists (ADR-0037 T7).
    pub base: BuiltInTheme,
    /// Changes to the base's light instance.
    pub light: Vec<Override>,
    /// Changes to the base's dark instance.
    pub dark: Vec<Override>,
}

impl CustomTheme {
    /// A custom theme with no overrides yet: it looks exactly like `base`.
    #[must_use]
    pub const fn new(base: BuiltInTheme) -> Self {
        Self {
            base,
            light: Vec::new(),
            dark: Vec::new(),
        }
    }

    /// The overrides of one half.
    #[must_use]
    pub fn overrides(&self, appearance: Appearance) -> &[Override] {
        match appearance {
            Appearance::Light => &self.light,
            Appearance::Dark => &self.dark,
        }
    }

    /// The complete pair: the base's two instances with the overrides applied.
    ///
    /// Called when the theme is chosen or edited, never per frame. O(o) in the number of
    /// overrides; no allocation (the pair is returned by value, 2 × 101 bytes).
    #[must_use]
    pub fn resolve(&self) -> ThemePair {
        let base = self.base.pair();
        let apply = |theme: Theme, overrides: &[Override]| {
            overrides
                .iter()
                .fold(theme, |theme, &change| change.apply(theme))
        };
        ThemePair {
            light: apply(base.light, &self.light),
            dark: apply(base.dark, &self.dark),
        }
    }
}
