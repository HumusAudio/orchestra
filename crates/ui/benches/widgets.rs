// SPDX-License-Identifier: GPL-3.0-or-later

//! Instruction-count benchmarks of the widgets' core (ADR-0045, F2.09a).
//!
//! # The panel
//! A column of 40 rows, each a row of five button-like leaves: 200 widgets plus 41 containers,
//! the size of the spike's panel (`spikes/widgets`). The leaves draw a filled rectangle (the
//! real widgets of F2.09b–c draw more, with text, but the core's costs are what is measured
//! here), in a 1000 × 1000 frame.
//!
//! # The cases
//! - `first_frame`: layout from nothing, recording every segment, drawing the whole frame.
//! - `click`: a press and a release on one leaf: routing, the leaf re-recorded, the damage drawn.
//! - `grow_one`: one leaf grows: the incremental layout, the moved siblings re-recorded and drawn.
//! - `relayout_all`: the density changes: everything laid out and recorded again (not drawn).
//! - `real_*` (F2.09b): the same panel with real widgets, a label and four buttons per row, all
//!   with text: the first frame (with the whole accessibility tree), a click (hover, press,
//!   release, the button re-recorded and drawn, the accessibility update), and the whole
//!   accessibility tree built again (an assistive technology connecting).
//! - `slider_drag_move` (F2.09c): 40 rows of a label and a slider, one move while a slider is
//!   dragged (its value changes, it is re-recorded and drawn, its node is sent).
//! - `slider_wheel_notch` (F2.09c2, ADR-0046 V2): the same rows, one wheel notch over the 20th
//!   slider under the pointer (the value moves the coarse step, re-recorded, drawn, sent).
//!
//! - `scroll_wheel_notch` (F2.09d): the real panel (a label and four buttons per row, 200
//!   widgets) inside a vertical scroll area filling a 1000 × 1000 window, the right pane of a
//!   splitter; one wheel notch: the content arranged again at the new offset, every widget
//!   re-recorded inside the viewport (those outside as empty segments), the viewport drawn, the
//!   moved nodes sent to the accessibility tree.
//! - `splitter_drag_move` (F2.09d): the same window, one move while the divider is dragged: the
//!   panes arranged again (the content keeps its rectangles, only the area's bar moves), drawn,
//!   sent.
//! - `splitter_drag_eliding` (F2.09f, ADR-0045 WJ1–WJ7): a 1000 × 1000 window whose left pane, a
//!   vertical scroll area 150 pixels wide, holds 40 labels and 40 switches whose texts are all
//!   wider than the pane, so all 80 are drawn shortened with an ellipsis; one move while the
//!   divider is dragged 30 pixels to the left: the panes arranged again, each of the 80 texts
//!   shortened again for its new width (two shapings each), the pane drawn, the nodes sent. The
//!   worst case of eliding: every control of a pane narrowed at once. Budget: a frame's, 33 M.
//! - `pointer_shape_over_a_widget` (F2.09e): the same window with the pointer over a widget of
//!   the scrolled content, which answers nothing: the question the window asks after every event
//!   walks the whole way up (the widget, its row, the column, the scroll area, the splitter, the
//!   root), the longest walk this window has. Asked alone, without an event, so its own cost is
//!   what is counted: the event's cost is in the cases above.
//! - `edit_type_key`, `edit_word_move` (F2.10b1, ADR-0047 TF13): the editing core alone, without
//!   drawing, in a 60-character name with the caret in the middle: one typed letter (the edit,
//!   its grouping into the open undo step, the caret's grapheme boundary), and one Ctrl+→ (a
//!   walk of the word segmenter over the text). The field's whole keystroke, with shaping and
//!   drawing, is measured in F2.10b2 against the 0.5 M of an event.
//! - `field_type_key`, `field_caret_blink` (F2.10b2, ADR-0047 TF13): the real panel (200 widgets)
//!   with a text field of 300 pixels on top, holding the same 60-character name (wider than the
//!   field, so it scrolls and is clipped), focused by a click in its middle with one letter typed
//!   since. `field_type_key` is one letter as a keyboard sends it (the key, its text, the
//!   release): the edit, the line shaped again, the scroll offset, the field and its caret
//!   re-recorded, the damage drawn, the accessibility update. `field_caret_blink` is the wake-up
//!   of the caret's next half: the overlay re-recorded and its small damage drawn. Targets: 0.5 M
//!   instructions for the key, 0.1 M for the blink.
//! - `field_preedit_update` (F2.10c2, ADR-0047 TF6): the same field with an input method's
//!   composition at the caret, "ni", growing to "nih" as the next letter of the pinyin is
//!   typed: the shown text rebuilt with the composition, the line shaped again, the field, its
//!   lines and its caret re-recorded, the window's question about the input method (the
//!   caret's area), the damage drawn, the accessibility update. Target: a key's 0.5 M.
//! - `tooltip_show`, `tooltip_hide` (F2.11c, ADR-0048 AM14–AM15): the real panel (200 widgets)
//!   with, on top, a button 60 pixels wide whose text is shortened with an ellipsis, the pointer
//!   resting on it. `tooltip_show` is the moment its 500 ms come: the tooltip's text copied, laid
//!   out (wrapped at 320 pixels), placed below the pointer, recorded in the top layer and drawn;
//!   `panel` where no tooltip was drawn before (the usual case: a tooltip opens at the pointer,
//!   so its glyphs and box are drawn there for the first time), `again` where the same tooltip
//!   was drawn before. `tooltip_hide` is the pointer moving off the button: the tooltip's segment
//!   recorded empty and its area drawn again. The tooltip's widget already exists (it is created
//!   at the first tooltip and kept). Measured: 2.0 M, 0.23 M and 0.14 M; with the overlay shadow
//!   the first was 15.4 M, which is why the tooltip has none (owner's choice B).
//! - `first_record_under_a_menu` (F2.11c, a precision of ADR-0039 D1): the same panel with a menu
//!   open; one label added to the main interface and recorded for the first time, so its segment
//!   is inserted below the menu's layer (the segments above it move up one place). Target: an
//!   event's 0.5 M.
//! - `shortcut_press` (F2.11d1, ADR-0048 AM4, AM15): the panel of 200 leaves with a registry of
//!   64 actions (62 global, 2 of a view context), the focus on the leaf in the middle of the 20th
//!   row; one Ctrl+S as a keyboard sends it (the press, delivered to the leaf and bubbling
//!   unhandled to the root, the contexts asked on the way back up, the probe, the action's message
//!   queued; then the release, consumed). Nothing is
//!   re-recorded or drawn: the cost is the routing and the lookup.
//! - `registry_lookup` (F2.11d1, AM15): the registry alone, 1 024 probes (half found, half
//!   missing) as a batch (ADR-0017 §4). Budget: under a microsecond per probe, that is well under
//!   a few thousand instructions.
//! - `context_menu_open` (F2.11d2a, ADR-0048 AM12, AM15): the real panel (200 widgets) with a
//!   focused text field below it whose context menu has the eight editing items of the program
//!   (two of them separators), the registry holding their names and shortcuts; one secondary
//!   press on the field (Linux opens on the press): the menu's widgets built with their texts,
//!   laid out, placed under the pointer, recorded in a new layer, drawn, and the accessibility
//!   update (the window node, the menu's nodes, the focus). `fresh` opens where no menu was drawn
//!   before (its glyphs and box drawn there for the first time, the usual case for a menu that
//!   opens at the pointer); `again` where the same menu was drawn before. Measured 4.6 M and
//!   0.71 M; with the overlay shadow they were 27.1 M and 4.0 M, which is why the menus have
//!   none (owner's choice B).
//! - `menu_highlight_move` (F2.11d2a, AM15): the same menu open, one Down as a keyboard sends it
//!   (press and release): the highlight moves, two entries re-recorded and drawn, the
//!   accessibility focus moves. Target: an event's 0.5 M; measured 0.17 M (0.82 M with the
//!   shadow).
//! - `bar_menu_switch` (F2.11d2b, AM12, AM15): the real panel with a menu bar of four titles on
//!   top (File with six items, Edit with seven, View and Help with one), File's menu open; the
//!   pointer moves onto *Edit*: File's menu closes, Edit's opens (its widgets built, laid out,
//!   recorded and drawn), both titles re-recorded, the accessibility update. The cost of opening
//!   a menu where none was drawn before, as `context_menu_open fresh`.
//! - `title_bar_focus_change` (F2.11b3, ADR-0049 TB12): the real panel (200 widgets) under our
//!   own title bar (the four menus, the title, three window buttons) in its frame; the window
//!   loses the focus: the bar and its three buttons re-recorded in `text-muted`, their area drawn
//!   again, nothing else laid out or recorded. Target: an event's 0.5 M.
//!
//! # Targets (ADR-0045, SCALETTA F2.09; F2.09d, choice WF2)
//! An event that touches a few widgets under 0.5 M instructions with drawing (the spike's
//! retained click was 0.17 M). A scroll step redraws the whole viewport, so its budget is a
//! frame's: 33 M instructions, 30 frames per second in software (F2.07c).

