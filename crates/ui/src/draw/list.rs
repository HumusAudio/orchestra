// SPDX-License-Identifier: GPL-3.0-or-later

//! The display list: segments of recorded commands, the image store and the damage they cause
//! (ADR-0039 D1, D4, D5).

use std::collections::HashMap;

use super::command::{Command, Entry, PathElement};
use super::damage::Damage;
use super::geometry::{Point, Rect, RoundedRect, Transform};
use super::image::{DrawError, Image, ImageId};
use super::paint::{FillRule, Paint, StrokeStyle};

/// How far a shadow reaches beyond its shape, in standard deviations: a Gaussian has less than
/// 0.15 % of its weight beyond 3σ, invisible in 8-bit colour (ADR-0039 D6).
pub(crate) const SHADOW_REACH_SIGMAS: f32 = 3.0;

/// The caller's name for a segment (ADR-0039 D1).
///
/// Any `u64` the caller likes, as long as it is unique within one list: a widget's id, a hash of
/// "staff 3, system 2", a counter. Opaque to the list, which only compares them; that keeps the
/// choice between retained and immediate widgets (F2.08) open.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SegmentId(pub u64);

/// One segment: the commands one piece of the interface recorded, and their path arena.
#[derive(Debug, Default)]
pub(crate) struct Segment {
    /// The caller's name for it.
    pub(crate) id: Option<SegmentId>,
    /// The commands, in drawing order, each with its bounds.
    pub(crate) entries: Vec<Entry>,
    /// The path elements of every path command of the segment.
    pub(crate) elements: Vec<PathElement>,
    /// The union of the entries' bounds: what the segment can touch.
    pub(crate) bounds: Rect,
}

/// The display list: what to draw, as ordered segments of themed commands (ADR-0039).
///
/// # Usage
/// ```
/// use orchestra_ui::{ColorToken, DisplayList, Rect, SegmentId};
///
/// let mut list = DisplayList::new();
/// list.record(SegmentId(1), |r| {
///     r.fill_rect(Rect::new(0.0, 0.0, 100.0, 30.0), ColorToken::SurfaceRaised);
/// });
/// // Later, only this segment changes: only its old and new area is damaged.
/// list.record(SegmentId(1), |r| {
///     r.fill_rect(Rect::new(0.0, 0.0, 120.0, 30.0), ColorToken::SurfaceRaised);
/// });
/// let damage = list.take_damage();
/// assert_eq!(damage.rects(), &[Rect::new(0.0, 0.0, 120.0, 30.0)]);
/// ```
///
/// # Order
/// Segments are drawn in the order they were first recorded; re-recording keeps a segment's
/// place. Later segments are drawn on top.
#[derive(Debug, Default)]
pub struct DisplayList {
    /// The segments in drawing order.
    segments: Vec<Segment>,
    /// From a segment's id to its index in `segments`.
    index: HashMap<SegmentId, usize>,
    /// The pictures [`Recorder::image`] can draw; `None` where one was removed.
    images: Vec<Option<Image>>,
    /// What changed since the last [`take_damage`](Self::take_damage).
    damage: Damage,
    /// Scratch stacks for the recorder, kept so re-recording does not allocate.
    scratch: RecorderStacks,
}

/// The transform and clip stacks a [`Recorder`] needs, owned by the list and reused.
#[derive(Debug, Default)]
struct RecorderStacks {
    /// Transforms in effect, innermost last (list coordinates ← local coordinates).
    transforms: Vec<Transform>,
    /// Clips in effect, in list coordinates, innermost last (each already intersected with the
    /// previous one).
    clips: Vec<Rect>,
}

