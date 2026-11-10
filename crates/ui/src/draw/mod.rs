// SPDX-License-Identifier: GPL-3.0-or-later

//! The drawing abstraction (ADR-0039): what to draw, as a display list of themed commands, and
//! the backend that turns it into pixels.
//!
//! # The two halves
//! - **Recording.** Every piece of the interface (a widget, a region of the score, a track of the
//!   timeline) records its commands into its own **segment** of a [`DisplayList`], through a
//!   [`Recorder`]. Commands use logical pixels and **theme tokens**, never colours (ADR-0014 §2).
//!   Re-recording one segment damages only its old and new area ([`Damage`]).
//! - **Drawing.** A [`Renderer`] draws the damaged parts of the list with a theme, at a scale,
//!   into an [`Image`]. Today the only backend is [`CpuRenderer`] on `tiny-skia`; a GPU backend
//!   (`vello`, F2.06) would implement the same trait from the same list.
//!
//! ```
//! use orchestra_ui::{
//!     Appearance, BuiltInTheme, ColorToken, CpuRenderer, DisplayList, Rect, RoundedRect,
//!     SegmentId, StrokeStyle,
//! };
//!
//! let mut list = DisplayList::new();
//! list.record(SegmentId(1), |r| {
//!     let button = RoundedRect::new(Rect::new(10.0, 10.0, 110.0, 40.0), 6.0);
//!     r.fill_rounded_rect(button, ColorToken::Control);
//!     r.stroke_rounded_rect(button, StrokeStyle::new(1.0), ColorToken::ControlBorder);
//! });
//! let theme = BuiltInTheme::Studio.pair().instance(Appearance::Light);
//! let image = CpuRenderer::new().render_to_image(&list, &theme, 120, 50, 1.0)?;
//! assert_eq!(image.width(), 120);
//! # Ok::<(), orchestra_ui::DrawError>(())
//! ```
//!
//! # Files
//! - `geometry`: points, rectangles, rounded rectangles, the transform;
//! - `paint`: paints (token + opacity), fill rules, strokes;
//! - `image`: the RGBA image and the module's error;
//! - `command`: the recorded commands (crate-internal);
//! - `list`: the display list, its segments and the recorder;
//! - `damage`: the damaged rectangles of a frame;
//! - `device`: the damage in device pixels, shared by the renderer and the window;
//! - `shadow`: the analytic blurred rounded rectangle;
//! - `skia`: the `tiny-skia` backend, the only file that knows `tiny-skia`.

pub(crate) mod command;
pub(crate) mod damage;
pub(crate) mod device;
pub(crate) mod geometry;
pub(crate) mod image;
pub(crate) mod list;
pub(crate) mod paint;
pub(crate) mod shadow;
pub(crate) mod skia;

use crate::Theme;

/// A backend that draws a display list (ADR-0039 D9).
///
/// One trait for the CPU backend now and a GPU backend later: the list does not change.
pub trait Renderer {
    /// Draws the parts of `list` that `damage` names into `target`, resolving tokens with
    /// `theme`, at `scale` device pixels per logical pixel. Pixels outside the damage are left as
    /// they are, so `target` must hold the previous frame (or use [`Damage::all`](crate::Damage)).
    ///
    /// # Errors
    /// [`DrawError::InvalidScale`](crate::DrawError) when `scale` is not positive and finite.
    fn render(
        &mut self,
        list: &list::DisplayList,
        theme: &Theme,
        damage: &damage::Damage,
        scale: f32,
        target: &mut image::Image,
    ) -> Result<(), image::DrawError>;
}