#![allow(
    missing_docs,
    clippy::missing_docs_in_private_items,
    unused_qualifications,
    reason = "items generated by Gungraun's macros have no docs and use qualified paths; our own \
              items are documented with plain comments because the macro rejects #[doc]"
)]
#![allow(
    clippy::unwrap_used,
    reason = "setup code with known-valid inputs; never part of the measured code"
)]
#![allow(
    clippy::needless_pass_by_value,
    reason = "Gungraun passes the prepared input by value, outside the measured region"
)]

use std::hint::black_box;

use gungraun::prelude::*;
use orchestra_ui::{
    Appearance, BuiltInTheme, Button, ButtonState, ColorToken, Constraints, CpuRenderer, Density,
    DisplayList, EventCx, Flex, Handled, Image, InputEvent, Label, LayoutCx, LayoutProps, Length,
    Point, PointerButton, RecordCx, Recorder, Rect, Renderer, Size, Slider, Space, Theme, Ui,
    WheelDelta, Widget, WidgetEvent, WidgetId,
};
use orchestra_ui::{Axis, ScrollArea, Splitter, Toggle};
use orchestra_ui::{Direction, EditAction, EditConvention, TextEditor, Unit};
use orchestra_ui::{ImeEvent, Key, Modifiers, NamedKey, Popup, PopupSide, TextField};
use std::time::Instant;

use orchestra_ui::{
    ActionId, ActionSpec, Context, Defaults, EditRole, KeyNames, MenuModel, PhysicalKey, Platform,
    Registry, Shortcut, ShortcutKey,
};

// A button-like leaf whose width can change.
#[derive(Debug)]
struct Pad {
    width: f32,
    on: bool,
}

impl Widget<()> for Pad {
    fn measure(&mut self, _cx: &mut LayoutCx<'_, ()>, constraints: Constraints) -> Size {
        constraints.constrain(Size::new(self.width, 20.0))
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
        if let WidgetEvent::PointerButton { state, .. } = event {
            if *state == ButtonState::Pressed {
                self.on = !self.on;
                cx.emit(());
                cx.request_record();
            }
            return Handled::Yes;
        }
        Handled::No
    }
}

fn theme() -> Theme {
    *BuiltInTheme::Studio.pair().instance(Appearance::Light)
}

const BOUNDS: Rect = Rect::new(0.0, 0.0, 1000.0, 1000.0);

struct Panel {
    ui: Ui<()>,
    list: DisplayList,
    renderer: CpuRenderer,
    image: Image,
    leaves: Vec<WidgetId>,
}

// The panel built, not yet laid out.
fn built() -> Panel {
    let mut ui = Ui::new(
        Flex::column(Space::Xs, Space::M),
        &theme(),
        orchestra_text::Faces::bundled().unwrap(),
    );
    let root = ui.root();
    let mut leaves = Vec::new();
    for _ in 0..40 {
        let row = ui
            .add(
                root,
                Flex::row(Space::S, Space::None),
                LayoutProps::default(),
            )
            .unwrap();
        for _ in 0..5 {
            leaves.push(
                ui.add(
                    row,
                    Pad {
                        width: 120.0,
                        on: false,
                    },
                    LayoutProps::default(),
                )
                .unwrap(),
            );
        }
    }
    Panel {
        ui,
        list: DisplayList::new(),
        renderer: CpuRenderer::new(orchestra_text::Faces::bundled().unwrap()),
        image: Image::new(1000, 1000).unwrap(),
        leaves,
    }
}

// The panel shown once (laid out, recorded, drawn), its first messages drained.
fn shown() -> Panel {
    let mut panel = built();
    panel.ui.layout(BOUNDS, &mut panel.list);
    draw(&mut panel);
    panel
}

fn draw(panel: &mut Panel) {
    let damage = panel.list.take_damage();
    panel
        .renderer
        .render(&panel.list, &theme(), &damage, 1.0, &mut panel.image)
        .unwrap();
}

#[library_benchmark]
#[bench::panel(built())]
fn first_frame(mut panel: Panel) -> Panel {
    panel.ui.layout(black_box(BOUNDS), &mut panel.list);
    draw(&mut panel);
    panel
}

#[library_benchmark]
#[bench::panel(shown())]
fn click(mut panel: Panel) -> Panel {
    // The leaf in the middle of the 20th row.
    let rect = panel.ui.rect(panel.leaves[97]).unwrap();
    let position = Point::new(
        f32::midpoint(rect.x0, rect.x1),
        f32::midpoint(rect.y0, rect.y1),
    );
    for state in [ButtonState::Pressed, ButtonState::Released] {
        panel.ui.input(
            black_box(&InputEvent::PointerButton {
                button: PointerButton::Left,
                state,
                position,
                clicks: 1,
            }),
            &mut panel.list,
        );
    }
    black_box(panel.ui.take_messages().count());
    draw(&mut panel);
    panel
}

#[library_benchmark]
#[bench::panel(shown())]
fn grow_one(mut panel: Panel) -> Panel {
    let leaf = panel.leaves[97];
    if let Some(pad) = panel.ui.widget_mut::<Pad>(leaf) {
        pad.width = 160.0;
    }
    panel.ui.request_layout(black_box(leaf));
    panel.ui.frame(&mut panel.list);
    draw(&mut panel);
    panel
}

#[library_benchmark]
#[bench::panel(shown())]
fn relayout_all(mut panel: Panel) -> Panel {
    panel
        .ui
        .set_theme(black_box(&theme().with_density(Density::Compact)));
    panel.ui.frame(&mut panel.list);
    panel
}

// The same panel with real widgets (F2.09b): each row a label and four buttons, all with text,
// laid out, recorded, drawn once and its accessibility tree handed out once.
struct Real {
    ui: Ui<usize>,
    list: DisplayList,
    renderer: CpuRenderer,
    image: Image,
    buttons: Vec<WidgetId>,
}

fn real_built() -> Real {
    let faces = orchestra_text::Faces::bundled().unwrap();
    let mut ui = Ui::new(Flex::column(Space::Xs, Space::M), &theme(), faces.clone());
    ui.set_title("Benchmark");
    let root = ui.root();
    let mut buttons = Vec::new();
    for row_index in 0..40 {
        let row = ui
            .add(
                root,
                Flex::row(Space::S, Space::None),
                LayoutProps::default(),
            )
            .unwrap();
        ui.add(
            row,
            Label::new(format!("Track {row_index}: Violin I")),
            LayoutProps::default(),
        )
        .unwrap();
        for column in 0..4 {
            let id = ui
                .add(
                    row,
                    Button::new(format!("Action {column}"), row_index * 4 + column),
                    LayoutProps::default(),
                )
                .unwrap();
            buttons.push(id);
        }
    }
    Real {
        ui,
        list: DisplayList::new(),
        renderer: CpuRenderer::new(faces),
        image: Image::new(1000, 1600).unwrap(),
        buttons,
    }
}

