// SPDX-License-Identifier: GPL-3.0-or-later

//! The semantic tokens of a theme (ADR-0014 §2, ADR-0037 T3) and one theme instance.
//!
//! # One list, generated code
//! The colour tokens are listed **once**, in the `color_tokens!` invocation below. The macro
//! generates from that list the public enum [`ColorToken`], its stable names, the array of all
//! tokens, and the private struct `Colors` with one named field per token. Because the built-in
//! themes fill `Colors` with a struct literal, forgetting a token in one of the twenty instances is
//! a **compile error**, never a silent default (ADR-0037, Consequences). Adding a token means
//! adding one line here and one value per instance.
//!
//! # Why named fields rather than an array indexed by the enum?
//! An array would also give O(1) reads, but a missing entry would only be found by a test, and
//! indexing would hide a theoretical panic. A `match` from token to field compiles to a jump
//! table: still O(1), no bounds check, no panic path.

use orchestra_text::TextStyle;

use crate::theme::color::Color;
use crate::theme::density::{Density, Spacing};
use crate::theme::text::{Family, FamilyToken, SizeToken, Typography, Weight};

/// Generates [`ColorToken`], its name table and the struct `Colors` from one list of
/// `Variant => field, "stable-name";` entries, each preceded by its doc comment.
macro_rules! color_tokens {
    ($( $(#[doc = $doc:literal])* $variant:ident => $field:ident, $name:literal; )*) => {
        /// A semantic colour token: a *role* (window background, muted text, focus ring…),
        /// never a literal colour. Widgets and renderers ask the active [`Theme`] for the colour
        /// of a token (ADR-0014 §2).
        ///
        /// The list and its meaning are fixed by ADR-0037 T3. Exhaustive on purpose
        /// (ADR-0034 D2): a new token must be handled by every consumer, starting with the
        /// theme editor.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum ColorToken {
            $( $(#[doc = $doc])* $variant, )*
        }

        impl ColorToken {
            /// Every colour token, in declaration order (the order of ADR-0037 T3's table).
            pub const ALL: &'static [Self] = &[$( Self::$variant, )*];

            /// The token's stable name, in kebab case as ADR-0014 writes them (`surface-raised`,
            /// `on-accent`…). It is the key under which a custom theme saves an override
            /// (ADR-0037 T7), so it never changes once released; it is **not** shown to the
            /// user (the theme editor shows Fluent strings, F2.12b).
            #[must_use]
            pub const fn name(self) -> &'static str {
                match self {
                    $( Self::$variant => $name, )*
                }
            }

            /// The token with this stable name, or `None` if no token has it.
            ///
            /// O(n) in the number of tokens (24): a linear scan of [`Self::ALL`]. It runs only
            /// when a custom theme is read from the settings file, never per frame, so a hash
            /// table would be overhead for nothing.
            #[must_use]
            pub fn from_name(name: &str) -> Option<Self> {
                Self::ALL.iter().copied().find(|token| token.name() == name)
            }
        }

        /// The colours of one theme instance, one named field per [`ColorToken`].
        ///
        /// Private: the rest of the program reads colours through [`Theme::color`], so the
        /// field layout can change freely.
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub(crate) struct Colors {
            $( $(#[doc = $doc])* pub(crate) $field: Color, )*
        }

        impl Colors {
            /// The colour of `token`. O(1): one `match`, compiled to a jump table.
            const fn get(&self, token: ColorToken) -> Color {
                match token {
                    $( ColorToken::$variant => self.$field, )*
                }
            }

            /// Replaces the colour of `token` (used to apply a custom theme's overrides).
            fn set(&mut self, token: ColorToken, color: Color) {
                match token {
                    $( ColorToken::$variant => self.$field = color, )*
                }
            }
        }
    };
}

color_tokens! {
    /// The window background: the base on which everything else sits.
    Surface => surface, "surface";
    /// Panels, title bar, status bar: one level above the window background.
    SurfaceRaised => surface_raised, "surface-raised";
    /// Text fields and the document area: one level below the window background.
    SurfaceSunken => surface_sunken, "surface-sunken";
    /// Menus, pop-ups and other overlays that float above the window.
    SurfaceOverlay => surface_overlay, "surface-overlay";
    /// Decorative separators between areas. Exempt from the contrast pairs (ADR-0037 T4):
    /// it carries no information a user must see.
    Border => border, "border";
    /// The edge that identifies a control (button, field, slider thumb): 3:1 against the
    /// surfaces, WCAG 1.4.11.
    ControlBorder => control_border, "control-border";
    /// Normal text.
    Text => text, "text";
    /// Secondary text: labels, hints, shortcuts in menus.
    TextMuted => text_muted, "text-muted";
    /// Text of inactive controls. Exempt from the contrast pairs, as WCAG 1.4.3 allows.
    TextDisabled => text_disabled, "text-disabled";
    /// Filled controls (the primary button, a toggle that is on, a slider's fill).
    Accent => accent, "accent";
    /// The accent under the pointer.
    AccentHover => accent_hover, "accent-hover";
    /// The accent while pressed.
    AccentPressed => accent_pressed, "accent-pressed";
    /// Text and icons drawn on the accent.
    OnAccent => on_accent, "on-accent";
    /// The accent used as text or as a link on a surface: often darker (light themes) or lighter
    /// (dark themes) than `accent`, so that it reaches 4.5:1.
    AccentText => accent_text, "accent-text";
    /// The fill of an ordinary (not primary) control.
    Control => control, "control";
    /// An ordinary control under the pointer, and the highlighted item of a menu.
    ControlHover => control_hover, "control-hover";
    /// An ordinary control while pressed.
    ControlPressed => control_pressed, "control-pressed";
    /// The background of selected items and selected text.
    Selection => selection, "selection";
    /// Text on `selection`.
    OnSelection => on_selection, "on-selection";
    /// The keyboard focus ring.
    Focus => focus, "focus";
    /// Error messages and icons.
    Error => error, "error";
    /// Warning messages and icons.
    Warning => warning, "warning";
    /// Confirmation messages and icons.
    Success => success, "success";
    /// The colour of the shadow under overlays, usually translucent black; alpha 0 means no
    /// shadow. Exempt from the contrast pairs: depth is not information.
    Shadow => shadow, "shadow";
}

/// A shape token: a size in **logical pixels**, multiplied by the window's scale factor when
/// drawn (ADR-0037 T3). Whole numbers 0–255.
///
/// Exhaustive on purpose (ADR-0034 D2), like [`ColorToken`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ShapeToken {
    /// Corner radius of controls, panels and menus; 0 gives square corners.
    CornerRadius,
    /// Width of borders and separators.
    LineWidth,
    /// Width of the keyboard focus ring.
    FocusWidth,
    /// Blur radius of the overlay shadow.
    ShadowBlur,
    /// Vertical offset of the overlay shadow (downwards).
    ShadowOffset,
}

impl ShapeToken {
    /// Every shape token, in the order of ADR-0037 T3's table.
    pub const ALL: &'static [Self] = &[
        Self::CornerRadius,
        Self::LineWidth,
        Self::FocusWidth,
        Self::ShadowBlur,
        Self::ShadowOffset,
    ];

    /// The token's stable name (kebab case), the key of a custom theme's override; never shown
    /// to the user, never changed once released (as [`ColorToken::name`]).
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::CornerRadius => "corner-radius",
            Self::LineWidth => "line-width",
            Self::FocusWidth => "focus-width",
            Self::ShadowBlur => "shadow-blur",
            Self::ShadowOffset => "shadow-offset",
        }
    }

    /// The token with this stable name, or `None`. O(n) over 5 tokens, as
    /// [`ColorToken::from_name`].
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|token| token.name() == name)
    }
}

