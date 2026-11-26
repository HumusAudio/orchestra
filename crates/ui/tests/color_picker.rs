// SPDX-License-Identifier: GPL-3.0-or-later

//! The colour picker (SCALETTA F2.12b3a): the HSV numbers and back, the pointer (a press jumps,
//! a drag follows, a fine drag moves a tenth), the keyboard (arrows, Home and End, Tab through
//! the parts and out), the transparency strip only when asked, the pictures in the display list
//! (made, replaced in place, freed with the widget), the colours really drawn, and what
//! assistive technologies are told.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test helpers: a failed unwrap or index fails the test"
)]
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "pixel coordinates of small, positive test rectangles"
)]

use accesskit::{NodeId, Role};
use orchestra_text::Faces;
use orchestra_ui::{
    Appearance, BuiltInTheme, ButtonState, Color, ColorPicker, CpuRenderer, DisplayList,
    FineModifier, Flex, Hsv, InputEvent, Key, LayoutProps, Length, Modifiers, NamedKey,
    PhysicalKey, PickerPart, Point, PointerButton, Rect, Space, Theme, Ui, WidgetId,
};

/// The messages: the picker's colour.
#[derive(Debug, Clone, PartialEq)]
enum Msg {
    /// A new colour.
    Color(Color),
}

/// Studio light.
fn theme() -> Theme {
    *BuiltInTheme::Studio.pair().instance(Appearance::Light)
}

/// The window, 400 × 400.
const BOUNDS: Rect = Rect::new(0.0, 0.0, 400.0, 400.0);

/// A picker of 240 logical pixels in a column, then a button, laid out; with the transparency
/// strip if `alpha`.
fn picker(color: u32, alpha: bool) -> (Ui<Msg>, WidgetId, DisplayList) {
    let mut ui = Ui::new(
        Flex::column(Space::M, Space::M),
        &theme(),
        Faces::bundled().unwrap(),
    );
    let root = ui.root();
    let id = ui
        .add(
            root,
            ColorPicker::new(Color::rgba(color), Msg::Color)
                .with_alpha(alpha)
                .labelled(
                    "Accent color",
                    "Intensity and brightness",
                    "Hue",
                    "Transparency",
                ),
            LayoutProps::default().width(Length::Fixed(240.0)),
        )
        .unwrap();
    // A button after it, so Tab out of the picker has somewhere to go (alone, the focus would
    // come back to it).
    ui.add(
        root,
        orchestra_ui::Button::new("OK", Msg::Color(Color::rgba(0))),
        LayoutProps::default(),
    )
    .unwrap();
    let mut list = DisplayList::new();
    ui.layout(BOUNDS, &mut list);
    (ui, id, list)
}

/// The picker `id`.
fn widget(ui: &Ui<Msg>, id: WidgetId) -> &ColorPicker<Msg> {
    ui.widget::<ColorPicker<Msg>>(id).unwrap()
}

/// The last colour sent, if any; the messages are drained.
fn sent(ui: &mut Ui<Msg>) -> Option<Color> {
    ui.take_messages().next_back().map(|Msg::Color(c)| c)
}

/// A pointer button at `at`.
fn button(ui: &mut Ui<Msg>, list: &mut DisplayList, state: ButtonState, at: Point) {
    ui.input(
        &InputEvent::PointerButton {
            button: PointerButton::Left,
            state,
            position: at,
            clicks: 1,
        },
        list,
    );
}

/// The pointer moves to `at`.
fn move_to(ui: &mut Ui<Msg>, list: &mut DisplayList, at: Point) {
    ui.input(&InputEvent::PointerMoved { position: at }, list);
}

/// `named` pressed with `modifiers`.
fn press(ui: &mut Ui<Msg>, list: &mut DisplayList, named: NamedKey, modifiers: Modifiers) {
    ui.input(
        &InputEvent::Key {
            key: Key::Named(named),
            physical: PhysicalKey::Other,
            state: ButtonState::Pressed,
            repeat: false,
            modifiers,
        },
        list,
    );
}

/// The square's rectangle: the top 150 logical pixels of the picker.
fn square(ui: &Ui<Msg>, id: WidgetId) -> Rect {
    let r = ui.rect(id).unwrap();
    Rect::new(r.x0, r.y0, r.x1, r.y0 + 150.0)
}

/// Every byte of `a` within `tolerance` of `b`'s.
fn close(a: Color, b: Color, tolerance: u8) -> bool {
    [(a.r, b.r), (a.g, b.g), (a.b, b.b), (a.a, b.a)]
        .iter()
        .all(|(x, y)| x.abs_diff(*y) <= tolerance)
}

