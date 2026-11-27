// SPDX-License-Identifier: GPL-3.0-or-later

//! Changing the interface's language while the program runs (SCALETTA F2.13, ADR-0053),
//! headless, through the demonstration and the user's own path: the Preferences opened with their
//! action, the Language section reached in the list, a language chosen in the drop-down with the
//! keyboard.
//!
//! What is checked:
//! - **no text stays behind**: after the change, the accessibility tree (every control's role
//!   and name, the menu bar's titles, the window's title, our title bar's buttons) is exactly the
//!   one of a demonstration built from the start in that language, for every language, from
//!   en-US and back, with the system's frame and with ours;
//! - what the user changed survives: the controls' values, the field's text, the name above the
//!   staff, the divider, a shortcut of their own, the open project and its unsaved changes;
//! - the Preferences open again on Language, the focus on the drop-down, the choice saved;
//! - *System language* reads the system's languages again and names the one they give;
//! - choosing the language already shown rebuilds nothing; a change waits while another dialog
//!   is open.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test helpers: a failed unwrap or index fails the test"
)]

use std::cell::RefCell;
use std::rc::Rc;

use accesskit::Role;
use orchestra_app::{Demo, DemoMessage, DemoTexts, SceneContext, StaffPreview};
use orchestra_core_model::Command;
use orchestra_core_time::BarIndex;
use orchestra_i18n::{Locale, Localizer};
use orchestra_ui::{
    ButtonState, Decorations, DisplayList, DropDown, FixedLanguages, InputEvent, Key,
    LanguageSource, ListBox, Modifiers, NamedKey, PhysicalKey, Rect, Shortcut, ShortcutKey, Slider,
    TextField, Toggle, WidgetId, WindowApp,
};

/// The window's size, the program's (`main.rs`).
fn window() -> Rect {
    Rect::from_origin_size(0.0, 0.0, 960.0, 600.0)
}

/// The demonstration built from the start in `locale`, with the frame `decorations` draws, laid
/// out in the program's window. Its language source is the default one (no languages: *System
/// language* means en-US), as in the screenshots.
fn demo_in(locale: Locale, decorations: Decorations) -> (Demo, DisplayList) {
    let context = SceneContext::new().unwrap();
    let localizer = Localizer::new(locale).unwrap();
    let texts = DemoTexts::new(&localizer).unwrap();
    let title = localizer.format("app-name", None).unwrap();
    let mut demo = Demo::with_decorations(
        &context.faces,
        &orchestra_app::theme(),
        &title,
        &texts,
        decorations,
        None,
    );
    let mut list = DisplayList::new();
    demo.layout(window(), &mut list);
    (demo, list)
}

/// Presses and releases `key`.
fn key(demo: &mut Demo, list: &mut DisplayList, key: &Key) {
    for state in [ButtonState::Pressed, ButtonState::Released] {
        demo.input(
            &InputEvent::Key {
                key: key.clone(),
                physical: PhysicalKey::Other,
                state,
                repeat: false,
                modifiers: Modifiers::default(),
            },
            list,
        );
    }
}

/// Opens the Preferences with their action and moves to the Language section: the list of
/// sections has the focus, Language is its second entry.
fn open_language_section(demo: &mut Demo, list: &mut DisplayList) {
    let ids = demo.actions().unwrap();
    demo.run_action(ids.preferences, list);
    key(demo, list, &Key::Named(NamedKey::ArrowDown));
    assert!(demo.language_choice().is_some(), "the Language section");
}

/// Chooses the drop-down's value number `index` (0: *System language*, then [`Locale::ALL`]'s
/// order) as the user does: the focus on the drop-down, Enter opens its list on the value in use,
/// the arrows move to `index`, Enter chooses.
fn choose_language(demo: &mut Demo, list: &mut DisplayList, index: usize) {
    let choice = demo.language_choice().unwrap();
    demo.ui_mut().set_focus(Some(choice), list).unwrap();
    let selected = demo
        .ui()
        .widget::<DropDown<DemoMessage>>(choice)
        .unwrap()
        .selected();
    key(demo, list, &Key::Named(NamedKey::Enter));
    let step = if index > selected {
        NamedKey::ArrowDown
    } else {
        NamedKey::ArrowUp
    };
    for _ in 0..index.abs_diff(selected) {
        key(demo, list, &Key::Named(step));
    }
    key(demo, list, &Key::Named(NamedKey::Enter));
}

