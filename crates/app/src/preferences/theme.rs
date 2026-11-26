// SPDX-License-Identifier: GPL-3.0-or-later

//! The theme editor of the Preferences' **Appearance** section (ADR-0037 T7–T8, ADR-0050 DG9,
//! SCALETTA F2.12b1, owner's choices 1–8 of 2026-11-25).
//!
//! # What the user sees (choice 1)
//! Under *Theme*, *Mode* and *Blinking cursor*:
//! - **Duplicate** and **Delete**: *Duplicate* makes a custom theme from the theme chosen (its
//!   base and its changes), named "Name (copy)" and chosen at once; *Delete* (custom themes only)
//!   asks first, then goes back to the deleted theme's base. The theme drop-down lists the ten
//!   built-in pairs and every custom theme.
//! - For a **built-in** theme, a muted line: they cannot be changed; duplicate one.
//! - For a **custom** theme: its **Name** (a field: the name is committed with Enter or when the
//!   field is left), then **Readability** (see below), then the heading **Colors**, a muted line
//!   saying which half is shown, then one row per colour token: a warning dot when the token is
//!   part of a pair that is hard to read, its name, a swatch, a field with its value (`#rrggbb`,
//!   or `#rrggbbaa` with transparency) and **Restore** (enabled while the value differs from the
//!   base's).
//!
//! # The half edited (choice 2)
//! The half on screen ([`Ui::appearance`], told by the window): the preview always shows what
//! changes. To edit the other half, the user chooses it in *Mode*, just above; the fields follow
//! at once ([`ThemeEditor::show_half`]).
//!
//! # A value typed (choices 3–4)
//! As soon as a field holds a valid colour ([`parse_color`]), the colour becomes the theme's
//! (saved, DG8) and the whole window is drawn with it: the Preferences themselves are the
//! preview. A value equal to the base's removes the override instead, so the theme keeps only its
//! differences (T7). A value that is not a colour is left in the field and changes nothing.
//!
//! # Readability (choice 5, T8; owner's requests of 2026-11-26)
//! At the **top** of a custom theme, under its name, so it is seen without scrolling. Every pair
//! of `CONTRAST_PAIRS` below its threshold (`Theme::contrast_failures`) is grouped by the colour
//! that is hard to see, and each group becomes **one sentence in plain words, without numbers**
//! ([`readability_issues`], [`issue_line`]): how bad it is ("almost invisible" below 2:1, "hard
//! to read" or "hard to make out" below 3:1, "readable, but not well enough for people who don't
//! see well" up to the threshold, 4.5:1 for text and 3:1 for graphics), on which colour it is
//! worst and on how many others too, and what to do: make it lighter or darker, depending on
//! which of the two colours is the lighter. With no problem, one muted line says every text and
//! control is easy to read. Never a block: the colour is the theme's all the same.
//!
//! # Where the theme lives
//! In the settings ([`Settings::theme`]): the custom themes by name, each a base and its
//! overrides per half, and the choice. Every function here changes the settings; the
//! demonstration then applies the theme the settings choose (`theme_from_settings`) to the window
//! and to the interface.

use orchestra_i18n::{FluentValue, I18nError, Localizer};
use orchestra_io::settings::{CustomTheme as FileCustom, Settings, ThemeChoice, ThemeOverride};
use orchestra_ui::{
    Appearance, BuiltInTheme, Button, ButtonRole, CONTRAST_PAIRS, Color, ColorToken, ContrastKind,
    DialogKind, DialogSpec, DisplayList, DropDown, FamilyToken, Flex, Grid, Label, LayoutProps,
    Length, SizeToken, Space, Swatch, TextField, Theme, Track, Ui, UiError, Weight, WidgetId,
    relative_luminance,
};

use crate::demo::DemoMessage;
use crate::settings::convert::theme_from_settings;
use crate::template::{ISOLATION_MARKS, Template};

/// What the theme editor's widgets send (inside [`DemoMessage::Theme`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThemeMessage {
    /// *Duplicate*.
    Duplicate,
    /// *Delete*.
    Delete,
    /// The answer to "Delete the theme…?": `true` for *Delete*.
    DeleteAnswer(bool),
    /// The name field was committed.
    Rename(String),
    /// The value field of the colour token number `.0` of `ColorToken::ALL` changed.
    Color(usize, String),
    /// *Restore* of the colour token number `.0`.
    Restore(usize),
}

/// The theme editor's texts in one language (CLAUDE.md §7).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThemeTexts {
    /// `prefs-theme-duplicate`.
    pub duplicate: String,
    /// `prefs-theme-delete`.
    pub delete: String,
    /// `prefs-theme-name`.
    pub name: String,
    /// `prefs-theme-copy-name`, with `$name`.
    pub copy_name: Template,
    /// `prefs-theme-colors`.
    pub colors: String,
    /// `prefs-theme-builtin`.
    pub builtin: String,
    /// `prefs-theme-editing-light`.
    pub editing_light: String,
    /// `prefs-theme-editing-dark`.
    pub editing_dark: String,
    /// `prefs-theme-restore`.
    pub restore: String,
    /// `prefs-theme-value-name`, with `$color`.
    pub value_name: Template,
    /// `prefs-theme-readability`.
    pub readability: String,
    /// `prefs-theme-readability-ok`.
    pub readability_ok: String,
    /// `prefs-theme-readability-issue`, with `$color` and `$background`, formatted once per
    /// combination of the values it selects on (see [`issue_template_index`]): a message that
    /// selects cannot take placeholders for them (`crate::template`).
    pub issues: Vec<Template>,
    /// `prefs-theme-delete-title`, with `$name`.
    pub delete_title: Template,
    /// `prefs-theme-delete-message`.
    pub delete_message: String,
    /// `dialog-delete`.
    pub delete_button: String,
    /// `dialog-cancel`.
    pub cancel: String,
    /// The colour tokens' names (`theme-color-*`), in `ColorToken::ALL`'s order.
    pub color_names: Vec<String>,
}

impl ThemeTexts {
    /// The texts in `localizer`'s language.
    ///
    /// # Errors
    /// The localizer's, if a message is missing (the catalogues' parity tests prevent it).
    pub fn new(localizer: &Localizer) -> Result<Self, I18nError> {
        let text = |key: &str| -> Result<String, I18nError> {
            Ok(localizer.format(key, None)?.replace(ISOLATION_MARKS, ""))
        };
        let template = |key: &str, variables: &[&str]| Template::new(localizer, key, variables);
        let mut color_names = Vec::with_capacity(ColorToken::ALL.len());
        for token in ColorToken::ALL {
            color_names.push(text(&format!("theme-color-{}", token.name()))?);
        }
        Ok(Self {
            duplicate: text("prefs-theme-duplicate")?,
            delete: text("prefs-theme-delete")?,
            name: text("prefs-theme-name")?,
            copy_name: template("prefs-theme-copy-name", &["name"])?,
            colors: text("prefs-theme-colors")?,
            builtin: text("prefs-theme-builtin")?,
            editing_light: text("prefs-theme-editing-light")?,
            editing_dark: text("prefs-theme-editing-dark")?,
            restore: text("prefs-theme-restore")?,
            value_name: template("prefs-theme-value-name", &["color"])?,
            readability: text("prefs-theme-readability")?,
            readability_ok: text("prefs-theme-readability-ok")?,
            issues: issue_templates(localizer)?,
            delete_title: template("prefs-theme-delete-title", &["name"])?,
            delete_message: text("prefs-theme-delete-message")?,
            delete_button: text("dialog-delete")?,
            cancel: text("dialog-cancel")?,
            color_names,
        })
    }

