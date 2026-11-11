// SPDX-License-Identifier: GPL-3.0-or-later

//! The CPU backend on `tiny-skia` (ADR-0039 D9): turns a display list into pixels of an
//! [`Image`], redrawing only the damage (D5), resolving theme tokens at draw time (D3), snapping
//! straight edges to device pixels (D7) and drawing shadows analytically (D6).
//!
//! This is the only file of the crate that knows about `tiny-skia`.
//!
//! # How a frame is drawn
//! 1. The damage's logical rectangles become device rectangles (times the scale, rounded
//!    outwards, plus one pixel of anti-aliasing fringe, clamped to the target). If they cover
//!    more than half the frame, the whole frame is drawn instead (D5).
//! 2. The whole frame is drawn straight into the target. A partial region is drawn into a
//!    scratch buffer of the region's size (reused between frames), with the drawing transform
//!    shifted by the region's origin, then copied row by row into the target. This costs work
//!    proportional to the region, with no clip mask for the damage itself.
//! 3. A region is first cleared with the theme's `surface` token; then every segment whose bounds
//!    meet the region is replayed, skipping drawing commands whose bounds do not (culling).
//!
//! # Allocations
//! The scratch buffer, the path builder and the clip masks are kept by the renderer and reused,
//! so a steady stream of frames of similar size stops allocating in our code. `tiny-skia` itself
//! allocates internally while filling paths (edge lists): that is its business, and the
//! benchmark counts its instructions too.

use tiny_skia::{
    BlendMode, Color as SkiaColor, FillRule as SkiaFillRule, IntSize, LineCap as SkiaLineCap,
    LineJoin as SkiaLineJoin, Mask, Paint as SkiaPaint, PathBuilder as SkiaPathBuilder, PixmapMut,
    PixmapPaint, PixmapRef, Rect as SkiaRect, Shader, Stroke, Transform as SkiaTransform,
};

use super::Renderer;
use super::command::{Command, PathElement};
use super::damage::Damage;
use super::device::{DeviceRect, DeviceRegions, device_region, pixel_span, to_f32};
use super::geometry::{Point, Rect, RoundedRect, Transform};
use super::image::{DrawError, Image};
use super::list::{DisplayList, SHADOW_REACH_SIGMAS, Segment};
use super::paint::{FillRule, LineCap, LineJoin, Paint, StrokeStyle};
use super::shadow;
use super::snap::{KAPPA, snap_rect as snap, snapped_line};
use crate::{ColorToken, Theme};

/// The `tiny-skia` renderer, kept during F2.06b only to compare it with the `vello_cpu` backend
/// (ADR-0042 R5); it leaves the workspace at the end of the step. Keep one per window (or per headless job) and reuse it: its buffers
/// are what makes later frames allocation-free in our code.
#[derive(Default)]
pub struct TinySkiaRenderer {
    /// Pixels of a partial region, reused between frames.
    scratch: Vec<u8>,
    /// A path builder whose buffers are reused from path to path.
    builder: Option<SkiaPathBuilder>,
    /// Spare buffers for clip masks.
    mask_pool: Vec<Vec<u8>>,
    /// Transforms in effect while replaying a segment (local → list coordinates).
    transforms: Vec<Transform>,
    /// Clip masks in effect while replaying a segment, innermost last.
    masks: Vec<Mask>,
}

impl std::fmt::Debug for TinySkiaRenderer {
    /// Only the sizes of the reusable buffers.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TinySkiaRenderer")
            .field("scratch_bytes", &self.scratch.capacity())
            .field("spare_masks", &self.mask_pool.len())
            .finish_non_exhaustive()
    }
}

impl TinySkiaRenderer {
    /// A renderer with empty buffers.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
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

