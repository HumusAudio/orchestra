// SPDX-License-Identifier: GPL-3.0-or-later

//! The shortcut editor of the Preferences' Keyboard section (ADR-0048 AM7, SCALETTA F2.12c1),
//! headless, through the demonstration: every command listed with its shortcuts, the search by
//! name and by shortcut, a shortcut added with the capture box and saved in the settings, a
//! fixed key refused with its reason, Escape cancelling, a conflict asked about and reassigned,
//! a default restored by taking it back, a shortcut removed, everything restored; and the texts
//! in the six languages, each value in its place once.
//!
//! The shortcuts are written with *Primary* and read back through the registry, so the tests
//! hold on the three platforms (Ctrl on Linux and Windows, ⌘ on macOS).

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test helpers: a failed unwrap or index fails the test"
)]

use orchestra_app::{Demo, KeyboardTexts, SceneContext, shortcut_matches};
use orchestra_i18n::{Locale, Localizer};
use orchestra_ui::{
    ActionId, ButtonState, DisplayList, InputEvent, Key, Label, ListBox, Modifiers, NamedKey,
    PhysicalKey, Point, PointerButton, Rect, Shortcut, ShortcutKey, WidgetId, WindowApp,
};

/// *Primary* alone.
const PRIMARY: Modifiers = Modifiers {
    primary: true,
    secondary: false,
    shift: false,
    alt: false,
};

/// The demonstration in en-US, in a window as large as the program's, with the Preferences open
/// on their Keyboard section (the last one: End in the list of sections, which has the focus).
fn keyboard_section() -> (Demo, DisplayList) {
    let context = SceneContext::new().unwrap();
    let mut demo = Demo::new(
        &context.faces,
        &orchestra_app::theme(),
        "Orchestra",
        &context.demo,
    );
    let mut list = DisplayList::new();
    demo.layout(Rect::from_origin_size(0.0, 0.0, 960.0, 600.0), &mut list);
    let ids = demo.actions().unwrap();
    demo.run_action(ids.preferences, &mut list);
    key(
        &mut demo,
        &mut list,
        &Key::Named(NamedKey::End),
        Modifiers::default(),
    );
    assert!(demo.shortcut_editor().is_some(), "the Keyboard section");
    (demo, list)
}

