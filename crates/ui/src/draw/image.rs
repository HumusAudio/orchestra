// SPDX-License-Identifier: GPL-3.0-or-later

//! The RGBA image type: the headless target of the renderer and the pictures a list can draw
//! (ADR-0039 D4, D8), its PNG encoding for the screenshots (ADR-0041), and the error type of the
//! drawing module.

use std::fmt;

use tiny_skia::PixmapRef;

/// An image of `width × height` pixels, 8 bits per channel, RGBA, **premultiplied** alpha, rows
/// top to bottom without padding (`4 × width` bytes per row).
///
/// # Why premultiplied
/// It is what `tiny-skia` draws into and what compositing needs (blending premultiplied colours
/// is one multiply-add per channel). It also means a pixel can be read back exactly as the
/// renderer left it, which is what the pixel tests compare. Converting to the window's format
/// (`softbuffer` wants `0RGB` in a `u32`) happens once, when presenting (F2.04).
#[derive(Clone, PartialEq, Eq)]
pub struct Image {
    /// Width in pixels, ≥ 1.
    width: u32,
    /// Height in pixels, ≥ 1.
    height: u32,
    /// `4 × width × height` bytes.
    data: Vec<u8>,
}

impl Image {
    /// A fully transparent image of the given size.
    ///
    /// # Errors
    /// [`DrawError::InvalidSize`] for a zero side, or a size whose byte count does not fit in
    /// memory addresses (`4 × width × height` overflowing `usize`).
    pub fn new(width: u32, height: u32) -> Result<Self, DrawError> {
        let len = byte_len(width, height)?;
        Ok(Self {
            width,
            height,
            data: vec![0; len],
        })
    }

    /// An image made from premultiplied RGBA bytes.
    ///
    /// # Errors
    /// [`DrawError::InvalidSize`] as for [`Image::new`]; [`DrawError::WrongDataLength`] when
    /// `data` is not exactly `4 × width × height` bytes.
    pub fn from_premultiplied_rgba(
        width: u32,
        height: u32,
        data: Vec<u8>,
    ) -> Result<Self, DrawError> {
        let expected = byte_len(width, height)?;
        if data.len() != expected {
            return Err(DrawError::WrongDataLength {
                expected,
                actual: data.len(),
            });
        }
        Ok(Self {
            width,
            height,
            data,
        })
    }

    /// Width in pixels.
    #[must_use]
    pub const fn width(&self) -> u32 {
        self.width
    }

    /// Height in pixels.
    #[must_use]
    pub const fn height(&self) -> u32 {
        self.height
    }

    /// The pixels, premultiplied RGBA, row by row.
    #[must_use]
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// The pixels, mutable: for the renderer.
    pub(crate) fn data_mut(&mut self) -> &mut [u8] {
        &mut self.data
    }

    /// The pixel at `(x, y)` as premultiplied `[r, g, b, a]`, or `None` outside the image.
    #[must_use]
    pub fn pixel(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        // Both factors are below the image size, whose byte count was checked at construction,
        // so this cannot overflow.
        let index = (y as usize * self.width as usize + x as usize) * 4;
        let bytes = self.data.get(index..index + 4)?;
        let mut pixel = [0; 4];
        pixel.copy_from_slice(bytes);
        Some(pixel)
    }

