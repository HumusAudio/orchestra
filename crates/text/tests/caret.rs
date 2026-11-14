// SPDX-License-Identifier: GPL-3.0-or-later

//! The caret's geometry (ADR-0047 TF1, TF4): caret stops recorded by the layout, `caret_x` at
//! cluster boundaries, inside a grapheme and inside a ligature, trailing spaces, wrapped lines,
//! and `hit_test` as its inverse, with a property test over random texts.

#![allow(
    clippy::unwrap_used,
    clippy::float_cmp,
    reason = "test helpers: a failed unwrap fails the test; caret positions compared are the same \
              stored f32 values, or sums the test computes the same way"
)]

use orchestra_text::{
    FaceId, Faces, Shaper, TextLayout, TextStyle, grapheme_boundaries, next_grapheme,
};
use proptest::prelude::*;

/// Inter Regular at 13 px, the body text.
const BODY: TextStyle = TextStyle::new(FaceId::InterRegular, 13.0);

/// Lays `text` out with an optional width.
fn lay_out(faces: &Faces, text: &str, width: Option<f32>) -> TextLayout {
    let mut layout = TextLayout::new();
    Shaper::new()
        .layout(faces, text, BODY, width, &mut layout)
        .unwrap();
    layout
}

#[test]
fn caret_positions_grow_from_zero_to_the_end_of_the_text() {
    let faces = Faces::bundled().unwrap();
    let text = "Flute 1";
    let layout = lay_out(&faces, text, None);
    assert_eq!(layout.caret_x(text, 0), 0.0);
    // Without trailing spaces, the end of the text is the line's width.
    assert!((layout.caret_x(text, text.len()) - layout.width()).abs() < 1e-3);
    let mut previous = -1.0;
    for boundary in grapheme_boundaries(text) {
        let x = layout.caret_x(text, boundary);
        assert!(x > previous, "{boundary}: {x} after {previous}");
        previous = x;
    }
    // One stop per cluster, plus the line's end.
    assert_eq!(layout.lines()[0].carets.len(), text.len() + 1);
}

#[test]
fn a_trailing_space_moves_the_caret_past_the_width() {
    let faces = Faces::bundled().unwrap();
    let text = "Oboe ";
    let layout = lay_out(&faces, text, None);
    // The space hangs: not in the width, but the caret after it stands further right.
    assert!(layout.caret_x(text, text.len()) > layout.width() + 1.0);
}

#[test]
fn inside_a_grapheme_the_caret_stays_at_its_start() {
    let faces = Faces::bundled().unwrap();
    // "e" + combining acute: one grapheme of three bytes.
    let text = "e\u{301}a";
    let layout = lay_out(&faces, text, None);
    assert_eq!(layout.caret_x(text, 1), layout.caret_x(text, 0));
    assert_eq!(layout.caret_x(text, 2), 0.0);
    assert!(layout.caret_x(text, 3) > 0.0);
}

#[test]
fn inside_a_ligature_the_advance_is_split_evenly() {
    let faces = Faces::bundled().unwrap();
    let text = "fi";
    let layout = lay_out(&faces, text, None);
    let end = layout.caret_x(text, 2);
    let middle = layout.caret_x(text, 1);
    let stops = &layout.caret_stops()[layout.lines()[0].carets.clone()];
    if stops.len() == 2 {
        // One cluster for both letters (the face made a ligature): half its advance.
        assert!((middle - end / 2.0).abs() < 1e-3, "{middle} of {end}");
    } else {
        // Two clusters: the second one's stop.
        assert_eq!(middle, stops[1].x);
    }
    assert_eq!(layout.hit_test(text, middle, 5.0), 1);
}

#[test]
fn clicks_find_the_nearest_caret_position() {
    let faces = Faces::bundled().unwrap();
    let text = "Horn in F";
    let layout = lay_out(&faces, text, None);
    let y = 5.0;
    assert_eq!(layout.hit_test(text, -20.0, y), 0);
    assert_eq!(layout.hit_test(text, 10_000.0, y), text.len());
    // Just left and just right of the middle between the carets around "o".
    let (a, b) = (layout.caret_x(text, 1), layout.caret_x(text, 2));
    let middle = f32::midpoint(a, b);
    assert_eq!(layout.hit_test(text, middle - 0.1, y), 1);
    assert_eq!(layout.hit_test(text, middle + 0.1, y), 2);
    // Above and below the line: the same line.
    assert_eq!(layout.hit_test(text, b, -50.0), 2);
    assert_eq!(layout.hit_test(text, b, 500.0), 2);
}

#[test]
fn an_empty_text_has_a_caret_at_zero() {
    let faces = Faces::bundled().unwrap();
    let layout = lay_out(&faces, "", None);
    assert_eq!(layout.caret_x("", 0), 0.0);
    assert_eq!(layout.hit_test("", 30.0, 5.0), 0);
    assert_eq!(layout.lines()[0].carets.len(), 1);
}

#[test]
fn wrapped_lines_have_their_own_carets() {
    let faces = Faces::bundled().unwrap();
    // A width just wider than the first word, so the second (shorter) word goes alone on the
    // second line, whatever the face's exact advances.
    let text = "Violoncello Viola";
    let unwrapped = lay_out(&faces, text, None);
    let width = unwrapped.caret_x(text, "Violoncello".len()) + 1.0;
    let layout = lay_out(&faces, text, Some(width));
    assert_eq!(layout.lines().len(), 2);
    let second = layout.lines()[1].text.start;
    // The second line's start belongs to it, at x 0.
    assert_eq!(layout.line_of(second), 1);
    assert_eq!(layout.caret_x(text, second), 0.0);
    let line_height = layout.line_metrics().line_height();
    // A click on the second line, far right: the end of the text.
    assert_eq!(layout.hit_test(text, 1000.0, line_height * 1.5), text.len());
    // Far right on the first line: before its last cluster (the trailing space), not the
    // second line's start.
    assert_eq!(layout.hit_test(text, 1000.0, line_height * 0.5), second - 1);
}

proptest! {
    /// For every grapheme boundary of a random text, a click at its caret's x lands on a caret
    /// with the same x (the same boundary, or an earlier one where zero-width graphemes share the
    /// place), and caret positions never decrease.
    #[test]
    fn hit_test_inverts_caret_x(text in "[a-zA-Zàéî fi\u{301}\u{1F1EE}\u{1F1F9}0-9,.]{0,24}") {
        let faces = Faces::bundled().unwrap();
        let layout = lay_out(&faces, &text, None);
        let mut previous = f32::NEG_INFINITY;
        for boundary in grapheme_boundaries(&text) {
            let x = layout.caret_x(&text, boundary);
            prop_assert!(x >= previous, "{boundary}: {x} after {previous}");
            previous = x;
            let hit = layout.hit_test(&text, x, 5.0);
            prop_assert!(hit <= boundary);
            prop_assert_eq!(layout.caret_x(&text, hit), x);
            // The hit is itself a caret position.
            prop_assert!(grapheme_boundaries(&text).any(|b| b == hit));
        }
        // Moving right from every boundary reaches the next one.
        let all: Vec<usize> = grapheme_boundaries(&text).collect();
        for pair in all.windows(2) {
            prop_assert_eq!(next_grapheme(&text, pair[0]), pair[1]);
        }
    }
}
