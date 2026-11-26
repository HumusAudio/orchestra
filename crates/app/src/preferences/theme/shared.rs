// SPDX-License-Identifier: GPL-3.0-or-later

//! What a custom theme shares between its two halves: the **text** (its size and families), the
//! **lines, corners and shadows** (the shape tokens) and the **density** (SCALETTA F2.12b2a,
//! owner's choices of 2026-11-26).
//!
//! # Why shared
//! A theme's colours differ between its light and dark halves; its corners, lines and text do
//! not, in practice (a dark half with rounder corners would only surprise). The owner chose that
//! these are **the same in both halves**: every function here writes its override into both of
//! the custom theme's lists, and reads the current value from the light half, which is also the
//! half the interface takes them from (the demonstration's `apply_theme`). A settings file
//! written by hand with different values keeps working: the light half's win.
//!
//! # Only the differences (ADR-0037 T7)
//! As for colours, a value equal to the base pair's removes the override instead of adding one,
//! so a theme stores only what the user changed, and tokens of future versions come from the
//! base.
//!
//! # Safe limits (owner's choices of 2026-11-26)
//! The editor's controls stop where the interface would become hard to use, instead of warning
//! after: text from 85% to 150% of the base's sizes (body text never below 11 px), lines 1–4 px
//! (0 would hide the controls' edges, WCAG 1.4.11), the focus decoration 2–6 px (WCAG 2.4.13
//! asks for 2), the dialogs' frame 1–8 px (0 would let a dialog blend into the window behind it);
//! corners 0–16 px and the shadow's blur and distance 0–32 and 0–16 px are free taste. The
//! families offered are the bundled text faces; the music face (Bravura) stays as it is: chosen
//! for interface text it would draw symbols instead of letters.

use orchestra_i18n::{I18nError, Localizer};
use orchestra_io::settings::{Settings, ThemeOverride};
use orchestra_ui::{
    Align, Appearance, BuiltInTheme, Button, Density, DropDown, Family, FamilyToken, Flex, Label,
    LayoutProps, Length, MAX_TEXT_SIZE, MIN_TEXT_SIZE, RadioGroup, ShapeToken, SizeToken, Slider,
    Space, Theme, Ui, UiError, Weight, WidgetId,
};

use super::{ThemeMessage, chosen_custom, instance, set_label};
use crate::demo::DemoMessage;
use crate::template::{ISOLATION_MARKS, Template};

/// The smallest text size the editor offers, percent of the base's sizes.
pub(crate) const TEXT_SCALE_MIN: u16 = 85;
/// The largest text size the editor offers, percent of the base's sizes.
pub(crate) const TEXT_SCALE_MAX: u16 = 150;
/// The text size's step, percent: 14 steps from 85 to 150, each a visible change.
pub(crate) const TEXT_SCALE_STEP: u16 = 5;

/// The family tokens the editor lets the user change, in the rows' order (the music face is not
/// offered, see the module documentation).
pub(crate) const EDITABLE_FAMILIES: [FamilyToken; 3] =
    [FamilyToken::Ui, FamilyToken::Numeric, FamilyToken::Code];

/// The families offered for them, in the drop-downs' order: the bundled text faces.
pub(crate) const FAMILY_CHOICES: [Family; 3] =
    [Family::Inter, Family::InterTabular, Family::JetBrainsMono];

/// Each shape token with its limits in logical pixels, in the rows' order (see the module
/// documentation for why).
pub(crate) const SHAPE_LIMITS: [(ShapeToken, u8, u8); 6] = [
    (ShapeToken::CornerRadius, 0, 16),
    (ShapeToken::LineWidth, 1, 4),
    (ShapeToken::FocusWidth, 2, 6),
    (ShapeToken::ShadowBlur, 0, 32),
    (ShapeToken::ShadowOffset, 0, 16),
    (ShapeToken::DialogFrame, 1, 8),
];