    /// Draws one device region: into the target itself when it is the whole frame, otherwise
    /// into the scratch buffer and then copied.
    ///
    /// # Why the scratch area is larger than the region
    /// `tiny-skia` rasterises an anti-aliased path differently depending on whether the path is
    /// entirely inside the pixmap it draws into (it takes another clipping path otherwise). A
    /// notehead cut by the edge of a small scratch pixmap would get edge pixels a few units off
    /// the full frame's (found by the owner in F2.04a: up to 28 of 255). So the scratch area is
    /// the region grown to contain **every command (and clip) that meets it**, clamped to the
    /// frame: then each path is either wholly inside the scratch area, as in the full frame, or
    /// cut by the frame's own edge, as in the full frame; its pixels are the full frame's, shifted
    /// by whole pixels. Only the region itself is copied back.
    ///
    /// # Cost
    /// A region over a large shape (a panel's rounded background) redraws the whole shape. That
    /// is the price of exactness; plain rectangles and lines are drawn without anti-aliasing
    /// (ADR-0039 D7) and still count, conservatively.
    fn render_region(
        &mut self,
        list: &DisplayList,
        theme: &Theme,
        scale: f32,
        region: DeviceRect,
        target: &mut Image,
    ) {
        let (width, height) = (target.width(), target.height());
        let area = Self::scratch_area(list, scale, region, width, height);
        let whole = area.x == 0 && area.y == 0 && area.width == width && area.height == height;
        if whole {
            // Drawing the whole frame gives every pixel its final value, the region's included.
            if let Some(pixmap) = PixmapMut::from_bytes(target.data_mut(), width, height) {
                self.draw_region(list, theme, scale, area, area, pixmap);
            }
            return;
        }
        let len = area.area().saturating_mul(4);
        let Ok(len) = usize::try_from(len) else {
            return;
        };
        let mut scratch = std::mem::take(&mut self.scratch);
        scratch.clear();
        scratch.resize(len, 0);
        if let Some(pixmap) = PixmapMut::from_bytes(&mut scratch, area.width, area.height) {
            self.draw_region(list, theme, scale, area, region, pixmap);
        }
        copy_region(&scratch, area, region, target);
        self.scratch = scratch;
    }

    /// The device area a partial redraw of `region` must draw: the region grown by the device
    /// bounds of every drawing command and clip of the segments that meet it, when they meet it,
    /// clamped to the frame (see [`render_region`](Self::render_region)).
    ///
    /// # Complexity
    /// O(commands of the segments meeting the region), the same scan the replay does.
    fn scratch_area(
        list: &DisplayList,
        scale: f32,
        region: DeviceRect,
        width: u32,
        height: u32,
    ) -> DeviceRect {
        if region.x == 0 && region.y == 0 && region.width == width && region.height == height {
            // Already the whole frame: nothing can grow it (and the scan would cost a full pass).
            return region;
        }
        let logical = region.logical(scale);
        let mut area = region;
        for segment in list.segments() {
            if !segment.bounds.intersects(&logical) {
                continue;
            }
            for entry in &segment.entries {
                if needs_whole_shape(&entry.command)
                    && entry.bounds.intersects(&logical)
                    && let Some(device) = device_region(&entry.bounds, scale, width, height)
                {
                    area = area.union(&device);
                }
            }
        }
        area
    }

    /// Clears `pixmap` (which shows `region` of the frame) and replays the segments that meet it.
    /// `needed` is the part of `region` that will be kept (copied to the target): commands whose
    /// pixels do not depend on the pixmap (the analytic shadow) only compute that part.
    fn draw_region(
        &mut self,
        list: &DisplayList,
        theme: &Theme,
        scale: f32,
        region: DeviceRect,
        needed: DeviceRect,
        mut pixmap: PixmapMut<'_>,
    ) {
        pixmap.fill(skia_color(theme, Paint::token(ColorToken::Surface)));
        // The region in logical pixels, for culling against the bounds the list recorded.
        let logical = region.logical(scale);
        let mut frame = Frame {
            pixmap,
            scale,
            origin: Point::new(to_f32(region.x), to_f32(region.y)),
            needed: Rect::new(
                to_f32(needed.x - region.x),
                to_f32(needed.y - region.y),
                to_f32(needed.x - region.x + needed.width),
                to_f32(needed.y - region.y + needed.height),
            ),
            logical,
            theme,
            list,
        };
        for segment in list.segments() {
            if segment.bounds.intersects(&logical) {
                self.replay(segment, &mut frame);
            }
        }
    }