    /// Every text with one value in it, each holding its placeholder once (tests). The
    /// readability sentences are in [`ThemeTexts::issues`].
    #[must_use]
    pub fn templates(&self) -> [&Template; 3] {
        [&self.copy_name, &self.value_name, &self.delete_title]
    }

    /// The name of `token`.
    fn color_name(&self, token: ColorToken) -> &str {
        ColorToken::ALL
            .iter()
            .position(|known| *known == token)
            .and_then(|index| self.color_names.get(index))
            .map_or("", String::as_str)
    }
}

// ------------------------------------------------------------------------------------------------
// Values: colours, ratios, names
// ------------------------------------------------------------------------------------------------

/// The colour written in `text` (choice 3): `#rrggbb` or `#rrggbbaa`, the `#` optional, either
/// case, spaces around ignored. `None` for anything else. O(length).
#[must_use]
pub(crate) fn parse_color(text: &str) -> Option<Color> {
    let digits = text.trim();
    let digits = digits.strip_prefix('#').unwrap_or(digits);
    if !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let value = u32::from_str_radix(digits, 16).ok()?;
    match digits.len() {
        8 => Some(Color::rgba(value)),
        // Opaque: the alpha byte appended.
        6 => Some(Color::rgba(value << 8 | 0xff)),
        _ => None,
    }
}

/// `color` written as the field shows it: `#rrggbb` when opaque, `#rrggbbaa` otherwise, lower
/// case.
#[must_use]
pub(crate) fn color_text(color: Color) -> String {
    if color.a == 0xff {
        format!("#{:02x}{:02x}{:02x}", color.r, color.g, color.b)
    } else {
        format!(
            "#{:02x}{:02x}{:02x}{:02x}",
            color.r, color.g, color.b, color.a
        )
    }
}

/// `wanted`, or with " (2)", " (3)"… appended until no custom theme of `settings` has it.
/// O(themes × tries).
#[must_use]
pub(crate) fn unique_name(settings: &Settings, wanted: &str) -> String {
    let taken = |name: &str| {
        settings
            .theme
            .custom
            .iter()
            .any(|custom| custom.name == name)
    };
    if !taken(wanted) {
        return wanted.to_owned();
    }
    // A bounded range, not `2..`: clippy cannot prove an open range's search ends. It does end
    // after at most `themes + 1` tries, since `themes` names cannot take more numbers than there
    // are themes; the fallback (the name as wanted, a duplicate) is therefore never reached.
    (2..=u32::MAX)
        .map(|n| format!("{wanted} ({n})"))
        .find(|name| !taken(name))
        .unwrap_or_else(|| wanted.to_owned())
}

/// The index of the chosen custom theme in `settings`' list, if a custom one is chosen.
#[must_use]
pub(crate) fn chosen_custom(settings: &Settings) -> Option<usize> {
    match &settings.theme.choice {
        ThemeChoice::Custom(name) => settings
            .theme
            .custom
            .iter()
            .position(|custom| &custom.name == name),
        ThemeChoice::BuiltIn(_) => None,
    }
}

/// The theme drop-down's values and the chosen index: the ten built-in pairs (named by `pairs`,
/// in `BuiltInTheme::ALL`'s order), then every custom theme by name, in the settings' order.
#[must_use]
pub fn theme_values(pairs: &[String], settings: &Settings) -> (Vec<String>, usize) {
    let mut values = pairs.to_vec();
    values.extend(
        settings
            .theme
            .custom
            .iter()
            .map(|custom| custom.name.clone()),
    );
    // The index of the default pair (Studio): what `theme_from_settings` draws for an unknown
    // built-in identifier and for a custom choice whose theme is missing, so the drop-down shows
    // what the window shows.
    let default = BuiltInTheme::ALL
        .iter()
        .position(|pair| *pair == BuiltInTheme::default())
        .unwrap_or(0);
    let selected = match &settings.theme.choice {
        ThemeChoice::BuiltIn(id) => BuiltInTheme::ALL
            .iter()
            .position(|pair| pair.id() == id)
            .unwrap_or(default),
        ThemeChoice::Custom(_) => {
            chosen_custom(settings).map_or(default, |index| BuiltInTheme::ALL.len() + index)
        }
    };
    (values, selected)
}

/// Chooses the drop-down's value `index` (see [`theme_values`]); out of the list: nothing.
pub(crate) fn choose(settings: &mut Settings, index: usize) {
    if let Some(pair) = BuiltInTheme::ALL.get(index) {
        settings.theme.choice = ThemeChoice::BuiltIn(pair.id().to_owned());
    } else if let Some(custom) = index
        .checked_sub(BuiltInTheme::ALL.len())
        .and_then(|index| settings.theme.custom.get(index))
    {
        settings.theme.choice = ThemeChoice::Custom(custom.name.clone());
    }
}

/// *Duplicate* (see the module documentation): a custom theme made from the chosen one, named
/// "Name (copy)" (made unique), and chosen. `pairs` names the built-in pairs.
pub(crate) fn duplicate(settings: &mut Settings, texts: &ThemeTexts, pairs: &[String]) {
    // A custom theme is copied whole (its base and its overrides of both halves); a built-in pair
    // becomes a custom theme with that base and no overrides yet.
    let copy = if let Some(custom) =
        chosen_custom(settings).and_then(|index| settings.theme.custom.get(index))
    {
        FileCustom {
            name: texts.copy_name.fill(&[&custom.name]),
            ..custom.clone()
        }
    } else {
        let base = match &settings.theme.choice {
            ThemeChoice::BuiltIn(id) => BuiltInTheme::from_id(id).unwrap_or_default(),
            ThemeChoice::Custom(_) => BuiltInTheme::default(),
        };
        let index = BuiltInTheme::ALL
            .iter()
            .position(|pair| *pair == base)
            .unwrap_or(0);
        let name = pairs.get(index).map_or(base.id(), String::as_str);
        FileCustom {
            name: texts.copy_name.fill(&[name]),
            base: base.id().to_owned(),
            light: Vec::new(),
            dark: Vec::new(),
        }
    };
    let name = unique_name(settings, &copy.name);
    settings.theme.choice = ThemeChoice::Custom(name.clone());
    settings.theme.custom.push(FileCustom { name, ..copy });
}

