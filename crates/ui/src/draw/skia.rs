// SPDX-License-Identifier: GPL-3.0-or-later

//! The CPU backend on `tiny-skia` (ADR-0039 D9): turns a display list into pixels of an
//! [`Image`], redrawing only the damage (D5), resolving theme tokens at draw time (D3), snapping
//! straight edges to device pixels (D7), drawing shadows analytically (D6) and glyphs from a
//! cache of coverage masks (ADR-0044 TX6–TX8).
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

use orchestra_text::{FaceId, Faces, GlyphId, OutlineSink};

use super::Renderer;
use super::command::{Command, GlyphPlacement, PathElement};
use super::damage::Damage;
use super::device::{DeviceRect, DeviceRegions, device_region, pixel_span, to_f32};
use super::geometry::{Point, Rect, RoundedRect, Transform};
use super::glyph_cache::{
    CoverageMask, GLYPH_CACHE_BYTES, GlyphCache, GlyphKey, MAX_CACHED_SIDE, MaskCache, Raster,
    SHAPE_CACHE_BYTES,
};
use super::image::{DrawError, Image};
use super::list::{DisplayList, SHADOW_REACH_SIGMAS, Segment};
use super::paint::{FillRule, LineCap, LineJoin, Paint, StrokeStyle};
use super::shadow;
use super::snap::{KAPPA, snap_rect as snap, snapped_line};
use crate::{ColorToken, Theme};

/// The `tiny-skia` renderer (ADR-0039 D9, confirmed by ADR-0043). Keep one per window (or per
/// headless job) and reuse it: its buffers and caches are what makes later frames
/// allocation-free in our code.
pub struct CpuRenderer {
    /// The bundled faces, for the outlines of glyphs that are not cached yet (ADR-0044 TC1).
    faces: Faces,
    /// Rasterised glyphs kept between frames (ADR-0044 TX8).
    glyphs: GlyphCache,
    /// Rasterised anti-aliased rounded rectangles kept between frames (F2.09b2, ADR-0043
    /// precision): every control is one, and filling it as a path cost most of a redraw.
    shapes: MaskCache<ShapeKey>,
    /// The identity cover table (coverage `c` blended at `c / 255`), for shapes.
    plain_cover: [f32; 256],
    /// The coverage of a glyph pixel (0–255) turned into the blend's cover (0–1), with the text
    /// contrast of [`TEXT_CONTRAST`] applied (ADR-0044 TX6). Built once, read per pixel.
    text_cover: [f32; 256],
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
    /// The last outermost rectangular clip's mask of the region being drawn, kept for the next
    /// segment with the same clip (F2.09d, precision of ADR-0043): every segment crossing a
    /// scroll area's viewport is clipped to the same rectangle, and building a mask the size of
    /// the frame for each one cost 24 M instructions per wheel notch (10 masks). Keyed by the
    /// snapped device rectangle; dropped at the start of every region, whose pixmap differs.
    clip_cache: Option<(ClipKey, Mask)>,
    /// `Some(key)` while the outermost mask of [`masks`](Self::masks) is a rectangular clip that
    /// goes back to [`clip_cache`](Self::clip_cache) when it is closed, instead of to the pool.
    bottom_clip: Option<ClipKey>,
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

/// The text contrast exponent γ of ADR-0044 TX6: a glyph pixel of coverage `c` is blended with
/// cover `c^(1/γ)`. 1 is plain coverage; above 1 the edges of the strokes are strengthened,
/// which makes light text on a dark surface look less thin (the effect FreeType's and Skia's
/// text gamma aim at).
///
/// Chosen by the owner on 2026-11-12 (ADR-0044 precisions), by eye, among 1 (plain), 1.2 and
/// 1.45 on screenshots of Studio light and dark at scales 1 and 2: 1.45. A pixel half covered
/// by a stroke is blended at 62 % instead of 50 %, which keeps 11–13 px text firm on dark
/// surfaces; fully covered and empty pixels do not change.
const TEXT_CONTRAST: f32 = 1.45;

/// The cover table of [`TEXT_CONTRAST`]: entry `c` is `(c / 255)^(1/γ)`.
fn text_cover_table() -> [f32; 256] {
    let mut table = [0.0; 256];
    for (value, entry) in (0_u16..).zip(table.iter_mut()) {
        *entry = (f32::from(value) / 255.0).powf(1.0 / TEXT_CONTRAST);
    }
    table
}

impl std::fmt::Debug for CpuRenderer {
    /// Only the sizes of the reusable buffers.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CpuRenderer")
            .field("scratch_bytes", &self.scratch.capacity())
            .field("spare_masks", &self.mask_pool.len())
            .field("cached_glyphs", &self.glyphs.len())
            .field("cached_glyph_bytes", &self.glyphs.bytes())
            .finish_non_exhaustive()
    }
}

