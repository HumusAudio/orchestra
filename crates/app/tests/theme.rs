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
fn a_colour_hard_to_read_is_explained_at_the_top() {
    let (mut demo, mut list) = appearance_section();
    duplicate(&mut demo, &mut list);
    let editor = demo.theme_editor().unwrap();
    let column = editor.readability_column().unwrap();
    let (text_row, surface_row) = (row(ColorToken::Text), row(ColorToken::Surface));
    let surface = field_text(&demo, editor.field(surface_row).unwrap());
    let text_field = editor.field(text_row).unwrap();
    // At the top: above the first colour's row, so it is seen without scrolling.
    let top = demo.ui().rect(column).unwrap().y0;
    assert!(top < demo.ui().rect(editor.field(0).unwrap()).unwrap().y0);
    assert_eq!(
        labels_under(&demo, column),
        ["Every text and control is easy to read."]
    );
    assert_eq!(label(&demo, editor.mark(text_row).unwrap()), "");
    // Normal text the colour of the window's background: both marked, one plain sentence that
    // names the two colours as the rows do, says how bad it is and what to do; no numbers.
    type_into(&mut demo, &mut list, text_field, &surface);
    let editor = demo.theme_editor().unwrap();
    assert_eq!(label(&demo, editor.mark(text_row).unwrap()), "\u{25cf}");
    assert_eq!(label(&demo, editor.mark(surface_row).unwrap()), "\u{25cf}");
    let lines = labels_under(&demo, column);
    assert_eq!(lines.len(), 2, "the dot and one sentence: {lines:?}");
    assert_eq!(lines[0], "\u{25cf}");
    let sentence = &lines[1];
    // Light text on the light half: the advice is to make it darker, the only way that works.
    assert_eq!(
        sentence,
        "The color “Normal text” is almost invisible on “Window background”. The color “Normal \
         text” has the same problem with 6 other colors. Make the color “Normal text” darker."
    );
    assert!(!sentence.contains(':'), "no ratio: {sentence}");
    // Never a block: the colour is the theme's all the same.
    assert!(saved(&demo, Appearance::Light, ColorToken::Text).is_some());
    // Restored: the reassuring line again, no mark.
    let restore = editor.restore_button(text_row).unwrap();
    press(&mut demo, &mut list, restore);
    assert_eq!(
        labels_under(&demo, column),
        ["Every text and control is easy to read."]
    );
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
        for template in texts
            .templates()
            .into_iter()
            .chain(texts.shared_templates())
        {
            assert_eq!(template.slots(0), 1, "{locale:?}: {template:?}");
            assert_eq!(template.slots(1), 0, "{locale:?}: {template:?}");
        }
        // The readability sentences: each colour's name where it belongs, nothing else left to
        // fill, no Fluent syntax left over.
        for (template, [first, second]) in texts.readability_templates() {
            assert_eq!(template.slots(0), first, "{locale:?}: {template:?}");
            assert_eq!(template.slots(1), second, "{locale:?}: {template:?}");
            assert_eq!(template.slots(2), 0, "{locale:?}: {template:?}");
            let text = template.fill(&["A", "B"]);
            assert!(!text.contains(['{', '}']), "{locale:?}: {text}");
        }
    }
}