/// Deletes the chosen custom theme and chooses its base (see the module documentation). Nothing
/// for a built-in theme.
pub(crate) fn delete_chosen(settings: &mut Settings) {
    let Some(index) = chosen_custom(settings) else {
        return;
    };
    let removed = settings.theme.custom.remove(index);
    let base = BuiltInTheme::from_id(&removed.base).unwrap_or_default();
    settings.theme.choice = ThemeChoice::BuiltIn(base.id().to_owned());
}

/// Renames the chosen custom theme to `name`, trimmed and made unique; an empty name, or the
/// same name, changes nothing. Answers whether it changed.
pub(crate) fn rename_chosen(settings: &mut Settings, name: &str) -> bool {
    let name = name.trim();
    let Some(index) = chosen_custom(settings) else {
        return false;
    };
    if name.is_empty()
        || settings
            .theme
            .custom
            .get(index)
            .is_some_and(|c| c.name == name)
    {
        return false;
    }
    let name = unique_name(settings, name);
    if let Some(custom) = settings.theme.custom.get_mut(index) {
        custom.name.clone_from(&name);
    }
    settings.theme.choice = ThemeChoice::Custom(name);
    true
}

/// Gives `token` the value `color` in the `half` of the chosen custom theme (`None` restores the
/// base's). The theme keeps only its differences (T7): the token's earlier overrides of that half
/// go, and a value equal to the base's adds none. Nothing for a built-in theme.
pub(crate) fn set_color(
    settings: &mut Settings,
    half: Appearance,
    token: ColorToken,
    color: Option<Color>,
) {
    let Some(index) = chosen_custom(settings) else {
        return;
    };
    let Some(custom) = settings.theme.custom.get_mut(index) else {
        return;
    };
    let base = BuiltInTheme::from_id(&custom.base)
        .unwrap_or_default()
        .pair()
        .instance(half)
        .color(token);
    let overrides = match half {
        Appearance::Light => &mut custom.light,
        Appearance::Dark => &mut custom.dark,
    };
    overrides.retain(
        |item| !matches!(item, ThemeOverride::Color { token: name, .. } if name == token.name()),
    );
    if let Some(color) = color.filter(|color| *color != base) {
        overrides.push(ThemeOverride::Color {
            token: token.name().to_owned(),
            rgba: u32::from_be_bytes([color.r, color.g, color.b, color.a]),
        });
    }
}

/// The instance of the chosen theme's `half`, as the window draws it.
#[must_use]
pub(crate) fn instance(settings: &Settings, half: Appearance) -> Theme {
    let (pair, _) = theme_from_settings(&settings.theme, &mut Vec::new());
    *pair.instance(half)
}

// ------------------------------------------------------------------------------------------------
// Readability: the contrast failures in plain words
// ------------------------------------------------------------------------------------------------

/// How hard a colour is to see on the colour behind it, in words (owner's thresholds of
/// 2026-11-26). Ordered from the worst, so sorting puts the worst problems first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Severity {
    /// Below 2:1: almost invisible.
    Invisible,
    /// From 2:1 to below 3:1: hard to read (text) or to make out (graphics).
    Hard,
    /// From 3:1 to below 4.5:1, text only (graphics need 3:1): readable, but not well enough for
    /// people who do not see well.
    Weak,
}

impl Severity {
    /// The severity of a pair that reaches `ratio` (below its threshold: the caller only asks
    /// for failures). 2:1 and 3:1 are the owner's choice; 3:1 is also WCAG's line for graphics
    /// and for large text, the point below which even large letters are hard to read.
    fn of(ratio: f64) -> Self {
        if ratio < 2.0 {
            Self::Invisible
        } else if ratio < 3.0 {
            Self::Hard
        } else {
            Self::Weak
        }
    }
}

/// Which way the hard-to-see colour should move: away from the colour behind it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Direction {
    /// It is the lighter of the two: make it lighter still, or the colours behind it darker.
    Lighter,
    /// It is the darker: make it darker still, or the colours behind it lighter.
    Darker,
}

/// One readability problem: a colour hard to see on one or more colours behind it (all the
/// failing pairs that share the same foreground, grouped into one sentence).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Issue {
    /// The colour that is hard to see (the pairs' foreground).
    pub(crate) color: ColorToken,
    /// The colour behind it where it is worst.
    pub(crate) background: ColorToken,
    /// Letters or a graphic (the worst pair's kind: a foreground is only ever one of them).
    pub(crate) kind: ContrastKind,
    /// How bad it is on `background`.
    pub(crate) severity: Severity,
    /// Which way to move `color`, measured against `background`.
    pub(crate) direction: Direction,
    /// How many other colours behind it fail too.
    pub(crate) others: usize,
    /// The worst ratio reached (never shown: it only chooses `background` and `severity`).
    worst: f64,
}

/// The readability problems of `theme`: one per foreground with a pair below its threshold, the
/// worst first (then in `CONTRAST_PAIRS`' order). None for every built-in pair (ADR-0037 T5).
///
/// # Complexity
/// O(pairs × issues): at most 33 pairs and 11 foregrounds, a few hundred comparisons, run once
/// per edit (never per frame).
pub(crate) fn readability_issues(theme: &Theme) -> Vec<Issue> {
    let mut issues: Vec<Issue> = Vec::new();
    for failure in theme.contrast_failures() {
        let (color, background) = (failure.pair.foreground, failure.pair.background);
        if let Some(issue) = issues.iter_mut().find(|issue| issue.color == color) {
            issue.others += 1;
            if failure.ratio < issue.worst {
                issue.background = background;
                issue.kind = failure.pair.kind;
                issue.worst = failure.ratio;
            }
        } else {
            issues.push(Issue {
                color,
                background,
                kind: failure.pair.kind,
                severity: Severity::Weak,
                direction: Direction::Lighter,
                others: 0,
                worst: failure.ratio,
            });
        }
    }
    for issue in &mut issues {
        issue.severity = Severity::of(issue.worst);
        // The colour as drawn: a translucent one is blended over the colour behind it first,
        // as the contrast check itself does, then the two luminances are compared.
        let behind = theme.color(issue.background);
        let seen = theme.color(issue.color).over(behind);
        issue.direction = if relative_luminance(seen) >= relative_luminance(behind) {
            Direction::Lighter
        } else {
            Direction::Darker
        };
    }
    // Stable: equal severities keep the pairs' order.
    issues.sort_by_key(|issue| issue.severity);
    issues
}

/// The five combinations of kind and severity that can occur (graphics are never `Weak`: their
/// threshold is 3:1), in the templates' order, with the values the message selects on.
const KIND_SEVERITY: [(ContrastKind, Severity, &str, &str); 5] = [
    (ContrastKind::Text, Severity::Invisible, "text", "invisible"),
    (ContrastKind::Text, Severity::Hard, "text", "hard"),
    (ContrastKind::Text, Severity::Weak, "text", "weak"),
    (
        ContrastKind::Graphic,
        Severity::Invisible,
        "graphic",
        "invisible",
    ),
    (ContrastKind::Graphic, Severity::Hard, "graphic", "hard"),
];

