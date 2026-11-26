// SPDX-License-Identifier: GPL-3.0-or-later

//! The theme editor of the Preferences' Appearance section (ADR-0037 T7–T8, SCALETTA F2.12b1),
//! headless, through the demonstration: a built-in pair cannot be changed and says so; *Duplicate*
//! makes a custom theme and chooses it; a colour typed into its field becomes the theme's at once
//! (saved, and sent to the window) and *Restore* takes it back; a value that is not a colour
//! changes nothing; the fields follow the half the window shows; a pair below its contrast
//! threshold is marked and listed; the name is committed and made unique; *Delete* asks first,
//! then goes back to the base; and the texts in the six languages, each value in its place once.
//!
//! # How the widgets are reached
//! The Appearance section scrolls (24 colour rows do not fit the window), so a widget may lie
//! outside the visible part, where a click would land on something else. The tests therefore give
//! the focus to the widget directly ([`orchestra_ui::Ui::set_focus`]) and use the keyboard: Space
//! presses a button, *Primary*+A then a text replace a field's content, Enter commits it. These
//! are the same paths a keyboard user takes.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test helpers: a failed unwrap or index fails the test"
)]

use orchestra_app::{Demo, DemoMessage, SceneContext, ThemeTexts};
use orchestra_i18n::{Locale, Localizer};
use orchestra_io::settings::{ThemeChoice, ThemeOverride};
use orchestra_ui::{
    Appearance, BuiltInTheme, Button, ButtonState, Color, ColorToken, DisplayList, InputEvent, Key,
    Label, Modifiers, NamedKey, PhysicalKey, Rect, TextField, WidgetId, WindowApp, WindowRequest,
};

/// *Primary* alone (Ctrl on Linux and Windows, ⌘ on macOS: the tests hold on the three).
const PRIMARY: Modifiers = Modifiers {
    primary: true,
    secondary: false,
    shift: false,
    alt: false,
};

