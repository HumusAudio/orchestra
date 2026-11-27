// SPDX-License-Identifier: GPL-3.0-or-later

//! The Preferences (ADR-0050 DG8–DG9, SCALETTA F2.12a4a–b1): their texts, the dialog's content,
//! and what each change does to the settings.
//!
//! # What the user sees
//! A large dialog (`Ui::open_large_dialog`): on the left the **list of sections**, on the right the
//! chosen section's controls, and one *Close* button. Choosing a section in the list replaces the
//! controls on the right ([`show_section`]). The sections (Playback comes with F2.12a4b2):
//! - **Appearance** (F2.12a4a): the **Theme** (a drop-down of the ten built-in pairs, ADR-0037
//!   T1, by their names in the catalogues, plus the custom theme in use, if the settings chose
//!   one, choice 4 of F2.12a4a), the **Mode** (*System / Light / Dark*, a group of options) and
//!   **Blinking cursor** (a check box, ADR-0047 TF8);
//! - **Language** (F2.12a4b1, F2.13): *System language (Italiano)*, naming the language the
//!   system's languages give (ADR-0053 LR5), or one of the six languages, each named in its own
//!   language (choice 1 of F2.12a4b1). The choice applies at once: the whole interface is shown
//!   in the new language, the Preferences open again on this section (ADR-0053 LR1);
//! - **Keyboard** (F2.12a4b1): the modifier of **fine adjustment** (ADR-0046 V3), a group of
//!   three options named as the platform writes the keys (Ctrl, Shift, Alt; ⌘, ⇧, ⌥ on macOS),
//!   then the **shortcut editor** (F2.12c1, `keyboard`): the commands with their shortcuts,
//!   a search, adding, removing and restoring shortcuts, and the questions about conflicts.
//!
//! # Changes apply at once (DG8)
//! Every change goes to the settings ([`apply`]), which the store writes half a second later
//! (DG6), and to the interface at once ([`Effect`]): a new pair or mode is a
//! [`WindowRequest::SetTheme`](orchestra_ui::WindowRequest) for the window's colours plus
//! `Ui::set_theme` for the pair's tokens; the blink is `Ui::set_caret_blink`; the fine modifier is
//! `Ui::set_fine_modifier`; a language is [`Effect::Language`], which the demonstration carries out
//! by showing its whole interface again in that language (F2.13). There is no OK, Cancel or
//! Apply.
//!
//! # Why the message type is the demonstration's
//! The widgets take plain `fn` pointers for their messages (no captured state, ADR-0045 WA7), so
//! the messages are written here in [`DemoMessage::Preferences`]: the demonstration is the
//! program's only view until the real ones exist, and it is the one that opens this dialog.

use orchestra_i18n::{I18nError, Locale, Localizer};
use orchestra_io::settings::Settings;
use orchestra_ui::{
    ActionId, Appearance, BuiltInTheme, DisplayList, DropDown, FineModifier, Flex, Grid, KeyNames,
    Label, LargeDialog, LayoutProps, Length, ListBox, Platform, RadioGroup, Space, ThemeMode,
    ThemePair, Toggle, Track, Ui, UiError, WidgetId, modifier_label,
};

use crate::demo::DemoMessage;
use crate::settings::convert::{
    SettingsProblem, fine_modifier_from_settings, fine_modifier_to_settings,
    language_from_settings, language_to_settings, mode_to_settings, theme_from_settings,
    tuning_from_settings, tuning_to_settings,
};

mod keyboard;
mod playback;
mod theme;

pub use keyboard::{
    KeyboardMessage, KeyboardOutcome, KeyboardTexts, ShortcutEditor, matches as shortcut_matches,
    shortcuts_text, shown_commands,
};
use orchestra_playback::{Tuning, TuningValue};
pub use playback::PlaybackTexts;
pub use theme::{
    ChoiceRow, SharedRows, SliderRow, ThemeEditor, ThemeMessage, ThemeOutcome, ThemeTexts,
    theme_values,
};

/// The modes of the group, in its order (ADR-0014 §1).
pub const MODES: [ThemeMode; 3] = [ThemeMode::System, ThemeMode::Light, ThemeMode::Dark];

/// The modifiers of fine adjustment of the Keyboard section, in its order (DG9).
pub const FINE_MODIFIERS: [FineModifier; 3] = [
    FineModifier::Primary,
    FineModifier::Shift,
    FineModifier::Alt,
];