/// The two directions in the templates' order, with the value the message selects on.
const DIRECTIONS: [(Direction, &str); 2] = [
    (Direction::Lighter, "lighter"),
    (Direction::Darker, "darker"),
];

/// The largest number of *other* colours behind one foreground: the most pairs any foreground
/// has in `CONTRAST_PAIRS`, minus one (7 for normal text, so 6). O(pairs²), at start-up only.
fn max_others() -> usize {
    CONTRAST_PAIRS
        .iter()
        .map(|pair| {
            CONTRAST_PAIRS
                .iter()
                .filter(|other| other.foreground == pair.foreground)
                .count()
        })
        .max()
        .unwrap_or(1)
        .saturating_sub(1)
}

/// The index in [`ThemeTexts::issues`] of the sentence for `kind`, `severity`, `direction` and
/// `others`; `None` for a combination that cannot occur (a graphic that is `Weak`, or more
/// others than any foreground has). The layout is kind-and-severity major, then direction, then
/// the count: `(ks × 2 + d) × (max + 1) + others`.
pub(crate) fn issue_template_index(
    kind: ContrastKind,
    severity: Severity,
    direction: Direction,
    others: usize,
) -> Option<usize> {
    let max = max_others();
    let ks = KIND_SEVERITY
        .iter()
        .position(|(k, s, _, _)| *k == kind && *s == severity)?;
    let d = DIRECTIONS
        .iter()
        .position(|(known, _)| *known == direction)?;
    (others <= max).then_some((ks * DIRECTIONS.len() + d) * (max + 1) + others)
}

/// Formats `prefs-theme-readability-issue` once for every combination [`issue_template_index`]
/// lists, with `$color` and `$background` left as placeholders: 5 × 2 × 7 = 70 short formats per
/// language, once when the language is chosen (well under a millisecond in all).
///
/// # Errors
/// The localizer's, if the message is missing.
fn issue_templates(localizer: &Localizer) -> Result<Vec<Template>, I18nError> {
    let max = max_others();
    let mut templates = Vec::with_capacity(KIND_SEVERITY.len() * DIRECTIONS.len() * (max + 1));
    for (_, _, kind, severity) in KIND_SEVERITY {
        for (_, direction) in DIRECTIONS {
            for others in 0..=max {
                // A plural needs a number, not a text: Fluent picks "one" or "other" from it.
                let count = FluentValue::from(u32::try_from(others).unwrap_or(u32::MAX));
                templates.push(Template::with_values(
                    localizer,
                    "prefs-theme-readability-issue",
                    &["color", "background"],
                    &[
                        ("kind", FluentValue::from(kind)),
                        ("severity", FluentValue::from(severity)),
                        ("direction", FluentValue::from(direction)),
                        ("count", count),
                    ],
                )?);
            }
        }
    }
    Ok(templates)
}

/// The sentence of `issue` in the texts' language: the two colours named as the editor's rows
/// name them. Empty for a combination that cannot occur (see [`issue_template_index`]).
pub(crate) fn issue_line(texts: &ThemeTexts, issue: &Issue) -> String {
    issue_template_index(issue.kind, issue.severity, issue.direction, issue.others)
        .and_then(|index| texts.issues.get(index))
        .map(|template| {
            template.fill(&[
                texts.color_name(issue.color),
                texts.color_name(issue.background),
            ])
        })
        .unwrap_or_default()
}

// ------------------------------------------------------------------------------------------------
// The widgets
// ------------------------------------------------------------------------------------------------

/// The message of the value field of colour token `I` (a plain function, ADR-0045 WA7).
fn color_message<const I: usize>(text: &str) -> DemoMessage {
    DemoMessage::Theme(ThemeMessage::Color(I, text.to_owned()))
}

/// One message function per colour token, in `ColorToken::ALL`'s order.
const COLOR_MESSAGES: [fn(&str) -> DemoMessage; 24] = [
    color_message::<0>,
    color_message::<1>,
    color_message::<2>,
    color_message::<3>,
    color_message::<4>,
    color_message::<5>,
    color_message::<6>,
    color_message::<7>,
    color_message::<8>,
    color_message::<9>,
    color_message::<10>,
    color_message::<11>,
    color_message::<12>,
    color_message::<13>,
    color_message::<14>,
    color_message::<15>,
    color_message::<16>,
    color_message::<17>,
    color_message::<18>,
    color_message::<19>,
    color_message::<20>,
    color_message::<21>,
    color_message::<22>,
    color_message::<23>,
];

/// The message of the name field.
fn rename_message(text: &str) -> DemoMessage {
    DemoMessage::Theme(ThemeMessage::Rename(text.to_owned()))
}

/// One colour's row.
#[derive(Debug, Clone, Copy)]
struct Row {
    /// The warning mark (a "●" in the warning colour, or nothing).
    mark: WidgetId,
    /// The value field.
    field: WidgetId,
    /// *Restore*.
    restore: WidgetId,
}

/// What the demonstration does after a message ([`ThemeEditor::handle`]).
#[derive(Debug)]
pub enum ThemeOutcome {
    /// Nothing more.
    None,
    /// The theme the settings choose changed: apply it (the window's colours, the interface's
    /// tokens).
    Theme,
    /// Open this question; its answer comes back as a [`ThemeMessage`].
    Ask(DialogSpec<DemoMessage>),
}

/// The theme editor while the Appearance section is shown (see the module documentation).
#[derive(Debug)]
pub struct ThemeEditor {
    /// The theme drop-down (its values follow the custom themes).
    themes: WidgetId,
    /// *Duplicate* (always enabled: every theme can be duplicated).
    duplicate: WidgetId,
    /// *Delete* (enabled for a custom theme).
    delete: WidgetId,
    /// The part rebuilt when another theme is chosen: the note, or the name and the colours.
    area: WidgetId,
    /// The name field (a custom theme).
    name: Option<WidgetId>,
    /// The note of the half shown (a custom theme).
    half_note: Option<WidgetId>,
    /// The rows, in `ColorToken::ALL`'s order (a custom theme).
    rows: Vec<Row>,
    /// The column of the readability sentences, at the top (a custom theme).
    readability: Option<WidgetId>,
    /// The half the fields show.
    half: Appearance,
}

impl ThemeEditor {
    /// Builds the editor at the end of `parent` (the Appearance section's column), under the
    /// drop-down `themes`, for `settings`' theme and the half `half`.
    ///
    /// # Errors
    /// The interface's, if its own tree is inconsistent (never expected).
    pub fn build(
        ui: &mut Ui<DemoMessage>,
        parent: WidgetId,
        themes: WidgetId,
        texts: &ThemeTexts,
        settings: &Settings,
        half: Appearance,
        list: &mut DisplayList,
    ) -> Result<Self, UiError> {
        let buttons = ui.add(
            parent,
            Flex::row(Space::S, Space::None),
            LayoutProps::default(),
        )?;
        let duplicate = ui.add(
            buttons,
            Button::new(
                texts.duplicate.clone(),
                DemoMessage::Theme(ThemeMessage::Duplicate),
            ),
            LayoutProps::default(),
        )?;
        let delete = ui.add(
            buttons,
            Button::new(
                texts.delete.clone(),
                DemoMessage::Theme(ThemeMessage::Delete),
            ),
            LayoutProps::default(),
        )?;
        let area = ui.add(
            parent,
            Flex::column(Space::M, Space::None),
            LayoutProps::default().width(Length::Fill(1.0)),
        )?;
        let mut editor = Self {
            themes,
            duplicate,
            delete,
            area,
            name: None,
            half_note: None,
            rows: Vec::new(),
            readability: None,
            half,
        };
        editor.rebuild(ui, texts, settings, list)?;
        Ok(editor)
    }