/// One shared value to set, or (`None`) to give back to the base.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SharedValue {
    /// The six text sizes, as one percentage of the base's (clamped to the editor's limits and
    /// to [`MIN_TEXT_SIZE`]..=[`MAX_TEXT_SIZE`]).
    TextScale(Option<u16>),
    /// One family token's family.
    Family(FamilyToken, Option<Family>),
    /// One shape token's value, logical pixels (clamped to its limits).
    Shape(ShapeToken, Option<u8>),
    /// The density.
    Density(Option<Density>),
}

/// The light half of the chosen custom theme's base pair: what the shared values are compared
/// with. `None` for a built-in pair (nothing to edit).
pub(crate) fn base(settings: &Settings) -> Option<Theme> {
    let custom = settings.theme.custom.get(chosen_custom(settings)?)?;
    Some(
        *BuiltInTheme::from_id(&custom.base)
            .unwrap_or_default()
            .pair()
            .instance(Appearance::Light),
    )
}

/// The size of `token` at `percent` of the base's `size`, rounded to the nearest pixel (half
/// up), within [`MIN_TEXT_SIZE`]..=[`MAX_TEXT_SIZE`]. Integer arithmetic: exact and the same on
/// every platform.
pub(crate) fn scaled(size: u8, percent: u16) -> u8 {
    let pixels = (u32::from(size) * u32::from(percent) + 50) / 100;
    u8::try_from(pixels)
        .unwrap_or(MAX_TEXT_SIZE)
        .clamp(MIN_TEXT_SIZE, MAX_TEXT_SIZE)
}

/// The text size the editor shows for `theme`, percent of `base`'s: the step of the editor's
/// range whose scaled sizes are exactly `theme`'s six (the value the user chose), else the step
/// nearest to the body text's ratio (sizes written by hand). O(steps × 6).
pub(crate) fn text_scale(theme: &Theme, base: &Theme) -> u16 {
    let steps = (TEXT_SCALE_MIN..=TEXT_SCALE_MAX).step_by(usize::from(TEXT_SCALE_STEP));
    let sizes = |t: &Theme| SizeToken::ALL.map(|token| t.typography().size(token));
    let (current, original) = (sizes(theme), sizes(base));
    let exact = steps
        .clone()
        .find(|&percent| original.map(|size| scaled(size, percent)) == current);
    exact.unwrap_or_else(|| {
        let body = |t: &Theme| u32::from(t.typography().size(SizeToken::Body)).max(1);
        let ratio = body(theme) * 100 / body(base);
        steps
            .min_by_key(|&percent| u32::from(percent).abs_diff(ratio))
            .unwrap_or(100)
    })
}

/// Whether `item` is an override of the same thing as `value` (so it is replaced).
fn same_target(item: &ThemeOverride, value: SharedValue) -> bool {
    match (item, value) {
        (ThemeOverride::TextSize { .. }, SharedValue::TextScale(_))
        | (ThemeOverride::Density(_), SharedValue::Density(_)) => true,
        (ThemeOverride::Family { token, .. }, SharedValue::Family(family, _)) => {
            token == family.name()
        }
        (ThemeOverride::Shape { token, .. }, SharedValue::Shape(shape, _)) => token == shape.name(),
        _ => false,
    }
}

/// The overrides that give `value` against `base`: none when it is the base's (T7).
fn overrides_for(value: SharedValue, base: &Theme) -> Vec<ThemeOverride> {
    match value {
        SharedValue::TextScale(percent) => {
            let percent = percent.map_or(100, |p| p.clamp(TEXT_SCALE_MIN, TEXT_SCALE_MAX));
            SizeToken::ALL
                .iter()
                .filter_map(|&token| {
                    let original = base.typography().size(token);
                    let size = scaled(original, percent);
                    (size != original).then(|| ThemeOverride::TextSize {
                        token: token.name().to_owned(),
                        size,
                    })
                })
                .collect()
        }
        SharedValue::Family(token, family) => family
            .filter(|family| *family != base.typography().family(token))
            .map(|family| ThemeOverride::Family {
                token: token.name().to_owned(),
                family: family.name().to_owned(),
            })
            .into_iter()
            .collect(),
        SharedValue::Shape(token, value) => {
            let (_, min, max) = SHAPE_LIMITS
                .iter()
                .copied()
                .find(|(known, _, _)| *known == token)
                .unwrap_or((token, 0, u8::MAX));
            value
                .map(|value| value.clamp(min, max))
                .filter(|value| *value != base.shape().get(token))
                .map(|value| ThemeOverride::Shape {
                    token: token.name().to_owned(),
                    value,
                })
                .into_iter()
                .collect()
        }
        SharedValue::Density(density) => density
            .filter(|density| *density != base.density())
            .map(|density| ThemeOverride::Density(density.name().to_owned()))
            .into_iter()
            .collect(),
    }
}