/// The demonstration in en-US, in a window as large as the program's, with the Preferences open
/// on their first section, Appearance, which holds the theme editor. The window requests made so
/// far are drained, so a test sees only those its own actions cause.
fn appearance_section() -> (Demo, DisplayList) {
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
    assert!(demo.theme_editor().is_some(), "the Appearance section");
    while demo.take_window_request().is_some() {}
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

/// Presses the button `id` from the keyboard: the focus on it, then Space.
fn press(demo: &mut Demo, list: &mut DisplayList, id: WidgetId) {
    demo.ui_mut().set_focus(Some(id), list).unwrap();
    key(
        demo,
        list,
        &Key::Named(NamedKey::Space),
        Modifiers::default(),
    );
}

/// Replaces the content of the field `id` with `text`, as a user would: the focus on it,
/// *Primary*+A selects everything, the text typed over it. Nothing is committed.
fn type_into(demo: &mut Demo, list: &mut DisplayList, id: WidgetId, text: &str) {
    demo.ui_mut().set_focus(Some(id), list).unwrap();
    key(demo, list, &Key::Character("a".into()), PRIMARY);
    demo.input(&InputEvent::Text(text.to_owned()), list);
}

/// The index of `token` in `ColorToken::ALL`, the rows' order.
fn row(token: ColorToken) -> usize {
    ColorToken::ALL.iter().position(|t| *t == token).unwrap()
}

/// The text of the field `id`.
fn field_text(demo: &Demo, id: WidgetId) -> String {
    demo.ui()
        .widget::<TextField<DemoMessage>>(id)
        .unwrap()
        .text()
        .to_owned()
}

/// `color` as the fields write an opaque colour: `#rrggbb`, lower case.
fn hex(color: Color) -> String {
    format!("#{:02x}{:02x}{:02x}", color.r, color.g, color.b)
}

/// The text of the label `id`.
fn label(demo: &Demo, id: WidgetId) -> String {
    demo.ui().widget::<Label>(id).unwrap().text().to_owned()
}

/// Whether the button `id` is enabled.
fn enabled(demo: &Demo, id: WidgetId) -> bool {
    demo.ui()
        .widget::<Button<DemoMessage>>(id)
        .unwrap()
        .is_enabled()
}

/// The texts of every label under `id`, depth first (the contrast lines).
fn labels_under(demo: &Demo, id: WidgetId) -> Vec<String> {
    let mut out = Vec::new();
    for child in demo.ui().children(id) {
        if let Some(label) = demo.ui().widget::<Label>(child) {
            out.push(label.text().to_owned());
        }
        out.extend(labels_under(demo, child));
    }
    out
}

/// The button of the topmost dialog whose text is `text`, searched depth first.
fn dialog_button(demo: &Demo, text: &str) -> WidgetId {
    /// The search under `id`.
    fn find(demo: &Demo, id: WidgetId, text: &str) -> Option<WidgetId> {
        if demo
            .ui()
            .widget::<Button<DemoMessage>>(id)
            .is_some_and(|button| button.text() == text)
        {
            return Some(id);
        }
        demo.ui()
            .children(id)
            .into_iter()
            .find_map(|child| find(demo, child, text))
    }
    let top = demo.ui().dialogs().last().unwrap();
    find(demo, top, text).unwrap()
}

/// The colour `token` of the pair the last theme request sends to the window, in `half`; the
/// requests are drained. `None` when no theme request was made.
fn sent_color(demo: &mut Demo, half: Appearance, token: ColorToken) -> Option<Color> {
    let mut color = None;
    while let Some(request) = demo.take_window_request() {
        if let WindowRequest::SetTheme { pair, .. } = request {
            color = Some(pair.instance(half).color(token));
        }
    }
    color
}

/// The override of `token` in the `half` of the chosen custom theme, as the settings hold it.
fn saved(demo: &Demo, half: Appearance, token: ColorToken) -> Option<u32> {
    let theme = &demo.settings().settings().theme;
    let ThemeChoice::Custom(name) = &theme.choice else {
        return None;
    };
    let custom = theme.custom.iter().find(|custom| &custom.name == name)?;
    let overrides = match half {
        Appearance::Light => &custom.light,
        Appearance::Dark => &custom.dark,
    };
    overrides.iter().find_map(|item| match item {
        ThemeOverride::Color { token: t, rgba } if t == token.name() => Some(*rgba),
        _ => None,
    })
}

/// Duplicates the theme in use (Studio at first) with the editor's *Duplicate*.
fn duplicate(demo: &mut Demo, list: &mut DisplayList) {
    let button = demo.theme_editor().unwrap().duplicate_button();
    press(demo, list, button);
}

#[test]
fn a_built_in_pair_is_duplicated_into_a_custom_theme() {
    let (mut demo, mut list) = appearance_section();
    let editor = demo.theme_editor().unwrap();
    // A built-in pair: no name, no colours, *Delete* disabled.
    assert_eq!(editor.name_field(), None);
    assert_eq!(editor.field(0), None);
    assert!(!enabled(&demo, editor.delete_button()));
    duplicate(&mut demo, &mut list);
    let theme = &demo.settings().settings().theme;
    assert_eq!(
        theme.choice,
        ThemeChoice::Custom("Studio (copy)".to_owned())
    );
    assert_eq!(theme.custom.len(), 1);
    assert_eq!(theme.custom[0].base, BuiltInTheme::default().id());
    // The theme drop-down lists it after the ten pairs and shows it.
    let editor = demo.theme_editor().unwrap();
    let name = editor.name_field().unwrap();
    assert_eq!(field_text(&demo, name), "Studio (copy)");
    assert!(enabled(&demo, editor.delete_button()));
    assert!(editor.field(ColorToken::ALL.len() - 1).is_some());
    // The theme is sent to the window, with Studio's colours (no change yet).
    let studio = BuiltInTheme::default().pair().instance(Appearance::Light);
    assert_eq!(
        sent_color(&mut demo, Appearance::Light, ColorToken::Accent),
        Some(studio.color(ColorToken::Accent))
    );
    // Duplicated again: a new name, chosen.
    duplicate(&mut demo, &mut list);
    assert_eq!(
        demo.settings().settings().theme.choice,
        ThemeChoice::Custom("Studio (copy) (copy)".to_owned())
    );
}

#[test]
fn a_color_typed_is_applied_and_restore_takes_it_back() {
    let (mut demo, mut list) = appearance_section();
    duplicate(&mut demo, &mut list);
    let _ = sent_color(&mut demo, Appearance::Light, ColorToken::Accent);
    let index = row(ColorToken::Accent);
    let editor = demo.theme_editor().unwrap();
    let (field, restore) = (
        editor.field(index).unwrap(),
        editor.restore_button(index).unwrap(),
    );
    let base = field_text(&demo, field);
    assert!(!enabled(&demo, restore), "the base's value");
    // Not a colour (yet): nothing changes, the text stays in the field.
    type_into(&mut demo, &mut list, field, "#12");
    assert_eq!(field_text(&demo, field), "#12");
    assert_eq!(saved(&demo, Appearance::Light, ColorToken::Accent), None);
    assert_eq!(
        sent_color(&mut demo, Appearance::Light, ColorToken::Accent),
        None
    );
    // A colour: saved as an override of the light half and sent to the window at once.
    type_into(&mut demo, &mut list, field, "#123456");
    assert_eq!(
        saved(&demo, Appearance::Light, ColorToken::Accent),
        Some(0x1234_56ff)
    );
    assert_eq!(
        sent_color(&mut demo, Appearance::Light, ColorToken::Accent),
        Some(Color::rgba(0x1234_56ff))
    );
    assert_eq!(saved(&demo, Appearance::Dark, ColorToken::Accent), None);
    assert!(enabled(&demo, restore));
    // Restore: the base's value back in the field, the override gone, the button disabled.
    press(&mut demo, &mut list, restore);
    assert_eq!(field_text(&demo, field), base);
    assert_eq!(saved(&demo, Appearance::Light, ColorToken::Accent), None);
    assert!(!enabled(&demo, restore));
    // Typing the base's value by hand also keeps no override (only the differences, T7).
    type_into(&mut demo, &mut list, field, "#123456");
    type_into(&mut demo, &mut list, field, &base.to_uppercase());
    assert_eq!(saved(&demo, Appearance::Light, ColorToken::Accent), None);
}

#[test]
fn the_fields_follow_the_half_on_screen() {
    let (mut demo, mut list) = appearance_section();
    duplicate(&mut demo, &mut list);
    let index = row(ColorToken::Surface);
    let editor = demo.theme_editor().unwrap();
    let (field, note) = (
        editor.field(index).unwrap(),
        editor.half_note_label().unwrap(),
    );
    let light_note = label(&demo, note);
    assert!(light_note.contains("light half"), "{light_note}");
    let studio = BuiltInTheme::default().pair();
    let light = field_text(&demo, field);
    // The window now draws the dark half: the fields and the note follow.
    demo.input(&InputEvent::Appearance(Appearance::Dark), &mut list);
    assert_eq!(demo.theme_editor().unwrap().half(), Appearance::Dark);
    let dark = field_text(&demo, field);
    assert_ne!(light, dark);
    assert_eq!(
        ThemeTexts::new(&Localizer::new(Locale::EnUs).unwrap())
            .unwrap()
            .editing_dark,
        label(&demo, note)
    );
    assert_eq!(
        dark,
        hex(studio.instance(Appearance::Dark).color(ColorToken::Surface))
    );
    // A value typed now changes the dark half only.
    type_into(&mut demo, &mut list, field, "#000000");
    assert_eq!(
        saved(&demo, Appearance::Dark, ColorToken::Surface),
        Some(0x0000_00ff)
    );
    assert_eq!(saved(&demo, Appearance::Light, ColorToken::Surface), None);
}

#[test]
fn a_pair_below_its_threshold_is_marked_and_listed() {
    let (mut demo, mut list) = appearance_section();
    duplicate(&mut demo, &mut list);
    let editor = demo.theme_editor().unwrap();
    let contrast = editor.contrast_column().unwrap();
    let (text_row, surface_row) = (row(ColorToken::Text), row(ColorToken::Surface));
    let surface = field_text(&demo, editor.field(surface_row).unwrap());
    let text_field = editor.field(text_row).unwrap();
    assert_eq!(
        labels_under(&demo, contrast),
        ["Every text and control keeps enough contrast."]
    );
    assert_eq!(label(&demo, editor.mark(text_row).unwrap()), "");
    // Normal text the colour of the window's background: both marked, the pair listed.
    type_into(&mut demo, &mut list, text_field, &surface);
    let editor = demo.theme_editor().unwrap();
    assert_eq!(label(&demo, editor.mark(text_row).unwrap()), "\u{25cf}");
    assert_eq!(label(&demo, editor.mark(surface_row).unwrap()), "\u{25cf}");
    let lines = labels_under(&demo, contrast);
    assert!(
        lines.contains(&"Normal text on Window background: 1.0:1, at least 4.5:1".to_owned()),
        "{lines:?}"
    );
    // Never a block: the colour is the theme's all the same.
    assert!(saved(&demo, Appearance::Light, ColorToken::Text).is_some());
    // Restored: no line, no mark.
    let restore = editor.restore_button(text_row).unwrap();
    press(&mut demo, &mut list, restore);
    assert_eq!(labels_under(&demo, contrast).len(), 1);
    let editor = demo.theme_editor().unwrap();
    assert_eq!(label(&demo, editor.mark(text_row).unwrap()), "");
}

#[test]
fn the_name_is_committed_and_made_unique() {
    let (mut demo, mut list) = appearance_section();
    duplicate(&mut demo, &mut list);
    duplicate(&mut demo, &mut list);
    let name = demo.theme_editor().unwrap().name_field().unwrap();
    // Typing alone commits nothing; Enter does.
    type_into(&mut demo, &mut list, name, "  Night  ");
    assert_eq!(
        demo.settings().settings().theme.choice,
        ThemeChoice::Custom("Studio (copy) (copy)".to_owned())
    );
    key(
        &mut demo,
        &mut list,
        &Key::Named(NamedKey::Enter),
        Modifiers::default(),
    );
    assert_eq!(
        demo.settings().settings().theme.choice,
        ThemeChoice::Custom("Night".to_owned())
    );
    assert_eq!(field_text(&demo, name), "Night", "trimmed");
    // The other theme's name: numbered, and the field shows the name kept.
    type_into(&mut demo, &mut list, name, "Studio (copy)");
    key(
        &mut demo,
        &mut list,
        &Key::Named(NamedKey::Enter),
        Modifiers::default(),
    );
    assert_eq!(
        demo.settings().settings().theme.choice,
        ThemeChoice::Custom("Studio (copy) (2)".to_owned())
    );
    assert_eq!(field_text(&demo, name), "Studio (copy) (2)");
}

#[test]
fn delete_asks_then_goes_back_to_the_base() {
    let (mut demo, mut list) = appearance_section();
    duplicate(&mut demo, &mut list);
    let delete = demo.theme_editor().unwrap().delete_button();
    // Escape keeps the theme.
    press(&mut demo, &mut list, delete);
    assert_eq!(
        demo.ui().dialogs().count(),
        2,
        "the Preferences and the question"
    );
    key(
        &mut demo,
        &mut list,
        &Key::Named(NamedKey::Escape),
        Modifiers::default(),
    );
    assert_eq!(demo.ui().dialogs().count(), 1);
    assert_eq!(demo.settings().settings().theme.custom.len(), 1);
    // Delete: gone, its base chosen, the editor back to the built-in note.
    press(&mut demo, &mut list, delete);
    let button = dialog_button(&demo, "Delete");
    press(&mut demo, &mut list, button);
    assert_eq!(demo.ui().dialogs().count(), 1);
    let theme = &demo.settings().settings().theme;
    assert_eq!(theme.custom.len(), 0);
    assert_eq!(
        theme.choice,
        ThemeChoice::BuiltIn(BuiltInTheme::default().id().to_owned())
    );
    let editor = demo.theme_editor().unwrap();
    assert_eq!(editor.name_field(), None);
    assert!(!enabled(&demo, editor.delete_button()));
}

#[test]
fn the_texts_exist_in_the_six_languages() {
    for locale in Locale::ALL {
        let texts = ThemeTexts::new(&Localizer::new(locale).unwrap()).unwrap();
        assert_eq!(texts.color_names.len(), ColorToken::ALL.len());
        assert!(
            texts.color_names.iter().all(|name| !name.is_empty()),
            "{locale:?}"
        );
        assert!(!texts.decimal.is_empty(), "{locale:?}");
        for (template, variables) in texts.templates() {
            for index in 0..variables {
                assert_eq!(template.slots(index), 1, "{locale:?}: {template:?}");
            }
            assert_eq!(template.slots(variables), 0, "{locale:?}: {template:?}");
        }
    }
}
