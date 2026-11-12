// SPDX-License-Identifier: GPL-3.0-or-later

//! Layout (ADR-0045 WA2, choice WB4): sizes, constraints, the size rules a parent applies to
//! its children, and the four containers: [`Flex`] (a row or a column), [`Stack`] and [`Grid`].
//!
//! # Two passes
//! 1. **Measure**, bottom up: a widget receives [`Constraints`] (a minimum and a maximum size)
//!    and answers the [`Size`] it wants, measuring its children as it needs.
//! 2. **Arrange**, top down: a widget receives its rectangle and places its children.
//!
//! A parent turns each child's [`LayoutProps`] (its width and height rules, [`Length`]) into the
//! constraints it measures the child with: `Fixed` gives a tight size, `Content` a loose one up
//! to the space available, `Fill` a share of what is left once the others are measured.
//!
//! # Why our own layout
//! ADR-0045 WA2: the spike measured `taffy` at 1.6 M instructions for a 198-widget panel against
//! a few thousand for a column of our own, and the interface's panels, inspectors and dialogs
//! need rows, columns, overlays and grids, not CSS. Every container here is linear in its
//! children.

use crate::draw::geometry::Rect;
use crate::theme::density::Spacing;

use super::tree::{LayoutCx, Widget};

/// A width and a height, logical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Size {
    /// Width.
    pub width: f32,
    /// Height.
    pub height: f32,
}

impl Size {
    /// A size.
    #[must_use]
    pub const fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }

    /// No size.
    pub const ZERO: Self = Self::new(0.0, 0.0);
}

/// The sizes a widget may take: from `min` to `max` in each direction. `max` may be infinite
/// (an unbounded direction, such as the height inside a scroll area).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Constraints {
    /// The smallest size allowed.
    pub min: Size,
    /// The largest size allowed (components may be `f32::INFINITY`).
    pub max: Size,
}

impl Constraints {
    /// Exactly this size.
    #[must_use]
    pub const fn tight(size: Size) -> Self {
        Self {
            min: size,
            max: size,
        }
    }

    /// Anything from nothing up to `max`.
    #[must_use]
    pub const fn loose(max: Size) -> Self {
        Self {
            min: Size::ZERO,
            max,
        }
    }

    /// `size` brought inside the constraints (a widget's answer must respect them). A NaN
    /// component becomes the minimum.
    #[must_use]
    pub fn constrain(&self, size: Size) -> Size {
        let clamp = |value: f32, min: f32, max: f32| {
            if value.is_nan() {
                min
            } else {
                value.max(min).min(max.max(min))
            }
        };
        Size::new(
            clamp(size.width, self.min.width, self.max.width),
            clamp(size.height, self.min.height, self.max.height),
        )
    }
}

/// How a parent sizes a child in one direction (WB4).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Length {
    /// Exactly this many logical pixels.
    Fixed(f32),
    /// As much as the child's content asks (its measured size).
    Content,
    /// A share of the space left once the fixed and content children are measured, in
    /// proportion to this weight (two `Fill(1.0)` children split it evenly). In a direction the
    /// parent cannot fill (unbounded), it behaves as `Content`.
    Fill(f32),
}

/// Where a child sits in the space its parent gives it, when it is smaller than that space.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Align {
    /// At the start (left, or top).
    #[default]
    Start,
    /// Centred.
    Center,
    /// At the end (right, or bottom).
    End,
}

impl Align {
    /// The offset of a span of `size` inside `space` (both in one direction).
    fn offset(self, space: f32, size: f32) -> f32 {
        let free = (space - size).max(0.0);
        match self {
            Self::Start => 0.0,
            Self::Center => free / 2.0,
            Self::End => free,
        }
    }
}

/// How a parent lays out one child: its size rules, its alignment, its cell in a grid. Kept by
/// the widget tree next to the child (WB2) and read by the parent through [`LayoutCx`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayoutProps {
    /// The width rule.
    pub width: Length,
    /// The height rule.
    pub height: Length,
    /// Horizontal alignment where the child is narrower than its space.
    pub align_x: Align,
    /// Vertical alignment where the child is lower than its space.
    pub align_y: Align,
    /// The child's cell in a [`Grid`]: (row, column). Ignored by the other containers.
    pub cell: (u16, u16),
}

impl Default for LayoutProps {
    /// Content-sized, at the start, in the first cell.
    fn default() -> Self {
        Self {
            width: Length::Content,
            height: Length::Content,
            align_x: Align::Start,
            align_y: Align::Start,
            cell: (0, 0),
        }
    }
}

