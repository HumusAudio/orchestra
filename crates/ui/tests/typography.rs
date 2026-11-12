// SPDX-License-Identifier: GPL-3.0-or-later

//! The text tokens of the themes (ADR-0044 TX11, choice TD1 of F2.07c): the default typography
//! of every built-in pair, the styles they resolve to, the weight fallback, custom overrides,
//! the size range, and the stable names.

use std::collections::HashSet;

use orchestra_text::FaceId;
use orchestra_ui::{
    Appearance, BuiltInTheme, CustomTheme, Family, FamilyToken, MAX_TEXT_SIZE, MIN_TEXT_SIZE,
    Override, SizeToken, Typography, Weight,
};

#[test]
fn every_built_in_theme_has_the_default_typography() {
    for &theme in BuiltInTheme::ALL {
        for appearance in [Appearance::Light, Appearance::Dark] {
            let instance = theme.pair().instance(appearance);
            assert_eq!(instance.typography(), Typography::DEFAULT, "{}", theme.id());
        }
    }
    let sizes: Vec<u8> = SizeToken::ALL
        .iter()
        .map(|&token| Typography::DEFAULT.size(token))
        .collect();
    assert_eq!(sizes, [11, 12, 13, 15, 18, 22]);
}

#[test]
fn styles_resolve_to_the_bundled_faces() {
    let theme = BuiltInTheme::Studio.pair().instance(Appearance::Dark);
    let body = theme.text_style(FamilyToken::Ui, SizeToken::Body, Weight::Regular);
    assert_eq!((body.face, body.size), (FaceId::InterRegular, 13.0));
    assert!(!body.features.tabular_figures);
    let heading = theme.text_style(FamilyToken::Ui, SizeToken::Heading, Weight::Bold);
    assert_eq!((heading.face, heading.size), (FaceId::InterBold, 18.0));
    let label = theme.text_style(FamilyToken::Ui, SizeToken::Small, Weight::SemiBold);
    assert_eq!(label.face, FaceId::InterSemiBold);
    // Numbers: Inter with tabular figures, in every weight.
    for weight in [Weight::Regular, Weight::SemiBold, Weight::Bold] {
        let number = theme.text_style(FamilyToken::Numeric, SizeToken::Caption, weight);
        assert!(number.features.tabular_figures);
        assert_eq!(number.size, 11.0);
    }
    // Faces with one weight fall back to it.
    for weight in [Weight::Regular, Weight::SemiBold, Weight::Bold] {
        let code = theme.text_style(FamilyToken::Code, SizeToken::Small, weight);
        assert_eq!(code.face, FaceId::JetBrainsMono);
        let music = theme.text_style(FamilyToken::Music, SizeToken::Title, weight);
        assert_eq!(music.face, FaceId::Bravura);
    }
}

#[test]
fn custom_themes_override_families_and_sizes() {
    let mut custom = CustomTheme::new(BuiltInTheme::Parchment);
    custom.light = vec![
        Override::Family(FamilyToken::Ui, Family::JetBrainsMono),
        Override::TextSize(SizeToken::Body, 15),
        Override::TextSize(SizeToken::Body, 14),
    ];
    custom.dark = vec![Override::Family(FamilyToken::Numeric, Family::Inter)];
    let resolved = custom.resolve();
    let light = resolved.light.typography();
    assert_eq!(light.family(FamilyToken::Ui), Family::JetBrainsMono);
    // The later override wins, as for colours.
    assert_eq!(light.size(SizeToken::Body), 14);
    assert_eq!(light.size(SizeToken::Title), 22);
    let dark = resolved.dark;
    let number = dark.text_style(FamilyToken::Numeric, SizeToken::Body, Weight::Regular);
    assert!(
        !number.features.tabular_figures,
        "proportional Inter chosen"
    );
    assert_eq!(dark.typography().family(FamilyToken::Ui), Family::Inter);
}

#[test]
fn sizes_are_kept_in_range() {
    let theme = *BuiltInTheme::Studio.pair().instance(Appearance::Light);
    let tiny = theme.with_text_size(SizeToken::Caption, 0);
    assert_eq!(tiny.typography().size(SizeToken::Caption), MIN_TEXT_SIZE);
    let huge = theme.with_text_size(SizeToken::Title, 250);
    assert_eq!(huge.typography().size(SizeToken::Title), MAX_TEXT_SIZE);
    let normal = theme.with_text_size(SizeToken::Body, 16);
    assert_eq!(normal.typography().size(SizeToken::Body), 16);
}

#[test]
fn names_are_unique_and_round_trip() {
    let mut seen = HashSet::new();
    for token in FamilyToken::ALL {
        assert!(seen.insert(token.name()));
        assert_eq!(FamilyToken::from_name(token.name()), Some(token));
    }
    for token in SizeToken::ALL {
        assert!(seen.insert(token.name()));
        assert_eq!(SizeToken::from_name(token.name()), Some(token));
    }
    for family in Family::ALL {
        assert_eq!(Family::from_name(family.name()), Some(family));
    }
    assert_eq!(FamilyToken::from_name("family-unknown"), None);
    assert_eq!(SizeToken::from_name("body"), None);
}

/// Every size token is set and read back on its own: no token writes another one's size.
#[test]
fn every_size_token_has_its_own_slot() {
    let mut theme = *BuiltInTheme::Studio.pair().instance(Appearance::Light);
    for (value, token) in (20_u8..).zip(SizeToken::ALL) {
        theme = theme.with_text_size(token, value);
    }
    for (value, token) in (20_u8..).zip(SizeToken::ALL) {
        assert_eq!(theme.typography().size(token), value, "{token:?}");
    }
    for (family, token) in Family::ALL.into_iter().zip(FamilyToken::ALL) {
        theme = theme.with_family(token, family);
    }
    for (family, token) in Family::ALL.into_iter().zip(FamilyToken::ALL) {
        assert_eq!(theme.typography().family(token), family, "{token:?}");
    }
}
