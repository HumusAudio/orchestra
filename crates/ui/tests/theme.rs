// SPDX-License-Identifier: GPL-3.0-or-later

//! The tests of ADR-0037 T5 for the theme system:
//!
//! 1. every instance of every built-in pair passes every contrast pair (all failures printed);
//! 2. every colour token is in a pair or in the exempt list, never both;
//! 3. the contrast formula agrees with an oracle written here from the WCAG definition, on known
//!    values and on random colours (property test);
//! 4. a custom theme resolves to its base plus its overrides.
//!
//! Plus the stable names and identifiers, the mode resolution (T9) and the defaults (T1).

use std::collections::HashSet;

use orchestra_ui::{
    Appearance, BuiltInTheme, CONTRAST_PAIRS, Color, ColorToken, ContrastKind, CustomTheme,
    EXEMPT_TOKENS, Override, ShapeToken, ThemeMode, contrast_ratio,
};
use proptest::prelude::*;

/// Both halves of every built-in pair, with a label for failure messages.
fn every_instance() -> impl Iterator<Item = (String, orchestra_ui::Theme)> {
    BuiltInTheme::ALL.iter().flat_map(|&theme| {
        let pair = theme.pair();
        [
            (format!("{} light", theme.id()), pair.light),
            (format!("{} dark", theme.id()), pair.dark),
        ]
    })
}

