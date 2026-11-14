// SPDX-License-Identifier: GPL-3.0-or-later

//! The demonstration window (ADR-0040 W8; rebuilt with real widgets in F2.09c, choice WD5): what
//! `cargo run -p orchestra-app` shows until the real interface arrives.
//!
//! It is a preview of the **Playback** options, built with the widgets of ADR-0045: a title
//! above a splitter (F2.09d, choice WF8). The left pane, a vertical scroll area, holds a check box
//! (*Metronome*), a switch (*Count-in*), a slider (*Metronome volume*), a text field (*Track
//! name*, F2.10b2) and two buttons (*Restore defaults*, *OK*); the right pane, a scroll area in
//! both directions, a staff with Bravura noteheads, bar numbers and stems (a widget of its own,
//! [`StaffPreview`]), wider than the pane in a small window. The name committed in the field
//! (Enter, or leaving it) is written above the staff, as a score names its instrument: it shows
//! the commit of ADR-0047 TF7 at work, and Escape bringing the old name back. Every text comes from the catalogues ([`DemoTexts`],
//! CLAUDE.md §7). It exists to check by eye, and with a screen reader (AccessKit):
//! - the widgets' looks in every theme and density, the focus ring, Tab order;
//! - scrolling (wheel, bars, keyboard, a hidden control brought into view by Tab) and moving the
//!   divider between the panes;
//! - activation by pointer, keyboard and assistive technology;
//! - the text field: typing, the platform's editing keys, the selection by keyboard and pointer,
//!   the blinking caret, scrolling a long name inside the field, commit and Escape;
//! - text, symbols and crisp lines at every scale factor;
//! - following the system's theme while the window is open (ADR-0038).
//!
//! The options do nothing yet (playback arrives in F3); *Restore defaults* puts the controls back
//! to their default values, to show the application changing widgets itself (ADR-0045 WA7).
//! Nothing here is business logic (the crate's rule): it is a throw-away view.

use std::time::Instant;

use orchestra_i18n::{I18nError, Localizer};
use orchestra_text::{FaceId, Faces, GlyphBox, GlyphId, TextLayout};
use orchestra_ui::{
    AccessAction, AccessUpdate, Align, Axis, Button, ButtonKind, ColorToken, Constraints,
    DisplayList, FamilyToken, Flex, InputEvent, Label, LayoutCx, LayoutProps, Length, Point,
    PointerShape, RecordCx, Recorder, Rect, ScrollArea, Size, SizeToken, Slider, Space, Splitter,
    StrokeStyle, TextField, Theme, Toggle, Ui, Weight, Widget, WidgetId, WindowApp,
};

/// The messages of the demonstration's widgets (ADR-0045 WA7).
#[derive(Debug, Clone, PartialEq)]
pub enum DemoMessage {
    /// The metronome check box changed.
    Metronome(bool),
    /// The count-in switch changed.
    CountIn(bool),
    /// The metronome volume changed (0–100).
    Volume(f64),
    /// The track's name was committed in its field (F2.10b2).
    TrackName(String),
    /// *Restore defaults* was pressed.
    RestoreDefaults,
    /// *OK* was pressed.
    Ok,
}

/// The texts of the demonstration, from the catalogues (CLAUDE.md §7).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DemoTexts {
    /// `playback-title`.
    pub title: String,
    /// `playback-metronome`.
    pub metronome: String,
    /// `playback-count-in`.
    pub count_in: String,
    /// `playback-metronome-volume`.
    pub volume: String,
    /// `playback-track-name`.
    pub track_name: String,
    /// `playback-track-name-placeholder`.
    pub track_name_placeholder: String,
    /// `dialog-restore-defaults`.
    pub restore_defaults: String,
    /// `dialog-ok`.
    pub ok: String,
}