    /// Encodes the image as a PNG file in memory: 8-bit RGBA, **straight** (not premultiplied)
    /// alpha, as the PNG format requires. Used by `cargo xtask screenshots` (ADR-0041).
    ///
    /// # How
    /// `tiny-skia`'s own encoder (feature `png-format`, which brings the `png` crate) does the
    /// work: it divides every colour channel by alpha ("demultiplies") and writes the rows with
    /// the `png` crate's default compression. We wrap our bytes in a borrowed `PixmapRef`, so no
    /// copy is made on our side; the encoder makes one demultiplied copy of its own.
    ///
    /// # Exactness
    /// For **opaque** pixels (alpha 255) premultiplied and straight values are the same bytes, so
    /// decoding the PNG gives back exactly this image. Translucent pixels lose a little precision
    /// in the division and may come back a unit or two off: harmless for a screenshot, which is
    /// opaque anyway (every scene paints its background).
    ///
    /// # Complexity
    /// O(width × height) time, and one extra image-sized buffer while encoding.
    ///
    /// # Errors
    /// [`DrawError::PngEncoding`] if the encoder fails. Writing into memory cannot run out of
    /// disk, so in practice this means the encoder rejected the size (`png` limits a side to
    /// 2³¹ − 1 pixels, larger than any [`Image`] we can allocate).
    pub fn encode_png(&self) -> Result<Vec<u8>, DrawError> {
        // `from_bytes` checks that the length matches `4 × width × height`, which our
        // constructors already guarantee; the `None` branch is therefore unreachable, but it is
        // still reported as an error rather than a panic (no panics in library code).
        let pixmap = PixmapRef::from_bytes(&self.data, self.width, self.height).ok_or(
            DrawError::WrongDataLength {
                expected: byte_len(self.width, self.height)?,
                actual: self.data.len(),
            },
        )?;
        pixmap.encode_png().map_err(|error| DrawError::PngEncoding {
            message: error.to_string(),
        })
    }
}

impl fmt::Debug for Image {
    /// The size only: printing megabytes of pixels helps nobody.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Image")
            .field("width", &self.width)
            .field("height", &self.height)
            .finish_non_exhaustive()
    }
}

/// The byte count of a `width × height` RGBA image.
///
/// # Errors
/// [`DrawError::InvalidSize`] for a zero side or an overflow.
fn byte_len(width: u32, height: u32) -> Result<usize, DrawError> {
    let invalid = DrawError::InvalidSize { width, height };
    if width == 0 || height == 0 {
        return Err(invalid);
    }
    (width as usize)
        .checked_mul(height as usize)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or(invalid)
}

/// A picture registered in a display list's image store, drawn with
/// [`Recorder::image`](crate::Recorder::image).
///
/// Opaque and only meaningful for the list that returned it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ImageId(pub(crate) u32);

/// What can go wrong in the drawing module. Exhaustive (ADR-0034 D2).
///
/// `PartialEq` only (not `Eq`): [`DrawError::InvalidScale`] carries the `f32` received, NaN
/// included, which is not equal to itself.
#[derive(Debug, Clone, PartialEq)]
pub enum DrawError {
    /// An image or a target of zero width or height, or too large to address.
    InvalidSize {
        /// The requested width.
        width: u32,
        /// The requested height.
        height: u32,
    },
    /// Image data whose length does not match its size.
    WrongDataLength {
        /// `4 × width × height`.
        expected: usize,
        /// The length received.
        actual: usize,
    },
    /// The scale factor is not positive and finite.
    InvalidScale {
        /// The scale received.
        scale: f32,
    },
    /// The display list holds more images than an [`ImageId`] can number (2³² − 1).
    TooManyImages,
    /// The PNG encoder failed ([`Image::encode_png`]).
    PngEncoding {
        /// The encoder's own description, in English. Kept as text because the `png` crate's
        /// error is neither `Clone` nor `PartialEq` and is not part of our public API (ADR-0034).
        message: String,
    },
}

impl fmt::Display for DrawError {
    /// One English line for developers (the interface never shows these).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSize { width, height } => {
                write!(f, "invalid image size {width} × {height}")
            }
            Self::WrongDataLength { expected, actual } => {
                write!(f, "image data of {actual} bytes, expected {expected}")
            }
            Self::InvalidScale { scale } => write!(f, "invalid scale factor {scale}"),
            Self::TooManyImages => write!(f, "too many images in one display list"),
            Self::PngEncoding { message } => write!(f, "encoding the PNG failed: {message}"),
        }
    }
}

impl std::error::Error for DrawError {}
