// SPDX-License-Identifier: GPL-3.0-or-later

//! The ten built-in theme pairs (ADR-0037 T1), as constants.
//!
//! # Where the values come from
//! These constants are the **only** source of the built-in colours. They were generated once from
//! the comparison page the owner approved on 2026-11-10 (version 1, two accents already darkened
//! there to pass WCAG), with the shadow values of ADR-0037 T3 added; from now on they are edited
//! here, and the tests of ADR-0037 T5 check every change.
//!
//! Every instance fills `Colors` with a struct literal, so a missing token is a compile error.
//! Colours are written as `0xRRGGBB` literals, the notation of the comparison page and of CSS.

#![allow(
    clippy::unreadable_literal,
    reason = "colours are written 0xRRGGBB so that they read exactly like the CSS `#rrggbb` of \
              the comparison page and of the theme editor; digit separators would break that"
)]

use crate::theme::color::Color;
use crate::theme::pair::ThemePair;
use crate::theme::token::{Colors, Shape, Theme};

/// Shorthand for an opaque colour from a `0xRRGGBB` literal, to keep the tables readable.
const fn c(hex: u32) -> Color {
    Color::rgb(hex)
}

/// The light shadow of ADR-0037 T3: black at 46/255 ≈ 18% coverage.
const SHADOW_LIGHT: Color = Color::rgba(0x0000_002e);
/// The dark shadow of ADR-0037 T3: black at 128/255 ≈ 50% coverage (a dark surface needs a
/// stronger shadow for the same perceived depth).
const SHADOW_DARK: Color = Color::rgba(0x0000_0080);
/// No shadow at all: alpha 0 (ADR-0037 T3, for the pairs built on lines).
const NO_SHADOW: Color = Color::rgba(0x0000_0000);

/// The shape of a pair: its corners and line width; the focus ring is 2 px everywhere (ADR-0037
/// T3, the thickness WCAG 2.4.13 asks for), the shadow 18 px of blur moved 6 px down, or nothing.
const fn shape(corner_radius: u8, line_width: u8, shadow: bool) -> Shape {
    Shape {
        corner_radius,
        line_width,
        focus_width: 2,
        shadow_blur: if shadow { 18 } else { 0 },
        shadow_offset: if shadow { 6 } else { 0 },
    }
}

/// The identifier of a built-in theme pair (ADR-0037 T1).
///
/// The identifiers are stable: never renamed, never reused, never removed, because the settings
/// file saves them and every custom theme names one as its base (T7). The names shown to the
/// user are Fluent strings added with the Preferences window (F2.12), not these identifiers.
///
/// Exhaustive (ADR-0034 D2). The default is [`BuiltInTheme::Studio`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub enum BuiltInTheme {
    /// Pure black and pure white only: every surface is white (or black) and the structure comes from
    /// lines. Colour appears only for errors, warnings and confirmations. No shadow.
    Absolute,
    /// Pure black and white for ink and paper, a grey scale to separate the layers (panels, fields,
    /// menus) with fewer lines. No accent hue: the accent is black (white in dark mode).
    Graphite,
    /// Pure black and white with thick lines and a single signal red for what is active or selected,
    /// like a technical manual or a control panel. No shadow.
    Signal,
    /// The colours of the PDF guides: night-blue ink, warm paper, amber. In light mode the amber is
    /// darker than the guides' `#c8892b`, which reaches only 2.68:1 on the paper.
    Parchment,
    /// Cool, slightly bluish greys and a firm blue: sober and familiar, close to today's professional
    /// applications, with soft corners.
    Slate,
    /// The default pair (ADR-0037 T1). Designed dark first, like mixers and DAWs: charcoal with a
    /// pilot-light orange. The light half is a warm control-room grey.
    #[default]
    Studio,
    /// Sage and moss greens, natural and restful tones for long writing sessions. Very rounded
    /// corners.
    Forest,
    /// Watery whites and a deep teal: bright and fresh, with more colour in the surfaces than the
    /// other pairs.
    Lagoon,
    /// Deep violet and lavender with very soft corners: the atmosphere of a stage with the lights
    /// down, more expressive and less office-like.
    Nocturne,
    /// Very light lilac greys, a raspberry accent, wide corners and double lines: the furthest from
    /// black-and-white minimalism.
    Snow,
}