fn real_shown() -> Real {
    let mut real = real_built();
    real.ui
        .layout(Rect::new(0.0, 0.0, 1000.0, 1600.0), &mut real.list);
    real_draw(&mut real);
    let _ = real.ui.take_access_update();
    real
}

fn real_draw(real: &mut Real) {
    let damage = real.list.take_damage();
    real.renderer
        .render(&real.list, &theme(), &damage, 1.0, &mut real.image)
        .unwrap();
}

#[library_benchmark]
#[bench::panel(real_built())]
fn real_first_frame(mut real: Real) -> Real {
    real.ui.layout(
        black_box(Rect::new(0.0, 0.0, 1000.0, 1600.0)),
        &mut real.list,
    );
    real_draw(&mut real);
    black_box(real.ui.take_access_update());
    real
}

#[library_benchmark]
#[bench::panel(real_shown())]
fn real_click(mut real: Real) -> Real {
    let rect = real.ui.rect(real.buttons[77]).unwrap();
    let position = Point::new(
        f32::midpoint(rect.x0, rect.x1),
        f32::midpoint(rect.y0, rect.y1),
    );
    real.ui
        .input(&InputEvent::PointerMoved { position }, &mut real.list);
    for state in [ButtonState::Pressed, ButtonState::Released] {
        real.ui.input(
            black_box(&InputEvent::PointerButton {
                button: PointerButton::Left,
                state,
                position,
                clicks: 1,
            }),
            &mut real.list,
        );
    }
    black_box(real.ui.take_messages().count());
    real_draw(&mut real);
    black_box(real.ui.take_access_update());
    real
}

#[library_benchmark]
#[bench::panel(real_shown())]
fn real_access_full(mut real: Real) -> Real {
    real.ui.reset_access();
    black_box(real.ui.take_access_update());
    real
}

library_benchmark_group!(
    name = core,
    benchmarks = [first_frame, click, grow_one, relayout_all]
);

// 40 rows of a label and a slider, shown once, with the 20th slider being dragged (pressed and
// captured): one move of the drag is measured (F2.09c).
struct Sliders {
    ui: Ui<f64>,
    list: DisplayList,
    renderer: CpuRenderer,
    image: Image,
    target: Rect,
}

fn sliders_dragging() -> Sliders {
    let faces = orchestra_text::Faces::bundled().unwrap();
    let mut ui = Ui::new(Flex::column(Space::Xs, Space::M), &theme(), faces.clone());
    let root = ui.root();
    let mut target = None;
    for row_index in 0..40 {
        let row = ui
            .add(
                root,
                Flex::row(Space::M, Space::None),
                LayoutProps::default().width(Length::Fixed(600.0)),
            )
            .unwrap();
        ui.add(
            row,
            Label::new(format!("Track {row_index} volume")),
            LayoutProps::default(),
        )
        .unwrap();
        let slider = ui
            .add(
                row,
                Slider::new(0.0, 100.0, 1.0, 50.0, |value| value).labelled("Volume"),
                LayoutProps::default().width(Length::Fill(1.0)),
            )
            .unwrap();
        if row_index == 20 {
            target = Some(slider);
        }
    }
    let mut list = DisplayList::new();
    ui.layout(Rect::new(0.0, 0.0, 1000.0, 1600.0), &mut list);
    let target = ui.rect(target.unwrap()).unwrap();
    let mut renderer = CpuRenderer::new(faces);
    let mut image = Image::new(1000, 1600).unwrap();
    let damage = list.take_damage();
    renderer
        .render(&list, &theme(), &damage, 1.0, &mut image)
        .unwrap();
    let _ = ui.take_access_update();
    let y = f32::midpoint(target.y0, target.y1);
    ui.input(
        &InputEvent::PointerButton {
            button: PointerButton::Left,
            state: ButtonState::Pressed,
            position: Point::new(f32::midpoint(target.x0, target.x1), y),
            clicks: 1,
        },
        &mut list,
    );
    let damage = list.take_damage();
    renderer
        .render(&list, &theme(), &damage, 1.0, &mut image)
        .unwrap();
    let _ = ui.take_access_update();
    let _ = ui.take_messages().count();
    Sliders {
        ui,
        list,
        renderer,
        image,
        target,
    }
}

#[library_benchmark]
#[bench::panel(sliders_dragging())]
fn slider_drag_move(mut sliders: Sliders) -> Sliders {
    let x = sliders.target.x0 + sliders.target.width() * 0.7;
    let y = f32::midpoint(sliders.target.y0, sliders.target.y1);
    sliders.ui.input(
        black_box(&InputEvent::PointerMoved {
            position: Point::new(x, y),
        }),
        &mut sliders.list,
    );
    black_box(sliders.ui.take_messages().count());
    let damage = sliders.list.take_damage();
    sliders
        .renderer
        .render(&sliders.list, &theme(), &damage, 1.0, &mut sliders.image)
        .unwrap();
    black_box(sliders.ui.take_access_update());
    sliders
}

// The same rows with the 20th slider hovered, not dragged: its press is released, so the wheel
// finds it under the pointer (F2.09c2).
fn sliders_hovered() -> Sliders {
    let mut sliders = sliders_dragging();
    let y = f32::midpoint(sliders.target.y0, sliders.target.y1);
    sliders.ui.input(
        &InputEvent::PointerButton {
            button: PointerButton::Left,
            state: ButtonState::Released,
            position: Point::new(f32::midpoint(sliders.target.x0, sliders.target.x1), y),
            clicks: 1,
        },
        &mut sliders.list,
    );
    let damage = sliders.list.take_damage();
    sliders
        .renderer
        .render(&sliders.list, &theme(), &damage, 1.0, &mut sliders.image)
        .unwrap();
    let _ = sliders.ui.take_access_update();
    sliders
}

#[library_benchmark]
#[bench::panel(sliders_hovered())]
fn slider_wheel_notch(mut sliders: Sliders) -> Sliders {
    sliders.ui.input(
        black_box(&InputEvent::Wheel {
            delta: WheelDelta::Lines { x: 0.0, y: 1.0 },
        }),
        &mut sliders.list,
    );
    black_box(sliders.ui.take_messages().count());
    let damage = sliders.list.take_damage();
    sliders
        .renderer
        .render(&sliders.list, &theme(), &damage, 1.0, &mut sliders.image)
        .unwrap();
    black_box(sliders.ui.take_access_update());
    sliders
}

library_benchmark_group!(
    name = controls,
    benchmarks = [
        real_first_frame,
        real_click,
        real_access_full,
        slider_drag_move,
        slider_wheel_notch
    ]
);

// The real panel inside a vertical scroll area, the right pane of a splitter (an empty column on
// the left), in a 1000 × 1000 window: shown once (drawn, accessibility tree handed out), the
// pointer over the content.
struct Scrolled {
    ui: Ui<usize>,
    list: DisplayList,
    renderer: CpuRenderer,
    image: Image,
    band: Point,
}

fn scrolled_shown() -> Scrolled {
    let faces = orchestra_text::Faces::bundled().unwrap();
    let mut ui = Ui::new(
        Flex::column(Space::None, Space::None),
        &theme(),
        faces.clone(),
    );
    ui.set_title("Benchmark");
    let root = ui.root();
    let fill = LayoutProps::default()
        .width(Length::Fill(1.0))
        .height(Length::Fill(1.0));
    let splitter = ui
        .add(root, Splitter::new(Axis::Horizontal, 0.2), fill)
        .unwrap();
    ui.add(splitter, Flex::column(Space::None, Space::None), fill)
        .unwrap();
    let scroll = ui.add(splitter, ScrollArea::vertical(), fill).unwrap();
    let column = ui
        .add(
            scroll,
            Flex::column(Space::Xs, Space::M),
            LayoutProps::default(),
        )
        .unwrap();
    for row_index in 0..40 {
        let row = ui
            .add(
                column,
                Flex::row(Space::S, Space::None),
                LayoutProps::default(),
            )
            .unwrap();
        ui.add(
            row,
            Label::new(format!("Track {row_index}: Violin I")),
            LayoutProps::default(),
        )
        .unwrap();
        for button in 0..4 {
            ui.add(
                row,
                Button::new(format!("Action {button}"), row_index * 4 + button),
                LayoutProps::default(),
            )
            .unwrap();
        }
    }
    let mut list = DisplayList::new();
    ui.layout(BOUNDS, &mut list);
    let mut renderer = CpuRenderer::new(faces);
    let mut image = Image::new(1000, 1000).unwrap();
    let damage = list.take_damage();
    renderer
        .render(&list, &theme(), &damage, 1.0, &mut image)
        .unwrap();
    let _ = ui.take_access_update();
    let area = ui.rect(scroll).unwrap();
    let band = Point::new(area.x0 - 4.0, 500.0);
    ui.input(
        &InputEvent::PointerMoved {
            position: Point::new(area.x0 + 100.0, 500.0),
        },
        &mut list,
    );
    let damage = list.take_damage();
    renderer
        .render(&list, &theme(), &damage, 1.0, &mut image)
        .unwrap();
    let _ = ui.take_access_update();
    Scrolled {
        ui,
        list,
        renderer,
        image,
        band,
    }
}

