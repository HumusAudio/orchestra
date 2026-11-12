// SPDX-License-Identifier: GPL-3.0-or-later

//! Shaping (ADR-0044 TX3, TX4, TX9, TB7): the bundled faces, the six interface languages,
//! kerning, tabular figures, Fluent's isolation marks, missing characters, control characters,
//! clusters, and the refusal of bad inputs.

#![allow(
    clippy::unwrap_used,
    reason = "test helpers: a failed unwrap fails the test"
)]

use std::path::Path;

use orchestra_text::{FaceId, Faces, GlyphId, Shaper, TextError, TextLayout, TextStyle};

/// The faces, parsed once per test.
fn faces() -> Faces {
    Faces::bundled().unwrap()
}

/// Lays out `text` on one line (no width) in `style`.
fn lay_out(faces: &Faces, text: &str, style: TextStyle) -> TextLayout {
    let mut layout = TextLayout::new();
    Shaper::new()
        .layout(faces, text, style, None, &mut layout)
        .unwrap();
    layout
}

/// Inter Regular at 13 px, the body text.
const BODY: TextStyle = TextStyle::new(FaceId::InterRegular, 13.0);

#[test]
fn every_bundled_face_parses_with_its_units_per_em() {
    let faces = faces();
    // Inter is drawn on 2048 units, Bravura and JetBrains Mono on 1000 (read from the files'
    // `head` tables on 2026-11-12).
    let expected = [2048, 2048, 2048, 1000, 1000];
    for (face, units) in FaceId::ALL.into_iter().zip(expected) {
        assert_eq!(faces.units_per_em(face), units, "{face:?}");
        assert_eq!(FaceId::ALL[face.index()], face);
    }
}

/// Every character of every message of the six catalogues exists in the three Inter faces, so
/// no interface text can show a `.notdef` box (TX4 is a limit for user content only).
///
/// The catalogues are read from `crates/i18n/locales/` as plain text: this crate must not
/// depend on `orchestra-i18n` (it has no internal dependencies), and checking the characters of
/// the whole file is stricter than checking the formatted messages (comments and keys are
/// included, all ASCII anyway).
#[test]
fn every_catalogue_character_is_in_inter() {
    let faces = faces();
    let locales = Path::new(env!("CARGO_MANIFEST_DIR")).join("../i18n/locales");
    let mut checked = 0;
    for entry in std::fs::read_dir(&locales).unwrap() {
        let catalogue = entry.unwrap().path().join("main.ftl");
        let text = std::fs::read_to_string(&catalogue).unwrap();
        for c in text.chars().filter(|c| !c.is_control()) {
            for face in [
                FaceId::InterRegular,
                FaceId::InterSemiBold,
                FaceId::InterBold,
            ] {
                assert!(
                    faces.glyph_for_char(face, c).is_some(),
                    "{c:?} (U+{:04X}) of {} is missing in {face:?}",
                    u32::from(c),
                    catalogue.display()
                );
            }
            checked += 1;
        }
    }
    // Six catalogues of several hundred characters each: the walk really happened.
    assert!(checked > 1000, "only {checked} characters checked");
}

/// One sentence per interface language, with its accents and its typography, shapes without
/// a single `.notdef`.
#[test]
fn the_six_languages_shape_without_missing_glyphs() {
    let faces = faces();
    let sentences = [
        "Quarter note, measure, staff: 4 beats",
        "Crotchet, bar, stave: 4 beats",
        "Semiminima, battuta, pentagramma: l’accordo è più lungo",
        "Noire, mesure, portée\u{202F}: «\u{A0}Allegro\u{A0}» — déjà",
        "Viertelnote, Takt, Notensystem: „Größe“ übernehmen",
        "Negra, compás, pentagrama: ¿Añadir señal? ¡Sí!",
    ];
    for sentence in sentences {
        let layout = lay_out(&faces, sentence, BODY);
        assert!(
            layout.glyphs().iter().all(|g| g.glyph != GlyphId::NOTDEF),
            "{sentence:?} has a .notdef"
        );
        assert_eq!(layout.lines().len(), 1);
    }
}