/// The drop-down's index of `locale`.
fn index_of(locale: Locale) -> usize {
    Locale::ALL
        .iter()
        .position(|known| *known == locale)
        .unwrap()
        + 1
}

/// Every node of the whole accessibility tree as (role, name), sorted: what a screen reader can
/// read, without the IDs (they differ between two builds of the same interface).
fn texts(demo: &mut Demo) -> Vec<(Role, Option<String>)> {
    let update = demo.access_update(true).unwrap();
    let mut nodes: Vec<_> = update
        .tree_update()
        .nodes
        .iter()
        .map(|(_, node)| (node.role(), node.label().map(str::to_owned)))
        .collect();
    nodes.sort_by(|a, b| format!("{a:?}").cmp(&format!("{b:?}")));
    nodes
}

/// Every action's name, mnemonic and shortcut text in the registry: what the menus' items and the
/// shortcut editor show (closed menus are not in the accessibility tree).
fn registry_texts(demo: &Demo) -> Vec<(String, Option<char>, String)> {
    let registry = demo.ui().actions().unwrap();
    registry
        .ids()
        .map(|id| {
            (
                registry.name_text(id).to_owned(),
                registry.mnemonic(id),
                registry.shortcut_text(id).to_owned(),
            )
        })
        .collect()
}

/// For every language, from en-US and back: after the change the interface says exactly what an
/// interface built in that language says, with the Preferences open on Language in both. Done
/// with the system's frame (the drawn menu bar) and with ours (the title bar and its buttons).
#[test]
fn no_text_stays_in_the_old_language() {
    for decorations in [Decorations::System, Decorations::Own] {
        for target in Locale::ALL.into_iter().skip(1) {
            let (mut changed, mut list) = demo_in(Locale::EnUs, decorations);
            open_language_section(&mut changed, &mut list);
            choose_language(&mut changed, &mut list, index_of(target));
            assert_eq!(changed.locale(), target);
            let (mut fresh, mut fresh_list) = demo_in(target, decorations);
            open_language_section(&mut fresh, &mut fresh_list);
            assert_eq!(
                texts(&mut changed),
                texts(&mut fresh),
                "en-US → {target}, {decorations:?}"
            );
            assert_eq!(changed.window_title(), fresh.window_title());
            assert_eq!(
                registry_texts(&changed),
                registry_texts(&fresh),
                "the menus' items and the shortcuts' texts, {target}"
            );
            // And back to en-US.
            choose_language(&mut changed, &mut list, index_of(Locale::EnUs));
            let (mut english, mut english_list) = demo_in(Locale::EnUs, decorations);
            open_language_section(&mut english, &mut english_list);
            assert_eq!(
                texts(&mut changed),
                texts(&mut english),
                "{target} → en-US, {decorations:?}"
            );
        }
    }
}

