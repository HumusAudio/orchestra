// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of the window's logic, without a display (ADR-0040 W9): the input translation (with the
//! primary modifier of each platform), the pixel conversion, the choice between partial and whole
//! copies, the damage rectangles for `present_with_damage`, and the order of window creation.

use std::cell::RefCell;

use winit::dpi::PhysicalPosition;
use winit::event::{ElementState, MouseButton, MouseScrollDelta};
use winit::keyboard::{Key as WinitKey, ModifiersState, NamedKey as WinitNamedKey, SmolStr};

use super::input::{
    self, ButtonState, Key, Modifiers, NamedKey, PointerButton, WheelDelta, to_logical,
};
use super::order::{Showable, open_in_order};
use super::present::{CopyPlan, copy_region, copy_whole, softbuffer_rect, to_0rgb};
use crate::draw::device::{DeviceRect, DeviceRegions};
use crate::{Damage, Image, Rect};

#[test]
fn the_primary_modifier_is_ctrl_except_on_macos() {
    // Ctrl held: primary on Linux and Windows, secondary on macOS.
    let ctrl = Modifiers::from_keys(true, false, false, false, false);
    assert!(ctrl.primary && !ctrl.secondary);
    let ctrl_mac = Modifiers::from_keys(true, false, false, false, true);
    assert!(!ctrl_mac.primary && ctrl_mac.secondary);
    // Cmd (the logo key) held: primary on macOS only.
    let cmd_mac = Modifiers::from_keys(false, true, false, false, true);
    assert!(cmd_mac.primary && !cmd_mac.secondary);
    let logo = Modifiers::from_keys(false, true, false, false, false);
    assert!(!logo.primary && logo.secondary);
    // Shift and Alt are the same everywhere.
    let shift_alt = Modifiers::from_keys(false, false, true, true, true);
    assert!(shift_alt.shift && shift_alt.alt && !shift_alt.primary);
}

#[test]
fn winit_modifiers_use_this_platforms_primary() {
    let held = Modifiers::from_winit(ModifiersState::CONTROL | ModifiersState::SHIFT);
    assert!(held.shift);
    assert_eq!(held.primary, !cfg!(target_os = "macos"));
    let none = Modifiers::from_winit(ModifiersState::empty());
    assert_eq!(none, Modifiers::default());
}

#[test]
fn keys_keep_their_character_or_name() {
    assert_eq!(
        input::key(&WinitKey::Character(SmolStr::new("é"))),
        Key::Character("é".to_owned())
    );
    assert_eq!(
        input::key(&WinitKey::Named(WinitNamedKey::Enter)),
        Key::Named(NamedKey::Enter)
    );
    assert_eq!(
        input::key(&WinitKey::Named(WinitNamedKey::Space)),
        Key::Named(NamedKey::Space)
    );
    assert_eq!(
        input::key(&WinitKey::Named(WinitNamedKey::F12)),
        Key::Named(NamedKey::Function(12))
    );
    assert_eq!(
        input::key(&WinitKey::Named(WinitNamedKey::F24)),
        Key::Named(NamedKey::Function(24))
    );
    // Keys we do not use yet are "unknown", not an error.
    assert_eq!(
        input::key(&WinitKey::Named(WinitNamedKey::MediaPlayPause)),
        Key::Unknown
    );
}

#[test]
fn buttons_states_and_wheels_translate() {
    assert_eq!(
        input::pointer_button(MouseButton::Left),
        PointerButton::Left
    );
    assert_eq!(
        input::pointer_button(MouseButton::Back),
        PointerButton::Back
    );
    assert_eq!(
        input::pointer_button(MouseButton::Other(9)),
        PointerButton::Other(9)
    );
    assert_eq!(
        input::button_state(ElementState::Pressed),
        ButtonState::Pressed
    );
    assert_eq!(
        input::wheel_delta(MouseScrollDelta::LineDelta(0.0, 1.0), 2.0),
        WheelDelta::Lines { x: 0.0, y: 1.0 }
    );
    // Pixel deltas become logical: 30 physical pixels at scale 2 are 15 logical.
    assert_eq!(
        input::wheel_delta(
            MouseScrollDelta::PixelDelta(PhysicalPosition::new(0.0, 30.0)),
            2.0
        ),
        WheelDelta::Pixels { x: 0.0, y: 15.0 }
    );
    assert!((to_logical(150.0, 1.5) - 100.0).abs() < 1e-4);
    assert!(
        (to_logical(7.0, 0.0) - 7.0).abs() < 1e-6,
        "a zero scale is ignored"
    );
}