/// Sets `value` in **both halves** of the chosen custom theme (see the module documentation):
/// the earlier overrides of the same thing go from both lists, then the new ones (none when equal
/// to the base's) are added to both. Nothing for a built-in pair. O(overrides).
pub(crate) fn set_shared(settings: &mut Settings, value: SharedValue) {
    let Some(base) = base(settings) else {
        return;
    };
    let Some(custom) = chosen_custom(settings).and_then(|i| settings.theme.custom.get_mut(i))
    else {
        return;
    };
    let added = overrides_for(value, &base);
    for list in [&mut custom.light, &mut custom.dark] {
        list.retain(|item| !same_target(item, value));
        list.extend(added.iter().cloned());
    }
}

/// The shared values the editor shows for the chosen custom theme, read from its light half
/// (see the module documentation), and whether each differs from the base. `None` for a built-in
/// pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SharedState {
    /// The text size, percent.
    pub(crate) text_scale: u16,
    /// The families of [`EDITABLE_FAMILIES`], in order.
    pub(crate) families: [Family; 3],
    /// The shape tokens of [`SHAPE_LIMITS`], in order.
    pub(crate) shapes: [u8; 6],
    /// The density.
    pub(crate) density: Density,
    /// The same values of the base, for *Restore*'s states and the sliders' defaults.
    pub(crate) base: (u16, [Family; 3], [u8; 6], Density),
}

impl SharedState {
    /// The state of `settings`' chosen custom theme.
    pub(crate) fn of(settings: &Settings) -> Option<Self> {
        let base = base(settings)?;
        let theme = instance(settings, Appearance::Light);
        let families = |t: &Theme| EDITABLE_FAMILIES.map(|token| t.typography().family(token));
        let shapes = |t: &Theme| SHAPE_LIMITS.map(|(token, _, _)| t.shape().get(token));
        Some(Self {
            text_scale: text_scale(&theme, &base),
            families: families(&theme),
            shapes: shapes(&theme),
            density: theme.density(),
            base: (100, families(&base), shapes(&base), base.density()),
        })
    }
}

// ------------------------------------------------------------------------------------------------
// The texts and the widgets
// ------------------------------------------------------------------------------------------------

/// The texts of the shared values' rows in one language (CLAUDE.md §7).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SharedTexts {
    /// `prefs-theme-text`: the heading of the text rows.
    pub(crate) text: String,
    /// `prefs-theme-text-size`.
    pub(crate) text_size: String,
    /// `prefs-theme-family-ui`, `-numeric`, `-code`, in [`EDITABLE_FAMILIES`]' order.
    pub(crate) family_rows: [String; 3],
    /// `prefs-theme-family-inter`, `-inter-tabular`, `-mono`, in [`FAMILY_CHOICES`]' order.
    pub(crate) family_names: Vec<String>,
    /// `prefs-theme-lines`: the heading of the shape rows.
    pub(crate) lines: String,
    /// `prefs-theme-shape-*`, in [`SHAPE_LIMITS`]' order.
    pub(crate) shape_rows: [String; 6],
    /// `prefs-theme-density`.
    pub(crate) density: String,
    /// `prefs-theme-density-comfortable`, `-compact`, in `Density::ALL`'s order.
    pub(crate) densities: Vec<String>,
    /// `prefs-theme-pixels`, with `$value`.
    pub(crate) pixels: Template,
    /// `prefs-theme-percent`, with `$value`.
    pub(crate) percent: Template,
}