impl DemoTexts {
    /// The texts in `localizer`'s language.
    ///
    /// # Errors
    /// The localizer's error if a message is missing (the catalogues' parity tests prevent it).
    pub fn new(localizer: &Localizer) -> Result<Self, I18nError> {
        Ok(Self {
            title: localizer.format("playback-title", None)?,
            metronome: localizer.format("playback-metronome", None)?,
            count_in: localizer.format("playback-count-in", None)?,
            volume: localizer.format("playback-metronome-volume", None)?,
            track_name: localizer.format("playback-track-name", None)?,
            track_name_placeholder: localizer.format("playback-track-name-placeholder", None)?,
            restore_defaults: localizer.format("dialog-restore-defaults", None)?,
            ok: localizer.format("dialog-ok", None)?,
        })
    }
}

/// The default values the controls start from and *Restore defaults* brings back.
const DEFAULT_METRONOME: bool = true;
/// See [`DEFAULT_METRONOME`].
const DEFAULT_COUNT_IN: bool = false;
/// See [`DEFAULT_METRONOME`] (percent).
const DEFAULT_VOLUME: f64 = 80.0;

/// The demonstration view: its interface and the IDs of the widgets it changes.
#[derive(Debug)]
pub struct Demo {
    /// The widgets.
    ui: Ui<DemoMessage>,
    /// The title label.
    title: WidgetId,
    /// The metronome check box.
    metronome: WidgetId,
    /// The count-in switch.
    count_in: WidgetId,
    /// The volume slider.
    volume: WidgetId,
    /// The track name's field.
    track_name: WidgetId,
    /// The staff, which shows the committed name.
    staff: WidgetId,
}

impl Demo {
    /// The demonstration in `theme`'s density, text and shape tokens, with `texts`, its window
    /// named `window_title` for assistive technologies, its texts laid out with `faces`.
    ///
    /// # Why the theme is given here
    /// `WindowApp` does not receive the window's theme yet (who owns the widgets' theme is decided
    /// with the Preferences, F2.12); the colours, which follow the system, are applied by the
    /// renderer anyway (ADR-0039 D3), so only density, text and shapes come from here.
    #[must_use]
    pub fn new(faces: &Faces, theme: &Theme, window_title: &str, texts: &DemoTexts) -> Self {
        let mut ui = Ui::new(Flex::column(Space::M, Space::L), theme, faces.clone());
        ui.set_title(window_title);
        let root = ui.root();
        let content = LayoutProps::default();
        // `add` fails only for an unknown parent; every parent here was just created, so a
        // failure would leave the demonstration incomplete, never crash it.
        let title = ui
            .add(
                root,
                Label::new(texts.title.clone()).styled(
                    FamilyToken::Ui,
                    SizeToken::Heading,
                    Weight::Bold,
                ),
                content,
            )
            .unwrap_or_else(|_| ui.root());
        let (options, staff_area) = panes(&mut ui);
        let metronome = ui
            .add(
                options,
                Toggle::checkbox(
                    texts.metronome.clone(),
                    DEFAULT_METRONOME,
                    DemoMessage::Metronome,
                ),
                content,
            )
            .unwrap_or_else(|_| ui.root());
        let count_in = ui
            .add(
                options,
                Toggle::switch(
                    texts.count_in.clone(),
                    DEFAULT_COUNT_IN,
                    DemoMessage::CountIn,
                ),
                content,
            )
            .unwrap_or_else(|_| ui.root());
        let volume_row = ui
            .add(
                options,
                Flex::row(Space::M, Space::None),
                content.width(Length::Fill(1.0)),
            )
            .unwrap_or_else(|_| ui.root());
        let _ = ui.add(
            volume_row,
            Label::new(texts.volume.clone()),
            content.align(Align::Start, Align::Center),
        );
        let volume = ui
            .add(
                volume_row,
                Slider::new(0.0, 100.0, 1.0, DEFAULT_VOLUME, DemoMessage::Volume)
                    .labelled(texts.volume.clone()),
                content.width(Length::Fill(1.0)),
            )
            .unwrap_or_else(|_| ui.root());
        let track_name = track_name_row(&mut ui, options, texts);
        let buttons = ui
            .add(options, Flex::row(Space::S, Space::None), content)
            .unwrap_or_else(|_| ui.root());
        let _ = ui.add(
            buttons,
            Button::new(texts.restore_defaults.clone(), DemoMessage::RestoreDefaults),
            content,
        );
        let _ = ui.add(
            buttons,
            Button::new(texts.ok.clone(), DemoMessage::Ok).kind(ButtonKind::Primary),
            content,
        );
        let staff = ui
            .add(
                staff_area,
                StaffPreview::new(faces),
                content
                    .width(Length::Fixed(480.0))
                    .height(Length::Fixed(110.0)),
            )
            .unwrap_or_else(|_| ui.root());
        Self {
            ui,
            title,
            metronome,
            count_in,
            volume,
            track_name,
            staff,
        }
    }

