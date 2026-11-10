// SPDX-License-Identifier: GPL-3.0-or-later

//! The colour value every theme token holds (ADR-0037 T2).
//!
//! # Why sRGB with 8 bits per channel and straight alpha?
//! It is the format `tiny-skia` and `softbuffer` work in, so a colour goes from the theme to the
//! renderer without any conversion, and it is the format a person types (`#rrggbbaa`) in the
//! theme editor of step F2.12b. Linear floating point was rejected in ADR-0037 T2: four times the
//! memory and a conversion at every draw, useful only to the GPU path, which can convert once
//! when a theme is loaded.

/// A colour in the sRGB encoding, 8 bits per channel, with **straight** (not premultiplied)
/// alpha.
///
/// "Straight" means that `r`, `g` and `b` are the colour as it would look fully opaque, and `a`
/// says how much of it covers what lies behind (0 = invisible, 255 = opaque). Premultiplied
/// alpha (the channels already multiplied by `a`) is what a renderer blends with internally; the
/// conversion belongs to the renderer (F2.03), not to the theme, because a person editing a theme
/// thinks in straight colours.
///
/// Four bytes, `Copy`: themes hold them by value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Color {
    /// Red channel, sRGB encoded, 0–255.
    pub r: u8,
    /// Green channel, sRGB encoded, 0–255.
    pub g: u8,
    /// Blue channel, sRGB encoded, 0–255.
    pub b: u8,
    /// Coverage, 0 (transparent) to 255 (opaque), straight (not premultiplied).
    pub a: u8,
}

impl Color {
    /// Opaque black, `#000000`.
    pub const BLACK: Self = Self::rgb(0x00_0000);
    /// Opaque white, `#ffffff`.
    pub const WHITE: Self = Self::rgb(0xff_ffff);

    /// An opaque colour from a `0xRRGGBB` literal, the way colours are written in CSS and in the
    /// theme comparison page: `Color::rgb(0x1c1c1e)` is `#1c1c1e`.
    ///
    /// Bits above the lowest 24 are ignored, so a literal with a stray alpha byte cannot shift
    /// the channels.
    #[must_use]
    pub const fn rgb(hex: u32) -> Self {
        // Each channel is one byte of the literal: red in bits 16–23, green in 8–15, blue in 0–7.
        // `to_be_bytes` gives [ignored, r, g, b] for a big-endian reading of the u32, which is
        // exactly how the literal is written; destructuring avoids any lossy cast.
        let [_, r, g, b] = hex.to_be_bytes();
        Self {
            r,
            g,
            b,
            a: u8::MAX,
        }
    }

    /// A colour from a `0xRRGGBBAA` literal: `Color::rgba(0x0000002e)` is black at 46/255
    /// (≈ 18%) coverage, the light shadow of ADR-0037 T3.
    #[must_use]
    pub const fn rgba(hex: u32) -> Self {
        // Same reasoning as `rgb`, with the alpha byte last, as in CSS `#rrggbbaa`.
        let [r, g, b, a] = hex.to_be_bytes();
        Self { r, g, b, a }
    }

    /// Whether the colour fully covers what lies behind it (`a == 255`).
    #[must_use]
    pub const fn is_opaque(self) -> bool {
        self.a == u8::MAX
    }

    /// This colour composited over an opaque `background` ("source over"), as it appears on
    /// screen. An opaque colour is returned unchanged; `background`'s own alpha is ignored
    /// (it is treated as opaque), because every background of a contrast pair is a surface or a
    /// filled control, opaque by construction (checked by the theme tests).
    ///
    /// # Why blend in sRGB, not in linear light?
    /// `tiny-skia`, browsers and most 2D renderers blend the encoded sRGB values directly. The
    /// contrast check must judge the colour the user actually sees, so it blends the same way
    /// the renderer does. (Linear blending is more physically correct, but would make the check
    /// disagree with the screen.)
    ///
    /// # Rounding
    /// Integer arithmetic, rounded to the nearest value: `(f·a + b·(255 − a) + 127) / 255`. The
    /// numerator is at most 255 · 255 + 127 = 65 152, so `u32` never overflows, and the result
    /// is at most 255, so the conversion back to `u8` cannot fail (it falls back to 255 if it
    /// ever did, which is the correct saturated value).
    #[must_use]
    pub fn over(self, background: Self) -> Self {
        if self.is_opaque() {
            return self;
        }
        let alpha = u32::from(self.a);
        let blend = |fg: u8, bg: u8| -> u8 {
            let mixed = (u32::from(fg) * alpha + u32::from(bg) * (255 - alpha) + 127) / 255;
            u8::try_from(mixed).unwrap_or(u8::MAX)
        };
        Self {
            r: blend(self.r, background.r),
            g: blend(self.g, background.g),
            b: blend(self.b, background.b),
            a: u8::MAX,
        }
    }
}