/// The controls of `demo`'s open Preferences section that draw a focus ring, under the first
/// scroll area found below `id`, with that area: (viewport, controls' rectangles).
fn ring_room(demo: &Demo, id: WidgetId) -> Option<(Rect, Vec<Rect>)> {
    /// Collects the rectangles of the controls under `id`, depth first.
    fn controls(demo: &Demo, id: WidgetId, out: &mut Vec<Rect>) {
        for child in demo.ui().children(id) {
            let ui = demo.ui();
            let control = ui.widget::<Button<DemoMessage>>(child).is_some()
                || ui.widget::<TextField<DemoMessage>>(child).is_some()
                || ui
                    .widget::<orchestra_ui::DropDown<DemoMessage>>(child)
                    .is_some()
                || ui
                    .widget::<orchestra_ui::RadioGroup<DemoMessage>>(child)
                    .is_some()
                || ui
                    .widget::<orchestra_ui::Toggle<DemoMessage>>(child)
                    .is_some()
                || ui
                    .widget::<orchestra_ui::Slider<DemoMessage>>(child)
                    .is_some()
                || ui
                    .widget::<orchestra_ui::ListBox<DemoMessage>>(child)
                    .is_some();
            if control {
                out.push(ui.rect(child).unwrap());
            }
            controls(demo, child, out);
        }
    }
    for child in demo.ui().children(id) {
        if demo
            .ui()
            .widget::<orchestra_ui::ScrollArea>(child)
            .is_some()
        {
            let mut out = Vec::new();
            controls(demo, child, &mut out);
            return Some((demo.ui().rect(child).unwrap(), out));
        }
        if let Some(found) = ring_room(demo, child) {
            return Some(found);
        }
    }
    None
}

/// Every control of the Appearance section (with a custom theme, so the editor's rows are there)
/// and of the Playback section leaves room for its focus ring inside the scroll area, which clips:
/// a ring reaches 1 px + `focus-width` (2 px) beyond its control. Left and right for every
/// control; at the top for those in view with the area scrolled to its start. The owner saw
/// *Duplicate*'s ring cut on the left (2026-11-26).
#[test]
fn focus_rings_are_not_cut_by_the_scroll_areas() {
    const REACH: f32 = 3.0;
    let (mut demo, mut list) = appearance_section();
    duplicate(&mut demo, &mut list);
    let check = |demo: &Demo, section: &str| {
        let panel = demo.preferences().unwrap();
        let (viewport, rects) = ring_room(demo, panel).unwrap();
        assert!(rects.len() > 3, "{section}: controls found");
        for rect in rects {
            assert!(
                rect.x0 - REACH >= viewport.x0,
                "{section}: {rect:?} in {viewport:?}"
            );
            assert!(
                rect.x1 + REACH <= viewport.x1,
                "{section}: {rect:?} in {viewport:?}"
            );
            if rect.y1 > viewport.y0 && rect.y0 < viewport.y1 {
                assert!(
                    rect.y0 - REACH >= viewport.y0,
                    "{section}: {rect:?} in {viewport:?}"
                );
            }
        }
    };
    // *Duplicate* with the focus: the section scrolls back to its top.
    let duplicate_button = demo.theme_editor().unwrap().duplicate_button();
    demo.ui_mut()
        .set_focus(Some(duplicate_button), &mut list)
        .unwrap();
    check(&demo, "Appearance");
    // Playback, in a new demonstration: the Preferences open with the focus on the list of
    // sections, where Down twice chooses the third section.
    let (mut demo, mut list) = appearance_section();
    for _ in 0..2 {
        key(
            &mut demo,
            &mut list,
            &Key::Named(NamedKey::ArrowDown),
            Modifiers::default(),
        );
    }
    assert!(demo.theme_editor().is_none(), "another section");
    check(&demo, "Playback");
    // Keyboard, the next section: the commands' list is the scroll area's only control.
    key(
        &mut demo,
        &mut list,
        &Key::Named(NamedKey::ArrowDown),
        Modifiers::default(),
    );
    let panel = demo.preferences().unwrap();
    let (viewport, rects) = ring_room(&demo, panel).unwrap();
    assert_eq!(rects.len(), 1, "the list of commands");
    let rect = rects[0];
    assert!(
        rect.x0 - REACH >= viewport.x0 && rect.x1 + REACH <= viewport.x1,
        "{rect:?}"
    );
    assert!(rect.y0 - REACH >= viewport.y0, "{rect:?} in {viewport:?}");
}