impl LayoutProps {
    /// These props with a width rule.
    #[must_use]
    pub const fn width(mut self, width: Length) -> Self {
        self.width = width;
        self
    }

    /// These props with a height rule.
    #[must_use]
    pub const fn height(mut self, height: Length) -> Self {
        self.height = height;
        self
    }

    /// These props with both alignments.
    #[must_use]
    pub const fn align(mut self, x: Align, y: Align) -> Self {
        self.align_x = x;
        self.align_y = y;
        self
    }

    /// These props placed in a grid cell.
    #[must_use]
    pub const fn cell(mut self, row: u16, column: u16) -> Self {
        self.cell = (row, column);
        self
    }
}

/// A spacing value: one of the density's gaps (so it follows the theme's density, WB10) or a
/// fixed number of logical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Space {
    /// No space.
    None,
    /// [`Spacing::gap_xs`].
    Xs,
    /// [`Spacing::gap_s`].
    S,
    /// [`Spacing::gap_m`].
    M,
    /// [`Spacing::gap_l`].
    L,
    /// [`Spacing::control_padding`].
    ControlPadding,
    /// A fixed number of logical pixels.
    Px(f32),
}

impl Space {
    /// The value in logical pixels with the theme's spacing.
    #[must_use]
    pub const fn resolve(self, spacing: &Spacing) -> f32 {
        match self {
            Self::None => 0.0,
            Self::Xs => spacing.gap_xs,
            Self::S => spacing.gap_s,
            Self::M => spacing.gap_m,
            Self::L => spacing.gap_l,
            Self::ControlPadding => spacing.control_padding,
            Self::Px(value) => value,
        }
    }
}

/// The direction of a [`Flex`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    /// Children side by side, left to right.
    Horizontal,
    /// Children one under the other, top to bottom.
    Vertical,
}

/// A size seen along an axis: (main, cross).
const fn split(axis: Axis, size: Size) -> (f32, f32) {
    match axis {
        Axis::Horizontal => (size.width, size.height),
        Axis::Vertical => (size.height, size.width),
    }
}

/// A size from (main, cross) along an axis.
const fn join(axis: Axis, main: f32, cross: f32) -> Size {
    match axis {
        Axis::Horizontal => Size::new(main, cross),
        Axis::Vertical => Size::new(cross, main),
    }
}

/// The constraints of one direction of a child from its rule: tight for `Fixed`, tight to the
/// whole space for `Fill` when the space is bounded, loose otherwise. Returns (min, max).
fn direction(rule: Length, space: f32) -> (f32, f32) {
    match rule {
        Length::Fixed(value) => (value, value),
        Length::Fill(_) if space.is_finite() => (space, space),
        Length::Content | Length::Fill(_) => (0.0, space),
    }
}

/// A row or a column (ADR-0045 WA2): children one after the other along an axis, with a spacing
/// between them and a padding around them; each child aligned on the other axis by its own
/// alignment, or stretched if its rule there is `Fill`.
///
/// # Complexity
/// O(children) per measure and per arrange: one pass for fixed and content children, one for
/// fill children.
#[derive(Debug, Clone, PartialEq)]
pub struct Flex {
    /// The direction.
    pub axis: Axis,
    /// The space between two children.
    pub spacing: Space,
    /// The space around all the children.
    pub padding: Space,
}

impl Flex {
    /// A column (children top to bottom).
    #[must_use]
    pub const fn column(spacing: Space, padding: Space) -> Self {
        Self {
            axis: Axis::Vertical,
            spacing,
            padding,
        }
    }

    /// A row (children left to right).
    #[must_use]
    pub const fn row(spacing: Space, padding: Space) -> Self {
        Self {
            axis: Axis::Horizontal,
            spacing,
            padding,
        }
    }

    /// The rules of child `index` along the main and the cross axis.
    fn rules<M: 'static>(&self, cx: &LayoutCx<'_, M>, index: usize) -> (Length, Length) {
        let props = cx.child_props(index);
        match self.axis {
            Axis::Horizontal => (props.width, props.height),
            Axis::Vertical => (props.height, props.width),
        }
    }
}

