// SPDX-License-Identifier: GPL-3.0-or-later

//! The CPU backend on `vello_cpu` (ADR-0042): turns a display list into pixels of an [`Image`],
//! redrawing only the damage (ADR-0039 D5), resolving theme tokens at draw time (D3), snapping
//! straight edges to device pixels (D7, through [`super::snap`]), on several threads when a region
//! is large enough to repay them (ADR-0042 R3).
//!
//! This is the only file of the crate that knows about `vello_cpu`.
//!
//! # How a frame is drawn
//! 1. The damage becomes device regions, exactly as for the window ([`DeviceRegions`]): the
//!    whole frame when the damage covers more than half of it.
//! 2. For each region, a `vello_cpu` scene **the size of the region** is encoded: the region's
//!    pixels are first cleared (set to transparent, then filled with the theme's `surface`, which
//!    gives exactly the `surface` colour even if a custom theme made it translucent), then every
//!    segment and command whose bounds meet the region is encoded, shifted by the region's
//!    origin.
//! 3. The scene is rasterised **directly into the target image** at the region's offset
//!    (`RasterizerSettings::offset`), compositing over the cleared pixels: no scratch buffer and
//!    no copy. Pixels outside the region are never written.
//!
//! # Why no grown scratch area (unlike the `tiny-skia` backend)
//! `tiny-skia` anti-aliases a path differently when the pixmap's edge cuts it, so its backend had
//! to draw every path that touches a region whole (F2.04a). `vello_cpu` computes coverage from the
//! path itself, wherever the scene's edge falls; the test "drawing only the damage equals drawing
//! everything" (`tests/draw.rs`) checks it on noteheads, discs and rounded clips cut by regions.
//!
//! # Threads
//! Two rendering contexts are kept: one single-threaded, for small regions (an edited note,
//! a pointer move), where starting the workers costs more than it saves; one with worker threads
//! for regions of at least [`MULTI_THREAD_MIN_AREA`] pixels (full frames, scrolling). Both give
//! the same pixels (tested): every pixel is computed by one worker with the same arithmetic.
//!
//! # Allocations
//! Contexts, the resource store, the path and the transform stack are kept and reused, so a
//! steady stream of frames of similar size stops allocating in our code; `vello_cpu` manages its
//! own buffers inside the contexts.

use std::sync::Arc;

use vello_cpu::kurbo::{Affine, BezPath, Cap, Join, Rect as KRect, Stroke};
use vello_cpu::peniko::{Fill, ImageAlphaType, ImageQuality, ImageSampler};
use vello_cpu::{
    Image as VImage, ImageSource, PixelMetadata, Pixmap, PixmapMut, RasterizerSettings,
    RenderContext, RenderSettings, Resources, TargetInit,
};

use super::Renderer;
use super::command::{Command, PathElement};
use super::damage::Damage;
use super::device::{DeviceRect, DeviceRegions, to_f32};
use super::geometry::{Point, Rect, RoundedRect, Transform};
use super::image::{DrawError, Image, ImageId};
use super::list::{DisplayList, SHADOW_REACH_SIGMAS, Segment};
use super::paint::{FillRule, LineCap, LineJoin, Paint, StrokeStyle};
use super::shadow;
use super::snap::{KAPPA, snap_rect, snapped_line};
use crate::{ColorToken, Theme};

/// The smallest region, in device pixels, drawn with the worker threads: 256 × 256. Below it the
/// single-threaded context is faster (measured in F2.06b on the reference scene: an edited
/// measure's region is about 100 × 50 pixels at scale 1).
pub(crate) const MULTI_THREAD_MIN_AREA: u64 = 256 * 256;

/// The most worker threads used by default (more brings nothing measurable on the reference
/// hardware's 4 cores / 8 threads, and `vello_cpu` itself caps its default at 8).
const MAX_DEFAULT_THREADS: u16 = 8;

/// The default number of worker threads (ADR-0042 R3): the cores minus one, so one core always
/// stays free for the audio thread, at most [`MAX_DEFAULT_THREADS`]; 0 (single-threaded) on a
/// machine with one or two cores, where workers would compete with everything else.
#[must_use]
pub fn default_render_threads() -> u16 {
    let cores = std::thread::available_parallelism().map_or(1, std::num::NonZero::get);
    if cores <= 2 {
        return 0;
    }
    u16::try_from(cores - 1).map_or(MAX_DEFAULT_THREADS, |n| n.min(MAX_DEFAULT_THREADS))
}

/// A picture of the list converted once for `vello_cpu`, with what identifies its source: if the
/// list's image at that id changes (different pixels buffer or size), the conversion is redone.
struct CachedImage {
    /// Address of the source pixels when converted (identity check, never dereferenced).
    source: usize,
    /// Source size.
    size: (u32, u32),
    /// The converted pixels.
    pixmap: Arc<Pixmap>,
}

/// What identifies a computed shadow: its geometry in frame device pixels, its blur and colour,
/// all as exact bits (two shadows share pixels only if every input is identical).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ShadowKey {
    /// The shape's edges (x0, y0, x1, y1), device pixels of the frame, as `f32` bits.
    rect: [u32; 4],
    /// Corner radius, device pixels, as bits.
    radius: u32,
    /// Standard deviation, device pixels, as bits.
    sigma: u32,
    /// Premultiplied colour at full coverage.
    color: [u8; 4],
    /// The frame's size (the shadow image is clamped to it).
    frame: (u32, u32),
}