impl CpuRenderer {
    /// A renderer with empty buffers and caches, drawing glyphs from `faces` (ADR-0044 TC1:
    /// the renderer keeps them, a reference-count increment, to read the outlines of glyphs it
    /// has not cached yet).
    #[must_use]
    pub fn new(faces: Faces) -> Self {
        Self {
            faces,
            glyphs: GlyphCache::new(GLYPH_CACHE_BYTES),
            shapes: MaskCache::new(SHAPE_CACHE_BYTES),
            plain_cover: plain_cover_table(),
            text_cover: text_cover_table(),
            scratch: Vec::new(),
            builder: None,
            mask_pool: Vec::new(),
            transforms: Vec::new(),
            masks: Vec::new(),
            clip_cache: None,
            bottom_clip: None,
            shadows: Vec::new(),
            frame_number: 0,
            surface: (0, 0),
        }
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
        // A cached clip mask belongs to the previous region's pixmap (another size, another
        // origin): it cannot serve this one.
        if let Some((_, mask)) = self.clip_cache.take() {
            self.mask_pool.push(mask.take());
        }
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
        // Clips a segment left open (the recorder closes them, so normally none).
        while !self.masks.is_empty() {
            self.pop_mask();
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
                Command::GlyphRun {
                    face,
                    size,
                    glyphs,
                    paint,
                } => {
                    let glyphs = segment.glyphs.get(glyphs.clone()).unwrap_or(&[]);
                    self.glyph_run(frame, &transform, *face, *size, glyphs, *paint);
                }
                Command::Image { image, dest } => self.image(frame, &transform, *image, dest),
                Command::PushClip { shape } => self.push_clip(frame, &transform, shape),
                Command::PopClip => self.pop_mask(),
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
        if self.cached_rounded(frame, transform, shape, None, paint) {
            return;
        }
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
        if self.cached_rounded(frame, transform, shape, Some(stroke), paint) {
            return;
        }
        let mut builder = self.builder.take().unwrap_or_default();
        push_rounded_rect(&mut builder, shape);
        self.stroke_built(frame, transform, builder, stroke, paint);
    }

    /// Draws a rounded rectangle (filled, or stroked with `stroke`) from the shape cache
    /// (F2.09b2): its coverage mask, rasterised once for its size, radius, stroke and sub-pixel
    /// phase in device pixels, is blended with the paint and the clip, like a glyph. Returns
    /// `false`, drawing nothing, when the shape is too large for the cache (a panel's
    /// background at a high scale), so the caller fills it as a path.
    ///
    /// # Why the mask is a function of the key only
    /// The mask is rasterised from the key's numbers (a rectangle of the key's size at the key's
    /// phase), never from the shape that first asked for it, so two shapes with the same key get
    /// the same pixels whatever was drawn first, and a partial redraw equals a full frame.
    fn cached_rounded(
        &mut self,
        frame: &mut Frame<'_, '_>,
        transform: &Transform,
        shape: &RoundedRect,
        stroke: Option<StrokeStyle>,
        paint: Paint,
    ) -> bool {
        let scale = transform.scale * frame.scale;
        // The rectangle's top-left corner in the frame's device pixels.
        let list = transform.apply(Point::new(shape.rect.x0, shape.rect.y0));
        let (device_x, device_y) = (list.x * frame.scale, list.y * frame.scale);
        let (column, row) = (device_x.floor(), device_y.floor());
        let width = shape.rect.width() * scale;
        let height = shape.rect.height() * scale;
        let limit = f32::from(u16::try_from(MAX_CACHED_SHAPE).unwrap_or(u16::MAX));
        if !(column.is_finite() && row.is_finite()) || width > limit || height > limit {
            return false;
        }
        let key = ShapeKey {
            width: width.to_bits(),
            height: height.to_bits(),
            radius: (shape.clamped_radius() * scale).to_bits(),
            stroke: stroke.map_or(0, |stroke| (stroke.width * scale).to_bits()),
            join: stroke.map_or(0, |stroke| match stroke.join {
                LineJoin::Miter => 1,
                LineJoin::Round => 2,
                LineJoin::Bevel => 3,
            }),
            phase_x: (device_x - column).to_bits(),
            phase_y: (device_y - row).to_bits(),
        };
        let builder = &mut self.builder;
        let raster = self
            .shapes
            .get_or_rasterise(key, self.frame_number, || rasterise_rounded(builder, key));
        match raster {
            Some(Raster::Mask(mask)) => {
                #[allow(
                    clippy::cast_possible_truncation,
                    reason = "device coordinates of a frame, far inside i64"
                )]
                let (x, y) = (
                    column as i64 + i64::from(mask.left) - i64::from(frame.corner.0),
                    row as i64 + i64::from(mask.top) - i64::from(frame.corner.1),
                );
                let source = premultiplied(frame.theme, paint);
                blit_glyph(
                    &mut frame.pixmap,
                    mask,
                    x,
                    y,
                    self.masks.last(),
                    &source,
                    &self.plain_cover,
                );
                true
            }
            Some(Raster::Empty) => true,
            Some(Raster::Large) | None => false,
        }
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

