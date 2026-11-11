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

/// The `tiny-skia` renderer (ADR-0039 D9, confirmed by ADR-0043). Keep one per window (or per headless job) and reuse it: its buffers
/// are what makes later frames allocation-free in our code.
#[derive(Default)]
pub struct CpuRenderer {
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
    /// Shadow coverages computed in recent frames (ADR-0043): a shadow's pixels are computed once,
    /// then only blended.
    shadows: Vec<CachedShadow>,
    /// Frames rendered so far (for the shadow cache's eviction).
    frame_number: u64,
    /// The size of the frame being rendered, device pixels (set by `render`).
    surface: (u32, u32),
}

/// What identifies a shadow's coverage: its shape in the frame's device pixels, its radius and
/// its blur, as exact bits (the colour is applied when blending, so it is not part of the key).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ShadowKey {
    /// The shape's edges (x0, y0, x1, y1), device pixels of the frame, as `f32` bits.
    rect: [u32; 4],
    /// Corner radius, device pixels, as bits.
    radius: u32,
    /// Standard deviation, device pixels, as bits.
    sigma: u32,
    /// The frame's size: the shadow is computed only inside it.
    surface: (u32, u32),
}

/// A shadow's coverage over its reach ([`SHADOW_REACH_SIGMAS`] standard deviations around the
/// shape), clamped to the frame, kept between frames.
struct CachedShadow {
    /// What it is.
    key: ShadowKey,
    /// The covered rectangle, in the frame's device pixels: `x, y, width, height`.
    area: DeviceRect,
    /// [`shadow::coverage`] (0–1) at each pixel centre of `area`, row by row.
    ///
    /// # Why coverage and not coloured pixels?
    /// ADR-0043 K2: the coverage depends only on the shape, so one entry serves every colour and
    /// theme, and keeping it in `f32` lets each frame blend it with a single rounding to a byte,
    /// exactly as the uncached formula did. Storing coloured 8-bit pixels and compositing them
    /// with `tiny-skia` rounds twice and was measured 1.6 steps of 255 off the formula. The
    /// memory is the same, four bytes a pixel.
    coverage: Vec<f32>,
    /// The part of `area` (frame device pixels) where the coverage is full (at least
    /// [`FULL_COVERAGE`]): usually most of the shadow, hidden under the panel that casts it.
    /// Empty when there is none.
    ///
    /// # Why it is kept apart
    /// Blending the whole area pixel by pixel cost 31.6 ms for a 1400 × 800 dialog at 1080p
    /// against 3.8 ms without the shadow (2026-11-11, development machine). At full coverage the
    /// blend of a channel depends only on the pixel's old value, so `shadow` fills this rectangle
    /// from a 256-entry table per channel ([`full_cover_table`]) instead, with the same single
    /// rounding, and blends only the soft band along the edges.
    full: DeviceRect,
    /// The frame number it was last drawn in, for eviction.
    last_used: u64,
}

/// The smallest coverage treated as full. Using 1 instead of a coverage `c` moves a channel by at
/// most `255 · (1 − c)` = 0.25 steps of 255, so with the final rounding the result stays within
/// the one step of the formula that ADR-0043 K2 promises.
const FULL_COVERAGE: f32 = 1.0 - 0.25 / 255.0;

/// The most shadows kept between frames: a window shows a handful (panels, menus, dialogs).
const MAX_CACHED_SHADOWS: usize = 32;

impl std::fmt::Debug for CpuRenderer {
    /// Only the sizes of the reusable buffers.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CpuRenderer")
            .field("scratch_bytes", &self.scratch.capacity())
            .field("spare_masks", &self.mask_pool.len())
            .finish_non_exhaustive()
    }
}

