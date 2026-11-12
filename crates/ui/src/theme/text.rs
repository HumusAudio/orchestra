// SPDX-License-Identifier: GPL-3.0-or-later

//! The text tokens of a theme (ADR-0044 TX11, ADR-0037; choice TD1 of F2.07c): which bundled
//! face each family of text uses, and the scale of sizes.
//!
//! # Three kinds of choice, two of them the theme's
//! - **Family** ([`FamilyToken`]): a role of text (interface text, numbers, code, musical
//!   symbols). The theme says which [`Family`] of the bundled fonts it uses; a custom theme may
//!   change it, but only among the bundled ones (ADR-0006: the same look everywhere).
//! - **Size** ([`SizeToken`]): a step of the scale (caption … title). The theme gives each step a
//!   size in logical pixels; a custom theme may change it, within [`MIN_TEXT_SIZE`] and
//!   [`MAX_TEXT_SIZE`].
//! - **Weight** ([`Weight`]): regular, semibold, bold. Chosen by the widget (a heading is bold
//!   whatever the theme), so it is not a token.
//!
//! [`Theme::text_style`](crate::Theme::text_style) combines the three into the
//! [`TextStyle`] that `orchestra-text` lays out. All ten built-in pairs share the default
//! typography ([`Typography::DEFAULT`]): they differ in colour and shape, not in letters.
//!
//! # Changing a text token re-lays out
//! Unlike a colour, which the renderer resolves at draw time (ADR-0039 D3), a family or a size
//! changes the glyphs and their positions: every text must be shaped again and its segment
//! re-recorded (ADR-0044 TX9). Today nothing changes them at run time; the theme editor of
//! F2.12b will.

use orchestra_text::{FaceId, TextStyle};

/// A role of text whose face the theme chooses (TX11).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FamilyToken {
    /// Interface text: labels, menus, dialogs.
    Ui,
    /// Numbers that change in place: counters, bar numbers, times (tabular figures, so digits
    /// do not shift as the value changes).
    Numeric,
    /// Code-like text: file paths, error details.
    Code,
    /// Musical symbols in the interface (SMuFL).
    Music,
}

impl FamilyToken {
    /// Every family token, in declaration order (also the order of [`Typography`]'s array).
    pub const ALL: [Self; 4] = [Self::Ui, Self::Numeric, Self::Code, Self::Music];

    /// The token's stable name, the key of a custom theme's override (as
    /// [`ColorToken::name`](crate::ColorToken::name)): never shown, never changed once released.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Ui => "family-ui",
            Self::Numeric => "family-numeric",
            Self::Code => "family-code",
            Self::Music => "family-music",
        }
    }

    /// The token with this stable name, or `None`. O(n) over 4 tokens.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|token| token.name() == name)
    }

    /// The position of the token in [`FamilyToken::ALL`].
    const fn index(self) -> usize {
        match self {
            Self::Ui => 0,
            Self::Numeric => 1,
            Self::Code => 2,
            Self::Music => 3,
        }
    }
}

/// A family of the bundled fonts that a [`FamilyToken`] can use (ADR-0044 TX3, TX11).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Family {
    /// Inter, in Regular, SemiBold and Bold, proportional figures.
    Inter,
    /// Inter with tabular figures (`tnum`): every digit has the same width.
    InterTabular,
    /// JetBrains Mono, Regular only (other weights fall back to it).
    JetBrainsMono,
    /// Bravura, the SMuFL music font, one weight.
    Bravura,
}

impl Family {
    /// Every family, in declaration order.
    pub const ALL: [Self; 4] = [
        Self::Inter,
        Self::InterTabular,
        Self::JetBrainsMono,
        Self::Bravura,
    ];

    /// The family's stable name, the value a custom theme saves (never shown, never changed).
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Inter => "inter",
            Self::InterTabular => "inter-tabular",
            Self::JetBrainsMono => "jetbrains-mono",
            Self::Bravura => "bravura",
        }
    }

    /// The family with this stable name, or `None`. O(n) over 4 families.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|family| family.name() == name)
    }

    /// The face of this family for `weight`: the face of that weight if the family has it,
    /// otherwise the family's regular face (TX11).
    #[must_use]
    pub const fn face(self, weight: Weight) -> FaceId {
        match (self, weight) {
            (Self::Inter | Self::InterTabular, Weight::Regular) => FaceId::InterRegular,
            (Self::Inter | Self::InterTabular, Weight::SemiBold) => FaceId::InterSemiBold,
            (Self::Inter | Self::InterTabular, Weight::Bold) => FaceId::InterBold,
            (Self::JetBrainsMono, _) => FaceId::JetBrainsMono,
            (Self::Bravura, _) => FaceId::Bravura,
        }
    }
}