    /// Replays one segment's commands into the frame.
    fn replay(&mut self, segment: &Segment, frame: &mut Frame<'_, '_>) {
        self.transforms.clear();
        for mask in self.masks.drain(..) {
            self.mask_pool.push(mask.take());
        }
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
                    self.fill_snapped(frame, &transform, rect, *paint);
                }
                Command::FillRoundedRect { shape, paint } => {
                    if shape.clamped_radius() <= 0.0 {
                        self.fill_snapped(frame, &transform, &shape.rect, *paint);
                    } else {
                        self.fill_rounded(frame, &transform, shape, *paint);
                    }
                }
                Command::StrokeRect {
                    rect,
                    stroke,
                    paint,
                } => self.stroke_snapped_rect(frame, &transform, rect, *stroke, *paint),
                Command::StrokeRoundedRect {
                    shape,
                    stroke,
                    paint,
                } => {
                    if shape.clamped_radius() <= 0.0 {
                        self.stroke_snapped_rect(frame, &transform, &shape.rect, *stroke, *paint);
                    } else {
                        self.stroke_rounded(frame, &transform, shape, *stroke, *paint);
                    }
                }
                Command::Line {
                    from,
                    to,
                    stroke,
                    paint,
                } => self.line(frame, &transform, *from, *to, *stroke, *paint),
                Command::FillPath {
                    elements,
                    rule,
                    paint,
                } => {
                    let elements = segment.elements.get(elements.clone()).unwrap_or(&[]);
                    self.fill_path(frame, &transform, elements, *rule, *paint);
                }
                Command::StrokePath {
                    elements,
                    stroke,
                    paint,
                } => {
                    let elements = segment.elements.get(elements.clone()).unwrap_or(&[]);
                    self.stroke_path(frame, &transform, elements, *stroke, *paint);
                }
                Command::Shadow { shape, blur, paint } => {
                    self.shadow(frame, &transform, shape, *blur, *paint);
                }
                Command::Image { image, dest } => self.image(frame, &transform, *image, dest),
                Command::PushClip { shape } => self.push_clip(frame, &transform, shape),
                Command::PopClip => {
                    if let Some(mask) = self.masks.pop() {
                        self.mask_pool.push(mask.take());
                    }
                }
                Command::PushTransform(inner) => {
                    self.transforms.push(transform.then(inner));
                }
                Command::PopTransform => {
                    self.transforms.pop();
                }
            }
        }
    }

    /// Fills an axis-aligned rectangle snapped to device pixels, without anti-aliasing (D7).
    fn fill_snapped(
        &self,
        frame: &mut Frame<'_, '_>,
        transform: &Transform,
        rect: &Rect,
        paint: Paint,
    ) {
        let device = frame.to_device_rect(transform, rect);
        if let Some(snapped) = snap_rect(&device) {
            let skia_paint = skia_paint(frame.theme, paint, false);
            frame.pixmap.fill_rect(
                snapped,
                &skia_paint,
                SkiaTransform::identity(),
                self.masks.last(),
            );
        }
    }

    /// Strokes the outline of an axis-aligned rectangle as four snapped bands (D7).
    fn stroke_snapped_rect(
        &self,
        frame: &mut Frame<'_, '_>,
        transform: &Transform,
        rect: &Rect,
        stroke: StrokeStyle,
        paint: Paint,
    ) {
        let width = stroke.width * transform.scale * frame.scale;
        let band = width.round().max(1.0);
        let device = frame.to_device_rect(transform, rect).inflate(width * 0.5);
        let Some(outer) = snap_rect(&device) else {
            return;
        };
        let skia_paint = skia_paint(frame.theme, paint, false);
        let mask = self.masks.last();
        let (x0, y0, x1, y1) = (outer.left(), outer.top(), outer.right(), outer.bottom());
        if x1 - x0 <= 2.0 * band || y1 - y0 <= 2.0 * band {
            // Too small for a hole: the outline is the whole rectangle.
            frame
                .pixmap
                .fill_rect(outer, &skia_paint, SkiaTransform::identity(), mask);
            return;
        }
        let bands = [
            SkiaRect::from_ltrb(x0, y0, x1, y0 + band),
            SkiaRect::from_ltrb(x0, y1 - band, x1, y1),
            SkiaRect::from_ltrb(x0, y0 + band, x0 + band, y1 - band),
            SkiaRect::from_ltrb(x1 - band, y0 + band, x1, y1 - band),
        ];
        for band in bands.into_iter().flatten() {
            frame
                .pixmap
                .fill_rect(band, &skia_paint, SkiaTransform::identity(), mask);
        }
    }

    /// Draws a line: snapped when horizontal or vertical with butt or square caps, otherwise
    /// stroked with anti-aliasing.
    fn line(
        &mut self,
        frame: &mut Frame<'_, '_>,
        transform: &Transform,
        from: Point,
        to: Point,
        stroke: StrokeStyle,
        paint: Paint,
    ) {
        let a = frame.to_device(transform, from);
        let b = frame.to_device(transform, to);
        let width = stroke.width * transform.scale * frame.scale;
        let extend = match stroke.cap {
            LineCap::Butt => Some(0.0),
            LineCap::Square => Some(width * 0.5),
            LineCap::Round => None,
        };
        if let Some(extend) = extend
            && let Some(rect) = snapped_line((a.x, a.y), (b.x, b.y), width, extend)
                .and_then(|r| SkiaRect::from_ltrb(r.x0, r.y0, r.x1, r.y1))
        {
            let skia_paint = skia_paint(frame.theme, paint, false);
            frame.pixmap.fill_rect(
                rect,
                &skia_paint,
                SkiaTransform::identity(),
                self.masks.last(),
            );
            return;
        }
        let mut builder = self.builder.take().unwrap_or_default();
        builder.move_to(from.x, from.y);
        builder.line_to(to.x, to.y);
        self.stroke_built(frame, transform, builder, stroke, paint);
    }

    /// Fills a rounded rectangle with anti-aliasing.
    fn fill_rounded(
        &mut self,
        frame: &mut Frame<'_, '_>,
        transform: &Transform,
        shape: &RoundedRect,
        paint: Paint,
    ) {
        let mut builder = self.builder.take().unwrap_or_default();
        push_rounded_rect(&mut builder, shape);
        self.fill_built(frame, transform, builder, FillRule::NonZero, paint);
    }

    /// Strokes a rounded rectangle with anti-aliasing.
    fn stroke_rounded(
        &mut self,
        frame: &mut Frame<'_, '_>,
        transform: &Transform,
        shape: &RoundedRect,
        stroke: StrokeStyle,
        paint: Paint,
    ) {
        let mut builder = self.builder.take().unwrap_or_default();
        push_rounded_rect(&mut builder, shape);
        self.stroke_built(frame, transform, builder, stroke, paint);
    }

    /// Fills a path of the segment's arena.
    fn fill_path(
        &mut self,
        frame: &mut Frame<'_, '_>,
        transform: &Transform,
        elements: &[PathElement],
        rule: FillRule,
        paint: Paint,
    ) {
        let mut builder = self.builder.take().unwrap_or_default();
        push_elements(&mut builder, elements);
        self.fill_built(frame, transform, builder, rule, paint);
    }

    /// Strokes a path of the segment's arena.
    fn stroke_path(
        &mut self,
        frame: &mut Frame<'_, '_>,
        transform: &Transform,
        elements: &[PathElement],
        stroke: StrokeStyle,
        paint: Paint,
    ) {
        let mut builder = self.builder.take().unwrap_or_default();
        push_elements(&mut builder, elements);
        self.stroke_built(frame, transform, builder, stroke, paint);
    }

    /// Finishes `builder` and fills the path, then keeps the builder's buffers for the next path.
    fn fill_built(
        &mut self,
        frame: &mut Frame<'_, '_>,
        transform: &Transform,
        builder: SkiaPathBuilder,
        rule: FillRule,
        paint: Paint,
    ) {
        let Some(path) = builder.finish() else {
            // An empty or degenerate path draws nothing; its builder is gone, a new one will be
            // made for the next path (rare).
            return;
        };
        let skia_paint = skia_paint(frame.theme, paint, true);
        frame.pixmap.fill_path(
            &path,
            &skia_paint,
            skia_fill_rule(rule),
            frame.skia_transform(transform),
            self.masks.last(),
        );
        self.builder = Some(path.clear());
    }

    /// Finishes `builder` and strokes the path, then keeps the builder's buffers.
    fn stroke_built(
        &mut self,
        frame: &mut Frame<'_, '_>,
        transform: &Transform,
        builder: SkiaPathBuilder,
        stroke: StrokeStyle,
        paint: Paint,
    ) {
        let Some(path) = builder.finish() else {
            return;
        };
        let skia_paint = skia_paint(frame.theme, paint, true);
        let skia_stroke = Stroke {
            width: stroke.width,
            miter_limit: StrokeStyle::MITER_LIMIT,
            line_cap: match stroke.cap {
                LineCap::Butt => SkiaLineCap::Butt,
                LineCap::Round => SkiaLineCap::Round,
                LineCap::Square => SkiaLineCap::Square,
            },
            line_join: match stroke.join {
                LineJoin::Miter => SkiaLineJoin::Miter,
                LineJoin::Round => SkiaLineJoin::Round,
                LineJoin::Bevel => SkiaLineJoin::Bevel,
            },
            dash: None,
        };
        frame.pixmap.stroke_path(
            &path,
            &skia_paint,
            &skia_stroke,
            frame.skia_transform(transform),
            self.masks.last(),
        );
        self.builder = Some(path.clear());
    }

    /// Draws a shadow analytically (D6): every pixel of the blurred bounds inside the frame gets
    /// the shadow colour with the coverage of [`shadow::coverage`], blended over what is there.
    fn shadow(
        &mut self,
        frame: &mut Frame<'_, '_>,
        transform: &Transform,
        shape: &RoundedRect,
        blur: f32,
        paint: Paint,
    ) {
        let factor = transform.scale * frame.scale;
        let sigma = blur * factor;
        if sigma <= 0.0 {
            // No blur: an ordinary fill. Any positive blur, however small, is computed
            // analytically, so its pixels never depend on the scratch area (`needs_whole_shape`).
            if shape.clamped_radius() <= 0.0 {
                self.fill_snapped(frame, transform, &shape.rect, paint);
            } else {
                self.fill_rounded(frame, transform, shape, paint);
            }
            return;
        }
        let device = frame.to_device_rect(transform, &shape.rect);
        let radius = shape.clamped_radius() * factor;
        // Only the pixels that are both within the blur's reach and kept (`needed`): a shadow's
        // pixels do not depend on the pixmap, so the rest of a grown scratch area is skipped.
        let reach = device
            .inflate(sigma * SHADOW_REACH_SIGMAS)
            .intersect(&frame.needed);
        let width = frame.pixmap.width();
        let height = frame.pixmap.height();
        // The pixels to visit: the blurred bounds, clamped to the pixmap.
        let Some((col0, row0, col1, row1)) = pixel_span(&reach, width, height) else {
            return;
        };
        let color = frame.theme.color(paint.token);
        let alpha = f32::from(color.a) * f32::from(paint.opacity) / (255.0 * 255.0);
        // Premultiplied source colour at full coverage, channels 0–255.
        let source = [
            f32::from(color.r) * alpha,
            f32::from(color.g) * alpha,
            f32::from(color.b) * alpha,
            255.0 * alpha,
        ];
        let mask = self.masks.last();
        let stride = width as usize;
        let data = frame.pixmap.data_mut();
        for row in row0..row1 {
            for col in col0..col1 {
                let mut cover = shadow::coverage(
                    to_f32(col) + 0.5,
                    to_f32(row) + 0.5,
                    device.x0,
                    device.y0,
                    device.x1,
                    device.y1,
                    radius,
                    sigma,
                );
                let index = row as usize * stride + col as usize;
                if let Some(mask) = mask {
                    let clip = mask.data().get(index).copied().unwrap_or(0);
                    cover *= f32::from(clip) / 255.0;
                }
                if cover <= 0.0 {
                    continue;
                }
                if let Some(pixel) = data.get_mut(index * 4..index * 4 + 4) {
                    blend_over(pixel, &source, cover);
                }
            }
        }
    }

    /// Draws a stored picture stretched into `dest`, with bilinear filtering.
    fn image(
        &self,
        frame: &mut Frame<'_, '_>,
        transform: &Transform,
        id: super::image::ImageId,
        dest: &Rect,
    ) {
        let Some(image) = frame.list.image(id) else {
            return;
        };
        let Some(source) = PixmapRef::from_bytes(image.data(), image.width(), image.height())
        else {
            return;
        };
        let sx = dest.width() / to_f32(image.width());
        let sy = dest.height() / to_f32(image.height());
        let placement = frame
            .skia_transform(transform)
            .pre_translate(dest.x0, dest.y0)
            .pre_scale(sx, sy);
        let paint = PixmapPaint {
            quality: tiny_skia::FilterQuality::Bilinear,
            ..PixmapPaint::default()
        };
        frame
            .pixmap
            .draw_pixmap(0, 0, source, &paint, placement, self.masks.last());
    }

    /// Starts a clip: a mask of the pixmap's size covering the shape (intersected with the clip
    /// already in effect). Rectangular clips are snapped to device pixels so their edges are
    /// sharp; rounded ones are anti-aliased.
    fn push_clip(&mut self, frame: &mut Frame<'_, '_>, transform: &Transform, shape: &RoundedRect) {
        let width = frame.pixmap.width();
        let height = frame.pixmap.height();
        let Some(size) = IntSize::from_wh(width, height) else {
            return;
        };
        let len = width as usize * height as usize;
        let mut buffer = self.mask_pool.pop().unwrap_or_default();
        buffer.clear();
        match self.masks.last() {
            // Nested: start from the outer clip, then intersect.
            Some(outer) => buffer.extend_from_slice(outer.data()),
            None => buffer.resize(len, 0),
        }
        let Some(mut mask) = Mask::from_vec(buffer, size) else {
            return;
        };
        // The path of the clip and the transform to draw it with.
        let mut builder = self.builder.take().unwrap_or_default();
        let path_transform = if shape.clamped_radius() <= 0.0 {
            // Snap the rectangle in device space and draw it untransformed.
            let device = frame.to_device_rect(transform, &shape.rect);
            let Some(snapped) = snap_rect(&device) else {
                // An empty clip hides everything: a mask of zeros.
                mask.clear();
                self.builder = Some(builder);
                self.masks.push(mask);
                return;
            };
            builder.push_rect(snapped);
            SkiaTransform::identity()
        } else {
            push_rounded_rect(&mut builder, shape);
            frame.skia_transform(transform)
        };
        let Some(path) = builder.finish() else {
            mask.clear();
            self.masks.push(mask);
            return;
        };
        if self.masks.is_empty() {
            mask.fill_path(&path, SkiaFillRule::Winding, true, path_transform);
        } else {
            mask.intersect_path(&path, SkiaFillRule::Winding, true, path_transform);
        }
        self.builder = Some(path.clear());
        self.masks.push(mask);
    }
}

