// SPDX-License-Identifier: GPL-3.0-or-later

//! The widgets' core (ADR-0045, F2.09a): layout containers and size rules, incremental layout,
//! segments, hit testing, bubbling, capture, hover, focus and Tab order, modal layers, messages,
//! removal and density. A probe widget logs what it receives.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test helpers: a failed unwrap or index fails the test"
)]

use orchestra_ui::{
    Align, Appearance, BuiltInTheme, ButtonState, ColorToken, Constraints, Damage, Density,
    DisplayList, EventCx, Flex, Grid, Handled, InputEvent, Key, LayoutCx, LayoutProps, Length,
    Modifiers, NamedKey, Point, PointerButton, RecordCx, Recorder, Rect, Size, Space, Stack, Theme,
    Track, Ui, Widget, WidgetEvent, WidgetId,
};

/// The application's messages in these tests.
#[derive(Debug, Clone, PartialEq)]
enum Msg {
    /// A probe was pressed.
    Pressed(WidgetId),
}

/// A leaf that wants a size, logs its events, counts its measures, may handle presses and take
/// the focus.
#[derive(Debug, Default)]
struct Probe {
    /// The size it asks for.
    size: Size,
    /// Handles pointer presses and releases (and emits a message on press).
    handles_pointer: bool,
    /// Takes the keyboard focus.
    focusable: bool,
    /// Handles keys.
    handles_keys: bool,
    /// What it received, by name.
    log: Vec<&'static str>,
    /// How many times it was measured.
    measures: u32,
}

impl Probe {
    fn sized(width: f32, height: f32) -> Self {
        Self {
            size: Size::new(width, height),
            ..Self::default()
        }
    }
    fn pointer(mut self) -> Self {
        self.handles_pointer = true;
        self
    }
    fn focusable(mut self) -> Self {
        self.focusable = true;
        self
    }
}

impl Widget<Msg> for Probe {
    fn measure(&mut self, _cx: &mut LayoutCx<'_, Msg>, constraints: Constraints) -> Size {
        self.measures += 1;
        constraints.constrain(self.size)
    }

    fn record(&self, cx: &RecordCx, recorder: &mut Recorder<'_>) {
        recorder.fill_rect(cx.rect, ColorToken::Control);
    }

    fn event(&mut self, cx: &mut EventCx<'_, Msg>, event: &WidgetEvent<'_>) -> Handled {
        let name = match event {
            WidgetEvent::PointerEntered => "entered",
            WidgetEvent::PointerLeft => "left",
            WidgetEvent::PointerMoved { .. } => "moved",
            WidgetEvent::PointerButton {
                state: ButtonState::Pressed,
                ..
            } => "pressed",
            WidgetEvent::PointerButton { .. } => "released",
            WidgetEvent::Wheel { .. } => "wheel",
            WidgetEvent::Key { .. } => "key",
            WidgetEvent::FocusGained => "focus",
            WidgetEvent::FocusLost => "blur",
        };
        self.log.push(name);
        match event {
            WidgetEvent::PointerButton { state, .. } if self.handles_pointer => {
                if *state == ButtonState::Pressed {
                    cx.emit(Msg::Pressed(cx.id()));
                    cx.request_record();
                }
                Handled::Yes
            }
            WidgetEvent::PointerMoved { .. } if self.handles_pointer => Handled::Yes,
            WidgetEvent::Key { .. } if self.handles_keys => Handled::Yes,
            _ => Handled::No,
        }
    }

    fn focusable(&self) -> bool {
        self.focusable
    }
}

/// Studio light.
fn theme() -> Theme {
    *BuiltInTheme::Studio.pair().instance(Appearance::Light)
}

/// A window of 400 × 300.
fn bounds() -> Rect {
    Rect::from_origin_size(0.0, 0.0, 400.0, 300.0)
}

/// A click (press and release) at `(x, y)`.
fn click(ui: &mut Ui<Msg>, list: &mut DisplayList, x: f32, y: f32) {
    for state in [ButtonState::Pressed, ButtonState::Released] {
        ui.input(
            &InputEvent::PointerButton {
                button: PointerButton::Left,
                state,
                position: Point::new(x, y),
            },
            list,
        );
    }
}