impl CpuRenderer {
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
                self.draw_region(list, theme, scale, area, pixmap);
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
            self.draw_region(list, theme, scale, area, pixmap);
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
    /// Every command draws its whole intersection with the pixmap: the shadow is composited from
    /// its cached coverage (ADR-0043), so it no longer needs to know which part will be kept.
    fn draw_region(
        &mut self,
        list: &DisplayList,
        theme: &Theme,
        scale: f32,
        region: DeviceRect,
        mut pixmap: PixmapMut<'_>,
    ) {
        pixmap.fill(skia_color(theme, Paint::token(ColorToken::Surface)));
        // The region in logical pixels, for culling against the bounds the list recorded.
        let logical = region.logical(scale);
        let mut frame = Frame {
            pixmap,
            scale,
            origin: Point::new(to_f32(region.x), to_f32(region.y)),
            logical,
            corner: (region.x, region.y),
            surface: self.surface,
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
        let local = frame.to_device_rect(transform, &shape.rect);
        let radius = shape.clamped_radius() * factor;
        let color = frame.theme.color(paint.token);
        let alpha = f32::from(color.a) * f32::from(paint.opacity) / (255.0 * 255.0);
        // Premultiplied source colour at full coverage, channels 0–255.
        let source = [
            f32::from(color.r) * alpha,
            f32::from(color.g) * alpha,
            f32::from(color.b) * alpha,
            255.0 * alpha,
        ];
        let index = self.cached_shadow(frame, &local, radius, sigma);
        let mask = self.masks.last();
        let Some(cached) = self.shadows.get_mut(index) else {
            return;
        };
        cached.last_used = self.frame_number;
        let area = cached.area;
        // The cached area is in the frame's device pixels; the pixmap shows the frame from
        // `frame.corner`. Visit only the rows and columns of the area that fall inside the
        // pixmap: in a partial redraw that is a small part of the shadow.
        let (width, height) = (frame.pixmap.width(), frame.pixmap.height());
        let (corner_x, corner_y) = frame.corner;
        let col0 = area.x.max(corner_x);
        let row0 = area.y.max(corner_y);
        let col1 = (area.x + area.width).min(corner_x.saturating_add(width));
        let row1 = (area.y + area.height).min(corner_y.saturating_add(height));
        if col0 >= col1 || row0 >= row1 {
            return;
        }
        // The full-coverage rectangle, as column and row ranges (empty ranges when there is none).
        let full = cached.full;
        let (full_col0, full_col1) = (full.x, full.x + full.width);
        let (full_row0, full_row1) = (full.y, full.y + full.height);
        // Built once per drawn shadow: 4 × 256 blends, negligible next to the pixels it serves.
        let table = full_cover_table(&source);
        let stride = width as usize;
        let data = frame.pixmap.data_mut();
        for row in row0..row1 {
            // Offsets of this row in the cached coverage and in the pixmap.
            let cached_row = (row - area.y) as usize * area.width as usize;
            let pixmap_row = (row - corner_y) as usize * stride;
            // The columns of this row at full coverage, clamped to the visited span; an empty
            // range (start == end) on rows outside the full rectangle.
            let (inner0, inner1) = if (full_row0..full_row1).contains(&row) {
                let start = full_col0.clamp(col0, col1);
                (start, full_col1.clamp(start, col1))
            } else {
                (col0, col0)
            };
            let span = |from: u32, to: u32| {
                (
                    cached_row + (from - area.x) as usize..cached_row + (to - area.x) as usize,
                    pixmap_row + (from - corner_x) as usize..pixmap_row + (to - corner_x) as usize,
                )
            };
            // The soft band left of the full rectangle, the full rectangle, the band right of it.
            for (from, to, full_cover) in [
                (col0, inner0, false),
                (inner0, inner1, true),
                (inner1, col1, false),
            ] {
                if from >= to {
                    continue;
                }
                let (coverage_range, pixel_range) = span(from, to);
                let clip = mask.and_then(|mask| mask.data().get(pixel_range.clone()));
                let Some(pixels) = data.get_mut(pixel_range.start * 4..pixel_range.end * 4) else {
                    continue;
                };
                if full_cover {
                    fill_full_cover(pixels, clip, &table, &source);
                } else if let Some(coverage) = cached.coverage.get(coverage_range) {
                    blend_span(pixels, coverage, clip, &source);
                }
            }
        }
    }

    /// The index in the cache of the shadow of `local` (the shape in the pixmap's device pixels),
    /// computing its coverage if it is not there (evicting the least recently used one when the
    /// cache is full).
    ///
    /// # Complexity
    /// O(cached shadows) to find it; a miss costs O(pixels of the reach) coverage evaluations, a
    /// few `erf` each, once.
    fn cached_shadow(
        &mut self,
        frame: &Frame<'_, '_>,
        local: &Rect,
        radius: f32,
        sigma: f32,
    ) -> usize {
        // The shape in the frame's device pixels: the coverage is valid for every region.
        let device = Rect::new(
            local.x0 + frame.origin.x,
            local.y0 + frame.origin.y,
            local.x1 + frame.origin.x,
            local.y1 + frame.origin.y,
        );
        let key = ShadowKey {
            rect: [
                device.x0.to_bits(),
                device.y0.to_bits(),
                device.x1.to_bits(),
                device.y1.to_bits(),
            ],
            radius: radius.to_bits(),
            sigma: sigma.to_bits(),
            surface: frame.surface,
        };
        if let Some(index) = self.shadows.iter().position(|cached| cached.key == key) {
            return index;
        }
        let cached = compute_shadow(key, &device, radius, sigma);
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
        self.surface = (target.width(), target.height());
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
    /// The region in logical pixels, for culling.
    logical: Rect,
    /// The region's top-left corner in whole device pixels (the same as `origin`).
    corner: (u32, u32),
    /// The size of the whole frame (the target image), device pixels.
    surface: (u32, u32),
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

/// Computes a shadow's coverage with our analytic formula (ADR-0039 D6) at every pixel centre
/// within its reach ([`SHADOW_REACH_SIGMAS`] standard deviations), clamped to the frame. An empty
/// area (a shadow entirely outside the frame) gives an empty coverage, cached all the same.
///
/// # Why a cache (ADR-0043)
/// Computing a shadow pixel by pixel costs a few `erf` per pixel. Done at every full frame, it was
/// most of the cost of the reference scene's panel (F2.06b: 25.8 ms a 1080p frame with it, 11.1 ms
/// without; 83.5 ms against 23.8 ms at 4K), and the reason the F2.06 spike made `tiny-skia` look
/// far slower than `vello_cpu`. Coverage does not depend on colour, theme or region, so it is
/// computed once per distinct shape and blur, and every frame only blends it.
///
/// # Complexity
/// O(pixels of the reach), once per distinct shadow.
fn compute_shadow(key: ShadowKey, device: &Rect, radius: f32, sigma: f32) -> CachedShadow {
    let reach = device.inflate(sigma * SHADOW_REACH_SIGMAS);
    let area = pixel_span(&reach, key.surface.0, key.surface.1).map_or(
        DeviceRect {
            x: 0,
            y: 0,
            width: 0,
            height: 0,
        },
        |(x0, y0, x1, y1)| DeviceRect {
            x: x0,
            y: y0,
            width: x1 - x0,
            height: y1 - y0,
        },
    );
    let mut coverage = Vec::with_capacity(area.width as usize * area.height as usize);
    for row in area.y..area.y + area.height {
        for col in area.x..area.x + area.width {
            coverage.push(shadow::coverage(
                to_f32(col) + 0.5,
                to_f32(row) + 0.5,
                device.x0,
                device.y0,
                device.x1,
                device.y1,
                radius,
                sigma,
            ));
        }
    }
    let full = full_rect(device, radius, sigma, area, &coverage);
    CachedShadow {
        key,
        area,
        coverage,
        full,
        last_used: 0,
    }
}

/// The rectangle of `area` (frame device pixels) where the coverage of the shadow of `device`
/// (corner `radius`, standard deviation `sigma`, device pixels) is full, or an empty rectangle.
///
/// # How
/// A pixel whose ±3σ window lies inside the shape, away from the rounded corners, is fully
/// covered (the formula divides by the share of the Gaussian inside ±3σ). So the candidate is the
/// shape shrunk by `3σ + radius` on every side, rounded inwards to whole pixels. Rounding in
/// `erf` could still leave a pixel a hair under 1, so every candidate pixel is checked against
/// [`FULL_COVERAGE`] in the coverage just computed; if one fails, there is no full rectangle and
/// the whole area is blended (always correct, only slower).
///
/// # Complexity
/// O(pixels of the candidate), once per cached shadow, next to the coverage's O(area) `erf`s.
fn full_rect(
    device: &Rect,
    radius: f32,
    sigma: f32,
    area: DeviceRect,
    coverage: &[f32],
) -> DeviceRect {
    let empty = DeviceRect {
        x: area.x,
        y: area.y,
        width: 0,
        height: 0,
    };
    let inset = sigma * SHADOW_REACH_SIGMAS + radius;
    // Whole pixels whose square lies entirely inside the inset shape:
    // ceil the near edges, floor the far ones, then clamp to the cached area.
    let x0 = (device.x0 + inset).ceil().max(to_f32(area.x));
    let y0 = (device.y0 + inset).ceil().max(to_f32(area.y));
    let x1 = (device.x1 - inset).floor().min(to_f32(area.x + area.width));
    let y1 = (device.y1 - inset)
        .floor()
        .min(to_f32(area.y + area.height));
    if !(x0 < x1 && y0 < y1) {
        return empty;
    }
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "whole, non-negative values clamped to the cached area, which fits in u32"
    )]
    let full = DeviceRect {
        x: x0 as u32,
        y: y0 as u32,
        width: (x1 - x0) as u32,
        height: (y1 - y0) as u32,
    };
    let stride = area.width as usize;
    for row in full.y..full.y + full.height {
        let start = (row - area.y) as usize * stride + (full.x - area.x) as usize;
        let all_full = coverage
            .get(start..start + full.width as usize)
            .is_some_and(|values| values.iter().all(|&value| value >= FULL_COVERAGE));
        if !all_full {
            return empty;
        }
    }
    full
}

/// For each channel, the result of [`blend_over`] at full coverage for every old value 0–255:
/// `table[channel][old] = round(source[channel] + old · (1 − source_alpha / 255))`.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "each entry is clamped to 0..=255 before the conversion"
)]
fn full_cover_table(source: &[f32; 4]) -> [[u8; 256]; 4] {
    let keep = 1.0 - source[3] / 255.0;
    let mut table = [[0_u8; 256]; 4];
    for (entries, value) in table.iter_mut().zip(source) {
        for (old, entry) in (0_u8..=255).zip(entries.iter_mut()) {
            *entry = (value + f32::from(old) * keep).round().clamp(0.0, 255.0) as u8;
        }
    }
    table
}