impl SharedTexts {
    /// The texts in `localizer`'s language.
    ///
    /// # Errors
    /// The localizer's, if a message is missing (the catalogues' parity tests prevent it).
    pub(crate) fn new(localizer: &Localizer) -> Result<Self, I18nError> {
        let text = |key: &str| -> Result<String, I18nError> {
            Ok(localizer.format(key, None)?.replace(ISOLATION_MARKS, ""))
        };
        Ok(Self {
            text: text("prefs-theme-text")?,
            text_size: text("prefs-theme-text-size")?,
            family_rows: [
                text("prefs-theme-family-ui")?,
                text("prefs-theme-family-numeric")?,
                text("prefs-theme-family-code")?,
            ],
            family_names: vec![
                text("prefs-theme-family-inter")?,
                text("prefs-theme-family-inter-tabular")?,
                text("prefs-theme-family-mono")?,
            ],
            lines: text("prefs-theme-lines")?,
            shape_rows: [
                text("prefs-theme-shape-corners")?,
                text("prefs-theme-shape-lines")?,
                text("prefs-theme-shape-focus")?,
                text("prefs-theme-shape-shadow-blur")?,
                text("prefs-theme-shape-shadow-distance")?,
                text("prefs-theme-shape-dialog-frame")?,
            ],
            density: text("prefs-theme-density")?,
            densities: vec![
                text("prefs-theme-density-comfortable")?,
                text("prefs-theme-density-compact")?,
            ],
            pixels: Template::new(localizer, "prefs-theme-pixels", &["value"])?,
            percent: Template::new(localizer, "prefs-theme-percent", &["value"])?,
        })
    }

    /// The texts with a value, each holding its placeholder once (tests).
    pub(crate) fn templates(&self) -> [&Template; 2] {
        [&self.pixels, &self.percent]
    }
}

/// The text size slider's message.
fn text_scale_message(value: f64) -> DemoMessage {
    DemoMessage::Theme(ThemeMessage::TextScale(whole(value)))
}

/// The message of the family drop-down number `I` (a plain function, ADR-0045 WA7).
fn family_message<const I: usize>(choice: usize) -> DemoMessage {
    DemoMessage::Theme(ThemeMessage::Family(I, choice))
}

/// The message of the shape slider number `I`.
fn shape_message<const I: usize>(value: f64) -> DemoMessage {
    DemoMessage::Theme(ThemeMessage::Shape(I, whole(value)))
}

/// The density group's message.
fn density_message(choice: usize) -> DemoMessage {
    DemoMessage::Theme(ThemeMessage::Density(choice))
}

/// One message function per editable family, in [`EDITABLE_FAMILIES`]' order.
const FAMILY_MESSAGES: [fn(usize) -> DemoMessage; 3] = [
    family_message::<0>,
    family_message::<1>,
    family_message::<2>,
];

/// One message function per shape token, in [`SHAPE_LIMITS`]' order.
const SHAPE_MESSAGES: [fn(f64) -> DemoMessage; 6] = [
    shape_message::<0>,
    shape_message::<1>,
    shape_message::<2>,
    shape_message::<3>,
    shape_message::<4>,
    shape_message::<5>,
];

/// The width of a value's label beside its slider, logical pixels: "150%" or "32 px" with room.
const VALUE_WIDTH: f32 = 64.0;

/// A slider row's widgets.
#[derive(Debug, Clone, Copy)]
pub struct SliderRow {
    /// The slider.
    pub slider: WidgetId,
    /// Its value, written beside it.
    pub value: WidgetId,
    /// *Restore*.
    pub restore: WidgetId,
}

/// A choice row's widgets (a drop-down or a radio group).
#[derive(Debug, Clone, Copy)]
pub struct ChoiceRow {
    /// The control.
    pub control: WidgetId,
    /// *Restore*.
    pub restore: WidgetId,
}