    /// Draws a glyph run (ADR-0044 TX6–TX8): each glyph from its cached coverage mask, placed at
    /// a quarter of a device pixel across and a whole pixel down (TX7), or filled as a path when
    /// it is larger than the cache takes.
    ///
    /// # Why positions are rounded in the frame's coordinates
    /// The quarter-pixel phase is computed from the glyph's position in the whole frame, not in
    /// the pixmap being drawn: a partial region's pixmap starts at a whole device pixel of the
    /// frame, so the phase, the mask and the pixels it gives are the same in a partial redraw as
    /// in a full frame.
    fn glyph_run(
        &mut self,
        frame: &mut Frame<'_, '_>,
        transform: &Transform,
        face: FaceId,
        size: f32,
        glyphs: &[GlyphPlacement],
        paint: Paint,
    ) {
        // The size in device pixels, quantised to 1/64 of a pixel (TX8): the cache's key and the
        // size the outline is drawn at.
        let ppem = size * transform.scale * frame.scale;
        let ppem_64 = (ppem * 64.0).round();
        if !(1.0..16_777_216.0).contains(&ppem_64) {
            // Not finite, not positive, or absurdly large (beyond f32's exact integers).
            return;
        }
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a whole number in 1..2^24, checked just above"
        )]
        let ppem_key = ppem_64 as u32;
        let ppem = ppem_64 / 64.0;
        let source = premultiplied(frame.theme, paint);
        for placement in glyphs {
            // The origin in the frame's device pixels.
            let list = transform.apply(placement.at);
            let device_x = list.x * frame.scale;
            let device_y = list.y * frame.scale;
            let quarters = (device_x * 4.0).round();
            let column = (quarters / 4.0).floor();
            let row = device_y.round();
            if !(column.is_finite() && row.is_finite()) {
                continue;
            }
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "the remainder of a division by 4 of a whole number: 0, 1, 2 or 3"
            )]
            let quarter = (quarters - column * 4.0).clamp(0.0, 3.0) as u8;
            let key = GlyphKey {
                face,
                glyph: placement.glyph,
                ppem_64: ppem_key,
                quarter,
            };
            let faces = &self.faces;
            let builder = &mut self.builder;
            let raster = self.glyphs.get_or_rasterise(key, self.frame_number, || {
                rasterise(faces, builder, key, ppem)
            });
            match raster {
                None | Some(Raster::Empty) => {}
                Some(Raster::Mask(mask)) => {
                    // The mask's top-left corner in the pixmap: the rounded origin, plus the
                    // mask's offset, minus the pixmap's corner in the frame.
                    #[allow(
                        clippy::cast_possible_truncation,
                        reason = "device coordinates of a frame, far inside i64"
                    )]
                    let (x, y) = (
                        column as i64 + i64::from(mask.left) - i64::from(frame.corner.0),
                        row as i64 + i64::from(mask.top) - i64::from(frame.corner.1),
                    );
                    blit_glyph(
                        &mut frame.pixmap,
                        mask,
                        x,
                        y,
                        self.masks.last(),
                        &source,
                        &self.text_cover,
                    );
                }
                Some(Raster::Large) => {
                    // Drawn where it is, unrounded, as any path: `needs_whole_shape` makes a
                    // partial redraw include the whole run, so its pixels match the full frame.
                    let at = Point::new(device_x - frame.origin.x, device_y - frame.origin.y);
                    self.fill_large_glyph(frame, face, placement.glyph, ppem, at, paint);
                }
            }
        }
    }

    /// Fills a glyph too large for the cache as an anti-aliased path, at `at` (pixmap device
    /// pixels, its origin on the baseline).
    fn fill_large_glyph(
        &mut self,
        frame: &mut Frame<'_, '_>,
        face: FaceId,
        glyph: GlyphId,
        ppem: f32,
        at: Point,
        paint: Paint,
    ) {
        let mut sink = SkiaSink {
            builder: self.builder.take().unwrap_or_default(),
            dx: 0.0,
        };
        if self
            .faces
            .draw_outline(face, glyph, ppem, &mut sink)
            .is_err()
        {
            self.builder = Some(sink.builder);
            return;
        }
        let Some(path) = sink.builder.finish() else {
            return;
        };
        let skia_paint = skia_paint(frame.theme, paint, true);
        frame.pixmap.fill_path(
            &path,
            &skia_paint,
            SkiaFillRule::Winding,
            SkiaTransform::from_translate(at.x, at.y),
            self.masks.last(),
        );
        self.builder = Some(path.clear());
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
    ///
    /// An outermost rectangular clip is taken from [`clip_cache`](Self::clip_cache) when the
    /// previous one of this region was the same rectangle (F2.09d): no mask is built at all.
    fn push_clip(&mut self, frame: &mut Frame<'_, '_>, transform: &Transform, shape: &RoundedRect) {
        let width = frame.pixmap.width();
        let height = frame.pixmap.height();
        let Some(size) = IntSize::from_wh(width, height) else {
            return;
        };
        // The key of an outermost rectangular clip, if this is one (and not empty).
        let key = if self.masks.is_empty() && shape.clamped_radius() <= 0.0 {
            snap_rect(&frame.to_device_rect(transform, &shape.rect)).map(|r| ClipKey::of(&r))
        } else {
            None
        };
        if self.masks.is_empty() {
            // Set (or cleared) at every outermost clip, so a stale key can never be attached to
            // another clip's mask: only a rectangular one is kept when it is closed.
            self.bottom_clip = key;
        }
        if let Some(key) = key
            && let Some((cached, mask)) = self.clip_cache.take()
        {
            if cached == key {
                self.masks.push(mask);
                return;
            }
            // A different rectangle: the cached one stays; this one is built below and
            // replaces it when it is closed.
            self.clip_cache = Some((cached, mask));
        }
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

    /// Closes the innermost clip: its mask goes back to the pool, or, when it is the outermost
    /// rectangular clip, into [`clip_cache`](Self::clip_cache) for the next segment (the mask it
    /// replaces there goes to the pool).
    fn pop_mask(&mut self) {
        let Some(mask) = self.masks.pop() else {
            return;
        };
        if self.masks.is_empty()
            && let Some(key) = self.bottom_clip.take()
        {
            if let Some((_, old)) = self.clip_cache.replace((key, mask)) {
                self.mask_pool.push(old.take());
            }
        } else {
            self.mask_pool.push(mask.take());
        }
    }
}

/// The identity of a rectangular clip in device pixels: its snapped edges, compared exactly (the
/// bits of the `f32`s, which snapping makes whole numbers).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ClipKey([u32; 4]);

impl ClipKey {
    /// The key of the snapped rectangle `rect`.
    fn of(rect: &SkiaRect) -> Self {
        Self([
            rect.left().to_bits(),
            rect.top().to_bits(),
            rect.right().to_bits(),
            rect.bottom().to_bits(),
        ])
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
        | Command::PushClip { .. }
        // Cached glyphs are blended from masks placed on whole pixels of the frame, the same in
        // any area; but a glyph too large for the cache is filled as a path, and whether it is
        // depends on the scale, unknown here. Conservative, and cheap: text runs are small.
        | Command::GlyphRun { .. } => true,
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

/// Receives a glyph's outline from `orchestra-text` into a `tiny-skia` path builder, shifted by
/// `dx` device pixels (the quarter-pixel phase of TX7).
struct SkiaSink {
    /// The path being built.
    builder: SkiaPathBuilder,
    /// Horizontal shift, device pixels.
    dx: f32,
}

impl OutlineSink for SkiaSink {
    fn move_to(&mut self, x: f32, y: f32) {
        self.builder.move_to(x + self.dx, y);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.builder.line_to(x + self.dx, y);
    }
    fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
        self.builder.quad_to(cx + self.dx, cy, x + self.dx, y);
    }
    fn cubic_to(&mut self, c1x: f32, c1y: f32, c2x: f32, c2y: f32, x: f32, y: f32) {
        self.builder
            .cubic_to(c1x + self.dx, c1y, c2x + self.dx, c2y, x + self.dx, y);
    }
    fn close(&mut self) {
        self.builder.close();
    }
}

/// Rasterises the glyph of `key` at `ppem` device pixels per em into a coverage mask (ADR-0044
/// TC6): its outline, shifted by its quarter-pixel phase, filled with anti-aliasing (non-zero
/// rule, as fonts require) into a mask just large enough for it. `builder` lends its buffers and
/// gets them back.
///
/// # Cost
/// The outline (≈ 13 000 instructions for a letter, `benches/text.rs` of `orchestra-text`) plus
/// the fill, once per glyph, size and phase; it allocates the mask.
fn rasterise(
    faces: &Faces,
    builder: &mut Option<SkiaPathBuilder>,
    key: GlyphKey,
    ppem: f32,
) -> Raster {
    let mut sink = SkiaSink {
        builder: builder.take().unwrap_or_default(),
        dx: f32::from(key.quarter) / 4.0,
    };
    if !matches!(
        faces.draw_outline(key.face, key.glyph, ppem, &mut sink),
        Ok(true)
    ) {
        *builder = Some(sink.builder);
        return Raster::Empty;
    }
    // An outline with no segment (a space) gives no path: no ink.
    let Some(path) = sink.builder.finish() else {
        return Raster::Empty;
    };
    let bounds = path.bounds();
    // Whole pixels enclosing the path: anti-aliased coverage never leaves the path's bounds.
    let (left, top) = (bounds.left().floor(), bounds.top().floor());
    let (right, bottom) = (bounds.right().ceil(), bounds.bottom().ceil());
    let (width, height) = (right - left, bottom - top);
    let limit = f32::from(u16::try_from(MAX_CACHED_SIDE).unwrap_or(u16::MAX));
    let raster = if width > limit || height > limit {
        Raster::Large
    } else {
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "whole numbers between 0 and MAX_CACHED_SIDE, and a glyph's offsets,                       small whole numbers"
        )]
        let (left_px, top_px, width_px, height_px) =
            (left as i32, top as i32, width as u32, height as u32);
        match Mask::new(width_px, height_px) {
            Some(mut mask) => {
                mask.fill_path(
                    &path,
                    SkiaFillRule::Winding,
                    true,
                    SkiaTransform::from_translate(-left, -top),
                );
                Raster::Mask(CoverageMask {
                    left: left_px,
                    top: top_px,
                    width: width_px,
                    height: height_px,
                    coverage: mask.take(),
                })
            }
            // A zero-sized mask: a degenerate outline with no area.
            None => Raster::Empty,
        }
    };
    *builder = Some(path.clear());
    raster
}

