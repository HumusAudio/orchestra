// SPDX-License-Identifier: GPL-3.0-or-later

//! The file choosers of the demonstration (ADR-0050 DG4, SCALETTA F2.12a5a), headless: *Open* and
//! *Save as* ask the window for a chooser with the catalogues' texts and the `.orchestra` filter,
//! a second request waits for the first answer, a missing file chosen shows an error, *Cancelled* changes
//! nothing, *Unavailable* opens the error dialog, and the texts exist in the six languages.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test helpers: a failed unwrap or index fails the test"
)]

use std::path::{Path, PathBuf};

use orchestra_app::{Demo, FileTexts, SceneContext};
use orchestra_i18n::{Locale, Localizer};
use orchestra_ui::{
    ButtonState, DisplayList, FileChoice, FileRequestKind, InputEvent, Key, Modifiers, PhysicalKey,
    Rect, WindowApp,
};

/// The demonstration, laid out, in en-US.
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

/// Presses and releases the character `c` with *Primary* (and Shift when `shift`).
fn primary(demo: &mut Demo, list: &mut DisplayList, c: &str, shift: bool) {
    let modifiers = Modifiers {
        primary: true,
        shift,
        ..Modifiers::default()
    };
    for state in [ButtonState::Pressed, ButtonState::Released] {
        demo.input(
            &InputEvent::Key {
                key: Key::Character(c.into()),
                physical: PhysicalKey::Other,
                state,
                repeat: false,
                modifiers,
            },
            list,
        );
    }
}

/// *Open* (Ctrl+O, ⌘O) asks for a chooser of `.orchestra` files with the catalogue's texts, and
/// blocks the interface until the answer: a second Ctrl+O asks for nothing.
#[test]
fn open_asks_for_a_chooser_once() {
    let (mut demo, mut list) = demo();
    assert_eq!(demo.take_file_request(), None);
    primary(&mut demo, &mut list, "o", false);
    let request = demo.take_file_request().unwrap();
    assert_eq!(request.kind, FileRequestKind::Open);
    assert_eq!(request.title, "Open project");
    assert_eq!(request.filter_name, "Orchestra project");
    assert_eq!(request.extensions, ["orchestra"]);
    primary(&mut demo, &mut list, "o", false);
    assert_eq!(demo.take_file_request(), None, "one chooser at a time");
}

/// *Save as* (Ctrl+Shift+S, ⇧⌘S) proposes "Untitled.orchestra" for a project never saved.
#[test]
fn save_as_proposes_untitled() {
    let (mut demo, mut list) = demo();
    primary(&mut demo, &mut list, "s", true);
    let request = demo.take_file_request().unwrap();
    assert_eq!(request.kind, FileRequestKind::Save);
    assert_eq!(request.title, "Save project as");
    assert_eq!(request.file_name.as_deref(), Some("Untitled.orchestra"));
}

/// A cancelled chooser frees the interface and changes nothing; a chosen file that cannot be
/// opened (it does not exist) shows the error and keeps the project (F2.12a5b).
#[test]
fn a_cancelled_or_failed_open_keeps_the_project() {
    let (mut demo, mut list) = demo();
    primary(&mut demo, &mut list, "o", false);
    let _ = demo.take_file_request();
    demo.file_chosen(FileChoice::Cancelled, &mut list);
    assert!(!demo.ui().choosing_file());
    assert_eq!(demo.ui().dialogs().count(), 0);
    primary(&mut demo, &mut list, "o", false);
    assert!(demo.take_file_request().is_some(), "free again");
    let path = PathBuf::from("no such folder").join("first.orchestra");
    demo.file_chosen(FileChoice::Chosen(path), &mut list);
    // The reading is the worker's (ADR-0054 AR1): its answer brings the error.
    assert!(demo.wait_for_project(&mut list));
    assert_eq!(demo.ui().dialogs().count(), 1, "the error");
    assert_eq!(demo.project().unwrap().project().path(), None::<&Path>);
}

/// *Unavailable* opens the error dialog (never silence, ADR-0036 B4); its *OK* closes it.
#[test]
fn unavailable_shows_the_error() {
    let (mut demo, mut list) = demo();
    primary(&mut demo, &mut list, "o", false);
    let _ = demo.take_file_request();
    demo.file_chosen(FileChoice::Unavailable, &mut list);
    assert_eq!(demo.ui().dialogs().count(), 1);
    assert_eq!(demo.project().unwrap().project().path(), None::<&Path>);
    for state in [ButtonState::Pressed, ButtonState::Released] {
        demo.input(
            &InputEvent::Key {
                key: Key::Named(orchestra_ui::NamedKey::Enter),
                physical: PhysicalKey::Other,
                state,
                repeat: false,
                modifiers: Modifiers::default(),
            },
            &mut list,
        );
    }
    assert_eq!(demo.ui().dialogs().count(), 0);
}

/// The texts exist in the six languages, without Fluent's isolation marks, and the error names
/// the package to install in every one.
#[test]
fn the_texts_exist_in_six_languages() {
    for locale in Locale::ALL {
        let localizer = Localizer::new(locale).unwrap();
        let texts = FileTexts::new(&localizer).unwrap();
        for text in [
            &texts.untitled,
            &texts.filter,
            &texts.open_title,
            &texts.save_title,
            &texts.unavailable_title,
            &texts.unavailable_message,
            &texts.ok,
        ] {
            assert!(!text.is_empty(), "{locale:?}");
            assert!(
                !text.contains(['\u{2068}', '\u{2069}']),
                "{locale:?}: {text}"
            );
        }
        assert!(texts.filter.contains("Orchestra"), "{locale:?}");
        assert!(
            texts.unavailable_message.contains("xdg-desktop-portal"),
            "{locale:?}"
        );
    }
}
