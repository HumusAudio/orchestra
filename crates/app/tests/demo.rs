// SPDX-License-Identifier: GPL-3.0-or-later

//! The demonstration window (F2.09c, WD5): its widgets work together through messages, and its
//! accessibility tree names every control; its divider shows the resize shape (F2.09e).

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test helpers: a failed unwrap or index fails the test"
)]

use accesskit::Role;
use orchestra_app::{Demo, SceneContext};
use orchestra_ui::{
    ButtonState, DisplayList, InputEvent, Key, Modifiers, NamedKey, Point, PointerButton,
    PointerShape, Rect, WidgetId, WindowApp,
};

/// The demonstration, laid out in a 640 × 370 window.
fn demo() -> (Demo, DisplayList) {
    let context = SceneContext::new().unwrap();
    let mut demo = Demo::new(
        &context.faces,
        &orchestra_app::theme(),
        "Orchestra",
        &context.demo,
    );
    let mut list = DisplayList::new();
    demo.layout(Rect::from_origin_size(0.0, 0.0, 640.0, 370.0), &mut list);
    (demo, list)
}

/// A click at the centre of `id`.
fn click(demo: &mut Demo, list: &mut DisplayList, id: WidgetId) {
    let r = demo.ui().rect(id).unwrap();
    let at = Point::new(f32::midpoint(r.x0, r.x1), f32::midpoint(r.y0, r.y1));
    for state in [ButtonState::Pressed, ButtonState::Released] {
        demo.input(
            &InputEvent::PointerButton {
                button: PointerButton::Left,
                state,
                position: at,
                clicks: 1,
            },
            list,
        );
    }
}

/// The nodes of the whole accessibility tree, by role, with their names.
fn controls(demo: &mut Demo) -> Vec<(Role, Option<String>, u64)> {
    let update = demo.access_update(true).unwrap();
    update
        .tree_update()
        .nodes
        .iter()
        .filter(|(_, node)| node.role() != Role::GenericContainer)
        .map(|(id, node)| (node.role(), node.label().map(str::to_owned), id.0))
        .collect()
}

#[test]
fn every_control_is_named_for_assistive_technologies() {
    let (mut demo, _) = demo();
    let nodes = controls(&mut demo);
    let named = |role: Role, name: &str| {
        nodes
            .iter()
            .any(|(r, label, _)| *r == role && label.as_deref() == Some(name))
    };
    assert!(named(Role::Window, "Orchestra"));
    assert!(named(Role::Label, "Playback"));
    assert!(named(Role::CheckBox, "Metronome"));
    assert!(named(Role::Switch, "Count-in"));
    assert!(named(Role::Slider, "Metronome volume"));
    assert!(named(Role::Button, "Restore defaults"));
    assert!(named(Role::Button, "OK"));
    assert!(named(Role::TextInput, "Track name"));
}

#[test]
fn restore_defaults_puts_the_controls_back() {
    let (mut demo, mut list) = demo();
    let nodes = controls(&mut demo);
    let find = |role: Role| {
        WidgetId::from_raw(nodes.iter().find(|(r, _, _)| *r == role).unwrap().2).unwrap()
    };
    let (check, switch) = (find(Role::CheckBox), find(Role::Switch));
    let restore = WidgetId::from_raw(
        nodes
            .iter()
            .find(|(r, label, _)| {
                *r == Role::Button && label.as_deref() == Some("Restore defaults")
            })
            .unwrap()
            .2,
    )
    .unwrap();
    let on = |demo: &Demo, id| {
        demo.ui()
            .widget::<orchestra_ui::Toggle<orchestra_app::DemoMessage>>(id)
            .unwrap()
            .is_on()
    };
    let (check_before, switch_before) = (on(&demo, check), on(&demo, switch));
    click(&mut demo, &mut list, check);
    click(&mut demo, &mut list, switch);
    assert_ne!(on(&demo, check), check_before);
    assert_ne!(on(&demo, switch), switch_before);
    click(&mut demo, &mut list, restore);
    assert_eq!(on(&demo, check), check_before);
    assert_eq!(on(&demo, switch), switch_before);
}

#[test]
fn the_options_and_the_staff_are_two_scrolling_panes() {
    // F2.09d (WF8): a splitter with two scroll areas; in a 640 × 370 window the staff (480 wide)
    // does not fit its pane, which then scrolls horizontally.
    let (mut demo, _) = demo();
    let update = demo.access_update(true).unwrap();
    let nodes = &update.tree_update().nodes;
    let count = |role: Role| nodes.iter().filter(|(_, n)| n.role() == role).count();
    assert_eq!(count(Role::Splitter), 1);
    assert_eq!(count(Role::ScrollView), 2);
    let staff = nodes
        .iter()
        .filter(|(_, n)| n.role() == Role::ScrollView)
        .find(|(_, n)| n.scroll_x_max().is_some_and(|max| max > 0.0));
    assert!(staff.is_some(), "the staff's pane should scroll sideways");
}

#[test]
fn the_divider_shows_the_column_resize_shape() {
    // F2.09e: somewhere across the window, at mid height, the pointer meets the splitter's band
    // and takes the column-resize shape; at the window's left edge, over the options, the arrow.
    // The band is found by sweeping instead of computing it, so the test does not depend on the
    // preview's initial share.
    let (mut demo, mut list) = demo();
    let mut at = |demo: &mut Demo, x: f32| {
        demo.input(
            &InputEvent::PointerMoved {
                position: Point::new(x, 200.0),
            },
            &mut list,
        );
        demo.pointer_shape()
    };
    assert_eq!(at(&mut demo, 5.0), PointerShape::Default);
    let hits = (0..640u16)
        .map(f32::from)
        .filter(|&x| at(&mut demo, x) == PointerShape::ResizeColumn)
        .count();
    // The band is as wide as the small gap in comfortable density: 8 logical pixels.
    assert_eq!(hits, 8, "the divider's band should give the resize shape");
}

/// Presses `key` alone.
fn press(demo: &mut Demo, list: &mut DisplayList, key: NamedKey) {
    demo.input(
        &InputEvent::Key {
            key: Key::Named(key),
            state: ButtonState::Pressed,
            repeat: false,
            modifiers: Modifiers::default(),
        },
        list,
    );
}

#[test]
fn the_committed_track_name_is_written_above_the_staff() {
    // F2.10b2: typing a name in the field and pressing Enter commits it (ADR-0047 TF7), and the
    // staff shows it; Escape after more typing brings the committed name back.
    let (mut demo, mut list) = demo();
    let field = demo.track_name();
    click(&mut demo, &mut list, field);
    for c in ["F", "l", "u", "t", "e"] {
        demo.input(&InputEvent::Text(c.to_owned()), &mut list);
    }
    press(&mut demo, &mut list, NamedKey::Enter);
    let staff = |demo: &Demo| {
        demo.ui()
            .widget::<orchestra_app::StaffPreview>(demo.staff())
            .unwrap()
            .name()
            .to_owned()
    };
    assert_eq!(staff(&demo), "Flute");
    demo.input(&InputEvent::Text("s".to_owned()), &mut list);
    press(&mut demo, &mut list, NamedKey::Escape);
    let text = demo
        .ui()
        .widget::<orchestra_ui::TextField<orchestra_app::DemoMessage>>(field)
        .unwrap()
        .text()
        .to_owned();
    assert_eq!(text, "Flute");
    assert_eq!(staff(&demo), "Flute");
    // The caret blinks: the window has a wake-up to sleep until.
    assert!(demo.next_wakeup().is_some());
}