/// A step of the size scale (TX11).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SizeToken {
    /// Captions, small print: 11 px by default.
    Caption,
    /// Secondary text: 12 px.
    Small,
    /// Body text, most of the interface: 13 px.
    Body,
    /// Subheadings: 15 px.
    Subheading,
    /// Headings: 18 px.
    Heading,
    /// Titles: 22 px.
    Title,
}

impl SizeToken {
    /// Every size token, smallest first (also the order of [`Typography`]'s array).
    pub const ALL: [Self; 6] = [
        Self::Caption,
        Self::Small,
        Self::Body,
        Self::Subheading,
        Self::Heading,
        Self::Title,
    ];

    /// The token's stable name, the key of a custom theme's override.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Caption => "size-caption",
            Self::Small => "size-small",
            Self::Body => "size-body",
            Self::Subheading => "size-subheading",
            Self::Heading => "size-heading",
            Self::Title => "size-title",
        }
    }

    /// The token with this stable name, or `None`. O(n) over 6 tokens.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|token| token.name() == name)
    }

    /// The position of the token in [`SizeToken::ALL`].
    const fn index(self) -> usize {
        match self {
            Self::Caption => 0,
            Self::Small => 1,
            Self::Body => 2,
            Self::Subheading => 3,
            Self::Heading => 4,
            Self::Title => 5,
        }
    }
}

/// A weight of text, chosen by the widget (not a theme token).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Weight {
    /// 400.
    Regular,
    /// 600.
    SemiBold,
    /// 700.
    Bold,
}

/// The smallest text size a theme can set, logical pixels: below it Latin text is unreadable at
/// scale 1.
pub const MIN_TEXT_SIZE: u8 = 6;

/// The largest text size a theme can set, logical pixels: far beyond any interface text, and
/// small enough that a mistyped value in a settings file cannot ask for absurd glyphs.
pub const MAX_TEXT_SIZE: u8 = 96;

/// The text tokens of one theme instance: a family per [`FamilyToken`], a size per
/// [`SizeToken`]. Ten bytes, `Copy`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Typography {
    /// The family of each family token, in [`FamilyToken::ALL`] order.
    families: [Family; 4],
    /// The size of each size token, logical pixels, in [`SizeToken::ALL`] order; always within
    /// [`MIN_TEXT_SIZE`]..=[`MAX_TEXT_SIZE`].
    sizes: [u8; 6],
}

impl Typography {
    /// The typography of every built-in theme (ADR-0044 TX11): Inter for interface text, Inter
    /// with tabular figures for numbers, JetBrains Mono for code, Bravura for symbols; sizes 11,
    /// 12, 13, 15, 18 and 22 px.
    pub const DEFAULT: Self = Self {
        families: [
            Family::Inter,
            Family::InterTabular,
            Family::JetBrainsMono,
            Family::Bravura,
        ],
        sizes: [11, 12, 13, 15, 18, 22],
    };

    /// The family of `token`. O(1).
    #[must_use]
    pub const fn family(&self, token: FamilyToken) -> Family {
        let [ui, numeric, code, music] = self.families;
        match token {
            FamilyToken::Ui => ui,
            FamilyToken::Numeric => numeric,
            FamilyToken::Code => code,
            FamilyToken::Music => music,
        }
    }

    /// The size of `token`, logical pixels. O(1).
    #[must_use]
    pub const fn size(&self, token: SizeToken) -> u8 {
        let [caption, small, body, subheading, heading, title] = self.sizes;
        match token {
            SizeToken::Caption => caption,
            SizeToken::Small => small,
            SizeToken::Body => body,
            SizeToken::Subheading => subheading,
            SizeToken::Heading => heading,
            SizeToken::Title => title,
        }
    }

    /// Replaces the family of `token`.
    pub fn set_family(&mut self, token: FamilyToken, family: Family) {
        if let Some(slot) = self.families.get_mut(token.index()) {
            *slot = family;
        }
    }

    /// Replaces the size of `token`, clamped to [`MIN_TEXT_SIZE`]..=[`MAX_TEXT_SIZE`] (a custom
    /// theme is never refused, ADR-0037 T8; an absurd size is brought into range instead).
    pub fn set_size(&mut self, token: SizeToken, size: u8) {
        if let Some(slot) = self.sizes.get_mut(token.index()) {
            *slot = size.clamp(MIN_TEXT_SIZE, MAX_TEXT_SIZE);
        }
    }

    /// The style for text of `family` at `size` in `weight`: the family's face for that weight
    /// (or its regular face), the size in logical pixels, and tabular figures for
    /// [`Family::InterTabular`].
    #[must_use]
    pub fn text_style(&self, family: FamilyToken, size: SizeToken, weight: Weight) -> TextStyle {
        let family = self.family(family);
        TextStyle::new(family.face(weight), f32::from(self.size(size)))
            .with_tabular_figures(matches!(family, Family::InterTabular))
    }
}

impl Default for Typography {
    fn default() -> Self {
        Self::DEFAULT
    }
}