/// The rows of the shared values (see the module documentation), as built.
#[derive(Debug, Clone, Copy)]
pub struct SharedRows {
    /// The text size.
    pub text_scale: SliderRow,
    /// The families, in [`EDITABLE_FAMILIES`]' order.
    pub families: [ChoiceRow; 3],
    /// The shapes, in [`SHAPE_LIMITS`]' order.
    pub shapes: [SliderRow; 6],
    /// The density.
    pub density: ChoiceRow,
}

/// A heading of the editor (semi-bold body text), as the colours' one.
fn heading(text: &str) -> Label {
    Label::new(text.to_owned()).styled(FamilyToken::Ui, SizeToken::Body, Weight::SemiBold)
}

/// The layout that fills the width it is given.
fn wide() -> LayoutProps {
    LayoutProps::default().width(Length::Fill(1.0))
}

/// One setting's block in `group`: its `name` on a line of its own (wrapping when long), then a
/// row for its control, its value and *Restore*. Answers that row.
///
/// # Why a name above and not beside
/// A grid with the names in a column of their own was wider than the section in Italian and with
/// the text at 150% (owner's report of 2026-11-26: the controls and *Restore* were cut at the
/// window's edge): names like "Spessore della decorazione del focus", fonts like "Inter, cifre
/// della stessa larghezza" and *Restore* side by side need more than the section's 540 or so
/// logical pixels. With the name above, the row only holds the control (which takes the room
/// left, a drop-down shortening its text with an ellipsis if it must, F2.09f), the value (fixed
/// width) and *Restore*: every row ends on the same right edge, so the values and the buttons
/// still stand in columns, and nothing can overflow at any width or text size.
///
/// # Errors
/// The interface's.
fn setting(ui: &mut Ui<DemoMessage>, group: WidgetId, name: &str) -> Result<WidgetId, UiError> {
    let block = ui.add(group, Flex::column(Space::Xs, Space::None), wide())?;
    ui.add(block, Label::new(name.to_owned()).wrapping(), wide())?;
    ui.add(block, Flex::row(Space::S, Space::None), wide())
}

/// The value's place in a row: its label, or (no value) an empty one of the same width, so that
/// *Restore* stands in the same place on every row.
fn value_label(ui: &mut Ui<DemoMessage>, row: WidgetId, text: String) -> Result<WidgetId, UiError> {
    ui.add(
        row,
        Label::new(text),
        LayoutProps::default()
            .width(Length::Fixed(VALUE_WIDTH))
            .align(Align::Start, Align::Center),
    )
}

/// *Restore* at the end of `row`, sending `message`.
fn restore_button(
    ui: &mut Ui<DemoMessage>,
    row: WidgetId,
    text: &str,
    message: ThemeMessage,
) -> Result<WidgetId, UiError> {
    ui.add(
        row,
        Button::new(text.to_owned(), DemoMessage::Theme(message)),
        LayoutProps::default().align(Align::Start, Align::Center),
    )
}

/// A slider setting in `group`: `name`, a slider from `min` to `max` by `step` at `value` (double
/// click: `default`, ADR-0046), its value written by `written`, and *Restore* sending `restore`.
///
/// # Errors
/// The interface's.
fn slider_row(
    ui: &mut Ui<DemoMessage>,
    group: WidgetId,
    name: &str,
    (min, max, step): (f64, f64, f64),
    (value, default): (f64, f64),
    (message, restore): (fn(f64) -> DemoMessage, ThemeMessage),
    (written, restore_text): (String, &str),
) -> Result<SliderRow, UiError> {
    let row = setting(ui, group, name)?;
    let slider = ui.add(
        row,
        Slider::new(min, max, step, value, message)
            .default_value(default)
            .labelled(name.to_owned()),
        wide().align(Align::Start, Align::Center),
    )?;
    let value = value_label(ui, row, written)?;
    let restore = restore_button(ui, row, restore_text, restore)?;
    Ok(SliderRow {
        slider,
        value,
        restore,
    })
}