/// Presses and releases `key` with `modifiers`.
fn key(demo: &mut Demo, list: &mut DisplayList, key: &Key, modifiers: Modifiers) {
    for state in [ButtonState::Pressed, ButtonState::Released] {
        demo.input(
            &InputEvent::Key {
                key: key.clone(),
                physical: PhysicalKey::Other,
                state,
                repeat: false,
                modifiers,
            },
            list,
        );
    }
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

/// Selects the command number `index` of the list: the focus on the list (it scrolls, so its
/// centre may be out of view), Home, then Down `index` times.
fn select_command(demo: &mut Demo, list: &mut DisplayList, index: usize) {
    let commands = demo.shortcut_editor().unwrap().commands();
    demo.ui_mut().set_focus(Some(commands), list).unwrap();
    key(
        demo,
        list,
        &Key::Named(NamedKey::Home),
        Modifiers::default(),
    );
    for _ in 0..index {
        key(
            demo,
            list,
            &Key::Named(NamedKey::ArrowDown),
            Modifiers::default(),
        );
    }
}

/// The shortcut *Primary* + `c`.
fn primary(c: char) -> Shortcut {
    Shortcut::new(PRIMARY, ShortcutKey::Character(c)).unwrap()
}

/// The shortcuts of `id` now.
fn shortcuts(demo: &Demo, id: ActionId) -> Vec<Shortcut> {
    demo.ui().actions().unwrap().shortcuts(id).to_vec()
}

/// The text of the label `id`.
fn label(demo: &Demo, id: WidgetId) -> String {
    demo.ui().widget::<Label>(id).unwrap().text().to_owned()
}

#[test]
fn every_command_is_listed_with_its_shortcuts() {
    let (demo, _) = keyboard_section();
    let editor = demo.shortcut_editor().unwrap();
    let ids = demo.actions().unwrap();
    assert_eq!(
        editor.shown(),
        ids.all(),
        "all fifteen, in the menus' order"
    );
    assert_eq!(editor.selected(), Some(ids.new));
    let commands = demo
        .ui()
        .widget::<ListBox<orchestra_app::DemoMessage>>(editor.commands())
        .unwrap();
    assert_eq!(commands.len(), 15);
}

#[test]
fn the_search_finds_names_and_shortcuts() {
    assert!(shortcut_matches("Save as…", "Ctrl+Shift+S", "save"));
    assert!(shortcut_matches("Save as…", "Ctrl+Shift+S", "  SHIFT+s "));
    assert!(shortcut_matches("Anything", "", ""));
    assert!(!shortcut_matches("Open…", "Ctrl+O", "save"));
    let (mut demo, mut list) = keyboard_section();
    let ids = demo.actions().unwrap();
    let search = demo.shortcut_editor().unwrap().search();
    click(&mut demo, &mut list, search);
    demo.input(&InputEvent::Text("save".to_owned()), &mut list);
    assert_eq!(
        demo.shortcut_editor().unwrap().shown(),
        [ids.save, ids.save_as]
    );
    // By shortcut: the text of Quit's first one, in this platform's style (Windows has none).
    let registry = demo.ui().actions().unwrap();
    if let Some(&quit) = registry.shortcuts(ids.quit).first() {
        let text = registry.shortcut_label(quit).to_lowercase();
        key(&mut demo, &mut list, &Key::Character("a".into()), PRIMARY);
        demo.input(&InputEvent::Text(text), &mut list);
        assert!(demo.shortcut_editor().unwrap().shown().contains(&ids.quit));
    }
    // Nothing matches: the line says so, and nothing is selected.
    key(&mut demo, &mut list, &Key::Character("a".into()), PRIMARY);
    demo.input(&InputEvent::Text("zzz".to_owned()), &mut list);
    let editor = demo.shortcut_editor().unwrap();
    assert_eq!(editor.shown(), [] as [ActionId; 0]);
    assert_eq!(editor.selected(), None);
}

#[test]
fn a_captured_shortcut_is_added_and_saved() {
    let (mut demo, mut list) = keyboard_section();
    let ids = demo.actions().unwrap();
    select_command(&mut demo, &mut list, 1);
    assert_eq!(demo.shortcut_editor().unwrap().selected(), Some(ids.open));
    let add = demo.shortcut_editor().unwrap().add_button();
    click(&mut demo, &mut list, add);
    let capture = demo.shortcut_editor().unwrap().capture().unwrap();
    assert_eq!(demo.ui().focused(), Some(capture));
    key(&mut demo, &mut list, &Key::Character("k".into()), PRIMARY);
    assert_eq!(
        shortcuts(&demo, ids.open),
        [primary('o'), primary('k')],
        "added after its own"
    );
    let editor = demo.shortcut_editor().unwrap();
    assert_eq!(editor.capture(), None);
    assert_eq!(demo.ui().focused(), Some(editor.add_button()));
    // Saved: the difference from the defaults, by identifier, written with the abstract keys.
    let saved = &demo.settings().settings().shortcuts;
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].action, "file.open");
    assert_eq!(saved[0].keys, ["Primary+o", "Primary+k"]);
}

#[test]
fn a_fixed_key_is_refused_and_escape_cancels() {
    let (mut demo, mut list) = keyboard_section();
    let ids = demo.actions().unwrap();
    let before = shortcuts(&demo, ids.new);
    let add = demo.shortcut_editor().unwrap().add_button();
    click(&mut demo, &mut list, add);
    key(
        &mut demo,
        &mut list,
        &Key::Named(NamedKey::Enter),
        Modifiers::default(),
    );
    let notice = demo.shortcut_editor().unwrap().notice();
    // Enter as this platform writes it ("Enter" on Linux and Windows, "↩" on macOS).
    let enter = demo.ui().actions().unwrap().shortcut_label(
        Shortcut::new(Modifiers::default(), ShortcutKey::Named(NamedKey::Enter)).unwrap(),
    );
    assert_eq!(
        label(&demo, notice),
        format!(
            "{enter} can't be assigned: moving around the program, the menus or the system use it."
        )
    );
    assert!(
        demo.shortcut_editor().unwrap().capture().is_some(),
        "still waiting"
    );
    assert!(
        demo.preferences().is_some(),
        "Enter did not close the Preferences"
    );
    key(
        &mut demo,
        &mut list,
        &Key::Named(NamedKey::Escape),
        Modifiers::default(),
    );
    assert_eq!(demo.shortcut_editor().unwrap().capture(), None);
    assert_eq!(label(&demo, notice), "");
    assert!(
        demo.preferences().is_some(),
        "Escape did not close the Preferences"
    );
    assert_eq!(shortcuts(&demo, ids.new), before);
}

