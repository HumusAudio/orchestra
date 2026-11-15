// SPDX-License-Identifier: GPL-3.0-or-later

//! Texts shortened with an ellipsis (step F2.09f; ADR-0045 WJ1–WJ7): a label, a check box, a
//! switch and a button that get less width than their text needs draw its start and an
//! ellipsis, never past their rectangle; the whole text comes back when there is room again;
//! assistive technologies always get the whole text; a wrapping label wraps instead.
//!
//! # How drawing is checked
//! The frame is rendered and its pixels are read: "ink" is any pixel that differs from the
//! background around it. That is the only honest check that nothing is drawn past a rectangle;
//! what exactly is drawn (the longest start that fits, the ellipsis glyph) is checked glyph for
//! glyph in `orchestra-text`'s own tests of `Shaper::elide`.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test helpers: a failed unwrap or index fails the test"
)]
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "pixel coordinates of small, positive test rectangles"
)]

use accesskit::NodeId;
use orchestra_text::{FaceId, Faces, Shaper, TextLayout, TextStyle};
use orchestra_ui::{
    Appearance, BuiltInTheme, Button, ColorToken, CpuRenderer, DisplayList, Flex, Label,
    LayoutProps, Length, Rect, Space, Theme, Toggle, Ui,
};

/// These tests emit one message kind, never read.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Msg {
    /// A button was pressed.
    Pressed,
    /// A toggle changed.
    Changed(bool),
}

/// Studio light.
fn theme() -> Theme {
    *BuiltInTheme::Studio.pair().instance(Appearance::Light)
}

/// The window: 300 × 100.
const BOUNDS: Rect = Rect::new(0.0, 0.0, 300.0, 100.0);
/// The window's width in pixels.
const WIDTH: u32 = 300;
/// The window's height in pixels.
const HEIGHT: u32 = 100;

/// A text far wider than the widths these tests give it (about 230 px in the body style).
const LONG: &str = "Metronome during the count-in bars";

/// An interface whose root is a row without spacing or padding.
fn row() -> Ui<Msg> {
    Ui::new(
        Flex::row(Space::None, Space::None),
        &theme(),
        Faces::bundled().unwrap(),
    )
}

/// The props of a child `width` pixels wide.
fn fixed(width: f32) -> LayoutProps {
    LayoutProps::default().width(Length::Fixed(width))
}

/// The rendered frame, RGBA.
fn frame(list: &DisplayList) -> Vec<u8> {
    CpuRenderer::new(Faces::bundled().unwrap())
        .render_to_image(list, &theme(), WIDTH, HEIGHT, 1.0)
        .unwrap()
        .data()
        .to_vec()
}

/// The pixel at (`x`, `y`) of a frame.
fn pixel(frame: &[u8], x: u32, y: u32) -> [u8; 4] {
    let at = ((y * WIDTH + x) * 4) as usize;
    [frame[at], frame[at + 1], frame[at + 2], frame[at + 3]]
}

/// The opaque pixel of a token.
fn rgba(token: ColorToken) -> [u8; 4] {
    let c = theme().color(token);
    [c.r, c.g, c.b, 255]
}

/// The columns (x) of the band `y0..y1` holding at least one pixel that is not `background`,
/// among the columns `x0..x1`.
fn ink_columns(frame: &[u8], background: [u8; 4], x0: u32, x1: u32, y0: u32, y1: u32) -> Vec<u32> {
    (x0..x1)
        .filter(|&x| (y0..y1).any(|y| pixel(frame, x, y) != background))
        .collect()
}

/// The natural width of `text` in the body style (Inter Regular 13 px, the labels' default).
fn natural_width(text: &str) -> f32 {
    let mut layout = TextLayout::new();
    Shaper::new()
        .layout(
            &Faces::bundled().unwrap(),
            text,
            TextStyle::new(FaceId::InterRegular, 13.0),
            None,
            &mut layout,
        )
        .unwrap();
    layout.width()
}

#[test]
fn a_label_narrower_than_its_text_ends_with_an_ellipsis_inside_its_rectangle() {
    let mut ui = row();
    let root = ui.root();
    let label = ui.add(root, Label::new(LONG), fixed(90.0)).unwrap();
    let mut list = DisplayList::new();
    ui.layout(BOUNDS, &mut list);
    let rect = ui.rect(label).unwrap();
    assert!((rect.width() - 90.0).abs() < f32::EPSILON);
    assert!(ui.widget::<Label>(label).unwrap().is_elided());
    let frame = frame(&list);
    let surface = rgba(ColorToken::Surface);
    let ink = ink_columns(&frame, surface, 0, WIDTH, 0, HEIGHT);
    // Something is drawn, and it uses most of the 90 pixels...
    assert!(ink.len() > 30, "ink {ink:?}");
    assert!(*ink.last().unwrap() >= 70, "ink {ink:?}");
    // ...but nothing past the rectangle (before this step the whole text went on to x ≈ 230).
    assert!(*ink.last().unwrap() < rect.x1 as u32, "ink {ink:?}");
}