impl BuiltInTheme {
    /// Every built-in pair, in the order of ADR-0037 T1's table (the black-and-white pairs
    /// first).
    pub const ALL: &'static [Self] = &[
        Self::Absolute,
        Self::Graphite,
        Self::Signal,
        Self::Parchment,
        Self::Slate,
        Self::Studio,
        Self::Forest,
        Self::Lagoon,
        Self::Nocturne,
        Self::Snow,
    ];

    /// The stable identifier saved in the settings file (`"studio"`, `"slate"`…).
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::Absolute => "absolute",
            Self::Graphite => "graphite",
            Self::Signal => "signal",
            Self::Parchment => "parchment",
            Self::Slate => "slate",
            Self::Studio => "studio",
            Self::Forest => "forest",
            Self::Lagoon => "lagoon",
            Self::Nocturne => "nocturne",
            Self::Snow => "snow",
        }
    }

    /// The pair with this stable identifier, or `None`. O(n) over 10 pairs; runs only when the
    /// settings file is read.
    #[must_use]
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|theme| theme.id() == id)
    }

    /// The pair's two instances. Returns a reference to a constant: no copy, no allocation.
    #[must_use]
    pub const fn pair(self) -> &'static ThemePair {
        match self {
            Self::Absolute => &ABSOLUTE,
            Self::Graphite => &GRAPHITE,
            Self::Signal => &SIGNAL,
            Self::Parchment => &PARCHMENT,
            Self::Slate => &SLATE,
            Self::Studio => &STUDIO,
            Self::Forest => &FOREST,
            Self::Lagoon => &LAGOON,
            Self::Nocturne => &NOCTURNE,
            Self::Snow => &SNOW,
        }
    }
}

/// The `absolute` pair: see [`BuiltInTheme::Absolute`].
const ABSOLUTE: ThemePair = ThemePair {
    light: Theme::from_parts(
        Colors {
            surface: c(0xffffff),
            surface_raised: c(0xffffff),
            surface_sunken: c(0xffffff),
            surface_overlay: c(0xffffff),
            border: c(0x000000),
            control_border: c(0x000000),
            text: c(0x000000),
            text_muted: c(0x595959),
            text_disabled: c(0xa6a6a6),
            accent: c(0x000000),
            accent_hover: c(0x333333),
            accent_pressed: c(0x595959),
            on_accent: c(0xffffff),
            accent_text: c(0x000000),
            control: c(0xffffff),
            control_hover: c(0xebebeb),
            control_pressed: c(0xd6d6d6),
            selection: c(0x000000),
            on_selection: c(0xffffff),
            focus: c(0x000000),
            error: c(0xc00000),
            warning: c(0x8a5300),
            success: c(0x1a7431),
            shadow: NO_SHADOW,
        },
        shape(0, 1, false),
    ),
    dark: Theme::from_parts(
        Colors {
            surface: c(0x000000),
            surface_raised: c(0x000000),
            surface_sunken: c(0x000000),
            surface_overlay: c(0x000000),
            border: c(0xffffff),
            control_border: c(0xffffff),
            text: c(0xffffff),
            text_muted: c(0xa6a6a6),
            text_disabled: c(0x595959),
            accent: c(0xffffff),
            accent_hover: c(0xd6d6d6),
            accent_pressed: c(0xadadad),
            on_accent: c(0x000000),
            accent_text: c(0xffffff),
            control: c(0x000000),
            control_hover: c(0x1f1f1f),
            control_pressed: c(0x333333),
            selection: c(0xffffff),
            on_selection: c(0x000000),
            focus: c(0xffffff),
            error: c(0xff6b6b),
            warning: c(0xffb340),
            success: c(0x4cd07d),
            shadow: NO_SHADOW,
        },
        shape(0, 1, false),
    ),
};