#[test]
fn a_conflict_is_asked_about_and_a_default_is_taken_back() {
    let (mut demo, mut list) = keyboard_section();
    let ids = demo.actions().unwrap();
    select_command(&mut demo, &mut list, 1);
    let add = demo.shortcut_editor().unwrap().add_button();
    click(&mut demo, &mut list, add);
    // New's own shortcut, for Open: the question opens over the Preferences.
    key(&mut demo, &mut list, &Key::Character("n".into()), PRIMARY);
    assert_eq!(
        demo.ui().dialogs().count(),
        2,
        "the Preferences and the question"
    );
    assert_eq!(shortcuts(&demo, ids.open), [primary('o')], "not yet");
    // Enter answers Reassign, the default button.
    key(
        &mut demo,
        &mut list,
        &Key::Named(NamedKey::Enter),
        Modifiers::default(),
    );
    assert_eq!(demo.ui().dialogs().count(), 1);
    assert_eq!(shortcuts(&demo, ids.open), [primary('o'), primary('n')]);
    assert_eq!(shortcuts(&demo, ids.new), []);
    // Restoring New asks too (Open has its default); Escape keeps things as they are.
    select_command(&mut demo, &mut list, 0);
    let restore = demo.shortcut_editor().unwrap().restore_button();
    click(&mut demo, &mut list, restore);
    assert_eq!(demo.ui().dialogs().count(), 2);
    key(
        &mut demo,
        &mut list,
        &Key::Named(NamedKey::Escape),
        Modifiers::default(),
    );
    assert_eq!(demo.ui().dialogs().count(), 1);
    assert_eq!(shortcuts(&demo, ids.new), []);
    // Asked again and reassigned: New has it back, Open loses it.
    click(&mut demo, &mut list, restore);
    key(
        &mut demo,
        &mut list,
        &Key::Named(NamedKey::Enter),
        Modifiers::default(),
    );
    assert_eq!(shortcuts(&demo, ids.new), [primary('n')]);
    assert_eq!(shortcuts(&demo, ids.open), [primary('o')]);
    assert_eq!(
        demo.settings().settings().shortcuts.len(),
        0,
        "both are defaults again"
    );
}

#[test]
fn a_shortcut_is_removed_and_everything_restored() {
    let (mut demo, mut list) = keyboard_section();
    let ids = demo.actions().unwrap();
    // Save (index 2): its only shortcut goes.
    select_command(&mut demo, &mut list, 2);
    let editor = demo.shortcut_editor().unwrap();
    let (remove, restore_all) = (editor.remove_button(), editor.restore_all_button());
    click(&mut demo, &mut list, remove);
    assert_eq!(shortcuts(&demo, ids.save), []);
    let saved = &demo.settings().settings().shortcuts;
    assert_eq!(saved.len(), 1);
    assert!(saved[0].keys.is_empty(), "no shortcut is a difference too");
    click(&mut demo, &mut list, restore_all);
    assert_eq!(shortcuts(&demo, ids.save), [primary('s')]);
    assert_eq!(demo.settings().settings().shortcuts.len(), 0);
}

#[test]
fn the_texts_exist_in_six_languages_with_each_value_once() {
    for locale in Locale::ALL {
        let texts = KeyboardTexts::new(&Localizer::new(locale).unwrap()).unwrap();
        for (template, variables) in texts.templates() {
            for index in 0..variables {
                assert_eq!(template.slots(index), 1, "{locale:?}: {template:?}");
            }
            assert_eq!(template.slots(variables), 0, "{locale:?}: {template:?}");
            let filled = template.fill(&["A", "B"]);
            assert!(
                !filled.contains(['\u{2068}', '\u{2069}', '\u{E000}', '\u{E001}']),
                "{locale:?}: {filled}"
            );
        }
        for text in [&texts.heading, &texts.search, &texts.add, &texts.reassign] {
            assert!(!text.is_empty(), "{locale:?}");
        }
    }
}