/// The shape tokens of one theme instance, in logical pixels (see [`ShapeToken`]).
///
/// Public with public fields: it is plain data that a renderer reads all at once (a button needs
/// the radius, the line width and the focus width together), and five bytes copy for free.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Shape {
    /// [`ShapeToken::CornerRadius`].
    pub corner_radius: u8,
    /// [`ShapeToken::LineWidth`].
    pub line_width: u8,
    /// [`ShapeToken::FocusWidth`].
    pub focus_width: u8,
    /// [`ShapeToken::ShadowBlur`].
    pub shadow_blur: u8,
    /// [`ShapeToken::ShadowOffset`].
    pub shadow_offset: u8,
}

impl Shape {
    /// The value of `token`. O(1).
    #[must_use]
    pub const fn get(&self, token: ShapeToken) -> u8 {
        match token {
            ShapeToken::CornerRadius => self.corner_radius,
            ShapeToken::LineWidth => self.line_width,
            ShapeToken::FocusWidth => self.focus_width,
            ShapeToken::ShadowBlur => self.shadow_blur,
            ShapeToken::ShadowOffset => self.shadow_offset,
        }
    }

    /// Replaces the value of `token`.
    pub fn set(&mut self, token: ShapeToken, value: u8) {
        match token {
            ShapeToken::CornerRadius => self.corner_radius = value,
            ShapeToken::LineWidth => self.line_width = value,
            ShapeToken::FocusWidth => self.focus_width = value,
            ShapeToken::ShadowBlur => self.shadow_blur = value,
            ShapeToken::ShadowOffset => self.shadow_offset = value,
        }
    }
}

