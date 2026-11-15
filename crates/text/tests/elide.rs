// SPDX-License-Identifier: GPL-3.0-or-later

//! Eliding a one-line text to a width (step F2.09f; ADR-0045 WJ1–WJ7): hand-written cases and a
//! property test against an oracle.
//!
//! # The oracle
//! It shares no code with `Shaper::elide`: it does not guess anything from caret positions. For
//! every grapheme boundary of the text it builds "start without trailing spaces + ellipsis" as a
//! plain string, shapes it on its own with a fresh call of `Shaper::layout`, and reads its
//! width. The expected answer is then the definition itself:
//! - the whole text if it fits (or has several lines);
//! - otherwise the **longest** start whose shortened text fits (the ellipsis alone if no
//!   grapheme fits before it);
//! - nothing if the ellipsis alone does not fit.
//!
//! The library's output layout must equal, glyph for glyph, the layout of that shortened string.

#![allow(
    clippy::unwrap_used,
    clippy::float_cmp,
    clippy::string_slice,
    reason = "test helpers: a failed unwrap or a slice off a character boundary fails the test; \
              the widths compared are the same stored f32 values, produced by the same shaping \
              of the same string"
)]

use orchestra_text::{
    Elision, FaceId, Faces, Shaper, TextError, TextLayout, TextStyle, grapheme_boundaries,
    is_grapheme_boundary,
};
use proptest::prelude::*;

/// Inter Regular at 13 px, the body text of the interface.
const BODY: TextStyle = TextStyle::new(FaceId::InterRegular, 13.0);

/// The ellipsis the bundled interface faces have.
const ELLIPSIS: &str = "\u{2026}";

/// Lays `text` out on one line (no wrapping width) in `style`.
fn lay_out(faces: &Faces, text: &str, style: TextStyle) -> TextLayout {
    let mut layout = TextLayout::new();
    Shaper::new()
        .layout(faces, text, style, None, &mut layout)
        .unwrap();
    layout
}

/// Elides `text` to `width` with a fresh shaper; returns the answer and the output layout.
fn elide(faces: &Faces, text: &str, style: TextStyle, width: f32) -> (Elision, TextLayout) {
    let full = lay_out(faces, text, style);
    let mut out = TextLayout::new();
    let elision = Shaper::new()
        .elide(faces, text, style, width, &full, &mut out)
        .unwrap();
    (elision, out)
}

/// The oracle (see the module documentation): what eliding `text` to `width` must answer, and
/// the string whose layout the output must equal (`None` for the whole text: the output is not
/// touched).
fn expected(faces: &Faces, text: &str, style: TextStyle, width: f32) -> (Elision, Option<String>) {
    let full = lay_out(faces, text, style);
    if full.width() <= width || full.lines().len() > 1 {
        return (Elision::Whole, None);
    }
    // Every candidate, shortest first: the start up to each boundary but the text's end.
    let mut best: Option<String> = None;
    let mut best_kept = 0;
    for boundary in grapheme_boundaries(text).filter(|&boundary| boundary < text.len()) {
        let kept = text[..boundary].trim_end();
        let shortened = format!("{kept}{ELLIPSIS}");
        if lay_out(faces, &shortened, style).width() <= width {
            // Later boundaries overwrite earlier ones: the longest fitting start wins.
            best_kept = kept.len();
            best = Some(shortened);
        }
    }
    match best {
        Some(shortened) => (Elision::Cut { kept: best_kept }, Some(shortened)),
        None => (Elision::Hidden, Some(String::new())),
    }
}

/// Checks the library against the oracle for one text, style and width.
fn check(faces: &Faces, text: &str, style: TextStyle, width: f32) {
    let (elision, out) = elide(faces, text, style, width);
    let (want, shortened) = expected(faces, text, style, width);
    assert_eq!(elision, want, "text {text:?}, width {width}");
    match shortened {
        // The whole text: the output layout was not touched.
        None => assert_eq!(out, TextLayout::new(), "text {text:?}, width {width}"),
        Some(shortened) => {
            assert_eq!(
                out,
                lay_out(faces, &shortened, style),
                "text {text:?}, width {width}"
            );
            // Whatever was decided, nothing is drawn wider than the width (WJ5).
            assert!(out.width() <= width, "text {text:?}, width {width}");
        }
    }
    if let Elision::Cut { kept } = elision {
        // The kept part ends on a grapheme boundary and not with white space (WJ4).
        assert!(
            is_grapheme_boundary(text, kept),
            "text {text:?}, kept {kept}"
        );
        assert_eq!(text[..kept].trim_end().len(), kept, "text {text:?}");
    }
}

/// Every width from zero to past the full text, in steps of three quarters of a pixel (finer
/// than the narrowest letter, and not a divisor of the usual advances, so the widths fall on
/// both sides of every boundary).
fn sweep(faces: &Faces, text: &str, style: TextStyle) {
    let full = lay_out(faces, text, style).width();
    let mut width = 0.0;
    while width <= full + 1.0 {
        check(faces, text, style, width);
        width += 0.75;
    }
}