/// A shadow image computed with our analytic formula ([`shadow::coverage`]), kept between frames.
struct CachedShadow {
    /// What it is.
    key: ShadowKey,
    /// Its top-left corner in the frame, device pixels.
    origin: (u32, u32),
    /// Premultiplied pixels of the blurred shape over its reach (3σ), clamped to the frame.
    pixmap: Arc<Pixmap>,
    /// The frame number it was last drawn in, for eviction.
    last_used: u64,
}

/// The most shadows kept between frames: a window shows a handful (panels, menus, dialogs).
const MAX_CACHED_SHADOWS: usize = 32;

/// The `vello_cpu` renderer. Keep one per window (or per headless job) and reuse it.
pub struct CpuRenderer {
    /// Worker threads for large regions; 0 = always single-threaded.
    threads: u16,
    /// The single-threaded context (created on first use).
    single: Option<RenderContext>,
    /// The multi-threaded context (created on first use, only if `threads > 0`).
    multi: Option<RenderContext>,
    /// `vello_cpu`'s resource store (image registry, caches), shared by both contexts.
    resources: Resources,
    /// A path reused from command to command.
    path: BezPath,
    /// The current path flattened in device pixels (see [`FLATTEN_TOLERANCE`]), reused.
    device: BezPath,
    /// Transforms in effect while replaying a segment (local → list coordinates).
    transforms: Vec<Transform>,
    /// Clips pushed and not yet popped in the segment being replayed.
    clips: usize,
    /// Converted pictures, indexed by `ImageId`.
    images: Vec<Option<CachedImage>>,
    /// Shadows computed in recent frames (ADR-0042 R4: our formula, not `vello_cpu`'s blur).
    shadows: Vec<CachedShadow>,
    /// Frames rendered so far (for the shadow cache's eviction).
    frame_number: u64,
}

impl Default for CpuRenderer {
    fn default() -> Self {
        Self::with_threads(default_render_threads())
    }
}

impl std::fmt::Debug for CpuRenderer {
    /// The settings only.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CpuRenderer")
            .field("threads", &self.threads)
            .finish_non_exhaustive()
    }
}

impl CpuRenderer {
    /// A renderer with the default number of worker threads ([`default_render_threads`]).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A renderer with `threads` worker threads for large regions (0 = single-threaded).
    #[must_use]
    pub fn with_threads(threads: u16) -> Self {
        Self {
            threads,
            single: None,
            multi: None,
            resources: Resources::new(),
            path: BezPath::new(),
            device: BezPath::new(),
            transforms: Vec::new(),
            clips: 0,
            images: Vec::new(),
            shadows: Vec::new(),
            frame_number: 0,
        }
    }

    /// The number of worker threads used for large regions.
    #[must_use]
    pub const fn threads(&self) -> u16 {
        self.threads
    }

    /// Draws the whole `list` into a new image of `width × height` device pixels at `scale`
    /// device pixels per logical pixel: the headless renderer (ADR-0039 D8).
    ///
    /// # Errors
    /// [`DrawError::InvalidSize`] for a zero or huge size, [`DrawError::InvalidScale`] for a scale
    /// that is not positive and finite.
    pub fn render_to_image(
        &mut self,
        list: &DisplayList,
        theme: &Theme,
        width: u32,
        height: u32,
        scale: f32,
    ) -> Result<Image, DrawError> {
        let mut image = Image::new(width, height)?;
        self.render(list, theme, &Damage::all(), scale, &mut image)?;
        Ok(image)
    }

    /// The context for a region of `area` pixels, reset to `width × height`.
    fn context(&mut self, area: u64, width: u16, height: u16) -> &mut RenderContext {
        let multi = self.threads > 0 && area >= MULTI_THREAD_MIN_AREA;
        let (slot, threads) = if multi {
            (&mut self.multi, self.threads)
        } else {
            (&mut self.single, 0)
        };
        match slot {
            Some(ctx) => {
                ctx.reset_and_resize(width, height);
                ctx
            }
            None => slot.insert(RenderContext::new_with(
                width,
                height,
                RenderSettings {
                    num_threads: threads,
                    ..RenderSettings::default()
                },
            )),
        }
    }