#[test]
fn a_label_that_fits_is_drawn_whole() {
    let mut ui = row();
    let root = ui.root();
    let label = ui.add(root, Label::new("Volume"), fixed(90.0)).unwrap();
    let free = ui
        .add(
            root,
            Label::new("Reverb send level"),
            LayoutProps::default(),
        )
        .unwrap();
    let mut list = DisplayList::new();
    ui.layout(BOUNDS, &mut list);
    assert!(!ui.widget::<Label>(label).unwrap().is_elided());
    // A label sized by its content has its whole text's width (it still fits the window).
    assert!(!ui.widget::<Label>(free).unwrap().is_elided());
    assert!((ui.rect(free).unwrap().width() - natural_width("Reverb send level")).abs() < 0.01);
}

#[test]
fn the_whole_text_comes_back_when_there_is_room_again() {
    // A label sized by its content in a window too narrow for it, then in a wide one: what a
    // splitter's pane does to its labels while the divider moves.
    let mut ui = row();
    let root = ui.root();
    let label = ui
        .add(root, Label::new(LONG), LayoutProps::default())
        .unwrap();
    let mut list = DisplayList::new();
    let surface = rgba(ColorToken::Surface);
    let mut last_ink = Vec::new();
    for width in [300.0, 120.0, 60.0, 20.0, 60.0, 120.0, 300.0] {
        ui.layout(Rect::new(0.0, 0.0, width, 100.0), &mut list);
        let rect = ui.rect(label).unwrap();
        let elided = ui.widget::<Label>(label).unwrap().is_elided();
        assert_eq!(elided, width < natural_width(LONG), "width {width}");
        let frame = frame(&list);
        let ink = ink_columns(&frame, surface, 0, WIDTH, 0, HEIGHT);
        assert_ne!(ink, [], "width {width}");
        assert!(
            *ink.last().unwrap() < rect.x1.ceil() as u32,
            "width {width}: {ink:?}"
        );
        last_ink.push(*ink.last().unwrap());
    }
    // The same width draws the same thing on the way down and on the way back.
    assert_eq!(last_ink[1], last_ink[5]);
    assert_eq!(last_ink[2], last_ink[4]);
    assert_eq!(last_ink[0], last_ink[6]);
    // Narrower means shorter.
    assert!(last_ink[3] < last_ink[2] && last_ink[2] < last_ink[1] && last_ink[1] < last_ink[0]);
}

#[test]
fn a_tiny_width_leaves_the_ellipsis_alone_and_then_nothing() {
    let ellipsis = natural_width("\u{2026}");
    let surface = rgba(ColorToken::Surface);
    // Room for the ellipsis but not for a letter before it: only the three dots, which are low
    // on the line (no ink in the upper half of the label's band).
    let mut ui = row();
    let root = ui.root();
    let label = ui
        .add(root, Label::new("Wide text"), fixed(ellipsis + 1.0))
        .unwrap();
    let mut list = DisplayList::new();
    ui.layout(BOUNDS, &mut list);
    assert!(ui.widget::<Label>(label).unwrap().is_elided());
    let rect = ui.rect(label).unwrap();
    let image = frame(&list);
    let middle = (rect.y0 + rect.height() / 2.0) as u32;
    assert_eq!(ink_columns(&image, surface, 0, WIDTH, 0, middle), []);
    assert_ne!(ink_columns(&image, surface, 0, WIDTH, middle, HEIGHT), []);
    // No room for the ellipsis either: nothing at all is drawn (WJ5).
    let mut ui = row();
    let root = ui.root();
    let label = ui
        .add(root, Label::new("Wide text"), fixed(ellipsis - 1.0))
        .unwrap();
    let mut list = DisplayList::new();
    ui.layout(BOUNDS, &mut list);
    assert!(ui.widget::<Label>(label).unwrap().is_elided());
    assert_eq!(ink_columns(&frame(&list), surface, 0, WIDTH, 0, HEIGHT), []);
    // The label keeps its line's height, so nothing below it moves.
    assert!(ui.rect(label).unwrap().height() > 10.0);
}

