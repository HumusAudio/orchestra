// SPDX-License-Identifier: GPL-3.0-or-later

//! Line breaking against an independent oracle, on random texts and widths (ADR-0044 TX10,
//! TB7).
//!
//! # The oracle
//! It does not re-implement the algorithm: it checks the properties a correct greedy wrapping
//! must have, measuring pieces of text by shaping them on their own, and it asks UAX #14 for
//! the break opportunities directly from `icu_segmenter`:
//! 1. the lines cover the text in order, with no gap and no overlap, and their glyph ranges
//!    cover the glyphs the same way;
//! 2. no line is wider than the width, unless its content is a single cluster;
//! 3. a line ends at a UAX #14 opportunity; if it does not (an emergency cut), no usable
//!    opportunity lay inside it;
//! 4. every hard line break ends a line, and none is inside one;
//! 5. every line is as long as it can be: extended to the next opportunity (or, after an
//!    emergency cut, by the next character), it would not fit.
//!
//! Shaping a piece on its own can differ from the piece inside the whole text by the kerning
//! across its ends, so the comparisons of 5 allow a tolerance of 1 logical pixel, well below
//! the narrowest letter (an "i" is about 3 px at 13 px).

#![allow(
    clippy::unwrap_used,
    reason = "test helpers: a failed unwrap fails the test"
)]

use icu_segmenter::LineSegmenter;
use icu_segmenter::options::LineBreakOptions;
use orchestra_text::{FaceId, Faces, Shaper, TextLayout, TextStyle};
use proptest::prelude::*;

/// Inter Regular at 13 px.
const BODY: TextStyle = TextStyle::new(FaceId::InterRegular, 13.0);

/// The tolerance of the maximality check, logical pixels (see the module documentation).
const TOLERANCE: f32 = 1.0;

/// Words of the generator: short and long, with kerning pairs, ligature candidates, a
/// decomposed accent, a mark that stays a separate glyph, German, Chinese.
const WORDS: [&str; 13] = [
    "a",
    "Takt",
    "wechsel",
    "Allegro",
    "pre",
    "roll",
    "office",
    "WAVE",
    "Größe",
    "cafe\u{301}",
    // A mark with no precomposed form: two glyphs in one cluster.
    "q\u{301}uq\u{323}",
    "中文",
    "Viertelnotenpause",
];

/// Separators: spaces, a no-break space, a hyphen, a soft hyphen, hard line breaks.
const SEPARATORS: [&str; 8] = [" ", "  ", "-", "\u{AD}", "\u{A0}", "\n", "\r\n", " "];

/// A random text of up to 14 words and a random width (or none).
fn text_and_width() -> impl Strategy<Value = (String, Option<f32>)> {
    let pieces = prop::collection::vec((0..WORDS.len(), 0..SEPARATORS.len()), 1..14);
    let width = prop_oneof![
        1 => Just(None),
        6 => (0.0_f32..220.0).prop_map(Some),
    ];
    (pieces, width).prop_map(|(pieces, width)| {
        let mut text = String::new();
        for (word, separator) in pieces {
            text.push_str(WORDS[word]);
            text.push_str(SEPARATORS[separator]);
        }
        (text, width)
    })
}

/// The width of `text` shaped alone on one line (trailing spaces excluded, as in a layout).
fn width_alone(faces: &Faces, shaper: &mut Shaper, text: &str) -> f32 {
    let mut layout = TextLayout::new();
    shaper.layout(faces, text, BODY, None, &mut layout).unwrap();
    layout.width()
}

