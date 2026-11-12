// SPDX-License-Identifier: GPL-3.0-or-later

//! The first widgets and the accessibility tree (F2.09b, choices WC2–WC9): labels (size,
//! wrapping, new text, new text tokens), buttons (size, activation by pointer, keyboard and
//! assistive technology, cancel by dragging out, disabled, looks, focus ring), and the AccessKit
//! updates (whole tree, incremental, actions, title).

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

use accesskit::{Action, NodeId, Role};
use orchestra_text::Faces;
use orchestra_ui::{
    AccessAction, AccessActionKind, Appearance, BuiltInTheme, Button, ButtonKind, ButtonState,
    ColorToken, CpuRenderer, DisplayList, FamilyToken, Flex, Image, InputEvent, Key, Label,
    LayoutProps, Modifiers, NamedKey, Point, PointerButton, Rect, SizeToken, Space, Theme, Ui,
    WidgetId,
};

/// The messages of these tests.
#[derive(Debug, Clone, PartialEq)]
enum Msg {
    /// The OK button.
    Ok,
}

/// Studio light.
fn theme() -> Theme {
    *BuiltInTheme::Studio.pair().instance(Appearance::Light)
}

/// A window of 400 × 200.
const BOUNDS: Rect = Rect::new(0.0, 0.0, 400.0, 200.0);

/// A column with a label and a button; returns the interface, the label, the button.
fn panel(button: Button<Msg>) -> (Ui<Msg>, WidgetId, WidgetId, DisplayList) {
    let mut ui = Ui::new(
        Flex::column(Space::M, Space::M),
        &theme(),
        Faces::bundled().unwrap(),
    );
    ui.set_title("Preferences");
    let root = ui.root();
    let label = ui
        .add(
            root,
            Label::new("Tempo of the score"),
            LayoutProps::default(),
        )
        .unwrap();
    let button = ui.add(root, button, LayoutProps::default()).unwrap();
    let mut list = DisplayList::new();
    ui.layout(BOUNDS, &mut list);
    (ui, label, button, list)
}

/// The centre of a widget.
fn centre(ui: &Ui<Msg>, id: WidgetId) -> Point {
    let r = ui.rect(id).unwrap();
    Point::new(f32::midpoint(r.x0, r.x1), f32::midpoint(r.y0, r.y1))
}

/// A pointer button event.
fn pointer(state: ButtonState, at: Point) -> InputEvent {
    InputEvent::PointerButton {
        button: PointerButton::Left,
        state,
        position: at,
    }
}

/// A key press.
fn key(named: NamedKey) -> InputEvent {
    InputEvent::Key {
        key: Key::Named(named),
        state: ButtonState::Pressed,
        repeat: false,
        modifiers: Modifiers::default(),
    }
}

/// The messages so far.
fn messages(ui: &mut Ui<Msg>) -> Vec<Msg> {
    ui.take_messages().collect()
}

/// The whole frame drawn.
fn draw(list: &DisplayList) -> Image {
    CpuRenderer::new(Faces::bundled().unwrap())
        .render_to_image(list, &theme(), 400, 200, 1.0)
        .unwrap()
}

/// The pixel of a token.
fn rgba(token: ColorToken) -> [u8; 4] {
    let c = theme().color(token);
    [c.r, c.g, c.b, 255]
}

#[test]
fn a_label_is_as_large_as_its_text_and_wraps_when_asked() {
    let (ui, label, _, _) = panel(Button::new("OK", Msg::Ok));
    let one_line = ui.rect(label).unwrap();
    assert!(
        one_line.width() > 80.0 && one_line.height() < 20.0,
        "{one_line:?}"
    );
    // In a 90 px column a wrapping label takes several lines.
    let mut narrow: Ui<Msg> = Ui::new(
        Flex::column(Space::None, Space::None),
        &theme(),
        Faces::bundled().unwrap(),
    );
    let root = narrow.root();
    let wrapped = narrow
        .add(
            root,
            Label::new("The project was saved by a newer version").wrapping(),
            LayoutProps::default(),
        )
        .unwrap();
    let mut list = DisplayList::new();
    narrow.layout(Rect::new(0.0, 0.0, 90.0, 300.0), &mut list);
    let r = narrow.rect(wrapped).unwrap();
    assert!(r.width() <= 90.0 && r.height() > 40.0, "{r:?}");
}