/// A key press.
fn press(ui: &mut Ui<Msg>, list: &mut DisplayList, key: NamedKey, shift: bool) {
    ui.input(
        &InputEvent::Key {
            key: Key::Named(key),
            state: ButtonState::Pressed,
            repeat: false,
            modifiers: Modifiers {
                shift,
                ..Modifiers::default()
            },
        },
        list,
    );
}

/// The log of probe `id`.
fn log(ui: &Ui<Msg>, id: WidgetId) -> Vec<&'static str> {
    ui.widget::<Probe>(id).unwrap().log.clone()
}

#[test]
fn a_column_places_fixed_content_and_fill_children() {
    let mut ui = Ui::new(Flex::column(Space::Px(10.0), Space::Px(5.0)), &theme());
    let root = ui.root();
    let fixed = ui
        .add(
            root,
            Probe::sized(50.0, 999.0),
            LayoutProps::default().height(Length::Fixed(40.0)),
        )
        .unwrap();
    let content = ui
        .add(root, Probe::sized(80.0, 30.0), LayoutProps::default())
        .unwrap();
    let fill = ui
        .add(
            root,
            Probe::sized(10.0, 10.0),
            LayoutProps::default()
                .height(Length::Fill(1.0))
                .width(Length::Fill(1.0)),
        )
        .unwrap();
    let mut list = DisplayList::new();
    ui.layout(bounds(), &mut list);
    assert_eq!(ui.rect(fixed), Some(Rect::new(5.0, 5.0, 55.0, 45.0)));
    assert_eq!(ui.rect(content), Some(Rect::new(5.0, 55.0, 85.0, 85.0)));
    // The fill child takes the rest of the height and the whole inner width.
    assert_eq!(ui.rect(fill), Some(Rect::new(5.0, 95.0, 395.0, 295.0)));
}

#[test]
fn a_row_shares_the_rest_by_weight_and_aligns_across() {
    let mut ui = Ui::new(Flex::row(Space::None, Space::None), &theme());
    let root = ui.root();
    let one = ui
        .add(
            root,
            Probe::sized(0.0, 20.0),
            LayoutProps::default()
                .width(Length::Fill(1.0))
                .align(Align::Start, Align::Center),
        )
        .unwrap();
    let two = ui
        .add(
            root,
            Probe::sized(0.0, 20.0),
            LayoutProps::default()
                .width(Length::Fill(2.0))
                .align(Align::Start, Align::End),
        )
        .unwrap();
    let fixed = ui
        .add(root, Probe::sized(100.0, 300.0), LayoutProps::default())
        .unwrap();
    let mut list = DisplayList::new();
    ui.layout(bounds(), &mut list);
    // 300 px left after the fixed child: one third and two thirds.
    assert_eq!(ui.rect(one), Some(Rect::new(0.0, 140.0, 100.0, 160.0)));
    assert_eq!(ui.rect(two), Some(Rect::new(100.0, 280.0, 300.0, 300.0)));
    assert_eq!(ui.rect(fixed), Some(Rect::new(300.0, 0.0, 400.0, 300.0)));
}

#[test]
fn a_stack_aligns_each_child_in_the_padded_area() {
    let mut ui = Ui::new(
        Stack {
            padding: Space::Px(10.0),
        },
        &theme(),
    );
    let root = ui.root();
    let centre = ui
        .add(
            root,
            Probe::sized(100.0, 50.0),
            LayoutProps::default().align(Align::Center, Align::Center),
        )
        .unwrap();
    let corner = ui
        .add(
            root,
            Probe::sized(20.0, 20.0),
            LayoutProps::default().align(Align::End, Align::End),
        )
        .unwrap();
    let mut list = DisplayList::new();
    ui.layout(bounds(), &mut list);
    assert_eq!(ui.rect(centre), Some(Rect::new(150.0, 125.0, 250.0, 175.0)));
    assert_eq!(ui.rect(corner), Some(Rect::new(370.0, 270.0, 390.0, 290.0)));
}

