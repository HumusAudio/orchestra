// SPDX-License-Identifier: GPL-3.0-or-later

//! The Preferences (ADR-0050 DG8–DG9, SCALETTA F2.12a4a): their texts, the dialog's content, and
//! what each change does to the settings.
//!
//! # What the user sees (F2.12a4a)
//! A large dialog (`Ui::open_large_dialog`): on the left the **list of sections**, on the right the
//! chosen section's controls, and one *Close* button. F2.12a4a has one section, **Appearance**
//! (owner's choice 7: the sections Language, Playback and Keyboard come with F2.12a4b, never
//! empty in the meantime):
//! - **Theme**: a drop-down of the ten built-in pairs (ADR-0037 T1), by their names in the
//!   catalogues, plus the custom theme in use, if the settings chose one (choice 4: it does not
//!   disappear from the list; choosing among custom themes comes with the theme editor, F2.12b);
//! - **Mode**: *System / Light / Dark*, a group of options;
//! - **Blinking cursor**: a check box (ADR-0047 TF8).
//!
//! # Changes apply at once (DG8)
//! Every change goes to the settings ([`apply`]), which the store writes half a second later
//! (DG6), and to the interface at once: a new pair or mode is a [`WindowRequest::SetTheme`] for the
//! window's colours (and the system's decorations) plus `Ui::set_theme` for the pair's density,
//! text and shape tokens; the blink is `Ui::set_caret_blink`. There is no OK, Cancel or Apply.
//!
//! # Why the message type is the demonstration's
//! The widgets take plain `fn` pointers for their messages (no captured state, ADR-0045 WA7), so
//! the messages are written here in [`DemoMessage::Preferences`]: the demonstration is the
//! program's only view until the real ones exist, and it is the one that opens this dialog.

use orchestra_i18n::{I18nError, Localizer};
use orchestra_io::settings::{Settings, ThemeChoice};
use orchestra_ui::{
    ActionId, BuiltInTheme, DisplayList, DropDown, Flex, Grid, LargeDialog, LayoutProps, Length,
    ListBox, RadioGroup, Space, ThemeMode, ThemePair, Toggle, Track, Ui, UiError, WidgetId,
};

use crate::demo::DemoMessage;
use crate::settings::convert::{SettingsProblem, mode_to_settings, theme_from_settings};

/// The modes of the group, in its order (ADR-0014 §1).
pub const MODES: [ThemeMode; 3] = [ThemeMode::System, ThemeMode::Light, ThemeMode::Dark];

/// What the Preferences' controls send (inside [`DemoMessage::Preferences`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreferencesMessage {
    /// A section was chosen in the list, by its index.
    Section(usize),
    /// A theme was chosen, by its index in the drop-down: the built-in pairs in
    /// [`BuiltInTheme::ALL`]'s order, then the custom theme in use, if any.
    Theme(usize),
    /// A mode was chosen, by its index in [`MODES`].
    Mode(usize),
    /// The cursor's blink was turned on or off.
    CaretBlink(bool),
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
    /// `prefs-section-appearance`.
    pub appearance: String,
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
}

impl PreferencesTexts {
    /// The texts in `localizer`'s language.
    ///
    /// # Errors
    /// The localizer's, if a message is missing (the catalogues' parity tests prevent it).
    pub fn new(localizer: &Localizer) -> Result<Self, I18nError> {
        let text = |key: &str| localizer.format(key, None);
        let mut pairs = Vec::with_capacity(BuiltInTheme::ALL.len());
        for pair in BuiltInTheme::ALL {
            pairs.push(text(&format!("theme-{}", pair.id()))?);
        }
        Ok(Self {
            title: text("prefs-title")?,
            close: text("dialog-close")?,
            sections: text("prefs-sections")?,
            appearance: text("prefs-section-appearance")?,
            theme: text("prefs-theme")?,
            mode: text("prefs-mode")?,
            modes: [
                text("prefs-mode-system")?,
                text("prefs-mode-light")?,
                text("prefs-mode-dark")?,
            ],
            caret_blink: text("prefs-caret-blink")?,
            pairs,
        })
    }
}

/// The values of the theme drop-down for `settings` and the index of the one in use: the
/// built-in pairs' names, then the custom theme's own name when the settings chose one
/// (choice 4).
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

/// Opens the Preferences on `settings` in `ui` and answers the dialog's panel ID; `closed_by` is
/// the action that opened them (its shortcut closes them, DG8).
///
/// # Errors
/// The interface's, if its own tree is inconsistent (never expected).
pub fn open_preferences(
    ui: &mut Ui<DemoMessage>,
    texts: &PreferencesTexts,
    settings: &Settings,
    closed_by: Option<ActionId>,
    list: &mut DisplayList,
) -> Result<WidgetId, UiError> {
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
        ListBox::new(vec![texts.appearance.clone()], 0, |index| {
            DemoMessage::Preferences(PreferencesMessage::Section(index))
        })
        .label(texts.sections.clone()),
        LayoutProps::default(),
    )?;
    // Appearance: a label and its control on each line, the labels in one column.
    let grid = ui.add(
        row,
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
    let (values, selected) = theme_values(texts, settings);
    ui.add(
        grid,
        orchestra_ui::Label::new(texts.theme.clone()),
        label(0),
    )?;
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
    ui.add(grid, orchestra_ui::Label::new(texts.mode.clone()), label(1))?;
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
    ui.focus_first(list);
    Ok(panel)
}

/// What a change asks of the interface, once written into the settings ([`apply`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(
    clippy::large_enum_variant,
    reason = "one value per user change in the Preferences, used at once and dropped: the size \
              of a pair costs nothing there, and boxing it would only add an allocation"
)]
pub enum Effect {
    /// Nothing more (a section chosen, the dialog closed, a value chosen again).
    None,
    /// Draw with this pair in this mode (the window's colours, the interface's tokens).
    Theme(ThemePair, ThemeMode),
    /// Turn the cursor's blink on or off.
    CaretBlink(bool),
}

/// Writes `message` into `settings` and answers what the interface must do now (DG8). A theme's
/// index beyond the built-in pairs keeps the custom theme in use (the only other value the list
/// holds, choice 4). Problems found while resolving the theme (an unknown identifier: never from
/// this list) are dropped: the drawing falls back to Studio, as at start-up.
pub fn apply(settings: &mut Settings, message: PreferencesMessage) -> Effect {
    match message {
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
        PreferencesMessage::Section(_) | PreferencesMessage::Closed => Effect::None,
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
    //! The changes written into the settings, and the drop-down's values.
    use super::*;

    /// Texts with recognisable names (the catalogues are tested elsewhere).
    fn texts() -> PreferencesTexts {
        PreferencesTexts::new(&Localizer::new(orchestra_i18n::Locale::EnUs).unwrap()).unwrap()
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

    /// The ten pairs by name, Studio chosen by default; a custom theme in use is added at the
    /// end and chosen.
    #[test]
    fn the_drop_down_lists_the_pairs_and_the_custom_theme_in_use() {
        let texts = texts();
        let (values, selected) = theme_values(&texts, &Settings::default());
        assert_eq!(values.len(), 10);
        assert_eq!(values[selected], "Studio");
        let mut settings = Settings::default();
        settings.theme.choice = ThemeChoice::Custom("Mine".to_owned());
        let (values, selected) = theme_values(&texts, &settings);
        assert_eq!(values.len(), 11);
        assert_eq!(values[selected], "Mine");
    }
}