/// The `graphite` pair: see [`BuiltInTheme::Graphite`].
const GRAPHITE: ThemePair = ThemePair {
    light: Theme::from_parts(
        Colors {
            surface: c(0xffffff),
            surface_raised: c(0xf2f2f2),
            surface_sunken: c(0xfafafa),
            surface_overlay: c(0xffffff),
            border: c(0xd4d4d4),
            control_border: c(0x767676),
            text: c(0x000000),
            text_muted: c(0x5c5c5c),
            text_disabled: c(0xa3a3a3),
            accent: c(0x000000),
            accent_hover: c(0x2e2e2e),
            accent_pressed: c(0x4d4d4d),
            on_accent: c(0xffffff),
            accent_text: c(0x000000),
            control: c(0xe8e8e8),
            control_hover: c(0xdcdcdc),
            control_pressed: c(0xc8c8c8),
            selection: c(0xd0d0d0),
            on_selection: c(0x000000),
            focus: c(0x000000),
            error: c(0xb3001b),
            warning: c(0x7a4b00),
            success: c(0x1d6b33),
            shadow: SHADOW_LIGHT,
        },
        shape(2, 1, true),
    ),
    dark: Theme::from_parts(
        Colors {
            surface: c(0x000000),
            surface_raised: c(0x141414),
            surface_sunken: c(0x0a0a0a),
            surface_overlay: c(0x1f1f1f),
            border: c(0x333333),
            control_border: c(0x8a8a8a),
            text: c(0xffffff),
            text_muted: c(0xa8a8a8),
            text_disabled: c(0x5e5e5e),
            accent: c(0xffffff),
            accent_hover: c(0xdedede),
            accent_pressed: c(0xbdbdbd),
            on_accent: c(0x000000),
            accent_text: c(0xffffff),
            control: c(0x262626),
            control_hover: c(0x333333),
            control_pressed: c(0x424242),
            selection: c(0x4a4a4a),
            on_selection: c(0xffffff),
            focus: c(0xffffff),
            error: c(0xff7070),
            warning: c(0xf2b84b),
            success: c(0x5fd38a),
            shadow: SHADOW_DARK,
        },
        shape(2, 1, true),
    ),
};

/// The `signal` pair: see [`BuiltInTheme::Signal`].
const SIGNAL: ThemePair = ThemePair {
    light: Theme::from_parts(
        Colors {
            surface: c(0xffffff),
            surface_raised: c(0xffffff),
            surface_sunken: c(0xf5f5f5),
            surface_overlay: c(0xffffff),
            border: c(0x000000),
            control_border: c(0x000000),
            text: c(0x000000),
            text_muted: c(0x575757),
            text_disabled: c(0xa6a6a6),
            accent: c(0xd40000),
            accent_hover: c(0xb30000),
            accent_pressed: c(0x8f0000),
            on_accent: c(0xffffff),
            accent_text: c(0xc20000),
            control: c(0xffffff),
            control_hover: c(0xf0f0f0),
            control_pressed: c(0xdedede),
            selection: c(0xd40000),
            on_selection: c(0xffffff),
            focus: c(0x000000),
            error: c(0xc20000),
            warning: c(0x7d4d00),
            success: c(0x1a6e30),
            shadow: NO_SHADOW,
        },
        shape(0, 2, false),
    ),
    dark: Theme::from_parts(
        Colors {
            surface: c(0x000000),
            surface_raised: c(0x000000),
            surface_sunken: c(0x0d0d0d),
            surface_overlay: c(0x000000),
            border: c(0xffffff),
            control_border: c(0xffffff),
            text: c(0xffffff),
            text_muted: c(0xa8a8a8),
            text_disabled: c(0x5a5a5a),
            accent: c(0xff3b30),
            accent_hover: c(0xff5e55),
            accent_pressed: c(0xff8078),
            on_accent: c(0x000000),
            accent_text: c(0xff4d42),
            control: c(0x000000),
            control_hover: c(0x1a1a1a),
            control_pressed: c(0x2e2e2e),
            selection: c(0xff3b30),
            on_selection: c(0x000000),
            focus: c(0xffffff),
            error: c(0xff6b61),
            warning: c(0xffb340),
            success: c(0x4cd07d),
            shadow: NO_SHADOW,
        },
        shape(0, 2, false),
    ),
};