#[test]
fn a_label_follows_its_text_and_the_text_tokens() {
    let (mut ui, label, _, mut list) = panel(Button::new("OK", Msg::Ok));
    let before = ui.rect(label).unwrap();
    ui.widget_mut::<Label>(label).unwrap().set_text("Tempo");
    ui.request_layout(label);
    ui.frame(&mut list);
    assert!(ui.rect(label).unwrap().width() < before.width());
    // A larger body size in the theme lays the label out again, taller.
    let height = ui.rect(label).unwrap().height();
    ui.set_theme(&theme().with_text_size(SizeToken::Body, 26));
    ui.frame(&mut list);
    assert!(ui.rect(label).unwrap().height() > 1.8 * height);
    let _ = FamilyToken::Ui;
}

#[test]
fn a_button_is_its_text_plus_padding_by_the_control_height() {
    let (ui, _, button, _) = panel(Button::new("Apply changes", Msg::Ok));
    let r = ui.rect(button).unwrap();
    let spacing = theme().spacing();
    assert_eq!(r.height(), spacing.control_height);
    assert!(r.width() > 2.0 * spacing.control_padding + 60.0, "{r:?}");
}

#[test]
fn a_click_activates_and_dragging_out_cancels() {
    let (mut ui, _, button, mut list) = panel(Button::new("OK", Msg::Ok));
    let at = centre(&ui, button);
    ui.input(&pointer(ButtonState::Pressed, at), &mut list);
    ui.input(&pointer(ButtonState::Released, at), &mut list);
    assert_eq!(messages(&mut ui), [Msg::Ok]);
    // Pressed inside, released outside: no activation.
    ui.input(&pointer(ButtonState::Pressed, at), &mut list);
    ui.input(
        &pointer(ButtonState::Released, Point::new(390.0, 190.0)),
        &mut list,
    );
    assert_eq!(messages(&mut ui), []);
}

#[test]
fn enter_and_space_activate_the_focused_button() {
    let (mut ui, _, button, mut list) = panel(Button::new("OK", Msg::Ok));
    ui.input(&key(NamedKey::Tab), &mut list);
    assert_eq!(
        ui.focused(),
        Some(button),
        "a button takes the focus, a label does not"
    );
    ui.input(&key(NamedKey::Enter), &mut list);
    ui.input(&key(NamedKey::Space), &mut list);
    assert_eq!(messages(&mut ui), [Msg::Ok, Msg::Ok]);
}

#[test]
fn a_disabled_button_does_nothing() {
    let mut disabled = Button::new("OK", Msg::Ok);
    disabled.set_enabled(false);
    let (mut ui, _, button, mut list) = panel(disabled);
    let at = centre(&ui, button);
    ui.input(&pointer(ButtonState::Pressed, at), &mut list);
    ui.input(&pointer(ButtonState::Released, at), &mut list);
    ui.input(&key(NamedKey::Tab), &mut list);
    assert_eq!(ui.focused(), None);
    assert_eq!(messages(&mut ui), []);
}

#[test]
fn buttons_look_their_state() {
    // Normal, at rest: `control`, with a `control-border` edge.
    let (mut ui, _, button, mut list) = panel(Button::new("OK", Msg::Ok));
    let r = ui.rect(button).unwrap();
    let inside = (r.x0 as u32 + 4, r.y0 as u32 + 4);
    assert_eq!(
        draw(&list).pixel(inside.0, inside.1),
        Some(rgba(ColorToken::Control))
    );
    // Pressed: `control-pressed`.
    ui.input(
        &pointer(ButtonState::Pressed, centre(&ui, button)),
        &mut list,
    );
    assert_eq!(
        draw(&list).pixel(inside.0, inside.1),
        Some(rgba(ColorToken::ControlPressed))
    );
    // Primary: `accent`.
    let (_, _, primary, list) = panel(Button::new("OK", Msg::Ok).kind(ButtonKind::Primary));
    let _ = primary;
    assert_eq!(
        draw(&list).pixel(inside.0, inside.1),
        Some(rgba(ColorToken::Accent))
    );
}

#[test]
fn the_focused_button_draws_its_ring() {
    let (mut ui, _, button, mut list) = panel(Button::new("OK", Msg::Ok));
    let r = ui.rect(button).unwrap();
    // Just outside the bottom edge, in the middle: the ring's band.
    let probe = (f32::midpoint(r.x0, r.x1) as u32, (r.y1 + 2.0) as u32);
    let surface = rgba(ColorToken::Surface);
    assert_eq!(draw(&list).pixel(probe.0, probe.1), Some(surface));
    ui.input(&key(NamedKey::Tab), &mut list);
    let ringed = draw(&list).pixel(probe.0, probe.1).unwrap();
    assert_ne!(ringed, surface, "the ring is drawn outside the button");
}