/// The premultiplied colour of a paint, channels 0–255, for our own blending (glyphs, shadows).
fn premultiplied(theme: &Theme, paint: Paint) -> [f32; 4] {
    let color = theme.color(paint.token);
    let alpha = f32::from(color.a) * f32::from(paint.opacity) / (255.0 * 255.0);
    [
        f32::from(color.r) * alpha,
        f32::from(color.g) * alpha,
        f32::from(color.b) * alpha,
        255.0 * alpha,
    ]
}

/// Blends a glyph's coverage `mask`, its top-left corner at pixmap pixel `(x, y)` (possibly
/// outside the pixmap), with `source` over the pixmap, scaled by the clip mask in effect, through
/// the text cover table (TX6). One rounding per channel, as for shadows ([`blend_over`]).
///
/// # Complexity
/// O(pixels of the mask inside the pixmap), no allocation.
fn blit_glyph(
    pixmap: &mut PixmapMut<'_>,
    mask: &CoverageMask,
    x: i64,
    y: i64,
    clip: Option<&Mask>,
    source: &[f32; 4],
    cover: &[f32; 256],
) {
    let (width, height) = (i64::from(pixmap.width()), i64::from(pixmap.height()));
    let (mask_width, mask_height) = (i64::from(mask.width), i64::from(mask.height));
    // The part of the mask inside the pixmap, in pixmap columns and rows.
    let (col0, col1) = (x.max(0), (x + mask_width).min(width));
    let (row0, row1) = (y.max(0), (y + mask_height).min(height));
    if col0 >= col1 || row0 >= row1 {
        return;
    }
    let clip = clip.map(Mask::data);
    // An opaque source at full coverage and no clip gives exactly the source's bytes:
    // `blend_over` keeps nothing of the old pixel and rounds whole numbers (the colour channels
    // times an alpha of exactly 1). The fully covered pixels are the insides of the strokes,
    // often a third of a glyph's pixels, so copying them skips most of the arithmetic, with the
    // same bytes (the pixel tests compare cached and fresh drawing byte for byte).
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "whole numbers in 0..=255 (a byte times an alpha of exactly 1)"
    )]
    let solid = (source[3] >= 255.0 && cover[255] >= 1.0)
        .then(|| source.map(|channel| (channel + 0.5) as u8));
    let data = pixmap.data_mut();
    for row in row0..row1 {
        // Offsets: the mask's row and the pixmap's, both as indices of whole pixels. Every value
        // is non-negative and inside its buffer by the clamping above.
        let mask_row = usize::try_from((row - y) * mask_width + (col0 - x)).unwrap_or(0);
        let pixel_row = usize::try_from(row * width + col0).unwrap_or(0);
        let count = usize::try_from(col1 - col0).unwrap_or(0);
        let Some(coverage) = mask.coverage.get(mask_row..mask_row + count) else {
            continue;
        };
        let Some(pixels) = data.get_mut(pixel_row * 4..(pixel_row + count) * 4) else {
            continue;
        };
        let clip_row = clip.and_then(|clip| clip.get(pixel_row..pixel_row + count));
        for (index, (pixel, &value)) in pixels
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .zip(coverage)
            .enumerate()
        {
            if value == 0 {
                continue;
            }
            if value == 255
                && clip_row.is_none()
                && let Some(solid) = solid
            {
                pixel.copy_from_slice(&solid);
                continue;
            }
            let mut amount = cover.get(usize::from(value)).copied().unwrap_or(1.0);
            if let Some(clip_row) = clip_row {
                amount *= f32::from(clip_row.get(index).copied().unwrap_or(0)) / 255.0;
            }
            if amount > 0.0 {
                blend_over(pixel, source, amount);
            }
        }
    }
}