impl Renderer for TinySkiaRenderer {
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
        let regions = DeviceRegions::new(damage, scale, target.width(), target.height());
        for region in regions.iter() {
            self.render_region(list, theme, scale, *region, target);
        }
        Ok(())
    }
}

/// `true` for the commands whose pixels depend on the scratch area they are drawn into: the ones
/// `tiny-skia` fills or strokes as anti-aliased paths, and clips (their masks are paths too).
///
/// The others give the same pixels whatever the area: snapped rectangles and straight lines are
/// filled without anti-aliasing on whole pixels (ADR-0039 D7), and a blurred shadow is computed
/// pixel by pixel from a formula. Growing the scratch area for them would only cost time: the
/// owner saw it in F2.04a, where growing it for the demonstration's shadowed panel made every
/// pointer move redraw the whole shadow (68 ms a frame).
fn needs_whole_shape(command: &Command) -> bool {
    match command {
        Command::FillRoundedRect { shape, .. } | Command::StrokeRoundedRect { shape, .. } => {
            shape.clamped_radius() > 0.0
        }
        // A line is snapped only when it is horizontal or vertical with butt or square caps;
        // our transforms (translate, uniform scale) keep it so. "Straight" means bit-for-bit equal
        // coordinates on purpose: then the renderer, which compares the transformed coordinates,
        // certainly sees it straight too; a nearly straight line counts as a path (the safe side:
        // growing the area is never wrong, only slower).
        Command::Line {
            from, to, stroke, ..
        } => {
            let straight = from.x.to_bits() == to.x.to_bits() || from.y.to_bits() == to.y.to_bits();
            !straight || stroke.cap == LineCap::Round
        }
        // A sharp shadow (blur 0) is an ordinary fill, a path when its corners are rounded.
        Command::Shadow { shape, blur, .. } => *blur <= 0.0 && shape.clamped_radius() > 0.0,
        Command::FillPath { .. }
        | Command::StrokePath { .. }
        | Command::Image { .. }
        | Command::PushClip { .. } => true,
        Command::FillRect { .. }
        | Command::StrokeRect { .. }
        | Command::PopClip
        | Command::PushTransform(_)
        | Command::PopTransform => false,
    }
}