#[test]
fn a_text_that_fits_is_left_whole_and_the_output_untouched() {
    let faces = Faces::bundled().unwrap();
    let text = "Volume";
    let full = lay_out(&faces, text, BODY);
    let (elision, out) = elide(&faces, text, BODY, full.width());
    assert_eq!(elision, Elision::Whole);
    assert_eq!(out, TextLayout::new());
    // An infinite width means "no limit".
    let (elision, _) = elide(&faces, text, BODY, f32::INFINITY);
    assert_eq!(elision, Elision::Whole);
}

#[test]
fn a_text_too_long_keeps_its_start_and_ends_with_the_ellipsis() {
    let faces = Faces::bundled().unwrap();
    let text = "Reverb send level";
    let full = lay_out(&faces, text, BODY);
    let width = full.width() * 0.6;
    let (elision, out) = elide(&faces, text, BODY, width);
    let Elision::Cut { kept } = elision else {
        panic!("expected a cut, got {elision:?}");
    };
    assert!(kept > 0 && kept < text.len());
    assert!(out.width() <= width);
    assert_eq!(out.lines().len(), 1);
    assert_eq!(out.height(), full.height());
    // The last glyph is the face's ellipsis.
    let ellipsis = faces.glyph_for_char(BODY.face, '\u{2026}').unwrap();
    assert_eq!(out.glyphs().last().unwrap().glyph, ellipsis);
    // One pixel less than the full width already cuts.
    let (elision, _) = elide(&faces, text, BODY, full.width() - 1.0);
    assert!(matches!(elision, Elision::Cut { .. }));
}

#[test]
fn the_space_before_the_cut_is_dropped() {
    let faces = Faces::bundled().unwrap();
    let text = "Reverb send level";
    // Wide enough for "Reverb " and the ellipsis, too narrow for "Reverb s" and the ellipsis:
    // the cut falls right after the space, which is not drawn.
    let width = lay_out(&faces, "Reverb\u{2026}", BODY).width() + 2.0;
    let (elision, out) = elide(&faces, text, BODY, width);
    assert_eq!(
        elision,
        Elision::Cut {
            kept: "Reverb".len()
        }
    );
    assert_eq!(out, lay_out(&faces, "Reverb\u{2026}", BODY));
}

#[test]
fn every_width_of_an_interface_label_agrees_with_the_oracle() {
    let faces = Faces::bundled().unwrap();
    sweep(&faces, "Reverb send level", BODY);
    sweep(&faces, "Metronome during count-in", BODY);
    sweep(
        &faces,
        "Track 12: Violin I",
        TextStyle::new(FaceId::InterSemiBold, 15.0),
    );
    sweep(
        &faces,
        "/home/user/Music/suite.orchestra",
        TextStyle::new(FaceId::JetBrainsMono, 12.0),
    );
}

#[test]
fn graphemes_are_never_split() {
    let faces = Faces::bundled().unwrap();
    // A decomposed accent, a mark with no precomposed form, a ligature candidate, an emoji with a
    // skin tone and a flag (both drawn as missing glyphs, but still one grapheme each), CJK.
    sweep(&faces, "cafe\u{301} q\u{301}uq\u{323} office", BODY);
    sweep(&faces, "a\u{1F44D}\u{1F3FD}b\u{1F1EE}\u{1F1F9}c", BODY);
    sweep(&faces, "中文 Größe", BODY);
}

#[test]
fn the_ellipsis_stands_alone_and_then_nothing_is_drawn() {
    let faces = Faces::bundled().unwrap();
    let text = "Wide text";
    let alone = lay_out(&faces, ELLIPSIS, BODY);
    // Room for the ellipsis, not for "W" before it.
    let (elision, out) = elide(&faces, text, BODY, alone.width() + 1.0);
    assert_eq!(elision, Elision::Cut { kept: 0 });
    assert_eq!(out, alone);
    // No room for the ellipsis either: nothing, on one empty line of the same height.
    for width in [alone.width() - 0.5, 0.0] {
        let (elision, out) = elide(&faces, text, BODY, width);
        assert_eq!(elision, Elision::Hidden);
        assert_eq!(out.glyphs(), []);
        assert_eq!(out.lines().len(), 1);
        assert_eq!(out.height(), alone.height());
        assert_eq!(out.width(), 0.0);
    }
}

#[test]
fn leading_spaces_alone_are_not_kept() {
    let faces = Faces::bundled().unwrap();
    // Wide enough for two spaces and the ellipsis by pen position, but spaces before the
    // ellipsis are dropped: the ellipsis stands alone.
    let text = "   Wide text";
    let width = lay_out(&faces, ELLIPSIS, BODY).width() + 7.0;
    let (elision, out) = elide(&faces, text, BODY, width);
    assert_eq!(elision, Elision::Cut { kept: 0 });
    assert_eq!(out, lay_out(&faces, ELLIPSIS, BODY));
    sweep(&faces, text, BODY);
}