/// The theme the last theme request sends to the window, in `half`; the requests are drained.
fn sent_theme(demo: &mut Demo, half: Appearance) -> Option<orchestra_ui::Theme> {
    let mut theme = None;
    while let Some(request) = demo.take_window_request() {
        if let WindowRequest::SetTheme { pair, .. } = request {
            theme = Some(*pair.instance(half));
        }
    }
    theme
}

/// The shared overrides (not colours) of the chosen custom theme's `half`, as saved.
fn shared_saved(demo: &Demo, half: Appearance) -> Vec<ThemeOverride> {
    let theme = &demo.settings().settings().theme;
    let ThemeChoice::Custom(name) = &theme.choice else {
        return Vec::new();
    };
    let custom = theme.custom.iter().find(|c| &c.name == name).unwrap();
    let list = match half {
        Appearance::Light => &custom.light,
        Appearance::Dark => &custom.dark,
    };
    list.iter()
        .filter(|item| !matches!(item, ThemeOverride::Color { .. }))
        .cloned()
        .collect()
}

/// Presses `key` with no modifier.
fn press_key(demo: &mut Demo, list: &mut DisplayList, named: NamedKey) {
    key(demo, list, &Key::Named(named), Modifiers::default());
}

/// A built-in pair has no text, lines or density rows; a custom theme has them, above the
/// colours (owner's choice: reached without scrolling past the 24 colours).
#[test]
fn the_text_lines_and_density_rows_come_before_the_colours() {
    let (mut demo, mut list) = appearance_section();
    assert!(demo.theme_editor().unwrap().shared_rows().is_none());
    duplicate(&mut demo, &mut list);
    let editor = demo.theme_editor().unwrap();
    let rows = editor.shared_rows().unwrap();
    let y = |id: WidgetId| demo.ui().rect(id).unwrap().y0;
    let first_colour = y(editor.field(0).unwrap());
    assert!(y(rows.text_scale.slider) < y(rows.shapes[0].slider));
    assert!(y(rows.shapes[5].slider) < y(rows.density.control));
    assert!(y(rows.density.control) < first_colour);
    assert_eq!(label(&demo, rows.text_scale.value), "100%");
    assert_eq!(label(&demo, rows.shapes[2].value), "2\u{a0}px");
    for id in [
        rows.text_scale.restore,
        rows.shapes[0].restore,
        rows.density.restore,
    ] {
        assert!(!enabled(&demo, id), "the base's values");
    }
}

/// The text size: a step up is saved in both halves, sent to the window (body text 13 → 14 px)
/// and written beside the slider; *Restore* brings everything back, the slider included.
#[test]
fn the_text_size_scales_every_size_in_both_halves() {
    let (mut demo, mut list) = appearance_section();
    duplicate(&mut demo, &mut list);
    let _ = sent_theme(&mut demo, Appearance::Light);
    let rows = *demo.theme_editor().unwrap().shared_rows().unwrap();
    demo.ui_mut()
        .set_focus(Some(rows.text_scale.slider), &mut list)
        .unwrap();
    press_key(&mut demo, &mut list, NamedKey::ArrowRight);
    assert_eq!(label(&demo, rows.text_scale.value), "105%");
    let light = shared_saved(&demo, Appearance::Light);
    assert_ne!(light, [], "the six sizes are overridden");
    assert_eq!(light, shared_saved(&demo, Appearance::Dark), "both halves");
    for half in [Appearance::Light, Appearance::Dark] {
        let size = sent_theme(&mut demo, half)
            .unwrap()
            .typography()
            .size(orchestra_ui::SizeToken::Body);
        assert_eq!(size, 14, "13 × 1.05 rounded");
        press_key(&mut demo, &mut list, NamedKey::ArrowRight);
        press_key(&mut demo, &mut list, NamedKey::ArrowLeft);
    }
    assert!(enabled(&demo, rows.text_scale.restore));
    press(&mut demo, &mut list, rows.text_scale.restore);
    assert_eq!(label(&demo, rows.text_scale.value), "100%");
    assert_eq!(shared_saved(&demo, Appearance::Light), []);
    let slider = demo
        .ui()
        .widget::<orchestra_ui::Slider<DemoMessage>>(rows.text_scale.slider)
        .unwrap();
    assert!(
        (slider.value() - 100.0).abs() < f64::EPSILON,
        "the slider moved back"
    );
}

