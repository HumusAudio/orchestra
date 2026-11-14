// SPDX-License-Identifier: GPL-3.0-or-later

//! Typing in a text field does not allocate (ADR-0047 TF13, F2.10b2): once the field's text,
//! its undo history, its layout, the display list and the interface's reused vectors have their
//! capacity, a typed key (its key event, its text, the shaping of the line, the re-recording of
//! the field and of its caret), a caret movement and a blink never touch the heap.
//!
//! This file is its own test binary with the counting allocator installed (ADR-0016 §5).

#![allow(
    clippy::unwrap_used,
    reason = "test setup: a failed unwrap fails the test"
)]

use std::time::{Duration, Instant};

use orchestra_test_support::assert_no_allocations;
use orchestra_text::Faces;
use orchestra_ui::{
    Appearance, BuiltInTheme, ButtonState, DisplayList, EditConvention, Flex, InputEvent, Key,
    LayoutProps, Modifiers, NamedKey, Rect, Space, TextField, Ui,
};

orchestra_test_support::install_counting_allocator!();

/// One key typed as a keyboard sends it: the key, its text, the release.
fn type_char(ui: &mut Ui<()>, list: &mut DisplayList, c: &str) {
    let key = Key::Character(c.to_owned());
    ui.input(
        &InputEvent::Key {
            key: key.clone(),
            state: ButtonState::Pressed,
            repeat: false,
            modifiers: Modifiers::default(),
        },
        list,
    );
    ui.input(&InputEvent::Text(c.to_owned()), list);
    ui.input(
        &InputEvent::Key {
            key,
            state: ButtonState::Released,
            repeat: false,
            modifiers: Modifiers::default(),
        },
        list,
    );
}

#[test]
fn typing_moving_and_blinking_never_allocate_within_capacity() {
    let theme = *BuiltInTheme::Studio.pair().instance(Appearance::Light);
    let mut ui = Ui::new(
        Flex::column(Space::M, Space::M),
        &theme,
        Faces::bundled().unwrap(),
    );
    let root = ui.root();
    let mut field = TextField::new("", |_| ()).convention(EditConvention::Linux);
    field.reserve(256, 4096, 512);
    let id = ui.add(root, field, LayoutProps::default()).unwrap();
    let mut list = DisplayList::new();
    let start = Instant::now();
    ui.advance(start, &mut list);
    ui.layout(Rect::new(0.0, 0.0, 400.0, 200.0), &mut list);
    ui.set_focus(Some(id), &mut list).unwrap();
    // The events are built outside the measured region: their strings are the test's, not the
    // field's.
    let home = InputEvent::Key {
        key: Key::Named(NamedKey::Home),
        state: ButtonState::Pressed,
        repeat: false,
        modifiers: Modifiers::default(),
    };
    let letters: Vec<InputEvent> = ["V", "i", "o", "l", "a"]
        .iter()
        .map(|c| InputEvent::Text((*c).to_owned()))
        .collect();
    // A first round gives every vector its capacity (the layout's, the segment's, the queues'):
    // a text longer than the measured round's, then emptied, so the measured text always fits.
    for c in "Viola da gamba, leggio uno".chars() {
        type_char(&mut ui, &mut list, &c.to_string());
    }
    let select_all = InputEvent::Key {
        key: Key::Character("a".to_owned()),
        state: ButtonState::Pressed,
        repeat: false,
        modifiers: Modifiers {
            primary: true,
            ..Modifiers::default()
        },
    };
    let backspace = InputEvent::Key {
        key: Key::Named(NamedKey::Backspace),
        state: ButtonState::Pressed,
        repeat: false,
        modifiers: Modifiers::default(),
    };
    ui.input(&select_all, &mut list);
    ui.input(&backspace, &mut list);
    ui.input(&home, &mut list);
    let mut elapsed = Duration::ZERO;
    for _ in 0..4 {
        elapsed += Duration::from_millis(530);
        ui.advance(start + elapsed, &mut list);
    }
    let _ = list.take_damage();
    assert_no_allocations(|| {
        for letter in &letters {
            ui.input(letter, &mut list);
        }
        ui.input(&home, &mut list);
        for _ in 0..4 {
            elapsed += Duration::from_millis(530);
            ui.advance(start + elapsed, &mut list);
        }
    });
}