// The same, with the divider pressed (captured), ready to be dragged.
fn scrolled_dragging() -> Scrolled {
    let mut scrolled = scrolled_shown();
    let band = scrolled.band;
    scrolled.ui.input(
        &InputEvent::PointerMoved { position: band },
        &mut scrolled.list,
    );
    scrolled.ui.input(
        &InputEvent::PointerButton {
            button: PointerButton::Left,
            state: ButtonState::Pressed,
            position: band,
            clicks: 1,
        },
        &mut scrolled.list,
    );
    scrolled_draw(&mut scrolled);
    let _ = scrolled.ui.take_access_update();
    scrolled
}

fn scrolled_draw(scrolled: &mut Scrolled) {
    let damage = scrolled.list.take_damage();
    scrolled
        .renderer
        .render(&scrolled.list, &theme(), &damage, 1.0, &mut scrolled.image)
        .unwrap();
}

#[library_benchmark]
#[bench::panel(scrolled_shown())]
fn scroll_wheel_notch(mut scrolled: Scrolled) -> Scrolled {
    scrolled.ui.input(
        black_box(&InputEvent::Wheel {
            delta: WheelDelta::Lines { x: 0.0, y: -1.0 },
        }),
        &mut scrolled.list,
    );
    scrolled_draw(&mut scrolled);
    black_box(scrolled.ui.take_access_update());
    scrolled
}

#[library_benchmark]
#[bench::panel(scrolled_dragging())]
fn splitter_drag_move(mut scrolled: Scrolled) -> Scrolled {
    let to = Point::new(scrolled.band.x + 30.0, scrolled.band.y);
    scrolled.ui.input(
        black_box(&InputEvent::PointerMoved { position: to }),
        &mut scrolled.list,
    );
    scrolled_draw(&mut scrolled);
    black_box(scrolled.ui.take_access_update());
    scrolled
}

// A window whose left pane holds 40 labels and 40 switches all wider than it (F2.09f), with the
// divider pressed, ready to be dragged: every text is already drawn shortened.
fn eliding_dragging() -> Scrolled {
    let faces = orchestra_text::Faces::bundled().unwrap();
    let mut ui = Ui::new(
        Flex::column(Space::None, Space::None),
        &theme(),
        faces.clone(),
    );
    ui.set_title("Benchmark");
    let root = ui.root();
    let fill = LayoutProps::default()
        .width(Length::Fill(1.0))
        .height(Length::Fill(1.0));
    let splitter = ui
        .add(root, Splitter::new(Axis::Horizontal, 0.15), fill)
        .unwrap();
    let scroll = ui.add(splitter, ScrollArea::vertical(), fill).unwrap();
    ui.add(splitter, Flex::column(Space::None, Space::None), fill)
        .unwrap();
    let column = ui
        .add(
            scroll,
            Flex::column(Space::Xs, Space::M),
            LayoutProps::default().width(Length::Fill(1.0)),
        )
        .unwrap();
    for row_index in 0..40 {
        ui.add(
            column,
            Label::new(format!("Track {row_index}: Violin I, solo, con sordino")),
            LayoutProps::default(),
        )
        .unwrap();
        ui.add(
            column,
            Toggle::switch(
                format!("Follow the playback cursor on track {row_index}"),
                row_index % 2 == 0,
                |_| 0,
            ),
            LayoutProps::default(),
        )
        .unwrap();
    }
    let mut list = DisplayList::new();
    ui.layout(BOUNDS, &mut list);
    let mut renderer = CpuRenderer::new(faces);
    let mut image = Image::new(1000, 1000).unwrap();
    let damage = list.take_damage();
    renderer
        .render(&list, &theme(), &damage, 1.0, &mut image)
        .unwrap();
    let _ = ui.take_access_update();
    let area = ui.rect(scroll).unwrap();
    let mut scrolled = Scrolled {
        ui,
        list,
        renderer,
        image,
        band: Point::new(area.x1 + 4.0, 500.0),
    };
    // Dragged once to the measured position and back, drawn each time: the shortened texts'
    // glyphs are then in the glyph cache at the positions the measured move gives them, and the
    // shaper's working memory has its size (the steady state of a drag).
    let band = scrolled.band;
    let moved = |scrolled: &mut Scrolled, position: Point| {
        scrolled
            .ui
            .input(&InputEvent::PointerMoved { position }, &mut scrolled.list);
        scrolled_draw(scrolled);
        let _ = scrolled.ui.take_access_update();
    };
    moved(&mut scrolled, band);
    scrolled.ui.input(
        &InputEvent::PointerButton {
            button: PointerButton::Left,
            state: ButtonState::Pressed,
            position: band,
            clicks: 1,
        },
        &mut scrolled.list,
    );
    moved(&mut scrolled, Point::new(band.x - 30.0, band.y));
    moved(&mut scrolled, band);
    scrolled
}

#[library_benchmark]
#[bench::panel(eliding_dragging())]
fn splitter_drag_eliding(mut scrolled: Scrolled) -> Scrolled {
    let to = Point::new(scrolled.band.x - 30.0, scrolled.band.y);
    scrolled.ui.input(
        black_box(&InputEvent::PointerMoved { position: to }),
        &mut scrolled.list,
    );
    scrolled_draw(&mut scrolled);
    black_box(scrolled.ui.take_access_update());
    scrolled
}

// One question of the pointer's shape (F2.09e), as the window asks after each event. The answer
// is the arrow: nobody on the way up asks for a shape, so every ancestor is asked.
#[library_benchmark]
#[bench::panel(scrolled_shown())]
fn pointer_shape_over_a_widget(scrolled: Scrolled) -> Scrolled {
    black_box(scrolled.ui.pointer_shape());
    scrolled
}

library_benchmark_group!(
    name = scrolling,
    benchmarks = [
        scroll_wheel_notch,
        splitter_drag_move,
        splitter_drag_eliding,
        pointer_shape_over_a_widget
    ]
);

// The editing core (F2.10b1): a 60-character name, the caret in the middle, an open typing group
// (one letter typed before), capacity reserved as a field does.
fn editor_ready() -> TextEditor {
    let mut editor = TextEditor::new(
        "Violin I, solo, con sordino - tutti divisi, sul ponticello mp",
        EditConvention::Linux,
    );
    editor.reserve(256, 4096, 256);
    editor.select(30, 30);
    editor.type_text("a");
    editor
}

#[library_benchmark]
#[bench::name(editor_ready())]
fn edit_type_key(mut editor: TextEditor) -> TextEditor {
    black_box(editor.type_text(black_box("b")));
    editor
}

#[library_benchmark]
#[bench::name(editor_ready())]
fn edit_word_move(mut editor: TextEditor) -> TextEditor {
    black_box(editor.apply(black_box(EditAction::Move {
        direction: Direction::Forward,
        unit: Unit::Word,
        extend: false,
    })));
    editor
}

// The real panel with a text field on top, focused, with one letter typed (F2.10b2).
struct Fielded {
    ui: Ui<usize>,
    list: DisplayList,
    renderer: CpuRenderer,
    image: Image,
    // When the caret's next half starts: the blink benchmark advances to it.
    wakeup: Instant,
}