/// Blends `source` at full coverage over a run of `pixels` (premultiplied RGBA bytes) with the
/// precomputed `table`. Under a clip mask (`clip`, one byte a pixel), pixels the clip only
/// partly covers go through [`blend_over`] with the clip as coverage; fully clipped ones are
/// left alone.
fn fill_full_cover(
    pixels: &mut [u8],
    clip: Option<&[u8]>,
    table: &[[u8; 256]; 4],
    source: &[f32; 4],
) {
    for (index, pixel) in pixels.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        let clip = clip.map_or(255, |clip| clip.get(index).copied().unwrap_or(0));
        if clip == 255 {
            for (channel, entries) in pixel.iter_mut().zip(table) {
                *channel = entries[usize::from(*channel)];
            }
        } else if clip > 0 {
            blend_over(pixel, source, f32::from(clip) / 255.0);
        }
    }
}

/// Blends `source` over a run of `pixels` with the matching run of cached `coverage`, scaled by
/// the clip mask's bytes (`clip`) when one is in effect.
fn blend_span(pixels: &mut [u8], coverage: &[f32], clip: Option<&[u8]>, source: &[f32; 4]) {
    for (index, (pixel, &cover)) in pixels
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(coverage)
        .enumerate()
    {
        let mut cover = cover;
        // The clip in effect (an 8-bit coverage mask of the pixmap's size) scales the shadow's
        // coverage, as `tiny-skia` does for its own fills.
        if let Some(clip) = clip {
            cover *= f32::from(clip.get(index).copied().unwrap_or(0)) / 255.0;
        }
        if cover > 0.0 {
            blend_over(pixel, source, cover);
        }
    }
}