    /// The value field of colour token number `index` (tests).
    #[must_use]
    pub fn field(&self, index: usize) -> Option<WidgetId> {
        self.rows.get(index).map(|row| row.field)
    }

    /// *Restore* of colour token number `index` (tests).
    #[must_use]
    pub fn restore_button(&self, index: usize) -> Option<WidgetId> {
        self.rows.get(index).map(|row| row.restore)
    }

    /// The warning mark of colour token number `index`: a label, empty unless the token is part
    /// of a pair below its contrast threshold (tests).
    #[must_use]
    pub fn mark(&self, index: usize) -> Option<WidgetId> {
        self.rows.get(index).map(|row| row.mark)
    }

    /// *Duplicate* (tests).
    #[must_use]
    pub const fn duplicate_button(&self) -> WidgetId {
        self.duplicate
    }

    /// *Delete* (tests).
    #[must_use]
    pub const fn delete_button(&self) -> WidgetId {
        self.delete
    }

    /// The note saying which half the fields show, for a custom theme (tests).
    #[must_use]
    pub const fn half_note_label(&self) -> Option<WidgetId> {
        self.half_note
    }

    /// The name field, for a custom theme (tests).
    #[must_use]
    pub const fn name_field(&self) -> Option<WidgetId> {
        self.name
    }

    /// The readability sentences' column, for a custom theme (tests).
    #[must_use]
    pub const fn readability_column(&self) -> Option<WidgetId> {
        self.readability
    }

    /// The half the fields show.
    #[must_use]
    pub const fn half(&self) -> Appearance {
        self.half
    }

    /// Acts on `message`, changing `settings` (the caller saves them, DG8) and the widgets, and
    /// answers what the demonstration must do next. `pairs` names the built-in pairs.
    pub fn handle(
        &mut self,
        ui: &mut Ui<DemoMessage>,
        texts: &ThemeTexts,
        pairs: &[String],
        settings: &mut Settings,
        message: ThemeMessage,
        list: &mut DisplayList,
    ) -> ThemeOutcome {
        match message {
            ThemeMessage::Duplicate => {
                duplicate(settings, texts, pairs);
                self.chosen_changed(ui, texts, pairs, settings, list);
                ThemeOutcome::Theme
            }
            ThemeMessage::Delete => {
                let Some(custom) =
                    chosen_custom(settings).and_then(|index| settings.theme.custom.get(index))
                else {
                    return ThemeOutcome::None;
                };
                ThemeOutcome::Ask(
                    DialogSpec::new(
                        DialogKind::Question,
                        texts.delete_title.fill(&[&custom.name]),
                        texts.delete_message.clone(),
                    )
                    .button(
                        texts.delete_button.clone(),
                        ButtonRole::Destructive,
                        DemoMessage::Theme(ThemeMessage::DeleteAnswer(true)),
                    )
                    .button(
                        texts.cancel.clone(),
                        ButtonRole::Cancel,
                        DemoMessage::Theme(ThemeMessage::DeleteAnswer(false)),
                    ),
                )
            }
            ThemeMessage::DeleteAnswer(true) => {
                delete_chosen(settings);
                self.chosen_changed(ui, texts, pairs, settings, list);
                ThemeOutcome::Theme
            }
            ThemeMessage::DeleteAnswer(false) => ThemeOutcome::None,
            ThemeMessage::Rename(name) => {
                if rename_chosen(settings, &name) {
                    self.refresh_themes(ui, pairs, settings);
                    // The field shows the name kept (made unique, trimmed).
                    if let (Some(field), Some(custom)) = (
                        self.name,
                        chosen_custom(settings).and_then(|i| settings.theme.custom.get(i)),
                    ) && let Some(widget) = ui.widget_mut::<TextField<DemoMessage>>(field)
                        && widget.text() != custom.name
                    {
                        widget.set_text(&custom.name);
                        ui.request_layout(field);
                    }
                }
                ThemeOutcome::None
            }
            ThemeMessage::Color(index, text) => {
                let (Some(&token), Some(color)) = (ColorToken::ALL.get(index), parse_color(&text))
                else {
                    return ThemeOutcome::None;
                };
                set_color(settings, self.half, token, Some(color));
                self.refresh_marks(ui, texts, settings, list);
                ThemeOutcome::Theme
            }
            ThemeMessage::Restore(index) => {
                let Some(&token) = ColorToken::ALL.get(index) else {
                    return ThemeOutcome::None;
                };
                set_color(settings, self.half, token, None);
                // The field shows the base's value again.
                let value = color_text(instance(settings, self.half).color(token));
                if let Some(row) = self.rows.get(index)
                    && let Some(widget) = ui.widget_mut::<TextField<DemoMessage>>(row.field)
                {
                    widget.set_text(&value);
                    ui.request_layout(row.field);
                }
                self.refresh_marks(ui, texts, settings, list);
                ThemeOutcome::Theme
            }
        }
    }

    /// The window now shows `half` (the system switched, or the user chose another mode): the
    /// fields, the note, the marks and the contrast lines follow, without rebuilding anything
    /// (the focus stays where it is).
    pub fn show_half(
        &mut self,
        ui: &mut Ui<DemoMessage>,
        texts: &ThemeTexts,
        settings: &Settings,
        half: Appearance,
        list: &mut DisplayList,
    ) {
        if half == self.half {
            return;
        }
        self.half = half;
        let theme = instance(settings, half);
        for (row, token) in self.rows.iter().zip(ColorToken::ALL) {
            if let Some(widget) = ui.widget_mut::<TextField<DemoMessage>>(row.field) {
                widget.set_text(&color_text(theme.color(*token)));
            }
            ui.request_layout(row.field);
        }
        if let Some(note) = self.half_note {
            set_label(ui, note, half_note(texts, half));
        }
        self.refresh_marks(ui, texts, settings, list);
    }

    /// Another theme is chosen (the drop-down, *Duplicate*, *Delete*): the drop-down's values and
    /// the area follow.
    pub fn chosen_changed(
        &mut self,
        ui: &mut Ui<DemoMessage>,
        texts: &ThemeTexts,
        pairs: &[String],
        settings: &Settings,
        list: &mut DisplayList,
    ) {
        self.refresh_themes(ui, pairs, settings);
        // Fails only if the interface's own tree is inconsistent: the area then keeps its rows.
        let _ = self.rebuild(ui, texts, settings, list);
    }

