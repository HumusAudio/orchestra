// SPDX-License-Identifier: GPL-3.0-or-later

//! The Preferences' **Playback** section (ADR-0050 DG9, ADR-0027 §14–§15, SCALETTA F2.12a4b2,
//! owner's choices 4–5): the eighteen values of the playback's tuning, each with a slider and its
//! value written beside it, in three groups, and *Restore defaults*.
//!
//! # The layout (choice 5)
//! The section scrolls (a vertical scroll area: eighteen values do not fit a small window). From
//! the top: a muted sentence saying what the values do and that they are personal, not the
//! project's (DG9); then three groups, each a heading and a grid with one row per mark:
//! - **Articulation accents**: soft accent, stress, accent, marcato (*Louder by*, *Back in*) and
//!   unstress (*Louder by*, negative);
//! - **Dynamic accents**: *rf, rfz*, *sf, sfz, fz*, *sffz* (*Back in*);
//! - **Changes of level**: *fp*, *sfp, sfpp*, *pf* (*Held for*, *Change in*);
//!
//! and at the end *Restore defaults*, which brings back the tuning by ear (the other sections'
//! values are untouched).
//!
//! # The value beside each slider (choice 4)
//! A slider draws no number, so a label beside it says the value: a step with its sign ("+22",
//! "−7", with the true minus sign) and a time in whole milliseconds ("250 ms"). Whole
//! milliseconds avoid writing decimals, whose separator differs between the languages and which
//! Fluent alone does not localise. The label is updated at each change ([`value_text`]).
//!
//! # Why the messages are a table of functions
//! A slider sends `fn(f64) -> M`, a plain function pointer with no captured state (ADR-0045 WA7),
//! so it cannot carry "which value" by itself. [`MESSAGES`] holds one non-capturing function per
//! value, each writing its own index into [`PreferencesMessage::Playback`].

use orchestra_i18n::{FluentArgs, I18nError, Localizer};
use orchestra_playback::{Tuning, TuningUnit, TuningValue};
use orchestra_ui::{
    Align, Button, ColorToken, DisplayList, FamilyToken, Flex, Grid, Label, LayoutProps, Length,
    ScrollArea, SizeToken, Slider, Space, Track, Ui, UiError, Weight, WidgetId,
};

use super::PreferencesMessage;
use crate::demo::DemoMessage;

/// A slider's value as the integer the tuning keeps: the slider's steps are whole numbers already
/// (1 for steps, 10 ms for times), so rounding only removes floating-point noise.
#[allow(
    clippy::cast_possible_truncation,
    reason = "the sliders' ranges are within ±2000, far inside i64; NaN never comes from a slider \
              (it clamps), and would give 0"
)]
fn whole(value: f64) -> i64 {
    value.round() as i64
}

/// The message of the slider of value number `I` in [`TuningValue::ALL`].
fn message<const I: usize>(value: f64) -> DemoMessage {
    DemoMessage::Preferences(PreferencesMessage::Playback(I, whole(value)))
}

/// One message function per value, in [`TuningValue::ALL`]'s order (see the module
/// documentation).
pub(super) const MESSAGES: [fn(f64) -> DemoMessage; 18] = [
    message::<0>,
    message::<1>,
    message::<2>,
    message::<3>,
    message::<4>,
    message::<5>,
    message::<6>,
    message::<7>,
    message::<8>,
    message::<9>,
    message::<10>,
    message::<11>,
    message::<12>,
    message::<13>,
    message::<14>,
    message::<15>,
    message::<16>,
    message::<17>,
];

/// A message with one number, split around it once: what comes before the number and what comes
/// after, as the translator wrote them (the number's place and the space before a unit are the
/// language's). Formatted at each change without a localizer, and without gluing words in code.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Around {
    /// The text before the number.
    before: String,
    /// The text after it.
    after: String,
}

