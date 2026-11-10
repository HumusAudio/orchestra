// SPDX-License-Identifier: GPL-3.0-or-later

//! Reading a theme never allocates (ADR-0037 T6): widgets read tokens at every draw, and the
//! theme editor checks the contrast at every edit, so neither may touch the heap. Resolving a
//! custom theme is included: its overrides are already in memory and the result is returned by
//! value.
//!
//! This file is its own test binary with the counting allocator installed (ADR-0016 §5); inputs
//! go through `black_box` so that nothing is computed at compile time.

use std::hint::black_box;

use orchestra_test_support::assert_no_allocations;
use orchestra_ui::{
    Appearance, BuiltInTheme, ColorToken, CustomTheme, Override, ShapeToken, ThemeMode,
};

orchestra_test_support::install_counting_allocator!();

#[test]
fn reading_and_checking_a_theme_never_allocates() {
    // Built before the measured closure: building the override list allocates, resolving does not.
    let mut custom = CustomTheme::new(BuiltInTheme::Forest);
    custom.dark = vec![Override::Shape(ShapeToken::CornerRadius, 0)];
    let custom = black_box(custom);

    assert_no_allocations(|| {
        let mut sum = 0_u64;
        for &theme in black_box(BuiltInTheme::ALL) {
            let appearance = ThemeMode::System.resolve(black_box(Some(Appearance::Light)));
            let instance = theme.pair().instance(appearance);
            for &token in ColorToken::ALL {
                sum += u64::from(instance.color(token).r);
            }
            sum += u64::from(instance.shape().corner_radius);
            sum += instance.contrast_failures().count() as u64;
        }
        let resolved = custom.resolve();
        sum += u64::from(resolved.dark.shape().corner_radius);
        black_box(sum);
    });
}