    /// The interface, to read its widgets (tests, screenshots).
    #[must_use]
    pub const fn ui(&self) -> &Ui<DemoMessage> {
        &self.ui
    }

    /// The title label.
    #[must_use]
    pub const fn title(&self) -> WidgetId {
        self.title
    }

    /// The track name's field.
    #[must_use]
    pub const fn track_name(&self) -> WidgetId {
        self.track_name
    }

    /// The staff preview.
    #[must_use]
    pub const fn staff(&self) -> WidgetId {
        self.staff
    }

    /// Acts on the widgets' messages (ADR-0045 WA7): *Restore defaults* puts the options back,
    /// a committed track name is written above the staff.
    fn handle_messages(&mut self, list: &mut DisplayList) {
        let mut restore = false;
        let mut name = None;
        for message in self.ui.take_messages() {
            match message {
                DemoMessage::RestoreDefaults => restore = true,
                DemoMessage::TrackName(text) => name = Some(text),
                _ => {}
            }
        }
        if let Some(name) = name {
            if let Some(staff) = self.ui.widget_mut::<StaffPreview>(self.staff) {
                staff.set_name(&name);
            }
            self.ui.request_layout(self.staff);
            self.ui.frame(list);
        }
        if restore {
            if let Some(check) = self.ui.widget_mut::<Toggle<DemoMessage>>(self.metronome) {
                check.set_on(DEFAULT_METRONOME);
            }
            if let Some(switch) = self.ui.widget_mut::<Toggle<DemoMessage>>(self.count_in) {
                switch.set_on(DEFAULT_COUNT_IN);
            }
            if let Some(slider) = self.ui.widget_mut::<Slider<DemoMessage>>(self.volume) {
                slider.set_value(DEFAULT_VOLUME);
            }
            for id in [self.metronome, self.count_in, self.volume] {
                self.ui.request_record(id);
            }
            self.ui.frame(list);
        }
    }
}

/// The *Track name* row of the options (F2.10b2): a label and a text field filling the rest of
/// the row, with its placeholder and its name for assistive technologies. Returns the field.
///
/// As in [`Demo::new`], `add` fails only for an unknown parent, and every parent here was just
/// created: a failure would leave the demonstration incomplete, never crash it.
fn track_name_row(ui: &mut Ui<DemoMessage>, options: WidgetId, texts: &DemoTexts) -> WidgetId {
    let content = LayoutProps::default();
    let name_row = ui
        .add(
            options,
            Flex::row(Space::M, Space::None),
            content.width(Length::Fill(1.0)),
        )
        .unwrap_or_else(|_| ui.root());
    let _ = ui.add(
        name_row,
        Label::new(texts.track_name.clone()),
        content.align(Align::Start, Align::Center),
    );
    ui.add(
        name_row,
        TextField::new("", |text| DemoMessage::TrackName(text.to_owned()))
            .placeholder(texts.track_name_placeholder.clone())
            .labelled(texts.track_name.clone()),
        content.width(Length::Fill(1.0)),
    )
    .unwrap_or_else(|_| ui.root())
}