/// The largest side of a cached shape, device pixels (F2.09b2): controls and small panels fit;
/// a large panel's background is filled as a path, rarely redrawn anyway.
const MAX_CACHED_SHAPE: u32 = 512;

/// What identifies a cached rounded rectangle (F2.09b2): its size, radius and stroke width in
/// device pixels, its stroke join, and its sub-pixel phase, all as exact `f32` bits (a stroke
/// width of 0 means filled).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct ShapeKey {
    /// Width, device pixels.
    width: u32,
    /// Height, device pixels.
    height: u32,
    /// Corner radius, device pixels (already clamped).
    radius: u32,
    /// Stroke width, device pixels; 0 for a fill.
    stroke: u32,
    /// The stroke's join (0 for a fill).
    join: u8,
    /// The fractional part of the top-left corner's x, device pixels.
    phase_x: u32,
    /// The fractional part of its y.
    phase_y: u32,
}

/// Rasterises the rounded rectangle of `key` into a coverage mask, from the key's numbers only
/// (see [`CpuRenderer::cached_rounded`]): the rectangle placed at its phase plus a margin wide
/// enough for the stroke and the anti-aliasing fringe, filled (non-zero) or stroked (the stroke
/// turned into a fillable outline by `tiny-skia`).
fn rasterise_rounded(builder: &mut Option<SkiaPathBuilder>, key: ShapeKey) -> Raster {
    let width = f32::from_bits(key.width);
    let height = f32::from_bits(key.height);
    let stroke = f32::from_bits(key.stroke);
    // Whole pixels of margin around the shape: half the stroke, plus one for the fringe.
    let margin = (stroke / 2.0).ceil() + 1.0;
    let x0 = margin + f32::from_bits(key.phase_x);
    let y0 = margin + f32::from_bits(key.phase_y);
    let shape = RoundedRect::new(
        Rect::new(x0, y0, x0 + width, y0 + height),
        f32::from_bits(key.radius),
    );
    let mut path_builder = builder.take().unwrap_or_default();
    push_rounded_rect(&mut path_builder, &shape);
    let Some(outline) = path_builder.finish() else {
        return Raster::Empty;
    };
    let filled = if key.stroke == 0 {
        Some(outline.clone())
    } else {
        let join = match key.join {
            2 => SkiaLineJoin::Round,
            3 => SkiaLineJoin::Bevel,
            _ => SkiaLineJoin::Miter,
        };
        outline.stroke(
            &Stroke {
                width: stroke,
                miter_limit: StrokeStyle::MITER_LIMIT,
                line_cap: SkiaLineCap::Butt,
                line_join: join,
                dash: None,
            },
            1.0,
        )
    };
    *builder = Some(outline.clear());
    let Some(filled) = filled else {
        return Raster::Empty;
    };
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "whole numbers below MAX_CACHED_SHAPE plus a small margin"
    )]
    let (mask_width, mask_height, margin_px) = (
        (x0 + width + margin).ceil() as u32,
        (y0 + height + margin).ceil() as u32,
        margin as i32,
    );
    let Some(mut mask) = Mask::new(mask_width, mask_height) else {
        return Raster::Empty;
    };
    mask.fill_path(
        &filled,
        SkiaFillRule::Winding,
        true,
        SkiaTransform::identity(),
    );
    Raster::Mask(CoverageMask {
        left: -margin_px,
        top: -margin_px,
        width: mask_width,
        height: mask_height,
        coverage: mask.take(),
    })
}