    /// The drop-down's values and choice from `settings`.
    fn refresh_themes(&self, ui: &mut Ui<DemoMessage>, pairs: &[String], settings: &Settings) {
        let (values, selected) = theme_values(pairs, settings);
        if let Some(widget) = ui.widget_mut::<DropDown<DemoMessage>>(self.themes) {
            widget.set_values(values, selected);
        }
        ui.request_layout(self.themes);
    }

    /// Builds the area again for `settings`' theme (see the module documentation).
    fn rebuild(
        &mut self,
        ui: &mut Ui<DemoMessage>,
        texts: &ThemeTexts,
        settings: &Settings,
        list: &mut DisplayList,
    ) -> Result<(), UiError> {
        for child in ui.children(self.area) {
            ui.remove(child, list)?;
        }
        self.name = None;
        self.half_note = None;
        self.rows.clear();
        self.readability = None;
        let custom = chosen_custom(settings).and_then(|index| settings.theme.custom.get(index));
        if let Some(widget) = ui.widget_mut::<Button<DemoMessage>>(self.delete) {
            widget.set_enabled(custom.is_some());
        }
        ui.request_record(self.delete);
        let wide = LayoutProps::default().width(Length::Fill(1.0));
        let muted = |text: &str| {
            Label::new(text.to_owned())
                .color(ColorToken::TextMuted)
                .wrapping()
        };
        let Some(custom) = custom else {
            ui.add(self.area, muted(&texts.builtin), wide)?;
            ui.frame(list);
            return Ok(());
        };
        // The name, then readability (at the top, owner's request of 2026-11-26), then the
        // colours.
        let row = ui.add(self.area, Flex::row(Space::M, Space::None), wide)?;
        ui.add(
            row,
            Label::new(texts.name.clone()),
            LayoutProps::default().align(orchestra_ui::Align::Start, orchestra_ui::Align::Center),
        )?;
        self.name = Some(ui.add(
            row,
            TextField::new(&custom.name, rename_message).labelled(texts.name.clone()),
            wide,
        )?);
        ui.add(
            self.area,
            Label::new(texts.readability.clone()).styled(
                FamilyToken::Ui,
                SizeToken::Body,
                Weight::SemiBold,
            ),
            LayoutProps::default(),
        )?;
        self.readability = Some(ui.add(self.area, Flex::column(Space::Xs, Space::None), wide)?);
        ui.add(
            self.area,
            Label::new(texts.colors.clone()).styled(
                FamilyToken::Ui,
                SizeToken::Body,
                Weight::SemiBold,
            ),
            LayoutProps::default(),
        )?;
        self.half_note = Some(ui.add(self.area, muted(half_note(texts, self.half)), wide)?);
        self.build_rows(ui, texts, &instance(settings, self.half))?;
        self.refresh_marks(ui, texts, settings, list);
        Ok(())
    }

    /// Adds to the area the grid of the colour rows, one per token of `ColorToken::ALL`, showing
    /// `theme`'s values (the half on screen), and records them in `self.rows`.
    ///
    /// # Layout
    /// Five columns: the warning mark (empty, or "●" when the token is part of a pair below its
    /// contrast threshold, set by [`Self::refresh_marks`]), the token's name, its swatch (drawn
    /// with the live token, so it follows every change without being told), the value field
    /// (the only column that grows), *Restore*. One row per token, as tall as its content.
    ///
    /// # Why separate from [`Self::rebuild`]?
    /// The rows are most of the area's widgets; split out, `rebuild` stays within the length the
    /// lints allow and each part reads on its own. O(tokens) widgets added.
    ///
    /// # Errors
    /// The interface's, if its own tree is inconsistent (never expected).
    fn build_rows(
        &mut self,
        ui: &mut Ui<DemoMessage>,
        texts: &ThemeTexts,
        theme: &Theme,
    ) -> Result<(), UiError> {
        let wide = LayoutProps::default().width(Length::Fill(1.0));
        let count = u16::try_from(ColorToken::ALL.len()).unwrap_or(u16::MAX);
        let grid = ui.add(
            self.area,
            Grid::new(
                vec![
                    Track::Content,
                    Track::Content,
                    Track::Content,
                    Track::Fill(1.0),
                    Track::Content,
                ],
                vec![Track::Content; usize::from(count)],
                Space::S,
                Space::None,
            ),
            wide,
        )?;
        let center = |row: u16, column: u16| {
            LayoutProps::default()
                .cell(row, column)
                .align(orchestra_ui::Align::Start, orchestra_ui::Align::Center)
        };
        for ((r, token), name) in (0_u16..).zip(ColorToken::ALL).zip(&texts.color_names) {
            let index = usize::from(r);
            let mark = ui.add(
                grid,
                Label::new(String::new()).color(ColorToken::Warning),
                center(r, 0),
            )?;
            ui.add(grid, Label::new(name.clone()), center(r, 1))?;
            ui.add(grid, Swatch::new(*token), center(r, 2))?;
            let message = COLOR_MESSAGES
                .get(index)
                .copied()
                .unwrap_or(color_message::<0>);
            let field = ui.add(
                grid,
                TextField::new(&color_text(theme.color(*token)), message)
                    .on_change(message)
                    .labelled(texts.value_name.fill(&[name])),
                center(r, 3).width(Length::Fill(1.0)),
            )?;
            let restore = ui.add(
                grid,
                Button::new(
                    texts.restore.clone(),
                    DemoMessage::Theme(ThemeMessage::Restore(index)),
                ),
                center(r, 4),
            )?;
            self.rows.push(Row {
                mark,
                field,
                restore,
            });
        }
        Ok(())
    }

    /// The restore buttons' states, the warning marks and the readability sentences for the half
    /// shown.
    fn refresh_marks(
        &self,
        ui: &mut Ui<DemoMessage>,
        texts: &ThemeTexts,
        settings: &Settings,
        list: &mut DisplayList,
    ) {
        let Some(custom) =
            chosen_custom(settings).and_then(|index| settings.theme.custom.get(index))
        else {
            return;
        };
        let theme = instance(settings, self.half);
        let base = *BuiltInTheme::from_id(&custom.base)
            .unwrap_or_default()
            .pair()
            .instance(self.half);
        let failing: Vec<ColorToken> = theme
            .contrast_failures()
            .flat_map(|failure| [failure.pair.foreground, failure.pair.background])
            .collect();
        for (row, token) in self.rows.iter().zip(ColorToken::ALL) {
            if let Some(widget) = ui.widget_mut::<Button<DemoMessage>>(row.restore) {
                widget.set_enabled(theme.color(*token) != base.color(*token));
            }
            ui.request_record(row.restore);
            // A filled circle: a symbol, the same in every language (the readability sentences at
            // the top say why).
            let mark = if failing.contains(token) {
                "\u{25cf}"
            } else {
                ""
            };
            set_label(ui, row.mark, mark);
        }
        if let Some(column) = self.readability {
            // Rebuilt whole: at most eleven short rows, once per edit. Removing or adding fails
            // only if the interface's own tree is inconsistent; the column then shows less.
            for child in ui.children(column) {
                let _ = ui.remove(child, list);
            }
            let issues = readability_issues(&theme);
            let wide = LayoutProps::default().width(Length::Fill(1.0));
            if issues.is_empty() {
                let _ = ui.add(
                    column,
                    Label::new(texts.readability_ok.clone())
                        .color(ColorToken::TextMuted)
                        .wrapping(),
                    wide,
                );
            }
            for issue in &issues {
                // The same dot as on the colours' rows, then the sentence, which wraps.
                let Ok(row) = ui.add(column, Flex::row(Space::S, Space::None), wide) else {
                    continue;
                };
                let _ = ui.add(
                    row,
                    Label::new("\u{25cf}".to_owned()).color(ColorToken::Warning),
                    LayoutProps::default(),
                );
                let _ = ui.add(row, Label::new(issue_line(texts, issue)).wrapping(), wide);
            }
        }
        ui.frame(list);
    }
}