#[test]
fn a_text_with_a_mandatory_line_break_is_left_whole() {
    let faces = Faces::bundled().unwrap();
    let (elision, out) = elide(&faces, "First line\nSecond line", BODY, 20.0);
    assert_eq!(elision, Elision::Whole);
    assert_eq!(out, TextLayout::new());
}

#[test]
fn a_face_without_the_ellipsis_gets_three_full_stops() {
    let faces = Faces::bundled().unwrap();
    // Bravura, the music font, has neither U+2026 nor the full stop: the fallback's three
    // characters are then three missing-glyph boxes, which is what any text in a face without
    // letters looks like (ADR-0044 TX4). What matters is that the fallback is three characters
    // and that the result still fits.
    assert!(faces.glyph_for_char(FaceId::Bravura, '\u{2026}').is_none());
    assert!(faces.glyph_for_char(FaceId::Bravura, '.').is_none());
    let style = TextStyle::new(FaceId::Bravura, 20.0);
    let text = "abcdefghij";
    let full = lay_out(&faces, text, style);
    let width = full.width() * 0.7;
    let (elision, out) = elide(&faces, text, style, width);
    let Elision::Cut { kept } = elision else {
        panic!("expected a cut, got {elision:?}");
    };
    assert!(kept > 0);
    assert!(out.width() <= width);
    assert_eq!(
        out,
        lay_out(&faces, &format!("{}...", &text[..kept]), style)
    );
    // One grapheme more would not fit.
    let longer = format!("{}...", &text[..=kept]);
    assert!(lay_out(&faces, &longer, style).width() > width);
}

#[test]
fn invalid_widths_and_sizes_are_refused() {
    let faces = Faces::bundled().unwrap();
    let text = "Volume";
    let full = lay_out(&faces, text, BODY);
    let mut out = TextLayout::new();
    let mut shaper = Shaper::new();
    for width in [f32::NAN, -1.0, f32::NEG_INFINITY] {
        assert_eq!(
            shaper.elide(&faces, text, BODY, width, &full, &mut out),
            Err(TextError::InvalidWidth)
        );
    }
    let no_size = TextStyle::new(FaceId::InterRegular, 0.0);
    assert_eq!(
        shaper.elide(&faces, text, no_size, 10.0, &full, &mut out),
        Err(TextError::InvalidSize)
    );
    assert_eq!(out, TextLayout::new());
}

#[test]
fn a_shaper_gives_the_same_answer_again_and_after_other_texts() {
    // The working memory (the shortened string, the scratch layout) must not leak from one call
    // into the next.
    let faces = Faces::bundled().unwrap();
    let mut shaper = Shaper::new();
    let texts = ["Reverb send level", "Metronome during count-in", "Volume"];
    let mut out = TextLayout::new();
    for _ in 0..2 {
        for text in texts {
            let mut full = TextLayout::new();
            shaper.layout(&faces, text, BODY, None, &mut full).unwrap();
            for width in [5.0, 30.0, 60.0, 90.0] {
                let elision = shaper
                    .elide(&faces, text, BODY, width, &full, &mut out)
                    .unwrap();
                let (want, shortened) = expected(&faces, text, BODY, width);
                assert_eq!(elision, want, "text {text:?}, width {width}");
                if let Some(shortened) = shortened {
                    assert_eq!(out, lay_out(&faces, &shortened, BODY));
                }
            }
        }
    }
}

/// Words of the generator: short and long, with kerning pairs, ligature candidates, a decomposed
/// accent, a mark that stays a separate glyph, an emoji sequence, German, Chinese.
const WORDS: [&str; 13] = [
    "a",
    "Takt",
    "wechsel",
    "Allegro",
    "AV",
    "To",
    "office",
    "WAVE",
    "Größe",
    "cafe\u{301}",
    "q\u{301}uq\u{323}",
    "\u{1F44D}\u{1F3FD}",
    "中文",
];

/// Separators: spaces, a no-break space, a hyphen, a soft hyphen (no width), isolation marks
/// (hidden), nothing.
const SEPARATORS: [&str; 8] = [
    " ", "  ", "-", "\u{AD}", "\u{A0}", "\u{2068}", "\u{2069}", "",
];

/// A random one-line text of up to 8 words, a face and a width from nothing to wider than most
/// of the texts.
fn text_style_width() -> impl Strategy<Value = (String, TextStyle, f32)> {
    let pieces = prop::collection::vec((0..WORDS.len(), 0..SEPARATORS.len()), 0..8);
    let style = prop_oneof![
        Just(BODY),
        Just(TextStyle::new(FaceId::InterBold, 18.0)),
        Just(TextStyle::new(FaceId::JetBrainsMono, 12.0)),
    ];
    (pieces, style, 0.0_f32..260.0).prop_map(|(pieces, style, width)| {
        let mut text = String::new();
        for (word, separator) in pieces {
            text.push_str(WORDS[word]);
            text.push_str(SEPARATORS[separator]);
        }
        (text, style, width)
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(600))]

    #[test]
    fn eliding_agrees_with_the_oracle((text, style, width) in text_style_width()) {
        let faces = Faces::bundled().unwrap();
        check(&faces, &text, style, width);
    }
}
