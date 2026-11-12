// SPDX-License-Identifier: GPL-3.0-or-later

//! The screenshots of the user guides (ADR-0041): a catalogue of **scenes**, each rendered headless
//! by the program's own views and renderer, then encoded as PNG by `orchestra-screenshots`.
//!
//! # Why a catalogue
//! Every screenshot in the guides is one entry of [`SCENES`]: a file name, a logical size and a
//! function that brings a view into the state to photograph. Adding a picture to the guides means
//! adding one entry here; renaming one means renaming the file the guides include. Nothing else in
//! the pipeline changes (ADR-0041 S2).
//!
//! # What is fixed for every scene (ADR-0041 S3)
//! - the theme: **Studio, dark half** ([`theme`]), set explicitly and never read from the system,
//!   so the pictures do not depend on the desktop of whoever runs the tool;
//! - the language: en-US (ADR-0009 §6): the texts come from the en-US catalogue, through the
//!   [`SceneContext`] that [`render`] prepares;
//! - the scale: [`SCALE`] device pixels per logical pixel.
//!
//! # How a scene is drawn
//! Exactly as a window would draw its first frame: the view's [`WindowApp::layout`] records the
//! whole interface for the scene's size, then the scene may feed it [`InputEvent`]s (for example
//! to place the pointer), and [`CpuRenderer::render_to_image`] draws the whole list. The window
//! itself, `winit` and `softbuffer` are not involved, so no display is needed (CLAUDE.md §3).

use std::fmt;

use orchestra_i18n::{Locale, Localizer};
use orchestra_text::Faces;
use orchestra_ui::{
    Appearance, BuiltInTheme, CpuRenderer, DisplayList, DrawError, Image, InputEvent, Point, Rect,
    Theme, WindowApp,
};

use crate::Demo;

/// Device pixels per logical pixel in every screenshot (ADR-0041 S3).
///
/// # Why 2
/// A guide page is A4 with about 16 cm of text width. A scene 640 logical pixels wide becomes
/// 1280 device pixels, which printed across the text width is ≈ 200 dpi: sharp in a PDF viewer at
/// any usual zoom and in print. It also exercises the high-DPI path of the renderer (lines snapped
/// to device pixels), which is what most users of recent laptops see. Scale 3 would make every file
/// 2.25 times larger (3² / 2²) for no visible gain.
pub const SCALE: f32 = 2.0;

/// The largest side of a screenshot, in device pixels: far beyond any page, and small enough that
/// a mistyped size in the catalogue is an error instead of gigabytes of pixels.
const MAX_SIDE: f32 = 16_384.0;

/// One screenshot: its file name, its size and how to bring the view into the state shown.
#[derive(Debug, Clone, Copy)]
pub struct Scene {
    /// The file name without extension, in kebab case: the guides include
    /// `/dist/screenshots/<name>.png`. Unique in [`SCENES`] (checked by a test).
    pub name: &'static str,
    /// Width in logical pixels.
    pub width: f32,
    /// Height in logical pixels.
    pub height: f32,
    /// Records the scene into an empty list for `bounds` (origin at the top left, the scene's
    /// size), with the fonts and texts of `context`.
    pub record: fn(bounds: Rect, list: &mut DisplayList, context: &SceneContext),
}

/// What every scene may need besides its size, prepared once by [`render`]: the bundled faces
/// (ADR-0044) and the en-US texts (ADR-0009 §6). Preparing them can fail (in theory: the fonts
/// and catalogues are inside the binary), so [`render`] does it and reports a typed error, and
/// the scenes stay plain functions.
#[derive(Debug, Clone)]
pub struct SceneContext {
    /// The bundled faces.
    pub faces: Faces,
    /// The program's name in en-US (the catalogue's `app-name`).
    pub app_name: String,
}

impl SceneContext {
    /// The faces and the en-US texts.
    ///
    /// # Errors
    /// [`ScreenshotError::Setup`] if the faces or the en-US catalogue cannot be read.
    pub fn new() -> Result<Self, ScreenshotError> {
        let faces = Faces::bundled().map_err(|error| ScreenshotError::Setup {
            detail: error.to_string(),
        })?;
        let app_name = Localizer::new(Locale::EnUs)
            .and_then(|localizer| localizer.format("app-name", None))
            .map_err(|error| ScreenshotError::Setup {
                detail: error.to_string(),
            })?;
        Ok(Self { faces, app_name })
    }
}

