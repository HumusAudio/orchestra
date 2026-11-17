// SPDX-License-Identifier: GPL-3.0-or-later

//! Input methods in the text field (ADR-0047 TF5, TF6; F2.10c2), headless: when the interface
//! asks the window for the input method and where it puts the candidates, the composition shown
//! but not part of the text, the commit inserted like typing (and undone like it), the
//! composition in place of the selection, plain keys that keep typing with the input method on
//! (rule (b) without a filter), the keys swallowed during a composition (D1), the click that
//! ends it and asks the window for a reset (D3), and the focus, the input method or the
//! application that drop it (D4). The drawing of its lines is checked on pixels.

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

use std::ops::Range;

use orchestra_text::Faces;
use orchestra_ui::{
    Appearance, BuiltInTheme, Button, ButtonState, ColorToken, Constraints, CpuRenderer,
    DisplayList, EditConvention, Image, ImeEvent, InputEvent, Key, LayoutCx, LayoutProps,
    Modifiers, NamedKey, Point, PointerButton, RecordCx, Recorder, Rect, Size, TextField, Theme,
    Ui, Widget, WidgetId,
};

/// The messages of these tests.
#[derive(Debug, Clone, PartialEq)]
enum Msg {
    /// The field committed this text.
    Committed(String),
    /// The button after the field was pressed.
    Other,
}

/// The root: the field at [`FIELD`], the button below it.
#[derive(Debug)]
struct Root;

/// Where the field is, logical pixels of the window.
const FIELD: Rect = Rect::new(10.0, 10.0, 210.0, 38.0);

impl Widget<Msg> for Root {
    fn measure(&mut self, cx: &mut LayoutCx<'_, Msg>, constraints: Constraints) -> Size {
        for child in 0..cx.child_count() {
            cx.measure_child(child, Constraints::tight(Size::new(FIELD.width(), 28.0)));
        }
        constraints.constrain(Size::new(400.0, 200.0))
    }

    fn arrange(&mut self, cx: &mut LayoutCx<'_, Msg>, _rect: Rect) {
        cx.place_child(0, FIELD);
        cx.place_child(1, Rect::new(10.0, 50.0, 210.0, 78.0));
    }

    fn record(&self, _cx: &RecordCx, _recorder: &mut Recorder<'_>) {}
}

/// Studio light.
fn theme() -> Theme {
    *BuiltInTheme::Studio.pair().instance(Appearance::Light)
}

/// A window of 400 × 200.
const BOUNDS: Rect = Rect::new(0.0, 0.0, 400.0, 200.0);

/// The interface with a field holding a text, and a button after it.
struct Bench {
    ui: Ui<Msg>,
    list: DisplayList,
    field: WidgetId,
    button: WidgetId,
}

impl Bench {
    /// A field holding `text`, with the Linux keys.
    fn new(text: &str) -> Self {
        let mut ui = Ui::new(Root, &theme(), Faces::bundled().unwrap());
        let root = ui.root();
        let field = ui
            .add(
                root,
                TextField::new(text, |text| Msg::Committed(text.to_owned()))
                    .convention(EditConvention::Linux),
                LayoutProps::default(),
            )
            .unwrap();
        let button = ui
            .add(root, Button::new("OK", Msg::Other), LayoutProps::default())
            .unwrap();
        let mut list = DisplayList::new();
        ui.layout(BOUNDS, &mut list);
        Self {
            ui,
            list,
            field,
            button,
        }
    }

    /// The same, with the field focused (by Tab: its text is selected) and the input method on.
    fn focused(text: &str) -> Self {
        let mut bench = Self::new(text);
        bench.press(&Key::Named(NamedKey::Tab), Modifiers::default());
        bench.ime(ImeEvent::Enabled);
        bench
    }

    /// The field.
    fn field(&self) -> &TextField<Msg> {
        self.ui.widget::<TextField<Msg>>(self.field).unwrap()
    }