/// What one region's replay needs to know.
struct Frame<'p, 'a> {
    /// Where the pixels go (the target, or the scratch buffer of a partial region).
    pixmap: PixmapMut<'p>,
    /// Device pixels per logical pixel.
    scale: f32,
    /// The region's top-left corner in the frame, device pixels: subtracted from every
    /// coordinate, so the pixmap's (0, 0) is the region's corner.
    origin: Point,
    /// The part of the pixmap that will be kept, in the pixmap's device pixels (the whole pixmap
    /// unless the scratch area was grown, see `render_region`).
    needed: Rect,
    /// The region in logical pixels, for culling.
    logical: Rect,
    /// The theme the tokens resolve with.
    theme: &'a Theme,
    /// The list (for its images).
    list: &'a DisplayList,
}

impl Frame<'_, '_> {
    /// A local point in the pixmap's device coordinates.
    fn to_device(&self, transform: &Transform, point: Point) -> Point {
        let list = transform.apply(point);
        Point::new(
            list.x * self.scale - self.origin.x,
            list.y * self.scale - self.origin.y,
        )
    }

    /// A local rectangle in the pixmap's device coordinates.
    fn to_device_rect(&self, transform: &Transform, rect: &Rect) -> Rect {
        let a = self.to_device(transform, Point::new(rect.x0, rect.y0));
        let b = self.to_device(transform, Point::new(rect.x1, rect.y1));
        Rect::new(a.x, a.y, b.x, b.y)
    }