#[test]
fn a_grid_sizes_fixed_content_and_fill_tracks() {
    let grid = Grid::new(
        vec![Track::Fixed(50.0), Track::Content, Track::Fill(1.0)],
        vec![Track::Content, Track::Fill(1.0)],
        Space::Px(10.0),
        Space::None,
    );
    let mut ui = Ui::new(grid, &theme());
    let root = ui.root();
    let a = ui
        .add(
            root,
            Probe::sized(30.0, 20.0),
            LayoutProps::default().cell(0, 0),
        )
        .unwrap();
    let b = ui
        .add(
            root,
            Probe::sized(70.0, 35.0),
            LayoutProps::default().cell(0, 1),
        )
        .unwrap();
    let c = ui
        .add(
            root,
            Probe::sized(10.0, 10.0),
            LayoutProps::default()
                .cell(1, 2)
                .width(Length::Fill(1.0))
                .height(Length::Fill(1.0)),
        )
        .unwrap();
    let mut list = DisplayList::new();
    ui.layout(bounds(), &mut list);
    // Columns: 50, 70 (content), and 400 − 50 − 70 − 2 gaps = 260. Rows: 35 (content), the rest
    // 300 − 35 − 10 = 255.
    assert_eq!(ui.rect(a), Some(Rect::new(0.0, 0.0, 30.0, 20.0)));
    assert_eq!(ui.rect(b), Some(Rect::new(60.0, 0.0, 130.0, 35.0)));
    assert_eq!(ui.rect(c), Some(Rect::new(140.0, 45.0, 400.0, 300.0)));
}

#[test]
fn layout_is_incremental() {
    let mut ui = Ui::new(Flex::column(Space::None, Space::None), &theme());
    let root = ui.root();
    let rows: Vec<WidgetId> = (0..3)
        .map(|_| {
            ui.add(
                root,
                Flex::row(Space::None, Space::None),
                LayoutProps::default(),
            )
            .unwrap()
        })
        .collect();
    let leaves: Vec<WidgetId> = rows
        .iter()
        .flat_map(|&row| {
            (0..3)
                .map(|_| {
                    ui.add(row, Probe::sized(20.0, 10.0), LayoutProps::default())
                        .unwrap()
                })
                .collect::<Vec<_>>()
        })
        .collect();
    let mut list = DisplayList::new();
    ui.layout(bounds(), &mut list);
    let measures = |ui: &Ui<Msg>| -> Vec<u32> {
        leaves
            .iter()
            .map(|&leaf| ui.widget::<Probe>(leaf).unwrap().measures)
            .collect()
    };
    assert!(measures(&ui).iter().all(|&count| count == 1));
    // Nothing changed: a frame measures nothing.
    ui.frame(&mut list);
    assert!(measures(&ui).iter().all(|&count| count == 1));
    // One leaf grows: it is measured again, and so is its sibling to the right (a row gives
    // each child the space still free, which changed); every other leaf, its left sibling and
    // the other rows' leaves, answers from its cache.
    ui.widget_mut::<Probe>(leaves[4]).unwrap().size = Size::new(60.0, 10.0);
    ui.request_layout(leaves[4]);
    let _ = list.take_damage();
    ui.frame(&mut list);
    let counts = measures(&ui);
    assert_eq!(counts, [1, 1, 1, 1, 2, 2, 1, 1, 1]);
    // Its siblings to the right moved: they are re-recorded, the other rows are not.
    assert_eq!(ui.rect(leaves[5]), Some(Rect::new(80.0, 10.0, 100.0, 20.0)));
    let damage = list.take_damage();
    assert!(
        damage
            .rects()
            .iter()
            .all(|rect| rect.y0 >= 10.0 && rect.y1 <= 20.0),
        "{damage:?}"
    );
}

