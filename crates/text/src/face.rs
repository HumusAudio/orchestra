// SPDX-License-Identifier: GPL-3.0-or-later

//! The bundled faces (ADR-0044 TX3): which fonts are inside the program, and the object that
//! holds them parsed.
//!
//! # Why a closed enum
//! The program draws only with the fonts it carries (ADR-0006: the same look everywhere), so the
//! set of faces is known at compile time. An enum makes a face a `Copy` value of one byte that
//! can sit in every glyph run and every cache key, and an exhaustive `match` reminds every user
//! of the set when a face is added.
//!
//! # Why no global state
//! The parsed faces live in a [`Faces`] value that the application creates once and passes by
//! reference (owner's choice TB2, 2026-11-12). A lazily initialised static would hide when the
//! parsing happens and who owns it, and would need `once_cell` or `OnceLock` in every reader.

use skrifa::Font;

use crate::error::TextError;

/// One of the faces embedded in the program (ADR-0044 TX3).
///
/// The discriminants are stable, small and dense (0..[`FaceId::COUNT`]), so a face can index the
/// array of parsed fonts in [`Faces`] directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FaceId {
    /// Inter Regular (weight 400): the interface text.
    InterRegular = 0,
    /// Inter SemiBold (weight 600): labels and emphasis.
    InterSemiBold = 1,
    /// Inter Bold (weight 700): headings.
    InterBold = 2,
    /// Bravura, the SMuFL reference music font: musical symbols in the interface and the score.
    Bravura = 3,
    /// JetBrains Mono Regular: code-like text (paths, error details).
    JetBrainsMono = 4,
}

impl FaceId {
    /// How many faces there are.
    pub const COUNT: usize = 5;

    /// Every face, in discriminant order (so `FaceId::ALL[f.index()] == f`).
    pub const ALL: [Self; Self::COUNT] = [
        Self::InterRegular,
        Self::InterSemiBold,
        Self::InterBold,
        Self::Bravura,
        Self::JetBrainsMono,
    ];

    /// The face's position in [`FaceId::ALL`] and in the array of parsed fonts.
    #[must_use]
    pub const fn index(self) -> usize {
        // The discriminants are 0..COUNT by construction (see the enum), so the cast is the
        // index; `as` on a field-less enum reads its discriminant and cannot truncate here.
        self as usize
    }

    /// The face's file bytes, embedded in the binary from `assets/fonts/` unchanged.
    ///
    /// # Why unchanged
    /// The OFL forbids distributing modified copies under the fonts' reserved names, and the
    /// guides use the same files; we never subset or convert them (ADR-0044 TX3).
    const fn data(self) -> &'static [u8] {
        match self {
            Self::InterRegular => {
                include_bytes!("../../../assets/fonts/inter/Inter-Regular.otf")
            }
            Self::InterSemiBold => {
                include_bytes!("../../../assets/fonts/inter/Inter-SemiBold.otf")
            }
            Self::InterBold => include_bytes!("../../../assets/fonts/inter/Inter-Bold.otf"),
            Self::Bravura => include_bytes!("../../../assets/fonts/bravura/Bravura.otf"),
            Self::JetBrainsMono => {
                include_bytes!("../../../assets/fonts/jetbrains-mono/JetBrainsMono-Regular.ttf")
            }
        }
    }
}

/// The bundled faces, parsed once (ADR-0044 TX3).
///
/// Parsing reads only each file's table directory; the tables themselves are read lazily by
/// `skrifa` and `harfrust` the first time they are needed, so creating a `Faces` is cheap
/// (microseconds) and needs no I/O: the bytes are inside the binary.
///
/// A `skrifa::Font` is a reference-counted handle, so `Faces` is cheap to clone too, but the
/// intended use is one value owned by the application and lent to every caller.
#[derive(Clone)]
pub struct Faces {
    /// The parsed fonts, indexed by [`FaceId::index`].
    fonts: [Font; FaceId::COUNT],
}

impl std::fmt::Debug for Faces {
    /// `skrifa::Font` has no `Debug`; printing the faces' names is what a reader of a test
    /// failure needs anyway.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Faces")
            .field("faces", &FaceId::ALL)
            .finish_non_exhaustive()
    }
}

impl Faces {
    /// Parses the bundled faces.
    ///
    /// # Errors
    /// [`TextError::FontUnreadable`] if a face's bytes are not a font `skrifa` can read. With the
    /// files in the repository this never happens (a test checks every face), but library code
    /// must not panic (CLAUDE.md §4), so the impossible case is still a typed error.
    pub fn bundled() -> Result<Self, TextError> {
        // Parse in `ALL` order, so position i holds the face whose index is i.
        let parse = |face: FaceId| Font::new(face.data(), 0).ok_or(TextError::FontUnreadable(face));
        Ok(Self {
            fonts: [
                parse(FaceId::InterRegular)?,
                parse(FaceId::InterSemiBold)?,
                parse(FaceId::InterBold)?,
                parse(FaceId::Bravura)?,
                parse(FaceId::JetBrainsMono)?,
            ],
        })
    }

    /// The parsed font of `face`, for the other modules of the crate.
    pub(crate) fn font(&self, face: FaceId) -> &Font {
        // Destructuring the array and matching every face is checked by the compiler on both
        // sides: the pattern must have exactly `COUNT` elements and the match must cover every
        // face. No index expression, so nothing here can panic; O(1).
        let [inter_regular, inter_semibold, inter_bold, bravura, mono] = &self.fonts;
        match face {
            FaceId::InterRegular => inter_regular,
            FaceId::InterSemiBold => inter_semibold,
            FaceId::InterBold => inter_bold,
            FaceId::Bravura => bravura,
            FaceId::JetBrainsMono => mono,
        }
    }

    /// Design units per em of `face` (2048 for Inter, 1000 for Bravura and JetBrains Mono).
    #[must_use]
    pub fn units_per_em(&self, face: FaceId) -> u16 {
        self.font(face).units_per_em()
    }

    /// The glyph `face` maps the character `c` to, or `None` if the face does not have it.
    ///
    /// This is a direct `cmap` lookup without shaping: the way SMuFL symbols are found in
    /// Bravura (their code points are in the Private Use Area, U+E000–U+F8FF), and the way the
    /// layout finds the hyphen it adds at a soft hyphen (TB6).
    ///
    /// # Complexity
    /// O(log n) in the number of `cmap` segments (a binary search inside `skrifa`).
    #[must_use]
    pub fn glyph_for_char(&self, face: FaceId, c: char) -> Option<crate::GlyphId> {
        self.font(face)
            .charmap()
            .map_unicode(c)
            .map(|glyph| crate::GlyphId(glyph.to_u32()))
    }

    /// The horizontal advance of `glyph` in `face`, in logical pixels at `size` pixels per em.
    ///
    /// For glyphs placed one by one (SMuFL symbols); shaped text gets its advances from the
    /// shaper, which also applies kerning.
    ///
    /// # Errors
    /// [`TextError::InvalidSize`] if `size` is not finite and positive.
    pub fn advance(
        &self,
        face: FaceId,
        glyph: crate::GlyphId,
        size: f32,
    ) -> Result<f32, TextError> {
        let scale = crate::style::checked_scale(self, face, size)?;
        let advance = self
            .font(face)
            .glyph_metrics()
            .h_advance(skrifa::GlyphId::new(glyph.0));
        Ok(advance * scale)
    }
}