#[test]
fn hsv_goes_there_and_back() {
    // Every colour of a 16-step grid of the cube comes back exactly.
    for r in (0..=255_u32).step_by(17) {
        for g in (0..=255_u32).step_by(17) {
            for b in (0..=255_u32).step_by(17) {
                let color = Color::rgba(r << 24 | g << 16 | b << 8 | 0xff);
                assert_eq!(Hsv::of(color, 0.0).color(255), color, "{color:?}");
            }
        }
    }
    // Known points: pure red, green, blue; a grey keeps the hue it is given.
    let hue = |rgba: u32| Hsv::of(Color::rgba(rgba), 0.0).h;
    assert!((hue(0xff00_00ff) - 0.0).abs() < 1e-9);
    assert!((hue(0x00ff_00ff) - 120.0).abs() < 1e-9);
    assert!((hue(0x0000_ffff) - 240.0).abs() < 1e-9);
    let grey = Hsv::of(Color::rgba(0x8080_80ff), 210.0);
    assert!((grey.h - 210.0).abs() < 1e-9 && grey.s == 0.0);
}

#[test]
fn a_press_jumps_and_a_drag_follows() {
    let (mut ui, id, mut list) = picker(0xff00_00ff, false);
    let sq = square(&ui, id);
    // The top right corner: full intensity and brightness of red.
    button(
        &mut ui,
        &mut list,
        ButtonState::Pressed,
        Point::new(sq.x1 - 0.5, sq.y0 + 0.5),
    );
    let c = sent(&mut ui).unwrap();
    assert!(close(c, Color::rgba(0xff00_00ff), 2), "{c:?}");
    // Dragged to the bottom (and past it): black, still captured outside the widget.
    move_to(&mut ui, &mut list, Point::new(sq.x1 - 0.5, sq.y1 + 80.0));
    assert_eq!(sent(&mut ui), Some(Color::rgba(0x0000_00ff)));
    button(
        &mut ui,
        &mut list,
        ButtonState::Released,
        Point::new(sq.x1, sq.y1 + 80.0),
    );
    // The hue kept although black has none: back up, red again.
    let h = widget(&ui, id).hsv().h;
    assert!(h.abs() < 1e-9, "{h}");
    // The hue strip, just under the square (8 px gap): its middle is cyan.
    let strip_y = sq.y1 + 8.0 + 8.0;
    button(
        &mut ui,
        &mut list,
        ButtonState::Pressed,
        Point::new(f32::midpoint(sq.x0, sq.x1), strip_y),
    );
    let hsv = widget(&ui, id).hsv();
    assert!((hsv.h - 180.0).abs() < 2.0, "{hsv:?}");
    button(
        &mut ui,
        &mut list,
        ButtonState::Released,
        Point::new(0.0, 0.0),
    );
}

#[test]
fn a_fine_drag_moves_a_tenth() {
    let (mut ui, id, mut list) = picker(0xff00_00ff, false);
    ui.set_fine_modifier(FineModifier::Shift);
    let sq = square(&ui, id);
    let start = Point::new(sq.x0 + 120.0, sq.y0 + 75.0);
    button(&mut ui, &mut list, ButtonState::Pressed, start);
    let before = widget(&ui, id).hsv();
    // Fine from here: the press did not jump, and 100 px to the left move a tenth of that.
    ui.input(
        &InputEvent::ModifiersChanged {
            modifiers: Modifiers {
                shift: true,
                ..Modifiers::default()
            },
        },
        &mut list,
    );
    move_to(&mut ui, &mut list, Point::new(start.x - 100.0, start.y));
    let after = widget(&ui, id).hsv();
    let expected = 100.0 / 240.0 * 0.1;
    assert!(
        (before.s - after.s - expected).abs() < 1e-6,
        "{before:?} {after:?}"
    );
    button(&mut ui, &mut list, ButtonState::Released, start);
}