    /// Draws one device region of the frame directly into `target`.
    ///
    /// # Errors
    /// [`DrawError::InvalidSize`] when the frame or the region exceeds `vello_cpu`'s 16-bit sizes
    /// (65 535 pixels a side: beyond any screen).
    fn render_region(
        &mut self,
        list: &DisplayList,
        theme: &Theme,
        scale: f32,
        region: DeviceRect,
        target: &mut Image,
    ) -> Result<(), DrawError> {
        let too_big = DrawError::InvalidSize {
            width: target.width(),
            height: target.height(),
        };
        let (Ok(frame_w), Ok(frame_h), Ok(w), Ok(h), Ok(x), Ok(y)) = (
            u16::try_from(target.width()),
            u16::try_from(target.height()),
            u16::try_from(region.width),
            u16::try_from(region.height),
            u16::try_from(region.x),
            u16::try_from(region.y),
        ) else {
            return Err(too_big);
        };
        clear_region(target, region);
        let logical = region.logical(scale);
        let frame = Frame {
            scale,
            origin: Point::new(to_f32(region.x), to_f32(region.y)),
            logical,
            surface_size: (target.width(), target.height()),
            theme,
        };
        self.cache_images(list);
        // Take the context out of `self` so replaying can borrow the rest of `self`.
        let area = region.area();
        let multi = self.threads > 0 && area >= MULTI_THREAD_MIN_AREA;
        self.context(area, w, h);
        let mut ctx = if multi {
            self.multi.take()
        } else {
            self.single.take()
        }
        .ok_or(DrawError::InvalidSize {
            width: region.width,
            height: region.height,
        })?;
        // Background: the theme's surface over the cleared (transparent) pixels.
        ctx.set_paint(color(theme, Paint::token(ColorToken::Surface)));
        ctx.fill_rect(&KRect::new(0.0, 0.0, f64::from(w), f64::from(h)));
        for segment in list.segments() {
            if segment.bounds.intersects(&logical) {
                self.replay(&mut ctx, segment, &frame);
            }
        }
        ctx.flush();
        let pixmap = PixmapMut::new(frame_w, frame_h, target.data_mut());
        if let Some(pixmap) = pixmap {
            ctx.render_with(
                pixmap,
                &mut self.resources,
                RasterizerSettings {
                    target_init: TargetInit::SrcOver,
                    offset: (x, y),
                    ..RasterizerSettings::default()
                },
            );
        }
        if theme.color(ColorToken::Surface).a == u8::MAX {
            make_opaque(target, region);
        }
        if multi {
            self.multi = Some(ctx);
        } else {
            self.single = Some(ctx);
        }
        Ok(())
    }

    /// Converts the list's pictures that are new or changed since the last frame.
    ///
    /// # Complexity
    /// O(images) checks per frame; a conversion copies one picture's pixels, once.
    fn cache_images(&mut self, list: &DisplayList) {
        for (index, image) in list.images().iter().enumerate() {
            if self.images.len() <= index {
                self.images.resize_with(index + 1, || None);
            }
            let Some(slot) = self.images.get_mut(index) else {
                continue;
            };
            let Some(image) = image else {
                *slot = None;
                continue;
            };
            let source = image.data().as_ptr() as usize;
            let size = (image.width(), image.height());
            if slot
                .as_ref()
                .is_some_and(|c| c.source == source && c.size == size)
            {
                continue;
            }
            let (Ok(w), Ok(h)) = (u16::try_from(size.0), u16::try_from(size.1)) else {
                *slot = None;
                continue;
            };
            let pixmap = Pixmap::from_parts(
                image.data().to_vec(),
                w,
                h,
                PixelMetadata {
                    may_have_transparency: true,
                    alpha_type: ImageAlphaType::AlphaPremultiplied,
                },
            );
            *slot = Some(CachedImage {
                source,
                size,
                pixmap: Arc::new(pixmap),
            });
        }
    }

    /// Encodes one segment's commands that meet the region.
    fn replay(&mut self, ctx: &mut RenderContext, segment: &Segment, frame: &Frame<'_>) {
        self.transforms.clear();
        self.clips = 0;
        for entry in &segment.entries {
            if !entry.command.is_state() && !entry.bounds.intersects(&frame.logical) {
                continue;
            }
            let transform = self
                .transforms
                .last()
                .copied()
                .unwrap_or(Transform::IDENTITY);
            match &entry.command {
                Command::FillRect { rect, paint } => {
                    fill_snapped(ctx, frame, &transform, rect, *paint);
                }
                Command::FillRoundedRect { shape, paint } => {
                    self.fill_rounded(ctx, frame, &transform, shape, *paint);
                }
                Command::StrokeRect {
                    rect,
                    stroke,
                    paint,
                } => stroke_snapped_rect(ctx, frame, &transform, rect, *stroke, *paint),
                Command::StrokeRoundedRect {
                    shape,
                    stroke,
                    paint,
                } => self.stroke_rounded(ctx, frame, &transform, shape, *stroke, *paint),
                Command::Line {
                    from,
                    to,
                    stroke: style,
                    paint,
                } => self.line(ctx, frame, &transform, *from, *to, *style, *paint),
                Command::FillPath {
                    elements,
                    rule,
                    paint,
                } => {
                    let elements = segment.elements.get(elements.clone()).unwrap_or(&[]);
                    elements_path(&mut self.path, elements);
                    let (path, device) = (&self.path, &mut self.device);
                    fill(ctx, frame, &transform, path, device, *rule, *paint);
                }
                Command::StrokePath {
                    elements,
                    stroke: style,
                    paint,
                } => {
                    let elements = segment.elements.get(elements.clone()).unwrap_or(&[]);
                    elements_path(&mut self.path, elements);
                    let (path, device) = (&self.path, &mut self.device);
                    stroke_path(ctx, frame, &transform, path, device, *style, *paint);
                }
                Command::Shadow { shape, blur, paint } => {
                    self.shadow(ctx, frame, &transform, shape, *blur, *paint);
                }
                Command::Image { image, dest } => self.image(ctx, frame, &transform, *image, dest),
                Command::PushClip { shape } => self.push_clip(ctx, frame, &transform, shape),
                Command::PopClip => {
                    if self.clips > 0 {
                        ctx.pop_clip();
                        self.clips -= 1;
                    }
                }
                Command::PushTransform(inner) => self.transforms.push(transform.then(inner)),
                Command::PopTransform => {
                    self.transforms.pop();
                }
            }
        }
        // A segment never leaks a clip into the next one (the recorder closes them, but a list
        // is data: be robust).
        while self.clips > 0 {
            ctx.pop_clip();
            self.clips -= 1;
        }
    }

