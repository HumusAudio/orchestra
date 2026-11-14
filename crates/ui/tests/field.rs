// SPDX-License-Identifier: GPL-3.0-or-later

//! The text field widget (ADR-0047, F2.10b2), headless: focus and its selection, typing, the
//! keys that bubble and those swallowed, commit on Enter and on leaving, Escape, the local undo,
//! the pointer (click, Shift+click, drag, double and triple click, the text beam), scrolling inside
//! the field, the caret's blink in made-up time (its halves, its stop after 10 s, the wake-ups the
//! window would sleep until, and the small area a blink redraws), a disabled field, and the
//! drawing of the text, the placeholder and the selection.

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

use std::time::{Duration, Instant};

use orchestra_text::Faces;
use orchestra_ui::{
    Appearance, BuiltInTheme, Button, ButtonState, ColorToken, Constraints, CpuRenderer,
    DisplayList, EditConvention, EventCx, Handled, Image, InputEvent, Key, LayoutCx, LayoutProps,
    Length, Modifiers, NamedKey, Point, PointerButton, PointerShape, RecordCx, Recorder, Rect,
    Size, TextField, Theme, Ui, Widget, WidgetEvent, WidgetId,
};

/// The messages of these tests.
#[derive(Debug, Clone, PartialEq)]
enum Msg {
    /// The field committed this text.
    Committed(String),
    /// The field's text changed to this (live).
    Changed(String),
    /// A key reached the root: the field let it bubble.
    Bubbled(Key),
    /// The button after the field was pressed.
    Other,
}

/// The root of these tests: a column of its children (the field, then a button), which logs the
/// keys that reach it, so a test sees which keys the field lets bubble.
#[derive(Debug)]
struct Root;

/// The height of each row of [`Root`], logical pixels.
const ROW: f32 = 40.0;

impl Widget<Msg> for Root {
    fn measure(&mut self, cx: &mut LayoutCx<'_, Msg>, constraints: Constraints) -> Size {
        for child in 0..cx.child_count() {
            cx.measure_child(child, Constraints::tight(Size::new(FIELD_WIDTH, 28.0)));
        }
        constraints.constrain(Size::new(400.0, 200.0))
    }

    fn arrange(&mut self, cx: &mut LayoutCx<'_, Msg>, rect: Rect) {
        for child in 0..cx.child_count() {
            #[allow(clippy::cast_precision_loss, reason = "two children")]
            let y = rect.y0 + 10.0 + ROW * child as f32;
            cx.place_child(
                child,
                Rect::from_origin_size(rect.x0 + 10.0, y, FIELD_WIDTH, 28.0),
            );
        }
    }

    fn record(&self, _cx: &RecordCx, _recorder: &mut Recorder<'_>) {}

    fn event(&mut self, cx: &mut EventCx<'_, Msg>, event: &WidgetEvent<'_>) -> Handled {
        if let WidgetEvent::Key {
            key,
            state: ButtonState::Pressed,
            ..
        } = event
            // Tab is left to the interface, which moves the focus with it.
            && **key != Key::Named(NamedKey::Tab)
        {
            cx.emit(Msg::Bubbled((*key).clone()));
            return Handled::Yes;
        }
        Handled::No
    }
}

/// The field's width: 200 logical pixels.
const FIELD_WIDTH: f32 = 200.0;

/// Studio light.
fn theme() -> Theme {
    *BuiltInTheme::Studio.pair().instance(Appearance::Light)
}

/// A window of 400 × 200.
const BOUNDS: Rect = Rect::new(0.0, 0.0, 400.0, 200.0);

/// A test bench: the interface, its list, the field, the button after it, and the clock the
/// tests advance by hand.
struct Bench {
    ui: Ui<Msg>,
    list: DisplayList,
    field: WidgetId,
    button: WidgetId,
    start: Instant,
}

impl Bench {
    /// A field holding `text` (Linux keys, live changes on), and a button after it.
    fn new(text: &str) -> Self {
        Self::with(
            TextField::new(text, |text| Msg::Committed(text.to_owned()))
                .on_change(|text| Msg::Changed(text.to_owned()))
                .placeholder("Name")
                .labelled("Track name")
                .convention(EditConvention::Linux),
        )
    }