fn fielded_shown() -> Fielded {
    let faces = orchestra_text::Faces::bundled().unwrap();
    let mut ui = Ui::new(Flex::column(Space::Xs, Space::M), &theme(), faces.clone());
    ui.set_title("Benchmark");
    let root = ui.root();
    let mut field = TextField::new(
        "Violin I, solo, con sordino - tutti divisi, sul ponticello mp",
        |_| 0,
    )
    .convention(EditConvention::Linux);
    field.reserve(256, 4096, 256);
    let field = ui
        .add(
            root,
            field,
            LayoutProps::default().width(Length::Fixed(300.0)),
        )
        .unwrap();
    for row_index in 0..40 {
        let row = ui
            .add(
                root,
                Flex::row(Space::S, Space::None),
                LayoutProps::default(),
            )
            .unwrap();
        ui.add(
            row,
            Label::new(format!("Track {row_index}: Violin I")),
            LayoutProps::default(),
        )
        .unwrap();
        for column in 0..4 {
            ui.add(
                row,
                Button::new(format!("Action {column}"), row_index * 4 + column),
                LayoutProps::default(),
            )
            .unwrap();
        }
    }
    let mut list = DisplayList::new();
    let start = Instant::now();
    ui.advance(start, &mut list);
    ui.layout(Rect::new(0.0, 0.0, 1000.0, 1600.0), &mut list);
    let rect = ui.rect(field).unwrap();
    let middle = Point::new(
        f32::midpoint(rect.x0, rect.x1),
        f32::midpoint(rect.y0, rect.y1),
    );
    ui.input(&InputEvent::PointerMoved { position: middle }, &mut list);
    for state in [ButtonState::Pressed, ButtonState::Released] {
        ui.input(
            &InputEvent::PointerButton {
                button: PointerButton::Left,
                state,
                position: middle,
                clicks: 1,
            },
            &mut list,
        );
    }
    for event in typed_key("a") {
        ui.input(&event, &mut list);
    }
    // The measured letter typed and deleted once, drawn each time: its glyph and the glyphs it
    // pushes along are then in the glyph cache at the very positions the measured key gives
    // them, the steady state of typing (a letter's first appearance at a new quarter-pixel
    // phase rasterises it once, about 0.14 M instructions measured, outside this budget).
    let mut renderer = CpuRenderer::new(faces.clone());
    let mut image = Image::new(1000, 1600).unwrap();
    let backspace = InputEvent::Key {
        key: Key::Named(NamedKey::Backspace),
        physical: orchestra_ui::PhysicalKey::Other,
        state: ButtonState::Pressed,
        repeat: false,
        modifiers: Modifiers::default(),
    };
    for event in [typed_key("b").as_slice(), &[backspace]].concat() {
        ui.input(&event, &mut list);
        let damage = list.take_damage();
        renderer
            .render(&list, &theme(), &damage, 1.0, &mut image)
            .unwrap();
    }
    let mut fielded = Fielded {
        wakeup: ui.next_wakeup().unwrap(),
        ui,
        list,
        renderer,
        image,
    };
    let damage = fielded.list.take_damage();
    fielded
        .renderer
        .render(&fielded.list, &theme(), &damage, 1.0, &mut fielded.image)
        .unwrap();
    let _ = fielded.ui.take_access_update();
    fielded
}

// A letter as a keyboard sends it: the key, its text, the release.
fn typed_key(letter: &str) -> [InputEvent; 3] {
    let key = |state| InputEvent::Key {
        key: Key::Character(letter.to_owned()),
        physical: orchestra_ui::PhysicalKey::Other,
        state,
        repeat: false,
        modifiers: Modifiers::default(),
    };
    [
        key(ButtonState::Pressed),
        InputEvent::Text(letter.to_owned()),
        key(ButtonState::Released),
    ]
}

fn fielded_draw(fielded: &mut Fielded) {
    let damage = fielded.list.take_damage();
    fielded
        .renderer
        .render(&fielded.list, &theme(), &damage, 1.0, &mut fielded.image)
        .unwrap();
}

#[library_benchmark]
#[bench::panel(setup = fielded_typing)]
fn field_type_key(input: (Fielded, [InputEvent; 3])) -> Fielded {
    let (mut fielded, events) = input;
    for event in &events {
        fielded.ui.input(black_box(event), &mut fielded.list);
    }
    black_box(fielded.ui.take_messages().count());
    fielded_draw(&mut fielded);
    black_box(fielded.ui.take_access_update());
    fielded
}

// The panel and the events of one letter, built outside the measured region.
fn fielded_typing() -> (Fielded, [InputEvent; 3]) {
    (fielded_shown(), typed_key("b"))
}

#[library_benchmark]
#[bench::panel(fielded_shown())]
fn field_caret_blink(mut fielded: Fielded) -> Fielded {
    let at = fielded.wakeup;
    fielded.ui.advance(black_box(at), &mut fielded.list);
    fielded_draw(&mut fielded);
    fielded
}

// A composition of the input method, as `winit` gives it: the text with the caret at its end.
fn preedit(text: &str) -> InputEvent {
    InputEvent::Ime(ImeEvent::Preedit {
        text: text.to_owned(),
        cursor: Some(text.len()..text.len()),
    })
}

// The field with the composition "ni" at its caret, drawn, and the next composition "nih" built
// outside the measured region. "nih" was shown and drawn once before, so its glyphs are in the
// cache at the positions the measured update gives them (the steady state, as for a typed key).
fn fielded_composing() -> (Fielded, InputEvent) {
    let mut fielded = fielded_shown();
    for event in [preedit("n"), preedit("ni"), preedit("nih"), preedit("ni")] {
        fielded.ui.input(&event, &mut fielded.list);
        let _ = fielded.ui.text_input();
        fielded_draw(&mut fielded);
    }
    let _ = fielded.ui.take_access_update();
    (fielded, preedit("nih"))
}

#[library_benchmark]
#[bench::panel(setup = fielded_composing)]
fn field_preedit_update(input: (Fielded, InputEvent)) -> Fielded {
    let (mut fielded, event) = input;
    fielded.ui.input(black_box(&event), &mut fielded.list);
    black_box(fielded.ui.text_input());
    fielded_draw(&mut fielded);
    black_box(fielded.ui.take_access_update());
    fielded
}

library_benchmark_group!(
    name = editing,
    benchmarks = [
        edit_type_key,
        edit_word_move,
        field_type_key,
        field_caret_blink,
        field_preedit_update
    ]
);

// The real panel with an elided button on top and the pointer resting on it (F2.11c): one
// tooltip already shown and hidden, so the tooltip's widget exists; `due` is when the pointer's
// second rest shows it again.
struct Tipped {
    real: Real,
    due: Instant,
    over: Point,
}

fn tipped_resting_drawn(drawn: bool) -> Tipped {
    let mut real = real_built();
    let root = real.ui.root();
    let tipped = real
        .ui
        .add(
            root,
            Button::new("Restore every option to its default value", usize::MAX),
            LayoutProps::default().width(Length::Fixed(60.0)),
        )
        .unwrap();
    real.ui
        .layout(Rect::new(0.0, 0.0, 1000.0, 1600.0), &mut real.list);
    let rect = real.ui.rect(tipped).unwrap();
    let over = Point::new(
        f32::midpoint(rect.x0, rect.x1),
        f32::midpoint(rect.y0, rect.y1),
    );
    let start = real.ui.now();
    let away = Point::new(900.0, 1500.0);
    // First rest: the tooltip's widget is created; then off and back after the warm time.
    let rest = |real: &mut Real, at: u64| {
        real.ui
            .input(&InputEvent::PointerMoved { position: over }, &mut real.list);
        real.ui.advance(
            start + std::time::Duration::from_millis(at + 500),
            &mut real.list,
        );
        // Drawn: the renderer has met this tooltip's shadow at this place before.
        if drawn {
            real_draw(real);
        }
        real.ui
            .input(&InputEvent::PointerMoved { position: away }, &mut real.list);
        real.ui.advance(
            start + std::time::Duration::from_millis(at + 2000),
            &mut real.list,
        );
    };
    rest(&mut real, 0);
    rest(&mut real, 3000);
    real.ui
        .input(&InputEvent::PointerMoved { position: over }, &mut real.list);
    real_draw(&mut real);
    let _ = real.ui.take_access_update();
    let due = real.ui.next_wakeup().unwrap();
    Tipped { real, due, over }
}