    /// A rounded rectangle filled: snapped when its radius is 0 (a plain rectangle, D7), otherwise
    /// an anti-aliased path.
    fn fill_rounded(
        &mut self,
        ctx: &mut RenderContext,
        frame: &Frame<'_>,
        transform: &Transform,
        shape: &RoundedRect,
        paint: Paint,
    ) {
        if shape.clamped_radius() <= 0.0 {
            fill_snapped(ctx, frame, transform, &shape.rect, paint);
        } else {
            rounded_path(&mut self.path, shape);
            let (path, device) = (&self.path, &mut self.device);
            fill(
                ctx,
                frame,
                transform,
                path,
                device,
                FillRule::NonZero,
                paint,
            );
        }
    }

    /// A rounded rectangle's outline: four snapped bands when its radius is 0 (D7), otherwise an
    /// anti-aliased stroke.
    fn stroke_rounded(
        &mut self,
        ctx: &mut RenderContext,
        frame: &Frame<'_>,
        transform: &Transform,
        shape: &RoundedRect,
        style: StrokeStyle,
        paint: Paint,
    ) {
        if shape.clamped_radius() <= 0.0 {
            stroke_snapped_rect(ctx, frame, transform, &shape.rect, style, paint);
        } else {
            rounded_path(&mut self.path, shape);
            let (path, device) = (&self.path, &mut self.device);
            stroke_path(ctx, frame, transform, path, device, style, paint);
        }
    }

    /// A line: snapped when horizontal or vertical with butt or square caps (D7), otherwise an
    /// anti-aliased stroke.
    #[allow(
        clippy::too_many_arguments,
        reason = "the replay's context, frame and transform plus the command's four fields"
    )]
    fn line(
        &mut self,
        ctx: &mut RenderContext,
        frame: &Frame<'_>,
        transform: &Transform,
        from: Point,
        to: Point,
        style: StrokeStyle,
        paint: Paint,
    ) {
        let a = frame.to_device(transform, from);
        let b = frame.to_device(transform, to);
        let width = style.width * transform.scale * frame.scale;
        let extend = match style.cap {
            LineCap::Butt => Some(0.0),
            LineCap::Square => Some(width * 0.5),
            LineCap::Round => None,
        };
        if let Some(extend) = extend
            && let Some(rect) = snapped_line((a.x, a.y), (b.x, b.y), width, extend)
        {
            fill_device_rect(ctx, frame.theme, &rect, paint);
            return;
        }
        self.path.truncate(0);
        self.path.move_to((f64::from(from.x), f64::from(from.y)));
        self.path.line_to((f64::from(to.x), f64::from(to.y)));
        stroke_path(
            ctx,
            frame,
            transform,
            &self.path,
            &mut self.device,
            style,
            paint,
        );
    }

    /// A blurred rounded rectangle (ADR-0039 D6) with **our** formula (see [`compute_shadow`]),
    /// cached between frames and composited as an image at its place in the drawing order, so
    /// clips and later commands apply as usual. A blur of 0 is an ordinary fill.
    fn shadow(
        &mut self,
        ctx: &mut RenderContext,
        frame: &Frame<'_>,
        transform: &Transform,
        shape: &RoundedRect,
        blur: f32,
        paint: Paint,
    ) {
        if blur <= 0.0 || !blur.is_finite() {
            self.fill_rounded(ctx, frame, transform, shape, paint);
            return;
        }
        let factor = transform.scale * frame.scale;
        let sigma = blur * factor;
        // The shape in the frame's device pixels (not the region's): the cached image is valid for
        // every region that shows it.
        let local = frame.to_device_rect(transform, &shape.rect);
        let device = Rect::new(
            local.x0 + frame.origin.x,
            local.y0 + frame.origin.y,
            local.x1 + frame.origin.x,
            local.y1 + frame.origin.y,
        );
        let radius = shape.clamped_radius() * factor;
        let c = frame.theme.color(paint.token);
        let alpha = f32::from(c.a) * f32::from(paint.opacity) / (255.0 * 255.0);
        let premultiplied = [
            f32::from(c.r) * alpha,
            f32::from(c.g) * alpha,
            f32::from(c.b) * alpha,
            255.0 * alpha,
        ];
        let key = ShadowKey {
            rect: [
                device.x0.to_bits(),
                device.y0.to_bits(),
                device.x1.to_bits(),
                device.y1.to_bits(),
            ],
            radius: radius.to_bits(),
            sigma: sigma.to_bits(),
            color: premultiplied.map(round_channel),
            frame: frame.surface_size,
        };
        let frame_number = self.frame_number;
        let index = if let Some(index) = self.shadows.iter().position(|cached| cached.key == key) {
            index
        } else {
            let Some(cached) = compute_shadow(
                key,
                &device,
                radius,
                sigma,
                premultiplied,
                frame.surface_size,
            ) else {
                return;
            };
            if self.shadows.len() >= MAX_CACHED_SHADOWS
                && let Some(oldest) = self
                    .shadows
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, cached)| cached.last_used)
                    .map(|(index, _)| index)
            {
                self.shadows.swap_remove(oldest);
            }
            self.shadows.push(cached);
            self.shadows.len() - 1
        };
        let Some(cached) = self.shadows.get_mut(index) else {
            return;
        };
        cached.last_used = frame_number;
        // Placed on whole pixels and sampled nearest: the image's pixels land unchanged.
        let (x, y) = cached.origin;
        ctx.set_transform(Affine::translate((
            f64::from(to_f32(x) - frame.origin.x),
            f64::from(to_f32(y) - frame.origin.y),
        )));
        ctx.set_paint(VImage {
            image: ImageSource::Pixmap(Arc::clone(&cached.pixmap)),
            sampler: ImageSampler {
                quality: ImageQuality::Low,
                ..ImageSampler::default()
            },
        });
        ctx.fill_rect(&KRect::new(
            0.0,
            0.0,
            f64::from(cached.pixmap.width()),
            f64::from(cached.pixmap.height()),
        ));
    }

    /// A stored picture stretched into `dest`, sampled bilinearly.
    fn image(
        &self,
        ctx: &mut RenderContext,
        frame: &Frame<'_>,
        transform: &Transform,
        id: ImageId,
        dest: &Rect,
    ) {
        let Some(cached) = self.images.get(id.0 as usize).and_then(Option::as_ref) else {
            return;
        };
        let sx = f64::from(dest.width()) / f64::from(cached.size.0);
        let sy = f64::from(dest.height()) / f64::from(cached.size.1);
        let placement = frame.affine(transform)
            * Affine::translate((f64::from(dest.x0), f64::from(dest.y0)))
            * Affine::scale_non_uniform(sx, sy);
        ctx.set_transform(placement);
        ctx.set_paint(VImage {
            image: ImageSource::Pixmap(Arc::clone(&cached.pixmap)),
            sampler: ImageSampler {
                quality: ImageQuality::Medium,
                ..ImageSampler::default()
            },
        });
        ctx.fill_rect(&KRect::new(
            0.0,
            0.0,
            f64::from(cached.size.0),
            f64::from(cached.size.1),
        ));
    }

    /// Starts a clip: rectangular clips are snapped to device pixels (sharp edges), rounded ones
    /// anti-aliased; nested clips intersect (`vello_cpu`'s clip stack).
    fn push_clip(
        &mut self,
        ctx: &mut RenderContext,
        frame: &Frame<'_>,
        transform: &Transform,
        shape: &RoundedRect,
    ) {
        if shape.clamped_radius() <= 0.0 {
            let device = frame.to_device_rect(transform, &shape.rect);
            // An empty clip hides everything: a zero-size rectangle.
            let rect = snap_rect(&device).unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0));
            ctx.set_transform(Affine::IDENTITY);
            ctx.push_clip_rect(&krect(&rect));
        } else {
            rounded_path(&mut self.path, shape);
            flatten_into(&mut self.device, &self.path, frame.affine(transform));
            ctx.set_transform(Affine::IDENTITY);
            ctx.set_fill_rule(Fill::NonZero);
            ctx.push_clip_path(&self.device);
        }
        self.clips += 1;
    }
}