#[test]
fn every_widget_has_its_segment_and_only_changes_are_recorded() {
    let mut ui = Ui::new(Flex::row(Space::Px(10.0), Space::None), &theme());
    let root = ui.root();
    let a = ui
        .add(
            root,
            Probe::sized(50.0, 50.0).pointer(),
            LayoutProps::default(),
        )
        .unwrap();
    let _b = ui
        .add(root, Probe::sized(50.0, 50.0), LayoutProps::default())
        .unwrap();
    let mut list = DisplayList::new();
    ui.layout(bounds(), &mut list);
    // The root and its two children.
    assert_eq!(list.segment_count(), 3);
    let _ = list.take_damage();
    // A press on `a` re-records `a` only.
    click(&mut ui, &mut list, 10.0, 10.0);
    let damage = list.take_damage();
    let expected = ui.rect(a).unwrap();
    assert_eq!(damage.rects(), &[expected]);
}

#[test]
fn pointer_events_hit_the_topmost_widget_and_bubble() {
    // A handling parent (a probe as a stack's child would be a leaf, so use the root: a stack
    // whose children are a handling probe and, above it, a non-handling one).
    let mut ui = Ui::new(
        Stack {
            padding: Space::None,
        },
        &theme(),
    );
    let root = ui.root();
    let below = ui
        .add(
            root,
            Probe::sized(100.0, 100.0).pointer(),
            LayoutProps::default(),
        )
        .unwrap();
    let above = ui
        .add(root, Probe::sized(50.0, 50.0), LayoutProps::default())
        .unwrap();
    let mut list = DisplayList::new();
    ui.layout(bounds(), &mut list);
    click(&mut ui, &mut list, 10.0, 10.0);
    // The topmost widget gets it first; it does not handle it; it bubbles to the root (a
    // stack, which ignores it), never sideways to `below`.
    // (No "entered": the hover follows pointer moves, and this click came without one.)
    assert_eq!(log(&ui, above), ["pressed", "released"]);
    assert_eq!(log(&ui, below), Vec::<&str>::new());
    // Where only `below` is, `below` gets it and emits its message.
    click(&mut ui, &mut list, 80.0, 80.0);
    assert!(log(&ui, below).contains(&"pressed"));
    assert_eq!(
        ui.take_messages().collect::<Vec<_>>(),
        [Msg::Pressed(below)]
    );
}

#[test]
fn a_handled_press_captures_the_pointer_until_the_release() {
    let mut ui = Ui::new(Flex::row(Space::None, Space::None), &theme());
    let root = ui.root();
    let a = ui
        .add(
            root,
            Probe::sized(100.0, 100.0).pointer(),
            LayoutProps::default(),
        )
        .unwrap();
    let b = ui
        .add(
            root,
            Probe::sized(100.0, 100.0).pointer(),
            LayoutProps::default(),
        )
        .unwrap();
    let mut list = DisplayList::new();
    ui.layout(bounds(), &mut list);
    let at = |x: f32| Point::new(x, 50.0);
    ui.input(&InputEvent::PointerMoved { position: at(50.0) }, &mut list);
    ui.input(
        &InputEvent::PointerButton {
            button: PointerButton::Left,
            state: ButtonState::Pressed,
            position: at(50.0),
        },
        &mut list,
    );
    // Dragging over `b`: `a` keeps every move, `b` hears nothing, the hover stays on `a`.
    ui.input(
        &InputEvent::PointerMoved {
            position: at(150.0),
        },
        &mut list,
    );
    ui.input(
        &InputEvent::PointerButton {
            button: PointerButton::Left,
            state: ButtonState::Released,
            position: at(150.0),
        },
        &mut list,
    );
    assert_eq!(
        log(&ui, a),
        ["entered", "moved", "pressed", "moved", "released", "left"]
    );
    // After the release the hover moves to `b`.
    assert_eq!(log(&ui, b), ["entered"]);
    assert_eq!(ui.hovered(), Some(b));
}