    /// The `tiny-skia` transform from local coordinates to the pixmap's device pixels.
    fn skia_transform(&self, transform: &Transform) -> SkiaTransform {
        let scale = transform.scale * self.scale;
        SkiaTransform::from_row(
            scale,
            0.0,
            0.0,
            scale,
            transform.offset.x * self.scale - self.origin.x,
            transform.offset.y * self.scale - self.origin.y,
        )
    }
}

/// The paint for a token, resolved with the theme (D3), with or without anti-aliasing.
fn skia_paint(theme: &Theme, paint: Paint, anti_alias: bool) -> SkiaPaint<'static> {
    SkiaPaint {
        shader: Shader::SolidColor(skia_color(theme, paint)),
        anti_alias,
        blend_mode: BlendMode::SourceOver,
        ..SkiaPaint::default()
    }
}

/// The colour of a token in the theme, with the paint's extra opacity multiplied into its alpha.
fn skia_color(theme: &Theme, paint: Paint) -> SkiaColor {
    let color = theme.color(paint.token);
    // (a · opacity + 127) / 255: the product of two 8-bit fractions, rounded to nearest.
    let alpha = (u16::from(color.a) * u16::from(paint.opacity) + 127) / 255;
    let alpha = u8::try_from(alpha).unwrap_or(u8::MAX);
    SkiaColor::from_rgba8(color.r, color.g, color.b, alpha)
}