#[test]
fn a_new_text_is_fitted_again_at_the_same_width() {
    let mut ui = row();
    let root = ui.root();
    let label = ui.add(root, Label::new(LONG), fixed(90.0)).unwrap();
    let mut list = DisplayList::new();
    ui.layout(BOUNDS, &mut list);
    assert!(ui.widget::<Label>(label).unwrap().is_elided());
    let surface = rgba(ColorToken::Surface);
    let before = ink_columns(&frame(&list), surface, 0, WIDTH, 0, HEIGHT);
    // A short text: whole.
    ui.widget_mut::<Label>(label).unwrap().set_text("Solo");
    ui.request_layout(label);
    ui.layout(BOUNDS, &mut list);
    assert!(!ui.widget::<Label>(label).unwrap().is_elided());
    let short = ink_columns(&frame(&list), surface, 0, WIDTH, 0, HEIGHT);
    assert!(short.last().unwrap() < before.last().unwrap());
    // Another long text, same rectangle: elided again, with its own letters.
    ui.widget_mut::<Label>(label)
        .unwrap()
        .set_text("WWWWWWWWWWWWWWWWWWWWWWWW");
    ui.request_layout(label);
    ui.layout(BOUNDS, &mut list);
    assert!(ui.widget::<Label>(label).unwrap().is_elided());
    let wide = ink_columns(&frame(&list), surface, 0, WIDTH, 0, HEIGHT);
    assert!(*wide.last().unwrap() < 90);
    assert_ne!(wide, before);
}

#[test]
fn assistive_technologies_get_the_whole_text() {
    let mut ui = row();
    ui.set_title("Eliding");
    let root = ui.root();
    let label = ui.add(root, Label::new(LONG), fixed(60.0)).unwrap();
    let check = ui
        .add(
            root,
            Toggle::checkbox(LONG, false, Msg::Changed),
            fixed(60.0),
        )
        .unwrap();
    let button = ui
        .add(root, Button::new(LONG, Msg::Pressed), fixed(60.0))
        .unwrap();
    let mut list = DisplayList::new();
    ui.layout(BOUNDS, &mut list);
    assert!(ui.widget::<Label>(label).unwrap().is_elided());
    assert!(ui.widget::<Toggle<Msg>>(check).unwrap().is_elided());
    assert!(ui.widget::<Button<Msg>>(button).unwrap().is_elided());
    assert_eq!(ui.widget::<Label>(label).unwrap().text(), LONG);
    let update = ui.take_access_update();
    for id in [label, check, button] {
        let node = update
            .tree_update()
            .nodes
            .iter()
            .find(|(node, _)| *node == NodeId(id.get()))
            .map(|(_, node)| node.clone())
            .unwrap();
        let name = node.label().or_else(|| node.value()).unwrap_or_default();
        assert_eq!(name, LONG, "role {:?}", node.role());
    }
}

#[test]
fn a_toggle_keeps_its_indicator_and_shortens_its_label() {
    for toggle in [
        Toggle::checkbox(LONG, true, Msg::Changed),
        Toggle::switch(LONG, true, Msg::Changed),
    ] {
        let mut ui = row();
        let root = ui.root();
        let id = ui.add(root, toggle, fixed(110.0)).unwrap();
        let mut list = DisplayList::new();
        ui.layout(BOUNDS, &mut list);
        assert!(ui.widget::<Toggle<Msg>>(id).unwrap().is_elided());
        let rect = ui.rect(id).unwrap();
        let image = frame(&list);
        let surface = rgba(ColorToken::Surface);
        let ink = ink_columns(&image, surface, 0, WIDTH, 0, HEIGHT);
        // The indicator starts at the left edge, the label uses the rest and stops inside.
        assert_eq!(ink[0], 0);
        assert!(*ink.last().unwrap() >= 90, "ink {ink:?}");
        assert!(*ink.last().unwrap() < rect.x1 as u32, "ink {ink:?}");
        // The indicator is still drawn whole, in the accent colour (the control is on): the
        // box is 16 px wide, the switch's track 32.
        let middle = (rect.y0 + rect.height() / 2.0) as u32;
        let accent = rgba(ColorToken::Accent);
        let filled = (0..32)
            .filter(|&x| pixel(&image, x, middle) == accent)
            .count();
        assert!(filled >= 8, "accent pixels {filled}");
    }
    // Too narrow even for the indicator and the gap: the indicator stays, no label is drawn.
    let mut ui = row();
    let root = ui.root();
    let id = ui
        .add(
            root,
            Toggle::checkbox(LONG, true, Msg::Changed),
            fixed(20.0),
        )
        .unwrap();
    let mut list = DisplayList::new();
    ui.layout(BOUNDS, &mut list);
    assert!(ui.widget::<Toggle<Msg>>(id).unwrap().is_elided());
    let image = frame(&list);
    let ink = ink_columns(&image, rgba(ColorToken::Surface), 0, WIDTH, 0, HEIGHT);
    // Only the 16-pixel box.
    assert!(*ink.last().unwrap() < 17, "ink {ink:?}");
    // With room for everything, nothing is shortened.
    let mut ui = row();
    let root = ui.root();
    let id = ui
        .add(
            root,
            Toggle::switch("Loop", false, Msg::Changed),
            LayoutProps::default(),
        )
        .unwrap();
    let mut list = DisplayList::new();
    ui.layout(BOUNDS, &mut list);
    assert!(!ui.widget::<Toggle<Msg>>(id).unwrap().is_elided());
}