// The pointer resting where no tooltip was drawn before: the usual case, as the tooltip opens
// at the pointer.
fn tipped_resting() -> Tipped {
    tipped_resting_drawn(false)
}

// The pointer resting where the same tooltip was drawn before.
fn tipped_resting_again() -> Tipped {
    tipped_resting_drawn(true)
}

fn tipped_shown() -> Tipped {
    let mut tipped = tipped_resting();
    tipped.real.ui.advance(tipped.due, &mut tipped.real.list);
    assert!(tipped.real.ui.tooltip_text().is_some());
    real_draw(&mut tipped.real);
    let _ = tipped.real.ui.take_access_update();
    tipped
}

#[library_benchmark]
#[bench::panel(tipped_resting())]
#[bench::again(tipped_resting_again())]
fn tooltip_show(mut tipped: Tipped) -> Tipped {
    tipped
        .real
        .ui
        .advance(black_box(tipped.due), &mut tipped.real.list);
    real_draw(&mut tipped.real);
    black_box(tipped.real.ui.take_access_update());
    tipped
}

#[library_benchmark]
#[bench::panel(tipped_shown())]
fn tooltip_hide(mut tipped: Tipped) -> Tipped {
    let position = Point::new(tipped.over.x + 200.0, tipped.over.y);
    tipped.real.ui.input(
        black_box(&InputEvent::PointerMoved { position }),
        &mut tipped.real.list,
    );
    real_draw(&mut tipped.real);
    black_box(tipped.real.ui.take_access_update());
    tipped
}

// The real panel with a menu of ten labels open near the top left (F2.11c).
fn menu_open() -> Real {
    let mut real = real_shown();
    let menu = real
        .ui
        .open_menu(
            Flex::column(Space::None, Space::None),
            Popup {
                anchor: Rect::new(10.0, 30.0, 10.0, 30.0),
                side: PopupSide::Below,
                owner: None,
            },
            &mut real.list,
        )
        .unwrap();
    for item in 0..10 {
        real.ui
            .add(
                menu,
                Label::new(format!("Menu item {item}")),
                LayoutProps::default(),
            )
            .unwrap();
    }
    real.ui.frame(&mut real.list);
    real_draw(&mut real);
    let _ = real.ui.take_access_update();
    real
}

#[library_benchmark]
#[bench::panel(menu_open())]
fn first_record_under_a_menu(mut real: Real) -> Real {
    let root = real.ui.root();
    real.ui
        .add(
            root,
            black_box(Label::new("Track 40: Viola")),
            LayoutProps::default(),
        )
        .unwrap();
    real.ui.frame(&mut real.list);
    real_draw(&mut real);
    black_box(real.ui.take_access_update());
    real
}

library_benchmark_group!(
    name = popups,
    benchmarks = [tooltip_show, tooltip_hide, first_record_under_a_menu]
);

// A registry of 62 global actions (Ctrl+letter, Ctrl+Shift+letter, Ctrl+digit), plus two of a
// view context: the size of a real program's global table.
fn bench_registry() -> (Registry, Context) {
    let mut registry = Registry::new(Platform::Linux);
    let view = registry.add_context("view").unwrap();
    let primary = Modifiers {
        primary: true,
        ..Modifiers::default()
    };
    let shifted = Modifiers {
        shift: true,
        ..primary
    };
    let mut keys = Vec::new();
    for letter in 'a'..='z' {
        keys.push(Shortcut::new(primary, ShortcutKey::Character(letter)).unwrap());
        keys.push(Shortcut::new(shifted, ShortcutKey::Character(letter)).unwrap());
    }
    for digit in 0..=9 {
        keys.push(Shortcut::new(primary, ShortcutKey::Digit(digit)).unwrap());
    }
    // Identifiers are `&'static str`: leaked once, in the setup.
    for (index, shortcut) in keys.into_iter().take(62).enumerate() {
        let identifier: &'static str = Box::leak(format!("bench.action-{index}").into_boxed_str());
        registry
            .register(ActionSpec {
                identifier,
                name: "action-test",
                context: Context::GLOBAL,
                defaults: Defaults::everywhere(&[shortcut]),
                repeatable: false,
                edit: None,
            })
            .unwrap();
    }
    for (identifier, letter) in [("view.first", 'f'), ("view.second", 'g')] {
        registry
            .register(ActionSpec {
                identifier,
                name: "action-test",
                context: view,
                defaults: Defaults::everywhere(&[Shortcut::new(
                    Modifiers::default(),
                    ShortcutKey::Character(letter),
                )
                .unwrap()]),
                repeatable: false,
                edit: None,
            })
            .unwrap();
    }
    (registry, view)
}

// The panel with the registry handed in and the focus on a leaf (F2.11d1).
fn shortcut_ready() -> Panel {
    let mut panel = shown();
    let (registry, _view) = bench_registry();
    panel.ui.set_actions(registry, |_: ActionId| ());
    let leaf = panel.leaves[97];
    panel.ui.set_focus(Some(leaf), &mut panel.list).unwrap();
    draw(&mut panel);
    panel
}

#[library_benchmark]
#[bench::panel(shortcut_ready())]
fn shortcut_press(mut panel: Panel) -> Panel {
    let modifiers = Modifiers {
        primary: true,
        ..Modifiers::default()
    };
    for state in [ButtonState::Pressed, ButtonState::Released] {
        panel.ui.input(
            black_box(&InputEvent::Key {
                key: Key::Character("s".into()),
                physical: PhysicalKey::Letter(18),
                state,
                repeat: false,
                modifiers,
            }),
            &mut panel.list,
        );
    }
    black_box(panel.ui.take_messages().count());
    panel
}

// The registry and 1 024 probes, prepared outside the measured region.
fn lookup_ready() -> (Registry, Vec<(Context, Shortcut)>) {
    let (registry, view) = bench_registry();
    let primary = Modifiers {
        primary: true,
        ..Modifiers::default()
    };
    let alt = Modifiers {
        alt: true,
        ..primary
    };
    let mut probes = Vec::with_capacity(1024);
    for index in 0..1024_u32 {
        let letter = char::from(b'a' + u8::try_from(index % 26).unwrap());
        // Even probes hit (Ctrl+letter, global), odd ones miss (Ctrl+Alt+letter, in the view).
        let probe = if index % 2 == 0 {
            (
                Context::GLOBAL,
                Shortcut::new(primary, ShortcutKey::Character(letter)),
            )
        } else {
            (view, Shortcut::new(alt, ShortcutKey::Character(letter)))
        };
        probes.push((probe.0, probe.1.unwrap()));
    }
    (registry, probes)
}

#[library_benchmark]
#[bench::batch(lookup_ready())]
fn registry_lookup(input: (Registry, Vec<(Context, Shortcut)>)) -> usize {
    let (registry, probes) = input;
    let mut found = 0;
    for &(context, shortcut) in &probes {
        if registry
            .lookup(black_box(context), black_box(shortcut))
            .is_some()
        {
            found += 1;
        }
    }
    found
}

library_benchmark_group!(
    name = actions,
    benchmarks = [shortcut_press, registry_lookup]
);

// The real panel with a focused text field below it whose context menu has the program's eight
// editing items, and a registry with their names and shortcuts (F2.11d2a). With `drawn_before`
// the same menu was opened, drawn and closed at the same place first, so the renderer met its
// shadow there. The pointer rests where the secondary press will come.
struct Menued {
    real: Real,
    at: Point,
}