impl Around {
    /// The message `key` with its `$value` split out.
    ///
    /// # Errors
    /// The localizer's, if the message is missing.
    fn new(localizer: &Localizer, key: &str) -> Result<Self, I18nError> {
        // A character no translation contains (a private-use code point) marks the number's
        // place; Fluent's isolation marks around it are dropped with it.
        const MARK: &str = "\u{E000}";
        let mut args = FluentArgs::new();
        args.set("value", MARK);
        let text = localizer
            .format(key, Some(&args))?
            .replace(['\u{2068}', '\u{2069}'], "");
        let (before, after) = text.split_once(MARK).unwrap_or((text.as_str(), ""));
        Ok(Self {
            before: before.to_owned(),
            after: after.to_owned(),
        })
    }

    /// The message with `number` in its place.
    fn with(&self, number: &str) -> String {
        let mut text = String::with_capacity(self.before.len() + number.len() + self.after.len());
        text.push_str(&self.before);
        text.push_str(number);
        text.push_str(&self.after);
        text
    }
}

/// The texts of the Playback section (CLAUDE.md §7).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaybackTexts {
    /// `prefs-section-playback`.
    pub(super) title: String,
    /// `prefs-playback-intro`.
    intro: String,
    /// The three groups' headings.
    groups: [String; 3],
    /// The columns' headings: *Louder by*, *Back in*, *Held for*, *Change in*.
    columns: [String; 4],
    /// The rows' names: the five articulations, then the six dynamic marks' groups.
    rows: [String; 11],
    /// `prefs-playback-ms` around its number.
    milliseconds: Around,
    /// `prefs-playback-step` around its number.
    step: Around,
    /// `dialog-restore-defaults`.
    restore: String,
}

impl PlaybackTexts {
    /// The texts in `localizer`'s language.
    ///
    /// # Errors
    /// The localizer's, if a message is missing (the catalogues' parity tests prevent it).
    pub fn new(localizer: &Localizer) -> Result<Self, I18nError> {
        let text = |key: &str| localizer.format(key, None);
        Ok(Self {
            title: text("prefs-section-playback")?,
            intro: text("prefs-playback-intro")?,
            groups: [
                text("prefs-playback-articulations")?,
                text("prefs-playback-dynamic-accents")?,
                text("prefs-playback-changes")?,
            ],
            columns: [
                text("prefs-playback-louder")?,
                text("prefs-playback-back")?,
                text("prefs-playback-held")?,
                text("prefs-playback-change")?,
            ],
            rows: [
                text("articulation-soft-accent")?,
                text("articulation-stress")?,
                text("articulation-accent")?,
                text("articulation-marcato")?,
                text("articulation-unstress")?,
                text("dynamic-rf")?,
                text("dynamic-sf")?,
                text("dynamic-sffz")?,
                text("dynamic-fp")?,
                text("dynamic-sfp")?,
                text("dynamic-pf")?,
            ],
            milliseconds: Around::new(localizer, "prefs-playback-ms")?,
            step: Around::new(localizer, "prefs-playback-step")?,
            restore: text("dialog-restore-defaults")?,
        })
    }
}

/// The value `n` of `value` as people read it: a step with its sign (U+2212 for minus), a time in
/// milliseconds with its unit (see the module documentation).
pub(super) fn value_text(texts: &PlaybackTexts, value: TuningValue, n: i64) -> String {
    match value.unit() {
        TuningUnit::Step => {
            let number = match n {
                1.. => format!("+{n}"),
                0 => "0".to_owned(),
                _ => format!("\u{2212}{}", n.unsigned_abs()),
            };
            texts.step.with(&number)
        }
        TuningUnit::Milliseconds => texts.milliseconds.with(&n.to_string()),
    }
}