/// The `parchment` pair: see [`BuiltInTheme::Parchment`].
const PARCHMENT: ThemePair = ThemePair {
    light: Theme::from_parts(
        Colors {
            surface: c(0xf6f3ec),
            surface_raised: c(0xfbf9f4),
            surface_sunken: c(0xefebe1),
            surface_overlay: c(0xfffdf8),
            border: c(0xd9d4c7),
            control_border: c(0x857e6c),
            text: c(0x1f2547),
            text_muted: c(0x5b6178),
            text_disabled: c(0xa9a89f),
            accent: c(0x9c6614),
            accent_hover: c(0x8c5a10),
            accent_pressed: c(0x7d500c),
            on_accent: c(0xffffff),
            accent_text: c(0x8a5a12),
            control: c(0xebe6da),
            control_hover: c(0xe2dccd),
            control_pressed: c(0xd6cfbd),
            selection: c(0x1f2547),
            on_selection: c(0xf6f3ec),
            focus: c(0x3d5bb5),
            error: c(0xa8261c),
            warning: c(0x80520f),
            success: c(0x3f6b1e),
            shadow: SHADOW_LIGHT,
        },
        shape(4, 1, true),
    ),
    dark: Theme::from_parts(
        Colors {
            surface: c(0x161a33),
            surface_raised: c(0x1f2547),
            surface_sunken: c(0x11142a),
            surface_overlay: c(0x262d55),
            border: c(0x343c66),
            control_border: c(0x7d84a8),
            text: c(0xf6f3ec),
            text_muted: c(0xb3b6c8),
            text_disabled: c(0x5f6585),
            accent: c(0xe0a040),
            accent_hover: c(0xe8b25e),
            accent_pressed: c(0xf0c47c),
            on_accent: c(0x161a33),
            accent_text: c(0xe8ad52),
            control: c(0x2a3160),
            control_hover: c(0x333b70),
            control_pressed: c(0x3d4680),
            selection: c(0xe0a040),
            on_selection: c(0x161a33),
            focus: c(0x8fa8ff),
            error: c(0xff8a7a),
            warning: c(0xf0c060),
            success: c(0x9ad16a),
            shadow: SHADOW_DARK,
        },
        shape(4, 1, true),
    ),
};

/// The `slate` pair: see [`BuiltInTheme::Slate`].
const SLATE: ThemePair = ThemePair {
    light: Theme::from_parts(
        Colors {
            surface: c(0xf7f8fa),
            surface_raised: c(0xffffff),
            surface_sunken: c(0xeef0f4),
            surface_overlay: c(0xffffff),
            border: c(0xdde1e8),
            control_border: c(0x7f8796),
            text: c(0x1b2230),
            text_muted: c(0x576070),
            text_disabled: c(0xa4aab5),
            accent: c(0x2f5fd0),
            accent_hover: c(0x2852b8),
            accent_pressed: c(0x21459e),
            on_accent: c(0xffffff),
            accent_text: c(0x2a56c0),
            control: c(0xe9ecf1),
            control_hover: c(0xdfe3ea),
            control_pressed: c(0xd2d7e0),
            selection: c(0xcfdcfa),
            on_selection: c(0x1b2230),
            focus: c(0x2f5fd0),
            error: c(0xc0262d),
            warning: c(0x8a5200),
            success: c(0x1f7a3d),
            shadow: SHADOW_LIGHT,
        },
        shape(6, 1, true),
    ),
    dark: Theme::from_parts(
        Colors {
            surface: c(0x14171c),
            surface_raised: c(0x1c2028),
            surface_sunken: c(0x0f1115),
            surface_overlay: c(0x232833),
            border: c(0x2c323d),
            control_border: c(0x767f8f),
            text: c(0xe8ecf2),
            text_muted: c(0x9ba4b3),
            text_disabled: c(0x545c69),
            accent: c(0x6b9bff),
            accent_hover: c(0x85adff),
            accent_pressed: c(0x9fbfff),
            on_accent: c(0x0d1526),
            accent_text: c(0x7ba6ff),
            control: c(0x262b35),
            control_hover: c(0x2f3541),
            control_pressed: c(0x3a414f),
            selection: c(0x24395f),
            on_selection: c(0xe8ecf2),
            focus: c(0x6b9bff),
            error: c(0xff7b7b),
            warning: c(0xf0b44c),
            success: c(0x5fce88),
            shadow: SHADOW_DARK,
        },
        shape(6, 1, true),
    ),
};