/// A shape stops at its limits (Home and End), is the same in both halves and reaches the
/// window; the density and a family are saved and sent too, and restored.
#[test]
fn shapes_families_and_density_are_saved_and_sent() {
    let (mut demo, mut list) = appearance_section();
    duplicate(&mut demo, &mut list);
    let rows = *demo.theme_editor().unwrap().shared_rows().unwrap();
    // The focus decoration: 2 to 6 px, never below WCAG's 2.
    demo.ui_mut()
        .set_focus(Some(rows.shapes[2].slider), &mut list)
        .unwrap();
    press_key(&mut demo, &mut list, NamedKey::End);
    assert_eq!(label(&demo, rows.shapes[2].value), "6\u{a0}px");
    let sent = sent_theme(&mut demo, Appearance::Dark).unwrap();
    assert_eq!(sent.shape().get(orchestra_ui::ShapeToken::FocusWidth), 6);
    press_key(&mut demo, &mut list, NamedKey::Home);
    assert_eq!(label(&demo, rows.shapes[2].value), "2\u{a0}px");
    // Corners: 0 to 16 px.
    demo.ui_mut()
        .set_focus(Some(rows.shapes[0].slider), &mut list)
        .unwrap();
    press_key(&mut demo, &mut list, NamedKey::End);
    assert_eq!(label(&demo, rows.shapes[0].value), "16\u{a0}px");
    assert_eq!(
        shared_saved(&demo, Appearance::Light),
        shared_saved(&demo, Appearance::Dark)
    );
    // The density: Compact, the group's second choice.
    demo.ui_mut()
        .set_focus(Some(rows.density.control), &mut list)
        .unwrap();
    press_key(&mut demo, &mut list, NamedKey::End);
    let sent = sent_theme(&mut demo, Appearance::Light).unwrap();
    assert_eq!(sent.density(), orchestra_ui::Density::Compact);
    assert!(enabled(&demo, rows.density.restore));
    // The interface font: JetBrains Mono, the drop-down's last choice (Space opens it).
    demo.ui_mut()
        .set_focus(Some(rows.families[0].control), &mut list)
        .unwrap();
    press_key(&mut demo, &mut list, NamedKey::Space);
    press_key(&mut demo, &mut list, NamedKey::End);
    press_key(&mut demo, &mut list, NamedKey::Enter);
    let sent = sent_theme(&mut demo, Appearance::Light).unwrap();
    assert_eq!(
        sent.typography().family(orchestra_ui::FamilyToken::Ui),
        orchestra_ui::Family::JetBrainsMono
    );
    // Everything restored, each control checked right after its own *Restore* (a later one
    // must not be what moves it back): no override left.
    press(&mut demo, &mut list, rows.density.restore);
    let group = demo
        .ui()
        .widget::<orchestra_ui::RadioGroup<DemoMessage>>(rows.density.control)
        .unwrap();
    assert_eq!(group.selected(), 0, "back to Comfortable");
    press(&mut demo, &mut list, rows.families[0].restore);
    let drop = demo
        .ui()
        .widget::<orchestra_ui::DropDown<DemoMessage>>(rows.families[0].control)
        .unwrap();
    assert_eq!(drop.selected(), 0, "back to Inter");
    press(&mut demo, &mut list, rows.shapes[0].restore);
    assert_eq!(shared_saved(&demo, Appearance::Light), []);
    assert_eq!(shared_saved(&demo, Appearance::Dark), []);
    let corners = BuiltInTheme::default()
        .pair()
        .instance(Appearance::Light)
        .shape()
        .get(orchestra_ui::ShapeToken::CornerRadius);
    assert_eq!(
        label(&demo, rows.shapes[0].value),
        format!("{corners}\u{a0}px")
    );
}