impl<M: 'static> Widget<M> for Flex {
    fn measure(&mut self, cx: &mut LayoutCx<'_, M>, constraints: Constraints) -> Size {
        let spacing = cx.spacing();
        let padding = self.padding.resolve(&spacing);
        let gap = self.spacing.resolve(&spacing);
        let count = cx.child_count();
        let (max_main, max_cross) = split(self.axis, constraints.max);
        let inner_cross = (max_cross - 2.0 * padding).max(0.0);
        // Gaps between `count` children: `count - 1` of them.
        #[allow(
            clippy::cast_precision_loss,
            reason = "a child count is far below 2^24, where f32 is exact"
        )]
        let gaps = gap * count.saturating_sub(1) as f32;
        let mut used = 0.0;
        let mut cross: f32 = 0.0;
        let mut weights = 0.0;
        // Pass 1: fixed and content children, measured against the space still free.
        for index in 0..count {
            let (main_rule, cross_rule) = self.rules(cx, index);
            if let Length::Fill(weight) = main_rule
                && max_main.is_finite()
            {
                weights += weight.max(0.0);
                continue;
            }
            let free = (max_main - 2.0 * padding - gaps - used).max(0.0);
            let (main_min, main_max) = direction(main_rule, free);
            let (cross_min, cross_max) = direction(cross_rule, inner_cross);
            let child = cx.measure_child(
                index,
                Constraints {
                    min: join(self.axis, main_min, cross_min),
                    max: join(self.axis, main_max, cross_max),
                },
            );
            let (child_main, child_cross) = split(self.axis, child);
            used += child_main;
            cross = cross.max(child_cross);
        }
        // Pass 2: fill children share what is left, by weight.
        let left = (max_main - 2.0 * padding - gaps - used).max(0.0);
        for index in 0..count {
            let (main_rule, cross_rule) = self.rules(cx, index);
            let Length::Fill(weight) = main_rule else {
                continue;
            };
            if !max_main.is_finite() {
                continue;
            }
            let share = if weights > 0.0 {
                left * weight.max(0.0) / weights
            } else {
                0.0
            };
            let (cross_min, cross_max) = direction(cross_rule, inner_cross);
            let child = cx.measure_child(
                index,
                Constraints {
                    min: join(self.axis, share, cross_min),
                    max: join(self.axis, share, cross_max),
                },
            );
            let (child_main, child_cross) = split(self.axis, child);
            used += child_main;
            cross = cross.max(child_cross);
        }
        let main = if weights > 0.0 && max_main.is_finite() {
            max_main
        } else {
            used + gaps + 2.0 * padding
        };
        constraints.constrain(join(self.axis, main, cross + 2.0 * padding))
    }

    fn arrange(&mut self, cx: &mut LayoutCx<'_, M>, rect: Rect) {
        let spacing = cx.spacing();
        let padding = self.padding.resolve(&spacing);
        let gap = self.spacing.resolve(&spacing);
        let (main_start, cross_start) = match self.axis {
            Axis::Horizontal => (rect.x0, rect.y0),
            Axis::Vertical => (rect.y0, rect.x0),
        };
        let (_, inner_cross) = split(self.axis, Size::new(rect.width(), rect.height()));
        let inner_cross = (inner_cross - 2.0 * padding).max(0.0);
        let mut position = main_start + padding;
        for index in 0..cx.child_count() {
            let size = cx.child_size(index);
            let (child_main, child_cross) = split(self.axis, size);
            let props = cx.child_props(index);
            let (cross_rule, align) = match self.axis {
                Axis::Horizontal => (props.height, props.align_y),
                Axis::Vertical => (props.width, props.align_x),
            };
            let (cross_offset, cross_size) = if matches!(cross_rule, Length::Fill(_)) {
                (0.0, inner_cross)
            } else {
                (align.offset(inner_cross, child_cross), child_cross)
            };
            let cross = cross_start + padding + cross_offset;
            let child = match self.axis {
                Axis::Horizontal => Rect::from_origin_size(position, cross, child_main, cross_size),
                Axis::Vertical => Rect::from_origin_size(cross, position, cross_size, child_main),
            };
            cx.place_child(index, child);
            position += child_main + gap;
        }
    }

    fn record(&self, _cx: &super::tree::RecordCx, _recorder: &mut crate::Recorder<'_>) {}
}

/// Children on top of each other (ADR-0045 WA2): each sized by its own rules inside the
/// padded area and aligned by its own alignment; later children are drawn on top.
///
/// # Complexity
/// O(children) per measure and per arrange.
#[derive(Debug, Clone, PartialEq)]
pub struct Stack {
    /// The space around the children.
    pub padding: Space,
}

