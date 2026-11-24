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
//! - **Language** (F2.12a4b1): *System language* or one of the six languages, each named in its
//!   own language (choice 1), with the note that the choice applies at the next start (until
//!   F2.13 changes the language without a restart);
//! - **Keyboard** (F2.12a4b1): the modifier of **fine adjustment** (ADR-0046 V3), a group of
//!   three options named as the platform writes the keys (Ctrl, Shift, Alt; ⌘, ⇧, ⌥ on macOS).
//!   Nothing else yet: the shortcut editor joins this section with F2.12c (choice 2).
//!
//! # Changes apply at once (DG8)
//! Every change goes to the settings ([`apply`]), which the store writes half a second later
//! (DG6), and to the interface at once ([`Effect`]): a new pair or mode is a
//! [`WindowRequest::SetTheme`](orchestra_ui::WindowRequest) for the window's colours plus
//! `Ui::set_theme` for the pair's tokens; the blink is `Ui::set_caret_blink`; the fine modifier is
//! `Ui::set_fine_modifier`. The language alone waits for the next start. There is no OK, Cancel or
//! Apply.
//!
//! # Why the message type is the demonstration's
//! The widgets take plain `fn` pointers for their messages (no captured state, ADR-0045 WA7), so
//! the messages are written here in [`DemoMessage::Preferences`]: the demonstration is the
//! program's only view until the real ones exist, and it is the one that opens this dialog.

use orchestra_i18n::{I18nError, Locale, Localizer};
use orchestra_io::settings::{Settings, ThemeChoice};
use orchestra_ui::{
    ActionId, BuiltInTheme, ColorToken, DisplayList, DropDown, FineModifier, Flex, Grid, KeyNames,
    Label, LargeDialog, LayoutProps, Length, ListBox, Platform, RadioGroup, Space, ThemeMode,
    ThemePair, Toggle, Track, Ui, UiError, WidgetId, modifier_label,
};

use crate::demo::DemoMessage;
use crate::settings::convert::{
    SettingsProblem, fine_modifier_from_settings, fine_modifier_to_settings,
    language_from_settings, language_to_settings, mode_to_settings, theme_from_settings,
    tuning_from_settings, tuning_to_settings,
};

mod playback;

use orchestra_playback::{Tuning, TuningValue};
pub use playback::PlaybackTexts;

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
    /// The language drop-down's values: `prefs-language-system`, then the six languages' own
    /// names (`language-en-us`…) in [`Locale::ALL`]'s order.
    pub languages: Vec<String>,
    /// `prefs-language-restart`, without Fluent's isolation marks around the program's name.
    pub language_restart: String,
    /// `prefs-fine-modifier`.
    pub fine_modifier: String,
    /// The modifiers' names in the platform's style, in [`FINE_MODIFIERS`]' order.
    pub fine_modifiers: Vec<String>,
    /// The Playback section's texts (F2.12a4b2).
    pub playback: PlaybackTexts,
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
        let mut languages = vec![text("prefs-language-system")?];
        for locale in Locale::ALL {
            languages.push(text(&format!(
                "language-{}",
                locale.tag().to_ascii_lowercase()
            ))?);
        }
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
            // The program's name is ours and left to right: the marks would only reach the
            // accessibility tree (as for the actions' names).
            language_restart: text("prefs-language-restart")?.replace(['\u{2068}', '\u{2069}'], ""),
            fine_modifier: text("prefs-fine-modifier")?,
            fine_modifiers: FINE_MODIFIERS
                .iter()
                .map(|fine| modifier_label(*fine, platform, keys))
                .collect(),
            playback: PlaybackTexts::new(localizer)?,
        })
    }
}

/// The values of the theme drop-down for `settings` and the index of the one in use: the
/// built-in pairs' names, then the custom theme's own name when the settings chose one
/// (choice 4 of F2.12a4a).
fn theme_values(texts: &PreferencesTexts, settings: &Settings) -> (Vec<String>, usize) {
    let mut values = texts.pairs.clone();
    let selected = match &settings.theme.choice {
        ThemeChoice::BuiltIn(id) => BuiltInTheme::ALL
            .iter()
            .position(|pair| pair.id() == id)
            // An unknown identifier is drawn as Studio, the default (theme_from_settings).
            .unwrap_or_else(|| {
                BuiltInTheme::ALL
                    .iter()
                    .position(|pair| *pair == BuiltInTheme::default())
                    .unwrap_or(0)
            }),
        ThemeChoice::Custom(name) => {
            values.push(name.clone());
            values.len() - 1
        }
    };
    (values, selected)
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
}

/// Opens the Preferences on `settings` in `ui`, on the first section, with the focus on the list
/// of sections; `closed_by` is the action that opened them (its shortcut closes them, DG8).
///
/// # Errors
/// The interface's, if its own tree is inconsistent (never expected).
pub fn open_preferences(
    ui: &mut Ui<DemoMessage>,
    texts: &PreferencesTexts,
    settings: &Settings,
    closed_by: Option<ActionId>,
    list: &mut DisplayList,
) -> Result<PreferencesView, UiError> {
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
    ui.add(
        row,
        ListBox::new(texts.section_names.clone(), 0, |index| {
            DemoMessage::Preferences(PreferencesMessage::Section(index))
        })
        .label(texts.sections.clone()),
        LayoutProps::default(),
    )?;
    let pane = ui.add(row, Flex::column(Space::M, Space::None), fill)?;
    let view = PreferencesView { panel, pane };
    show_section(ui, view, Section::Appearance, texts, settings, list)?;
    ui.focus_first(list);
    Ok(view)
}

