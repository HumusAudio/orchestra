// SPDX-License-Identifier: GPL-3.0-or-later

//! Line breaking, case by case: every row of the table of ADR-0044 TX10, plus mandatory breaks
//! and the hanging of spaces.

#![allow(
    clippy::unwrap_used,
    reason = "test helpers: a failed unwrap fails the test"
)]

use orchestra_text::{FaceId, Faces, Shaper, TextLayout, TextStyle};

/// Inter Regular at 13 px.
const BODY: TextStyle = TextStyle::new(FaceId::InterRegular, 13.0);

/// Lays out `text` at `width` and returns the text of each line, with the layout.
fn lines(text: &str, width: Option<f32>) -> (Vec<String>, TextLayout) {
    let faces = Faces::bundled().unwrap();
    let mut layout = TextLayout::new();
    Shaper::new()
        .layout(&faces, text, BODY, width, &mut layout)
        .unwrap();
    let pieces = layout
        .lines()
        .iter()
        .map(|line| text.get(line.text.clone()).unwrap().to_owned())
        .collect();
    (pieces, layout)
}

/// The width of `text` on one line.
fn width_of(text: &str) -> f32 {
    lines(text, None).1.width()
}

#[test]
fn ordinary_spaces_are_break_opportunities() {
    // Just too narrow for the whole text: it breaks at the last space that fits.
    let text = "Save the project";
    let width = width_of("Save the") + 1.0;
    let (pieces, layout) = lines(text, Some(width));
    assert_eq!(pieces, ["Save the ", "project"]);
    // The space at the end of the first line hangs: it is not part of its width.
    assert!((layout.lines()[0].width - width_of("Save the")).abs() < 1e-3);
}

#[test]
fn no_break_spaces_never_break() {
    // French: a narrow no-break space before "?", no-break spaces inside the guillemets. Even
    // at a width where only one word fits, "?" and "»" never start a line.
    let text = "Mesure\u{202F}? «\u{A0}Allegro\u{A0}»";
    // The width fits the longer of the two pieces, but not both.
    let width = width_of("Mesure\u{202F}?").max(width_of("«\u{A0}Allegro\u{A0}»")) + 1.0;
    assert!(width < width_of(text));
    let (pieces, _) = lines(text, Some(width));
    assert_eq!(pieces, ["Mesure\u{202F}? ", "«\u{A0}Allegro\u{A0}»"]);
    // Everywhere: "120 BPM" and "4 bars" with a no-break space stay together.
    let text = "at 120\u{A0}BPM for 4\u{A0}bars";
    let (pieces, _) = lines(text, Some(width_of("at 120\u{A0}BPM") + 1.0));
    assert_eq!(pieces, ["at 120\u{A0}BPM ", "for 4\u{A0}bars"]);
}

#[test]
fn a_line_may_break_after_a_hyphen() {
    let text = "pre-roll";
    let (pieces, _) = lines(text, Some(width_of("pre-") + 1.0));
    assert_eq!(pieces, ["pre-", "roll"]);
}

#[test]
fn soft_hyphens_show_only_where_the_line_breaks() {
    let text = "Takt\u{AD}art\u{AD}wechsel";
    // On one line: no hyphen drawn, and the soft hyphens take no space.
    let (pieces, layout) = lines(text, None);
    assert_eq!(pieces.len(), 1);
    assert!(!layout.lines()[0].hyphenated);
    assert!((layout.width() - width_of("Taktartwechsel")).abs() < 1e-3);
    let one_line_glyphs = layout.glyphs().len();
    // Broken after "Taktart": a visible hyphen ends the first line, counted in its width.
    // Wide enough for "Taktart‐" (U+2010, the hyphen drawn) and for "wechsel", too narrow for the whole word.
    let width = width_of("Taktart\u{2010}").max(width_of("wechsel")) + 0.5;
    assert!(width < width_of("Taktartwechsel"));
    let (pieces, layout) = lines(text, Some(width));
    assert_eq!(pieces, ["Takt\u{AD}art\u{AD}", "wechsel"]);
    let first = &layout.lines()[0];
    assert!(first.hyphenated && !layout.lines()[1].hyphenated);
    assert!(first.width <= width);
    assert!(
        (first.width - width_of("Taktart\u{2010}")).abs() < 0.2,
        "{}",
        first.width
    );
    // One glyph more than on one line: the added hyphen, the last of the first line.
    assert_eq!(layout.glyphs().len(), one_line_glyphs + 1);
    let faces = Faces::bundled().unwrap();
    let hyphen = faces
        .glyph_for_char(FaceId::InterRegular, '\u{2010}')
        .unwrap();
    let last = layout.glyphs()[first.glyphs.end - 1];
    assert_eq!(last.glyph, hyphen);
    // Its cluster is the soft hyphen's byte offset ("Takt" 4 + U+00AD 2 + "art" 3 = 9).
    assert_eq!(last.cluster, 9);
}