#[test]
fn the_first_access_update_is_the_whole_tree() {
    let (mut ui, label, button, _) = panel(Button::new("OK", Msg::Ok));
    let update = ui.take_access_update();
    let tree = update.tree_update();
    assert!(tree.tree.is_some(), "the first update describes the tree");
    let node = |id: NodeId| &tree.nodes.iter().find(|(n, _)| *n == id).unwrap().1;
    // The window, named after the title, holds the root widget.
    let window = node(NodeId(0));
    assert_eq!(window.role(), Role::Window);
    assert_eq!(window.label(), Some("Preferences"));
    assert_eq!(window.children(), [NodeId(ui.root().get())]);
    // One node per widget: the root column and its two children.
    assert_eq!(tree.nodes.len(), 4);
    let b = node(NodeId(button.get()));
    assert_eq!(b.role(), Role::Button);
    assert_eq!(b.label(), Some("OK"));
    assert!(b.supports_action(Action::Click) && b.supports_action(Action::Focus));
    assert!(b.bounds().is_some());
    let l = node(NodeId(label.get()));
    assert_eq!(l.role(), Role::Label);
    assert_eq!(l.label(), Some("Tempo of the score"));
    assert_eq!(tree.focus, NodeId(0), "nothing focused: the window");
}

#[test]
fn later_updates_carry_only_what_changed() {
    let (mut ui, _, button, mut list) = panel(Button::new("OK", Msg::Ok));
    let _ = ui.take_access_update();
    // Nothing changed: no node, no tree description.
    let quiet = ui.take_access_update();
    assert!(quiet.tree_update().nodes.is_empty() && quiet.tree_update().tree.is_none());
    // Focusing the button: its node (re-recorded for its ring) and the focus.
    ui.input(&key(NamedKey::Tab), &mut list);
    let update = ui.take_access_update();
    let tree = update.tree_update();
    assert_eq!(tree.focus, NodeId(button.get()));
    assert!(
        tree.nodes.iter().all(|(id, _)| *id == NodeId(button.get())),
        "{:?}",
        tree.nodes
    );
    // Disabling it: its node says so, and offers no action.
    ui.widget_mut::<Button<Msg>>(button)
        .unwrap()
        .set_enabled(false);
    ui.request_record(button);
    ui.frame(&mut list);
    let update = ui.take_access_update();
    let (_, node) = &update.tree_update().nodes[0];
    assert!(node.is_disabled() && !node.supports_action(Action::Click));
    // After a reset (an assistive technology connected), the whole tree again.
    ui.reset_access();
    assert!(ui.take_access_update().tree_update().tree.is_some());
}

#[test]
fn assistive_technologies_can_click_and_focus() {
    let (mut ui, _, button, mut list) = panel(Button::new("OK", Msg::Ok));
    ui.access_action(
        AccessAction {
            target: button,
            kind: AccessActionKind::Click,
        },
        &mut list,
    )
    .unwrap();
    assert_eq!(messages(&mut ui), [Msg::Ok]);
    ui.access_action(
        AccessAction {
            target: button,
            kind: AccessActionKind::Focus,
        },
        &mut list,
    )
    .unwrap();
    assert_eq!(ui.focused(), Some(button));
    let gone = WidgetId::from_raw(9_999).unwrap();
    assert!(
        ui.access_action(
            AccessAction {
                target: gone,
                kind: AccessActionKind::Click
            },
            &mut list
        )
        .is_err()
    );
}

/// A node lists its children in order; adding a widget later sends the new node and its
/// parent (whose list of children changed), and nothing else.
#[test]
fn children_are_listed_and_additions_resend_the_parent() {
    let (mut ui, label, button, mut list) = panel(Button::new("OK", Msg::Ok));
    let root = ui.root();
    let first = ui.take_access_update();
    let root_node = &first
        .tree_update()
        .nodes
        .iter()
        .find(|(id, _)| *id == NodeId(root.get()))
        .unwrap()
        .1;
    assert_eq!(
        root_node.children(),
        [NodeId(label.get()), NodeId(button.get())]
    );
    let added = ui
        .add(root, Label::new("Added"), LayoutProps::default())
        .unwrap();
    ui.frame(&mut list);
    let update = ui.take_access_update();
    let mut ids: Vec<NodeId> = update
        .tree_update()
        .nodes
        .iter()
        .map(|(id, _)| *id)
        .collect();
    ids.sort_by_key(|id| id.0);
    assert_eq!(ids, [NodeId(root.get()), NodeId(added.get())]);
}