    /// A bench around `field`.
    fn with(field: TextField<Msg>) -> Self {
        let mut ui = Ui::new(Root, &theme(), Faces::bundled().unwrap());
        let root = ui.root();
        let field = ui.add(root, field, LayoutProps::default()).unwrap();
        let button = ui
            .add(root, Button::new("OK", Msg::Other), LayoutProps::default())
            .unwrap();
        let mut list = DisplayList::new();
        let start = Instant::now();
        ui.advance(start, &mut list);
        ui.layout(BOUNDS, &mut list);
        Self {
            ui,
            list,
            field,
            button,
            start,
        }
    }

    /// The field.
    fn field(&self) -> &TextField<Msg> {
        self.ui.widget::<TextField<Msg>>(self.field).unwrap()
    }

    /// Gives `event` to the interface.
    fn input(&mut self, event: &InputEvent) {
        self.ui.input(event, &mut self.list);
    }

    /// Tab, from wherever the focus is.
    fn tab(&mut self) {
        self.press(Key::Named(NamedKey::Tab), Modifiers::default());
    }

    /// Presses and releases `key` with `modifiers`.
    fn press(&mut self, key: Key, modifiers: Modifiers) {
        self.input(&InputEvent::Key {
            key: key.clone(),
            state: ButtonState::Pressed,
            repeat: false,
            modifiers,
        });
        self.input(&InputEvent::Key {
            key,
            state: ButtonState::Released,
            repeat: false,
            modifiers,
        });
    }

    /// Types `text` as a keyboard would: each character's key, then its text.
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

    /// A left button event at `at` with `clicks`.
    fn button_event(&mut self, state: ButtonState, at: Point, clicks: u8) {
        self.input(&InputEvent::PointerButton {
            button: PointerButton::Left,
            state,
            position: at,
            clicks,
        });
    }

    /// A click (press and release) at `at`, the `clicks`-th of a series.
    fn click(&mut self, at: Point, clicks: u8) {
        self.input(&InputEvent::PointerMoved { position: at });
        self.button_event(ButtonState::Pressed, at, clicks);
        self.button_event(ButtonState::Released, at, clicks);
    }

    /// The point inside the field at `x` from its text's left edge (the side padding included),
    /// on the line's middle.
    fn at_text_x(&self, x: f32) -> Point {
        let rect = self.ui.rect(self.field).unwrap();
        let padding = theme().spacing().control_padding;
        Point::new(rect.x0 + padding + x, f32::midpoint(rect.y0, rect.y1))
    }

