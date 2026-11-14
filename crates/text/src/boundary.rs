// SPDX-License-Identifier: GPL-3.0-or-later

//! Where a caret may stand and where words begin and end (ADR-0047 TF1): grapheme clusters and
//! words of Unicode text segmentation, UAX #29, from `icu_segmenter`.
//!
//! # Why graphemes
//! A *grapheme cluster* is what a reader sees as one character: `é` written as `e` plus a
//! combining accent, a flag made of two regional indicators, an emoji with a skin-tone modifier.
//! The text field keeps every caret and selection offset on a grapheme boundary, so the caret
//! never stands between a letter and its accent and Backspace never leaves half an emoji.
//!
//! # Why words without dictionaries
//! The word segmenter is built with `new_for_non_complex_scripts`: the UAX #29 rules compiled
//! into the binary, no dictionary or machine-learning data. That is exact for the scripts of the
//! six languages (Latin, and Greek and Cyrillic too); Chinese, Japanese, Thai, Khmer, Lao and
//! Myanmar need dictionaries to find words and get one segment per run here, a recorded limit
//! with ADR-0044 TX4–TX5. The crate is already in the workspace for line breaking (TX10): no new
//! dependency.
//!
//! # Units and complexity
//! Every offset is a **UTF-8 byte offset** into `text` (TF1). Each function walks the text from
//! its start with the segmenter's iterator, so it costs O(n) in the text's length and allocates
//! nothing (the iterators are rule state machines over borrowed static data). That is right for
//! a field of one line (tens or hundreds of bytes); a long multi-line editor would keep the
//! boundaries of each line instead, which TF2 postpones with the multi-line field.
//!
//! Offsets beyond the end are treated as the end; an offset inside a UTF-8 sequence is simply a
//! number between two boundaries, so the answers stay well defined (they are always boundaries).

use std::ops::Range;

use icu_segmenter::options::WordBreakInvariantOptions;
use icu_segmenter::{
    GraphemeClusterSegmenter, GraphemeClusterSegmenterBorrowed, WordSegmenter,
    WordSegmenterBorrowed,
};

/// The grapheme segmenter with its rules compiled into the binary. A `const fn` returning a
/// handle on static data: calling it costs nothing, so no segmenter is stored anywhere.
const fn graphemes() -> GraphemeClusterSegmenterBorrowed<'static> {
    GraphemeClusterSegmenter::new()
}

/// The word segmenter for the scripts that need no dictionary (see the module's documentation).
/// Not a `const fn` only because the options' `Default` is not one; it still just wraps static
/// data, at no cost.
fn words() -> WordSegmenterBorrowed<'static> {
    WordSegmenter::new_for_non_complex_scripts(WordBreakInvariantOptions::default())
}

/// The first grapheme boundary after `offset`: where the caret goes with →. The text's length
/// when `offset` is at (or past) the end.
#[must_use]
pub fn next_grapheme(text: &str, offset: usize) -> usize {
    // The iterator yields 0, then every boundary in increasing order, ending with the length.
    graphemes()
        .segment_str(text)
        .find(|&boundary| boundary > offset)
        .unwrap_or(text.len())
}

/// The last grapheme boundary before `offset`: where the caret goes with ←. Zero when `offset`
/// is at the start.
#[must_use]
pub fn prev_grapheme(text: &str, offset: usize) -> usize {
    graphemes()
        .segment_str(text)
        .take_while(|&boundary| boundary < offset)
        .last()
        .unwrap_or(0)
}

/// The grapheme boundary at or before `offset`: `offset` itself if it is a boundary.
#[must_use]
pub fn floor_grapheme(text: &str, offset: usize) -> usize {
    graphemes()
        .segment_str(text)
        .take_while(|&boundary| boundary <= offset)
        .last()
        .unwrap_or(0)
}

/// The grapheme boundary at or after `offset`: `offset` itself if it is a boundary, the text's
/// length past the end.
#[must_use]
pub fn ceil_grapheme(text: &str, offset: usize) -> usize {
    graphemes()
        .segment_str(text)
        .find(|&boundary| boundary >= offset)
        .unwrap_or(text.len())
}

/// `true` when `offset` is a grapheme boundary of `text` (0 and the length always are).
#[must_use]
pub fn is_grapheme_boundary(text: &str, offset: usize) -> bool {
    graphemes()
        .segment_str(text)
        .take_while(|&boundary| boundary <= offset)
        .last()
        == Some(offset)
}

/// The grapheme boundaries of `text` in increasing order, from 0 to the length included: the
/// stops a caret may take, for hit testing and for the accessibility node's character lengths.
pub fn grapheme_boundaries(text: &str) -> impl Iterator<Item = usize> + '_ {
    graphemes().segment_str(text)
}

/// The segments of `text` in order: each one's byte range and whether it is a **word** (letters
/// or numbers) or something else (spaces, punctuation, symbols). Consecutive ranges cover the
/// whole text. An empty text has none.
fn word_segments(text: &str) -> impl Iterator<Item = (Range<usize>, bool)> + '_ {
    // The iterator yields 0 first (no segment ends there), then each segment's end with the type
    // of the segment that ends there; the previous boundary is that segment's start.
    let mut start = 0;
    words()
        .segment_str(text)
        .iter_with_word_type()
        .filter(|&(end, _)| end > 0)
        .map(move |(end, kind)| {
            let range = start..end;
            start = end;
            (range, kind.is_word_like())
        })
}