/// The Preferences' sections, in the list's order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    /// Theme, mode, blinking cursor.
    Appearance,
    /// The interface's language.
    Language,
    /// The playback's tuning (F2.12a4b2, `playback`).
    Playback,
    /// The modifier of fine adjustment.
    Keyboard,
}

/// The sections in the list's order (DG9's: Appearance, Language, Playback, Keyboard).
pub const SECTIONS: [Section; 4] = [
    Section::Appearance,
    Section::Language,
    Section::Playback,
    Section::Keyboard,
];

/// What the Preferences' controls send (inside [`DemoMessage::Preferences`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreferencesMessage {
    /// A section was chosen in the list, by its index in [`SECTIONS`].
    Section(usize),
    /// A theme was chosen, by its index in the drop-down: the built-in pairs in
    /// [`BuiltInTheme::ALL`]'s order, then the custom theme in use, if any.
    Theme(usize),
    /// A mode was chosen, by its index in [`MODES`].
    Mode(usize),
    /// The cursor's blink was turned on or off.
    CaretBlink(bool),
    /// A language was chosen: 0 for the system's, then [`Locale::ALL`]'s order.
    Language(usize),
    /// A modifier of fine adjustment was chosen, by its index in [`FINE_MODIFIERS`].
    FineModifier(usize),
    /// A playback value changed: its index in [`TuningValue::ALL`] and its new value
    /// (F2.12a4b2).
    Playback(usize, i64),
    /// *Restore defaults* in the Playback section: the tuning by ear again.
    RestorePlayback,
    /// The dialog closed (*Close*, Escape, or the shortcut that opened it).
    Closed,
}

/// The texts of the Preferences in one language (CLAUDE.md §7).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreferencesTexts {
    /// `prefs-title`.
    pub title: String,
    /// `dialog-close`.
    pub close: String,
    /// `prefs-sections`: the list's accessible name.
    pub sections: String,
    /// The sections' names, in [`SECTIONS`]' order (`prefs-section-appearance`…).
    pub section_names: Vec<String>,
    /// `prefs-theme`.
    pub theme: String,
    /// `prefs-mode`.
    pub mode: String,
    /// `prefs-mode-system`, `prefs-mode-light`, `prefs-mode-dark`, in [`MODES`]' order.
    pub modes: [String; 3],
    /// `prefs-caret-blink`.
    pub caret_blink: String,
    /// The built-in pairs' names (`theme-studio`…), in [`BuiltInTheme::ALL`]'s order.
    pub pairs: Vec<String>,
    /// `prefs-language`.
    pub language: String,
    /// The language drop-down's values: *System language (…)* for the system's language now
    /// ([`PreferencesTexts::set_system_language`]), then the six languages' own names
    /// (`language-en-us`…) in [`Locale::ALL`]'s order.
    pub languages: Vec<String>,
    /// `prefs-language-system` for each of the six languages the system's could give, in
    /// [`Locale::ALL`]'s order: *System language (English (US))*, *System language (Italiano)*…
    /// Made once with the texts, so naming another system language needs no localizer.
    pub system_languages: Vec<String>,
    /// `prefs-fine-modifier`.
    pub fine_modifier: String,
    /// The modifiers' names in the platform's style, in [`FINE_MODIFIERS`]' order.
    pub fine_modifiers: Vec<String>,
    /// The Playback section's texts (F2.12a4b2).
    pub playback: PlaybackTexts,
    /// The shortcut editor's texts (F2.12c1).
    pub keyboard: KeyboardTexts,
    /// The theme editor's texts (F2.12b1).
    pub theme_editor: ThemeTexts,
}