    /// Advances the interface's time to `start + elapsed`.
    fn at(&mut self, elapsed: Duration) {
        self.ui.advance(self.start + elapsed, &mut self.list);
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

/// The modifiers with Shift held.
fn shift() -> Modifiers {
    Modifiers {
        shift: true,
        ..Modifiers::default()
    }
}

/// The pixel of a token.
fn rgba(token: ColorToken) -> [u8; 4] {
    let c = theme().color(token);
    [c.r, c.g, c.b, 255]
}

/// How many pixels of `image` inside `rect` have exactly `token`'s colour.
fn count(image: &Image, rect: Rect, token: ColorToken) -> usize {
    let wanted = rgba(token);
    let mut found = 0;
    for y in rect.y0 as u32..rect.y1 as u32 {
        for x in rect.x0 as u32..rect.x1 as u32 {
            if image.pixel(x, y) == Some(wanted) {
                found += 1;
            }
        }
    }
    found
}

#[test]
fn tab_focuses_the_field_and_selects_its_text_so_typing_replaces_it() {
    let mut bench = Bench::new("Violin");
    bench.tab();
    assert_eq!(bench.ui.focused(), Some(bench.field));
    assert_eq!(bench.field().selection(), 0..6);
    bench.type_text("Viola");
    assert_eq!(bench.field().text(), "Viola");
    assert_eq!(bench.field().caret(), 5);
    // Each edit was announced to the live filter; the plain keys never bubbled.
    let messages = bench.messages();
    assert_eq!(messages.first(), Some(&Msg::Changed("V".to_owned())));
    assert_eq!(messages.last(), Some(&Msg::Changed("Viola".to_owned())));
    assert!(!messages.iter().any(|m| matches!(m, Msg::Bubbled(_))));
}

#[test]
fn enter_commits_once_and_bubbles() {
    let mut bench = Bench::new("Violin");
    bench.tab();
    bench.type_text("Cello");
    bench.messages();
    bench.press(Key::Named(NamedKey::Enter), Modifiers::default());
    assert_eq!(
        bench.messages(),
        vec![
            Msg::Committed("Cello".to_owned()),
            Msg::Bubbled(Key::Named(NamedKey::Enter)),
        ]
    );
    // Nothing changed since: a second Enter commits nothing, and still bubbles.
    bench.press(Key::Named(NamedKey::Enter), Modifiers::default());
    assert_eq!(
        bench.messages(),
        vec![Msg::Bubbled(Key::Named(NamedKey::Enter))]
    );
}

#[test]
fn leaving_the_field_commits_a_change_and_only_a_change() {
    let mut bench = Bench::new("Violin");
    bench.tab();
    bench.tab();
    // Focus came and went with no edit: nothing to commit.
    assert_eq!(bench.ui.focused(), Some(bench.button));
    assert!(
        !bench
            .messages()
            .iter()
            .any(|m| matches!(m, Msg::Committed(_)))
    );
    // Back with Shift+Tab, an edit, and out with a click on the button.
    bench.press(Key::Named(NamedKey::Tab), shift());
    bench.type_text("Flute");
    let button = bench.ui.rect(bench.button).unwrap();
    bench.click(Point::new(button.x0 + 5.0, button.y0 + 5.0), 1);
    assert!(
        bench
            .messages()
            .contains(&Msg::Committed("Flute".to_owned()))
    );
}

#[test]
fn escape_brings_back_the_text_of_the_focus_and_then_bubbles() {
    let mut bench = Bench::new("Violin");
    bench.tab();
    bench.type_text("Oboe");
    bench.messages();
    bench.press(Key::Named(NamedKey::Escape), Modifiers::default());
    assert_eq!(bench.field().text(), "Violin");
    // Restored, not committed; the restoring is an edit the live filter hears about.
    assert_eq!(bench.messages(), vec![Msg::Changed("Violin".to_owned())]);
    // Nothing more to restore: the second Escape goes on to the root (a dialog would close).
    bench.press(Key::Named(NamedKey::Escape), Modifiers::default());
    assert_eq!(
        bench.messages(),
        vec![Msg::Bubbled(Key::Named(NamedKey::Escape))]
    );
    // And leaving commits nothing: the text is the one it had.
    bench.tab();
    assert!(
        !bench
            .messages()
            .iter()
            .any(|m| matches!(m, Msg::Committed(_)))
    );
}

#[test]
fn shortcuts_bubble_and_editing_keys_do_not() {
    let mut bench = Bench::new("Violin");
    bench.tab();
    bench.messages();
    // Ctrl+K is not an editing key: an application shortcut, it bubbles.
    bench.press(Key::Character("k".to_owned()), ctrl());
    assert_eq!(
        bench.messages(),
        vec![Msg::Bubbled(Key::Character("k".to_owned()))]
    );
    // Ctrl+← and Undo with nothing to undo are the field's, even when they change nothing.
    bench.press(Key::Named(NamedKey::ArrowLeft), ctrl());
    bench.press(Key::Character("z".to_owned()), ctrl());
    // ↑ does nothing in a one-line field on Linux: it bubbles (a list around may use it).
    bench.press(Key::Named(NamedKey::ArrowUp), Modifiers::default());
    assert_eq!(
        bench.messages(),
        vec![Msg::Bubbled(Key::Named(NamedKey::ArrowUp))]
    );
}

#[test]
fn the_local_undo_takes_back_typing_by_words() {
    let mut bench = Bench::new("");
    bench.tab();
    bench.type_text("Violin solo");
    // The group ends where a word starts after a space (F2.10b1): "solo", then "Violin ".
    bench.press(Key::Character("z".to_owned()), ctrl());
    assert_eq!(bench.field().text(), "Violin ");
    bench.press(Key::Character("z".to_owned()), ctrl());
    assert_eq!(bench.field().text(), "");
    bench.press(
        Key::Character("Z".to_owned()),
        Modifiers {
            shift: true,
            ..ctrl()
        },
    );
    assert_eq!(bench.field().text(), "Violin ");
}

#[test]
fn a_click_puts_the_caret_at_the_nearest_boundary_and_shift_extends() {
    let mut bench = Bench::new("Violin solo");
    // The first click focuses and places the caret: no select-all, no extension.
    bench.click(bench.at_text_x(0.5), 1);
    assert_eq!(bench.ui.focused(), Some(bench.field));
    assert_eq!(bench.field().selection(), 0..0);
    assert_eq!(bench.field().caret(), 0);
    // Far past the end: the end.
    bench.click(bench.at_text_x(150.0), 1);
    assert_eq!(bench.field().caret(), 11);
    // Shift+click near the start extends from the anchor (the end).
    bench.input(&InputEvent::ModifiersChanged { modifiers: shift() });
    bench.click(bench.at_text_x(0.5), 1);
    bench.input(&InputEvent::ModifiersChanged {
        modifiers: Modifiers::default(),
    });
    assert_eq!(bench.field().selection(), 0..11);
    assert_eq!(bench.field().caret(), 0);
}

#[test]
fn dragging_selects_and_double_and_triple_clicks_select_words_and_all() {
    let mut bench = Bench::new("Violin solo tutti");
    let start = bench.at_text_x(0.5);
    let end = bench.at_text_x(150.0);
    bench.input(&InputEvent::PointerMoved { position: start });
    bench.button_event(ButtonState::Pressed, start, 1);
    bench.input(&InputEvent::PointerMoved { position: end });
    // While dragging, the beam stays even outside the field (the pointer is captured).
    bench.input(&InputEvent::PointerMoved {
        position: Point::new(390.0, 190.0),
    });
    assert_eq!(bench.ui.pointer_shape(), PointerShape::Text);
    bench.button_event(ButtonState::Released, Point::new(390.0, 190.0), 1);
    assert_eq!(bench.field().selection(), 0..17);
    // A double click inside "solo" selects it.
    // "Violin " is about 45 px wide in the body size: aim inside the next word.
    let solo = bench.at_text_x(60.0);
    bench.click(solo, 1);
    bench.button_event(ButtonState::Pressed, solo, 2);
    assert_eq!(bench.field().selection(), 7..11);
    // Dragging back to "Violin" extends by words, keeping "solo".
    bench.input(&InputEvent::PointerMoved { position: start });
    assert_eq!(bench.field().selection(), 0..11);
    assert_eq!(bench.field().caret(), 0);
    bench.button_event(ButtonState::Released, start, 2);
    // A triple click selects everything.
    bench.button_event(ButtonState::Pressed, solo, 3);
    bench.button_event(ButtonState::Released, solo, 3);
    assert_eq!(bench.field().selection(), 0..17);
}

#[test]
fn the_pointer_is_the_text_beam_over_an_enabled_field_only() {
    let mut bench = Bench::new("Violin");
    let inside = bench.at_text_x(10.0);
    bench.input(&InputEvent::PointerMoved { position: inside });
    assert_eq!(bench.ui.pointer_shape(), PointerShape::Text);
    bench.input(&InputEvent::PointerMoved {
        position: Point::new(390.0, 190.0),
    });
    assert_eq!(bench.ui.pointer_shape(), PointerShape::Default);
    let mut disabled = Bench::new("Violin");
    let id = disabled.field;
    disabled
        .ui
        .widget_mut::<TextField<Msg>>(id)
        .unwrap()
        .set_enabled(false);
    disabled.input(&InputEvent::PointerMoved { position: inside });
    assert_eq!(disabled.ui.pointer_shape(), PointerShape::Default);
    // A disabled field takes no focus: Tab goes to the button.
    disabled.tab();
    assert_eq!(disabled.ui.focused(), Some(disabled.button));
}

#[test]
fn a_long_text_scrolls_to_keep_the_caret_in_view_and_comes_back_when_deleted() {
    let mut bench = Bench::new("");
    bench.tab();
    bench.type_text("Violino primo, leggio uno, divisi a tre");
    // The caret at the end of a text wider than the field: scrolled.
    let scrolled = bench.field().scroll();
    assert!(scrolled > 0.0, "scroll {scrolled}");
    // Home brings the start back into view.
    bench.press(Key::Named(NamedKey::Home), Modifiers::default());
    assert!(bench.field().scroll().abs() < f32::EPSILON);
    // End again, then deleting words at the end lets the text come back to fill the field.
    bench.press(Key::Named(NamedKey::End), Modifiers::default());
    assert!((bench.field().scroll() - scrolled).abs() < 0.01);
    for _ in 0..3 {
        bench.press(Key::Named(NamedKey::Backspace), ctrl());
    }
    assert!(bench.field().scroll() < scrolled);
    // Short enough to fit: no scroll at all.
    bench.press(Key::Character("a".to_owned()), ctrl());
    bench.type_text("Viola");
    assert!(bench.field().scroll().abs() < f32::EPSILON);
}

#[test]
fn the_caret_blinks_in_halves_of_530_ms_and_stops_visible_after_10_s() {
    let mut bench = Bench::new("");
    // No focus, no wake-up: the window would wait for input.
    assert_eq!(bench.ui.next_wakeup(), None);
    bench.tab();
    let ms = Duration::from_millis;
    assert!(bench.field().caret_on());
    assert_eq!(bench.ui.next_wakeup(), Some(bench.start + ms(530)));
    bench.at(ms(530));
    assert!(!bench.field().caret_on());
    assert_eq!(bench.ui.next_wakeup(), Some(bench.start + ms(1060)));
    // A late wake-up keeps the rhythm: at 1200 ms the caret is in its third half (on), and the
    // next half starts at 1590 ms, not 530 ms after this late wake-up.
    bench.at(ms(1200));
    assert!(bench.field().caret_on());
    assert_eq!(bench.ui.next_wakeup(), Some(bench.start + ms(1590)));
    // A key at 2000 ms shows it solid and starts again from there.
    bench.at(ms(2000));
    bench.type_text("V");
    assert!(bench.field().caret_on());
    assert_eq!(bench.ui.next_wakeup(), Some(bench.start + ms(2530)));
    // Ten seconds after that key, it stops, visible, and no wake-up is left.
    bench.at(ms(2000 + 9_900));
    assert_eq!(bench.ui.next_wakeup(), Some(bench.start + ms(12_000)));
    bench.at(ms(12_000));
    assert!(bench.field().caret_on());
    assert_eq!(bench.ui.next_wakeup(), None);
}

#[test]
fn losing_the_focus_stops_the_blink() {
    let mut bench = Bench::new("Violin");
    bench.tab();
    assert!(bench.ui.next_wakeup().is_some());
    bench.tab();
    assert_eq!(bench.ui.next_wakeup(), None);
}

#[test]
fn a_blink_redraws_only_the_caret() {
    let mut bench = Bench::new("");
    bench.click(bench.at_text_x(0.0), 1);
    bench.list.take_damage();
    bench.at(Duration::from_millis(530));
    let damage = bench.list.take_damage();
    let area: f32 = damage.rects().iter().map(Rect::area).sum();
    // The caret is 1.5 × one line (about 18 px): its old place, nothing else.
    assert_ne!(damage.rects(), &[] as &[Rect]);
    assert!(area < 40.0, "damage {:?}", damage.rects());
    // A time with nothing due redraws nothing.
    bench.at(Duration::from_millis(600));
    assert_eq!(bench.list.take_damage().rects(), &[] as &[Rect]);
}

#[test]
fn text_placeholder_caret_and_selection_are_drawn_in_their_tokens() {
    let mut bench = Bench::new("");
    let rect = bench.ui.rect(bench.field).unwrap();
    // Empty and unfocused: the placeholder, in `text-muted` (antialiased: a few exact pixels).
    let image = bench.draw();
    assert!(count(&image, rect, ColorToken::SurfaceSunken) > 100);
    // Focused by a click: the caret, a column of `text`.
    bench.click(bench.at_text_x(0.0), 1);
    bench.type_text("Violin");
    let image = bench.draw();
    let caret_x = bench.at_text_x(0.0).x;
    let caret_band = Rect::new(caret_x + 25.0, rect.y0, rect.x1, rect.y1);
    assert!(count(&image, caret_band, ColorToken::Text) > 5);
    // Everything selected: a band of `selection` behind the text.
    bench.press(Key::Character("a".to_owned()), ctrl());
    let image = bench.draw();
    assert!(count(&image, rect, ColorToken::Selection) > 100);
    // Without the focus the selection is kept but not drawn.
    bench.tab();
    assert_eq!(bench.field().selection(), 0..6);
    let image = bench.draw();
    assert_eq!(count(&image, rect, ColorToken::Selection), 0);
}

#[test]
fn a_field_can_be_given_a_text_by_the_application() {
    let mut bench = Bench::new("Violin");
    let id = bench.field;
    bench
        .ui
        .widget_mut::<TextField<Msg>>(id)
        .unwrap()
        .set_text("Viola\nda gamba");
    bench.ui.request_layout(id);
    bench.ui.frame(&mut bench.list);
    // Cleaned into one line, and committed: leaving the field afterwards sends nothing.
    assert_eq!(bench.field().text(), "Viola da gamba");
    bench.tab();
    bench.tab();
    assert!(
        !bench
            .messages()
            .iter()
            .any(|m| matches!(m, Msg::Committed(_)))
    );
}

#[test]
fn the_fields_width_follows_its_parent() {
    // Laid out by a column that fills: the field takes the width it is given.
    let mut ui = Ui::new(
        orchestra_ui::Flex::column(orchestra_ui::Space::None, orchestra_ui::Space::None),
        &theme(),
        Faces::bundled().unwrap(),
    );
    let root = ui.root();
    let id = ui
        .add(
            root,
            TextField::new("", |_| ()),
            LayoutProps::default().width(Length::Fill(1.0)),
        )
        .unwrap();
    let mut list = DisplayList::new();
    ui.layout(BOUNDS, &mut list);
    let rect = ui.rect(id).unwrap();
    assert!((rect.width() - 400.0).abs() < 0.01);
    assert!((rect.height() - theme().spacing().control_height).abs() < 0.01);
}

#[test]
fn removing_a_blinking_field_drops_its_wake_up_and_its_caret() {
    let mut bench = Bench::new("Violin");
    bench.click(bench.at_text_x(0.0), 1);
    assert!(bench.ui.next_wakeup().is_some());
    let segments = bench.list.segment_count();
    bench.ui.remove(bench.field, &mut bench.list).unwrap();
    assert_eq!(bench.ui.next_wakeup(), None);
    // Both of its segments went: the main one and the caret's overlay.
    assert_eq!(bench.list.segment_count(), segments - 2);
    // A wake-up time passing afterwards wakes nobody (the slot may be reused by now).
    bench.at(Duration::from_secs(1));
    assert!(bench.list.take_damage().rects().len() <= 2);
}