/// The start of the word before `offset`: where Ctrl+← (⌥← on macOS) goes, and where the word
/// deleted by Ctrl+Backspace begins. Spaces and punctuation between are skipped, so from
/// `"one two|"` and from `"one two |"` it is the `t`. Zero when no word starts before.
#[must_use]
pub fn prev_word_start(text: &str, offset: usize) -> usize {
    word_segments(text)
        .filter(|(range, word)| *word && range.start < offset)
        .last()
        .map_or(0, |(range, _)| range.start)
}

/// The end of the next word after `offset`: where Ctrl+→ goes on Linux, ⌥→ on macOS. From inside
/// a word it is that word's end; from a space, the end of the word after it. The text's length
/// when no word ends after.
#[must_use]
pub fn next_word_end(text: &str, offset: usize) -> usize {
    word_segments(text)
        .find(|(range, word)| *word && range.end > offset)
        .map_or(text.len(), |(range, _)| range.end)
}

/// The start of the next word after `offset`: where Ctrl+→ goes on Windows, which skips the rest
/// of the word and the spaces after it. The text's length when no word starts after.
#[must_use]
pub fn next_word_start(text: &str, offset: usize) -> usize {
    word_segments(text)
        .find(|(range, word)| *word && range.start > offset)
        .map_or(text.len(), |(range, _)| range.start)
}

/// The segment that contains the grapheme starting at `offset`: the word a double click selects,
/// or the run of spaces or the punctuation when the click falls there. At (or past) the end, the
/// last segment; in an empty text, the empty range at 0.
#[must_use]
pub fn word_at(text: &str, offset: usize) -> Range<usize> {
    let mut last = 0..0;
    for (range, _) in word_segments(text) {
        if range.contains(&offset) {
            return range;
        }
        last = range;
    }
    last
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        reason = "test helpers: a failed unwrap fails the test"
    )]

    use super::*;

    /// Every boundary of a text with a combining accent, a flag and plain letters.
    #[test]
    fn grapheme_boundaries_keep_accents_and_flags_whole() {
        // "e" + U+0301 is one grapheme (3 bytes); the flag is two regional indicators (8 bytes).
        let text = "e\u{301}x\u{1F1EE}\u{1F1F9}y";
        let all: Vec<usize> = grapheme_boundaries(text).collect();
        assert_eq!(all, vec![0, 3, 4, 12, 13]);
        assert_eq!(next_grapheme(text, 0), 3);
        assert_eq!(next_grapheme(text, 4), 12);
        assert_eq!(next_grapheme(text, 13), 13);
        assert_eq!(prev_grapheme(text, 12), 4);
        assert_eq!(prev_grapheme(text, 0), 0);
        // Inside the accent's sequence, or inside the flag: the boundaries around.
        assert_eq!(floor_grapheme(text, 2), 0);
        assert_eq!(ceil_grapheme(text, 2), 3);
        assert_eq!(floor_grapheme(text, 8), 4);
        assert_eq!(ceil_grapheme(text, 8), 12);
        assert!(is_grapheme_boundary(text, 3));
        assert!(!is_grapheme_boundary(text, 1));
        // Past the end.
        assert_eq!(ceil_grapheme(text, 99), 13);
        assert_eq!(floor_grapheme(text, 99), 13);
    }

    /// The empty text has one boundary, 0.
    #[test]
    fn an_empty_text_has_only_the_start() {
        assert_eq!(grapheme_boundaries("").collect::<Vec<_>>(), vec![0]);
        assert_eq!(next_grapheme("", 0), 0);
        assert_eq!(prev_grapheme("", 0), 0);
        assert_eq!(word_at("", 0), 0..0);
        assert_eq!(prev_word_start("", 0), 0);
        assert_eq!(next_word_end("", 0), 0);
    }

    /// Word movement over words, spaces and punctuation.
    #[test]
    fn words_skip_spaces_and_punctuation() {
        // Segments (UAX #29): "Violin" 0–6, " " 6–7, "I" 7–8, "," 8–9, " " 9–10, "solo" 10–14:
        // the comma and the space after it are two segments.
        let text = "Violin I, solo";
        assert_eq!(next_word_end(text, 0), 6);
        assert_eq!(next_word_end(text, 6), 8);
        assert_eq!(next_word_end(text, 8), 14);
        assert_eq!(next_word_start(text, 0), 7);
        assert_eq!(next_word_start(text, 7), 10);
        assert_eq!(next_word_start(text, 10), 14);
        assert_eq!(prev_word_start(text, 14), 10);
        assert_eq!(prev_word_start(text, 10), 7);
        assert_eq!(prev_word_start(text, 9), 7);
        assert_eq!(prev_word_start(text, 3), 0);
        assert_eq!(word_at(text, 2), 0..6);
        assert_eq!(word_at(text, 6), 6..7);
        assert_eq!(word_at(text, 8), 8..9);
        assert_eq!(word_at(text, 9), 9..10);
        assert_eq!(word_at(text, 14), 10..14);
    }

    /// Accented letters belong to their word, and numbers are words.
    #[test]
    fn accented_words_and_numbers() {
        let text = "Crème brûlée 42";
        let end = "Crème".len();
        assert_eq!(next_word_end(text, 0), end);
        assert_eq!(word_at(text, 3), 0..end);
        assert_eq!(next_word_end(text, "Crème brûlée".len()), text.len());
    }
}