/// The `studio` pair: see [`BuiltInTheme::Studio`].
const STUDIO: ThemePair = ThemePair {
    light: Theme::from_parts(
        Colors {
            surface: c(0xececea),
            surface_raised: c(0xf6f6f4),
            surface_sunken: c(0xe2e2df),
            surface_overlay: c(0xfafaf8),
            border: c(0xcfcfcb),
            control_border: c(0x7a7a74),
            text: c(0x1a1a19),
            text_muted: c(0x585854),
            text_disabled: c(0xa3a39e),
            accent: c(0xc96000),
            accent_hover: c(0xd96b08),
            accent_pressed: c(0xe57a14),
            on_accent: c(0x000000),
            accent_text: c(0xa34d00),
            control: c(0xdededa),
            control_hover: c(0xd3d3ce),
            control_pressed: c(0xc5c5bf),
            selection: c(0xffd9b3),
            on_selection: c(0x1a1a19),
            focus: c(0x1a1a19),
            error: c(0xb82020),
            warning: c(0x7a4f00),
            success: c(0x2b6b2b),
            shadow: SHADOW_LIGHT,
        },
        shape(3, 1, true),
    ),
    dark: Theme::from_parts(
        Colors {
            surface: c(0x1c1c1e),
            surface_raised: c(0x26262a),
            surface_sunken: c(0x151517),
            surface_overlay: c(0x2e2e33),
            border: c(0x38383d),
            control_border: c(0x7c7c84),
            text: c(0xededed),
            text_muted: c(0xa2a2a8),
            text_disabled: c(0x56565c),
            accent: c(0xff8a1f),
            accent_hover: c(0xff9c42),
            accent_pressed: c(0xffae63),
            on_accent: c(0x000000),
            accent_text: c(0xff9433),
            control: c(0x303035),
            control_hover: c(0x3a3a40),
            control_pressed: c(0x45454c),
            selection: c(0x5c3510),
            on_selection: c(0xffffff),
            focus: c(0xff8a1f),
            error: c(0xff6e6e),
            warning: c(0xffc04d),
            success: c(0x6ad46a),
            shadow: SHADOW_DARK,
        },
        shape(3, 1, true),
    ),
};