/// Every control of the Appearance section fits the section's width, its focus ring included, in
/// every language and with the text at its largest (150%): the owner saw the text rows and their
/// *Restore* buttons cut at the window's right edge in Italian (2026-11-26).
#[test]
fn the_rows_fit_in_every_language_with_the_largest_text() {
    const REACH: f32 = 3.0;
    let faces = orchestra_text::Faces::bundled().unwrap();
    for locale in Locale::ALL {
        let localizer = Localizer::new(locale).unwrap();
        let texts = orchestra_app::DemoTexts::new(&localizer).unwrap();
        let mut demo = Demo::new(&faces, &orchestra_app::theme(), "Orchestra", &texts);
        let mut list = DisplayList::new();
        demo.layout(Rect::from_origin_size(0.0, 0.0, 960.0, 600.0), &mut list);
        let ids = demo.actions().unwrap();
        demo.run_action(ids.preferences, &mut list);
        duplicate(&mut demo, &mut list);
        let rows = *demo.theme_editor().unwrap().shared_rows().unwrap();
        demo.ui_mut()
            .set_focus(Some(rows.text_scale.slider), &mut list)
            .unwrap();
        press_key(&mut demo, &mut list, NamedKey::End);
        demo.layout(Rect::from_origin_size(0.0, 0.0, 960.0, 600.0), &mut list);
        let panel = demo.preferences().unwrap();
        let (viewport, rects) = ring_room(&demo, panel).unwrap();
        for rect in rects {
            assert!(
                rect.x1 + REACH <= viewport.x1 && rect.x0 - REACH >= viewport.x0,
                "{locale:?}: {rect:?} outside {viewport:?}"
            );
        }
        // The other sections, with the same large text: every control inside the pane. The
        // list of sections has the focus after Home on it; Down goes to the next section.
        for section in 1..4 {
            demo.ui_mut().set_focus(None, &mut list).unwrap();
            let ids = demo.actions().unwrap();
            // Close and open again: the list of sections takes the focus, then Down `section`
            // times.
            demo.run_action(ids.preferences, &mut list);
            demo.run_action(ids.preferences, &mut list);
            for _ in 0..section {
                press_key(&mut demo, &mut list, NamedKey::ArrowDown);
            }
            let panel = demo.preferences().unwrap();
            let pane = demo.ui().rect(panel).unwrap();
            let mut rects = Vec::new();
            all_controls(&demo, panel, &mut rects);
            for rect in rects {
                assert!(
                    rect.x1 + REACH <= pane.x1,
                    "{locale:?}, section {section}: {rect:?} outside {pane:?}"
                );
            }
        }
    }
}

/// The rectangles of every control under `id` (buttons, fields, drop-downs, groups, toggles,
/// sliders, lists), depth first.
fn all_controls(demo: &Demo, id: WidgetId, out: &mut Vec<Rect>) {
    for child in demo.ui().children(id) {
        let ui = demo.ui();
        let control = ui.widget::<Button<DemoMessage>>(child).is_some()
            || ui.widget::<TextField<DemoMessage>>(child).is_some()
            || ui
                .widget::<orchestra_ui::DropDown<DemoMessage>>(child)
                .is_some()
            || ui
                .widget::<orchestra_ui::RadioGroup<DemoMessage>>(child)
                .is_some()
            || ui
                .widget::<orchestra_ui::Toggle<DemoMessage>>(child)
                .is_some()
            || ui
                .widget::<orchestra_ui::Slider<DemoMessage>>(child)
                .is_some()
            || ui
                .widget::<orchestra_ui::ListBox<DemoMessage>>(child)
                .is_some();
        if control {
            out.push(ui.rect(child).unwrap());
        }
        all_controls(demo, child, out);
    }
}