#[test]
fn the_keyboard_moves_each_part_and_tab_goes_through_them() {
    let (mut ui, id, mut list) = picker(0x8080_80ff, true);
    ui.set_focus(Some(id), &mut list).unwrap();
    let none = Modifiers::default();
    assert_eq!(widget(&ui, id).part(), PickerPart::Square);
    // Square: Home is no intensity, End full; Up is brighter by 4%.
    press(&mut ui, &mut list, NamedKey::End, none);
    assert!((widget(&ui, id).hsv().s - 1.0).abs() < 1e-9);
    let v = widget(&ui, id).hsv().v;
    press(&mut ui, &mut list, NamedKey::ArrowUp, none);
    assert!((widget(&ui, id).hsv().v - (v + 0.04)).abs() < 1e-9);
    // Tab to the hue: Right is 6°, End is 359°.
    press(&mut ui, &mut list, NamedKey::Tab, none);
    assert_eq!(widget(&ui, id).part(), PickerPart::Hue);
    let h = widget(&ui, id).hsv().h;
    press(&mut ui, &mut list, NamedKey::ArrowRight, none);
    assert!((widget(&ui, id).hsv().h - (h + 6.0)).abs() < 1e-9);
    // Tab to the transparency: Home is transparent.
    press(&mut ui, &mut list, NamedKey::Tab, none);
    assert_eq!(widget(&ui, id).part(), PickerPart::Alpha);
    press(&mut ui, &mut list, NamedKey::Home, none);
    assert_eq!(widget(&ui, id).color().a, 0);
    assert_eq!(sent(&mut ui).unwrap().a, 0);
    // Shift+Tab back one part; Tab past the last leaves the picker.
    let shift = Modifiers {
        shift: true,
        ..Modifiers::default()
    };
    press(&mut ui, &mut list, NamedKey::Tab, shift);
    assert_eq!(widget(&ui, id).part(), PickerPart::Hue);
    press(&mut ui, &mut list, NamedKey::Tab, none);
    press(&mut ui, &mut list, NamedKey::Tab, none);
    assert_ne!(ui.focused(), Some(id), "the focus left");
}

#[test]
fn without_transparency_the_alpha_stays_and_tab_skips_it() {
    let (mut ui, id, mut list) = picker(0x3366_cc80, false);
    ui.set_focus(Some(id), &mut list).unwrap();
    press(&mut ui, &mut list, NamedKey::Tab, Modifiers::default());
    assert_eq!(widget(&ui, id).part(), PickerPart::Hue);
    press(&mut ui, &mut list, NamedKey::Tab, Modifiers::default());
    assert_ne!(ui.focused(), Some(id));
    assert_eq!(widget(&ui, id).color().a, 0x80, "kept");
}

#[test]
fn the_pictures_are_drawn_replaced_and_freed() {
    let (mut ui, id, mut list) = picker(0xff00_00ff, false);
    assert_eq!(list.live_images(), 4, "square, hue, transparency, swatches");
    // (The button owns none.)
    // The colours really drawn: the square's top right corner is pure red, the swatches show it.
    let image = CpuRenderer::new(Faces::bundled().unwrap())
        .render_to_image(&list, &theme(), 400, 400, 1.0)
        .unwrap();
    let sq = square(&ui, id);
    let pixel = |x: f32, y: f32| image.pixel(x as u32, y as u32).unwrap();
    let corner = pixel(sq.x1 - 3.0, sq.y0 + 3.0);
    assert!(
        corner[0] > 240 && corner[1] < 20 && corner[2] < 20,
        "{corner:?}"
    );
    let bottom = pixel(f32::midpoint(sq.x0, sq.x1), sq.y1 - 2.0);
    assert!(
        bottom[0] < 10 && bottom[1] < 10 && bottom[2] < 10,
        "black at the bottom: {bottom:?}"
    );
    // A hue change replaces the pictures in place: the store does not grow.
    ui.set_focus(Some(id), &mut list).unwrap();
    for _ in 0..30 {
        press(&mut ui, &mut list, NamedKey::Tab, Modifiers::default());
        press(
            &mut ui,
            &mut list,
            NamedKey::ArrowRight,
            Modifiers::default(),
        );
        press(
            &mut ui,
            &mut list,
            NamedKey::Tab,
            Modifiers {
                shift: true,
                ..Modifiers::default()
            },
        );
    }
    assert_eq!(list.live_images(), 4);
    // Removed, its pictures go.
    ui.remove(id, &mut list).unwrap();
    assert_eq!(list.live_images(), 0);
}

#[test]
fn assistive_technologies_hear_the_part_its_value_and_the_colour() {
    let (mut ui, id, mut list) = picker(0x3366_ccff, false);
    ui.set_focus(Some(id), &mut list).unwrap();
    let update = ui.take_access_update();
    let tree = update.tree_update();
    let node = &tree
        .nodes
        .iter()
        .find(|(n, _)| *n == NodeId(id.get()))
        .unwrap()
        .1;
    assert_eq!(node.role(), Role::Slider);
    assert_eq!(node.label(), Some("Accent color, Intensity and brightness"));
    assert_eq!(node.description(), Some("#3366cc"));
    assert_eq!(
        node.numeric_value(),
        Some(75.0),
        "saturation of #3366cc: 75%"
    );
}
