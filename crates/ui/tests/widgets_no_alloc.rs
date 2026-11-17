// SPDX-License-Identifier: GPL-3.0-or-later

//! Routing events and re-recording a widget do not allocate (ADR-0045 WA3, WB11): once the
//! message queue and the display list have their capacity, a click, a drag and a Tab move the
//! state, emit messages and re-record segments without touching the heap. Since F2.09d the same
//! holds for scrolling (wheel, thumb drag, Tab into a hidden widget) and for dragging a
//! splitter's divider, and since F2.09e for asking the pointer's shape after each event.
//! Since F2.09f it also holds while the divider narrows real labels, toggles and buttons below
//! their texts' widths: shortening a text with an ellipsis uses the shaper's own working memory.
//!
//! This file is its own test binary with the counting allocator installed (ADR-0016 §5).

#![allow(
    clippy::unwrap_used,
    reason = "test helpers: a failed unwrap fails the test"
)]

use std::hint::black_box;

use orchestra_test_support::assert_no_allocations;
use orchestra_ui::{
    Appearance, BuiltInTheme, ButtonState, ColorToken, Constraints, DisplayList, EventCx, Flex,
    Handled, InputEvent, Key, LayoutCx, LayoutProps, Modifiers, NamedKey, Point, PointerButton,
    RecordCx, Recorder, Rect, Size, Space, Ui, Widget, WidgetEvent,
};
use orchestra_ui::{Axis, Length, ScrollArea, Splitter, WheelDelta};

orchestra_test_support::install_counting_allocator!();

/// A button-like leaf: handles presses (emitting a unit message) and takes the focus.
#[derive(Debug)]
struct Pad {
    /// Toggled at each press, so its look changes and it is re-recorded.
    on: bool,
}

impl Widget<()> for Pad {
    fn measure(&mut self, _cx: &mut LayoutCx<'_, ()>, constraints: Constraints) -> Size {
        constraints.constrain(Size::new(40.0, 20.0))
    }
    fn record(&self, cx: &RecordCx, recorder: &mut Recorder<'_>) {
        let token = if self.on {
            ColorToken::Accent
        } else {
            ColorToken::Control
        };
        recorder.fill_rect(cx.rect, token);
    }
    fn event(&mut self, cx: &mut EventCx<'_, ()>, event: &WidgetEvent<'_>) -> Handled {
        match event {
            WidgetEvent::PointerButton { state, .. } => {
                if *state == ButtonState::Pressed {
                    self.on = !self.on;
                    cx.emit(());
                    cx.request_record();
                }
                Handled::Yes
            }
            WidgetEvent::PointerMoved { .. } => Handled::Yes,
            _ => Handled::No,
        }
    }
    fn focusable(&self) -> bool {
        true
    }
}

#[test]
fn routing_never_allocates() {
    let theme = *BuiltInTheme::Studio.pair().instance(Appearance::Light);
    let mut ui = Ui::new(
        Flex::column(Space::S, Space::M),
        &theme,
        orchestra_text::Faces::bundled().unwrap(),
    );
    let root = ui.root();
    for _ in 0..10 {
        let row = ui
            .add(
                root,
                Flex::row(Space::S, Space::None),
                LayoutProps::default(),
            )
            .unwrap();
        for _ in 0..5 {
            ui.add(row, Pad { on: false }, LayoutProps::default())
                .unwrap();
        }
    }
    let mut list = DisplayList::new();
    ui.layout(Rect::from_origin_size(0.0, 0.0, 600.0, 400.0), &mut list);
    let tab = InputEvent::Key {
        key: Key::Named(NamedKey::Tab),
        state: ButtonState::Pressed,
        repeat: false,
        modifiers: Modifiers::default(),
    };
    let round = |ui: &mut Ui<()>, list: &mut DisplayList| {
        for x in [30.0, 80.0, 130.0] {
            let position = Point::new(x, 30.0);
            ui.input(&InputEvent::PointerMoved { position }, list);
            for state in [ButtonState::Pressed, ButtonState::Released] {
                ui.input(
                    &InputEvent::PointerButton {
                        button: PointerButton::Left,
                        state,
                        position,
                        clicks: 1,
                    },
                    list,
                );
            }
        }
        ui.input(&tab, list);
        black_box(ui.take_messages().count());
        black_box(list.take_damage());
    };
    // The first round sizes the queue, the stacks and the segments.
    round(&mut ui, &mut list);
    assert_no_allocations(|| round(&mut ui, &mut list));
}