impl<M: 'static> Widget<M> for Stack {
    fn measure(&mut self, cx: &mut LayoutCx<'_, M>, constraints: Constraints) -> Size {
        let padding = self.padding.resolve(&cx.spacing());
        let inner = Size::new(
            (constraints.max.width - 2.0 * padding).max(0.0),
            (constraints.max.height - 2.0 * padding).max(0.0),
        );
        let mut size = Size::ZERO;
        for index in 0..cx.child_count() {
            let props = cx.child_props(index);
            let (min_w, max_w) = direction(props.width, inner.width);
            let (min_h, max_h) = direction(props.height, inner.height);
            let child = cx.measure_child(
                index,
                Constraints {
                    min: Size::new(min_w, min_h),
                    max: Size::new(max_w, max_h),
                },
            );
            size.width = size.width.max(child.width);
            size.height = size.height.max(child.height);
        }
        constraints.constrain(Size::new(
            size.width + 2.0 * padding,
            size.height + 2.0 * padding,
        ))
    }

    fn arrange(&mut self, cx: &mut LayoutCx<'_, M>, rect: Rect) {
        let padding = self.padding.resolve(&cx.spacing());
        let inner_w = (rect.width() - 2.0 * padding).max(0.0);
        let inner_h = (rect.height() - 2.0 * padding).max(0.0);
        for index in 0..cx.child_count() {
            let size = cx.child_size(index);
            let props = cx.child_props(index);
            let x = rect.x0 + padding + props.align_x.offset(inner_w, size.width);
            let y = rect.y0 + padding + props.align_y.offset(inner_h, size.height);
            cx.place_child(index, Rect::from_origin_size(x, y, size.width, size.height));
        }
    }

    fn record(&self, _cx: &super::tree::RecordCx, _recorder: &mut crate::Recorder<'_>) {}
}

/// A track (a row or a column) of a [`Grid`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Track {
    /// Exactly this many logical pixels.
    Fixed(f32),
    /// As large as its largest child.
    Content,
    /// A share of the space left, by weight.
    Fill(f32),
}

/// Rows and columns of tracks (ADR-0045 WA2): each child in its cell (its [`LayoutProps::cell`]),
/// sized by the cell and aligned in it by its own alignment. A child whose cell is outside the
/// grid is laid out at size zero in the first cell.
///
/// # How the tracks are sized
/// Columns first: fixed ones take their size, content ones the widest child they hold (measured
/// loose), fill ones share what is left. Then rows the same way, with every child measured at its
/// column's width (so wrapped text gets its real height).
///
/// # Complexity
/// O(children + tracks) per measure, with each child measured at most twice (loose for a content
/// column, then at its column's width).
#[derive(Debug, Clone, PartialEq)]
pub struct Grid {
    /// The columns, left to right.
    pub columns: Vec<Track>,
    /// The rows, top to bottom.
    pub rows: Vec<Track>,
    /// The space between tracks.
    pub gap: Space,
    /// The space around the grid.
    pub padding: Space,
    /// The resolved column widths of the last measure (kept to arrange; reused, no allocation
    /// once sized).
    widths: Vec<f32>,
    /// The resolved row heights of the last measure.
    heights: Vec<f32>,
    /// Working memory: the content size of each track while measuring.
    content: Vec<f32>,
}

impl Grid {
    /// A grid of these tracks.
    #[must_use]
    pub const fn new(columns: Vec<Track>, rows: Vec<Track>, gap: Space, padding: Space) -> Self {
        Self {
            columns,
            rows,
            gap,
            padding,
            widths: Vec::new(),
            heights: Vec::new(),
            content: Vec::new(),
        }
    }

    /// Resolves `tracks` into `out` given each track's content size and the space available.
    fn resolve(tracks: &[Track], content: &[f32], space: f32, gap: f32, out: &mut Vec<f32>) {
        out.clear();
        let mut used = 0.0;
        let mut weights = 0.0;
        for (track, &content) in tracks.iter().zip(content) {
            let size = match *track {
                Track::Fixed(value) => value,
                Track::Content => content,
                Track::Fill(weight) => {
                    if space.is_finite() {
                        weights += weight.max(0.0);
                        0.0
                    } else {
                        content
                    }
                }
            };
            used += size;
            out.push(size);
        }
        #[allow(
            clippy::cast_precision_loss,
            reason = "a track count is far below 2^24, where f32 is exact"
        )]
        let gaps = gap * tracks.len().saturating_sub(1) as f32;
        let left = (space - used - gaps).max(0.0);
        if weights > 0.0 && space.is_finite() {
            for (track, size) in tracks.iter().zip(out.iter_mut()) {
                if let Track::Fill(weight) = *track {
                    *size = left * weight.max(0.0) / weights;
                }
            }
        }
    }
}