#[test]
fn clicks_and_tab_move_the_focus_in_tree_order() {
    let mut ui = Ui::new(Flex::row(Space::None, Space::None), &theme());
    let root = ui.root();
    let a = ui
        .add(
            root,
            Probe::sized(50.0, 50.0).focusable(),
            LayoutProps::default(),
        )
        .unwrap();
    let _plain = ui
        .add(root, Probe::sized(50.0, 50.0), LayoutProps::default())
        .unwrap();
    let b = ui
        .add(
            root,
            Probe::sized(50.0, 50.0).focusable(),
            LayoutProps::default(),
        )
        .unwrap();
    let c = ui
        .add(
            root,
            Probe::sized(50.0, 50.0).focusable(),
            LayoutProps::default(),
        )
        .unwrap();
    let mut list = DisplayList::new();
    ui.layout(bounds(), &mut list);
    assert_eq!(ui.focused(), None);
    press(&mut ui, &mut list, NamedKey::Tab, false);
    assert_eq!(ui.focused(), Some(a));
    press(&mut ui, &mut list, NamedKey::Tab, false);
    assert_eq!(ui.focused(), Some(b), "the non-focusable widget is skipped");
    press(&mut ui, &mut list, NamedKey::Tab, false);
    press(&mut ui, &mut list, NamedKey::Tab, false);
    assert_eq!(ui.focused(), Some(a), "wraps around");
    press(&mut ui, &mut list, NamedKey::Tab, true);
    assert_eq!(ui.focused(), Some(c), "Shift+Tab goes back, wrapping");
    click(&mut ui, &mut list, 110.0, 10.0);
    assert_eq!(ui.focused(), Some(b));
    assert_eq!(log(&ui, a).iter().filter(|&&e| e == "blur").count(), 2);
    // Keys go to the focused widget.
    press(&mut ui, &mut list, NamedKey::Enter, false);
    assert_eq!(log(&ui, b).last(), Some(&"key"));
}

#[test]
fn a_handled_tab_does_not_move_the_focus() {
    let mut ui = Ui::new(Flex::row(Space::None, Space::None), &theme());
    let root = ui.root();
    let mut keeper = Probe::sized(50.0, 50.0).focusable();
    keeper.handles_keys = true;
    let keeper = ui.add(root, keeper, LayoutProps::default()).unwrap();
    let _other = ui
        .add(
            root,
            Probe::sized(50.0, 50.0).focusable(),
            LayoutProps::default(),
        )
        .unwrap();
    let mut list = DisplayList::new();
    ui.layout(bounds(), &mut list);
    ui.set_focus(Some(keeper), &mut list).unwrap();
    press(&mut ui, &mut list, NamedKey::Tab, false);
    assert_eq!(ui.focused(), Some(keeper));
}

#[test]
fn a_modal_layer_blocks_the_pointer_outside_and_traps_the_focus() {
    let mut ui = Ui::new(
        Stack {
            padding: Space::None,
        },
        &theme(),
    );
    let root = ui.root();
    let page = ui
        .add(
            root,
            Flex::row(Space::None, Space::None),
            LayoutProps::default(),
        )
        .unwrap();
    let outside = ui
        .add(
            page,
            Probe::sized(100.0, 100.0).pointer().focusable(),
            LayoutProps::default(),
        )
        .unwrap();
    let dialog = ui
        .add(
            root,
            Flex::row(Space::None, Space::None),
            LayoutProps::default().align(Align::End, Align::End),
        )
        .unwrap();
    let ok = ui
        .add(
            dialog,
            Probe::sized(60.0, 40.0).pointer().focusable(),
            LayoutProps::default(),
        )
        .unwrap();
    let cancel = ui
        .add(
            dialog,
            Probe::sized(60.0, 40.0).pointer().focusable(),
            LayoutProps::default(),
        )
        .unwrap();
    let mut list = DisplayList::new();
    ui.layout(bounds(), &mut list);
    ui.set_focus(Some(outside), &mut list).unwrap();
    ui.open_modal(dialog, &mut list).unwrap();
    assert_eq!(
        ui.focused(),
        Some(ok),
        "the dialog's first focusable widget"
    );
    // A click outside the dialog reaches nobody.
    click(&mut ui, &mut list, 10.0, 10.0);
    assert!(!log(&ui, outside).contains(&"pressed"));
    assert_eq!(ui.take_messages().count(), 0);
    // Tab cycles inside the dialog.
    press(&mut ui, &mut list, NamedKey::Tab, false);
    assert_eq!(ui.focused(), Some(cancel));
    press(&mut ui, &mut list, NamedKey::Tab, false);
    assert_eq!(ui.focused(), Some(ok));
    // Closing gives the focus back.
    ui.close_modal(dialog, &mut list).unwrap();
    assert_eq!(ui.focused(), Some(outside));
    click(&mut ui, &mut list, 10.0, 10.0);
    assert!(log(&ui, outside).contains(&"pressed"));
}