/// The note of the half shown.
fn half_note(texts: &ThemeTexts, half: Appearance) -> &str {
    match half {
        Appearance::Light => &texts.editing_light,
        Appearance::Dark => &texts.editing_dark,
    }
}

/// Gives the label `id` the text `text`, laying it out again only when it changed.
fn set_label(ui: &mut Ui<DemoMessage>, id: WidgetId, text: &str) {
    let changed = ui.widget_mut::<Label>(id).is_some_and(|label| {
        if label.text() == text {
            false
        } else {
            label.set_text(text);
            true
        }
    });
    if changed {
        ui.request_layout(id);
    }
}

#[cfg(test)]
mod tests {
    //! The pure parts: colours read and written, ratios, names.
    use super::*;

    /// `#rrggbb` and `#rrggbbaa`, with or without `#`, either case; nothing else.
    #[test]
    fn colors_are_read_and_written() {
        assert_eq!(parse_color("#3366CC"), Some(Color::rgba(0x3366_ccff)));
        assert_eq!(parse_color(" 3366cc80 "), Some(Color::rgba(0x3366_cc80)));
        for wrong in ["", "#", "#336", "#3366c", "#3366ccx", "#+366cc", "red"] {
            assert_eq!(parse_color(wrong), None, "{wrong}");
        }
        assert_eq!(color_text(Color::rgba(0x3366_ccff)), "#3366cc");
        assert_eq!(color_text(Color::rgba(0x3366_cc80)), "#3366cc80");
    }

    /// The owner's thresholds: below 2:1 almost invisible, below 3:1 hard, then weak.
    #[test]
    fn severity_follows_the_thresholds() {
        assert_eq!(Severity::of(1.0), Severity::Invisible);
        assert_eq!(Severity::of(1.99), Severity::Invisible);
        assert_eq!(Severity::of(2.0), Severity::Hard);
        assert_eq!(Severity::of(2.99), Severity::Hard);
        assert_eq!(Severity::of(3.0), Severity::Weak);
        assert_eq!(Severity::of(4.49), Severity::Weak);
    }

    /// The en-US texts (the catalogues' parity and the six languages are tested elsewhere).
    fn texts() -> ThemeTexts {
        ThemeTexts::new(&Localizer::new(orchestra_i18n::Locale::EnUs).unwrap()).unwrap()
    }

    /// The ten built-in pairs' names as the drop-down shows them: here simply their identifiers,
    /// recognisable in the assertions.
    fn pairs() -> Vec<String> {
        BuiltInTheme::ALL
            .iter()
            .map(|pair| pair.id().to_owned())
            .collect()
    }

    /// The colour token `name` of the chosen custom theme's `half`, as the overrides store it, or
    /// `None` when the theme keeps the base's value.
    fn override_of(settings: &Settings, half: Appearance, token: ColorToken) -> Option<u32> {
        let custom = &settings.theme.custom[chosen_custom(settings)?];
        let overrides = match half {
            Appearance::Light => &custom.light,
            Appearance::Dark => &custom.dark,
        };
        overrides.iter().find_map(|item| match item {
            ThemeOverride::Color { token: name, rgba } if name == token.name() => Some(*rgba),
            _ => None,
        })
    }

    /// A free name is kept; a taken one gets the first free number from 2.
    #[test]
    fn names_are_made_unique() {
        let mut settings = Settings::default();
        assert_eq!(unique_name(&settings, "Mine"), "Mine");
        for name in ["Mine", "Mine (2)"] {
            settings.theme.custom.push(FileCustom {
                name: name.to_owned(),
                base: "studio".to_owned(),
                light: Vec::new(),
                dark: Vec::new(),
            });
        }
        assert_eq!(unique_name(&settings, "Mine"), "Mine (3)");
        assert_eq!(unique_name(&settings, "Mine (2)"), "Mine (2) (2)");
    }

    /// Duplicating a built-in pair makes an empty custom theme on that base, named after the
    /// pair and chosen; duplicating it again gets a numbered name; duplicating a custom theme
    /// copies its overrides. Choosing by index goes back and forth between the two kinds.
    #[test]
    fn duplicate_choose_rename_and_delete() {
        let (texts, pairs) = (texts(), pairs());
        let mut settings = Settings::default();
        choose(&mut settings, 1);
        let base = BuiltInTheme::ALL[1];
        duplicate(&mut settings, &texts, &pairs);
        let first = format!("{} (copy)", base.id());
        assert_eq!(settings.theme.choice, ThemeChoice::Custom(first.clone()));
        assert_eq!(settings.theme.custom[0].base, base.id());
        assert_eq!(settings.theme.custom[0].light.len(), 0);
        set_color(
            &mut settings,
            Appearance::Dark,
            ColorToken::Accent,
            Some(Color::rgba(0x1234_56ff)),
        );
        // A custom theme duplicated: its overrides come along.
        duplicate(&mut settings, &texts, &pairs);
        let second = format!("{first} (copy)");
        assert_eq!(settings.theme.choice, ThemeChoice::Custom(second.clone()));
        assert_eq!(
            override_of(&settings, Appearance::Dark, ColorToken::Accent),
            Some(0x1234_56ff)
        );
        // The first again, by its index in the drop-down (after the ten pairs).
        choose(&mut settings, BuiltInTheme::ALL.len());
        assert_eq!(settings.theme.choice, ThemeChoice::Custom(first.clone()));
        duplicate(&mut settings, &texts, &pairs);
        assert_eq!(
            settings.theme.choice,
            ThemeChoice::Custom(format!("{first} (copy) (2)")),
            "the name is taken by the second"
        );
        // Out of the list: nothing.
        choose(&mut settings, 99);
        assert_eq!(settings.theme.custom.len(), 3);
        // Renaming: trimmed; empty or unchanged does nothing; a taken name is numbered.
        assert!(!rename_chosen(&mut settings, "   "));
        assert!(rename_chosen(&mut settings, "  Night  "));
        assert_eq!(
            settings.theme.choice,
            ThemeChoice::Custom("Night".to_owned())
        );
        assert!(!rename_chosen(&mut settings, "Night"));
        assert!(rename_chosen(&mut settings, &second));
        assert_eq!(
            settings.theme.choice,
            ThemeChoice::Custom(format!("{second} (2)"))
        );
        // Deleting goes back to the base; nothing for a built-in pair.
        delete_chosen(&mut settings);
        assert_eq!(settings.theme.custom.len(), 2);
        assert_eq!(
            settings.theme.choice,
            ThemeChoice::BuiltIn(base.id().to_owned())
        );
        delete_chosen(&mut settings);
        assert_eq!(settings.theme.custom.len(), 2);
        assert!(!rename_chosen(&mut settings, "Other"));
    }