/// Kerning is applied: "AV" is narrower than "A" and "V" set apart.
#[test]
fn kerning_brings_a_and_v_closer() {
    let faces = faces();
    let pair = lay_out(&faces, "AV", BODY).width();
    let apart = lay_out(&faces, "A", BODY).width() + lay_out(&faces, "V", BODY).width();
    assert!(pair < apart - 0.1, "AV {pair} against {apart}");
}

/// Tabular figures (`tnum`) give every digit the same advance; Inter's default figures are
/// proportional, so a one is narrower than a zero.
#[test]
fn tabular_figures_make_digits_equally_wide() {
    let faces = faces();
    let tabular = BODY.with_tabular_figures(true);
    let ones = lay_out(&faces, "1111", tabular).width();
    let zeros = lay_out(&faces, "0000", tabular).width();
    assert!(
        (ones - zeros).abs() < 1e-4,
        "tabular: {ones} against {zeros}"
    );
    let ones = lay_out(&faces, "1111", BODY).width();
    let zeros = lay_out(&faces, "0000", BODY).width();
    assert!(ones < zeros - 0.5, "proportional: {ones} against {zeros}");
}

/// Fluent wraps placeables in U+2068 … U+2069: they must neither show nor take space (TX9).
#[test]
fn isolation_marks_are_invisible_and_take_no_space() {
    let faces = faces();
    let plain = lay_out(&faces, "Bar 12 of 40", BODY);
    let isolated = lay_out(&faces, "Bar \u{2068}12\u{2069} of \u{2068}40\u{2069}", BODY);
    assert!((plain.width() - isolated.width()).abs() < 1e-4);
    assert!(isolated.glyphs().iter().all(|g| g.glyph != GlyphId::NOTDEF));
    // The digits keep their place: the glyph after "Bar " starts where it does without marks.
    let x_of = |layout: &TextLayout, glyph: GlyphId| {
        layout.glyphs().iter().find(|g| g.glyph == glyph).unwrap().x
    };
    let one = faces.glyph_for_char(FaceId::InterRegular, '1').unwrap();
    assert!((x_of(&plain, one) - x_of(&isolated, one)).abs() < 1e-4);
}

/// A character the face does not have becomes `.notdef`, with an advance (TX4).
#[test]
fn a_missing_character_is_notdef() {
    let faces = faces();
    let layout = lay_out(&faces, "a中b", BODY);
    let glyphs = layout.glyphs();
    assert_eq!(glyphs.len(), 3);
    assert_eq!(glyphs[1].glyph, GlyphId::NOTDEF);
    assert!(glyphs[2].x > glyphs[1].x, ".notdef has an advance");
    // Its cluster is the byte offset of the character (one byte after "a").
    assert_eq!(glyphs[1].cluster, 1);
    assert_eq!(glyphs[2].cluster, 4);
}

/// Control characters (a tab) are neither drawn nor advancing.
#[test]
fn control_characters_are_hidden() {
    let faces = faces();
    let tabbed = lay_out(&faces, "a\tb", BODY);
    let joined = lay_out(&faces, "ab", BODY);
    assert_eq!(tabbed.glyphs().len(), 2);
    assert!((tabbed.width() - joined.width()).abs() < 1e-4);
}

/// Clusters are byte offsets of the text, never decreasing (left to right), and a combining
/// accent stays in the cluster of its letter.
#[test]
fn clusters_are_byte_offsets_in_order() {
    let faces = faces();
    // "e" + U+0301 COMBINING ACUTE ACCENT: one cluster of three bytes.
    let text = "caf\u{65}\u{301} au lait";
    let layout = lay_out(&faces, text, BODY);
    let clusters: Vec<u32> = layout.glyphs().iter().map(|g| g.cluster).collect();
    assert!(clusters.windows(2).all(|w| w[0] <= w[1]), "{clusters:?}");
    assert!(clusters.iter().all(|&c| (c as usize) < text.len()));
    // No glyph starts inside the accented letter's cluster (bytes 4 and 5 are the accent).
    assert!(
        !clusters.contains(&4) && !clusters.contains(&5),
        "{clusters:?}"
    );
}