#[test]
fn removing_a_subtree_drops_its_segments_and_its_focus() {
    let mut ui = Ui::new(Flex::row(Space::None, Space::None), &theme());
    let root = ui.root();
    let group = ui
        .add(
            root,
            Flex::column(Space::None, Space::None),
            LayoutProps::default(),
        )
        .unwrap();
    let inner = ui
        .add(
            group,
            Probe::sized(50.0, 50.0).focusable(),
            LayoutProps::default(),
        )
        .unwrap();
    let mut list = DisplayList::new();
    ui.layout(bounds(), &mut list);
    ui.set_focus(Some(inner), &mut list).unwrap();
    assert_eq!(list.segment_count(), 3);
    ui.remove(group, &mut list).unwrap();
    assert_eq!(list.segment_count(), 1);
    assert_eq!(ui.focused(), None);
    assert!(ui.widget::<Probe>(inner).is_none());
    assert_eq!(
        ui.remove(inner, &mut list),
        Err(orchestra_ui::UiError::UnknownWidget(inner))
    );
    assert_eq!(
        ui.remove(root, &mut list),
        Err(orchestra_ui::UiError::RemoveRoot)
    );
    // A new widget never gets a removed widget's ID.
    let fresh = ui
        .add(root, Probe::sized(1.0, 1.0), LayoutProps::default())
        .unwrap();
    assert!(fresh != inner && fresh != group);
}

#[test]
fn the_density_changes_the_spacing() {
    let comfortable = theme();
    let compact = comfortable.with_density(Density::Compact);
    let mut ui = Ui::new(Flex::column(Space::M, Space::L), &comfortable);
    let root = ui.root();
    let _a = ui
        .add(root, Probe::sized(10.0, 10.0), LayoutProps::default())
        .unwrap();
    let b = ui
        .add(root, Probe::sized(10.0, 10.0), LayoutProps::default())
        .unwrap();
    let mut list = DisplayList::new();
    ui.layout(bounds(), &mut list);
    // Comfortable: padding 16, gap 12.
    assert_eq!(ui.rect(b).unwrap().y0, 16.0 + 10.0 + 12.0);
    ui.set_theme(&compact);
    ui.frame(&mut list);
    // Compact: padding 12, gap 8.
    assert_eq!(ui.rect(b).unwrap().y0, 12.0 + 10.0 + 8.0);
}

#[test]
fn the_wheel_goes_under_the_pointer_and_bubbles() {
    let mut ui = Ui::new(Flex::row(Space::None, Space::None), &theme());
    let root = ui.root();
    let a = ui
        .add(root, Probe::sized(100.0, 100.0), LayoutProps::default())
        .unwrap();
    let mut list = DisplayList::new();
    ui.layout(bounds(), &mut list);
    ui.input(
        &InputEvent::PointerMoved {
            position: Point::new(20.0, 20.0),
        },
        &mut list,
    );
    ui.input(
        &InputEvent::Wheel {
            delta: orchestra_ui::WheelDelta::Lines { x: 0.0, y: 1.0 },
        },
        &mut list,
    );
    assert_eq!(log(&ui, a), ["entered", "moved", "wheel"]);
    let _ = Damage::none();
}

/// A container that lays its children out as a row and handles the presses its children leave
/// unhandled: the target of bubbling (WB7).
#[derive(Debug)]
struct Catcher {
    /// The layout it delegates to.
    row: Flex,
    /// Presses that reached it.
    caught: u32,
}