#[test]
fn zero_width_spaces_are_invisible_break_opportunities() {
    let text = "path/\u{200B}to/\u{200B}file";
    let (pieces, layout) = lines(text, Some(width_of("path/to/") + 1.0));
    assert_eq!(pieces, ["path/\u{200B}to/\u{200B}", "file"]);
    // On one line it is as wide as without the invisible spaces.
    let (_, one) = lines(text, None);
    assert!((one.width() - width_of("path/to/file")).abs() < 1e-3);
    assert!(layout.width() <= width_of("path/to/") + 1.0);
}

#[test]
fn a_word_longer_than_the_line_is_cut_between_clusters() {
    let text = "Taktartwechsel";
    let width = 30.0;
    let (pieces, layout) = lines(text, Some(width));
    assert!(pieces.len() > 1);
    assert_eq!(pieces.concat(), text);
    for (piece, line) in pieces.iter().zip(layout.lines()) {
        assert!(line.width <= width, "{piece:?} is {} wide", line.width);
        // Each piece is as long as it can be: one more letter would not fit.
        let next = text.get(line.text.end..).unwrap().chars().next();
        if let Some(next) = next {
            assert!(width_of(&format!("{piece}{next}")) > width, "{piece:?}");
        }
    }
}

#[test]
fn a_letter_and_its_combining_mark_are_never_separated() {
    // Every "q" carries a combining acute accent. There is no precomposed "q" with an acute
    // (unlike "é", which the shaper composes into one glyph), so each cluster has two glyphs:
    // the letter and the mark. At a width of zero each cluster gets a line, and no line
    // separates a mark from its letter.
    let text = "q\u{301}q\u{301}q\u{301}";
    let (pieces, layout) = lines(text, Some(0.0));
    assert_eq!(pieces, ["q\u{301}"; 3]);
    assert!(layout.lines().iter().all(|line| line.glyphs.len() == 2));
}

#[test]
fn chinese_breaks_between_characters() {
    let text = "中文字符测试";
    let (pieces, _) = lines(text, Some(1.5 * width_of("中")));
    assert_eq!(pieces, ["中", "文", "字", "符", "测", "试"]);
}

#[test]
fn mandatory_breaks_end_lines_whatever_the_width() {
    // LF, CR LF (one break, after the LF), and a final LF, which leaves an empty last line.
    let (pieces, layout) = lines("one\ntwo\r\nthree\n", None);
    assert_eq!(pieces, ["one\n", "two\r\n", "three\n", ""]);
    // The line breaks are not drawn, and give no width.
    assert!((layout.lines()[1].width - width_of("two")).abs() < 1e-3);
    let drawn: usize = layout.lines().iter().map(|line| line.glyphs.len()).sum();
    assert_eq!(drawn, "onetwothree".len());
    // Unicode's line separator breaks too.
    let (pieces, _) = lines("a\u{2028}b", Some(1000.0));
    assert_eq!(pieces, ["a\u{2028}", "b"]);
}

#[test]
fn spaces_hang_past_the_width() {
    // A width smaller than one letter: each word on its line, its spaces with it.
    let (pieces, layout) = lines("a  b   ", Some(5.0));
    assert_eq!(pieces, ["a  ", "b   "]);
    assert!((layout.lines()[0].width - width_of("a")).abs() < 1e-3);
    assert!((layout.lines()[1].width - width_of("b")).abs() < 1e-3);
}

#[test]
fn lines_are_placed_one_line_height_apart() {
    let (_, layout) = lines("one\ntwo\nthree", None);
    let metrics = layout.line_metrics();
    for (row, line) in layout.lines().iter().enumerate() {
        let expected =
            metrics.ascent + f32::from(u16::try_from(row).unwrap()) * metrics.line_height();
        assert!((line.baseline - expected).abs() < 1e-3);
        // Every glyph of the line sits on its baseline (Latin letters have no vertical offset)
        // and starts again from the left edge.
        let glyphs = &layout.glyphs()[line.glyphs.clone()];
        assert!(glyphs.iter().all(|g| (g.y - line.baseline).abs() < 1e-4));
        assert!(glyphs.first().is_some_and(|g| g.x.abs() < 1e-4));
    }
    assert!((layout.height() - 3.0 * metrics.line_height()).abs() < 1e-3);
}

#[test]
fn reusing_a_layout_replaces_its_content() {
    let faces = Faces::bundled().unwrap();
    let mut shaper = Shaper::new();
    let mut layout = TextLayout::new();
    shaper
        .layout(
            &faces,
            "a long first text\nwith two lines",
            BODY,
            None,
            &mut layout,
        )
        .unwrap();
    shaper
        .layout(&faces, "short", BODY, None, &mut layout)
        .unwrap();
    assert_eq!(layout.lines().len(), 1);
    assert_eq!(layout.glyphs().len(), 5);
    assert!((layout.width() - width_of("short")).abs() < 1e-3);
}
