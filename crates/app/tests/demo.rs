// SPDX-License-Identifier: GPL-3.0-or-later

//! The demonstration window (F2.09c, WD5): its widgets work together through messages, and its
//! accessibility tree names every control.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test helpers: a failed unwrap or index fails the test"
)]

use accesskit::Role;
use orchestra_app::{Demo, SceneContext};
use orchestra_ui::{
    ButtonState, DisplayList, InputEvent, Point, PointerButton, Rect, WidgetId, WindowApp,
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
