// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of the screenshot catalogue (ADR-0041 S5). They run on Linux, Windows and macOS, in
//! memory, with no display: every scene must render on every platform, even though the guides'
//! files are generated on Linux only.
//!
//! No pixel is compared with a reference image: the scenes are still the throw-away demonstration
//! window (ADR-0041 S5), and `tiny-skia` may round anti-aliased edges differently on x86 and ARM.
//! What is checked is what every screenshot must satisfy whatever it shows.

#![allow(
    clippy::unwrap_used,
    reason = "test code: a failed unwrap fails the test with its message"
)]

use std::collections::HashSet;

use orchestra_app::{SCALE, SCENES, Scene, ScreenshotError, render, theme};
use orchestra_ui::{Appearance, BuiltInTheme, ColorToken, CpuRenderer, Rect};

/// Names are unique (one file each) and in kebab case (lower-case ASCII letters, digits and
/// single hyphens), so they are valid file names on every platform and case-insensitive file
/// systems cannot merge two of them.
#[test]
fn scene_names_are_unique_kebab_case_file_names() {
    let mut seen = HashSet::new();
    for scene in SCENES {
        let name = scene.name;
        assert!(seen.insert(name), "duplicate scene name `{name}`");
        assert!(!name.is_empty(), "empty scene name");
        assert!(
            name.chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
            "`{name}` is not kebab case"
        );
        assert!(
            !name.starts_with('-') && !name.ends_with('-') && !name.contains("--"),
            "`{name}` has a stray hyphen"
        );
    }
}

/// Every scene renders at its logical size times the scale, is fully opaque (the guides' pages
/// must not show through), encodes as PNG, and renders identically twice in a row (nothing in a
/// scene depends on the clock, the system or a previous scene).
#[test]
fn every_scene_renders_opaque_and_deterministic() {
    let mut renderer = CpuRenderer::new(orchestra_text::Faces::bundled().unwrap());
    for scene in SCENES {
        let image = render(scene, &mut renderer).unwrap();
        let (width, height) = scene.device_size().unwrap();
        assert_eq!(
            (image.width(), image.height()),
            (width, height),
            "{}",
            scene.name
        );
        assert!(
            image
                .data()
                .as_chunks::<4>()
                .0
                .iter()
                .all(|pixel| pixel[3] == 255),
            "scene `{}` has translucent pixels",
            scene.name
        );
        // The PNG signature: the encoder really produced a PNG file.
        assert_eq!(
            image.encode_png().unwrap().get(..8),
            Some(&b"\x89PNG\r\n\x1a\n"[..])
        );
        // A fresh renderer, so leftover scratch state from the first render cannot hide a
        // difference.
        let again = render(
            scene,
            &mut CpuRenderer::new(orchestra_text::Faces::bundled().unwrap()),
        )
        .unwrap();
        assert!(
            image == again,
            "scene `{}` is not deterministic",
            scene.name
        );
    }
}

/// The screenshots use Studio's dark half, whatever the system says (ADR-0041 S3): the corner
/// pixel of every scene is the dark surface.
#[test]
fn screenshots_use_studio_dark() {
    let dark = *BuiltInTheme::Studio.pair().instance(Appearance::Dark);
    assert_eq!(theme(), dark);
    let surface = dark.color(ColorToken::Surface);
    for scene in SCENES {
        let image = render(
            scene,
            &mut CpuRenderer::new(orchestra_text::Faces::bundled().unwrap()),
        )
        .unwrap();
        assert_eq!(
            image.pixel(0, 0).unwrap(),
            [surface.r, surface.g, surface.b, 255],
            "scene `{}`",
            scene.name
        );
    }
}

/// A bad catalogue entry is a typed error, not a panic or a giant allocation.
#[test]
fn bad_sizes_are_rejected() {
    /// Records nothing.
    fn nothing(_: Rect, _: &mut orchestra_ui::DisplayList, _: &orchestra_app::SceneContext) {}
    for (width, height) in [(0.0, 10.0), (10.0, -1.0), (f32::NAN, 10.0), (1e9, 10.0)] {
        let scene = Scene {
            name: "bad",
            width,
            height,
            record: nothing,
        };
        let error = render(
            &scene,
            &mut CpuRenderer::new(orchestra_text::Faces::bundled().unwrap()),
        )
        .unwrap_err();
        assert!(matches!(error, ScreenshotError::Size { scene: "bad", .. }));
        assert!(error.to_string().contains("bad"));
    }
    // The scale the module documents.
    assert!((SCALE - 2.0).abs() < f32::EPSILON);
}

/// The demonstration window shows its title (F2.07c): in the screenshot, the area right of the
/// three controls holds light text pixels (the theme is dark) and nothing else is there.
#[test]
fn the_demo_window_shows_its_title() {
    let scene = SCENES
        .iter()
        .find(|scene| scene.name == "demo-window")
        .unwrap();
    let image = render(
        scene,
        &mut CpuRenderer::new(orchestra_text::Faces::bundled().unwrap()),
    )
    .unwrap();
    // The title's area, logical x 384–520, y 44–76 (MARGIN + 360, the controls' row), at
    // scale 2: count the pixels much lighter than the raised panel around them.
    let panel = theme().color(ColorToken::SurfaceRaised);
    let mut light = 0;
    for y in 88..152 {
        for x in 768..1040 {
            let [r, g, b, _] = image.pixel(x, y).unwrap();
            if u16::from(r) + u16::from(g) + u16::from(b)
                > u16::from(panel.r) + u16::from(panel.g) + u16::from(panel.b) + 300
            {
                light += 1;
            }
        }
    }
    assert!(light > 400, "only {light} light pixels where the title is");
}