/// One theme **instance**: the complete set of tokens for one appearance (the light or the dark
/// half of a [`ThemePair`](crate::ThemePair)).
///
/// Widgets never store a colour of their own: they ask the active instance at draw time, so
/// switching colours is a full redraw and nothing more (ADR-0037 T6); switching the text tokens
/// re-lays out the texts (ADR-0044 TX9). Reading a token is O(1) and allocation-free; the whole
/// instance is about 24 × 4 + 5 + 10 + 1 = 112 bytes, `Copy`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    /// The colour tokens.
    colors: Colors,
    /// The shape tokens.
    shape: Shape,
    /// The text tokens (ADR-0044 TX11).
    text: Typography,
    /// The density (ADR-0037, F2.09a).
    density: Density,
}

impl Theme {
    /// Builds an instance from its parts. Crate-private: themes come from the built-in pairs or
    /// from resolving a [`CustomTheme`](crate::CustomTheme), so every instance has a value for
    /// every token by construction.
    ///
    /// Every built-in instance has the default typography (ADR-0044 TX11: the ten pairs differ
    /// in colour and shape, not in letters), so it is not a parameter.
    pub(crate) const fn from_parts(colors: Colors, shape: Shape) -> Self {
        Self {
            colors,
            shape,
            text: Typography::DEFAULT,
            density: Density::Comfortable,
        }
    }

    /// The colour of `token` in this instance. O(1), no allocation.
    #[must_use]
    pub const fn color(&self, token: ColorToken) -> Color {
        self.colors.get(token)
    }

    /// The shape tokens of this instance.
    #[must_use]
    pub const fn shape(&self) -> Shape {
        self.shape
    }

    /// This instance with the colour of `token` replaced.
    #[must_use]
    pub fn with_color(mut self, token: ColorToken, color: Color) -> Self {
        self.colors.set(token, color);
        self
    }

    /// This instance with the shape token `token` replaced.
    #[must_use]
    pub fn with_shape(mut self, token: ShapeToken, value: u8) -> Self {
        self.shape.set(token, value);
        self
    }

    /// The text tokens of this instance (ADR-0044 TX11).
    #[must_use]
    pub const fn typography(&self) -> Typography {
        self.text
    }

    /// The style of text in the family `family`, at the size `size`, in `weight`, as this
    /// instance sets them (see [`Typography::text_style`]).
    #[must_use]
    pub fn text_style(&self, family: FamilyToken, size: SizeToken, weight: Weight) -> TextStyle {
        self.text.text_style(family, size, weight)
    }

    /// This instance with the family of `token` replaced.
    #[must_use]
    pub fn with_family(mut self, token: FamilyToken, family: Family) -> Self {
        self.text.set_family(token, family);
        self
    }

    /// The density of this instance.
    #[must_use]
    pub const fn density(&self) -> Density {
        self.density
    }

    /// The spacing values of this instance's density.
    #[must_use]
    pub const fn spacing(&self) -> Spacing {
        self.density.spacing()
    }

    /// This instance with another density.
    #[must_use]
    pub const fn with_density(mut self, density: Density) -> Self {
        self.density = density;
        self
    }

    /// This instance with the size of `token` replaced (clamped to the allowed range, see
    /// [`Typography::set_size`]).
    #[must_use]
    pub fn with_text_size(mut self, token: SizeToken, size: u8) -> Self {
        self.text.set_size(token, size);
        self
    }
}