/// Adds the shared values' settings to `area`, under three headings: **Text** (the size, then
/// the three families), **Lines, corners and shadows** (the six shapes) and **Density**, each
/// setting a block with its name above its control (see [`setting`]). O(rows).
///
/// # Errors
/// The interface's, if its own tree is inconsistent (never expected).
pub(crate) fn build(
    ui: &mut Ui<DemoMessage>,
    area: WidgetId,
    texts: &SharedTexts,
    restore: &str,
    state: &SharedState,
) -> Result<SharedRows, UiError> {
    // The settings of one heading sit closer together than the editor's parts.
    let group = |ui: &mut Ui<DemoMessage>, title: &str| -> Result<WidgetId, UiError> {
        ui.add(area, heading(title), LayoutProps::default())?;
        ui.add(area, Flex::column(Space::S, Space::None), wide())
    };
    let text_group = group(ui, &texts.text)?;
    let text_scale = slider_row(
        ui,
        text_group,
        &texts.text_size,
        (
            f64::from(TEXT_SCALE_MIN),
            f64::from(TEXT_SCALE_MAX),
            f64::from(TEXT_SCALE_STEP),
        ),
        (f64::from(state.text_scale), f64::from(state.base.0)),
        (text_scale_message, ThemeMessage::RestoreText),
        (
            texts.percent.fill(&[&state.text_scale.to_string()]),
            restore,
        ),
    )?;
    let families = family_rows(ui, text_group, texts, restore, state)?;
    let lines_group = group(ui, &texts.lines)?;
    let mut shapes = Vec::with_capacity(SHAPE_LIMITS.len());
    for (index, (((name, (_, min, max)), message), (value, base))) in texts
        .shape_rows
        .iter()
        .zip(SHAPE_LIMITS)
        .zip(SHAPE_MESSAGES)
        .zip(state.shapes.into_iter().zip(state.base.2))
        .enumerate()
    {
        shapes.push(slider_row(
            ui,
            lines_group,
            name,
            (f64::from(min), f64::from(max), 1.0),
            (f64::from(value), f64::from(base)),
            (message, ThemeMessage::RestoreShape(index)),
            (texts.pixels.fill(&[&value.to_string()]), restore),
        )?);
    }
    let density_group = group(ui, &texts.density)?;
    let density = density_row(ui, density_group, texts, restore, state)?;
    let shapes: [SliderRow; 6] = shapes
        .try_into()
        .map_err(|_| UiError::UnknownWidget(area))?;
    Ok(SharedRows {
        text_scale,
        families,
        shapes,
        density,
    })
}

/// The three family settings, in [`EDITABLE_FAMILIES`]' order: the name, a drop-down of
/// [`FAMILY_CHOICES`] taking the row's room, an empty value's place, *Restore*.
///
/// # Errors
/// The interface's.
fn family_rows(
    ui: &mut Ui<DemoMessage>,
    group: WidgetId,
    texts: &SharedTexts,
    restore: &str,
    state: &SharedState,
) -> Result<[ChoiceRow; 3], UiError> {
    let mut families = Vec::with_capacity(EDITABLE_FAMILIES.len());
    for (index, ((name, family), message)) in texts
        .family_rows
        .iter()
        .zip(state.families)
        .zip(FAMILY_MESSAGES)
        .enumerate()
    {
        let selected = FAMILY_CHOICES
            .iter()
            .position(|choice| *choice == family)
            .unwrap_or(0);
        let row = setting(ui, group, name)?;
        let control = ui.add(
            row,
            DropDown::new(texts.family_names.clone(), selected, message).label(name.clone()),
            wide().align(Align::Start, Align::Center),
        )?;
        value_label(ui, row, String::new())?;
        let restore = restore_button(ui, row, restore, ThemeMessage::RestoreFamily(index))?;
        families.push(ChoiceRow { control, restore });
    }
    families
        .try_into()
        .map_err(|_| UiError::UnknownWidget(group))
}