impl<M: 'static> Widget<M> for Grid {
    fn measure(&mut self, cx: &mut LayoutCx<'_, M>, constraints: Constraints) -> Size {
        let spacing = cx.spacing();
        let padding = self.padding.resolve(&spacing);
        let gap = self.gap.resolve(&spacing);
        let inner_w = (constraints.max.width - 2.0 * padding).max(0.0);
        let inner_h = (constraints.max.height - 2.0 * padding).max(0.0);
        let count = cx.child_count();
        // Columns: the widest child of each content column, measured loose.
        let mut content = std::mem::take(&mut self.content);
        content.clear();
        content.resize(self.columns.len(), 0.0);
        for index in 0..count {
            let (_, column) = cx.child_props(index).cell;
            let column = usize::from(column);
            if matches!(self.columns.get(column), Some(Track::Content)) {
                let size = cx.measure_child(index, Constraints::loose(Size::new(inner_w, inner_h)));
                if let Some(slot) = content.get_mut(column) {
                    *slot = slot.max(size.width);
                }
            }
        }
        let mut widths = std::mem::take(&mut self.widths);
        Self::resolve(&self.columns, &content, inner_w, gap, &mut widths);
        // Rows: every child measured at its column's width.
        content.clear();
        content.resize(self.rows.len(), 0.0);
        for index in 0..count {
            let (row, column) = cx.child_props(index).cell;
            let width = widths.get(usize::from(column)).copied().unwrap_or(0.0);
            let props = cx.child_props(index);
            let (min_w, max_w) = direction(props.width, width);
            let size = cx.measure_child(
                index,
                Constraints {
                    min: Size::new(min_w, 0.0),
                    max: Size::new(max_w, inner_h),
                },
            );
            if let Some(slot) = content.get_mut(usize::from(row)) {
                *slot = slot.max(size.height);
            }
        }
        let mut heights = std::mem::take(&mut self.heights);
        Self::resolve(&self.rows, &content, inner_h, gap, &mut heights);
        self.content = content;
        #[allow(
            clippy::cast_precision_loss,
            reason = "a track count is far below 2^24, where f32 is exact"
        )]
        let (gaps_w, gaps_h) = (
            gap * self.columns.len().saturating_sub(1) as f32,
            gap * self.rows.len().saturating_sub(1) as f32,
        );
        let size = Size::new(
            widths.iter().sum::<f32>() + gaps_w + 2.0 * padding,
            heights.iter().sum::<f32>() + gaps_h + 2.0 * padding,
        );
        self.widths = widths;
        self.heights = heights;
        constraints.constrain(size)
    }

    fn arrange(&mut self, cx: &mut LayoutCx<'_, M>, rect: Rect) {
        let spacing = cx.spacing();
        let padding = self.padding.resolve(&spacing);
        let gap = self.gap.resolve(&spacing);
        // The start of a track: the padding plus every earlier track and gap.
        let start = |sizes: &[f32], index: usize| -> f32 {
            sizes.iter().take(index).map(|size| size + gap).sum::<f32>() + padding
        };
        for index in 0..cx.child_count() {
            let props = cx.child_props(index);
            let (row, column) = (usize::from(props.cell.0), usize::from(props.cell.1));
            let (Some(&width), Some(&height)) = (self.widths.get(column), self.heights.get(row))
            else {
                cx.place_child(index, Rect::from_origin_size(rect.x0, rect.y0, 0.0, 0.0));
                continue;
            };
            let size = cx.child_size(index);
            let (w, h) = (
                if matches!(props.width, Length::Fill(_)) {
                    width
                } else {
                    size.width.min(width)
                },
                if matches!(props.height, Length::Fill(_)) {
                    height
                } else {
                    size.height.min(height)
                },
            );
            let x = rect.x0 + start(&self.widths, column) + props.align_x.offset(width, w);
            let y = rect.y0 + start(&self.heights, row) + props.align_y.offset(height, h);
            cx.place_child(index, Rect::from_origin_size(x, y, w, h));
        }
    }

    fn record(&self, _cx: &super::tree::RecordCx, _recorder: &mut crate::Recorder<'_>) {}
}