    /// A colour different from the base's is one override of its half; a second value replaces
    /// it; the base's value, or `None`, removes it (T7: only the differences); the other half is
    /// untouched; a built-in pair is never changed.
    #[test]
    fn colors_keep_only_the_differences() {
        let (texts, pairs) = (texts(), pairs());
        let mut settings = Settings::default();
        set_color(
            &mut settings,
            Appearance::Light,
            ColorToken::Accent,
            Some(Color::rgba(0x1234_56ff)),
        );
        assert!(settings.theme.custom.is_empty(), "a built-in pair");
        duplicate(&mut settings, &texts, &pairs);
        let base = instance(&settings, Appearance::Light).color(ColorToken::Accent);
        for rgba in [0x1234_56ff, 0x6543_2180] {
            set_color(
                &mut settings,
                Appearance::Light,
                ColorToken::Accent,
                Some(Color::rgba(rgba)),
            );
            assert_eq!(
                override_of(&settings, Appearance::Light, ColorToken::Accent),
                Some(rgba)
            );
            assert_eq!(
                instance(&settings, Appearance::Light).color(ColorToken::Accent),
                Color::rgba(rgba),
                "the window draws it"
            );
        }
        let custom = &settings.theme.custom[0];
        assert_eq!((custom.light.len(), custom.dark.len()), (1, 0));
        set_color(
            &mut settings,
            Appearance::Light,
            ColorToken::Accent,
            Some(base),
        );
        assert_eq!(
            override_of(&settings, Appearance::Light, ColorToken::Accent),
            None
        );
        set_color(
            &mut settings,
            Appearance::Light,
            ColorToken::Accent,
            Some(Color::rgba(0x1234_56ff)),
        );
        set_color(&mut settings, Appearance::Light, ColorToken::Accent, None);
        assert_eq!(settings.theme.custom[0].light.len(), 0);
        assert_eq!(
            instance(&settings, Appearance::Light).color(ColorToken::Accent),
            base
        );
    }

    /// A custom theme duplicated from Studio with `token` set to `rgba` in `half`.
    fn with_color(half: Appearance, token: ColorToken, rgba: u32) -> Settings {
        let mut settings = Settings::default();
        duplicate(&mut settings, &texts(), &pairs());
        set_color(&mut settings, half, token, Some(Color::rgba(rgba)));
        settings
    }

    /// No built-in pair has a problem (ADR-0037 T5). Secondary text made a light grey on the
    /// light half: one issue (its four backgrounds grouped), almost invisible, to be made darker;
    /// a dark grey on the dark half: to be made lighter.
    #[test]
    fn problems_are_grouped_by_colour_with_their_direction() {
        for pair in BuiltInTheme::ALL {
            for half in [Appearance::Light, Appearance::Dark] {
                assert_eq!(
                    readability_issues(pair.pair().instance(half)),
                    [],
                    "{pair:?} {half:?}"
                );
            }
        }
        let light = with_color(Appearance::Light, ColorToken::TextMuted, 0xc8c8_c8ff);
        let issues = readability_issues(&instance(&light, Appearance::Light));
        assert_eq!(issues.len(), 1, "{issues:?}");
        let issue = issues[0];
        assert_eq!(issue.color, ColorToken::TextMuted);
        assert_eq!(issue.kind, ContrastKind::Text);
        assert_eq!(issue.severity, Severity::Invisible);
        assert_eq!(issue.direction, Direction::Darker);
        assert_eq!(issue.others, 3, "four surfaces in all");
        let dark = with_color(Appearance::Dark, ColorToken::TextMuted, 0x4a4a_48ff);
        let issues = readability_issues(&instance(&dark, Appearance::Dark));
        assert_eq!(issues[0].direction, Direction::Lighter);
        // The half not edited has no problem.
        assert_eq!(readability_issues(&instance(&dark, Appearance::Light)), []);
    }

    /// A grey between 3:1 and 4.5:1 on every light surface is readable but weak; a control
    /// border the colour of the window is a graphic almost invisible; the worst problem comes
    /// first.
    #[test]
    fn weak_text_and_graphics() {
        let weak = with_color(Appearance::Light, ColorToken::TextMuted, 0x7777_77ff);
        let issues = readability_issues(&instance(&weak, Appearance::Light));
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert_eq!(issues[0].severity, Severity::Weak);
        let mut both = weak;
        let surface = instance(&both, Appearance::Light).color(ColorToken::Surface);
        set_color(
            &mut both,
            Appearance::Light,
            ColorToken::ControlBorder,
            Some(surface),
        );
        let issues = readability_issues(&instance(&both, Appearance::Light));
        assert_eq!(issues.len(), 2, "{issues:?}");
        assert_eq!(
            issues[0].color,
            ColorToken::ControlBorder,
            "the worst first"
        );
        assert_eq!(issues[0].kind, ContrastKind::Graphic);
        assert_eq!(issues[0].severity, Severity::Invisible);
        assert_eq!(issues[0].background, ColorToken::Surface);
        assert_eq!(issues[1].color, ColorToken::TextMuted);
    }

    /// Every possible combination has its own sentence; impossible ones have none; a sentence
    /// names both colours.
    #[test]
    fn every_combination_has_a_sentence() {
        let texts = texts();
        let max = max_others();
        assert_eq!(max, 6, "normal text has seven backgrounds");
        let mut seen = Vec::new();
        for (kind, severity, _, _) in KIND_SEVERITY {
            for (direction, _) in DIRECTIONS {
                for others in 0..=max {
                    let index = issue_template_index(kind, severity, direction, others).unwrap();
                    assert!(index < texts.issues.len());
                    seen.push(index);
                }
            }
        }
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), texts.issues.len(), "no index used twice");
        assert_eq!(
            issue_template_index(ContrastKind::Graphic, Severity::Weak, Direction::Darker, 0),
            None
        );
        assert_eq!(
            issue_template_index(
                ContrastKind::Text,
                Severity::Hard,
                Direction::Darker,
                max + 1
            ),
            None
        );
        let line = issue_line(
            &texts,
            &readability_issues(&instance(
                &with_color(Appearance::Light, ColorToken::TextMuted, 0xc8c8_c8ff),
                Appearance::Light,
            ))[0],
        );
        assert!(
            line.contains(texts.color_name(ColorToken::TextMuted)),
            "{line}"
        );
        assert!(!line.contains(':'), "no ratio: {line}");
    }
}