impl Renderer for CpuRenderer {
    /// Draws the damaged parts of `list` into `target` (see the module documentation).
    fn render(
        &mut self,
        list: &DisplayList,
        theme: &Theme,
        damage: &Damage,
        scale: f32,
        target: &mut Image,
    ) -> Result<(), DrawError> {
        if !(scale > 0.0 && scale.is_finite()) {
            return Err(DrawError::InvalidScale { scale });
        }
        if damage.is_empty() {
            return Ok(());
        }
        // The same device regions the window presents (ADR-0040 W4): one computation for both.
        self.frame_number = self.frame_number.wrapping_add(1);
        let regions = DeviceRegions::new(damage, scale, target.width(), target.height());
        for region in regions.iter() {
            self.render_region(list, theme, scale, *region, target)?;
        }
        Ok(())
    }
}

/// What one region's replay needs to know.
struct Frame<'a> {
    /// Device pixels per logical pixel.
    scale: f32,
    /// The region's top-left corner in the frame, device pixels: subtracted from every
    /// coordinate, so the scene's (0, 0) is the region's corner.
    origin: Point,
    /// The region in logical pixels, for culling.
    logical: Rect,
    /// The size of the whole frame (the target image), device pixels: shadow images are clamped
    /// to it.
    surface_size: (u32, u32),
    /// The theme the tokens resolve with.
    theme: &'a Theme,
}

impl Frame<'_> {
    /// A local point in the scene's device coordinates.
    fn to_device(&self, transform: &Transform, point: Point) -> Point {
        let list = transform.apply(point);
        Point::new(
            list.x * self.scale - self.origin.x,
            list.y * self.scale - self.origin.y,
        )
    }

    /// A local rectangle in the scene's device coordinates.
    fn to_device_rect(&self, transform: &Transform, rect: &Rect) -> Rect {
        let a = self.to_device(transform, Point::new(rect.x0, rect.y0));
        let b = self.to_device(transform, Point::new(rect.x1, rect.y1));
        Rect::new(a.x, a.y, b.x, b.y)
    }

    /// The affine transform from local coordinates to the scene's device pixels.
    fn affine(&self, transform: &Transform) -> Affine {
        let scale = f64::from(transform.scale) * f64::from(self.scale);
        Affine::new([
            scale,
            0.0,
            0.0,
            scale,
            f64::from(transform.offset.x) * f64::from(self.scale) - f64::from(self.origin.x),
            f64::from(transform.offset.y) * f64::from(self.scale) - f64::from(self.origin.y),
        ])
    }
}