/// The splitter under the title, filling the rest of the window (F2.09d, WF8): a vertical scroll
/// area on the left (a little less than half) holding the options' column, and a scroll area in
/// both directions on the right for the staff. Returns (the options' column, the staff's area).
///
/// As in [`Demo::new`], `add` fails only for an unknown parent, and every parent here was just
/// created: a failure would leave the demonstration incomplete, never crash it.
fn panes(ui: &mut Ui<DemoMessage>) -> (WidgetId, WidgetId) {
    let root = ui.root();
    let fill = LayoutProps::default()
        .width(Length::Fill(1.0))
        .height(Length::Fill(1.0));
    // The splitter under the title, filling the rest of the window: the options on the left
    // (a little less than half), the staff on the right.
    let splitter = ui
        .add(root, Splitter::new(Axis::Horizontal, 0.45), fill)
        .unwrap_or_else(|_| ui.root());
    let options_area = ui
        .add(splitter, ScrollArea::vertical(), fill)
        .unwrap_or_else(|_| ui.root());
    let staff_area = ui
        .add(splitter, ScrollArea::both(), fill)
        .unwrap_or_else(|_| ui.root());
    // The options' column, as wide as its pane (the volume row stretches the slider), with a
    // small padding: the viewport clips its content, and a focus ring drawn outside a control at
    // the column's edge would be cut (the rule of `ScrollArea`'s documentation).
    let options = ui
        .add(
            options_area,
            Flex::column(Space::M, Space::S),
            LayoutProps::default().width(Length::Fill(1.0)),
        )
        .unwrap_or_else(|_| ui.root());
    (options, staff_area)
}

impl WindowApp for Demo {
    fn layout(&mut self, bounds: Rect, list: &mut DisplayList) {
        self.ui.layout(bounds, list);
    }

    fn input(&mut self, event: &InputEvent, list: &mut DisplayList) {
        self.ui.input(event, list);
        self.handle_messages(list);
    }

    fn access_update(&mut self, full: bool) -> Option<AccessUpdate> {
        if full {
            self.ui.reset_access();
        }
        Some(self.ui.take_access_update())
    }

    fn access_action(&mut self, action: AccessAction, list: &mut DisplayList) {
        // An action on a widget that no longer exists is ignored: the assistive technology will
        // get the current tree with the next update.
        let _ = self.ui.access_action(action, list);
        self.handle_messages(list);
    }

    fn pointer_shape(&self) -> PointerShape {
        // The interface decides (F2.09e): the resize shape over the splitter's divider, the text
        // beam over the field, the arrow elsewhere.
        self.ui.pointer_shape()
    }

    fn advance(&mut self, now: Instant, list: &mut DisplayList) {
        // The field's caret blinks with the interface's wake-ups (F2.10b2).
        self.ui.advance(now, list);
    }

    fn next_wakeup(&self) -> Option<Instant> {
        self.ui.next_wakeup()
    }
}

/// The staff of the demonstration: five crisp lines, three bars of Bravura noteheads with stems,
/// barlines and tabular bar numbers (text and symbols from the glyph cache, ADR-0044).
#[derive(Debug)]
pub struct StaffPreview {
    /// Bravura's black notehead and its own box at 40 px per em, if the font has it.
    notehead: Option<(GlyphId, GlyphBox)>,
    /// The bar numbers 1, 2, 3, laid out.
    numbers: [TextLayout; 3],
    /// The text generation the numbers and the name were laid out with.
    laid_out: Option<u64>,
    /// The track's name written above the staff (F2.10b2): what the field committed, the user's
    /// own text (nothing to translate). Empty: none.
    name: String,
    /// The name laid out.
    name_layout: TextLayout,
}

/// The staff space, logical pixels: Bravura at 40 px per em (an em is four staff spaces).
const SPACE: f32 = 10.0;

/// Notes per bar of the preview.
const NOTES_PER_BAR: u8 = 4;