/// The `forest` pair: see [`BuiltInTheme::Forest`].
const FOREST: ThemePair = ThemePair {
    light: Theme::from_parts(
        Colors {
            surface: c(0xf3f5f0),
            surface_raised: c(0xfafbf8),
            surface_sunken: c(0xe9ede4),
            surface_overlay: c(0xffffff),
            border: c(0xd3dacb),
            control_border: c(0x78826e),
            text: c(0x1d2a1f),
            text_muted: c(0x536152),
            text_disabled: c(0xa2ab9e),
            accent: c(0x2f6b3f),
            accent_hover: c(0x285c36),
            accent_pressed: c(0x214d2d),
            on_accent: c(0xffffff),
            accent_text: c(0x2c653b),
            control: c(0xe3e8dc),
            control_hover: c(0xd9dfd0),
            control_pressed: c(0xccd4c1),
            selection: c(0xcde3c8),
            on_selection: c(0x1d2a1f),
            focus: c(0x2f6b3f),
            error: c(0xb02a2a),
            warning: c(0x7c5200),
            success: c(0x2f6b3f),
            shadow: SHADOW_LIGHT,
        },
        shape(8, 1, true),
    ),
    dark: Theme::from_parts(
        Colors {
            surface: c(0x141a15),
            surface_raised: c(0x1b231c),
            surface_sunken: c(0x0f1410),
            surface_overlay: c(0x222c23),
            border: c(0x2c382d),
            control_border: c(0x728070),
            text: c(0xe4ece2),
            text_muted: c(0x9cab9a),
            text_disabled: c(0x515e50),
            accent: c(0x7fc28e),
            accent_hover: c(0x95cfa2),
            accent_pressed: c(0xabdbb5),
            on_accent: c(0x0f1a12),
            accent_text: c(0x86c794),
            control: c(0x243026),
            control_hover: c(0x2d3a2f),
            control_pressed: c(0x374639),
            selection: c(0x2e4a33),
            on_selection: c(0xe4ece2),
            focus: c(0x7fc28e),
            error: c(0xff8080),
            warning: c(0xe8c060),
            success: c(0x7fc28e),
            shadow: SHADOW_DARK,
        },
        shape(8, 1, true),
    ),
};

/// The `lagoon` pair: see [`BuiltInTheme::Lagoon`].
const LAGOON: ThemePair = ThemePair {
    light: Theme::from_parts(
        Colors {
            surface: c(0xeef6f6),
            surface_raised: c(0xf8fcfc),
            surface_sunken: c(0xe3eff0),
            surface_overlay: c(0xffffff),
            border: c(0xc9dfe0),
            control_border: c(0x6b8689),
            text: c(0x0f2a2e),
            text_muted: c(0x4a6468),
            text_disabled: c(0x9db3b5),
            accent: c(0x007c80),
            accent_hover: c(0x006a6e),
            accent_pressed: c(0x00595c),
            on_accent: c(0xffffff),
            accent_text: c(0x00767a),
            control: c(0xdcebec),
            control_hover: c(0xd0e4e5),
            control_pressed: c(0xc1dadc),
            selection: c(0xb8e3e4),
            on_selection: c(0x0f2a2e),
            focus: c(0x007c80),
            error: c(0xb3261e),
            warning: c(0x7a5000),
            success: c(0x1f6f3a),
            shadow: SHADOW_LIGHT,
        },
        shape(4, 1, true),
    ),
    dark: Theme::from_parts(
        Colors {
            surface: c(0x0b1d20),
            surface_raised: c(0x10272b),
            surface_sunken: c(0x071518),
            surface_overlay: c(0x153136),
            border: c(0x1f3d42),
            control_border: c(0x5f8a90),
            text: c(0xe2f2f2),
            text_muted: c(0x93b5b8),
            text_disabled: c(0x4a6a6e),
            accent: c(0x3cc8c8),
            accent_hover: c(0x5fd4d4),
            accent_pressed: c(0x82dfdf),
            on_accent: c(0x04191b),
            accent_text: c(0x4fd0d0),
            control: c(0x163338),
            control_hover: c(0x1d3e44),
            control_pressed: c(0x254a51),
            selection: c(0x14535a),
            on_selection: c(0xe2f2f2),
            focus: c(0x3cc8c8),
            error: c(0xff8a80),
            warning: c(0xf2c25c),
            success: c(0x6fd49a),
            shadow: SHADOW_DARK,
        },
        shape(4, 1, true),
    ),
};