impl PreferencesTexts {
    /// The texts in `localizer`'s language, the modifiers named as `platform` writes them with
    /// the keys' names `keys` (the registry's, F2.11d2a).
    ///
    /// # Errors
    /// The localizer's, if a message is missing (the catalogues' parity tests prevent it).
    pub fn new(
        localizer: &Localizer,
        platform: Platform,
        keys: &KeyNames,
    ) -> Result<Self, I18nError> {
        let text = |key: &str| localizer.format(key, None);
        let mut pairs = Vec::with_capacity(BuiltInTheme::ALL.len());
        for pair in BuiltInTheme::ALL {
            pairs.push(text(&format!("theme-{}", pair.id()))?);
        }
        // The six languages' own names, then *System language (name)* for each of them: the
        // name goes into the sentence as an argument (no concatenation, CLAUDE.md §7), and the
        // isolation marks Fluent puts around it are dropped, as for the program's name below
        // (every name is written left to right; the marks would only reach assistive
        // technologies).
        let mut names = Vec::with_capacity(Locale::ALL.len());
        for locale in Locale::ALL {
            names.push(text(&format!(
                "language-{}",
                locale.tag().to_ascii_lowercase()
            ))?);
        }
        let mut system_languages = Vec::with_capacity(names.len());
        for name in &names {
            let mut args = orchestra_i18n::FluentArgs::new();
            args.set("language", name.as_str());
            system_languages.push(
                localizer
                    .format("prefs-language-system", Some(&args))?
                    .replace(['\u{2068}', '\u{2069}'], ""),
            );
        }
        // *System language* names en-US until the demonstration is told the system's.
        let mut languages = Vec::with_capacity(names.len() + 1);
        languages.push(system_languages.first().cloned().unwrap_or_default());
        languages.extend(names);
        Ok(Self {
            title: text("prefs-title")?,
            close: text("dialog-close")?,
            sections: text("prefs-sections")?,
            section_names: vec![
                text("prefs-section-appearance")?,
                text("prefs-section-language")?,
                text("prefs-section-playback")?,
                text("prefs-section-keyboard")?,
            ],
            theme: text("prefs-theme")?,
            mode: text("prefs-mode")?,
            modes: [
                text("prefs-mode-system")?,
                text("prefs-mode-light")?,
                text("prefs-mode-dark")?,
            ],
            caret_blink: text("prefs-caret-blink")?,
            pairs,
            language: text("prefs-language")?,
            languages,
            system_languages,
            fine_modifier: text("prefs-fine-modifier")?,
            fine_modifiers: FINE_MODIFIERS
                .iter()
                .map(|fine| modifier_label(*fine, platform, keys))
                .collect(),
            playback: PlaybackTexts::new(localizer)?,
            keyboard: KeyboardTexts::new(localizer)?,
            theme_editor: ThemeTexts::new(localizer)?,
        })
    }
}

impl PreferencesTexts {
    /// Makes the language drop-down's first value name `system`, the language the system's
    /// languages give now (ADR-0053 LR5): *System language (Deutsch)*. The demonstration calls it
    /// whenever it reads the system's languages (start-up, *System language* chosen again).
    pub fn set_system_language(&mut self, system: Locale) {
        let index = Locale::ALL
            .iter()
            .position(|locale| *locale == system)
            .unwrap_or(0);
        if let (Some(first), Some(text)) =
            (self.languages.first_mut(), self.system_languages.get(index))
        {
            text.clone_into(first);
        }
    }
}

/// The language drop-down's index for `settings`: 0 for the system's (and for a tag this program
/// does not know), then [`Locale::ALL`]'s order plus one.
fn language_index(settings: &Settings) -> usize {
    let mut problems = Vec::new();
    language_from_settings(&settings.language, &mut problems)
        .and_then(|locale| Locale::ALL.iter().position(|known| *known == locale))
        .map_or(0, |index| index + 1)
}

/// The open Preferences: their panel (for closing) and the pane on the right that holds the
/// chosen section's controls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PreferencesView {
    /// The dialog's panel.
    pub panel: WidgetId,
    /// The pane of the chosen section.
    pub pane: WidgetId,
    /// The section shown (F2.13: the Preferences open again on it after a change of language).
    pub section: Section,
}

