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

use orchestra_i18n::{I18nError, Localizer};
use std::path::PathBuf;

use orchestra_io::settings::{CustomTheme as FileCustom, Settings, ThemeChoice, ThemeOverride};
use orchestra_io::theme_file;
use orchestra_ui::FileRequest;
use orchestra_ui::{
    Appearance, BuiltInTheme, Button, ButtonRole, Color, ColorPicker, ColorToken, DialogKind,
    DialogSpec, DisplayList, DropDown, FamilyToken, Flex, Label, LayoutProps, Length, SizeToken,
    Space, SwatchButton, TextField, Theme, Ui, UiError, Weight, WidgetId,
};

use crate::demo::DemoMessage;
use crate::settings::convert::theme_from_settings;
use crate::template::{ISOLATION_MARKS, Template};

mod files;
mod readability;
mod shared;

use readability::{ReadabilityTexts, issue_line, readability_issues};
pub use shared::{ChoiceRow, SharedRows, SliderRow};
use shared::{
    EDITABLE_FAMILIES, FAMILY_CHOICES, SHAPE_LIMITS, SharedState, SharedTexts, SharedValue,
    set_shared,
};

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
    /// The text size slider moved: percent of the base's sizes (F2.12b2a), rounded to a whole
    /// number by the slider's message (so messages stay comparable, as the Playback section's).
    TextScale(u16),
    /// The family drop-down number `.0` (in the editable families' order) chose `.1`.
    Family(usize, usize),
    /// The shape slider number `.0` (in the shapes' order) moved, logical pixels (whole).
    Shape(usize, u16),
    /// The density group chose `.0` (in `Density::ALL`'s order).
    Density(usize),
    /// *Restore* of the text size.
    RestoreText,
    /// *Restore* of the family number `.0`.
    RestoreFamily(usize),
    /// *Restore* of the shape number `.0`.
    RestoreShape(usize),
    /// *Restore* of the density.
    RestoreDensity,
    /// *Import…*: the chooser of a theme file to read (F2.12b2b, ADR-0052).
    Import,
    /// *Export…*: the chooser of a theme file to write.
    Export,
    /// The answer to "… already exists. Replace it?" when exporting: `true` for *Replace*.
    ExportReplace(bool),
    /// A report or an error of a theme file was closed.
    Closed,
    /// The swatch of the colour token number `.0` was clicked: open its picker under its row,
    /// or close it if it is the one open (F2.12b3b).
    OpenPicker(usize),
    /// The open picker's colour changed.
    Picked(Color),
    /// The open picker's *Enter* (`true`: keep the colour) or *Escape* (`false`: back to the
    /// colour it opened with); it closes.
    PickerDone(bool),
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
    /// The readability warnings' sentences (`prefs-theme-readability-problem`, `-same`,
    /// `-advice`), formatted once per combination of the values they select on.
    pub(crate) issues: ReadabilityTexts,
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
    /// The texts of the text, lines and density rows (F2.12b2a).
    pub(crate) shared: SharedTexts,
    /// The texts of the theme files (F2.12b2b).
    pub(crate) files: files::FileTexts,
    /// `prefs-theme-choose`, with `$color`: the swatch button's name (F2.12b3b).
    pub choose: Template,
    /// `prefs-theme-picker-square`, `-hue`, `-alpha`: the picker's parts for assistive
    /// technologies.
    pub picker_parts: [String; 3],
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
            issues: ReadabilityTexts::new(localizer)?,
            delete_title: template("prefs-theme-delete-title", &["name"])?,
            delete_message: text("prefs-theme-delete-message")?,
            delete_button: text("dialog-delete")?,
            cancel: text("dialog-cancel")?,
            color_names,
            shared: SharedTexts::new(localizer)?,
            files: files::FileTexts::new(localizer)?,
            choose: template("prefs-theme-choose", &["color"])?,
            picker_parts: [
                text("prefs-theme-picker-square")?,
                text("prefs-theme-picker-hue")?,
                text("prefs-theme-picker-alpha")?,
            ],
        })
    }

    /// Every text with one value in it, each holding its placeholder once (tests). The
    /// readability sentences are in [`ThemeTexts::issues`].
    #[must_use]
    pub fn templates(&self) -> [&Template; 4] {
        [
            &self.copy_name,
            &self.value_name,
            &self.delete_title,
            &self.choose,
        ]
    }

    /// Every readability sentence's template with how many times each of its two placeholders
    /// must appear in it (tests: the six catalogues must keep every colour's name in place).
    #[must_use]
    pub fn readability_templates(&self) -> Vec<(&Template, [usize; 2])> {
        self.issues.expectations()
    }

    /// The texts of the theme files that hold values, with their number of variables (tests).
    #[must_use]
    pub fn file_templates(&self) -> [(&Template, usize); 4] {
        self.files.templates()
    }

    /// The texts of the text, lines and density rows that hold a value, each holding its
    /// placeholder once (tests).
    #[must_use]
    pub fn shared_templates(&self) -> [&Template; 2] {
        self.shared.templates()
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

/// The open colour picker's message (F2.12b3b): one picker is open at a time, so its colour
/// needs no index.
fn picked_message(color: Color) -> DemoMessage {
    DemoMessage::Theme(ThemeMessage::Picked(color))
}

/// The open colour picker's *Enter* and *Escape*.
fn picker_done_message(keep: bool) -> DemoMessage {
    DemoMessage::Theme(ThemeMessage::PickerDone(keep))
}

/// The width of the warning mark's place in a colour's row, logical pixels: the same in every row,
/// marked or not, so the names start on one line (an empty label measures nothing).
const MARK_WIDTH: f32 = 18.0;

/// The colour picker's width, logical pixels (it asks for 240; fixed, so the square keeps its
/// proportions whatever the section's width).
const PICKER_WIDTH: f32 = 240.0;

/// The colour picker open under a row (F2.12b3b).
#[derive(Debug, Clone, Copy)]
struct OpenPicker {
    /// The row's colour token number, in `ColorToken::ALL`'s order.
    index: usize,
    /// The colour when it opened: what *Escape* gives back.
    original: Color,
    /// The picker.
    widget: WidgetId,
}

/// The message of the name field.
fn rename_message(text: &str) -> DemoMessage {
    DemoMessage::Theme(ThemeMessage::Rename(text.to_owned()))
}

/// One colour's row.
#[derive(Debug, Clone, Copy)]
struct Row {
    /// The row's block: the row, and the colour picker under it while open (F2.12b3b).
    block: WidgetId,
    /// The warning mark (a "●" in the warning colour, or nothing).
    mark: WidgetId,
    /// The swatch, a button that opens the picker (F2.12b3b).
    swatch: WidgetId,
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
    /// The theme changed (an import, F2.12b2b): apply it, then show this report.
    ThemeAndReport(DialogSpec<DemoMessage>),
    /// Open the system's file chooser with this request (F2.12b2b); its answer goes back to
    /// [`ThemeEditor::file_chosen`].
    Choose(FileRequest),
}

/// Which file chooser the editor is waiting for (F2.12b2b).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Choosing {
    /// A theme file to import.
    Import,
    /// A file to export the chosen theme to.
    Export,
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
    /// *Import…* (F2.12b2b; always enabled).
    import: WidgetId,
    /// *Export…* (enabled for a custom theme: a built-in pair has nothing of the user's, TF2).
    export: WidgetId,
    /// The chooser the editor is waiting for.
    choosing: Option<Choosing>,
    /// The file to export to once its replacement is confirmed.
    pending_export: Option<PathBuf>,
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
    /// The text, lines and density rows (a custom theme, F2.12b2a).
    shared: Option<SharedRows>,
    /// The colour picker open under a row, if any (F2.12b3b).
    picker: Option<OpenPicker>,
    /// The scroll area the editor is in, if any: the open picker is kept still in it while the
    /// content above it changes height ([`ThemeEditor::keep_picker_still`]).
    scroll: Option<WidgetId>,
    /// The colour the user changed last since this theme and half were shown: the readability
    /// warnings name it as the colour to fix (`readability`'s "culprit"). Forgotten when another
    /// theme or half is shown, and when the colour is restored.
    last_edited: Option<ColorToken>,
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
        // Import and export beside them (ADR-0052 TF6).
        let import = ui.add(
            buttons,
            Button::new(
                texts.files.import.clone(),
                DemoMessage::Theme(ThemeMessage::Import),
            ),
            LayoutProps::default(),
        )?;
        let export = ui.add(
            buttons,
            Button::new(
                texts.files.export.clone(),
                DemoMessage::Theme(ThemeMessage::Export),
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
            import,
            export,
            choosing: None,
            pending_export: None,
            area,
            name: None,
            half_note: None,
            rows: Vec::new(),
            readability: None,
            shared: None,
            picker: None,
            scroll: None,
            half,
            last_edited: None,
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
            ThemeMessage::DeleteAnswer(false) | ThemeMessage::Closed => ThemeOutcome::None,
            message @ (ThemeMessage::Import
            | ThemeMessage::Export
            | ThemeMessage::ExportReplace(_)) => self.handle_files(texts, settings, &message),
            ThemeMessage::OpenPicker(index) => self.toggle_picker(ui, texts, settings, index, list),
            ThemeMessage::Picked(color) => self.picked(ui, texts, settings, color, list),
            ThemeMessage::PickerDone(keep) => self.close_picker(ui, texts, settings, keep, list),
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
                self.last_edited = Some(token);
                // The open picker follows a code typed in its row's field (choice 3).
                self.follow_in_picker(ui, index, color);
                self.refresh_marks(ui, texts, settings, list);
                ThemeOutcome::Theme
            }
            ThemeMessage::Restore(index) => {
                let Some(&token) = ColorToken::ALL.get(index) else {
                    return ThemeOutcome::None;
                };
                set_color(settings, self.half, token, None);
                // Back to its base value: no longer a change to blame.
                if self.last_edited == Some(token) {
                    self.last_edited = None;
                }
                // The field, and the open picker of this row, show the base's value again.
                let value = instance(settings, self.half).color(token);
                self.show_field(ui, index, value);
                self.follow_in_picker(ui, index, value);
                self.refresh_marks(ui, texts, settings, list);
                ThemeOutcome::Theme
            }
            message @ (ThemeMessage::TextScale(_)
            | ThemeMessage::Family(..)
            | ThemeMessage::Shape(..)
            | ThemeMessage::Density(_)
            | ThemeMessage::RestoreText
            | ThemeMessage::RestoreFamily(_)
            | ThemeMessage::RestoreShape(_)
            | ThemeMessage::RestoreDensity) => self.handle_shared(ui, texts, settings, &message),
        }
    }

    /// A message of *Import…* and *Export…* (F2.12b2b, ADR-0052): the chooser asked for, or the
    /// answer to the question about replacing a file.
    fn handle_files(
        &mut self,
        texts: &ThemeTexts,
        settings: &Settings,
        message: &ThemeMessage,
    ) -> ThemeOutcome {
        match *message {
            ThemeMessage::Import => {
                self.choosing = Some(Choosing::Import);
                ThemeOutcome::Choose(FileRequest::open(
                    texts.files.import_title.clone(),
                    texts.files.filter.clone(),
                    theme_file::EXTENSION,
                ))
            }
            ThemeMessage::Export => {
                let Some(custom) =
                    chosen_custom(settings).and_then(|index| settings.theme.custom.get(index))
                else {
                    return ThemeOutcome::None;
                };
                self.choosing = Some(Choosing::Export);
                ThemeOutcome::Choose(FileRequest::save(
                    texts.files.export_title.clone(),
                    texts.files.filter.clone(),
                    theme_file::EXTENSION,
                    format!("{}.{}", custom.name, theme_file::EXTENSION),
                ))
            }
            ThemeMessage::ExportReplace(replace) => match self.pending_export.take() {
                Some(path) if replace => Self::export(texts, settings, &path),
                _ => ThemeOutcome::None,
            },
            _ => ThemeOutcome::None,
        }
    }

    /// The swatch of row `index` was clicked (F2.12b3b): the picker opens under it (closing
    /// another one, keeping its colour), or closes if it is this row's (keeping the colour). The
    /// picker shows the half on screen, the transparency strip only for a colour transparent in
    /// the theme's base (choice 4), and takes the focus.
    fn toggle_picker(
        &mut self,
        ui: &mut Ui<DemoMessage>,
        texts: &ThemeTexts,
        settings: &mut Settings,
        index: usize,
        list: &mut DisplayList,
    ) -> ThemeOutcome {
        let same = self.picker.is_some_and(|open| open.index == index);
        let outcome = self.close_picker(ui, texts, settings, true, list);
        if same {
            return outcome;
        }
        let (Some(&token), Some(row), Some(name)) = (
            ColorToken::ALL.get(index),
            self.rows.get(index).copied(),
            texts.color_names.get(index),
        ) else {
            return outcome;
        };
        let current = instance(settings, self.half).color(token);
        let base = chosen_custom(settings)
            .and_then(|i| settings.theme.custom.get(i))
            .map_or(current, |custom| {
                BuiltInTheme::from_id(&custom.base)
                    .unwrap_or_default()
                    .pair()
                    .instance(self.half)
                    .color(token)
            });
        let [square, hue, alpha] = &texts.picker_parts;
        let picker = ColorPicker::new(current, picked_message)
            .with_alpha(base.a < u8::MAX)
            .labelled(name, square, hue, alpha)
            .on_done(picker_done_message);
        // Fails only if the interface's own tree is inconsistent: no picker opens then.
        if let Ok(widget) = ui.add(
            row.block,
            picker,
            LayoutProps::default().width(Length::Fixed(PICKER_WIDTH)),
        ) {
            self.picker = Some(OpenPicker {
                index,
                original: current,
                widget,
            });
            // Laid out first, then focused: the focus brings the picker into view (WF5), which
            // needs its rectangle (focused before its layout, it stayed below the visible part
            // when its row was low in the section, as the screenshot showed).
            ui.frame(list);
            let _ = ui.set_focus(Some(widget), list);
        }
        ui.frame(list);
        outcome
    }

    /// The open picker's colour changed (F2.12b3b): the colour is the theme's at once (the whole
    /// window is the preview, choice 6), its field shows the code, and the readability follows.
    fn picked(
        &mut self,
        ui: &mut Ui<DemoMessage>,
        texts: &ThemeTexts,
        settings: &mut Settings,
        color: Color,
        list: &mut DisplayList,
    ) -> ThemeOutcome {
        let Some(open) = self.picker else {
            return ThemeOutcome::None;
        };
        let Some(&token) = ColorToken::ALL.get(open.index) else {
            return ThemeOutcome::None;
        };
        set_color(settings, self.half, token, Some(color));
        self.last_edited = Some(token);
        self.show_field(ui, open.index, color);
        self.refresh_marks(ui, texts, settings, list);
        ThemeOutcome::Theme
    }

    /// Closes the open picker, if any (F2.12b3b): `keep` leaves the colour as it is (Enter, a new
    /// click on the swatch, another swatch); otherwise the colour goes back to the one it opened
    /// with (Escape). The focus goes back to the row's swatch.
    fn close_picker(
        &mut self,
        ui: &mut Ui<DemoMessage>,
        texts: &ThemeTexts,
        settings: &mut Settings,
        keep: bool,
        list: &mut DisplayList,
    ) -> ThemeOutcome {
        let Some(open) = self.picker.take() else {
            return ThemeOutcome::None;
        };
        let mut outcome = ThemeOutcome::None;
        if !keep && let Some(&token) = ColorToken::ALL.get(open.index) {
            set_color(settings, self.half, token, Some(open.original));
            self.show_field(ui, open.index, open.original);
            self.refresh_marks(ui, texts, settings, list);
            outcome = ThemeOutcome::Theme;
        }
        // Fails only if the interface's own tree is inconsistent: the picker then stays drawn.
        let _ = ui.remove(open.widget, list);
        if let Some(row) = self.rows.get(open.index) {
            let _ = ui.set_focus(Some(row.swatch), list);
        }
        ui.frame(list);
        outcome
    }

    /// Writes `color`'s code in the field of row `index`, as the field shows it.
    fn show_field(&self, ui: &mut Ui<DemoMessage>, index: usize, color: Color) {
        if let Some(row) = self.rows.get(index)
            && let Some(widget) = ui.widget_mut::<TextField<DemoMessage>>(row.field)
        {
            let text = color_text(color);
            if widget.text() != text {
                widget.set_text(&text);
                ui.request_layout(row.field);
            }
        }
    }

    /// The open picker follows `color`, set elsewhere (typed in the field, restored), when it is
    /// row `index`'s (F2.12b3b, choice 3).
    fn follow_in_picker(&self, ui: &mut Ui<DemoMessage>, index: usize, color: Color) {
        if let Some(open) = self.picker.filter(|open| open.index == index)
            && let Some(picker) = ui.widget_mut::<ColorPicker<DemoMessage>>(open.widget)
        {
            picker.set_color(color);
            ui.request_record(open.widget);
        }
    }

    /// The scroll area the editor is in (the Appearance section's), so the open colour picker can
    /// be kept still while the readability warnings above it come and go (F2.12b3b).
    pub const fn set_scroll_area(&mut self, scroll: Option<WidgetId>) {
        self.scroll = scroll;
    }

    /// Keeps the open picker where it was in the window, `before` being its top edge before a
    /// change was laid out (F2.12b3b, the owner's report of 2026-11-26).
    ///
    /// # Why
    /// The readability warnings sit at the top of the section, above the picker. A colour that
    /// makes a pair hard to read adds a sentence, one that fixes it removes one: everything below
    /// moves by a line while the scroll offset stays, so the picker, and the colour under the
    /// pointer, ran away during a drag. As browsers' scroll anchoring does, the offset moves by
    /// the same amount, so the picker stays put. Clamped by the area at its next layout, so it
    /// never scrolls past the content.
    fn keep_picker_still(
        &self,
        ui: &mut Ui<DemoMessage>,
        before: Option<f32>,
        list: &mut DisplayList,
    ) {
        let (Some(before), Some(open), Some(scroll)) = (before, self.picker, self.scroll) else {
            return;
        };
        let Some(after) = ui.rect(open.widget).map(|rect| rect.y0) else {
            return;
        };
        let moved = after - before;
        if moved.abs() < 0.01 {
            return;
        }
        if let Some(area) = ui.widget_mut::<orchestra_ui::ScrollArea>(scroll) {
            let offset = area.offset();
            area.set_offset(orchestra_ui::Point::new(offset.x, offset.y + moved));
            ui.request_layout(scroll);
            ui.frame(list);
        }
    }

    /// The colour picker open now, if any (tests).
    #[must_use]
    pub fn open_picker(&self) -> Option<WidgetId> {
        self.picker.map(|open| open.widget)
    }

    /// The swatch of colour token number `index` (tests).
    #[must_use]
    pub fn swatch(&self, index: usize) -> Option<WidgetId> {
        self.rows.get(index).map(|row| row.swatch)
    }

    /// The file chooser answered (F2.12b2b): `path`, or `None` when cancelled. An import reads
    /// the file (an error says why and changes nothing, TF5), adds its theme and chooses it (TF3),
    /// then reports the entries not used if there are some (TF4); an export adds the extension
    /// when the typed name lacks it and asks before replacing a file the chooser did not confirm
    /// (TF2).
    pub fn file_chosen(
        &mut self,
        ui: &mut Ui<DemoMessage>,
        texts: &ThemeTexts,
        pairs: &[String],
        settings: &mut Settings,
        path: Option<PathBuf>,
        list: &mut DisplayList,
    ) -> ThemeOutcome {
        let (Some(choosing), Some(path)) = (self.choosing.take(), path) else {
            return ThemeOutcome::None;
        };
        match choosing {
            Choosing::Import => {
                let name = crate::project::file_name(&path);
                let file = match theme_file::load(&path) {
                    Ok(file) => file,
                    Err(error) => {
                        return ThemeOutcome::Ask(files::error_dialog(
                            &texts.files,
                            texts.files.import_error.fill(&[&name]),
                            &error,
                            &path,
                            false,
                        ));
                    }
                };
                let stem = path
                    .file_stem()
                    .map(|stem| stem.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let unused = files::import(settings, file, &stem);
                self.chosen_changed(ui, texts, pairs, settings, list);
                files::report(&texts.files, &name, unused)
                    .map_or(ThemeOutcome::Theme, ThemeOutcome::ThemeAndReport)
            }
            Choosing::Export => {
                let target =
                    crate::project::with_extension_added(path.clone(), theme_file::EXTENSION);
                if target != path && target.try_exists().unwrap_or(false) {
                    let spec = files::replace_question(&texts.files, &texts.cancel, &target);
                    self.pending_export = Some(target);
                    ThemeOutcome::Ask(spec)
                } else {
                    Self::export(texts, settings, &target)
                }
            }
        }
    }

    /// Writes the chosen custom theme to `path` (TF2); an error says why, the file there intact.
    fn export(texts: &ThemeTexts, settings: &Settings, path: &std::path::Path) -> ThemeOutcome {
        let Some(custom) =
            chosen_custom(settings).and_then(|index| settings.theme.custom.get(index))
        else {
            return ThemeOutcome::None;
        };
        match theme_file::save(custom, path) {
            Ok(()) => ThemeOutcome::None,
            Err(error) => ThemeOutcome::Ask(files::error_dialog(
                &texts.files,
                texts
                    .files
                    .export_error
                    .fill(&[&crate::project::file_name(path)]),
                &error,
                path,
                true,
            )),
        }
    }

    /// *Import…* (tests).
    #[must_use]
    pub const fn import_button(&self) -> WidgetId {
        self.import
    }

    /// *Export…* (tests).
    #[must_use]
    pub const fn export_button(&self) -> WidgetId {
        self.export
    }

    /// A message of the text, lines and density rows (F2.12b2a): the value it sets (`None` for
    /// *Restore*, which also moves the control back), applied by [`Self::change_shared`]. An
    /// index out of its list (never sent) changes nothing.
    fn handle_shared(
        &mut self,
        ui: &mut Ui<DemoMessage>,
        texts: &ThemeTexts,
        settings: &mut Settings,
        message: &ThemeMessage,
    ) -> ThemeOutcome {
        match *message {
            ThemeMessage::TextScale(value) => {
                let value = SharedValue::TextScale(Some(value));
                self.change_shared(ui, texts, settings, value, false)
            }
            ThemeMessage::Family(row, choice) => {
                let (Some(&token), Some(&family)) =
                    (EDITABLE_FAMILIES.get(row), FAMILY_CHOICES.get(choice))
                else {
                    return ThemeOutcome::None;
                };
                self.change_shared(
                    ui,
                    texts,
                    settings,
                    SharedValue::Family(token, Some(family)),
                    false,
                )
            }
            ThemeMessage::Shape(row, value) => {
                let Some(&(token, _, _)) = SHAPE_LIMITS.get(row) else {
                    return ThemeOutcome::None;
                };
                let value = u8::try_from(value).unwrap_or(u8::MAX);
                self.change_shared(
                    ui,
                    texts,
                    settings,
                    SharedValue::Shape(token, Some(value)),
                    false,
                )
            }
            ThemeMessage::Density(choice) => {
                let Some(&density) = orchestra_ui::Density::ALL.get(choice) else {
                    return ThemeOutcome::None;
                };
                self.change_shared(
                    ui,
                    texts,
                    settings,
                    SharedValue::Density(Some(density)),
                    false,
                )
            }
            ThemeMessage::RestoreText => {
                self.change_shared(ui, texts, settings, SharedValue::TextScale(None), true)
            }
            ThemeMessage::RestoreFamily(row) => {
                let Some(&token) = EDITABLE_FAMILIES.get(row) else {
                    return ThemeOutcome::None;
                };
                self.change_shared(ui, texts, settings, SharedValue::Family(token, None), true)
            }
            ThemeMessage::RestoreShape(row) => {
                let Some(&(token, _, _)) = SHAPE_LIMITS.get(row) else {
                    return ThemeOutcome::None;
                };
                self.change_shared(ui, texts, settings, SharedValue::Shape(token, None), true)
            }
            ThemeMessage::RestoreDensity => {
                self.change_shared(ui, texts, settings, SharedValue::Density(None), true)
            }
            _ => ThemeOutcome::None,
        }
    }

    /// Sets a shared value (text, lines, density; F2.12b2a) in both halves of the theme and shows
    /// it in the rows; `controls` also moves the controls (after *Restore*). The theme changed, so
    /// the demonstration applies it: a new text size lays the whole window out again (ADR-0044
    /// TX9, done by `Ui::set_theme`).
    fn change_shared(
        &mut self,
        ui: &mut Ui<DemoMessage>,
        texts: &ThemeTexts,
        settings: &mut Settings,
        value: SharedValue,
        controls: bool,
    ) -> ThemeOutcome {
        set_shared(settings, value);
        if let (Some(rows), Some(state)) = (self.shared.as_ref(), SharedState::of(settings)) {
            shared::refresh(ui, rows, &texts.shared, &state, controls);
        }
        ThemeOutcome::Theme
    }

    /// The text size, family, shape and density rows, for a custom theme (tests).
    #[must_use]
    pub const fn shared_rows(&self) -> Option<&SharedRows> {
        self.shared.as_ref()
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
        self.last_edited = None;
        // A picker showing the other half closes, its colour kept (it was applied as it moved).
        if let Some(open) = self.picker.take() {
            let _ = ui.remove(open.widget, list);
        }
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
        self.shared = None;
        // Its block goes with the rows: the picker is removed with them.
        self.picker = None;
        self.last_edited = None;
        let custom = chosen_custom(settings).and_then(|index| settings.theme.custom.get(index));
        for button in [self.delete, self.export] {
            if let Some(widget) = ui.widget_mut::<Button<DemoMessage>>(button) {
                widget.set_enabled(custom.is_some());
            }
            ui.request_record(button);
        }
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
        // Text, lines and density before the 24 colours, so they are reached without scrolling
        // past them (owner's choice of 2026-11-26).
        if let Some(state) = SharedState::of(settings) {
            let rows = shared::build(ui, self.area, &texts.shared, &texts.restore, &state)?;
            shared::refresh(ui, &rows, &texts.shared, &state, false);
            self.shared = Some(rows);
        }
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
    /// contrast threshold, set by [`Self::refresh_marks`]), the token's name (sharing the room
    /// with the field, wrapping when long), its swatch (drawn with the live token, so it follows
    /// every change without being told), the value field, *Restore*. One row per token, as tall
    /// as its content.
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
        let centred = |props: LayoutProps| {
            props.align(orchestra_ui::Align::Start, orchestra_ui::Align::Center)
        };
        // One block per colour (F2.12b3b): its row, and below it the colour picker while it is
        // open (choice B: inside the list, under its row). The rows are not a grid any more, so
        // that a picker can sit between two of them; they still line up, every row having the
        // same parts with the same widths (the mark fixed, the name and the field sharing the
        // rest equally, the swatch and *Restore* the same size in every row).
        let column = ui.add(self.area, Flex::column(Space::S, Space::None), wide)?;
        for (index, (token, name)) in ColorToken::ALL.iter().zip(&texts.color_names).enumerate() {
            let block = ui.add(column, Flex::column(Space::S, Space::None), wide)?;
            let row = ui.add(block, Flex::row(Space::S, Space::None), wide)?;
            let mark = ui.add(
                row,
                Label::new(String::new()).color(ColorToken::Warning),
                centred(LayoutProps::default().width(Length::Fixed(MARK_WIDTH))),
            )?;
            ui.add(row, Label::new(name.clone()).wrapping(), centred(wide))?;
            let swatch = ui.add(
                row,
                SwatchButton::new(
                    *token,
                    DemoMessage::Theme(ThemeMessage::OpenPicker(index)),
                    texts.choose.fill(&[name]),
                ),
                centred(LayoutProps::default()),
            )?;
            let message = COLOR_MESSAGES
                .get(index)
                .copied()
                .unwrap_or(color_message::<0>);
            let field = ui.add(
                row,
                TextField::new(&color_text(theme.color(*token)), message)
                    .on_change(message)
                    .labelled(texts.value_name.fill(&[name])),
                centred(wide),
            )?;
            let restore = ui.add(
                row,
                Button::new(
                    texts.restore.clone(),
                    DemoMessage::Theme(ThemeMessage::Restore(index)),
                ),
                centred(LayoutProps::default()),
            )?;
            self.rows.push(Row {
                block,
                mark,
                swatch,
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
        // Where the open picker is now, to keep it there however the sentences above change.
        let picker_top = self
            .picker
            .and_then(|open| ui.rect(open.widget))
            .map(|rect| rect.y0);
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
            let issues = readability_issues(&theme, &base, self.last_edited);
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
                let _ = ui.add(
                    row,
                    Label::new(issue_line(
                        &texts.issues,
                        |token| texts.color_name(token),
                        issue,
                    ))
                    .wrapping(),
                    wide,
                );
            }
        }
        ui.frame(list);
        self.keep_picker_still(ui, picker_top, list);
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
}