#[test]
fn pixels_become_0rgb() {
    assert_eq!(to_0rgb([0x12, 0x34, 0x56, 0xff]), 0x0012_3456);
    assert_eq!(to_0rgb([0xff, 0xff, 0xff, 0xff]), 0x00ff_ffff);
}

/// A 4 × 3 frame whose pixel (x, y) is red = x, green = y, blue = 9, opaque.
fn numbered_frame() -> Image {
    let mut data = Vec::new();
    for y in 0..3_u8 {
        for x in 0..4_u8 {
            data.extend_from_slice(&[x, y, 9, 255]);
        }
    }
    Image::from_premultiplied_rgba(4, 3, data).unwrap()
}

#[test]
fn copying_a_region_touches_only_that_region() {
    let frame = numbered_frame();
    let mut buffer = vec![u32::MAX; 12];
    let region = DeviceRect {
        x: 1,
        y: 1,
        width: 2,
        height: 2,
    };
    copy_region(&frame, &region, &mut buffer);
    for y in 0..3_u32 {
        for x in 0..4_u32 {
            let value = buffer[(y * 4 + x) as usize];
            let inside = (1..3).contains(&x) && (1..3).contains(&y);
            if inside {
                assert_eq!(value, (x << 16) | (y << 8) | 9, "({x}, {y})");
            } else {
                assert_eq!(value, u32::MAX, "({x}, {y}) must be untouched");
            }
        }
    }
    let mut whole = vec![0; 12];
    copy_whole(&frame, &mut whole);
    assert_eq!(whole[11], (3 << 16) | (2 << 8) | 9);
}

#[test]
fn only_a_buffer_of_age_one_gets_a_partial_copy() {
    let mut damage = Damage::none();
    damage.add(Rect::new(10.0, 10.0, 20.0, 20.0));
    let partial = DeviceRegions::new(&damage, 1.0, 200, 100);
    assert_eq!(CopyPlan::choose(1, &partial, 200, 100), CopyPlan::Damage);
    // Unknown contents (after a resize) or an older buffer: everything is copied.
    assert_eq!(CopyPlan::choose(0, &partial, 200, 100), CopyPlan::Whole);
    assert_eq!(CopyPlan::choose(2, &partial, 200, 100), CopyPlan::Whole);
    // Damage that is already the whole frame is a whole copy at any age.
    let all = DeviceRegions::new(&Damage::all(), 1.0, 200, 100);
    assert_eq!(CopyPlan::choose(1, &all, 200, 100), CopyPlan::Whole);
}

#[test]
fn presented_rectangles_are_the_drawn_regions_in_device_pixels() {
    // At scale 2, the logical 10..20 becomes 20..40, grown by the 1 px fringe: 19..41.
    let mut damage = Damage::none();
    damage.add(Rect::new(10.0, 10.0, 20.0, 20.0));
    let regions = DeviceRegions::new(&damage, 2.0, 400, 400);
    let rects: Vec<_> = regions.iter().filter_map(softbuffer_rect).collect();
    assert_eq!(rects.len(), 1);
    let rect = rects[0];
    assert_eq!((rect.x, rect.y), (19, 19));
    assert_eq!((rect.width.get(), rect.height.get()), (22, 22));
    // No damage: nothing to present.
    assert_eq!(
        DeviceRegions::new(&Damage::none(), 1.0, 10, 10)
            .iter()
            .count(),
        0
    );
}

/// A fake window recording what happens to it, in order.
struct FakeWindow<'a>(&'a RefCell<Vec<&'static str>>);

impl Showable for FakeWindow<'_> {
    fn show(&self) {
        self.0.borrow_mut().push("show");
    }
}

#[test]
fn a_window_is_set_up_before_it_is_shown() {
    let log = RefCell::new(Vec::new());
    let result: Result<_, ()> = open_in_order(
        || {
            log.borrow_mut().push("create hidden");
            Ok(FakeWindow(&log))
        },
        |_| log.borrow_mut().push("setup"),
    );
    assert!(result.is_ok());
    assert_eq!(*log.borrow(), ["create hidden", "setup", "show"]);
}

#[test]
fn a_window_that_cannot_be_created_is_neither_set_up_nor_shown() {
    let log = RefCell::new(Vec::new());
    let result = open_in_order(
        || -> Result<FakeWindow<'_>, &str> { Err("no display") },
        |_| log.borrow_mut().push("setup"),
    );
    assert_eq!(result.err(), Some("no display"));
    assert!(log.borrow().is_empty());
}