/// One row of a group: the row name's index in [`PlaybackTexts::rows`] and the value of each
/// column (`None` for an empty cell).
type Row = (usize, &'static [Option<TuningValue>]);

/// One group: its columns' headings' indices in [`PlaybackTexts::columns`], and its rows.
type Group = (&'static [usize], &'static [Row]);

/// The rows of the three groups: (row name's index in [`PlaybackTexts::rows`], the value of each
/// column, `None` for an empty cell), with the columns' headings' indices.
const GROUPS: [Group; 3] = {
    use TuningValue as V;
    [
        (
            &[0, 1],
            &[
                (0, &[Some(V::SoftAccentStep), Some(V::SoftAccentSettle)]),
                (1, &[Some(V::StressStep), Some(V::StressSettle)]),
                (2, &[Some(V::AccentStep), Some(V::AccentSettle)]),
                (3, &[Some(V::MarcatoStep), Some(V::MarcatoSettle)]),
                (4, &[Some(V::UnstressStep), None]),
            ],
        ),
        (
            &[1],
            &[
                (5, &[Some(V::RfSettle)]),
                (6, &[Some(V::SfSettle)]),
                (7, &[Some(V::SffzSettle)]),
            ],
        ),
        (
            &[2, 3],
            &[
                (8, &[Some(V::FpHold), Some(V::FpChange)]),
                (9, &[Some(V::SfpHold), Some(V::SfpChange)]),
                (10, &[Some(V::PfHold), Some(V::PfChange)]),
            ],
        ),
    ]
};

/// The width of a slider in the section, logical pixels: enough for the step of a 10 ms time to
/// be a pixel or more over 0–2000 ms (ADR-0046 V3's fine drag finds every value).
const SLIDER_WIDTH: f32 = 160.0;

/// The width of a value's label, logical pixels: "2000 ms" and "−40" fit, so the columns do not
/// move while a value changes.
const VALUE_WIDTH: f32 = 72.0;

/// Builds the section in `pane` with `tuning`'s values and answers the value labels, in
/// [`TuningValue::ALL`]'s order, for [`value_text`]'s updates.
///
/// # Errors
/// The interface's, if its own tree is inconsistent (never expected).
pub(super) fn build(
    ui: &mut Ui<DemoMessage>,
    pane: WidgetId,
    texts: &PlaybackTexts,
    tuning: &Tuning,
    list: &mut DisplayList,
) -> Result<Vec<WidgetId>, UiError> {
    let fill = LayoutProps::default()
        .width(Length::Fill(1.0))
        .height(Length::Fill(1.0));
    let scroll = ui.add(pane, ScrollArea::vertical(), fill)?;
    let column = ui.add(
        scroll,
        Flex::column(Space::M, Space::None),
        LayoutProps::default().width(Length::Fill(1.0)),
    )?;
    let wide = LayoutProps::default().width(Length::Fill(1.0));
    ui.add(
        column,
        Label::new(texts.intro.clone())
            .color(ColorToken::TextMuted)
            .wrapping(),
        wide,
    )?;
    let mut labels: Vec<Option<WidgetId>> = vec![None; TuningValue::ALL.len()];
    for ((columns, rows), heading) in GROUPS.iter().zip(&texts.groups) {
        ui.add(
            column,
            Label::new(heading.clone()).styled(FamilyToken::Ui, SizeToken::Body, Weight::SemiBold),
            LayoutProps::default(),
        )?;
        let mut tracks = vec![Track::Content];
        tracks.extend(columns.iter().map(|_| Track::Content));
        let grid = ui.add(
            column,
            Grid::new(
                tracks,
                vec![Track::Content; rows.len() + 1],
                Space::S,
                Space::None,
            ),
            LayoutProps::default(),
        )?;
        // The columns' headings on the first row, above their sliders.
        for (c, heading) in (1_u16..).zip(columns.iter()) {
            let name = texts.columns.get(*heading).cloned().unwrap_or_default();
            ui.add(
                grid,
                Label::new(name).color(ColorToken::TextMuted),
                LayoutProps::default().cell(0, c),
            )?;
        }
        for (r, (name, cells)) in (1_u16..).zip(rows.iter()) {
            let name = texts.rows.get(*name).cloned().unwrap_or_default();
            ui.add(
                grid,
                Label::new(name.clone()),
                LayoutProps::default()
                    .cell(r, 0)
                    .align(Align::Start, Align::Center),
            )?;
            for (c, cell) in (1_u16..).zip(cells.iter()) {
                let Some(value) = cell else {
                    continue;
                };
                let header = columns
                    .get(usize::from(c - 1))
                    .and_then(|h| texts.columns.get(*h))
                    .cloned()
                    .unwrap_or_default();
                let label = add_cell(ui, grid, (r, c), *value, tuning, texts, (&name, &header))?;
                if let Some(slot) = labels.get_mut(value_index(*value)) {
                    *slot = Some(label);
                }
            }
        }
    }
    ui.add(
        column,
        Button::new(
            texts.restore.clone(),
            DemoMessage::Preferences(PreferencesMessage::RestorePlayback),
        ),
        LayoutProps::default(),
    )?;
    ui.frame(list);
    Ok(labels.into_iter().flatten().collect())
}

/// The index of `value` in [`TuningValue::ALL`].
fn value_index(value: TuningValue) -> usize {
    TuningValue::ALL
        .iter()
        .position(|known| *known == value)
        .unwrap_or(0)
}

/// One cell: the slider of `value` and its value's label, in the grid's cell (row, column); the
/// slider is named "row: column" for assistive technologies (`names`). Answers the label.
///
/// # Errors
/// The interface's.
fn add_cell(
    ui: &mut Ui<DemoMessage>,
    grid: WidgetId,
    (row, column): (u16, u16),
    value: TuningValue,
    tuning: &Tuning,
    texts: &PlaybackTexts,
    (name, header): (&str, &str),
) -> Result<WidgetId, UiError> {
    let cell = ui.add(
        grid,
        Flex::row(Space::S, Space::None),
        LayoutProps::default().cell(row, column),
    )?;
    let (min, max) = value.range();
    let step = match value.unit() {
        TuningUnit::Step => 1.0,
        TuningUnit::Milliseconds => 10.0,
    };
    let n = tuning.get(value);
    #[allow(
        clippy::cast_precision_loss,
        reason = "values within ±2000, exact in f64"
    )]
    let (min, max, current) = (min as f64, max as f64, n as f64);
    let message = MESSAGES
        .get(value_index(value))
        .copied()
        .unwrap_or(message::<0>);
    // The accessible name joins the row's mark and the column ("Accent", "Back in"), as a
    // screen reader reads a table's cell: both are already translated.
    let accessible = format!("{name}, {header}");
    ui.add(
        cell,
        Slider::new(min, max, step, current, message).labelled(accessible),
        LayoutProps::default().width(Length::Fixed(SLIDER_WIDTH)),
    )?;
    ui.add(
        cell,
        Label::new(value_text(texts, value, n)),
        LayoutProps::default()
            .width(Length::Fixed(VALUE_WIDTH))
            .align(Align::Start, Align::Center),
    )
}

#[cfg(test)]
mod tests {
    //! The values' texts and the messages' table.
    use super::*;

    /// Steps with their sign, times in ms, as en-US writes them.
    #[test]
    fn values_are_written_with_their_unit() {
        let localizer = Localizer::new(orchestra_i18n::Locale::EnUs).unwrap();
        let texts = PlaybackTexts::new(&localizer).unwrap();
        assert_eq!(value_text(&texts, TuningValue::AccentStep, 22), "+22");
        assert_eq!(
            value_text(&texts, TuningValue::UnstressStep, -7),
            "\u{2212}7"
        );
        assert_eq!(value_text(&texts, TuningValue::UnstressStep, 0), "0");
        assert_eq!(value_text(&texts, TuningValue::FpHold, 600), "600\u{a0}ms");
    }

    /// Each message function carries its own index and rounds the slider's value.
    #[test]
    fn each_message_names_its_value() {
        for (i, message) in MESSAGES.iter().enumerate() {
            assert_eq!(
                message(249.99),
                DemoMessage::Preferences(PreferencesMessage::Playback(i, 250))
            );
        }
        assert_eq!(whole(-6.6), -7);
    }
}
