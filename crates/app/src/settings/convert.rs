// SPDX-License-Identifier: GPL-3.0-or-later

//! Between the settings file's plain values (`orchestra_io::settings`) and the interface's types
//! (`orchestra_ui`, `orchestra_i18n`), in both directions (ADR-0050 DG6, choice S7).
//!
//! # Why here
//! `orchestra-io` and `orchestra-ui` do not depend on each other (DG6): the file knows identifiers
//! and texts, the interface knows tokens, pairs and shortcuts. The application is the one crate
//! that sees both, so the translation, and the warnings for what the interface refuses (an
//! unknown pair, token, language or shortcut), live here.
//!
//! # Leniency, continued
//! As in the file's reader, nothing here fails: a value the interface does not know gives the
//! default (or is dropped, for one override or one shortcut) and a [`SettingsProblem`].
//!
//! # Cost
//! O(size of the settings), with linear lookups of tokens and pairs over a few dozen names; run at
//! start-up and when the Preferences change something, never per frame.

use std::fmt;

use orchestra_i18n::Locale;
use orchestra_io::settings::{
    self as file, CustomTheme as FileCustom, Language, Mode, ShortcutEntry, ThemeChoice,
    ThemeOverride,
};
use orchestra_playback::{Tuning, TuningValue};
use orchestra_ui::{
    BuiltInTheme, ColorToken, CustomTheme, Density, Family, FamilyToken, FineModifier, LoadWarning,
    Override, Registry, ShapeToken, Shortcut, ShortcutTextError, SizeToken, ThemeMode, ThemePair,
};

/// A setting the interface could not use as written. Logged, never shown as an error (DG6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsProblem {
    /// What the file's reader reported.
    File(file::Warning),
    /// `theme.use` or a custom theme's `base` names no built-in pair; Studio is used.
    UnknownPair(String),
    /// A custom theme's override names an unknown token, family or density; it is dropped.
    UnknownOverride {
        /// The custom theme's name.
        theme: String,
        /// The name that is not known.
        name: String,
    },
    /// The language tag is none of the six catalogues; the system's language is used.
    UnknownLanguage(String),
    /// A shortcut's text could not be read; it is dropped from its action's list.
    Shortcut {
        /// The action.
        action: String,
        /// The text.
        text: String,
        /// Why.
        error: ShortcutTextError,
    },
    /// What the action registry reported while applying the shortcuts (ADR-0048 AM7).
    Registry(LoadWarning),
    /// A playback value outside its range (ADR-0027 §14–§15, F2.12a4b2): brought to the nearest
    /// limit.
    PlaybackValue {
        /// Its name in the file.
        name: String,
        /// The value written there.
        value: i64,
    },
}

impl fmt::Display for SettingsProblem {
    /// A short English description for the log (never shown on screen).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::File(warning) => write!(f, "{warning}"),
            Self::UnknownPair(id) => write!(f, "no theme pair is called {id:?}; Studio is used"),
            Self::UnknownOverride { theme, name } => {
                write!(f, "custom theme {theme:?}: {name:?} is not known; dropped")
            }
            Self::UnknownLanguage(tag) => {
                write!(f, "no interface language {tag:?}; the system's is used")
            }
            Self::Shortcut {
                action,
                text,
                error,
            } => write!(f, "shortcut {text:?} of {action}: {error}; dropped"),
            Self::Registry(LoadWarning::UnknownAction(action)) => {
                write!(f, "no action {action}; its shortcuts are kept and ignored")
            }
            Self::Registry(LoadWarning::FixedKey { shortcut, .. }) => write!(
                f,
                "{} is a fixed key and cannot be a shortcut; dropped",
                shortcut.settings_text()
            ),
            Self::Registry(LoadWarning::Conflict { shortcut, .. }) => write!(
                f,
                "{} was given to two actions; the later entry keeps it",
                shortcut.settings_text()
            ),
            Self::PlaybackValue { name, value } => {
                write!(
                    f,
                    "playback value {name} = {value} is out of range; clamped"
                )
            }
        }
    }
}

