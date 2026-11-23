// SPDX-License-Identifier: GPL-3.0-or-later

//! The contrast check of ADR-0014 §3 and ADR-0037 T4: the WCAG 2.x contrast ratio and the table
//! of token pairs that must reach it.
//!
//! The same public function serves the tests of the built-in themes (ADR-0037 T5) and, later, the
//! live warnings of the theme editor (T8), so the program and its tests can never disagree on
//! what "passes" means.

use crate::theme::color::Color;
use crate::theme::token::{ColorToken, Theme};

/// How much contrast a pair needs, by what its foreground is (WCAG 2.2, level AA).
///
/// Exhaustive (ADR-0034 D2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ContrastKind {
    /// Text and its background: at least **4.5:1** (WCAG 1.4.3). We do not use the 3:1 allowance
    /// for large text: a token does not know the size of the text drawn with it.
    Text,
    /// Graphics a user needs to see (a control's boundary, a state shown by the accent, the focus
    /// ring) against what is next to them: at least **3:1** (WCAG 1.4.11).
    Graphic,
}

impl ContrastKind {
    /// The minimum contrast ratio for this kind: 4.5 or 3.0.
    #[must_use]
    pub const fn minimum(self) -> f64 {
        match self {
            Self::Text => 4.5,
            Self::Graphic => 3.0,
        }
    }
}

/// One pair of tokens that must contrast: `foreground` drawn on `background`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ContrastPair {
    /// The token drawn on top (text, a border, the focus ring…).
    pub foreground: ColorToken,
    /// The token it is drawn on (a surface, a control, the accent…). Opaque in every built-in
    /// instance (checked by the theme tests).
    pub background: ColorToken,
    /// Which threshold applies.
    pub kind: ContrastKind,
}

/// Shorthand for the table below: a *text* pair.
const fn text(foreground: ColorToken, background: ColorToken) -> ContrastPair {
    ContrastPair {
        foreground,
        background,
        kind: ContrastKind::Text,
    }
}

/// Shorthand for the table below: a *graphic* pair.
const fn graphic(foreground: ColorToken, background: ColorToken) -> ContrastPair {
    ContrastPair {
        foreground,
        background,
        kind: ContrastKind::Graphic,
    }
}

/// The 31 pairs of ADR-0037 T4, the only place where they are listed.
///
/// # How the list was chosen
/// Each pair is a combination the interface really draws: a foreground token on every background
/// it can appear on. Pairs that never occur (for example `on-accent` on a surface) are left out,
/// because requiring them would forbid good palettes for no visible benefit. A token added later
/// enters this table in the same commit, or the exempt list with a reason; the test "every token
/// is covered" fails otherwise.
pub const CONTRAST_PAIRS: &[ContrastPair] = {
    use ColorToken as T;
    &[
        // Normal text on everything it is written on.
        text(T::Text, T::Surface),
        text(T::Text, T::SurfaceRaised),
        text(T::Text, T::SurfaceSunken),
        text(T::Text, T::SurfaceOverlay),
        text(T::Text, T::Control),
        text(T::Text, T::ControlHover),
        text(T::Text, T::ControlPressed),
        // Secondary text on the four surfaces (labels, menu shortcuts, the status bar).
        text(T::TextMuted, T::Surface),
        text(T::TextMuted, T::SurfaceRaised),
        text(T::TextMuted, T::SurfaceSunken),
        text(T::TextMuted, T::SurfaceOverlay),
        // The accent as text (links) and the state colours, on the two main surfaces.
        text(T::AccentText, T::Surface),
        text(T::AccentText, T::SurfaceRaised),
        // Text on the accent, in all three of its states.
        text(T::OnAccent, T::Accent),
        text(T::OnAccent, T::AccentHover),
        text(T::OnAccent, T::AccentPressed),
        // Selected text.
        text(T::OnSelection, T::Selection),
        // Messages.
        text(T::Error, T::Surface),
        text(T::Error, T::SurfaceRaised),
        text(T::Warning, T::Surface),
        text(T::Warning, T::SurfaceRaised),
        text(T::Success, T::Surface),
        text(T::Success, T::SurfaceRaised),
        // The edge of a control on the surfaces controls sit on.
        graphic(T::ControlBorder, T::Surface),
        graphic(T::ControlBorder, T::SurfaceRaised),
        graphic(T::ControlBorder, T::SurfaceSunken),
        // The accent as a state indicator (a toggle that is on, a slider's fill).
        graphic(T::Accent, T::Surface),
        graphic(T::Accent, T::SurfaceRaised),
        // The focus ring.
        graphic(T::Focus, T::Surface),
        graphic(T::Focus, T::SurfaceRaised),
        graphic(T::Focus, T::SurfaceSunken),
    ]
};