impl StaffPreview {
    /// The preview, finding the notehead in `faces`.
    #[must_use]
    pub fn new(faces: &Faces) -> Self {
        let notehead = faces
            .glyph_for_char(FaceId::Bravura, '\u{E0A4}')
            .and_then(|glyph| {
                // The notehead's own box, not the face's (ADR-0044 TD2).
                let own = faces
                    .glyph_box(FaceId::Bravura, glyph, 4.0 * SPACE)
                    .ok()
                    .flatten()?;
                Some((glyph, own))
            });
        Self {
            notehead,
            numbers: [TextLayout::new(), TextLayout::new(), TextLayout::new()],
            laid_out: None,
            name: String::new(),
            name_layout: TextLayout::new(),
        }
    }

    /// Writes `name` above the staff; the caller asks the interface to lay the staff out again.
    pub fn set_name(&mut self, name: &str) {
        if name != self.name {
            name.clone_into(&mut self.name);
            self.laid_out = None;
        }
    }

    /// The name written above the staff.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
}

impl<M: 'static> Widget<M> for StaffPreview {
    fn measure(&mut self, cx: &mut LayoutCx<'_, M>, constraints: Constraints) -> Size {
        let generation = cx.text_generation();
        if self.laid_out != Some(generation) {
            let style = (FamilyToken::Numeric, SizeToken::Caption, Weight::Regular);
            for (number, layout) in ["1", "2", "3"].into_iter().zip(&mut self.numbers) {
                if cx.lay_out_text(number, style, None, layout).is_err() {
                    *layout = TextLayout::new();
                }
            }
            let style = (FamilyToken::Ui, SizeToken::Body, Weight::Bold);
            if cx
                .lay_out_text(&self.name, style, None, &mut self.name_layout)
                .is_err()
            {
                self.name_layout = TextLayout::new();
            }
            self.laid_out = Some(generation);
        }
        constraints.constrain(Size::new(480.0, 110.0))
    }

    fn record(&self, cx: &RecordCx, recorder: &mut Recorder<'_>) {
        let left = cx.rect.x0;
        let top = cx.rect.y0 + 40.0;
        let bar_width = f32::from(NOTES_PER_BAR) * 34.0;
        recorder.push_clip_rect(cx.rect);
        // The track's name at the top left, above the bar numbers (empty: nothing drawn).
        recorder.text(
            &self.name_layout,
            Point::new(left, cx.rect.y0),
            ColorToken::Text,
        );
        for line in 0..5_u8 {
            let y = top + f32::from(line) * SPACE;
            recorder.line(
                Point::new(left, y),
                Point::new(left + 440.0, y),
                StrokeStyle::new(1.0),
                ColorToken::Text,
            );
        }
        for (bar, number) in (0_u8..).zip(&self.numbers) {
            let start = left + 14.0 + f32::from(bar) * bar_width;
            let barline = start + bar_width;
            recorder.line(
                Point::new(barline, top),
                Point::new(barline, top + 4.0 * SPACE),
                StrokeStyle::new(1.0),
                ColorToken::Text,
            );
            let above = top - 6.0 - number.height();
            recorder.text(
                number,
                Point::new(start + 2.0, above),
                ColorToken::TextMuted,
            );
        }
        let notes = (0..3 * NOTES_PER_BAR).map(|note| {
            let x = left + 24.0 + f32::from(note) * 34.0;
            let y = top + 4.0 * SPACE - f32::from(note % 9) * SPACE * 0.5;
            (x, y)
        });
        if let Some((glyph, own)) = self.notehead {
            recorder.glyphs(
                FaceId::Bravura,
                4.0 * SPACE,
                own,
                notes.clone().map(|(x, y)| (glyph, Point::new(x, y))),
                ColorToken::Text,
            );
        }
        // Stems up from the notehead's right side (SMuFL's stemUpSE anchor: 1.18 spaces right).
        for (x, y) in notes {
            let stem = x + 1.18 * SPACE - 0.5;
            recorder.line(
                Point::new(stem, y - 0.168 * SPACE),
                Point::new(stem, y - 3.5 * SPACE),
                StrokeStyle::new(1.0),
                ColorToken::Text,
            );
        }
        recorder.pop_clip();
    }
}