#[test]
fn scrolling_and_moving_a_divider_never_allocate() {
    let theme = *BuiltInTheme::Studio.pair().instance(Appearance::Light);
    let mut ui: Ui<()> = Ui::new(
        Flex::column(Space::None, Space::None),
        &theme,
        orchestra_text::Faces::bundled().unwrap(),
    );
    let root = ui.root();
    let fill = LayoutProps::default()
        .width(Length::Fill(1.0))
        .height(Length::Fill(1.0));
    // A splitter: a scroll area of 50 pads on the left, one pad on the right.
    let splitter = ui
        .add(root, Splitter::new(Axis::Horizontal, 0.5), fill)
        .unwrap();
    let scroll = ui.add(splitter, ScrollArea::vertical(), fill).unwrap();
    ui.add(splitter, Pad { on: false }, fill).unwrap();
    let column = ui
        .add(
            scroll,
            Flex::column(Space::None, Space::None),
            LayoutProps::default(),
        )
        .unwrap();
    for _ in 0..50 {
        ui.add(column, Pad { on: false }, LayoutProps::default())
            .unwrap();
    }
    let mut list = DisplayList::new();
    ui.layout(Rect::from_origin_size(0.0, 0.0, 600.0, 400.0), &mut list);
    let band = ui.rect(scroll).unwrap().x1 + 2.0;
    let thumb_x = ui.widget::<ScrollArea>(scroll).unwrap().viewport().x1 + 4.0;
    let button = |ui: &mut Ui<()>, list: &mut DisplayList, state, position| {
        ui.input(
            &InputEvent::PointerButton {
                button: PointerButton::Left,
                state,
                position,
                clicks: 1,
            },
            list,
        );
    };
    let tab = InputEvent::Key {
        key: Key::Named(NamedKey::Tab),
        state: ButtonState::Pressed,
        repeat: false,
        modifiers: Modifiers::default(),
    };
    let round = |ui: &mut Ui<()>, list: &mut DisplayList| {
        // The wheel down and back up over the content.
        ui.input(
            &InputEvent::PointerMoved {
                position: Point::new(20.0, 100.0),
            },
            list,
        );
        for y in [-1.0, -1.0, 2.0] {
            ui.input(
                &InputEvent::Wheel {
                    delta: WheelDelta::Lines { x: 0.0, y },
                },
                list,
            );
        }
        // The thumb dragged down and back.
        button(ui, list, ButtonState::Pressed, Point::new(thumb_x, 10.0));
        for y in [60.0, 120.0, 10.0] {
            ui.input(
                &InputEvent::PointerMoved {
                    position: Point::new(thumb_x, y),
                },
                list,
            );
        }
        button(ui, list, ButtonState::Released, Point::new(thumb_x, 10.0));
        // Tab far enough to reach hidden pads (the area scrolls to show them), then back.
        for _ in 0..25 {
            ui.input(&tab, list);
        }
        // The divider dragged right and back, asking the pointer's shape after every event as
        // the window does (F2.09e): over the content, during the drag, after the release.
        black_box(ui.pointer_shape());
        button(ui, list, ButtonState::Pressed, Point::new(band, 200.0));
        black_box(ui.pointer_shape());
        for x in [band + 40.0, band - 30.0, band] {
            ui.input(
                &InputEvent::PointerMoved {
                    position: Point::new(x, 200.0),
                },
                list,
            );
            black_box(ui.pointer_shape());
        }
        button(ui, list, ButtonState::Released, Point::new(band, 200.0));
        black_box(ui.pointer_shape());
        black_box(ui.take_messages().count());
        black_box(list.take_damage());
    };
    // Two rounds size everything (the second reaches the Tab positions the first did not).
    round(&mut ui, &mut list);
    round(&mut ui, &mut list);
    assert_no_allocations(|| round(&mut ui, &mut list));
}