/// The density setting: the group of densities at its natural width (a segmented control is not
/// stretched), the room left empty, then *Restore* on the rows' right edge.
///
/// # Errors
/// The interface's.
fn density_row(
    ui: &mut Ui<DemoMessage>,
    group: WidgetId,
    texts: &SharedTexts,
    restore: &str,
    state: &SharedState,
) -> Result<ChoiceRow, UiError> {
    let row = ui.add(group, Flex::row(Space::S, Space::None), wide())?;
    let selected = Density::ALL
        .iter()
        .position(|density| *density == state.density)
        .unwrap_or(0);
    let control = ui.add(
        row,
        RadioGroup::new(texts.densities.clone(), selected, density_message)
            .label(texts.density.clone()),
        LayoutProps::default().align(Align::Start, Align::Center),
    )?;
    // The room between the group and *Restore*: an empty label that takes it (it draws nothing).
    ui.add(row, Label::new(String::new()), wide())?;
    let restore = restore_button(ui, row, restore, ThemeMessage::RestoreDensity)?;
    Ok(ChoiceRow { control, restore })
}

/// Shows `state` in `rows`: the values written beside the sliders and *Restore* enabled where a
/// value differs from the base. With `controls`, the sliders, drop-downs and the density group
/// are set too (after *Restore*: the control did not move by itself). O(rows).
pub(crate) fn refresh(
    ui: &mut Ui<DemoMessage>,
    rows: &SharedRows,
    texts: &SharedTexts,
    state: &SharedState,
    controls: bool,
) {
    let enable = |ui: &mut Ui<DemoMessage>, id: WidgetId, on: bool| {
        if let Some(button) = ui.widget_mut::<Button<DemoMessage>>(id) {
            button.set_enabled(on);
        }
        ui.request_record(id);
    };
    let slide = |ui: &mut Ui<DemoMessage>, id: WidgetId, value: f64| {
        if controls && let Some(slider) = ui.widget_mut::<Slider<DemoMessage>>(id) {
            slider.set_value(value);
            ui.request_record(id);
        }
    };
    let (base_scale, base_families, base_shapes, base_density) = state.base;
    set_label(
        ui,
        rows.text_scale.value,
        &texts.percent.fill(&[&state.text_scale.to_string()]),
    );
    enable(ui, rows.text_scale.restore, state.text_scale != base_scale);
    slide(ui, rows.text_scale.slider, f64::from(state.text_scale));
    for ((row, family), base) in rows.families.iter().zip(state.families).zip(base_families) {
        enable(ui, row.restore, family != base);
        if controls && let Some(drop) = ui.widget_mut::<DropDown<DemoMessage>>(row.control) {
            drop.set_selected(
                FAMILY_CHOICES
                    .iter()
                    .position(|choice| *choice == family)
                    .unwrap_or(0),
            );
            ui.request_layout(row.control);
        }
    }
    for ((row, value), base) in rows.shapes.iter().zip(state.shapes).zip(base_shapes) {
        set_label(ui, row.value, &texts.pixels.fill(&[&value.to_string()]));
        enable(ui, row.restore, value != base);
        slide(ui, row.slider, f64::from(value));
    }
    enable(ui, rows.density.restore, state.density != base_density);
    if controls && let Some(group) = ui.widget_mut::<RadioGroup<DemoMessage>>(rows.density.control)
    {
        group.set_selected(
            Density::ALL
                .iter()
                .position(|density| *density == state.density)
                .unwrap_or(0),
        );
        ui.request_record(rows.density.control);
    }
}

/// A slider's value as the shared value it stands for: rounded to a whole number, within
/// `0..=u16::MAX` (the slider's own limits keep it in range; this only makes the conversion
/// total).
fn whole(value: f64) -> u16 {
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "clamped to 0..=65 535 and rounded just before"
    )]
    let whole = value.round().clamp(0.0, f64::from(u16::MAX)) as u16;
    whole
}

#[cfg(test)]
mod tests {
    //! Scaling, the limits, both halves written, only the differences kept.
    use super::*;
    use orchestra_io::settings::{CustomTheme, ThemeChoice};

    /// A custom theme on Studio, chosen.
    fn studio_copy() -> Settings {
        let mut settings = Settings::default();
        settings.theme.custom.push(CustomTheme {
            name: "Mine".to_owned(),
            base: BuiltInTheme::Studio.id().to_owned(),
            light: Vec::new(),
            dark: Vec::new(),
        });
        settings.theme.choice = ThemeChoice::Custom("Mine".to_owned());
        settings
    }