/// The messages of the toggle and slider tests.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Set {
    /// A check box or a switch changed.
    On(bool),
    /// A slider changed.
    Value(f64),
}

/// A column with one widget, laid out in a 400 × 200 window.
fn single<W: orchestra_ui::Widget<Set>>(widget: W) -> (Ui<Set>, WidgetId, DisplayList) {
    let mut ui = Ui::new(
        Flex::column(Space::M, Space::M),
        &theme(),
        Faces::bundled().unwrap(),
    );
    let root = ui.root();
    let id = ui
        .add(
            root,
            widget,
            LayoutProps::default().width(orchestra_ui::Length::Fixed(300.0)),
        )
        .unwrap();
    let mut list = DisplayList::new();
    ui.layout(BOUNDS, &mut list);
    (ui, id, list)
}

/// A click at `at`.
fn click_at(ui: &mut Ui<Set>, list: &mut DisplayList, at: Point) {
    ui.input(&pointer(ButtonState::Pressed, at), list);
    ui.input(&pointer(ButtonState::Released, at), list);
}

#[test]
fn a_checkbox_toggles_by_click_on_its_label_space_and_access() {
    let (mut ui, id, mut list) =
        single(orchestra_ui::Toggle::checkbox("Metronome", false, Set::On));
    let r = ui.rect(id).unwrap();
    // A click on the label (right part of the control) toggles.
    click_at(
        &mut ui,
        &mut list,
        Point::new(r.x0 + 60.0, f32::midpoint(r.y0, r.y1)),
    );
    assert_eq!(ui.take_messages().collect::<Vec<_>>(), [Set::On(true)]);
    assert!(ui.widget::<orchestra_ui::Toggle<Set>>(id).unwrap().is_on());
    // Space when focused, AccessKit's click: off, then on again.
    ui.input(&key(NamedKey::Tab), &mut list);
    ui.input(&key(NamedKey::Space), &mut list);
    ui.access_action(
        AccessAction {
            target: id,
            kind: AccessActionKind::Click,
        },
        &mut list,
    )
    .unwrap();
    assert_eq!(
        ui.take_messages().collect::<Vec<_>>(),
        [Set::On(false), Set::On(true)]
    );
    // Its node: a check box, named, checked.
    ui.reset_access();
    let update = ui.take_access_update();
    let node = &update
        .tree_update()
        .nodes
        .iter()
        .find(|(n, _)| *n == NodeId(id.get()))
        .unwrap()
        .1;
    assert_eq!(node.role(), Role::CheckBox);
    assert_eq!(node.label(), Some("Metronome"));
    assert_eq!(node.toggled(), Some(accesskit::Toggled::True));
}

#[test]
fn a_switch_shows_its_state_with_the_accent() {
    let (mut ui, id, mut list) = single(orchestra_ui::Toggle::switch("Count-in", false, Set::On));
    let r = ui.rect(id).unwrap();
    let height = (theme().spacing().control_height * 4.0 / 7.0).round();
    // A pixel inside the track, between the knob positions.
    let probe = ((r.x0 + height) as u32, f32::midpoint(r.y0, r.y1) as u32 + 4);
    let off = draw(&list).pixel(probe.0, probe.1).unwrap();
    click_at(
        &mut ui,
        &mut list,
        Point::new(r.x0 + 5.0, f32::midpoint(r.y0, r.y1)),
    );
    assert_eq!(ui.take_messages().collect::<Vec<_>>(), [Set::On(true)]);
    let on = draw(&list).pixel(probe.0, probe.1).unwrap();
    assert_ne!(off, on);
    assert_eq!(on, rgba(ColorToken::Accent));
    ui.reset_access();
    let update = ui.take_access_update();
    let node = &update
        .tree_update()
        .nodes
        .iter()
        .find(|(n, _)| *n == NodeId(id.get()))
        .unwrap()
        .1;
    assert_eq!(node.role(), Role::Switch);
}

#[test]
fn a_disabled_toggle_ignores_everything() {
    let mut toggle = orchestra_ui::Toggle::checkbox("Metronome", false, Set::On);
    toggle.set_enabled(false);
    let (mut ui, id, mut list) = single(toggle);
    let r = ui.rect(id).unwrap();
    click_at(
        &mut ui,
        &mut list,
        Point::new(r.x0 + 5.0, f32::midpoint(r.y0, r.y1)),
    );
    ui.input(&key(NamedKey::Tab), &mut list);
    assert_eq!(ui.focused(), None);
    assert_eq!(ui.take_messages().count(), 0);
}