impl DisplayList {
    /// An empty list, with no damage.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Records (or re-records) the segment `id`: `draw` receives a [`Recorder`] and calls its
    /// methods. The segment's old area and its new area are added to the damage.
    ///
    /// A new id is appended on top of the existing segments; an existing one keeps its place, and
    /// its previous commands are replaced. The vectors keep their capacity, so after the first
    /// frame re-recording a segment of the same size does not allocate (ADR-0039 D1).
    ///
    /// # Complexity
    /// O(commands recorded) plus one hash lookup.
    pub fn record<F>(&mut self, id: SegmentId, draw: F)
    where
        F: FnOnce(&mut Recorder<'_>),
    {
        let index = if let Some(&index) = self.index.get(&id) {
            index
        } else {
            let index = self.segments.len();
            self.segments.push(Segment {
                id: Some(id),
                ..Segment::default()
            });
            self.index.insert(id, index);
            index
        };
        let Some(segment) = self.segments.get_mut(index) else {
            return;
        };
        let old_bounds = segment.bounds;
        segment.entries.clear();
        segment.elements.clear();
        self.scratch.transforms.clear();
        self.scratch.clips.clear();
        let mut recorder = Recorder {
            entries: &mut segment.entries,
            elements: &mut segment.elements,
            stacks: &mut self.scratch,
            bounds: Rect::EMPTY,
        };
        draw(&mut recorder);
        recorder.finish();
        let new_bounds = recorder.bounds;
        segment.bounds = new_bounds;
        self.damage.add(old_bounds);
        self.damage.add(new_bounds);
    }

    /// Removes the segment `id`, damaging its area. Unknown ids are ignored.
    ///
    /// # Complexity
    /// O(segments): later segments move down one place and their indices are updated.
    pub fn remove(&mut self, id: SegmentId) {
        let Some(index) = self.index.remove(&id) else {
            return;
        };
        if index >= self.segments.len() {
            return;
        }
        let segment = self.segments.remove(index);
        self.damage.add(segment.bounds);
        for later in self.segments.iter().skip(index) {
            if let Some(later_id) = later.id
                && let Some(slot) = self.index.get_mut(&later_id)
            {
                *slot = slot.saturating_sub(1);
            }
        }
    }

    /// Stores a picture for [`Recorder::image`] and returns its id.
    ///
    /// # Errors
    /// [`DrawError::TooManyImages`] when the list already numbers 2³² − 1 pictures.
    pub fn add_image(&mut self, image: Image) -> Result<ImageId, DrawError> {
        let id = u32::try_from(self.images.len()).map_err(|_| DrawError::TooManyImages)?;
        if id == u32::MAX {
            return Err(DrawError::TooManyImages);
        }
        self.images.push(Some(image));
        Ok(ImageId(id))
    }

    /// Frees a picture. Commands that still refer to it draw nothing; the caller re-records the
    /// segments that showed it (which damages their area). The id is not reused.
    pub fn remove_image(&mut self, id: ImageId) {
        if let Some(slot) = self.images.get_mut(id.0 as usize) {
            *slot = None;
        }
    }

    /// Marks everything as damaged: after a theme change (ADR-0039 D3: no re-recording needed), a
    /// resize or a scale change.
    pub fn damage_all(&mut self) {
        self.damage.add_all();
    }

    /// The damage collected since the last call, which starts again from nothing.
    pub fn take_damage(&mut self) -> Damage {
        std::mem::take(&mut self.damage)
    }

    /// The damage collected so far, without resetting it.
    #[must_use]
    pub fn damage(&self) -> &Damage {
        &self.damage
    }

    /// The union of every segment's bounds: the area the list draws on.
    #[must_use]
    pub fn bounds(&self) -> Rect {
        self.segments
            .iter()
            .fold(Rect::EMPTY, |acc, segment| acc.union(&segment.bounds))
    }

    /// How many segments the list holds.
    #[must_use]
    pub fn segment_count(&self) -> usize {
        self.segments.len()
    }

    /// The segments in drawing order, for the backends.
    pub(crate) fn segments(&self) -> &[Segment] {
        &self.segments
    }

    /// A stored picture, if it still exists.
    pub(crate) fn image(&self, id: ImageId) -> Option<&Image> {
        self.images.get(id.0 as usize).and_then(Option::as_ref)
    }
}

/// Records the commands of one segment: given by [`DisplayList::record`].
///
/// Every drawing method computes the command's bounds (with stroke reach, shadow reach, the
/// transforms and clips in effect) so the backend can skip it when it is outside the damage.
/// A shape that is invisible (empty, or entirely clipped away) is not recorded at all.
///
/// Transforms and clips are pushed and popped. An unmatched pop is ignored, and pushes still open
/// when the recording ends are closed automatically, so a segment can never leave its state to
/// the next one.
#[derive(Debug)]
pub struct Recorder<'a> {
    /// Where the commands go.
    entries: &'a mut Vec<Entry>,
    /// Where the path elements go.
    elements: &'a mut Vec<PathElement>,
    /// The transform and clip stacks.
    stacks: &'a mut RecorderStacks,
    /// The union of the bounds recorded so far.
    bounds: Rect,
}