/// The theme the settings choose: the resolved pair and the mode (ADR-0037 T1, T7).
///
/// A custom theme is resolved from its base and overrides; a built-in identifier that is not
/// known, or a custom theme whose base is not known, gives Studio with a problem.
#[must_use]
pub fn theme_from_settings(
    settings: &file::ThemeSettings,
    problems: &mut Vec<SettingsProblem>,
) -> (ThemePair, ThemeMode) {
    let mode = match settings.mode {
        Mode::System => ThemeMode::System,
        Mode::Light => ThemeMode::Light,
        Mode::Dark => ThemeMode::Dark,
    };
    let pair = match &settings.choice {
        ThemeChoice::BuiltIn(id) => *built_in(id, problems).pair(),
        ThemeChoice::Custom(name) => settings
            .custom
            .iter()
            .find(|custom| &custom.name == name)
            .map_or_else(
                // The file's reader only keeps a choice whose theme exists; this is a guard.
                || *BuiltInTheme::default().pair(),
                |custom| custom_theme_from_settings(custom, problems).resolve(),
            ),
    };
    (pair, mode)
}

/// The built-in pair `id`, or Studio with a problem.
fn built_in(id: &str, problems: &mut Vec<SettingsProblem>) -> BuiltInTheme {
    BuiltInTheme::from_id(id).unwrap_or_else(|| {
        problems.push(SettingsProblem::UnknownPair(id.to_owned()));
        BuiltInTheme::default()
    })
}

/// A custom theme of the file as the interface's, dropping the overrides it does not know.
#[must_use]
pub fn custom_theme_from_settings(
    custom: &FileCustom,
    problems: &mut Vec<SettingsProblem>,
) -> CustomTheme {
    let mut theme = CustomTheme::new(built_in(&custom.base, problems));
    for (from, to) in [
        (&custom.light, &mut theme.light),
        (&custom.dark, &mut theme.dark),
    ] {
        for item in from {
            match override_of(item) {
                Ok(converted) => to.push(converted),
                Err(name) => problems.push(SettingsProblem::UnknownOverride {
                    theme: custom.name.clone(),
                    name: name.to_owned(),
                }),
            }
        }
    }
    theme
}

/// One override of the file as the interface's, or the name that is not known.
fn override_of(item: &ThemeOverride) -> Result<Override, &str> {
    Ok(match item {
        ThemeOverride::Color { token, rgba } => Override::Color(
            ColorToken::from_name(token).ok_or(token.as_str())?,
            orchestra_ui::Color::rgba(*rgba),
        ),
        ThemeOverride::Shape { token, value } => {
            Override::Shape(ShapeToken::from_name(token).ok_or(token.as_str())?, *value)
        }
        ThemeOverride::Family { token, family } => Override::Family(
            FamilyToken::from_name(token).ok_or(token.as_str())?,
            Family::from_name(family).ok_or(family.as_str())?,
        ),
        ThemeOverride::TextSize { token, size } => {
            Override::TextSize(SizeToken::from_name(token).ok_or(token.as_str())?, *size)
        }
        ThemeOverride::Density(name) => {
            Override::Density(Density::from_name(name).ok_or(name.as_str())?)
        }
    })
}

/// The interface's custom theme as the file's, named `name` (the inverse of [`custom_theme_from_settings`]).
#[must_use]
pub fn custom_theme_to_settings(name: &str, theme: &CustomTheme) -> FileCustom {
    FileCustom {
        name: name.to_owned(),
        base: theme.base.id().to_owned(),
        light: theme
            .light
            .iter()
            .map(|&item| override_to_file(item))
            .collect(),
        dark: theme
            .dark
            .iter()
            .map(|&item| override_to_file(item))
            .collect(),
    }
}

/// One override of the interface as the file's.
fn override_to_file(item: Override) -> ThemeOverride {
    match item {
        Override::Color(token, color) => ThemeOverride::Color {
            token: token.name().to_owned(),
            rgba: u32::from_be_bytes([color.r, color.g, color.b, color.a]),
        },
        Override::Shape(token, value) => ThemeOverride::Shape {
            token: token.name().to_owned(),
            value,
        },
        Override::Family(token, family) => ThemeOverride::Family {
            token: token.name().to_owned(),
            family: family.name().to_owned(),
        },
        Override::TextSize(token, size) => ThemeOverride::TextSize {
            token: token.name().to_owned(),
            size,
        },
        Override::Density(density) => ThemeOverride::Density(density.name().to_owned()),
    }
}

/// The interface's mode as the file's.
#[must_use]
pub const fn mode_to_settings(mode: ThemeMode) -> Mode {
    match mode {
        ThemeMode::System => Mode::System,
        ThemeMode::Light => Mode::Light,
        ThemeMode::Dark => Mode::Dark,
    }
}