impl Scene {
    /// The image size in device pixels: the logical size times [`SCALE`], rounded to the nearest
    /// whole pixel (the scenes use sizes that are exact at scale 2, so nothing is rounded today).
    ///
    /// # Errors
    /// [`ScreenshotError::Size`] when a side is not between 1 and 16 384 device pixels (a mistake
    /// in the catalogue, caught by the tests).
    pub fn device_size(&self) -> Result<(u32, u32), ScreenshotError> {
        let side = |logical: f32| {
            let device = (logical * SCALE).round();
            // `as` would saturate silently; check the range first so a bad entry is an error. NaN
            // fails both comparisons, so it is rejected too.
            if (1.0..=MAX_SIDE).contains(&device) {
                #[allow(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    reason = "checked just above: between 1 and 16 384, and already rounded to a \
                              whole number, so the conversion is exact"
                )]
                Ok(device as u32)
            } else {
                Err(ScreenshotError::Size {
                    scene: self.name,
                    width: self.width,
                    height: self.height,
                })
            }
        };
        Ok((side(self.width)?, side(self.height)?))
    }
}

/// Every screenshot of the guides, in the order they are written.
///
/// Today only the demonstration window exists (ADR-0040 W8): it is here to prove the pipeline end
/// to end, and the guides do **not** include it, because it is a throw-away view that the reader
/// will never see (ADR-0041 S7). The first pictures the guides show arrive with the real views.
pub const SCENES: &[Scene] = &[Scene {
    name: "demo-window",
    width: 640.0,
    height: 330.0,
    record: record_demo_window,
}];

/// The demonstration window at rest, with the pointer to the right of the panel (so its disc is
/// visible without covering the staff or the controls) and no button or modifier held.
fn record_demo_window(bounds: Rect, list: &mut DisplayList, context: &SceneContext) {
    let mut demo = Demo::new(&context.faces, &context.app_name);
    demo.layout(bounds, list);
    // The panel ends at x = 544 (24 px margin + 520 px), so x = 588 lies in the free strip on its
    // right; y = 200 is about the panel's middle.
    demo.input(
        &InputEvent::PointerMoved {
            position: Point::new(588.0, 200.0),
        },
        list,
    );
}

/// The theme of every screenshot: Studio's dark half (ADR-0041 S3, which amends ADR-0014 §7 and
/// ADR-0037 T10 on the owner's request).
#[must_use]
pub fn theme() -> Theme {
    *BuiltInTheme::Studio.pair().instance(Appearance::Dark)
}

/// Draws `scene` into a new image with `renderer` (reuse one renderer for the whole catalogue: it
/// keeps its scratch buffers).
///
/// # Complexity
/// One full frame: O(commands + device pixels).
///
/// # Errors
/// [`ScreenshotError::Size`] for a bad catalogue entry, [`ScreenshotError::Setup`] if the fonts or
/// the texts cannot be read, [`ScreenshotError::Draw`] if the renderer rejects the image.
pub fn render(scene: &Scene, renderer: &mut CpuRenderer) -> Result<Image, ScreenshotError> {
    let (width, height) = scene.device_size()?;
    let context = SceneContext::new()?;
    let mut list = DisplayList::new();
    (scene.record)(
        Rect::from_origin_size(0.0, 0.0, scene.width, scene.height),
        &mut list,
        &context,
    );
    renderer
        .render_to_image(&list, &theme(), width, height, SCALE)
        .map_err(|error| ScreenshotError::Draw {
            scene: scene.name,
            error,
        })
}

/// What can go wrong while rendering a screenshot. Exhaustive (ADR-0034 D2). The messages are for
/// developers (the tool is never shown to users), so they are English and not translated.
#[derive(Debug, Clone, PartialEq)]
pub enum ScreenshotError {
    /// A catalogue entry whose size is not a positive, representable number of device pixels.
    Size {
        /// The scene's name.
        scene: &'static str,
        /// Its logical width.
        width: f32,
        /// Its logical height.
        height: f32,
    },
    /// The bundled faces or the en-US catalogue could not be read (never with the files in the
    /// binary; a typed error because library code does not panic).
    Setup {
        /// The underlying error's message.
        detail: String,
    },
    /// The renderer failed, or the PNG encoder did.
    Draw {
        /// The scene's name.
        scene: &'static str,
        /// The drawing module's error.
        error: DrawError,
    },
}

impl fmt::Display for ScreenshotError {
    /// One English line naming the scene.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Size {
                scene,
                width,
                height,
            } => write!(
                f,
                "scene `{scene}`: size {width} × {height} at scale {SCALE} is not a valid image"
            ),
            Self::Setup { detail } => write!(f, "the fonts or texts of the scenes: {detail}"),
            Self::Draw { scene, error } => write!(f, "scene `{scene}`: {error}"),
        }
    }
}

impl std::error::Error for ScreenshotError {
    /// The drawing error, when there is one.
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Size { .. } | Self::Setup { .. } => None,
            Self::Draw { error, .. } => Some(error),
        }
    }
}