/// [`snap`] as a `tiny-skia` rectangle.
fn snap_rect(rect: &Rect) -> Option<SkiaRect> {
    snap(rect).and_then(|r| SkiaRect::from_ltrb(r.x0, r.y0, r.x1, r.y1))
}

/// Our fill rule as `tiny-skia`'s.
const fn skia_fill_rule(rule: FillRule) -> SkiaFillRule {
    match rule {
        FillRule::NonZero => SkiaFillRule::Winding,
        FillRule::EvenOdd => SkiaFillRule::EvenOdd,
    }
}

/// Blends a premultiplied `source` colour (channels 0–255) with `cover` (0–1) over `pixel`
/// (premultiplied RGBA bytes): `out = source · cover + pixel · (1 − source_alpha · cover)`.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "each channel is clamped to 0..=255 before the conversion"
)]
fn blend_over(pixel: &mut [u8], source: &[f32; 4], cover: f32) {
    let keep = 1.0 - source[3] * cover / 255.0;
    for (channel, value) in pixel.iter_mut().zip(source) {
        let blended = value * cover + f32::from(*channel) * keep;
        *channel = blended.round().clamp(0.0, 255.0) as u8;
    }
}

/// Copies `region` of the frame from `scratch`, which holds the drawn `area` (a rectangle of the
/// frame containing `region`, `area.width × area.height` pixels), into its place in `target`, row
/// by row.
fn copy_region(scratch: &[u8], area: DeviceRect, region: DeviceRect, target: &mut Image) {
    let scratch_stride = area.width as usize * 4;
    let target_stride = target.width() as usize * 4;
    let row_bytes = region.width as usize * 4;
    let column = (region.x - area.x) as usize * 4;
    let data = target.data_mut();
    for row in 0..region.height as usize {
        let source_start =
            (region.y - area.y) as usize * scratch_stride + row * scratch_stride + column;
        let source = scratch.get(source_start..source_start + row_bytes);
        let start = (region.y as usize + row) * target_stride + region.x as usize * 4;
        let destination = data.get_mut(start..start + row_bytes);
        if let (Some(source), Some(destination)) = (source, destination) {
            destination.copy_from_slice(source);
        }
    }
}