    /// Rounding half up, the limits of the model kept.
    #[test]
    fn sizes_scale_and_round() {
        assert_eq!(scaled(13, 100), 13);
        assert_eq!(scaled(13, 85), 11, "11.05");
        assert_eq!(scaled(11, 150), 17, "16.5 rounds up");
        assert_eq!(scaled(6, 85), MIN_TEXT_SIZE);
        assert_eq!(scaled(90, 150), MAX_TEXT_SIZE);
    }

    /// Every step of the range reads back as itself: the six sizes tell the steps apart, so the
    /// slider never jumps when the Preferences open again.
    #[test]
    fn every_step_reads_back() {
        for pair in BuiltInTheme::ALL {
            let base = *pair.pair().instance(Appearance::Light);
            for percent in (TEXT_SCALE_MIN..=TEXT_SCALE_MAX).step_by(5) {
                let mut theme = base;
                for token in SizeToken::ALL {
                    theme =
                        theme.with_text_size(token, scaled(base.typography().size(token), percent));
                }
                assert_eq!(text_scale(&theme, &base), percent, "{pair:?}");
            }
        }
    }

    /// Each kind of value lands in both halves, equally; the base's value removes it; a value
    /// beyond the limits is brought within them; a built-in pair is never changed.
    #[test]
    fn both_halves_and_only_the_differences() {
        let mut builtin = Settings::default();
        set_shared(
            &mut builtin,
            SharedValue::Shape(ShapeToken::CornerRadius, Some(9)),
        );
        assert_eq!(builtin, Settings::default());
        let mut settings = studio_copy();
        set_shared(&mut settings, SharedValue::TextScale(Some(120)));
        set_shared(
            &mut settings,
            SharedValue::Family(FamilyToken::Ui, Some(Family::JetBrainsMono)),
        );
        set_shared(
            &mut settings,
            SharedValue::Shape(ShapeToken::FocusWidth, Some(0)),
        );
        set_shared(&mut settings, SharedValue::Density(Some(Density::Compact)));
        let custom = &settings.theme.custom[0];
        assert_eq!(custom.light, custom.dark, "the same in both halves");
        let state = SharedState::of(&settings).unwrap();
        assert_eq!(state.text_scale, 120);
        assert_eq!(state.families[0], Family::JetBrainsMono);
        // 0 is below the focus decoration's limit: 2, which is Studio's, so no override at all.
        assert_eq!(state.shapes[2], 2);
        assert!(
            !custom
                .light
                .iter()
                .any(|o| matches!(o, ThemeOverride::Shape { .. }))
        );
        assert_eq!(state.density, Density::Compact);
        // Back to the base, one by one: nothing left.
        set_shared(&mut settings, SharedValue::TextScale(None));
        set_shared(&mut settings, SharedValue::Family(FamilyToken::Ui, None));
        set_shared(
            &mut settings,
            SharedValue::Density(Some(Density::Comfortable)),
        );
        let custom = &settings.theme.custom[0];
        assert_eq!((custom.light.len(), custom.dark.len()), (0, 0));
        // A shape beyond its limit is clamped, in both halves.
        set_shared(
            &mut settings,
            SharedValue::Shape(ShapeToken::CornerRadius, Some(200)),
        );
        assert_eq!(SharedState::of(&settings).unwrap().shapes[0], 16);
        let custom = &settings.theme.custom[0];
        assert_eq!(custom.light, custom.dark);
    }

    /// The colours' overrides are left alone by the shared values.
    #[test]
    fn colours_are_untouched() {
        let mut settings = studio_copy();
        let colour = ThemeOverride::Color {
            token: "accent".to_owned(),
            rgba: 0x1234_56ff,
        };
        settings.theme.custom[0].dark.push(colour.clone());
        set_shared(&mut settings, SharedValue::TextScale(Some(110)));
        set_shared(&mut settings, SharedValue::TextScale(None));
        assert_eq!(settings.theme.custom[0].dark, [colour]);
        assert_eq!(settings.theme.custom[0].light, []);
    }
}