/// Replaces the controls in the Preferences' pane with those of `section`, showing `settings`'
/// values. The list of sections keeps the focus (the user is moving in it). Answers the labels of
/// the playback values, in [`TuningValue::ALL`]'s order, for the Playback section (their text
/// follows each change, [`playback_value_text`]); nothing for the others.
///
/// # Errors
/// The interface's, if its own tree is inconsistent (never expected).
pub fn show_section(
    ui: &mut Ui<DemoMessage>,
    view: PreferencesView,
    section: Section,
    texts: &PreferencesTexts,
    settings: &Settings,
    list: &mut DisplayList,
) -> Result<Vec<WidgetId>, UiError> {
    for child in ui.children(view.pane) {
        ui.remove(child, list)?;
    }
    if section == Section::Playback {
        let tuning = tuning_from_settings(&settings.playback, &mut Vec::new());
        return playback::build(ui, view.pane, &texts.playback, &tuning, list);
    }
    // A label and its control on each line, the labels in one column.
    let grid = ui.add(
        view.pane,
        Grid::new(
            vec![Track::Content, Track::Fill(1.0)],
            vec![Track::Content, Track::Content, Track::Content],
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
            let (values, selected) = theme_values(texts, settings);
            ui.add(grid, Label::new(texts.theme.clone()), label(0))?;
            ui.add(
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
        }
        Section::Language => {
            ui.add(grid, Label::new(texts.language.clone()), label(0))?;
            ui.add(
                grid,
                DropDown::new(texts.languages.clone(), language_index(settings), |index| {
                    DemoMessage::Preferences(PreferencesMessage::Language(index))
                })
                .label(texts.language.clone()),
                control(0),
            )?;
            // Until F2.13 the language changes at the next start: said beside the choice (DG8).
            ui.add(
                grid,
                Label::new(texts.language_restart.clone())
                    .color(ColorToken::TextMuted)
                    .wrapping(),
                control(1).width(Length::Fill(1.0)),
            )?;
        }
        // Built above, in its own scroll area.
        Section::Playback => {}
        Section::Keyboard => {
            let fine = fine_modifier_from_settings(settings.fine_modifier);
            let selected = FINE_MODIFIERS
                .iter()
                .position(|known| *known == fine)
                .unwrap_or(0);
            ui.add(grid, Label::new(texts.fine_modifier.clone()), label(0))?;
            ui.add(
                grid,
                RadioGroup::new(texts.fine_modifiers.clone(), selected, |index| {
                    DemoMessage::Preferences(PreferencesMessage::FineModifier(index))
                })
                .label(texts.fine_modifier.clone()),
                control(0),
            )?;
        }
    }
    ui.frame(list);
    Ok(Vec::new())
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
    /// Nothing more now (the dialog closed, a value chosen again, the language: next start).
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
            if let Some(pair) = BuiltInTheme::ALL.get(index) {
                settings.theme.choice = ThemeChoice::BuiltIn(pair.id().to_owned());
            }
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
            Effect::None
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

    /// A language is written as its tag (0: the system's) and waits for the next start; a fine
    /// modifier applies at once; a section is shown.
    #[test]
    fn language_and_fine_modifier_are_written() {
        let mut settings = Settings::default();
        // Italian: `Locale::ALL[2]`, the drop-down's index 3.
        assert_eq!(
            apply(&mut settings, PreferencesMessage::Language(3)),
            Effect::None
        );
        assert_eq!(
            settings.language,
            orchestra_io::settings::Language::Tag("it".to_owned())
        );
        assert_eq!(language_index(&settings), 3);
        assert_eq!(
            apply(&mut settings, PreferencesMessage::Language(0)),
            Effect::None
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

    /// The ten pairs by name, Studio chosen by default; a custom theme in use is added at the
    /// end and chosen. The languages: the system's first, then six names; the modifiers as Linux
    /// writes them.
    #[test]
    fn the_drop_downs_list_their_values() {
        let texts = texts();
        let (values, selected) = theme_values(&texts, &Settings::default());
        assert_eq!(values.len(), 10);
        assert_eq!(values[selected], "Studio");
        let mut settings = Settings::default();
        settings.theme.choice = ThemeChoice::Custom("Mine".to_owned());
        let (values, selected) = theme_values(&texts, &settings);
        assert_eq!(values.len(), 11);
        assert_eq!(values[selected], "Mine");
        assert_eq!(texts.languages.len(), 7);
        assert_eq!(texts.languages[3], "Italiano");
        assert_eq!(texts.fine_modifiers, vec!["Ctrl", "Shift", "Alt"]);
        assert!(!texts.language_restart.contains('\u{2068}'));
    }
}