/// Opens the Preferences on `settings` in `ui`, on `section` (the first one when the user opens
/// them; the Language section again after a change of language, ADR-0053 LR1), with the focus on
/// the list of sections, or on the language's drop-down when `section` is Language (the user was
/// choosing there: the focus comes back where it was); `closed_by` is the action that opened them
/// (its shortcut closes them, DG8).
///
/// # Errors
/// The interface's, if its own tree is inconsistent (never expected).
pub fn open_preferences(
    ui: &mut Ui<DemoMessage>,
    texts: &PreferencesTexts,
    settings: &Settings,
    (section, closed_by): (Section, Option<ActionId>),
    appearance: Appearance,
    list: &mut DisplayList,
) -> Result<(PreferencesView, SectionWidgets), UiError> {
    let (panel, content) = ui.open_large_dialog(
        LargeDialog {
            title: texts.title.clone(),
            close_text: texts.close.clone(),
            close_message: DemoMessage::Preferences(PreferencesMessage::Closed),
            closed_by,
        },
        list,
    )?;
    let fill = LayoutProps::default()
        .width(Length::Fill(1.0))
        .height(Length::Fill(1.0));
    // The sections on the left, the chosen section on the right.
    let row = ui.add(content, Flex::row(Space::L, Space::None), fill)?;
    let index = SECTIONS
        .iter()
        .position(|known| *known == section)
        .unwrap_or(0);
    ui.add(
        row,
        ListBox::new(texts.section_names.clone(), index, |index| {
            DemoMessage::Preferences(PreferencesMessage::Section(index))
        })
        .label(texts.sections.clone()),
        LayoutProps::default(),
    )?;
    let pane = ui.add(row, Flex::column(Space::M, Space::None), fill)?;
    let view = PreferencesView {
        panel,
        pane,
        section,
    };
    let widgets = show_section(ui, view, section, texts, settings, appearance, list)?;
    match widgets.language {
        Some(language) => ui.set_focus(Some(language), list)?,
        None => ui.focus_first(list),
    }
    Ok((view, widgets))
}

/// What a section shown keeps for its changes: the labels of the playback values, in
/// [`TuningValue::ALL`]'s order (Playback; their text follows each change,
/// [`playback_value_text`]), and the shortcut editor (Keyboard, F2.12c1). Empty for the others.
#[derive(Debug, Default)]
pub struct SectionWidgets {
    /// The playback values' labels.
    pub playback_labels: Vec<WidgetId>,
    /// The shortcut editor.
    pub keyboard: Option<ShortcutEditor>,
    /// The theme editor (Appearance, F2.12b1).
    pub theme: Option<ThemeEditor>,
    /// The language's drop-down (Language, F2.13): the focus goes back to it when the
    /// Preferences open again after a change of language.
    pub language: Option<WidgetId>,
}

/// Replaces the controls in the Preferences' pane with those of `section`, showing `settings`'
/// values. The list of sections keeps the focus (the user is moving in it). Answers what the
/// section keeps for its changes ([`SectionWidgets`]).
///
/// # Errors
/// The interface's, if its own tree is inconsistent (never expected).
pub fn show_section(
    ui: &mut Ui<DemoMessage>,
    view: PreferencesView,
    section: Section,
    texts: &PreferencesTexts,
    settings: &Settings,
    appearance: Appearance,
    list: &mut DisplayList,
) -> Result<SectionWidgets, UiError> {
    for child in ui.children(view.pane) {
        ui.remove(child, list)?;
    }
    if section == Section::Playback {
        let tuning = tuning_from_settings(&settings.playback, &mut Vec::new());
        return Ok(SectionWidgets {
            playback_labels: playback::build(ui, view.pane, &texts.playback, &tuning, list)?,
            ..SectionWidgets::default()
        });
    }
    // A label and its control on each line, the labels in one column; as many lines as the
    // section has (an empty line would still take the grid's spacing).
    let lines = match section {
        Section::Appearance => 3,
        Section::Language | Section::Playback | Section::Keyboard => 1,
    };
    // Appearance scrolls (F2.12b1: the theme editor's 24 colours do not fit a small window); the
    // others are short.
    // The column the section's controls go in, and for Appearance the scroll area around it (the
    // theme editor keeps its open colour picker still in it, F2.12b3b).
    let (parent, scroll) = if section == Section::Appearance {
        let fill = LayoutProps::default()
            .width(Length::Fill(1.0))
            .height(Length::Fill(1.0));
        let scroll = ui.add(view.pane, orchestra_ui::ScrollArea::vertical(), fill)?;
        // The small padding keeps the focus rings of the controls at the column's edges (the
        // theme editor's *Duplicate*, the name field, *Restore*) inside the viewport, which clips
        // everything: a ring reaches 1 px + `focus-width` (3 px) beyond its control, and the
        // small gap is 8 px (4 px compact). Without it the ring's left side was cut (owner's
        // report of 2026-11-26; the rule is in `ScrollArea`'s documentation).
        let column = ui.add(
            scroll,
            Flex::column(Space::M, Space::S),
            LayoutProps::default().width(Length::Fill(1.0)),
        )?;
        (column, Some(scroll))
    } else {
        (view.pane, None)
    };
    let grid = ui.add(
        parent,
        Grid::new(
            vec![Track::Content, Track::Fill(1.0)],
            vec![Track::Content; lines],
            Space::M,
            Space::None,
        ),
        LayoutProps::default().width(Length::Fill(1.0)),
    )?;
    let label = |row: u16| {
        LayoutProps::default()
            .cell(row, 0)
            .align(orchestra_ui::Align::Start, orchestra_ui::Align::Center)
    };
    let control = |row: u16| LayoutProps::default().cell(row, 1);
    match section {
        Section::Appearance => {
            return appearance_section(
                ui,
                (parent, scroll),
                grid,
                texts,
                settings,
                appearance,
                list,
            );
        }
        Section::Language => {
            ui.add(grid, Label::new(texts.language.clone()), label(0))?;
            let language = ui.add(
                grid,
                DropDown::new(texts.languages.clone(), language_index(settings), |index| {
                    DemoMessage::Preferences(PreferencesMessage::Language(index))
                })
                .label(texts.language.clone()),
                control(0),
            )?;
            // The note "applies at the next start" of F2.12a4b1 is gone: the language changes
            // at once (F2.13, ADR-0053 LR1).
            ui.frame(list);
            return Ok(SectionWidgets {
                language: Some(language),
                ..SectionWidgets::default()
            });
        }
        // Built above, in its own scroll area.
        Section::Playback => {}
        Section::Keyboard => {
            return keyboard_section(ui, view, grid, texts, settings, list);
        }
    }
    ui.frame(list);
    Ok(SectionWidgets::default())
}