/// Appends a rounded rectangle to `builder`: four straight sides and four quarter circles, each
/// a cubic Bézier with the [`KAPPA`] control distance. A radius of 0 is a plain rectangle.
fn push_rounded_rect(builder: &mut SkiaPathBuilder, shape: &RoundedRect) {
    let Rect { x0, y0, x1, y1 } = shape.rect;
    let r = shape.clamped_radius();
    if r <= 0.0 {
        if let Some(rect) = SkiaRect::from_ltrb(x0, y0, x1, y1) {
            builder.push_rect(rect);
        }
        return;
    }
    let k = r * KAPPA;
    builder.move_to(x0 + r, y0);
    builder.line_to(x1 - r, y0);
    builder.cubic_to(x1 - r + k, y0, x1, y0 + r - k, x1, y0 + r);
    builder.line_to(x1, y1 - r);
    builder.cubic_to(x1, y1 - r + k, x1 - r + k, y1, x1 - r, y1);
    builder.line_to(x0 + r, y1);
    builder.cubic_to(x0 + r - k, y1, x0, y1 - r + k, x0, y1 - r);
    builder.line_to(x0, y0 + r);
    builder.cubic_to(x0, y0 + r - k, x0 + r - k, y0, x0 + r, y0);
    builder.close();
}

/// Appends the elements of a recorded path to `builder`.
fn push_elements(builder: &mut SkiaPathBuilder, elements: &[PathElement]) {
    for element in elements {
        match *element {
            PathElement::MoveTo(p) => builder.move_to(p.x, p.y),
            PathElement::LineTo(p) => builder.line_to(p.x, p.y),
            PathElement::QuadTo(c, p) => builder.quad_to(c.x, c.y, p.x, p.y),
            PathElement::CubicTo(c1, c2, p) => builder.cubic_to(c1.x, c1.y, c2.x, c2.y, p.x, p.y),
            PathElement::Close => builder.close(),
        }
    }
}