/// The `nocturne` pair: see [`BuiltInTheme::Nocturne`].
const NOCTURNE: ThemePair = ThemePair {
    light: Theme::from_parts(
        Colors {
            surface: c(0xf6f4fb),
            surface_raised: c(0xfcfbfe),
            surface_sunken: c(0xece8f5),
            surface_overlay: c(0xffffff),
            border: c(0xdcd6ea),
            control_border: c(0x80799a),
            text: c(0x211a33),
            text_muted: c(0x5d5675),
            text_disabled: c(0xaaa4bd),
            accent: c(0x6a3fd0),
            accent_hover: c(0x5c35b8),
            accent_pressed: c(0x4f2c9e),
            on_accent: c(0xffffff),
            accent_text: c(0x6238c4),
            control: c(0xe8e3f3),
            control_hover: c(0xddd6ee),
            control_pressed: c(0xd0c7e6),
            selection: c(0xdccffa),
            on_selection: c(0x211a33),
            focus: c(0x6a3fd0),
            error: c(0xb3203f),
            warning: c(0x7d4f00),
            success: c(0x26703f),
            shadow: SHADOW_LIGHT,
        },
        shape(10, 1, true),
    ),
    dark: Theme::from_parts(
        Colors {
            surface: c(0x15121f),
            surface_raised: c(0x1d192b),
            surface_sunken: c(0x100d18),
            surface_overlay: c(0x262138),
            border: c(0x312a47),
            control_border: c(0x7d7499),
            text: c(0xece8f6),
            text_muted: c(0xa59dbf),
            text_disabled: c(0x5a5372),
            accent: c(0xb18cff),
            accent_hover: c(0xc0a2ff),
            accent_pressed: c(0xcfb8ff),
            on_accent: c(0x1a1030),
            accent_text: c(0xb996ff),
            control: c(0x29233b),
            control_hover: c(0x332c49),
            control_pressed: c(0x3e3657),
            selection: c(0x3d2d6e),
            on_selection: c(0xece8f6),
            focus: c(0xb18cff),
            error: c(0xff7d9a),
            warning: c(0xf0bf5a),
            success: c(0x6fd394),
            shadow: SHADOW_DARK,
        },
        shape(10, 1, true),
    ),
};

/// The `snow` pair: see [`BuiltInTheme::Snow`].
const SNOW: ThemePair = ThemePair {
    light: Theme::from_parts(
        Colors {
            surface: c(0xfaf9fc),
            surface_raised: c(0xffffff),
            surface_sunken: c(0xf2f0f6),
            surface_overlay: c(0xffffff),
            border: c(0xe6e2ee),
            control_border: c(0x8a8499),
            text: c(0x2a2633),
            text_muted: c(0x625c70),
            text_disabled: c(0xb4afbf),
            accent: c(0xc2185b),
            accent_hover: c(0xa8144f),
            accent_pressed: c(0x8f1143),
            on_accent: c(0xffffff),
            accent_text: c(0xb5165a),
            control: c(0xefecf4),
            control_hover: c(0xe6e2ee),
            control_pressed: c(0xdad5e5),
            selection: c(0xf8d3e2),
            on_selection: c(0x2a2633),
            focus: c(0xc2185b),
            error: c(0xb3261e),
            warning: c(0x7a4f00),
            success: c(0x25703c),
            shadow: SHADOW_LIGHT,
        },
        shape(12, 2, true),
    ),
    dark: Theme::from_parts(
        Colors {
            surface: c(0x1e1b24),
            surface_raised: c(0x26222e),
            surface_sunken: c(0x18161d),
            surface_overlay: c(0x2e2a38),
            border: c(0x3a3545),
            control_border: c(0x857e95),
            text: c(0xf0edf5),
            text_muted: c(0xaaa3b8),
            text_disabled: c(0x5e5869),
            accent: c(0xff6fa6),
            accent_hover: c(0xff8ab6),
            accent_pressed: c(0xffa5c6),
            on_accent: c(0x2a0a18),
            accent_text: c(0xff7aad),
            control: c(0x322d3c),
            control_hover: c(0x3b3547),
            control_pressed: c(0x463f54),
            selection: c(0x5a2440),
            on_selection: c(0xf0edf5),
            focus: c(0xff6fa6),
            error: c(0xff8a80),
            warning: c(0xf2c25c),
            success: c(0x6fd49a),
            shadow: SHADOW_DARK,
        },
        shape(12, 2, true),
    ),
};