/// The Appearance section's controls (theme, mode, blinking cursor, then the theme editor of
/// F2.12b1) in `grid`, whose three lines it fills, and in `parent`, the scrolling column that holds
/// the grid and receives the editor underneath it.
///
/// # Why a function of its own?
/// The section grew with the theme editor past the length the project's lints allow for one
/// function (`too_many_lines`); the Keyboard section was already split out the same way
/// ([`keyboard_section`]). The behaviour is unchanged: `show_section` only dispatches.
///
/// # Errors
/// The interface's, if its own tree is inconsistent (never expected).
fn appearance_section(
    ui: &mut Ui<DemoMessage>,
    (parent, scroll): (WidgetId, Option<WidgetId>),
    grid: WidgetId,
    texts: &PreferencesTexts,
    settings: &Settings,
    appearance: Appearance,
    list: &mut DisplayList,
) -> Result<SectionWidgets, UiError> {
    // The label of line `row` (first column) and the control of line `row` (second column), as
    // in `show_section`.
    let label = |row: u16| {
        LayoutProps::default()
            .cell(row, 0)
            .align(orchestra_ui::Align::Start, orchestra_ui::Align::Center)
    };
    let control = |row: u16| LayoutProps::default().cell(row, 1);
    let (values, selected) = theme_values(&texts.pairs, settings);
    ui.add(grid, Label::new(texts.theme.clone()), label(0))?;
    let themes = ui.add(
        grid,
        DropDown::new(values, selected, |index| {
            DemoMessage::Preferences(PreferencesMessage::Theme(index))
        })
        .label(texts.theme.clone()),
        control(0),
    )?;
    let mode = match settings.theme.mode {
        orchestra_io::settings::Mode::System => 0,
        orchestra_io::settings::Mode::Light => 1,
        orchestra_io::settings::Mode::Dark => 2,
    };
    ui.add(grid, Label::new(texts.mode.clone()), label(1))?;
    ui.add(
        grid,
        RadioGroup::new(texts.modes.to_vec(), mode, |index| {
            DemoMessage::Preferences(PreferencesMessage::Mode(index))
        })
        .label(texts.mode.clone()),
        control(1),
    )?;
    ui.add(
        grid,
        Toggle::checkbox(texts.caret_blink.clone(), settings.caret_blink, |on| {
            DemoMessage::Preferences(PreferencesMessage::CaretBlink(on))
        }),
        control(2),
    )?;
    // The theme editor under the grid (F2.12b1).
    let mut editor = ThemeEditor::build(
        ui,
        parent,
        themes,
        &texts.theme_editor,
        settings,
        appearance,
        list,
    )?;
    editor.set_scroll_area(scroll);
    ui.frame(list);
    Ok(SectionWidgets {
        theme: Some(editor),
        ..SectionWidgets::default()
    })
}