impl Widget<Msg> for Catcher {
    fn measure(&mut self, cx: &mut LayoutCx<'_, Msg>, constraints: Constraints) -> Size {
        self.row.measure(cx, constraints)
    }
    fn arrange(&mut self, cx: &mut LayoutCx<'_, Msg>, rect: Rect) {
        self.row.arrange(cx, rect);
    }
    fn record(&self, _cx: &RecordCx, _recorder: &mut Recorder<'_>) {}
    fn event(&mut self, _cx: &mut EventCx<'_, Msg>, event: &WidgetEvent<'_>) -> Handled {
        if let WidgetEvent::PointerButton {
            state: ButtonState::Pressed,
            ..
        } = event
        {
            self.caught += 1;
            return Handled::Yes;
        }
        Handled::No
    }
}

#[test]
fn an_unhandled_press_bubbles_to_the_parent_that_handles_it() {
    let mut ui = Ui::new(Flex::column(Space::None, Space::None), &theme());
    let root = ui.root();
    let catcher = ui
        .add(
            root,
            Catcher {
                row: Flex::row(Space::None, Space::None),
                caught: 0,
            },
            LayoutProps::default(),
        )
        .unwrap();
    let ignoring = ui
        .add(catcher, Probe::sized(50.0, 50.0), LayoutProps::default())
        .unwrap();
    let handling = ui
        .add(
            catcher,
            Probe::sized(50.0, 50.0).pointer(),
            LayoutProps::default(),
        )
        .unwrap();
    let mut list = DisplayList::new();
    ui.layout(bounds(), &mut list);
    click(&mut ui, &mut list, 10.0, 10.0);
    // The child saw the press and let it bubble; the parent handled it, so the parent captured
    // the pointer and got the release; then the hover was updated and the child was entered.
    assert_eq!(log(&ui, ignoring), ["pressed", "entered"]);
    assert_eq!(ui.widget::<Catcher>(catcher).unwrap().caught, 1);
    // A child that handles the press keeps it.
    click(&mut ui, &mut list, 60.0, 10.0);
    assert!(log(&ui, handling).contains(&"pressed"));
    assert_eq!(ui.widget::<Catcher>(catcher).unwrap().caught, 1);
}

/// A leaf filled with one colour.
#[derive(Debug)]
struct Swatch(ColorToken, Size);

impl Widget<Msg> for Swatch {
    fn measure(&mut self, _cx: &mut LayoutCx<'_, Msg>, constraints: Constraints) -> Size {
        constraints.constrain(self.1)
    }
    fn record(&self, cx: &RecordCx, recorder: &mut Recorder<'_>) {
        recorder.fill_rect(cx.rect, self.0);
    }
}

/// Segments are recorded in tree order (WA6): a later sibling is drawn on top of an earlier one,
/// and children on top of their parent.
#[test]
fn later_siblings_are_drawn_on_top() {
    let theme = theme();
    let mut ui = Ui::new(
        Stack {
            padding: Space::None,
        },
        &theme,
    );
    let root = ui.root();
    let _under = ui
        .add(
            root,
            Swatch(ColorToken::Control, Size::new(100.0, 100.0)),
            LayoutProps::default(),
        )
        .unwrap();
    let _over = ui
        .add(
            root,
            Swatch(ColorToken::Accent, Size::new(40.0, 40.0)),
            LayoutProps::default().align(Align::Center, Align::Center),
        )
        .unwrap();
    let mut list = DisplayList::new();
    ui.layout(Rect::from_origin_size(0.0, 0.0, 100.0, 100.0), &mut list);
    let image = orchestra_ui::CpuRenderer::new(orchestra_text::Faces::bundled().unwrap())
        .render_to_image(&list, &theme, 100, 100, 1.0)
        .unwrap();
    let accent = theme.color(ColorToken::Accent);
    let control = theme.color(ColorToken::Control);
    assert_eq!(
        image.pixel(50, 50).unwrap(),
        [accent.r, accent.g, accent.b, 255]
    );
    assert_eq!(
        image.pixel(5, 5).unwrap(),
        [control.r, control.g, control.b, 255]
    );
}