    /// Gives `event` to the interface.
    fn input(&mut self, event: &InputEvent) {
        self.ui.input(event, &mut self.list);
    }

    /// An input method's event.
    fn ime(&mut self, event: ImeEvent) {
        self.input(&InputEvent::Ime(event));
    }

    /// A composition `text` with the input method's `cursor`.
    fn preedit(&mut self, text: &str, cursor: Option<Range<usize>>) {
        self.ime(ImeEvent::Preedit {
            text: text.to_owned(),
            cursor,
        });
    }

    /// A commit as `winit` sends it: an empty composition, then the text.
    fn commit(&mut self, text: &str) {
        self.preedit("", None);
        self.ime(ImeEvent::Commit(text.to_owned()));
    }

    /// Presses and releases `key` with `modifiers`.
    fn press(&mut self, key: &Key, modifiers: Modifiers) {
        for state in [ButtonState::Pressed, ButtonState::Released] {
            self.input(&InputEvent::Key {
                key: key.clone(),
                state,
                repeat: false,
                modifiers,
            });
        }
    }

    /// Types `text` as a keyboard does: each character's key, then its text.
    fn type_text(&mut self, text: &str) {
        for c in text.chars() {
            let key = Key::Character(c.to_string());
            self.input(&InputEvent::Key {
                key: key.clone(),
                state: ButtonState::Pressed,
                repeat: false,
                modifiers: Modifiers::default(),
            });
            self.input(&InputEvent::Text(c.to_string()));
            self.input(&InputEvent::Key {
                key,
                state: ButtonState::Released,
                repeat: false,
                modifiers: Modifiers::default(),
            });
        }
    }

    /// A left click at `at`.
    fn click(&mut self, at: Point) {
        self.input(&InputEvent::PointerMoved { position: at });
        for state in [ButtonState::Pressed, ButtonState::Released] {
            self.input(&InputEvent::PointerButton {
                button: PointerButton::Left,
                state,
                position: at,
                clicks: 1,
            });
        }
    }

    /// The messages so far.
    fn messages(&mut self) -> Vec<Msg> {
        self.ui.take_messages().collect()
    }