/// The controls' values, the field's text, the name above the staff, the divider, a shortcut of
/// the user's, the open project with its unsaved changes: all still there in the new language.
#[test]
fn what_the_user_changed_survives() {
    let (mut demo, mut list) = demo_in(Locale::EnUs, Decorations::System);
    // The options: the metronome off, the count-in on, the volume at 30, a track name typed and
    // committed (Enter), which the staff writes above it.
    let (metronome, count_in, volume) = controls(&demo);
    // Set on the widgets themselves: what matters here is that the values survive, not how
    // they were changed (the demonstration's own tests drive the controls).
    for (id, on) in [(metronome, false), (count_in, true)] {
        demo.ui_mut()
            .widget_mut::<Toggle<DemoMessage>>(id)
            .unwrap()
            .set_on(on);
    }
    demo.ui_mut()
        .widget_mut::<Slider<DemoMessage>>(volume)
        .unwrap()
        .set_value(30.0);
    let field = demo.track_name();
    demo.ui_mut().set_focus(Some(field), &mut list).unwrap();
    demo.input(&InputEvent::Text("Violino I".into()), &mut list);
    key(&mut demo, &mut list, &Key::Named(NamedKey::Enter));
    // A shortcut of the user's own for About: Primary+Shift+A.
    let ids = demo.actions().unwrap();
    let shortcut = Shortcut::new(
        Modifiers {
            primary: true,
            secondary: false,
            shift: true,
            alt: false,
        },
        ShortcutKey::Character('a'),
    )
    .unwrap();
    demo.ui_mut()
        .actions_mut()
        .unwrap()
        .set_shortcuts(ids.about, &[shortcut])
        .unwrap();
    // The open project gets unsaved changes.
    demo.project_mut()
        .unwrap()
        .project_mut()
        .editor_mut()
        .execute(&Command::InsertBars {
            at: BarIndex::new(0),
            count: 2,
        })
        .unwrap();
    demo.refresh_title();
    let document = demo
        .project()
        .unwrap()
        .project()
        .editor()
        .document()
        .clone();
    let before = values(&demo);
    assert_eq!(
        before,
        (false, true, 30.0, "Violino I".to_owned()),
        "the changes were made"
    );

    open_language_section(&mut demo, &mut list);
    choose_language(&mut demo, &mut list, index_of(Locale::It));
    assert_eq!(demo.locale(), Locale::It);
    assert_eq!(values(&demo), before);
    let staff = demo.ui().widget::<StaffPreview>(demo.staff()).unwrap();
    assert_eq!(staff.name(), "Violino I");
    assert_eq!(demo.actions(), Some(ids), "the same actions, renamed");
    assert_eq!(
        demo.ui().actions().unwrap().shortcuts(ids.about),
        &[shortcut]
    );
    let project = demo.project().unwrap().project();
    assert!(project.is_modified());
    assert!(project.editor().document().same_content(&document));
    // "Senza titolo", with the modification mark, in Italian.
    let (fresh, _) = demo_in(Locale::It, Decorations::System);
    assert_ne!(demo.window_title(), fresh.window_title(), "the mark");
    assert!(
        demo.window_title()
            .contains(fresh.window_title().split(' ').next().unwrap())
    );
}

/// The content's metronome check box, count-in switch and volume slider: the window's first two
/// toggles and its only slider, whatever their names' language.
fn controls(demo: &Demo) -> (WidgetId, WidgetId, WidgetId) {
    let ui = demo.ui();
    let all = descendants(demo, ui.root());
    let toggles: Vec<WidgetId> = all
        .iter()
        .copied()
        .filter(|id| ui.widget::<Toggle<DemoMessage>>(*id).is_some())
        .collect();
    let slider = all
        .iter()
        .copied()
        .find(|id| ui.widget::<Slider<DemoMessage>>(*id).is_some())
        .unwrap();
    (toggles[0], toggles[1], slider)
}

/// The content's values: the metronome, the count-in, the volume, the field's text.
fn values(demo: &Demo) -> (bool, bool, f64, String) {
    let ui = demo.ui();
    let (metronome, count_in, volume) = controls(demo);
    let on = |id| ui.widget::<Toggle<DemoMessage>>(id).unwrap().is_on();
    let field = ui
        .widget::<TextField<DemoMessage>>(demo.track_name())
        .unwrap()
        .text()
        .to_owned();
    (
        on(metronome),
        on(count_in),
        ui.widget::<Slider<DemoMessage>>(volume).unwrap().value(),
        field,
    )
}

/// `id` and every widget under it, depth first.
fn descendants(demo: &Demo, id: WidgetId) -> Vec<WidgetId> {
    let mut out = vec![id];
    for child in demo.ui().children(id) {
        out.extend(descendants(demo, child));
    }
    out
}

/// After the change the Preferences are open again on Language, the focus on the drop-down
/// (where the user was), and the choice is in the settings.
#[test]
fn the_preferences_open_again_on_language() {
    let (mut demo, mut list) = demo_in(Locale::EnUs, Decorations::System);
    open_language_section(&mut demo, &mut list);
    let panel = demo.preferences();
    choose_language(&mut demo, &mut list, index_of(Locale::Fr));
    assert!(demo.preferences().is_some());
    assert_ne!(demo.preferences(), panel, "built again");
    let choice = demo.language_choice().unwrap();
    assert_eq!(demo.ui().focused(), Some(choice));
    let drop_down = demo.ui().widget::<DropDown<DemoMessage>>(choice).unwrap();
    assert_eq!(drop_down.selected(), index_of(Locale::Fr));
    assert_eq!(drop_down.values()[index_of(Locale::Fr)], "Français");
    // The list of sections shows Language.
    let sections = descendants(&demo, demo.preferences().unwrap())
        .into_iter()
        .find_map(|id| demo.ui().widget::<ListBox<DemoMessage>>(id))
        .unwrap();
    assert_eq!(sections.selected(), 1);
    assert_eq!(
        demo.settings().settings().language,
        orchestra_io::settings::Language::Tag("fr".to_owned())
    );
}