/// Sets the pixels of `region` in `target` to transparent, row by row, so the scene composited
/// over them (`SrcOver`) gives exactly what it draws.
fn clear_region(target: &mut Image, region: DeviceRect) {
    let stride = target.width() as usize * 4;
    let row_bytes = region.width as usize * 4;
    let data = target.data_mut();
    for row in 0..region.height as usize {
        let start = (region.y as usize + row) * stride + region.x as usize * 4;
        if let Some(bytes) = data.get_mut(start..start + row_bytes) {
            bytes.fill(0);
        }
    }
}

/// Sets the alpha of every pixel of `region` to 255.
///
/// # Why
/// With an opaque `surface` (every built-in theme) the frame is opaque by construction: everything
/// is composited over it. `vello_cpu`'s 8-bit compositing can still leave alpha 254 where a
/// translucent paint covers it (found in F2.06b on the pointer's disc, opacity 160: 136 pixels),
/// which a PNG screenshot would show as faint transparency. Restoring 255 changes the colour
/// channels' meaning by at most 1/255 (they are premultiplied by 254/255 instead of 1): within the
/// one step the 8-bit pipeline already rounds by. A translucent custom surface is left alone.
fn make_opaque(target: &mut Image, region: DeviceRect) {
    let stride = target.width() as usize * 4;
    let row_bytes = region.width as usize * 4;
    let data = target.data_mut();
    for row in 0..region.height as usize {
        let start = (region.y as usize + row) * stride + region.x as usize * 4;
        if let Some(bytes) = data.get_mut(start..start + row_bytes) {
            for pixel in bytes.as_chunks_mut::<4>().0 {
                pixel[3] = u8::MAX;
            }
        }
    }
}

/// The colour of a token in the theme, with the paint's extra opacity multiplied into its alpha.
fn color(theme: &Theme, paint: Paint) -> vello_cpu::color::AlphaColor<vello_cpu::color::Srgb> {
    let c = theme.color(paint.token);
    // (a · opacity + 127) / 255: the product of two 8-bit fractions, rounded to nearest (the same
    // rule as the tiny-skia backend, so both resolve a paint to the same bytes).
    let alpha = (u16::from(c.a) * u16::from(paint.opacity) + 127) / 255;
    let alpha = u8::try_from(alpha).unwrap_or(u8::MAX);
    vello_cpu::color::AlphaColor::from_rgba8(c.r, c.g, c.b, alpha)
}

/// Our rectangle as `kurbo`'s.
fn krect(r: &Rect) -> KRect {
    KRect::new(
        f64::from(r.x0),
        f64::from(r.y0),
        f64::from(r.x1),
        f64::from(r.y1),
    )
}

/// Fills a device rectangle (already snapped) with no transform.
fn fill_device_rect(ctx: &mut RenderContext, theme: &Theme, rect: &Rect, paint: Paint) {
    ctx.set_transform(Affine::IDENTITY);
    ctx.set_paint(color(theme, paint));
    ctx.fill_rect(&krect(rect));
}

/// Fills an axis-aligned rectangle snapped to device pixels (D7).
fn fill_snapped(
    ctx: &mut RenderContext,
    frame: &Frame<'_>,
    transform: &Transform,
    rect: &Rect,
    paint: Paint,
) {
    let device = frame.to_device_rect(transform, rect);
    if let Some(snapped) = snap_rect(&device) {
        fill_device_rect(ctx, frame.theme, &snapped, paint);
    }
}

/// Strokes the outline of an axis-aligned rectangle as four snapped bands (D7).
fn stroke_snapped_rect(
    ctx: &mut RenderContext,
    frame: &Frame<'_>,
    transform: &Transform,
    rect: &Rect,
    style: StrokeStyle,
    paint: Paint,
) {
    let width = style.width * transform.scale * frame.scale;
    let band = width.round().max(1.0);
    let device = frame.to_device_rect(transform, rect).inflate(width * 0.5);
    let Some(outer) = snap_rect(&device) else {
        return;
    };
    let (x0, y0, x1, y1) = (outer.x0, outer.y0, outer.x1, outer.y1);
    if x1 - x0 <= 2.0 * band || y1 - y0 <= 2.0 * band {
        // Too small for a hole: the outline is the whole rectangle.
        fill_device_rect(ctx, frame.theme, &outer, paint);
        return;
    }
    for band_rect in [
        Rect::new(x0, y0, x1, y0 + band),
        Rect::new(x0, y1 - band, x1, y1),
        Rect::new(x0, y0 + band, x0 + band, y1 - band),
        Rect::new(x1 - band, y0 + band, x1, y1 - band),
    ] {
        fill_device_rect(ctx, frame.theme, &band_rect, paint);
    }
}

/// The largest distance, in device pixels, between a curve and the segments that replace it
/// (option A chosen by the owner in F2.06b, recorded in ADR-0042 R4).
///
/// # Why we flatten curves ourselves
/// `vello_cpu` 0.3 flattens curves with a fixed tolerance of 0.25 device pixels, and the segments
/// always lie inside the curve, so small round shapes lose area: a notehead at zoom 1 (5 × 4 px)
/// lost 4.4 % of its area (9.8 % at zoom 0.5), against 0.8 % with `tiny-skia`. Flattening the path
/// ourselves in device space at 0.05 px and handing `vello_cpu` only straight segments brings the
/// loss to 1.1 % (2.5 % at zoom 0.5), at the cost of more segments per curve (measured in F2.06b on
/// the reference scene, single-threaded: 9.7 ms → about 13 ms a full frame, still faster than
/// `tiny-skia`'s 21 ms). Noteheads become cached glyphs in F2.07, which removes most of that cost.
/// If a later `vello_cpu` makes its tolerance configurable, this step can go.
const FLATTEN_TOLERANCE: f64 = 0.05;