/// `true` for the characters after which a line must break.
fn is_hard_break(c: char) -> bool {
    matches!(
        c,
        '\n' | '\r' | '\u{0B}' | '\u{0C}' | '\u{85}' | '\u{2028}' | '\u{2029}'
    )
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(600))]

    #[test]
    fn wrapping_is_greedy_and_complete((text, width) in text_and_width()) {
        let faces = Faces::bundled().unwrap();
        let mut shaper = Shaper::new();
        let mut layout = TextLayout::new();
        shaper.layout(&faces, &text, BODY, width, &mut layout).unwrap();
        let lines = layout.lines();
        let hyphen = width_alone(&faces, &mut shaper, "\u{2010}");

        // The break opportunities, straight from UAX #14 (0 excluded, as in the library).
        let opportunities: Vec<usize> =
            LineSegmenter::new_for_non_complex_scripts(LineBreakOptions::default())
                .segment_str(&text)
                .filter(|&position| position > 0)
                .collect();
        // Every cluster start of the text, from the layout's own glyphs on one line.
        let mut one_line = TextLayout::new();
        shaper.layout(&faces, &text, BODY, None, &mut one_line).unwrap();
        let cluster_starts: Vec<usize> =
            one_line.glyphs().iter().map(|g| g.cluster as usize).collect();

        // 1. Coverage of the text and of the glyphs.
        prop_assert!(!lines.is_empty());
        prop_assert_eq!(lines[0].text.start, 0);
        prop_assert_eq!(lines[lines.len() - 1].text.end, text.len());
        prop_assert_eq!(lines[0].glyphs.start, 0);
        prop_assert_eq!(lines[lines.len() - 1].glyphs.end, layout.glyphs().len());
        for pair in lines.windows(2) {
            prop_assert_eq!(pair[0].text.end, pair[1].text.start);
            prop_assert_eq!(pair[0].glyphs.end, pair[1].glyphs.start);
        }

        for (row, line) in lines.iter().enumerate() {
            let piece = text.get(line.text.clone()).unwrap();
            let last = row + 1 == lines.len();
            let content = piece.trim_end_matches(char::is_whitespace);
            // The clusters of the line's content (its glyphs before the hanging spaces).
            let content_end = line.text.start + content.len();
            let clusters = cluster_starts
                .iter()
                .filter(|&&c| c >= line.text.start && c < content_end)
                .collect::<std::collections::BTreeSet<_>>()
                .len();

            // 2. Fits, or a single cluster.
            if let Some(max) = width {
                prop_assert!(
                    line.width <= max + 1e-3 || clusters <= 1,
                    "line {:?} is {} wide, max {}", piece, line.width, max
                );
            }

            // 4. Hard breaks only at the end of a line, and always ending one.
            let inner = piece.trim_end_matches(is_hard_break);
            prop_assert!(!inner.contains(is_hard_break), "hard break inside {:?}", piece);
            if !last && !piece.ends_with(is_hard_break) {
                // A line that does not end with a hard break must not be followed by one
                // that belongs to it: the next line does not start with one.
                prop_assert!(!text.get(line.text.end..).unwrap().starts_with(is_hard_break));
            }

            if last {
                continue;
            }
            let end = line.text.end;
            let at_opportunity = opportunities.contains(&end);
            // 3. An emergency cut only where no opportunity lay inside the line's content.
            if !at_opportunity {
                prop_assert!(
                    !opportunities.iter().any(|&o| o > line.text.start && o < content_end),
                    "line {:?} was cut although it had an opportunity", piece
                );
                prop_assert!(cluster_starts.contains(&end));
            }
            prop_assert_eq!(line.hyphenated, at_opportunity && piece.ends_with('\u{AD}'));

            // 5. Maximal: the next piece would not have fitted.
            let Some(max) = width else { continue };
            if piece.ends_with(is_hard_break) {
                continue;
            }
            if at_opportunity {
                // Extended to the next opportunity: too wide (with a hyphen if it ends at one).
                let next = opportunities.iter().copied().find(|&o| o > end).unwrap();
                let extended = text.get(line.text.start..next).unwrap();
                let mut extended_width = width_alone(&faces, &mut shaper, extended);
                if extended.ends_with('\u{AD}') {
                    extended_width += hyphen;
                }
                prop_assert!(
                    extended_width > max - TOLERANCE,
                    "line {:?} could have taken {:?} ({} <= {})",
                    piece, extended, extended_width, max
                );
            } else if clusters > 1 || line.width <= max {
                // Extended by the next cluster: too wide.
                let next = cluster_starts.iter().copied().find(|&c| c > end).unwrap_or(text.len());
                let extended = text.get(line.text.start..next).unwrap();
                let mut extended_width = width_alone(&faces, &mut shaper, extended);
                // A line ending at a soft hyphen that is an opportunity would show the hyphen.
                if extended.ends_with('\u{AD}') && opportunities.contains(&next) {
                    extended_width += hyphen;
                }
                prop_assert!(
                    extended_width > max - TOLERANCE,
                    "cut line {:?} could have taken {:?} ({} <= {})",
                    piece, extended, extended_width, max
                );
            }
        }
    }
}