#[test]
fn a_slider_drags_snaps_and_clamps() {
    let (mut ui, id, mut list) =
        single(orchestra_ui::Slider::new(0.0, 100.0, 5.0, 50.0, Set::Value).labelled("Volume"));
    let r = ui.rect(id).unwrap();
    let y = f32::midpoint(r.y0, r.y1);
    // Press at the far left: the value jumps to the minimum and the pointer is captured.
    ui.input(
        &pointer(ButtonState::Pressed, Point::new(r.x0, y)),
        &mut list,
    );
    // Dragging beyond the right end clamps to the maximum, even outside the slider.
    ui.input(
        &InputEvent::PointerMoved {
            position: Point::new(r.x1 + 80.0, y + 40.0),
        },
        &mut list,
    );
    ui.input(
        &pointer(ButtonState::Released, Point::new(r.x1 + 80.0, y + 40.0)),
        &mut list,
    );
    assert_eq!(
        ui.take_messages().collect::<Vec<_>>(),
        [Set::Value(0.0), Set::Value(100.0)]
    );
    // A press about a third of the way snaps to a multiple of the step.
    ui.input(
        &pointer(ButtonState::Pressed, Point::new(r.x0 + r.width() / 3.0, y)),
        &mut list,
    );
    ui.input(
        &pointer(ButtonState::Released, Point::new(r.x0 + r.width() / 3.0, y)),
        &mut list,
    );
    let value = ui.widget::<orchestra_ui::Slider<Set>>(id).unwrap().value();
    assert!(
        (value / 5.0).fract() == 0.0 && (25.0..=40.0).contains(&value),
        "{value}"
    );
}

#[test]
fn a_slider_answers_the_keyboard_and_assistive_technologies() {
    let (mut ui, id, mut list) =
        single(orchestra_ui::Slider::new(0.0, 100.0, 1.0, 50.0, Set::Value).labelled("Volume"));
    ui.input(&key(NamedKey::Tab), &mut list);
    assert_eq!(ui.focused(), Some(id));
    for (named, expected) in [
        (NamedKey::ArrowRight, 51.0),
        (NamedKey::ArrowDown, 50.0),
        (NamedKey::PageUp, 60.0),
        (NamedKey::PageDown, 50.0),
        (NamedKey::End, 100.0),
        (NamedKey::Home, 0.0),
    ] {
        ui.input(&key(named), &mut list);
        assert_eq!(
            ui.widget::<orchestra_ui::Slider<Set>>(id).unwrap().value(),
            expected,
            "{named:?}"
        );
    }
    // At the minimum, going down sends nothing (the value did not change).
    let _ = ui.take_messages().count();
    ui.input(&key(NamedKey::ArrowLeft), &mut list);
    assert_eq!(ui.take_messages().count(), 0);
    for (kind, expected) in [
        (AccessActionKind::Increment, 1.0),
        (AccessActionKind::SetValue(42.4), 42.0),
        (AccessActionKind::Decrement, 41.0),
    ] {
        ui.access_action(AccessAction { target: id, kind }, &mut list)
            .unwrap();
        assert_eq!(
            ui.widget::<orchestra_ui::Slider<Set>>(id).unwrap().value(),
            expected
        );
    }
    // Its node: a slider, named by its label, with its value, range and value actions.
    ui.reset_access();
    let update = ui.take_access_update();
    let node = &update
        .tree_update()
        .nodes
        .iter()
        .find(|(n, _)| *n == NodeId(id.get()))
        .unwrap()
        .1;
    assert_eq!(node.role(), Role::Slider);
    assert_eq!(node.label(), Some("Volume"));
    assert_eq!(node.numeric_value(), Some(41.0));
    assert_eq!(
        (node.min_numeric_value(), node.max_numeric_value()),
        (Some(0.0), Some(100.0))
    );
    assert!(node.supports_action(Action::Increment) && node.supports_action(Action::SetValue));
}

/// Pressing a check box and releasing outside it cancels, as for buttons.
#[test]
fn dragging_out_of_a_toggle_cancels() {
    let (mut ui, id, mut list) =
        single(orchestra_ui::Toggle::checkbox("Metronome", false, Set::On));
    let r = ui.rect(id).unwrap();
    ui.input(
        &pointer(
            ButtonState::Pressed,
            Point::new(r.x0 + 5.0, f32::midpoint(r.y0, r.y1)),
        ),
        &mut list,
    );
    ui.input(
        &pointer(ButtonState::Released, Point::new(390.0, 190.0)),
        &mut list,
    );
    assert_eq!(ui.take_messages().count(), 0);
    assert!(!ui.widget::<orchestra_ui::Toggle<Set>>(id).unwrap().is_on());
}