#[test]
fn eliding_while_a_divider_moves_never_allocates() {
    use orchestra_ui::{Button, Label, Toggle};
    let theme = *BuiltInTheme::Studio.pair().instance(Appearance::Light);
    let mut ui: Ui<()> = Ui::new(
        Flex::column(Space::None, Space::None),
        &theme,
        orchestra_text::Faces::bundled().unwrap(),
    );
    let root = ui.root();
    let fill = LayoutProps::default()
        .width(Length::Fill(1.0))
        .height(Length::Fill(1.0));
    // A splitter: on the left a column of real controls whose texts are wider than the pane at
    // every position the divider takes below, on the right a pad.
    let splitter = ui
        .add(
            root,
            Splitter::new(Axis::Horizontal, 0.3).minimum_sizes(0.0, 0.0),
            fill,
        )
        .unwrap();
    let column = ui
        .add(splitter, Flex::column(Space::None, Space::None), fill)
        .unwrap();
    ui.add(splitter, Pad { on: false }, fill).unwrap();
    let label = ui
        .add(
            column,
            Label::new("Metronome during the count-in bars of a recording"),
            LayoutProps::default(),
        )
        .unwrap();
    let toggle = ui
        .add(
            column,
            Toggle::switch("Follow the playback cursor while playing", true, |_| ()),
            LayoutProps::default(),
        )
        .unwrap();
    let button = ui
        .add(
            column,
            Button::new("Restore every option to its default value", ()),
            LayoutProps::default(),
        )
        .unwrap();
    let mut list = DisplayList::new();
    ui.layout(Rect::from_origin_size(0.0, 0.0, 600.0, 400.0), &mut list);
    let band = ui.rect(column).unwrap().x1 + 2.0;
    let round = |ui: &mut Ui<()>, list: &mut DisplayList| {
        ui.input(
            &InputEvent::PointerButton {
                button: PointerButton::Left,
                state: ButtonState::Pressed,
                position: Point::new(band, 200.0),
                clicks: 1,
            },
            list,
        );
        // Narrower and narrower, down to no room at all, then back: every move gives the three
        // controls a new width, so each shortens its text again.
        for x in [
            band - 30.0,
            band - 90.0,
            band - 150.0,
            2.0,
            band - 60.0,
            band,
        ] {
            ui.input(
                &InputEvent::PointerMoved {
                    position: Point::new(x, 200.0),
                },
                list,
            );
        }
        ui.input(
            &InputEvent::PointerButton {
                button: PointerButton::Left,
                state: ButtonState::Released,
                position: Point::new(band, 200.0),
                clicks: 1,
            },
            list,
        );
        black_box(ui.take_messages().count());
        black_box(list.take_damage());
    };
    round(&mut ui, &mut list);
    round(&mut ui, &mut list);
    assert_no_allocations(|| round(&mut ui, &mut list));
    // The premise: the three texts really are shortened at the end.
    assert!(ui.widget::<Label>(label).unwrap().is_elided());
    assert!(ui.widget::<Toggle<()>>(toggle).unwrap().is_elided());
    assert!(ui.widget::<Button<()>>(button).unwrap().is_elided());
}

/// Showing and hiding tooltips allocates nothing once warm (F2.11c, ADR-0048 AM15): the tooltip's
/// widget is created once and kept hidden, its text is copied into the string it already has,
/// and its segment is re-recorded in place. Opening a menu does allocate (it adds widgets), and is
/// not tested here: it happens once per click, not at every pointer rest.
#[test]
fn showing_and_hiding_tooltips_never_allocates() {
    use std::time::Duration;

    use orchestra_ui::Button;
    let theme = *BuiltInTheme::Studio.pair().instance(Appearance::Light);
    let mut ui: Ui<()> = Ui::new(
        Flex::row(Space::None, Space::None),
        &theme,
        orchestra_text::Faces::bundled().unwrap(),
    );
    let root = ui.root();
    let narrow = LayoutProps::default().width(Length::Fixed(60.0));
    // Two elided buttons side by side; the longer text first, so the tooltip's string has room
    // for both after the first round.
    for text in [
        "Restore every option to its default value",
        "Reverb send level",
    ] {
        ui.add(root, Button::new(text, ()), narrow).unwrap();
    }
    let mut list = DisplayList::new();
    ui.layout(Rect::from_origin_size(0.0, 0.0, 600.0, 400.0), &mut list);
    let start = ui.now();
    let mut at = Duration::ZERO;
    let mut round = |ui: &mut Ui<()>, list: &mut DisplayList| {
        // Rest on the first button until its tooltip shows, slide to the second (shown at once,
        // the warm time), press (dismissed), leave the window.
        for (x, wait) in [(30.0, 500), (90.0, 0)] {
            ui.input(
                &InputEvent::PointerMoved {
                    position: Point::new(x, 14.0),
                },
                list,
            );
            at += Duration::from_millis(wait);
            ui.advance(start + at, list);
            assert!(ui.tooltip_text().is_some());
        }
        for state in [ButtonState::Pressed, ButtonState::Released] {
            ui.input(
                &InputEvent::PointerButton {
                    button: PointerButton::Left,
                    state,
                    position: Point::new(90.0, 14.0),
                    clicks: 1,
                },
                list,
            );
        }
        ui.input(&InputEvent::PointerLeft, list);
        at += Duration::from_secs(2);
        ui.advance(start + at, list);
        black_box(ui.take_messages().count());
        black_box(list.take_damage());
    };
    round(&mut ui, &mut list);
    round(&mut ui, &mut list);
    assert_no_allocations(|| round(&mut ui, &mut list));
}