/// A right-to-left text is still laid out left to right (TX5), in logical order.
#[test]
fn right_to_left_text_is_laid_out_left_to_right() {
    let faces = faces();
    // Hebrew FIRST: the shaper guesses the direction from the first strong script, so a text
    // starting with Latin letters would be left to right anyway and prove nothing.
    let layout = lay_out(&faces, "שלום abc", BODY);
    let glyphs = layout.glyphs();
    assert_eq!(glyphs.len(), "שלום abc".chars().count());
    assert!(glyphs.windows(2).all(|w| w[0].x <= w[1].x));
    assert!(glyphs.windows(2).all(|w| w[0].cluster <= w[1].cluster));
}

/// The SMuFL symbols are found by code point in Bravura; a black notehead is 1.18 staff
/// spaces wide (Bravura's metadata), an em being four staff spaces: 0.295 em.
#[test]
fn smufl_glyphs_are_found_by_code_point() {
    let faces = faces();
    let notehead = faces.glyph_for_char(FaceId::Bravura, '\u{E0A4}').unwrap();
    let advance = faces.advance(FaceId::Bravura, notehead, 32.0).unwrap();
    assert!((advance - 0.295 * 32.0).abs() < 1e-3, "{advance}");
    // The G clef (U+E050) exists too.
    let clef = faces.glyph_for_char(FaceId::Bravura, '\u{E050}').unwrap();
    assert_ne!(clef, notehead);
    // Found on 2026-11-12: Inter maps U+E0A4 to a private glyph of its own (fonts may use the
    // Private Use Area freely). A SMuFL code point means a musical symbol only in Bravura, so
    // symbols are always looked up there, never in the text face.
    assert!(
        faces
            .glyph_for_char(FaceId::InterRegular, '\u{E0A4}')
            .is_some()
    );
}

#[test]
fn bad_sizes_and_widths_are_refused() {
    let faces = faces();
    let mut shaper = Shaper::new();
    let mut layout = TextLayout::new();
    for size in [0.0, -1.0, f32::NAN, f32::INFINITY] {
        let style = TextStyle::new(FaceId::InterRegular, size);
        assert_eq!(
            shaper.layout(&faces, "a", style, None, &mut layout),
            Err(TextError::InvalidSize),
            "{size}"
        );
    }
    for width in [-1.0, f32::NAN, f32::INFINITY] {
        assert_eq!(
            shaper.layout(&faces, "a", BODY, Some(width), &mut layout),
            Err(TextError::InvalidWidth),
            "{width}"
        );
    }
    // A zero width is valid: every cluster gets a line of its own.
    shaper
        .layout(&faces, "ab cd", BODY, Some(0.0), &mut layout)
        .unwrap();
    assert_eq!(layout.lines().len(), 4);
}

/// An empty text has one empty line, one line height tall, so a caret has a place (F2.10).
#[test]
fn an_empty_text_has_one_empty_line() {
    let faces = faces();
    let layout = lay_out(&faces, "", BODY);
    assert_eq!(layout.lines().len(), 1);
    assert_eq!(layout.glyphs(), &[]);
    assert_eq!(layout.width(), 0.0);
    let height = layout.line_metrics().line_height();
    assert!((layout.height() - height).abs() < 1e-5);
}

/// Every face shapes the same short text (its own glyphs, its own metrics).
#[test]
fn every_face_shapes() {
    let faces = faces();
    for face in FaceId::ALL {
        let layout = lay_out(&faces, "Ab1", TextStyle::new(face, 20.0));
        assert_eq!(layout.face(), face);
        assert_eq!(layout.size(), 20.0);
        assert!(layout.width() > 0.0, "{face:?}");
    }
}