fn menued(drawn_before: bool) -> Menued {
    let mut real = real_shown();
    let primary = Modifiers {
        primary: true,
        ..Modifiers::default()
    };
    let shifted = Modifiers {
        shift: true,
        ..primary
    };
    let mut registry = Registry::new(Platform::Linux);
    let mut menu = MenuModel::new();
    let editing = [
        ("edit.undo", "Undo", primary, 'z', EditRole::Undo),
        ("edit.redo", "Redo", shifted, 'z', EditRole::Redo),
        ("edit.cut", "Cut", primary, 'x', EditRole::Cut),
        ("edit.copy", "Copy", primary, 'c', EditRole::Copy),
        ("edit.paste", "Paste", primary, 'v', EditRole::Paste),
        (
            "edit.select-all",
            "Select all",
            primary,
            'a',
            EditRole::SelectAll,
        ),
    ];
    for (index, (identifier, name, modifiers, key, role)) in editing.into_iter().enumerate() {
        let id = registry
            .register(ActionSpec {
                identifier,
                name,
                context: Context::GLOBAL,
                defaults: Defaults::everywhere(&[Shortcut::new(
                    modifiers,
                    ShortcutKey::Character(key),
                )
                .unwrap()]),
                repeatable: false,
                edit: Some(role),
            })
            .unwrap();
        menu = menu.action(id);
        // Undo, Redo | Cut, Copy, Paste | Select all.
        if index == 1 || index == 4 {
            menu = menu.separator();
        }
    }
    registry.set_texts(KeyNames::new(|key| format!("{key:?}")), str::to_owned);
    real.ui
        .set_actions(registry, |id: ActionId| usize::from(id.get()));
    let root = real.ui.root();
    let field = real
        .ui
        .add(
            root,
            TextField::new("Violin I", |_| 0).with_context_menu(menu),
            LayoutProps::default().width(Length::Fixed(300.0)),
        )
        .unwrap();
    real.ui
        .layout(Rect::new(0.0, 0.0, 1000.0, 1600.0), &mut real.list);
    real.ui.set_focus(Some(field), &mut real.list).unwrap();
    let rect = real.ui.rect(field).unwrap();
    // Near the top of the window would be better for the menu, but the field is the panel's last
    // row: the menu opens above the pointer, which costs the same.
    let at = Point::new(rect.x0 + 40.0, f32::midpoint(rect.y0, rect.y1));
    real.ui
        .input(&InputEvent::PointerMoved { position: at }, &mut real.list);
    if drawn_before {
        real.ui.input(&secondary_press(at), &mut real.list);
        real_draw(&mut real);
        let _ = real.ui.take_access_update();
        for state in [ButtonState::Pressed, ButtonState::Released] {
            real.ui
                .input(&named_key(NamedKey::Escape, state), &mut real.list);
        }
    }
    real_draw(&mut real);
    let _ = real.ui.take_access_update();
    Menued { real, at }
}

fn secondary_press(at: Point) -> InputEvent {
    InputEvent::PointerButton {
        button: PointerButton::Right,
        state: ButtonState::Pressed,
        position: at,
        clicks: 1,
    }
}

fn named_key(key: NamedKey, state: ButtonState) -> InputEvent {
    InputEvent::Key {
        key: Key::Named(key),
        physical: PhysicalKey::Other,
        state,
        repeat: false,
        modifiers: Modifiers::default(),
    }
}

fn menued_fresh() -> Menued {
    menued(false)
}

fn menued_again() -> Menued {
    menued(true)
}

#[library_benchmark]
#[bench::fresh(menued_fresh())]
#[bench::again(menued_again())]
fn context_menu_open(mut menued: Menued) -> Menued {
    menued.real.ui.input(
        black_box(&secondary_press(menued.at)),
        &mut menued.real.list,
    );
    real_draw(&mut menued.real);
    black_box(menued.real.ui.take_access_update());
    menued
}

fn menued_open() -> Menued {
    let mut menued = menued(false);
    menued
        .real
        .ui
        .input(&secondary_press(menued.at), &mut menued.real.list);
    // Released where it was pressed: the menu stays open with nothing highlighted; one Down
    // highlights the first item, so the measured Down moves from one item to the next.
    let release = InputEvent::PointerButton {
        button: PointerButton::Right,
        state: ButtonState::Released,
        position: menued.at,
        clicks: 1,
    };
    menued.real.ui.input(&release, &mut menued.real.list);
    for state in [ButtonState::Pressed, ButtonState::Released] {
        menued.real.ui.input(
            &named_key(NamedKey::ArrowDown, state),
            &mut menued.real.list,
        );
    }
    real_draw(&mut menued.real);
    let _ = menued.real.ui.take_access_update();
    assert_eq!(menued.real.ui.menus().count(), 1);
    menued
}

#[library_benchmark]
#[bench::menu(menued_open())]
fn menu_highlight_move(mut menued: Menued) -> Menued {
    for state in [ButtonState::Pressed, ButtonState::Released] {
        menued.real.ui.input(
            black_box(&named_key(NamedKey::ArrowDown, state)),
            &mut menued.real.list,
        );
    }
    real_draw(&mut menued.real);
    black_box(menued.real.ui.take_access_update());
    menued
}

// The real panel with a menu bar on top (F2.11d2b): four titles whose menus hold the program's
// fourteen actions, File's menu open by a click; the pointer rests on File.
struct Barred {
    real: Real,
    edit: Point,
}

// The real panel's 40 rows (a label and four buttons each) under `root` (F2.11d2b's bench).
fn add_rows(ui: &mut Ui<usize>, root: WidgetId) {
    let column = ui
        .add(
            root,
            Flex::column(Space::Xs, Space::M),
            LayoutProps::default(),
        )
        .unwrap();
    for row_index in 0..40 {
        let row = ui
            .add(
                column,
                Flex::row(Space::S, Space::None),
                LayoutProps::default(),
            )
            .unwrap();
        ui.add(
            row,
            Label::new(format!("Track {row_index}: Violin I")),
            LayoutProps::default(),
        )
        .unwrap();
        for button in 0..4 {
            ui.add(
                row,
                Button::new(format!("Action {button}"), row_index * 4 + button),
                LayoutProps::default(),
            )
            .unwrap();
        }
    }
}

fn barred_open() -> Barred {
    let faces = orchestra_text::Faces::bundled().unwrap();
    let mut ui = Ui::new(
        Flex::column(Space::None, Space::None),
        &theme(),
        faces.clone(),
    );
    ui.set_title("Benchmark");
    let primary = Modifiers {
        primary: true,
        ..Modifiers::default()
    };
    let mut registry = Registry::new(Platform::Linux);
    let mut ids = Vec::new();
    for (index, letter) in "nosqzyxcvapfhg".chars().enumerate() {
        let identifier: &'static str = Box::leak(format!("bench.item-{index}").into_boxed_str());
        ids.push(
            registry
                .register(ActionSpec {
                    identifier,
                    name: identifier,
                    context: Context::GLOBAL,
                    defaults: Defaults::everywhere(&[Shortcut::new(
                        primary,
                        ShortcutKey::Character(letter),
                    )
                    .unwrap()]),
                    repeatable: false,
                    edit: None,
                })
                .unwrap(),
        );
    }
    registry.set_texts(KeyNames::new(|key| format!("{key:?}")), |name| {
        format!("Menu item {name}")
    });
    registry.set_mnemonics(|_| Some('m'));
    ui.set_actions(registry, |id: ActionId| usize::from(id.get()));
    let menu = |range: std::ops::Range<usize>| {
        ids[range]
            .iter()
            .fold(MenuModel::new(), |menu, &id| menu.action(id))
    };
    let model = MenuModel::new()
        .submenu("File", Some('f'), menu(0..6))
        .submenu("Edit", Some('e'), menu(6..13))
        .submenu("View", Some('v'), menu(13..14))
        .submenu("Help", Some('h'), menu(13..14));
    let root = ui.root();
    ui.add_menu_bar(
        root,
        &model,
        LayoutProps::default().width(Length::Fill(1.0)),
    )
    .unwrap();
    add_rows(&mut ui, root);
    let mut real = Real {
        ui,
        list: DisplayList::new(),
        renderer: CpuRenderer::new(faces),
        image: Image::new(1000, 1600).unwrap(),
        buttons: Vec::new(),
    };
    real.ui
        .layout(Rect::new(0.0, 0.0, 1000.0, 1600.0), &mut real.list);
    let file = real.ui.menu_bar_title_point(0).unwrap();
    let edit = real.ui.menu_bar_title_point(1).unwrap();
    real.ui
        .input(&InputEvent::PointerMoved { position: file }, &mut real.list);
    for state in [ButtonState::Pressed, ButtonState::Released] {
        real.ui.input(
            &InputEvent::PointerButton {
                button: PointerButton::Left,
                state,
                position: file,
                clicks: 1,
            },
            &mut real.list,
        );
    }
    assert_eq!(real.ui.menu_bar_open(), Some(0));
    real_draw(&mut real);
    let _ = real.ui.take_access_update();
    Barred { real, edit }
}

