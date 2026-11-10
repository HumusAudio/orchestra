// SPDX-License-Identifier: GPL-3.0-or-later

//! Recording does not allocate after the first frame (ADR-0039 D1, D10).
//!
//! The interface re-records segments at every change (a note dragged, a button hovered): if that
//! touched the heap, a long editing session would churn the allocator for nothing. Each segment
//! keeps its command vector and its path arena, and the list keeps the recorder's stacks, so
//! re-recording a segment of the same size reuses their capacity. Damage is a fixed-size array.
//!
//! Drawing is not covered here: `tiny-skia` allocates internally while filling paths, which
//! ADR-0039 accepts (its instructions are counted by the benchmark instead).
//!
//! This file is its own test binary with the counting allocator installed (ADR-0016 §5).

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "test loop counters and grid sizes, all small and non-negative, turned into \
              coordinates and back"
)]

use std::hint::black_box;

use orchestra_test_support::assert_no_allocations;
use orchestra_ui::{
    ColorToken, Damage, DisplayList, FillRule, Point, Rect, RoundedRect, SegmentId, StrokeStyle,
    Transform,
};

orchestra_test_support::install_counting_allocator!();

/// A segment using every recording method, the same size at every call.
fn record_everything(list: &mut DisplayList, shift: f32) {
    list.record(SegmentId(7), |r| {
        r.push_transform(Transform::translate(shift, 0.0));
        r.push_clip(RoundedRect::new(Rect::new(0.0, 0.0, 200.0, 100.0), 4.0));
        r.fill_rect(Rect::new(1.0, 1.0, 50.0, 20.0), ColorToken::Surface);
        r.fill_rounded_rect(
            RoundedRect::new(Rect::new(1.0, 30.0, 50.0, 50.0), 6.0),
            ColorToken::Control,
        );
        r.stroke_rect(
            Rect::new(60.0, 1.0, 90.0, 20.0),
            StrokeStyle::new(1.0),
            ColorToken::Border,
        );
        r.line(
            Point::new(0.0, 60.0),
            Point::new(100.0, 60.0),
            StrokeStyle::new(1.0),
            ColorToken::Text,
        );
        for i in 0..16 {
            let x = i as f32 * 10.0;
            r.fill_path(FillRule::NonZero, ColorToken::Text, |p| {
                p.move_to(x, 70.0);
                p.cubic_to(x + 3.0, 66.0, x + 7.0, 66.0, x + 8.0, 70.0);
                p.quad_to(x + 4.0, 74.0, x, 70.0);
                p.close();
            });
        }
        r.shadow(
            RoundedRect::new(Rect::new(120.0, 10.0, 180.0, 40.0), 4.0),
            3.0,
            ColorToken::Shadow,
        );
        r.pop_clip();
        r.pop_transform();
    });
}

#[test]
fn re_recording_a_segment_of_the_same_size_never_allocates() {
    let mut list = DisplayList::new();
    // The first recording creates the segment and sizes its vectors.
    record_everything(&mut list, 0.0);
    let _ = list.take_damage();
    assert_no_allocations(|| {
        for frame in 0..10 {
            record_everything(black_box(&mut list), black_box(frame as f32));
            black_box(list.take_damage());
        }
    });
}

#[test]
fn computing_damage_never_allocates() {
    assert_no_allocations(|| {
        let mut damage = Damage::none();
        for i in 0..100 {
            let x = black_box(i as f32 * 37.0 % 900.0);
            damage.add(Rect::new(x, x * 0.5, x + 12.0, x * 0.5 + 9.0));
        }
        black_box(damage);
    });
}