/// The Keyboard section's controls in `grid` and under it (F2.12a4b1, F2.12c1): the modifier of
/// fine adjustment, then the shortcut editor, which `view`'s pane gives the room left.
///
/// # Errors
/// The interface's, if its own tree is inconsistent (never expected).
fn keyboard_section(
    ui: &mut Ui<DemoMessage>,
    view: PreferencesView,
    grid: WidgetId,
    texts: &PreferencesTexts,
    settings: &Settings,
    list: &mut DisplayList,
) -> Result<SectionWidgets, UiError> {
    let label = LayoutProps::default()
        .cell(0, 0)
        .align(orchestra_ui::Align::Start, orchestra_ui::Align::Center);
    let control = LayoutProps::default().cell(0, 1);
    let fine = fine_modifier_from_settings(settings.fine_modifier);
    let selected = FINE_MODIFIERS
        .iter()
        .position(|known| *known == fine)
        .unwrap_or(0);
    ui.add(grid, Label::new(texts.fine_modifier.clone()), label)?;
    ui.add(
        grid,
        RadioGroup::new(texts.fine_modifiers.clone(), selected, |index| {
            DemoMessage::Preferences(PreferencesMessage::FineModifier(index))
        })
        .label(texts.fine_modifier.clone()),
        control,
    )?;
    // The shortcut editor under the grid (F2.12c1), taking the room left.
    let editor = ShortcutEditor::build(ui, view.pane, &texts.keyboard, list)?;
    ui.frame(list);
    Ok(SectionWidgets {
        keyboard: Some(editor),
        ..SectionWidgets::default()
    })
}

/// The text of a playback value's label: `n` for the value number `index` of
/// [`TuningValue::ALL`] (an index out of the list gives an empty text).
#[must_use]
pub fn playback_value_text(texts: &PreferencesTexts, index: usize, n: i64) -> String {
    TuningValue::ALL
        .get(index)
        .map_or_else(String::new, |value| {
            playback::value_text(&texts.playback, *value, n)
        })
}

/// What a change asks of the interface, once written into the settings ([`apply`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(
    clippy::large_enum_variant,
    reason = "one value per user change in the Preferences, used at once and dropped: the size \
              of a pair costs nothing there, and boxing it would only add an allocation"
)]
pub enum Effect {
    /// Nothing more now (the dialog closed, a value outside its list).
    None,
    /// Show this section's controls.
    Section(Section),
    /// Draw with this pair in this mode (the window's colours, the interface's tokens).
    Theme(ThemePair, ThemeMode),
    /// Turn the cursor's blink on or off.
    CaretBlink(bool),
    /// Use this modifier of fine adjustment.
    FineModifier(FineModifier),
    /// The playback value number `.0` of [`TuningValue::ALL`] is now `.1` (its label follows; the
    /// performance, when there is one, takes the new tuning).
    PlaybackValue(usize, i64),
    /// Show the interface in this language from now on (F2.13, ADR-0053 LR1): `Some` for one of
    /// the six, `None` for the system's, which the demonstration reads again (LR4).
    Language(Option<Locale>),
}

