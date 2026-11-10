// SPDX-License-Identifier: GPL-3.0-or-later

//! A theme pair: the light and the dark instance of one theme (ADR-0037 T1).

use crate::theme::mode::Appearance;
use crate::theme::token::Theme;

/// A complete theme: a light instance and a dark instance, both with every token (ADR-0014
/// requires both from the first widget; ADR-0037 T1 makes every theme a pair).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThemePair {
    /// The light half.
    pub light: Theme,
    /// The dark half.
    pub dark: Theme,
}

impl ThemePair {
    /// The half for `appearance` (see [`ThemeMode::resolve`](crate::ThemeMode::resolve)).
    #[must_use]
    pub const fn instance(&self, appearance: Appearance) -> &Theme {
        match appearance {
            Appearance::Light => &self.light,
            Appearance::Dark => &self.dark,
        }
    }
}