#[test]
fn a_button_keeps_its_padding_and_centres_the_shortened_text() {
    let mut ui = row();
    let root = ui.root();
    let id = ui
        .add(root, Button::new(LONG, Msg::Pressed), fixed(100.0))
        .unwrap();
    let mut list = DisplayList::new();
    ui.layout(BOUNDS, &mut list);
    assert!(ui.widget::<Button<Msg>>(id).unwrap().is_elided());
    let rect = ui.rect(id).unwrap();
    let image = frame(&list);
    // Inside the button, away from its border and corners: the fill, with the text's ink on it.
    let fill = rgba(ColorToken::Control);
    let (y0, y1) = ((rect.y0 + 4.0) as u32, (rect.y1 - 4.0) as u32);
    let ink = ink_columns(&image, fill, 4, 96, y0, y1);
    assert!(ink.len() > 30, "ink {ink:?}");
    // The comfortable density pads a control by 12 px on each side: the text stays inside the
    // padding, and what is left over on the two sides differs by at most a couple of pixels
    // (the text is centred; glyphs have their own side bearings).
    let (first, last) = (ink[0], *ink.last().unwrap());
    assert!(first >= 12 && last < 88, "ink {first}..={last}");
    let (left, right) = (first - 12, 87 - last);
    assert!(left.abs_diff(right) <= 3, "left {left}, right {right}");
    // A button with room is not touched.
    let mut ui = row();
    let root = ui.root();
    let id = ui
        .add(
            root,
            Button::new("OK", Msg::Pressed),
            LayoutProps::default(),
        )
        .unwrap();
    let mut list = DisplayList::new();
    ui.layout(BOUNDS, &mut list);
    assert!(!ui.widget::<Button<Msg>>(id).unwrap().is_elided());
}

#[test]
fn a_wrapping_label_wraps_and_is_never_elided() {
    let mut ui = row();
    let root = ui.root();
    let wrapping = ui
        .add(root, Label::new(LONG).wrapping(), fixed(90.0))
        .unwrap();
    let mut list = DisplayList::new();
    ui.layout(BOUNDS, &mut list);
    assert!(!ui.widget::<Label>(wrapping).unwrap().is_elided());
    // Several lines instead: taller than one line of the same text.
    let mut one = row();
    let one_root = one.root();
    let single = one.add(one_root, Label::new(LONG), fixed(90.0)).unwrap();
    one.layout(BOUNDS, &mut list);
    assert!(ui.rect(wrapping).unwrap().height() > 2.0 * one.rect(single).unwrap().height());
}

#[test]
fn a_change_of_the_text_tokens_fits_the_text_again() {
    // A larger body size makes a label that fitted too wide for the same rectangle: the theme's
    // text generation changes, the text is laid out again and so is its shortened form.
    let mut ui = row();
    let root = ui.root();
    let text = "Count-in bars";
    let label = ui
        .add(root, Label::new(text), fixed(natural_width(text) + 2.0))
        .unwrap();
    let mut list = DisplayList::new();
    ui.layout(BOUNDS, &mut list);
    assert!(!ui.widget::<Label>(label).unwrap().is_elided());
    let larger = theme().with_text_size(orchestra_ui::SizeToken::Body, 18);
    ui.set_theme(&larger);
    ui.layout(BOUNDS, &mut list);
    assert!(ui.widget::<Label>(label).unwrap().is_elided());
}