/// The tokens exempt from the contrast pairs, as WCAG allows (ADR-0037 T4): decorative lines,
/// inactive controls, shadows.
pub const EXEMPT_TOKENS: &[ColorToken] = &[
    ColorToken::Border,
    ColorToken::TextDisabled,
    ColorToken::Shadow,
];

/// The relative luminance of an opaque colour, WCAG 2.x definition: 0.0 (black) to 1.0 (white).
///
/// Each channel is decoded from sRGB to linear light (`c / 12.92` below the threshold 0.04045,
/// `((c + 0.055) / 1.055)^2.4` above it), then weighted by the eye's sensitivity: 0.2126 red,
/// 0.7152 green, 0.0722 blue. The alpha channel is ignored: composite first ([`Color::over`]).
///
/// # Why `f64`?
/// This is interface code, not musical time, so ADR-0005's exact arithmetic does not apply; the
/// power 2.4 has no exact rational form anyway. `f64` keeps the value well within the precision
/// needed to compare with 4.5 and 3.0.
#[must_use]
pub fn relative_luminance(color: Color) -> f64 {
    /// Decodes one 8-bit sRGB channel to linear light, 0.0–1.0.
    fn linear(channel: u8) -> f64 {
        let c = f64::from(channel) / 255.0;
        if c <= 0.040_45 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    }
    0.2126 * linear(color.r) + 0.7152 * linear(color.g) + 0.0722 * linear(color.b)
}

/// The WCAG 2.x contrast ratio of `foreground` drawn on `background`: 1.0 (identical) to 21.0
/// (black on white). Symmetric in the two colours once composited.
///
/// A translucent `foreground` is first composited over `background` ([`Color::over`]), so the
/// ratio judges what is actually on screen; `background` is treated as opaque.
///
/// The value is not rounded: the comparison with a threshold is made on the computed ratio, so a
/// pair at 4.499 fails even though it would print as "4.50" (ADR-0037 T4).
#[must_use]
pub fn contrast_ratio(foreground: Color, background: Color) -> f64 {
    let lighter = relative_luminance(foreground.over(background));
    let darker = relative_luminance(background);
    // The formula needs the lighter luminance on top; `max`/`min` make the order irrelevant.
    let (hi, lo) = (lighter.max(darker), lighter.min(darker));
    (hi + 0.05) / (lo + 0.05)
}

/// A pair of [`CONTRAST_PAIRS`] that an instance does not satisfy, with the ratio it reaches.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ContrastFailure {
    /// The pair that fails.
    pub pair: ContrastPair,
    /// The ratio reached, below `pair.kind.minimum()`.
    pub ratio: f64,
}

impl Theme {
    /// Every pair of [`CONTRAST_PAIRS`] this instance fails, in table order.
    ///
    /// Lazy and allocation-free: the theme editor (F2.12b) can show the failures as the user
    /// edits, and the tests collect them all to print every failure at once. O(p) in the number
    /// of pairs (31).
    pub fn contrast_failures(&self) -> impl Iterator<Item = ContrastFailure> + '_ {
        CONTRAST_PAIRS.iter().filter_map(move |&pair| {
            let ratio = contrast_ratio(self.color(pair.foreground), self.color(pair.background));
            (ratio < pair.kind.minimum()).then_some(ContrastFailure { pair, ratio })
        })
    }
}