/// Writes `message` into `settings` and answers what the interface must do now (DG8). A theme's
/// index beyond the built-in pairs keeps the custom theme in use (the only other value the list
/// holds, choice 4 of F2.12a4a). Problems found while resolving the theme (an unknown
/// identifier: never from this list) are dropped: the drawing falls back to Studio, as at
/// start-up. An index outside its list changes nothing.
pub fn apply(settings: &mut Settings, message: PreferencesMessage) -> Effect {
    match message {
        PreferencesMessage::Section(index) => SECTIONS
            .get(index)
            .map_or(Effect::None, |section| Effect::Section(*section)),
        PreferencesMessage::Theme(index) => {
            // A built-in pair, or a custom theme by its place after them (F2.12b1).
            theme::choose(settings, index);
            theme_effect(settings)
        }
        PreferencesMessage::Mode(index) => {
            let Some(mode) = MODES.get(index) else {
                return Effect::None;
            };
            settings.theme.mode = mode_to_settings(*mode);
            theme_effect(settings)
        }
        PreferencesMessage::CaretBlink(on) => {
            settings.caret_blink = on;
            Effect::CaretBlink(on)
        }
        PreferencesMessage::Language(index) => {
            // 0 is the system's language; the others are `Locale::ALL` shifted by one.
            let locale = match index.checked_sub(1) {
                None => None,
                Some(shifted) => match Locale::ALL.get(shifted) {
                    Some(locale) => Some(*locale),
                    None => return Effect::None,
                },
            };
            settings.language = language_to_settings(locale);
            Effect::Language(locale)
        }
        PreferencesMessage::FineModifier(index) => {
            let Some(fine) = FINE_MODIFIERS.get(index) else {
                return Effect::None;
            };
            settings.fine_modifier = fine_modifier_to_settings(*fine);
            Effect::FineModifier(*fine)
        }
        PreferencesMessage::Playback(index, n) => {
            let Some(value) = TuningValue::ALL.get(index) else {
                return Effect::None;
            };
            let mut tuning = tuning_from_settings(&settings.playback, &mut Vec::new());
            tuning.set(*value, n);
            tuning_to_settings(&tuning, &mut settings.playback);
            Effect::PlaybackValue(index, tuning.get(*value))
        }
        PreferencesMessage::RestorePlayback => {
            // The tuning by ear: the known names leave the file, the unknown ones stay (S2); the
            // section is built again with the defaults.
            tuning_to_settings(&Tuning::default(), &mut settings.playback);
            Effect::Section(Section::Playback)
        }
        PreferencesMessage::Closed => Effect::None,
    }
}

/// The theme the settings choose now, as an [`Effect`].
fn theme_effect(settings: &Settings) -> Effect {
    let mut problems: Vec<SettingsProblem> = Vec::new();
    let (pair, mode) = theme_from_settings(&settings.theme, &mut problems);
    Effect::Theme(pair, mode)
}

#[cfg(test)]
mod tests {
    //! The changes written into the settings, and the drop-downs' values.
    use super::*;
    use orchestra_io::settings::ThemeChoice;

    /// Texts with recognisable names (the catalogues are tested elsewhere), on Linux.
    fn texts() -> PreferencesTexts {
        let localizer = Localizer::new(Locale::EnUs).unwrap();
        let actions = crate::actions::action_texts(&localizer).unwrap();
        PreferencesTexts::new(&localizer, Platform::Linux, actions.keys()).unwrap()
    }

    /// A theme and a mode go into the settings and come back as the pair and mode to draw.
    #[test]
    fn theme_and_mode_are_written_and_applied() {
        let mut settings = Settings::default();
        let forest = BuiltInTheme::ALL
            .iter()
            .position(|pair| *pair == BuiltInTheme::Forest)
            .unwrap();
        assert_eq!(
            apply(&mut settings, PreferencesMessage::Theme(forest)),
            Effect::Theme(*BuiltInTheme::Forest.pair(), ThemeMode::System)
        );
        assert_eq!(
            settings.theme.choice,
            ThemeChoice::BuiltIn("forest".to_owned())
        );
        assert_eq!(
            apply(&mut settings, PreferencesMessage::Mode(2)),
            Effect::Theme(*BuiltInTheme::Forest.pair(), ThemeMode::Dark)
        );
        assert_eq!(settings.theme.mode, orchestra_io::settings::Mode::Dark);
        assert_eq!(
            apply(&mut settings, PreferencesMessage::CaretBlink(false)),
            Effect::CaretBlink(false)
        );
        assert!(!settings.caret_blink);
        assert_eq!(
            apply(&mut settings, PreferencesMessage::Mode(7)),
            Effect::None
        );
    }