/// *System language* names the language the system's languages give, reads them again when it is
/// chosen, and shows the interface in it.
#[test]
fn system_language_reads_the_system_again() {
    let (mut demo, mut list) = demo_in(Locale::EnUs, Decorations::System);
    demo.set_language_source(Box::new(FixedLanguages::new(["pt-BR", "de-CH"])));
    assert_eq!(demo.system_locale(), Locale::De);
    open_language_section(&mut demo, &mut list);
    let choice = demo.language_choice().unwrap();
    let first = |demo: &Demo| {
        demo.ui()
            .widget::<DropDown<DemoMessage>>(demo.language_choice().unwrap())
            .unwrap()
            .values()[0]
            .clone()
    };
    assert_eq!(first(&demo), "System language (Deutsch)");
    // English (UK) explicitly, then the system's: German.
    choose_language(&mut demo, &mut list, index_of(Locale::EnGb));
    assert_eq!(demo.locale(), Locale::EnGb);
    let _ = choice;
    choose_language(&mut demo, &mut list, 0);
    assert_eq!(demo.locale(), Locale::De);
    assert_eq!(first(&demo), "Systemsprache (Deutsch)");
    assert_eq!(
        demo.settings().settings().language,
        orchestra_io::settings::Language::System
    );
    // The system's languages change behind the program's back (a source that answers
    // something else from now on): *System language* chosen again, after another value
    // (choosing the value already shown sends nothing), reads them again.
    let languages = Rc::new(RefCell::new(vec!["de-DE".to_owned()]));
    demo.set_language_source(Box::new(Changing(Rc::clone(&languages))));
    *languages.borrow_mut() = vec!["es-MX".to_owned()];
    choose_language(&mut demo, &mut list, index_of(Locale::EnUs));
    assert_eq!(
        first(&demo),
        "System language (Deutsch)",
        "not read again yet"
    );
    choose_language(&mut demo, &mut list, 0);
    assert_eq!(demo.locale(), Locale::Es);
    assert_eq!(first(&demo), "Idioma del sistema (Español)");
}

/// A language source whose answer the test changes while the demonstration holds it.
#[derive(Debug)]
struct Changing(Rc<RefCell<Vec<String>>>);

impl LanguageSource for Changing {
    fn preferred_languages(&self) -> Vec<String> {
        self.0.borrow().clone()
    }
}

/// Choosing the language already shown rebuilds nothing: the content keeps its widgets.
#[test]
fn the_same_language_rebuilds_nothing() {
    let (mut demo, mut list) = demo_in(Locale::EnUs, Decorations::System);
    let title = demo.title();
    open_language_section(&mut demo, &mut list);
    let panel = demo.preferences();
    choose_language(&mut demo, &mut list, index_of(Locale::EnUs));
    assert_eq!(demo.title(), title);
    assert_eq!(demo.preferences(), panel);
    assert_eq!(demo.set_language(Locale::EnUs, &mut list), Ok(false));
}

/// While a dialog other than the Preferences is open (here About), a change waits: closing it
/// would lose what it was asking.
#[test]
fn another_dialog_keeps_the_language_waiting() {
    let (mut demo, mut list) = demo_in(Locale::EnUs, Decorations::System);
    let ids = demo.actions().unwrap();
    demo.run_action(ids.about, &mut list);
    assert_eq!(demo.ui().dialogs().count(), 1);
    assert_eq!(demo.set_language(Locale::It, &mut list), Ok(false));
    assert_eq!(demo.locale(), Locale::EnUs);
    assert_eq!(demo.ui().dialogs().count(), 1, "About is still open");
    // Without the Preferences open, a change rebuilds the window and opens nothing.
    key(&mut demo, &mut list, &Key::Named(NamedKey::Escape));
    assert_eq!(demo.ui().dialogs().count(), 0);
    assert_eq!(demo.set_language(Locale::It, &mut list), Ok(true));
    assert_eq!(demo.ui().dialogs().count(), 0);
    let (mut fresh, _) = demo_in(Locale::It, Decorations::System);
    assert_eq!(texts(&mut demo), texts(&mut fresh));
}