impl Recorder<'_> {
    /// Fills an axis-aligned rectangle (snapped to device pixels, ADR-0039 D7).
    pub fn fill_rect(&mut self, rect: Rect, paint: impl Into<Paint>) {
        self.draw(
            rect,
            Command::FillRect {
                rect,
                paint: paint.into(),
            },
        );
    }

    /// Fills a rounded rectangle.
    pub fn fill_rounded_rect(&mut self, shape: RoundedRect, paint: impl Into<Paint>) {
        self.draw(
            shape.rect,
            Command::FillRoundedRect {
                shape,
                paint: paint.into(),
            },
        );
    }

    /// Strokes the outline of a rectangle, centred on its edges (snapped).
    pub fn stroke_rect(&mut self, rect: Rect, stroke: StrokeStyle, paint: impl Into<Paint>) {
        if !stroke_is_visible(stroke) {
            return;
        }
        self.draw(
            rect.inflate(stroke.reach()),
            Command::StrokeRect {
                rect,
                stroke,
                paint: paint.into(),
            },
        );
    }

    /// Strokes the outline of a rounded rectangle.
    pub fn stroke_rounded_rect(
        &mut self,
        shape: RoundedRect,
        stroke: StrokeStyle,
        paint: impl Into<Paint>,
    ) {
        if !stroke_is_visible(stroke) {
            return;
        }
        self.draw(
            shape.rect.inflate(stroke.reach()),
            Command::StrokeRoundedRect {
                shape,
                stroke,
                paint: paint.into(),
            },
        );
    }

    /// Draws a straight line (snapped when horizontal or vertical, ADR-0039 D7).
    pub fn line(&mut self, from: Point, to: Point, stroke: StrokeStyle, paint: impl Into<Paint>) {
        if !stroke_is_visible(stroke) {
            return;
        }
        // The hull of the two points, grown by the reach: a degenerate (zero-area) hull of a
        // horizontal line becomes a band as thick as the line.
        let hull = Rect::new(
            from.x.min(to.x),
            from.y.min(to.y),
            from.x.max(to.x),
            from.y.max(to.y),
        );
        self.draw(
            hull.inflate(stroke.reach()),
            Command::Line {
                from,
                to,
                stroke,
                paint: paint.into(),
            },
        );
    }

    /// Fills the path that `build` describes.
    pub fn fill_path<F>(&mut self, rule: FillRule, paint: impl Into<Paint>, build: F)
    where
        F: FnOnce(&mut PathBuilder<'_>),
    {
        let paint = paint.into();
        let (elements, hull) = self.build_path(build);
        self.draw(
            hull,
            Command::FillPath {
                elements,
                rule,
                paint,
            },
        );
    }

    /// Strokes the path that `build` describes.
    pub fn stroke_path<F>(&mut self, stroke: StrokeStyle, paint: impl Into<Paint>, build: F)
    where
        F: FnOnce(&mut PathBuilder<'_>),
    {
        if !stroke_is_visible(stroke) {
            return;
        }
        let paint = paint.into();
        let (elements, hull) = self.build_path(build);
        self.draw(
            hull.inflate(stroke.reach()),
            Command::StrokePath {
                elements,
                stroke,
                paint,
            },
        );
    }

    /// Draws a shadow: `shape` blurred with a Gaussian of standard deviation `blur` (logical
    /// pixels), ADR-0039 D6. A negative or non-finite blur counts as 0 (a sharp fill).
    ///
    /// The shadow is drawn where the shape is: offsetting it (ADR-0037's `shadow-offset`) is the
    /// caller's choice of `shape`.
    pub fn shadow(&mut self, shape: RoundedRect, blur: f32, paint: impl Into<Paint>) {
        let blur = if blur.is_finite() { blur.max(0.0) } else { 0.0 };
        self.draw(
            shape.rect.inflate(blur * SHADOW_REACH_SIGMAS),
            Command::Shadow {
                shape,
                blur,
                paint: paint.into(),
            },
        );
    }

    /// Draws a stored picture stretched into `dest`.
    pub fn image(&mut self, image: ImageId, dest: Rect) {
        self.draw(dest, Command::Image { image, dest });
    }

    /// Starts clipping to a rectangle, until the matching [`pop_clip`](Self::pop_clip).
    pub fn push_clip_rect(&mut self, rect: Rect) {
        self.push_clip(RoundedRect::new(rect, 0.0));
    }

    /// Starts clipping to a rounded rectangle, until the matching [`pop_clip`](Self::pop_clip).
    ///
    /// For the bounds of later commands the clip counts as its outer rectangle (conservative:
    /// the rounded corners are not subtracted).
    pub fn push_clip(&mut self, shape: RoundedRect) {
        let in_list = self.transform().apply_rect(&shape.rect);
        let clip = match self.stacks.clips.last() {
            Some(outer) => outer.intersect(&in_list),
            None => in_list,
        };
        self.stacks.clips.push(clip);
        self.entries.push(Entry {
            command: Command::PushClip { shape },
            bounds: Rect::EMPTY,
        });
    }

    /// Ends the innermost clip; ignored when no clip is open.
    pub fn pop_clip(&mut self) {
        if self.stacks.clips.pop().is_some() {
            self.entries.push(Entry {
                command: Command::PopClip,
                bounds: Rect::EMPTY,
            });
        }
    }

    /// Starts a transform, applied after (inside) the one in effect, until the matching
    /// [`pop_transform`](Self::pop_transform). An invalid transform (scale not positive and
    /// finite) is replaced by the identity, so pushes and pops still match.
    pub fn push_transform(&mut self, transform: Transform) {
        let transform = if transform.is_valid() {
            transform
        } else {
            Transform::IDENTITY
        };
        let combined = self.transform().then(&transform);
        self.stacks.transforms.push(combined);
        self.entries.push(Entry {
            command: Command::PushTransform(transform),
            bounds: Rect::EMPTY,
        });
    }

    /// Ends the innermost transform; ignored when none is open.
    pub fn pop_transform(&mut self) {
        if self.stacks.transforms.pop().is_some() {
            self.entries.push(Entry {
                command: Command::PopTransform,
                bounds: Rect::EMPTY,
            });
        }
    }

    /// The transform in effect, from local to list coordinates.
    fn transform(&self) -> Transform {
        self.stacks
            .transforms
            .last()
            .copied()
            .unwrap_or(Transform::IDENTITY)
    }

    /// Records a drawing command whose local bounds are `local`: transforms and clips them, and
    /// drops the command when nothing of it can be seen.
    fn draw(&mut self, local: Rect, command: Command) {
        let mut bounds = self.transform().apply_rect(&local);
        if let Some(clip) = self.stacks.clips.last() {
            bounds = bounds.intersect(clip);
        }
        if bounds.is_empty() {
            // Invisible: empty, inverted, NaN, or clipped away. Its path elements (if any) stay
            // in the arena until the next recording: harmless, and cheaper than truncating.
            return;
        }
        self.bounds = self.bounds.union(&bounds);
        self.entries.push(Entry { command, bounds });
    }

    /// Runs `build` on a [`PathBuilder`] writing into the arena; returns the elements' range and
    /// the hull of their points (local coordinates).
    fn build_path<F>(&mut self, build: F) -> (std::ops::Range<usize>, Rect)
    where
        F: FnOnce(&mut PathBuilder<'_>),
    {
        let start = self.elements.len();
        let mut builder = PathBuilder {
            elements: self.elements,
            hull: None,
        };
        build(&mut builder);
        let hull = builder.hull.unwrap_or(Rect::EMPTY);
        (start..self.elements.len(), hull)
    }

    /// Closes the clips and transforms left open, so the segment's state cannot leak.
    fn finish(&mut self) {
        while !self.stacks.clips.is_empty() {
            self.pop_clip();
        }
        while !self.stacks.transforms.is_empty() {
            self.pop_transform();
        }
    }
}

/// `true` when a stroke would draw something: a positive, finite width.
fn stroke_is_visible(stroke: StrokeStyle) -> bool {
    stroke.width > 0.0 && stroke.width.is_finite()
}

/// Describes a path for [`Recorder::fill_path`] and [`Recorder::stroke_path`].
///
/// Coordinates are local (the recorder's transform applies). A path that never moves draws
/// nothing; a `line_to` before any `move_to` starts at the origin, as in SVG.
#[derive(Debug)]
pub struct PathBuilder<'a> {
    /// The segment's path arena.
    elements: &'a mut Vec<PathElement>,
    /// The hull of every point given so far (control points included, which bounds Béziers).
    hull: Option<Rect>,
}

impl PathBuilder<'_> {
    /// Starts a new sub-path at `(x, y)`.
    pub fn move_to(&mut self, x: f32, y: f32) {
        self.include(x, y);
        self.elements.push(PathElement::MoveTo(Point::new(x, y)));
    }

    /// A straight line to `(x, y)`.
    pub fn line_to(&mut self, x: f32, y: f32) {
        self.include(x, y);
        self.elements.push(PathElement::LineTo(Point::new(x, y)));
    }

    /// A quadratic Bézier with control point `(x1, y1)`, ending at `(x, y)`.
    pub fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.include(x1, y1);
        self.include(x, y);
        self.elements
            .push(PathElement::QuadTo(Point::new(x1, y1), Point::new(x, y)));
    }

    /// A cubic Bézier with control points `(x1, y1)` and `(x2, y2)`, ending at `(x, y)`.
    #[allow(
        clippy::too_many_arguments,
        reason = "the six coordinates of a cubic, in the order every path API uses"
    )]
    pub fn cubic_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.include(x1, y1);
        self.include(x2, y2);
        self.include(x, y);
        self.elements.push(PathElement::CubicTo(
            Point::new(x1, y1),
            Point::new(x2, y2),
            Point::new(x, y),
        ));
    }

    /// Closes the current sub-path.
    pub fn close(&mut self) {
        self.elements.push(PathElement::Close);
    }

    /// Grows the hull to include `(x, y)`.
    ///
    /// The hull of a single point is a zero-area rectangle, which [`Rect::union`] would treat as
    /// empty; so the first point sets it directly and later points widen it edge by edge.
    fn include(&mut self, x: f32, y: f32) {
        self.hull = Some(match self.hull {
            None => Rect::new(x, y, x, y),
            Some(hull) => Rect::new(
                hull.x0.min(x),
                hull.y0.min(y),
                hull.x1.max(x),
                hull.y1.max(y),
            ),
        });
    }
}