/// The playback's tuning the settings choose (ADR-0027 §14–§15, F2.12a4b2): the tuning by ear,
/// changed by the values the file names. A name this program does not know is left alone (it
/// stays in the file, S2: a newer Orchestra may know it); a value out of its range is clamped and
/// reported. O(entries × 18).
#[must_use]
pub fn tuning_from_settings(
    playback: &std::collections::BTreeMap<String, i64>,
    problems: &mut Vec<SettingsProblem>,
) -> Tuning {
    let mut tuning = Tuning::default();
    for (name, value) in playback {
        if let Some(known) = TuningValue::from_name(name)
            && tuning.set(known, *value)
        {
            problems.push(SettingsProblem::PlaybackValue {
                name: name.clone(),
                value: *value,
            });
        }
    }
    tuning
}

/// Writes `tuning` into the file's map: the values that differ from the tuning by ear, by name;
/// a known value back at its default is removed, the names this program does not know are kept
/// (S2).
pub fn tuning_to_settings(tuning: &Tuning, playback: &mut std::collections::BTreeMap<String, i64>) {
    for value in TuningValue::ALL {
        playback.remove(value.name());
    }
    for (name, value) in tuning.differences() {
        playback.insert(name.to_owned(), value);
    }
}

/// The modifier of fine adjustment the settings choose (ADR-0046 V3, DG9, F2.12a4b1).
#[must_use]
pub const fn fine_modifier_from_settings(fine: file::FineModifier) -> FineModifier {
    match fine {
        file::FineModifier::Primary => FineModifier::Primary,
        file::FineModifier::Shift => FineModifier::Shift,
        file::FineModifier::Alt => FineModifier::Alt,
    }
}

/// The interface's modifier of fine adjustment as the file's.
#[must_use]
pub const fn fine_modifier_to_settings(fine: FineModifier) -> file::FineModifier {
    match fine {
        FineModifier::Primary => file::FineModifier::Primary,
        FineModifier::Shift => file::FineModifier::Shift,
        FineModifier::Alt => file::FineModifier::Alt,
    }
}

/// The language the settings choose: `None` for the system's (and for an unknown tag, with a
/// problem). Until F2.13 the program shows en-US for `None`.
#[must_use]
pub fn language_from_settings(
    language: &Language,
    problems: &mut Vec<SettingsProblem>,
) -> Option<Locale> {
    match language {
        Language::System => None,
        Language::Tag(tag) => {
            let locale = Locale::from_tag(tag);
            if locale.is_none() {
                problems.push(SettingsProblem::UnknownLanguage(tag.clone()));
            }
            locale
        }
    }
}

/// The interface's language as the file's.
#[must_use]
pub fn language_to_settings(locale: Option<Locale>) -> Language {
    locale.map_or(Language::System, |locale| {
        Language::Tag(locale.tag().to_owned())
    })
}

/// Applies the settings' shortcuts to `registry` (ADR-0048 AM7): each text read with
/// [`Shortcut::from_settings_text`] (one that cannot be read is dropped alone), then the
/// registry's own rules (fixed keys, conflicts, unknown actions).
pub fn apply_settings_shortcuts(
    entries: &[ShortcutEntry],
    registry: &mut Registry,
    problems: &mut Vec<SettingsProblem>,
) {
    let mut converted = Vec::with_capacity(entries.len());
    for entry in entries {
        let mut shortcuts = Vec::with_capacity(entry.keys.len());
        for text in &entry.keys {
            match Shortcut::from_settings_text(text) {
                Ok(shortcut) => shortcuts.push(shortcut),
                Err(error) => problems.push(SettingsProblem::Shortcut {
                    action: entry.action.clone(),
                    text: text.clone(),
                    error,
                }),
            }
        }
        converted.push((entry.action.clone(), shortcuts));
    }
    problems.extend(
        registry
            .load_overrides(converted)
            .into_iter()
            .map(SettingsProblem::Registry),
    );
}

/// The registry's differences from the defaults as the file's entries (the inverse of
/// [`apply_settings_shortcuts`]), in the registry's order.
#[must_use]
pub fn shortcuts_to_settings(registry: &Registry) -> Vec<ShortcutEntry> {
    registry
        .overrides()
        .map(|(action, shortcuts)| ShortcutEntry {
            action: action.to_owned(),
            keys: shortcuts.iter().map(Shortcut::settings_text).collect(),
        })
        .collect()
}