    /// A language is written as its tag (0: the system's) and applies at once (F2.13); a fine
    /// modifier applies at once; a section is shown.
    #[test]
    fn language_and_fine_modifier_are_written() {
        let mut settings = Settings::default();
        // Italian: `Locale::ALL[2]`, the drop-down's index 3.
        assert_eq!(
            apply(&mut settings, PreferencesMessage::Language(3)),
            Effect::Language(Some(Locale::It))
        );
        assert_eq!(
            settings.language,
            orchestra_io::settings::Language::Tag("it".to_owned())
        );
        assert_eq!(language_index(&settings), 3);
        assert_eq!(
            apply(&mut settings, PreferencesMessage::Language(0)),
            Effect::Language(None)
        );
        assert_eq!(settings.language, orchestra_io::settings::Language::System);
        assert_eq!(
            apply(&mut settings, PreferencesMessage::Language(9)),
            Effect::None
        );
        assert_eq!(
            apply(&mut settings, PreferencesMessage::FineModifier(1)),
            Effect::FineModifier(FineModifier::Shift)
        );
        assert_eq!(
            settings.fine_modifier,
            orchestra_io::settings::FineModifier::Shift
        );
        assert_eq!(
            apply(&mut settings, PreferencesMessage::Section(3)),
            Effect::Section(Section::Keyboard)
        );
    }

    /// A playback value goes into the file's map by name, clamped; back at its default it leaves
    /// the map; *Restore defaults* removes the known names and keeps the unknown ones (S2).
    #[test]
    fn playback_values_are_written_by_name() {
        let mut settings = Settings::default();
        settings.playback.insert("from-the-future".to_owned(), 3);
        let accent = TuningValue::ALL
            .iter()
            .position(|value| *value == TuningValue::AccentStep)
            .unwrap();
        assert_eq!(
            apply(&mut settings, PreferencesMessage::Playback(accent, 99)),
            Effect::PlaybackValue(accent, 40)
        );
        assert_eq!(settings.playback.get("accent.step"), Some(&40));
        assert_eq!(
            apply(&mut settings, PreferencesMessage::Playback(accent, 22)),
            Effect::PlaybackValue(accent, 22)
        );
        assert_eq!(settings.playback.get("accent.step"), None);
        apply(&mut settings, PreferencesMessage::Playback(0, 10));
        assert_eq!(
            apply(&mut settings, PreferencesMessage::RestorePlayback),
            Effect::Section(Section::Playback)
        );
        assert_eq!(settings.playback.len(), 1);
        assert_eq!(settings.playback.get("from-the-future"), Some(&3));
        assert_eq!(
            apply(&mut settings, PreferencesMessage::Playback(99, 1)),
            Effect::None
        );
    }

    /// The ten pairs by name, Studio chosen by default; every custom theme follows them, in the
    /// settings' order, and the one in use is chosen (F2.12b1); a custom choice whose theme is
    /// missing shows Studio, as the window draws it. The languages: the system's first, then six
    /// names; the modifiers as Linux writes them.
    #[test]
    fn the_drop_downs_list_their_values() {
        let texts = texts();
        let (values, selected) = theme_values(&texts.pairs, &Settings::default());
        assert_eq!(values.len(), 10);
        assert_eq!(values[selected], "Studio");
        let mut settings = Settings::default();
        settings.theme.choice = ThemeChoice::Custom("Mine".to_owned());
        let (values, selected) = theme_values(&texts.pairs, &settings);
        assert_eq!(values.len(), 10, "no theme called Mine exists");
        assert_eq!(values[selected], "Studio");
        for name in ["Other", "Mine"] {
            settings
                .theme
                .custom
                .push(orchestra_io::settings::CustomTheme {
                    name: name.to_owned(),
                    base: "studio".to_owned(),
                    light: Vec::new(),
                    dark: Vec::new(),
                });
        }
        let (values, selected) = theme_values(&texts.pairs, &settings);
        assert_eq!(values.len(), 12);
        assert_eq!(&values[10..], ["Other", "Mine"]);
        assert_eq!(values[selected], "Mine");
        assert_eq!(texts.languages.len(), 7);
        assert_eq!(texts.languages[3], "Italiano");
        assert_eq!(texts.fine_modifiers, vec!["Ctrl", "Shift", "Alt"]);
        // *System language* names en-US until told the system's, then follows it (F2.13), without
        // Fluent's isolation marks.
        assert_eq!(texts.languages[0], "System language (English (US))"); // i18n-ignore: test data
        let mut texts = texts;
        texts.set_system_language(Locale::De);
        assert_eq!(texts.languages[0], "System language (Deutsch)"); // i18n-ignore: test data
        assert!(
            texts
                .system_languages
                .iter()
                .all(|text| !text.contains(['\u{2068}', '\u{2069}']))
        );
    }
}