/// The demonstration's window takes the registry's changes in its menus at once: Open's new
/// shortcut runs it (a chooser is asked for).
#[test]
fn a_new_shortcut_runs_its_command() {
    let (mut demo, mut list) = keyboard_section();
    select_command(&mut demo, &mut list, 1);
    let add = demo.shortcut_editor().unwrap().add_button();
    click(&mut demo, &mut list, add);
    key(&mut demo, &mut list, &Key::Character("k".into()), PRIMARY);
    // Close the Preferences (Escape on the panel), then Primary+K.
    key(
        &mut demo,
        &mut list,
        &Key::Named(NamedKey::Escape),
        Modifiers::default(),
    );
    assert!(demo.preferences().is_none());
    key(&mut demo, &mut list, &Key::Character("k".into()), PRIMARY);
    assert!(demo.take_file_request().is_some(), "Open ran");
}

/// A fresh folder for the test `name`, in the system's temporary directory.
fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("orchestra-keys-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Presses Tab then Enter: in a question whose focus starts on *Cancel* (no default button,
/// ADR-0050 C4), this presses the other button (*Replace*).
fn tab_enter(demo: &mut Demo, list: &mut DisplayList) {
    key(demo, list, &Key::Named(NamedKey::Tab), Modifiers::default());
    key(
        demo,
        list,
        &Key::Named(NamedKey::Enter),
        Modifiers::default(),
    );
}

/// *Export…* asks for a file with the shortcut files' filter, proposes the translated name, adds
/// the extension and writes the whole keyboard (ADR-0051 SF2, SF5); the open project is not
/// touched by the answer.
#[test]
fn export_writes_the_whole_keyboard() {
    let dir = scratch("export");
    let (mut demo, mut list) = keyboard_section();
    let export = demo.shortcut_editor().unwrap().export_button();
    click(&mut demo, &mut list, export);
    let request = demo.take_file_request().unwrap();
    assert_eq!(request.kind, orchestra_ui::FileRequestKind::Save);
    assert_eq!(request.title, "Export shortcuts");
    assert_eq!(request.filter_name, "Orchestra shortcuts");
    assert_eq!(request.extensions, ["orchestra-shortcuts"]);
    assert_eq!(
        request.file_name.as_deref(),
        Some("Shortcuts.orchestra-shortcuts")
    );
    demo.file_chosen(
        orchestra_ui::FileChoice::Chosen(dir.join("Mine")),
        &mut list,
    );
    let written = orchestra_io::shortcuts::load(&dir.join("Mine.orchestra-shortcuts")).unwrap();
    assert_eq!(
        written.entries.len(),
        15,
        "every command, defaults included"
    );
    assert_eq!(written.entries[0].action, "file.new");
    assert_eq!(written.entries[0].keys, ["Primary+n"]);
    assert_eq!(
        demo.project().unwrap().project().path(),
        None,
        "not the project's"
    );
    assert_eq!(demo.ui().dialogs().count(), 1, "only the Preferences");
    // Again onto the same name typed without the extension: asked; Escape keeps the file.
    std::fs::write(dir.join("Mine.orchestra-shortcuts"), b"old").unwrap();
    click(&mut demo, &mut list, export);
    let _ = demo.take_file_request();
    demo.file_chosen(
        orchestra_ui::FileChoice::Chosen(dir.join("Mine")),
        &mut list,
    );
    assert_eq!(demo.ui().dialogs().count(), 2, "the question");
    key(
        &mut demo,
        &mut list,
        &Key::Named(NamedKey::Escape),
        Modifiers::default(),
    );
    assert_eq!(
        std::fs::read(dir.join("Mine.orchestra-shortcuts")).unwrap(),
        b"old"
    );
    // And replaced.
    click(&mut demo, &mut list, export);
    let _ = demo.take_file_request();
    demo.file_chosen(
        orchestra_ui::FileChoice::Chosen(dir.join("Mine")),
        &mut list,
    );
    tab_enter(&mut demo, &mut list);
    assert!(orchestra_io::shortcuts::load(&dir.join("Mine.orchestra-shortcuts")).is_ok());
    let _ = std::fs::remove_dir_all(&dir);
}

/// *Import…* replaces every shortcut with the file's (SF3), reports the entries not used (a fixed
/// key, an entry the reader dropped) and saves the differences; an unknown action is kept, not
/// reported.
#[test]
fn import_replaces_the_shortcuts_and_reports_what_was_not_used() {
    let dir = scratch("import");
    let path = dir.join("Theirs.orchestra-shortcuts");
    std::fs::write(
        &path,
        br#"{ "orchestra_shortcuts": 1, "shortcuts": [
              { "action": "file.open", "keys": ["Primary+k"] },
              { "action": "edit.undo", "keys": ["Tab"] },
              { "action": "future.action", "keys": ["F5"] },
              { "keys": [] }
            ] }"#,
    )
    .unwrap();
    let (mut demo, mut list) = keyboard_section();
    let ids = demo.actions().unwrap();
    let import = demo.shortcut_editor().unwrap().import_button();
    click(&mut demo, &mut list, import);
    let request = demo.take_file_request().unwrap();
    assert_eq!(request.kind, orchestra_ui::FileRequestKind::Open);
    // The defaults are in use: nothing to lose, no question.
    demo.file_chosen(orchestra_ui::FileChoice::Chosen(path.clone()), &mut list);
    assert_eq!(shortcuts(&demo, ids.open), [primary('k')]);
    assert_eq!(shortcuts(&demo, ids.undo), [], "Tab is fixed: dropped");
    assert_eq!(demo.ui().dialogs().count(), 2, "the report");
    let saved: Vec<String> = demo
        .settings()
        .settings()
        .shortcuts
        .iter()
        .map(|entry| entry.action.clone())
        .collect();
    assert_eq!(saved, ["file.open", "edit.undo", "future.action"]);
    key(
        &mut demo,
        &mut list,
        &Key::Named(NamedKey::Enter),
        Modifiers::default(),
    );
    assert_eq!(demo.ui().dialogs().count(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

/// With shortcuts of one's own, importing asks first: Escape keeps them, *Replace* replaces.
#[test]
fn import_asks_before_replacing_ones_own_shortcuts() {
    let dir = scratch("import-ask");
    let path = dir.join("Theirs.orchestra-shortcuts");
    std::fs::write(
        &path,
        br#"{ "orchestra_shortcuts": 1, "shortcuts": [ { "action": "file.save", "keys": ["Primary+j"] } ] }"#,
    )
    .unwrap();
    let (mut demo, mut list) = keyboard_section();
    let ids = demo.actions().unwrap();
    // One of one's own: Open without shortcut.
    select_command(&mut demo, &mut list, 1);
    let editor = demo.shortcut_editor().unwrap();
    let (remove, import) = (editor.remove_button(), editor.import_button());
    click(&mut demo, &mut list, remove);
    assert_eq!(shortcuts(&demo, ids.open), []);
    for replace in [false, true] {
        click(&mut demo, &mut list, import);
        let _ = demo.take_file_request();
        demo.file_chosen(orchestra_ui::FileChoice::Chosen(path.clone()), &mut list);
        assert_eq!(demo.ui().dialogs().count(), 2, "the question");
        if replace {
            tab_enter(&mut demo, &mut list);
        } else {
            key(
                &mut demo,
                &mut list,
                &Key::Named(NamedKey::Escape),
                Modifiers::default(),
            );
        }
    }
    // Replaced: Open has its default back (the file does not name it), Save has the file's.
    assert_eq!(shortcuts(&demo, ids.open), [primary('o')]);
    assert_eq!(shortcuts(&demo, ids.save), [primary('j')]);
    let _ = std::fs::remove_dir_all(&dir);
}

/// A file that is not a shortcut file is an error that names it, and changes nothing.
#[test]
fn a_file_that_cannot_be_imported_changes_nothing() {
    let dir = scratch("import-error");
    let path = dir.join("Notes.orchestra-shortcuts");
    std::fs::write(&path, b"just some text").unwrap();
    let (mut demo, mut list) = keyboard_section();
    let ids = demo.actions().unwrap();
    let before = shortcuts(&demo, ids.new);
    let import = demo.shortcut_editor().unwrap().import_button();
    click(&mut demo, &mut list, import);
    let _ = demo.take_file_request();
    demo.file_chosen(orchestra_ui::FileChoice::Chosen(path), &mut list);
    assert_eq!(demo.ui().dialogs().count(), 2, "the error");
    assert_eq!(shortcuts(&demo, ids.new), before);
    assert_eq!(demo.settings().settings().shortcuts.len(), 0);
    key(
        &mut demo,
        &mut list,
        &Key::Named(NamedKey::Enter),
        Modifiers::default(),
    );
    assert_eq!(demo.ui().dialogs().count(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}