/// Replaces `device` with `path` (local coordinates) transformed by `affine` into device pixels
/// and flattened to straight segments within [`FLATTEN_TOLERANCE`].
///
/// # Complexity
/// O(segments produced): kurbo picks the number of segments per curve from the curve's size and
/// the tolerance (about √(size / tolerance)). No allocation once `device` has grown.
fn flatten_into(device: &mut BezPath, path: &BezPath, affine: Affine) {
    device.truncate(0);
    vello_cpu::kurbo::flatten(
        path.iter().map(|element| affine * element),
        FLATTEN_TOLERANCE,
        |element| {
            device.push(element);
        },
    );
}

/// Fills `path` (local coordinates) with anti-aliasing, flattened in device space first.
fn fill(
    ctx: &mut RenderContext,
    frame: &Frame<'_>,
    transform: &Transform,
    path: &BezPath,
    device: &mut BezPath,
    rule: FillRule,
    paint: Paint,
) {
    flatten_into(device, path, frame.affine(transform));
    ctx.set_transform(Affine::IDENTITY);
    ctx.set_fill_rule(match rule {
        FillRule::NonZero => Fill::NonZero,
        FillRule::EvenOdd => Fill::EvenOdd,
    });
    ctx.set_paint(color(frame.theme, paint));
    ctx.fill_path(device);
}

/// Strokes `path` (local coordinates; the width scales with the transform, as in the recorder's
/// bounds) with anti-aliasing. The centre line is flattened in device space first (see
/// [`FLATTEN_TOLERANCE`]), so the width is scaled to device pixels here.
fn stroke_path(
    ctx: &mut RenderContext,
    frame: &Frame<'_>,
    transform: &Transform,
    path: &BezPath,
    device: &mut BezPath,
    style: StrokeStyle,
    paint: Paint,
) {
    flatten_into(device, path, frame.affine(transform));
    let width = f64::from(style.width) * f64::from(transform.scale) * f64::from(frame.scale);
    let cap = match style.cap {
        LineCap::Butt => Cap::Butt,
        LineCap::Round => Cap::Round,
        LineCap::Square => Cap::Square,
    };
    let join = match style.join {
        LineJoin::Miter => Join::Miter,
        LineJoin::Round => Join::Round,
        LineJoin::Bevel => Join::Bevel,
    };
    ctx.set_transform(Affine::IDENTITY);
    ctx.set_stroke(
        Stroke::new(width)
            .with_caps(cap)
            .with_join(join)
            .with_miter_limit(f64::from(StrokeStyle::MITER_LIMIT)),
    );
    ctx.set_paint(color(frame.theme, paint));
    ctx.stroke_path(device);
}

/// Replaces `path` with a rounded rectangle: four sides and four quarter circles, each a cubic
/// with the [`KAPPA`] control distance (the same curve as the `tiny-skia` backend draws).
fn rounded_path(path: &mut BezPath, shape: &RoundedRect) {
    path.truncate(0);
    let Rect { x0, y0, x1, y1 } = shape.rect;
    let (x0, y0, x1, y1) = (f64::from(x0), f64::from(y0), f64::from(x1), f64::from(y1));
    let r = f64::from(shape.clamped_radius());
    let k = r * f64::from(KAPPA);
    path.move_to((x0 + r, y0));
    path.line_to((x1 - r, y0));
    path.curve_to((x1 - r + k, y0), (x1, y0 + r - k), (x1, y0 + r));
    path.line_to((x1, y1 - r));
    path.curve_to((x1, y1 - r + k), (x1 - r + k, y1), (x1 - r, y1));
    path.line_to((x0 + r, y1));
    path.curve_to((x0 + r - k, y1), (x0, y1 - r + k), (x0, y1 - r));
    path.line_to((x0, y0 + r));
    path.curve_to((x0, y0 + r - k), (x0 + r - k, y0), (x0 + r, y0));
    path.close_path();
}

/// Replaces `path` with the elements of a recorded path.
fn elements_path(path: &mut BezPath, elements: &[PathElement]) {
    path.truncate(0);
    let p = |point: Point| (f64::from(point.x), f64::from(point.y));
    for element in elements {
        match *element {
            PathElement::MoveTo(a) => path.move_to(p(a)),
            PathElement::LineTo(a) => path.line_to(p(a)),
            PathElement::QuadTo(c, a) => path.quad_to(p(c), p(a)),
            PathElement::CubicTo(c1, c2, a) => path.curve_to(p(c1), p(c2), p(a)),
            PathElement::Close => path.close_path(),
        }
    }
}

/// A premultiplied channel (0–255 as `f32`) rounded to a byte.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "clamped to 0..=255 and rounded before the conversion"
)]
fn round_channel(value: f32) -> u8 {
    value.round().clamp(0.0, 255.0) as u8
}