    /// The whole frame drawn.
    fn draw(&self) -> Image {
        CpuRenderer::new(Faces::bundled().unwrap())
            .render_to_image(&self.list, &theme(), 400, 200, 1.0)
            .unwrap()
    }
}

/// The modifiers with the primary one (Ctrl on Linux) held.
fn ctrl() -> Modifiers {
    Modifiers {
        primary: true,
        ..Modifiers::default()
    }
}

/// How many pixels of `image` on row `y`, between `x0` and `x1`, have exactly `token`'s colour.
fn count_row(image: &Image, y: u32, x0: u32, x1: u32, token: ColorToken) -> usize {
    let c = theme().color(token);
    let want = [c.r, c.g, c.b, 255];
    (x0..x1)
        .filter(|&x| {
            let i = ((y * image.width() + x) * 4) as usize;
            image.data()[i..i + 4] == want
        })
        .count()
}

#[test]
fn the_input_method_is_wanted_only_while_a_field_has_the_focus() {
    let mut bench = Bench::new("Violin");
    assert_eq!(bench.ui.text_input(), None, "nothing focused");
    bench.press(&Key::Named(NamedKey::Tab), Modifiers::default());
    let input = bench.ui.text_input().expect("the field has the focus");
    // The caret's area: inside the field, a thin bar the line's height.
    assert!(FIELD.x0 <= input.area.x0 && input.area.x1 <= FIELD.x1);
    assert!(FIELD.y0 <= input.area.y0 && input.area.y1 <= FIELD.y1);
    assert!(input.area.width() < 3.0 && input.area.height() > 10.0);
    bench.press(&Key::Named(NamedKey::Tab), Modifiers::default());
    assert_eq!(bench.ui.focused(), Some(bench.button));
    assert_eq!(bench.ui.text_input(), None, "a button takes no text");
}

#[test]
fn the_candidates_follow_the_caret() {
    let mut bench = Bench::focused("");
    let at_start = bench.ui.text_input().unwrap().area;
    bench.type_text("abc");
    let after_typing = bench.ui.text_input().unwrap().area;
    assert!(after_typing.x0 > at_start.x0);
    // Inside a composition, the input method's caret.
    bench.preedit("nihao", Some(2..2));
    let inside = bench.ui.text_input().unwrap().area;
    bench.preedit("nihao", Some(5..5));
    let at_end = bench.ui.text_input().unwrap().area;
    assert!(after_typing.x0 < inside.x0 && inside.x0 < at_end.x0);
}

#[test]
fn a_composition_is_shown_but_is_not_the_text() {
    let mut bench = Bench::focused("");
    bench.type_text("ab");
    bench.preedit("にほん", Some(9..9));
    let field = bench.field();
    assert!(field.composing());
    assert_eq!(field.preedit(), ("にほん", Some(9..9)));
    assert_eq!(field.text(), "ab", "not part of the text");
    assert_eq!(field.caret(), 2);
    // Repeating the same composition changes nothing; an empty one ends it.
    bench.preedit("にほん", Some(9..9));
    assert!(bench.field().composing());
    bench.preedit("", None);
    assert!(!bench.field().composing());
    assert_eq!(bench.field().text(), "ab");
    assert!(bench.messages().is_empty(), "nothing committed");
}

#[test]
fn a_commit_is_inserted_like_typing_and_undone_like_it() {
    let mut bench = Bench::focused("");
    bench.preedit("nihao", Some(5..5));
    bench.commit("你好");
    assert!(!bench.field().composing());
    assert_eq!(bench.field().text(), "你好");
    assert_eq!(bench.field().caret(), "你好".len());
    // Grouped with the typing around it (TF7, D7): one undo takes both back.
    bench.type_text("ab");
    bench.press(&Key::Character("z".to_owned()), ctrl());
    assert_eq!(bench.field().text(), "");
    // Committed to the application only at Enter, as typed text is.
    bench.commit("日本");
    bench.press(&Key::Named(NamedKey::Enter), Modifiers::default());
    assert_eq!(bench.messages(), vec![Msg::Committed("日本".to_owned())]);
}

#[test]
fn a_composition_takes_the_place_of_the_selection_and_its_commit_replaces_it() {
    // Tab selects the whole text.
    let mut bench = Bench::focused("Violin");
    assert_eq!(bench.field().selection(), 0.."Violin".len());
    bench.preedit("viola", Some(5..5));
    // Not replaced yet: only shown in its place.
    assert_eq!(bench.field().text(), "Violin");
    bench.commit("Viola");
    assert_eq!(bench.field().text(), "Viola");
}

#[test]
fn plain_keys_keep_typing_with_the_input_method_on() {
    // Rule (b) of TF6 without a filter: `winit` never reports a key both ways.
    let mut bench = Bench::focused("");
    bench.type_text("Ab");
    bench.commit("é");
    bench.type_text("c");
    assert_eq!(bench.field().text(), "Abéc");
}

#[test]
fn during_a_composition_the_keys_belong_to_the_input_method() {
    let mut bench = Bench::focused("Violin");
    bench.press(&Key::Named(NamedKey::End), Modifiers::default());
    bench.preedit("ka", Some(2..2));
    for key in [
        NamedKey::Enter,
        NamedKey::Escape,
        NamedKey::Backspace,
        NamedKey::ArrowLeft,
        NamedKey::Tab,
    ] {
        bench.press(&Key::Named(key), Modifiers::default());
    }
    let field = bench.field();
    assert_eq!(field.text(), "Violin", "nothing edited");
    assert_eq!(field.caret(), "Violin".len(), "nothing moved");
    assert!(field.composing());
    assert_eq!(
        bench.ui.focused(),
        Some(bench.field),
        "Tab did not move the focus"
    );
    assert!(bench.messages().is_empty(), "Enter committed nothing");
    // Once it ends, the keys are the field's again.
    bench.preedit("", None);
    bench.press(&Key::Named(NamedKey::Backspace), Modifiers::default());
    assert_eq!(bench.field().text(), "Violi");
}

#[test]
fn a_click_during_a_composition_ends_it_and_asks_for_a_reset() {
    let mut bench = Bench::focused("Violin");
    bench.press(&Key::Named(NamedKey::End), Modifiers::default());
    let before = bench.ui.text_input().unwrap().reset;
    bench.preedit("ka", Some(2..2));
    // A click on the text's left end: the caret goes there.
    bench.click(Point::new(
        FIELD.x0 + 9.0,
        f32::midpoint(FIELD.y0, FIELD.y1),
    ));
    assert!(!bench.field().composing());
    assert_eq!(bench.field().caret(), 0);
    let after = bench.ui.text_input().unwrap().reset;
    assert_ne!(
        before, after,
        "the window is asked to reset the input method"
    );
    // The input method commits it at the new caret (most do).
    bench.commit("か");
    assert_eq!(bench.field().text(), "かViolin");
    // A click without a composition asks for no reset.
    bench.click(Point::new(
        FIELD.x0 + 9.0,
        f32::midpoint(FIELD.y0, FIELD.y1),
    ));
    assert_eq!(bench.ui.text_input().unwrap().reset, after);
}

#[test]
fn losing_the_focus_the_input_method_or_the_text_drops_the_composition() {
    // The focus.
    let mut bench = Bench::focused("");
    bench.preedit("ka", Some(2..2));
    let button = bench.button;
    bench.ui.set_focus(Some(button), &mut bench.list).unwrap();
    assert!(!bench.field().composing());
    assert_eq!(bench.field().text(), "");
    // A commit arriving afterwards goes to the button, which ignores it.
    bench.commit("か");
    assert_eq!(bench.field().text(), "");

    // The input method turned off.
    let mut bench = Bench::focused("");
    bench.preedit("ka", Some(2..2));
    bench.ime(ImeEvent::Disabled);
    assert!(!bench.field().composing());

    // A text set by the application.
    let mut bench = Bench::focused("");
    bench.preedit("ka", Some(2..2));
    let field = bench.field;
    bench
        .ui
        .widget_mut::<TextField<Msg>>(field)
        .unwrap()
        .set_text("Cello");
    assert!(!bench.field().composing());
    assert_eq!(bench.field().text(), "Cello");
}

#[test]
fn a_composition_is_underlined_and_its_segment_twice_as_thick() {
    let mut bench = Bench::focused("");
    // Latin letters: the bundled faces have them (Japanese would draw boxes, ADR-0044 TX4).
    bench.preedit("nihao", Some(0..2));
    let image = bench.draw();
    let text = ColorToken::Text;
    // Find the rows with a line in the text's colour below the letters: a long run of it.
    let x0 = (FIELD.x0 + 2.0) as u32;
    let x1 = (FIELD.x1 - 2.0) as u32;
    let rows: Vec<(u32, usize)> = (FIELD.y0 as u32..FIELD.y1 as u32)
        .map(|y| (y, count_row(&image, y, x0, x1, text)))
        .filter(|&(_, n)| n >= 10)
        .collect();
    assert!(
        !rows.is_empty(),
        "a line under the composition: rows {rows:?}"
    );
    // The thin line covers the whole composition, the thick one only "ni": the first row is
    // the longest, the second (only under the segment) shorter.
    let lengths: Vec<usize> = rows.iter().map(|&(_, n)| n).collect();
    assert_eq!(
        rows.len(),
        2,
        "one row for the line, one more for the segment: {rows:?}"
    );
    assert!(lengths[0] > lengths[1], "{lengths:?}");
    // Without a composition there is no line.
    bench.preedit("", None);
    let image = bench.draw();
    assert!(
        (FIELD.y0 as u32..FIELD.y1 as u32).all(|y| count_row(&image, y, x0, x1, text) < 10),
        "the line is gone"
    );
}