/// Blends a premultiplied `source` colour (channels 0–255) with `cover` (0–1) over `pixel`
/// (premultiplied RGBA bytes): `out = source · cover + pixel · (1 − source_alpha · cover)`.
///
/// The whole expression stays in `f32` and is rounded to a byte once, which keeps the shadow
/// within one step of 255 of the analytic formula (ADR-0043 K2).
///
/// # Why `+ 0.5` and a cast instead of `round()`
/// This runs for every pixel of a shadow's soft band, four times. On the baseline x86-64 target
/// (no SSE4.1) `f32::round` is a call into the maths library, which made it the largest cost of
/// the band. For the non-negative values here, adding 0.5 and truncating rounds the same way
/// (halves away from zero); and a float-to-integer `as` cast saturates (negative values and NaN
/// give 0, values above 255 give 255), so no separate clamp is needed.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the float-to-integer cast saturates to 0..=255, which is the clamp we want"
)]
fn blend_over(pixel: &mut [u8], source: &[f32; 4], cover: f32) {
    let keep = 1.0 - source[3] * cover / 255.0;
    for (channel, value) in pixel.iter_mut().zip(source) {
        let blended = value * cover + f32::from(*channel) * keep;
        *channel = (blended + 0.5) as u8;
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

    /// Shadows are our analytic formula blended once (ADR-0039 D6): within one step of 255 of
    /// [`shadow::coverage`] at scale 1 and 2, for rounded and square shapes, small and large
    /// blurs, on the first frame (coverage computed) and the second (coverage from the cache,
    /// ADR-0043). Coverage is read back from the red channel, between the surface's and the
    /// text's. Measured in F2.06b: 0.61 steps.
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
                let mut renderer = CpuRenderer::new();
                for pass in ["computed", "cached"] {
                    let image = renderer
                        .render_to_image(&list, &theme, side, side, scale)
                        .unwrap();
                    let mut worst = 0.0_f32;
                    for y in 0..side {
                        for x in 0..side {
                            let got = (f32::from(surface.r)
                                - f32::from(image.pixel(x, y).unwrap()[0]))
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
                        worst <= 1.0,
                        "radius {radius}, sigma {sigma}, scale {scale}, {pass}: {worst} steps"
                    );
                }
            }
        }
    }

    /// A large shadow takes the full-coverage shortcut (its cached `full` rectangle is not
    /// empty), and the result still follows the formula within one step, with and without a
    /// clip that cuts through both the soft band and the full rectangle. The clip is a whole-pixel
    /// rectangle, so its mask is 255 inside and 0 outside and the expected value is exact.
    #[test]
    fn large_shadows_fill_their_full_part_exactly() {
        // The shadow token is translucent, so the old pixel matters: a wrong full-coverage table
        // shows up here, while an opaque colour would hide it.
        let theme = *BuiltInTheme::Studio.pair().instance(Appearance::Light);
        let color = theme.color(ColorToken::Shadow);
        assert!(color.a < 255, "the test needs a translucent shadow colour");
        let alpha = f32::from(color.a) / 255.0;
        let surface = f32::from(theme.color(ColorToken::Surface).r);
        let source = f32::from(color.r) * alpha;
        let rect = Rect::new(20.0, 24.0, 300.0, 210.0);
        let (radius, sigma) = (10.0_f32, 8.0_f32);
        let clip = Rect::new(0.0, 0.0, 160.0, 240.0);
        for clipped in [false, true] {
            let mut list = DisplayList::new();
            list.record(SegmentId(1), |r| {
                if clipped {
                    r.push_clip_rect(clip);
                }
                r.shadow(RoundedRect::new(rect, radius), sigma, ColorToken::Shadow);
                if clipped {
                    r.pop_clip();
                }
            });
            let (width, height) = (320_u32, 240_u32);
            let mut renderer = CpuRenderer::new();
            for pass in ["computed", "cached"] {
                let image = renderer
                    .render_to_image(&list, &theme, width, height, 1.0)
                    .unwrap();
                let full = renderer.shadows.first().unwrap().full;
                assert!(full.width > 200 && full.height > 100, "{full:?}");
                let mut worst = 0.0_f32;
                for y in 0..height {
                    for x in 0..width {
                        let (cx, cy) = (x as f32 + 0.5, y as f32 + 0.5);
                        let inside = !clipped || (cx < clip.x1 && cy < clip.y1);
                        let cover = if inside {
                            shadow::coverage(
                                cx, cy, rect.x0, rect.y0, rect.x1, rect.y1, radius, sigma,
                            )
                        } else {
                            0.0
                        };
                        // Source over, red channel, in 0–255: the uncached formula.
                        let want = source * cover + surface * (1.0 - alpha * cover);
                        let got = f32::from(image.pixel(x, y).unwrap()[0]);
                        worst = worst.max((got - want).abs());
                    }
                }
                assert!(worst <= 1.0, "clipped {clipped}, {pass}: {worst} steps");
            }
        }
    }
}
