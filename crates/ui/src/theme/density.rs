// SPDX-License-Identifier: GPL-3.0-or-later

//! The density of the interface (ADR-0037, decided in F2.09a, ADR-0045 precisions): how much
//! room controls and panels take.
//!
//! # One choice, a scale of values
//! A theme chooses one [`Density`]; the density gives the [`Spacing`] the widgets and the
//! layout read: the padding inside controls, four gaps between elements, and the height of a
//! control. Two densities, as most professional tools offer (a DAW full of tracks wants compact,
//! a dialog for a first-time user comfortable):
//!
//! | | comfortable (default) | compact |
//! |---|---|---|
//! | padding inside a control | 10 | 6 |
//! | gaps: extra small, small, medium, large | 4, 8, 12, 16 | 2, 4, 8, 12 |
//! | height of a control | 28 | 22 |
//!
//! All in logical pixels. The values are the owner's first approval (2026-11-12), meant to be
//! tuned once real panels exist.
//!
//! # Changing the density re-lays out
//! Like the text tokens (ADR-0044 TX9), a density change moves widgets: the interface lays itself
//! out again, it does not only redraw.

/// How much room the interface takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Density {
    /// Generous spacing: the default of every built-in theme.
    #[default]
    Comfortable,
    /// Tight spacing, for screens full of controls.
    Compact,
}

impl Density {
    /// Every density, in declaration order.
    pub const ALL: [Self; 2] = [Self::Comfortable, Self::Compact];

    /// The density's stable name, the value a custom theme saves (never shown, never changed).
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Comfortable => "comfortable",
            Self::Compact => "compact",
        }
    }

    /// The density with this stable name, or `None`.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|density| density.name() == name)
    }

    /// The spacing values of this density (see the module's table).
    #[must_use]
    pub const fn spacing(self) -> Spacing {
        match self {
            Self::Comfortable => Spacing {
                control_padding: 10.0,
                gap_xs: 4.0,
                gap_s: 8.0,
                gap_m: 12.0,
                gap_l: 16.0,
                control_height: 28.0,
            },
            Self::Compact => Spacing {
                control_padding: 6.0,
                gap_xs: 2.0,
                gap_s: 4.0,
                gap_m: 8.0,
                gap_l: 12.0,
                control_height: 22.0,
            },
        }
    }
}

/// The spacing values of a [`Density`], logical pixels. Plain data, read all at once by a widget
/// or a container.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spacing {
    /// Space between a control's border and its content (a button's text).
    pub control_padding: f32,
    /// The smallest gap (between an icon and its label).
    pub gap_xs: f32,
    /// A small gap (between the controls of a row).
    pub gap_s: f32,
    /// A medium gap (between the rows of a form).
    pub gap_m: f32,
    /// A large gap (between groups, around a panel's content).
    pub gap_l: f32,
    /// The height of a single-line control (button, field, checkbox row).
    pub control_height: f32,
}