/// The identity cover table: coverage `c` blended at `c / 255`.
fn plain_cover_table() -> [f32; 256] {
    let mut table = [0.0; 256];
    for (value, entry) in (0_u16..).zip(table.iter_mut()) {
        *entry = f32::from(value) / 255.0;
    }
    table
}

#[cfg(test)]
mod tests {
    //! Tests that need the crate's private shadow formula and glyph blending; the pixel tests of
    //! the backend are in `tests/draw.rs` and `tests/text.rs`.

    #![allow(
        clippy::unwrap_used,
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "test code: a failed unwrap fails the test; pixel counters and sizes are small \
                  and positive"
    )]

    /// The text contrast table (ADR-0044 TX6): empty and full coverage are unchanged, partial
    /// coverage is strengthened as `c^(1/1.45)`, and the table grows with the coverage.
    #[test]
    fn the_text_contrast_table_strengthens_partial_coverage() {
        let table = super::text_cover_table();
        assert_eq!(table[0], 0.0);
        assert_eq!(table[255], 1.0);
        let half = (128.0_f32 / 255.0).powf(1.0 / 1.45);
        assert!((table[128] - half).abs() < 1e-6);
        assert!(table[128] > 128.0 / 255.0 + 0.1, "{}", table[128]);
        assert!(table.windows(2).all(|pair| pair[0] < pair[1]));
    }

    /// Blending a glyph mask goes through the contrast table: a pixel of coverage 128 is
    /// blended at the table's value, not at 128/255.
    #[test]
    fn glyph_pixels_are_blended_through_the_contrast_table() {
        let mut bytes = vec![255_u8; 4];
        let mut pixmap = super::PixmapMut::from_bytes(&mut bytes, 1, 1).unwrap();
        let mask = super::CoverageMask {
            left: 0,
            top: 0,
            width: 1,
            height: 1,
            coverage: vec![128],
        };
        let table = super::text_cover_table();
        // Opaque black text over a white pixel.
        let source = [0.0, 0.0, 0.0, 255.0];
        super::blit_glyph(&mut pixmap, &mask, 0, 0, None, &source, &table);
        let mut expected = [255_u8; 4];
        super::blend_over(&mut expected, &source, table[128]);
        assert_eq!(bytes, expected);
        // 128/255 would have left the pixel at 127; the table darkens it further.
        assert!(bytes[0] < 100, "{bytes:?}");
    }

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
                let mut renderer = CpuRenderer::new(orchestra_text::Faces::bundled().unwrap());
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
            let mut renderer = CpuRenderer::new(orchestra_text::Faces::bundled().unwrap());
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