/// Computes a shadow image with our analytic formula (ADR-0039 D6, kept by ADR-0042 R4): every
/// pixel within the blur's reach ([`SHADOW_REACH_SIGMAS`] standard deviations), clamped to the
/// frame, gets the premultiplied colour times [`shadow::coverage`] at its centre. `None` when
/// nothing of it is inside the frame.
///
/// # Why not `vello_cpu`'s blur
/// Measured in F2.06b against this formula (itself within 0.84 steps of a brute-force
/// integration): `vello_cpu` 0.3's `fill_blurred_rounded_rect` differed by 28–40 steps of 255 with
/// the same standard deviation (its parameter behaves like ours divided by √2) and still by 3.6–4.9
/// steps once rescaled, and it cuts the blur at 2.5σ. ADR-0042 R4 keeps ours when the library's
/// misses one step.
///
/// # Complexity
/// O(pixels of the reach), a few `erf` each; done once per distinct shadow, then cached.
fn compute_shadow(
    key: ShadowKey,
    device: &Rect,
    radius: f32,
    sigma: f32,
    premultiplied: [f32; 4],
    surface_size: (u32, u32),
) -> Option<CachedShadow> {
    let reach = device.inflate(sigma * SHADOW_REACH_SIGMAS);
    let (frame_w, frame_h) = (to_f32(surface_size.0), to_f32(surface_size.1));
    let x0 = reach.x0.floor().clamp(0.0, frame_w);
    let y0 = reach.y0.floor().clamp(0.0, frame_h);
    let x1 = reach.x1.ceil().clamp(0.0, frame_w);
    let y1 = reach.y1.ceil().clamp(0.0, frame_h);
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    // Whole numbers within the frame (clamped above), so the conversions are exact.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "whole numbers between 0 and the frame size, which fits u32"
    )]
    let (px, py, w, h) = (x0 as u32, y0 as u32, (x1 - x0) as u32, (y1 - y0) as u32);
    let (Ok(w16), Ok(h16)) = (u16::try_from(w), u16::try_from(h)) else {
        return None;
    };
    let mut data = Vec::with_capacity(w as usize * h as usize * 4);
    for row in 0..h {
        let cy = to_f32(py + row) + 0.5;
        for column in 0..w {
            let cx = to_f32(px + column) + 0.5;
            let cover = shadow::coverage(
                cx, cy, device.x0, device.y0, device.x1, device.y1, radius, sigma,
            );
            data.extend(premultiplied.map(|channel| round_channel(channel * cover)));
        }
    }
    let pixmap = Pixmap::from_parts(
        data,
        w16,
        h16,
        PixelMetadata {
            may_have_transparency: true,
            alpha_type: ImageAlphaType::AlphaPremultiplied,
        },
    );
    Some(CachedShadow {
        key,
        origin: (px, py),
        pixmap: Arc::new(pixmap),
        last_used: 0,
    })
}

#[cfg(test)]
mod tests {
    //! Tests that need the crate's private shadow formula; the pixel tests of the backend are in
    //! `tests/draw.rs`.

    #![allow(
        clippy::unwrap_used,
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "test code: a failed unwrap fails the test; pixel counters and sizes are small \
                  and positive"
    )]

    use super::CpuRenderer;
    use crate::draw::shadow;
    use crate::{Appearance, BuiltInTheme, ColorToken, DisplayList, Rect, RoundedRect, SegmentId};

    /// ADR-0042 R4: the backend's shadows follow our analytic formula within 2.5 steps of 255 at
    /// scale 1 and 2, for rounded and square shapes, small and large blurs. Coverage is read back
    /// from the red channel, between the surface's and the text's.
    ///
    /// # Why 2.5 and not 1
    /// The shadow *is* our formula; what differs is compositing. The `tiny-skia` backend blended
    /// it in floating point with one rounding (0.61 steps measured with this test). Here the
    /// coverage is rounded once into premultiplied bytes and `vello_cpu`'s 8-bit pipeline
    /// (`u8_pipeline`, ADR-0042) rounds again when compositing: 1.6–2.2 steps measured in F2.06b,
    /// invisible. The test still fails for a wrong blur (28–40 steps with `vello_cpu`'s own).
    #[test]
    fn shadows_follow_the_analytic_formula() {
        let theme = *BuiltInTheme::Studio.pair().instance(Appearance::Light);
        let text = theme.color(ColorToken::Text);
        let surface = theme.color(ColorToken::Surface);
        let span = f32::from(surface.r) - f32::from(text.r);
        for (radius, sigma) in [(6.0_f32, 4.0_f32), (10.0, 8.0), (0.0, 3.0), (3.0, 1.5)] {
            for scale in [1.0_f32, 2.0] {
                let rect = Rect::new(30.0, 30.0, 90.0, 70.0);
                let mut list = DisplayList::new();
                list.record(SegmentId(1), |r| {
                    r.shadow(RoundedRect::new(rect, radius), sigma, ColorToken::Text);
                });
                let side = (120.0 * scale) as u32;
                let image = CpuRenderer::with_threads(0)
                    .render_to_image(&list, &theme, side, side, scale)
                    .unwrap();
                let mut worst = 0.0_f32;
                for y in 0..side {
                    for x in 0..side {
                        let got = (f32::from(surface.r) - f32::from(image.pixel(x, y).unwrap()[0]))
                            / span;
                        let want = shadow::coverage(
                            x as f32 + 0.5,
                            y as f32 + 0.5,
                            rect.x0 * scale,
                            rect.y0 * scale,
                            rect.x1 * scale,
                            rect.y1 * scale,
                            radius * scale,
                            sigma * scale,
                        );
                        worst = worst.max((got - want).abs() * 255.0);
                    }
                }
                assert!(
                    worst <= 2.5,
                    "radius {radius}, sigma {sigma}, scale {scale}: {worst} steps"
                );
            }
        }
    }
}