#[library_benchmark]
#[bench::bar(barred_open())]
fn bar_menu_switch(mut barred: Barred) -> Barred {
    barred.real.ui.input(
        black_box(&InputEvent::PointerMoved {
            position: barred.edit,
        }),
        &mut barred.real.list,
    );
    real_draw(&mut barred.real);
    black_box(barred.real.ui.take_access_update());
    barred
}

library_benchmark_group!(
    name = menus,
    benchmarks = [context_menu_open, menu_highlight_move, bar_menu_switch]
);

// The real panel under our own title bar (F2.11b3): the bar first, with the four menus of the
// program and three buttons, then the 40 rows; laid out in its frame, drawn once, its
// accessibility tree handed out once.
fn titled_shown() -> Real {
    use orchestra_ui::{TitleBarSpec, WindowButton, WindowButtonNames};
    let faces = orchestra_text::Faces::bundled().unwrap();
    let mut ui = Ui::new(
        Flex::column(Space::None, Space::None),
        &theme(),
        faces.clone(),
    );
    ui.set_title("Benchmark");
    let root = ui.root();
    let names = WindowButtonNames {
        close: "Close".to_owned(),
        maximize: "Maximize".to_owned(),
        restore: "Restore".to_owned(),
        minimize: "Minimize".to_owned(),
    };
    let spec = TitleBarSpec {
        title: "Orchestra".to_owned(),
        leading: Vec::new(),
        trailing: vec![
            WindowButton::Minimize,
            WindowButton::Maximize,
            WindowButton::Close,
        ],
        names,
    };
    let mut model = MenuModel::new();
    for title in ["File", "Edit", "View", "Help"] {
        model = model.submenu(title, None, MenuModel::new());
    }
    ui.add_title_bar(root, &model, &spec).unwrap();
    let mut buttons = Vec::new();
    for row_index in 0..40 {
        let row = ui
            .add(
                root,
                Flex::row(Space::S, Space::None),
                LayoutProps::default(),
            )
            .unwrap();
        ui.add(
            row,
            Label::new(format!("Track {row_index}: Violin I")),
            LayoutProps::default(),
        )
        .unwrap();
        for column in 0..4 {
            buttons.push(
                ui.add(
                    row,
                    Button::new(format!("Action {column}"), row_index * 4 + column),
                    LayoutProps::default(),
                )
                .unwrap(),
            );
        }
    }
    let mut real = Real {
        ui,
        list: DisplayList::new(),
        renderer: CpuRenderer::new(faces),
        image: Image::new(1000, 1600).unwrap(),
        buttons,
    };
    real.ui
        .layout(Rect::new(0.0, 0.0, 1000.0, 1600.0), &mut real.list);
    real_draw(&mut real);
    let _ = real.ui.take_access_update();
    real
}

#[library_benchmark]
#[bench::panel(titled_shown())]
fn title_bar_focus_change(mut real: Real) -> Real {
    real.ui
        .input(black_box(&InputEvent::Focus(false)), &mut real.list);
    real_draw(&mut real);
    black_box(real.ui.take_access_update());
    real
}

library_benchmark_group!(name = frame, benchmarks = [title_bar_focus_change]);

// The question of the dialogs' benchmarks (ADR-0050, F2.12a3a): a title, a message, a detail and
// three buttons, the usual "Save the changes?".
fn question() -> orchestra_ui::DialogSpec<()> {
    use orchestra_ui::{ButtonRole, DialogKind, DialogSpec};
    DialogSpec::new(
        DialogKind::Question,
        "Save the changes to the project before closing?",
        "If you don't save, the changes made since the last save will be lost.",
    )
    .detail("Symphony No. 1.orchestra")
    .button("Cancel", ButtonRole::Cancel, ())
    .button("Don't save", ButtonRole::Destructive, ())
    .button("Save", ButtonRole::Affirmative, ())
}

// Opening a question over the panel of 200 widgets and drawing it: the widgets built, the texts
// shaped, the panel and its frame recorded and rendered (only the panel's rectangle is damaged:
// no veil, owner's choice in F2.12a3a).
// Once per dialog, never on a frame's path; the budget is the 50 ms of an input (CLAUDE.md §5).
#[library_benchmark]
#[bench::panel(shown())]
fn dialog_open(mut panel: Panel) -> Panel {
    let id = panel
        .ui
        .open_dialog(black_box(question()), &mut panel.list)
        .unwrap();
    draw(&mut panel);
    black_box(id);
    panel
}

// The question open and drawn once, its accessibility tree handed out.
fn asked() -> Panel {
    let mut panel = shown();
    panel.ui.open_dialog(question(), &mut panel.list).unwrap();
    draw(&mut panel);
    let _ = panel.ui.take_access_update();
    panel
}

// Tab pressed and released inside the open question, and the frame drawn: the focus ring moves
// from one button to the next (two buttons recorded again and repainted).
#[library_benchmark]
#[bench::panel(asked())]
fn dialog_tab(mut panel: Panel) -> Panel {
    for state in [ButtonState::Pressed, ButtonState::Released] {
        panel.ui.input(
            black_box(&InputEvent::Key {
                key: Key::Named(NamedKey::Tab),
                physical: PhysicalKey::Other,
                state,
                repeat: false,
                modifiers: Modifiers::default(),
            }),
            &mut panel.list,
        );
    }
    draw(&mut panel);
    panel
}

// The About dialog's shape (F2.12a3b, ADR-0050 DG10): the logo (10 filled paths, about 860
// elements), a title, the version and four details, one button. The texts are the en-US
// catalogue's, written here so the benchmark does not depend on `orchestra-app`.
fn about() -> orchestra_ui::DialogSpec<()> {
    use orchestra_ui::{ButtonRole, DialogKind, DialogSpec};
    DialogSpec::new(DialogKind::Information, "About Orchestra", "Version 0.0.0")
        .logo("Orchestra logo")
        .detail("Free software under the GNU General Public License, version 3 or later.")
        .detail(
            "The logo is © 2026 Emanuele Rainoldi, all rights reserved: it is not covered by \
             that license.",
        )
        .detail("Source code: https://github.com/HumusAudio/orchestra")
        .detail(
            "Bundled fonts: Inter, Bravura and JetBrains Mono, under the SIL Open Font License \
             1.1.",
        )
        .button("Close", ButtonRole::Cancel, ())
}

// Opening the About dialog over the panel of 200 widgets and drawing it: as `dialog_open`, plus
// the logo's paths recorded and filled by `tiny-skia`. Once per opening, never on a frame's path;
// the budget is the 50 ms of an input (CLAUDE.md §5).
#[library_benchmark]
#[bench::panel(shown())]
fn about_open(mut panel: Panel) -> Panel {
    let id = panel
        .ui
        .open_dialog(black_box(about()), &mut panel.list)
        .unwrap();
    draw(&mut panel);
    black_box(id);
    panel
}

library_benchmark_group!(
    name = dialogs,
    benchmarks = [dialog_open, dialog_tab, about_open]
);

main!(
    library_benchmark_groups = core,
    controls,
    scrolling,
    editing,
    popups,
    actions,
    menus,
    frame,
    dialogs
);
