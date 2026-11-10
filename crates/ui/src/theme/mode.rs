// SPDX-License-Identifier: GPL-3.0-or-later

//! The user's mode setting (*System / Light / Dark*, ADR-0014 §1) and how it picks one half of a
//! theme pair (ADR-0037 T1, T9).

/// One of the two halves of a theme pair.
///
/// Exhaustive (ADR-0034 D2): there are exactly two.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Appearance {
    /// Light backgrounds, dark text.
    Light,
    /// Dark backgrounds, light text.
    Dark,
}

/// The user's mode setting. The default is [`ThemeMode::System`] (ADR-0014 §1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ThemeMode {
    /// Follow the operating system's preference, and its changes while the program runs.
    #[default]
    System,
    /// Always the light half.
    Light,
    /// Always the dark half.
    Dark,
}

impl ThemeMode {
    /// The appearance to show, given what the operating system reports.
    ///
    /// `system` is what [`AppearanceWatch::current`](crate::AppearanceWatch::current) returns:
    /// `None` when the preference cannot be read (no desktop portal, an error, a system that
    /// does not say; ADR-0038). Then *System* gives **dark** (ADR-0037 T9, owner's choice):
    /// the default pair, Studio, is designed dark first. *Light* and *Dark* ignore `system`.
    #[must_use]
    pub const fn resolve(self, system: Option<Appearance>) -> Appearance {
        match (self, system) {
            (Self::Light, _) => Appearance::Light,
            (Self::Dark, _) | (Self::System, None) => Appearance::Dark,
            (Self::System, Some(appearance)) => appearance,
        }
    }
}
