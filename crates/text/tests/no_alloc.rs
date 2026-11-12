// SPDX-License-Identifier: GPL-3.0-or-later

//! Rebuilding a layout of the same size does not allocate (ADR-0044 TX9, TB7).
//!
//! A widget lays its text out again whenever it changes (a counter ticking, a label being
//! translated, a value being dragged). If every layout touched the heap, a busy interface
//! would churn the allocator for nothing. After the first layout, the shaper's buffer, plans,
//! glyph and break lists, and the layout's own vectors all have the capacity they need.
//!
//! This file is its own test binary with the counting allocator installed (ADR-0016 §5).

#![allow(
    clippy::unwrap_used,
    reason = "test helpers: a failed unwrap fails the test"
)]

use std::hint::black_box;

use orchestra_test_support::assert_no_allocations;
use orchestra_text::{FaceId, Faces, Shaper, TextLayout, TextStyle};

orchestra_test_support::install_counting_allocator!();

/// Texts of every kind the layout handles: wrapping, a soft hyphen, a hard break, isolation
/// marks, a missing character, tabular figures.
const TEXTS: [&str; 5] = [
    "The quick brown fox jumps over the lazy dog, again and again.",
    "Takt\u{AD}art\u{AD}wechsel in Takt \u{2068}12\u{2069}\nzweite Zeile",
    "Mesure\u{202F}? «\u{A0}Allegro\u{A0}» — déjà vu",
    "中文 and 0123456789",
    "",
];

#[test]
fn relaying_out_the_same_texts_does_not_allocate() {
    let faces = Faces::bundled().unwrap();
    let mut shaper = Shaper::new();
    let mut layout = TextLayout::new();
    let styles = [
        TextStyle::new(FaceId::InterRegular, 13.0),
        TextStyle::new(FaceId::InterBold, 18.0).with_tabular_figures(true),
        TextStyle::new(FaceId::JetBrainsMono, 12.0),
    ];
    let round = |shaper: &mut Shaper, layout: &mut TextLayout| {
        for style in styles {
            for text in TEXTS {
                for width in [None, Some(80.0)] {
                    shaper
                        .layout(&faces, black_box(text), style, width, layout)
                        .unwrap();
                    black_box(&*layout);
                }
            }
        }
    };
    // The first round grows every vector and compiles every plan.
    round(&mut shaper, &mut layout);
    // The second round finds everything ready.
    assert_no_allocations(|| round(&mut shaper, &mut layout));
}

#[test]
fn measuring_does_not_allocate() {
    let faces = Faces::bundled().unwrap();
    let notehead = faces.glyph_for_char(FaceId::Bravura, '\u{E0A4}').unwrap();
    // Warm the fonts' lazily read tables.
    faces.line_metrics(FaceId::Bravura, 32.0).unwrap();
    faces.advance(FaceId::Bravura, notehead, 32.0).unwrap();
    assert_no_allocations(|| {
        for face in FaceId::ALL {
            black_box(faces.line_metrics(face, black_box(13.0)).unwrap());
            black_box(faces.bounds(face, black_box(13.0)).unwrap());
            black_box(faces.glyph_for_char(face, black_box('A')));
        }
        black_box(faces.advance(FaceId::Bravura, notehead, 32.0).unwrap());
    });
}