/// T5, first point: the twenty built-in instances meet WCAG 2.2 AA on every pair. All failures are
/// collected before failing, so one run shows everything that needs fixing.
#[test]
fn every_builtin_instance_passes_every_pair() {
    let mut failures = Vec::new();
    for (label, theme) in every_instance() {
        for failure in theme.contrast_failures() {
            failures.push(format!(
                "{label}: {} on {} = {:.3}, needs {}",
                failure.pair.foreground.name(),
                failure.pair.background.name(),
                failure.ratio,
                failure.pair.kind.minimum()
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "contrast failures:\n{}",
        failures.join("\n")
    );
}

/// There are ten pairs, so twenty instances, and 31 pairs (ADR-0037 T1, T4): a change to either
/// number is a decision, not an accident.
#[test]
fn counts_match_the_adr() {
    assert_eq!(BuiltInTheme::ALL.len(), 10);
    assert_eq!(CONTRAST_PAIRS.len(), 31);
    assert_eq!(ColorToken::ALL.len(), 24);
    assert_eq!(ShapeToken::ALL.len(), 5);
}

/// T5, second point: no token can be forgotten. Every colour token is a foreground or a background
/// of some pair, or exempt; an exempt token is in no pair (exempt means exempt).
#[test]
fn every_token_is_covered_exactly_once() {
    let in_pairs: HashSet<ColorToken> = CONTRAST_PAIRS
        .iter()
        .flat_map(|pair| [pair.foreground, pair.background])
        .collect();
    let exempt: HashSet<ColorToken> = EXEMPT_TOKENS.iter().copied().collect();
    for &token in ColorToken::ALL {
        assert!(
            in_pairs.contains(&token) ^ exempt.contains(&token),
            "{} must be in a pair or exempt, not both and not neither",
            token.name()
        );
    }
}

/// No pair is listed twice, and no pair compares a token with itself.
#[test]
fn pairs_are_distinct() {
    let mut seen = HashSet::new();
    for pair in CONTRAST_PAIRS {
        assert_ne!(pair.foreground, pair.background);
        assert!(
            seen.insert((pair.foreground, pair.background)),
            "duplicate pair {pair:?}"
        );
    }
}

/// Backgrounds of pairs are opaque in every built-in instance, as `Color::over` and
/// `contrast_ratio` assume; in fact every built-in colour is opaque except the shadow.
#[test]
fn builtin_colours_are_opaque_except_the_shadow() {
    for (label, theme) in every_instance() {
        for &token in ColorToken::ALL {
            if token != ColorToken::Shadow {
                assert!(
                    theme.color(token).is_opaque(),
                    "{label}: {} is translucent",
                    token.name()
                );
            }
        }
    }
}

/// The shapes of ADR-0037 T1 and T3: corners and lines per pair, a 2 px focus ring everywhere,
/// no shadow on the pairs built on lines, the same shape in both halves.
#[test]
fn shapes_match_the_adr() {
    let expected = [
        (BuiltInTheme::Absolute, 0, 1, false),
        (BuiltInTheme::Graphite, 2, 1, true),
        (BuiltInTheme::Signal, 0, 2, false),
        (BuiltInTheme::Parchment, 4, 1, true),
        (BuiltInTheme::Slate, 6, 1, true),
        (BuiltInTheme::Studio, 3, 1, true),
        (BuiltInTheme::Forest, 8, 1, true),
        (BuiltInTheme::Lagoon, 4, 1, true),
        (BuiltInTheme::Nocturne, 10, 1, true),
        (BuiltInTheme::Snow, 12, 2, true),
    ];
    for (theme, radius, line, shadow) in expected {
        let pair = theme.pair();
        assert_eq!(pair.light.shape(), pair.dark.shape(), "{}", theme.id());
        let shape = pair.light.shape();
        assert_eq!(
            (shape.corner_radius, shape.line_width, shape.focus_width),
            (radius, line, 2)
        );
        assert_eq!(shape.shadow_blur > 0, shadow, "{}", theme.id());
        for instance in [pair.light, pair.dark] {
            assert_eq!(
                instance.color(ColorToken::Shadow).a > 0,
                shadow,
                "{}",
                theme.id()
            );
        }
    }
}

/// Stable names and identifiers are unique, kebab case, and round-trip through `from_*`.
#[test]
fn names_and_ids_are_stable_keys() {
    let kebab = |s: &str| {
        !s.is_empty()
            && s.bytes().all(|b| b.is_ascii_lowercase() || b == b'-')
            && !s.starts_with('-')
            && !s.ends_with('-')
    };
    let mut seen = HashSet::new();
    for &token in ColorToken::ALL {
        assert!(
            kebab(token.name()) && seen.insert(token.name()),
            "{}",
            token.name()
        );
        assert_eq!(ColorToken::from_name(token.name()), Some(token));
    }
    for &token in ShapeToken::ALL {
        assert!(
            kebab(token.name()) && seen.insert(token.name()),
            "{}",
            token.name()
        );
        assert_eq!(ShapeToken::from_name(token.name()), Some(token));
    }
    let mut ids = HashSet::new();
    for &theme in BuiltInTheme::ALL {
        assert!(
            kebab(theme.id()) && ids.insert(theme.id()),
            "{}",
            theme.id()
        );
        assert_eq!(BuiltInTheme::from_id(theme.id()), Some(theme));
    }
    assert_eq!(ColorToken::from_name("Surface"), None);
    assert_eq!(BuiltInTheme::from_id("light"), None);
}

/// ADR-0037 T1: Studio and *System* are the defaults.
#[test]
fn defaults() {
    assert_eq!(BuiltInTheme::default(), BuiltInTheme::Studio);
    assert_eq!(ThemeMode::default(), ThemeMode::System);
}

/// ADR-0014 §1 and ADR-0037 T9: the forced modes ignore the system; *System* follows it, and
/// falls back to dark when the system says nothing.
#[test]
fn mode_resolution() {
    use Appearance::{Dark, Light};
    let systems = [None, Some(Light), Some(Dark)];
    for system in systems {
        assert_eq!(ThemeMode::Light.resolve(system), Light);
        assert_eq!(ThemeMode::Dark.resolve(system), Dark);
    }
    assert_eq!(ThemeMode::System.resolve(Some(Light)), Light);
    assert_eq!(ThemeMode::System.resolve(Some(Dark)), Dark);
    assert_eq!(ThemeMode::System.resolve(None), Dark);
    let pair = BuiltInTheme::Studio.pair();
    assert_eq!(*pair.instance(Light), pair.light);
    assert_eq!(*pair.instance(Dark), pair.dark);
}

/// T5, fourth point: with no overrides a custom theme is its base exactly; overrides change only
/// their tokens, in their half; a later override of the same token wins.
#[test]
fn custom_theme_resolution() {
    for &base in BuiltInTheme::ALL {
        assert_eq!(CustomTheme::new(base).resolve(), *base.pair());
    }

    let red = Color::rgb(0xff_0000);
    let blue = Color::rgb(0x00_00ff);
    let mut custom = CustomTheme::new(BuiltInTheme::Slate);
    custom.light = vec![
        Override::Color(ColorToken::Accent, red),
        Override::Shape(ShapeToken::CornerRadius, 0),
        Override::Color(ColorToken::Accent, blue),
    ];
    custom.dark = vec![Override::Shape(ShapeToken::LineWidth, 3)];
    assert_eq!(custom.overrides(Appearance::Light).len(), 3);

    let base = BuiltInTheme::Slate.pair();
    let resolved = custom.resolve();
    // Later override wins.
    assert_eq!(resolved.light.color(ColorToken::Accent), blue);
    assert_eq!(resolved.light.shape().corner_radius, 0);
    // Everything else is the base, in both halves.
    for &token in ColorToken::ALL {
        if token != ColorToken::Accent {
            assert_eq!(
                resolved.light.color(token),
                base.light.color(token),
                "{}",
                token.name()
            );
        }
        assert_eq!(
            resolved.dark.color(token),
            base.dark.color(token),
            "{}",
            token.name()
        );
    }
    for &token in ShapeToken::ALL {
        if token != ShapeToken::CornerRadius {
            assert_eq!(
                resolved.light.shape().get(token),
                base.light.shape().get(token)
            );
        }
        if token != ShapeToken::LineWidth {
            assert_eq!(
                resolved.dark.shape().get(token),
                base.dark.shape().get(token)
            );
        }
    }
    assert_eq!(resolved.dark.shape().line_width, 3);
}

/// ADR-0037 T8: a custom theme below the thresholds still resolves; the failures are reported,
/// not refused.
#[test]
fn custom_theme_below_wcag_is_reported_not_refused() {
    let mut custom = CustomTheme::new(BuiltInTheme::Absolute);
    custom.light = vec![Override::Color(ColorToken::Text, Color::rgb(0xee_eeee))];
    let failures: Vec<_> = custom.resolve().light.contrast_failures().collect();
    assert_ne!(failures.len(), 0, "a too-light text must be reported");
    assert!(
        failures
            .iter()
            .all(|f| f.pair.foreground == ColorToken::Text)
    );
    assert!(
        failures
            .iter()
            .all(|f| f.ratio < ContrastKind::Text.minimum())
    );
}

/// The check compares with the exact threshold: on white, `#767676` (≈ 4.54:1) passes as normal
/// text and `#777777` (≈ 4.48:1) fails, though both would print as "4.5" with one decimal.
#[test]
fn the_check_is_exact_at_the_threshold() {
    let with_text = |hex| {
        let mut custom = CustomTheme::new(BuiltInTheme::Absolute);
        custom.light = vec![Override::Color(ColorToken::Text, Color::rgb(hex))];
        let theme = custom.resolve().light;
        theme.contrast_failures().any(|f| {
            f.pair.foreground == ColorToken::Text && f.pair.background == ColorToken::Surface
        })
    };
    assert!(!with_text(0x76_7676), "#767676 on white must pass");
    assert!(with_text(0x77_7777), "#777777 on white must fail");
}

// ---- The oracle -------------------------------------------------------------------------------

/// The WCAG 2.x relative luminance, written here from the definition and sharing no code with the
/// library: the channel is decoded with the 0.03928 threshold of the original WCAG 2.0 text (the
/// library uses the corrected 0.04045 of IEC 61966-2-1; no 8-bit value lies between the two, so
/// both must give the same result).
fn oracle_luminance(r: u8, g: u8, b: u8) -> f64 {
    let decode = |v: u8| {
        let s = f64::from(v) / 255.0;
        if s <= 0.039_28 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * decode(r) + 0.7152 * decode(g) + 0.0722 * decode(b)
}

/// The contrast ratio from the definition, with compositing of a translucent foreground done in
/// floating point (the library rounds to 8 bits, so the two may differ by the rounding only).
fn oracle_ratio(fg: Color, bg: Color) -> f64 {
    let a = f64::from(fg.a) / 255.0;
    let mix = |f: u8, b: u8| (f64::from(f) * a + f64::from(b) * (1.0 - a)).round();
    let channel = |v: f64| {
        // Back to a byte: `v` is in 0.0..=255.0 after rounding, so the comparison chain picks it.
        (0..=255_u8).find(|&x| f64::from(x) >= v).unwrap_or(255)
    };
    let l1 = oracle_luminance(
        channel(mix(fg.r, bg.r)),
        channel(mix(fg.g, bg.g)),
        channel(mix(fg.b, bg.b)),
    );
    let l2 = oracle_luminance(bg.r, bg.g, bg.b);
    let (hi, lo) = if l1 > l2 { (l1, l2) } else { (l2, l1) };
    (hi + 0.05) / (lo + 0.05)
}

/// Known values: black on white is 21:1 (both orders), a colour on itself is 1:1, and two
/// published reference pairs.
#[test]
fn contrast_known_values() {
    let close = |a: f64, b: f64| (a - b).abs() < 1e-9;
    assert!(close(contrast_ratio(Color::BLACK, Color::WHITE), 21.0));
    assert!(close(contrast_ratio(Color::WHITE, Color::BLACK), 21.0));
    for v in [0x00_0000, 0x76_7676, 0xff_ffff] {
        assert!(close(contrast_ratio(Color::rgb(v), Color::rgb(v)), 1.0));
    }
    // #767676 on white is the classic "lightest grey that passes AA", ≈ 4.54:1.
    let grey = contrast_ratio(Color::rgb(0x76_7676), Color::WHITE);
    assert!((grey - 4.54).abs() < 0.005 && grey >= 4.5, "{grey}");
    // #777777 on white is the first that fails, ≈ 4.48:1.
    assert!(contrast_ratio(Color::rgb(0x77_7777), Color::WHITE) < 4.5);
    // A fully transparent foreground is the background itself.
    assert!(close(
        contrast_ratio(Color::rgba(0x0000_0000), Color::WHITE),
        1.0
    ));
}

proptest! {
    /// T5, third point: on random opaque and translucent colours the library agrees with the
    /// oracle, and the ratio is symmetric for opaque colours and always within 1..=21.
    #[test]
    fn contrast_agrees_with_the_oracle(fg in any::<u32>(), bg in any::<u32>()) {
        let fg = Color::rgba(fg);
        let bg = Color::rgb(bg);
        let ratio = contrast_ratio(fg, bg);
        prop_assert!((ratio - oracle_ratio(fg, bg)).abs() < 1e-9, "{ratio} vs oracle");
        prop_assert!((1.0..=21.0 + 1e-9).contains(&ratio));
        let opaque = Color { a: 255, ..fg };
        prop_assert!((contrast_ratio(opaque, bg) - contrast_ratio(bg, opaque)).abs() < 1e-12);
    }
}
