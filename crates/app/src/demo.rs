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
//! - the text field's context menu (F2.11d2a): the program's editing items, opened by the
//!   secondary button, the Menu key or Shift+F10, with their shortcuts written in the platform's
//!   style, acting on the field;
//! - text, symbols and crisp lines at every scale factor;
//! - following the system's theme while the window is open (ADR-0038).
//!
//! Its window holds the **open project** (F2.12a5b, [`crate::project`]): the title names it, the
//! File menu's New, Open, Save and Save as act on it with the system's file chooser, and Quit or
//! the window's close button ask about unsaved changes first. Nothing in the demonstration changes
//! the document yet (the views arrive in F2.21); the tests change it through the editor.
//!
//! The options do nothing yet (playback arrives in F3); *Restore defaults* puts the controls back
//! to their default values, to show the application changing widgets itself (ADR-0045 WA7).
//! Nothing here is business logic (the crate's rule): it is a throw-away view.

use std::collections::VecDeque;
use std::time::Instant;

use orchestra_i18n::{I18nError, Localizer};
use orchestra_text::{FaceId, Faces, GlyphBox, GlyphId, TextLayout};
use orchestra_ui::{
    AccessAction, AccessUpdate, Align, Axis, Button, ButtonKind, ClipboardSource, ColorToken,
    Constraints, DisplayList, FamilyToken, Flex, InputEvent, Label, LayoutCx, LayoutProps, Length,
    Point, PointerShape, RecordCx, Recorder, Rect, ScrollArea, Size, SizeToken, Slider, Space,
    Splitter, StrokeStyle, TextField, TextInput, Theme, Toggle, Ui, Weight, Widget, WidgetId,
    WindowApp, WindowRequest,
};
use orchestra_ui::{
    ActionId, ActionState, Decorations, FileChoice, FileRequest, MenuModel, Platform, SystemMenu,
    SystemTexts, TitleBarSpec, WindowButtonNames,
};

use crate::about::{AboutTexts, about_dialog};
use crate::actions::{self, ActionIds, ActionTexts};
use crate::files::{
    FileTexts, ProjectTexts, open_request, project_error_dialog, replace_dialog,
    save_changes_dialog, save_request, unavailable_dialog, window_title,
};
use crate::preferences::{
    self, Effect, KeyboardMessage, KeyboardOutcome, PreferencesMessage, PreferencesTexts,
    PreferencesView, ShortcutEditor, ThemeEditor, ThemeMessage, ThemeOutcome,
};
use crate::project::{OpenProject, ProjectEffect, ProjectFlow, SaveAnswer};
use crate::settings::SettingsStore;
use crate::settings::convert::{
    SettingsProblem, apply_settings_shortcuts, fine_modifier_from_settings,
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
    /// An action ran from its shortcut (ADR-0048 AM4, F2.11d1).
    Action(ActionId),
    /// The About dialog's *Close* was pressed (F2.12a3b): the dialog closes by itself, nothing
    /// else to do.
    AboutClosed,
    /// A control of the Preferences changed, or they closed (F2.12a4a).
    Preferences(PreferencesMessage),
    /// The *OK* of the error "Files can't be chosen" was pressed (F2.12a5a): the dialog closes
    /// by itself, nothing else to do.
    FileErrorClosed,
    /// An answer to "Save the changes?" (F2.12a5b, ADR-0050 DG5).
    SaveChanges(SaveAnswer),
    /// An answer to "Replace it?" (F2.12a5b): `true` for *Replace*.
    Replace(bool),
    /// The *OK* of an error of the open project (F2.12a5b): the dialog closes by itself.
    ProjectErrorClosed,
    /// The shortcut editor of the Preferences' Keyboard section (F2.12c1).
    Keyboard(KeyboardMessage),
    /// The theme editor of the Preferences' Appearance section (F2.12b1).
    Theme(ThemeMessage),
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
    /// The actions' and the keys' names, for the registry (F2.11d2a).
    pub actions: ActionTexts,
    /// The names of macOS's own menu items (F2.11e2); unused on Linux and Windows.
    pub system: SystemTexts,
    /// The names of our title bar's window buttons (F2.11b2), used only where the system draws
    /// no frame.
    pub window_buttons: WindowButtonNames,
    /// The About dialog's texts (F2.12a3b).
    pub about: AboutTexts,
    /// The Preferences' texts (F2.12a4a).
    pub preferences: PreferencesTexts,
    /// The file choosers' texts (F2.12a5a).
    pub files: FileTexts,
    /// The open project's texts: the window title, the questions, the errors (F2.12a5b).
    pub project: ProjectTexts,
}

impl DemoTexts {
    /// The texts in `localizer`'s language.
    ///
    /// # Errors
    /// The localizer's error if a message is missing (the catalogues' parity tests prevent it).
    pub fn new(localizer: &Localizer) -> Result<Self, I18nError> {
        // The actions' texts first: the Preferences write the modifiers' names with their keys.
        let actions = actions::action_texts(localizer)?;
        Ok(Self {
            title: localizer.format("playback-title", None)?,
            metronome: localizer.format("playback-metronome", None)?,
            count_in: localizer.format("playback-count-in", None)?,
            volume: localizer.format("playback-metronome-volume", None)?,
            track_name: localizer.format("playback-track-name", None)?,
            track_name_placeholder: localizer.format("playback-track-name-placeholder", None)?,
            restore_defaults: localizer.format("dialog-restore-defaults", None)?,
            ok: localizer.format("dialog-ok", None)?,
            actions: actions.clone(),
            system: actions::system_texts(localizer)?,
            window_buttons: WindowButtonNames {
                close: localizer.format("window-close", None)?,
                maximize: localizer.format("window-maximize", None)?,
                restore: localizer.format("window-restore", None)?,
                minimize: localizer.format("window-minimize", None)?,
            },
            about: AboutTexts::new(localizer)?,
            preferences: PreferencesTexts::new(localizer, Platform::CURRENT, actions.keys())?,
            files: FileTexts::new(localizer)?,
            project: ProjectTexts::new(localizer)?,
        })
    }
}

/// What macOS's system menu bar is built from (F2.11e1, F2.11e2): kept by the demonstration on
/// macOS only, `None` elsewhere (the drawn bar is used there).
#[derive(Debug)]
struct MacMenu {
    /// The menus: the same model as the drawn bar of Linux and Windows.
    model: MenuModel,
    /// The program's name, the title of the application menu.
    app_name: String,
    /// The names of macOS's own menu items.
    texts: SystemTexts,
}

impl MacMenu {
    /// On macOS, with valid actions: the bar's model, `app_name` and the system's names of
    /// `texts`. Elsewhere, or without actions, `None`.
    fn new(actions: Option<ActionIds>, texts: &DemoTexts, app_name: &str) -> Option<Self> {
        if Platform::CURRENT != Platform::MacOs {
            return None;
        }
        let ids = actions?;
        Some(Self {
            model: actions::menu_bar(&ids, &texts.actions),
            app_name: app_name.to_owned(),
            texts: texts.system.clone(),
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
    /// The *Restore defaults* button (its tooltip is the screenshots' example, F2.11c).
    restore_defaults: WidgetId,
    /// The menu bar (F2.11d2b), on Linux and Windows.
    menu_bar: Option<WidgetId>,
    /// What the system's bar needs (macOS only, F2.11e1), handed to the window when it asks.
    mac_menu: Option<MacMenu>,
    /// The program's actions (F2.11d1), `None` if their tables were wrong (the tests prevent it).
    actions: Option<ActionIds>,
    /// What the actions asked of the window, waiting for it ([`WindowApp::take_window_request`]).
    requests: VecDeque<WindowRequest>,
    /// The About dialog's texts, for `help.about` (F2.12a3b).
    about: AboutTexts,
    /// The Preferences' texts, for `edit.preferences` (F2.12a4a).
    preferences_texts: PreferencesTexts,
    /// The open Preferences, while they are open.
    preferences: Option<PreferencesView>,
    /// The labels of the playback values while the Playback section is shown, in
    /// `TuningValue::ALL`'s order (F2.12a4b2); empty otherwise.
    playback_labels: Vec<WidgetId>,
    /// The shortcut editor while the Keyboard section is shown (F2.12c1).
    shortcut_editor: Option<ShortcutEditor>,
    /// Who asked for the file chooser that is open (F2.12c2): its answer goes back there.
    chooser: Option<ChooserOwner>,
    /// The theme editor while the Appearance section is shown (F2.12b1).
    theme_editor: Option<ThemeEditor>,
    /// The settings of the session (ADR-0050 DG6): the program hands its store in with
    /// [`Demo::attach_settings`]; until then (tests, screenshots) defaults that are never written.
    settings: SettingsStore,
    /// The file choosers' texts, for *Open* and *Save as* (F2.12a5a).
    files: FileTexts,
    /// The open project's texts (F2.12a5b).
    project_texts: ProjectTexts,
    /// The open project and its flow (F2.12a5b, ADR-0050 DG5): `None` only if the new document
    /// could not be made, which its constant values never cause (the tests check it); the file
    /// actions then do nothing, as the actions do without their tables.
    flow: Option<ProjectFlow>,
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
        Self::with_decorations(faces, theme, window_title, texts, Decorations::System, None)
    }

    /// [`Demo::new`] for a window whose frame `decorations` draws (F2.11b2, ADR-0049 TB1): with
    /// [`Decorations::Own`] the menus go into our own title bar, with the window's buttons,
    /// instead of a menu bar under the system's frame. The window must be opened with the same
    /// answer ([`WindowOptions::decorations`](orchestra_ui::WindowOptions)). `button_layout` is
    /// GNOME's setting as [`orchestra_ui::button_layout`] read it (`None`: only Close); the
    /// program reads it, so the tests and the screenshots never depend on the desktop.
    #[must_use]
    pub fn with_decorations(
        faces: &Faces,
        theme: &Theme,
        window_title: &str,
        texts: &DemoTexts,
        decorations: Decorations,
        button_layout: Option<&str>,
    ) -> Self {
        // The menu bar (F2.11d2b), or our title bar holding it (F2.11b2), above the content.
        let mut ui = Ui::new(Flex::column(Space::None, Space::None), theme, faces.clone());
        ui.set_title(window_title);
        let content = LayoutProps::default();
        let frame = Frame::new(texts, window_title, decorations, button_layout);
        let (actions, menu_bar, root) = frame_window(&mut ui, &frame);
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
        let (options, staff_area) = panes(&mut ui, root);
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
        let track_name = track_name_row(&mut ui, options, texts, actions.as_ref());
        let restore_defaults = buttons_row(&mut ui, options, texts);
        let staff = ui
            .add(
                staff_area,
                StaffPreview::new(faces),
                content
                    .width(Length::Fixed(480.0))
                    .height(Length::Fixed(110.0)),
            )
            .unwrap_or_else(|_| ui.root());
        let mut demo = Self {
            ui,
            actions,
            requests: VecDeque::new(),
            about: texts.about.clone(),
            preferences_texts: texts.preferences.clone(),
            preferences: None,
            playback_labels: Vec::new(),
            shortcut_editor: None,
            chooser: None,
            theme_editor: None,
            settings: SettingsStore::with(orchestra_io::settings::Settings::default(), None),
            files: texts.files.clone(),
            project_texts: texts.project.clone(),
            flow: OpenProject::new().ok().map(ProjectFlow::new),
            title,
            metronome,
            count_in,
            volume,
            track_name,
            staff,
            restore_defaults,
            menu_bar,
            mac_menu: MacMenu::new(actions, texts, window_title),
        };
        // The window is named after the project from the start: "Untitled — Orchestra".
        demo.refresh_title();
        demo
    }

    /// The program's actions (F2.11d1), if their tables were valid.
    #[must_use]
    pub const fn actions(&self) -> Option<ActionIds> {
        self.actions
    }

    /// The interface, to read its widgets (tests, screenshots).
    #[must_use]
    pub const fn ui(&self) -> &Ui<DemoMessage> {
        &self.ui
    }

    /// The interface, to drive it from the screenshots' scenes and the tests (focus a widget,
    /// open a menu).
    pub const fn ui_mut(&mut self) -> &mut Ui<DemoMessage> {
        &mut self.ui
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

    /// The menu bar (F2.11d2b): `None` on macOS, where the system's bar comes with F2.11e.
    #[must_use]
    pub const fn menu_bar(&self) -> Option<WidgetId> {
        self.menu_bar
    }

    /// The *Restore defaults* button.
    #[must_use]
    pub const fn restore_defaults(&self) -> WidgetId {
        self.restore_defaults
    }

    /// Hands the program's settings store in (ADR-0050 DG6, F2.12a4a): the Preferences change
    /// it, and the window wakes to write it ([`WindowApp::next_wakeup`]). What it holds applies at
    /// once (F2.12a4b1): the cursor's blink, the modifier of fine adjustment and the user's
    /// shortcuts (ADR-0048 AM7; the menus and their shortcuts' texts follow the registry). Its
    /// theme was given to the window when it opened, its language to the texts before this.
    /// Answers what in the settings could not be used (a shortcut that cannot be read, a conflict),
    /// for the program to report (choice S4 of F2.12a2: the standard error, no log yet).
    pub fn attach_settings(&mut self, store: SettingsStore) -> Vec<SettingsProblem> {
        let settings = store.settings();
        self.ui.set_caret_blink(settings.caret_blink);
        self.ui
            .set_fine_modifier(fine_modifier_from_settings(settings.fine_modifier));
        let mut problems = Vec::new();
        // The playback values are read once here only to report the ones out of range: the
        // tuning itself is read from the settings when needed (`Demo::tuning`).
        let _ = crate::settings::convert::tuning_from_settings(&settings.playback, &mut problems);
        if let Some(registry) = self.ui.actions_mut() {
            apply_settings_shortcuts(&settings.shortcuts, registry, &mut problems);
        }
        self.settings = store;
        problems
    }

    /// Takes the settings store back (at exit, to close it: `SettingsStore::close`), leaving
    /// defaults that are never written.
    pub fn take_settings(&mut self) -> SettingsStore {
        std::mem::replace(
            &mut self.settings,
            SettingsStore::with(orchestra_io::settings::Settings::default(), None),
        )
    }

    /// The settings of the session.
    #[must_use]
    pub const fn settings(&self) -> &SettingsStore {
        &self.settings
    }

    /// The playback's tuning the settings choose now (ADR-0027 §14–§15, F2.12a4b2): what the
    /// performance will be computed with when the program plays (F3). Values out of range are
    /// clamped (and were reported when the settings were handed in).
    #[must_use]
    pub fn tuning(&self) -> orchestra_playback::Tuning {
        crate::settings::convert::tuning_from_settings(
            &self.settings.settings().playback,
            &mut Vec::new(),
        )
    }

    /// The open project and its flow (F2.12a5b): `None` only if the new document could not be
    /// made (never, see the field).
    #[must_use]
    pub const fn project(&self) -> Option<&ProjectFlow> {
        self.flow.as_ref()
    }

    /// The open project and its flow, to change the document (the tests: nothing in the
    /// demonstration changes it yet, F2.21). Call [`Demo::refresh_title`] after a change.
    pub const fn project_mut(&mut self) -> Option<&mut ProjectFlow> {
        self.flow.as_mut()
    }

    /// The window title of the open project now: `Name — Orchestra`, with the modification mark
    /// while it has unsaved changes (ADR-0050 DG5).
    #[must_use]
    pub fn window_title(&self) -> &str {
        self.ui.title()
    }

    /// Gives the interface the open project's title (see [`Demo::window_title`]): the window, our
    /// title bar and the accessibility tree follow it, and nothing happens when it did not
    /// change. Called after every step of the flow.
    pub fn refresh_title(&mut self) {
        if let Some(flow) = self.flow.as_ref() {
            let project = flow.project();
            let title = window_title(
                &self.project_texts,
                project.name().as_deref(),
                project.is_modified(),
            );
            self.ui.set_title(&title);
        }
    }

    /// Runs one step of the open project's flow (F2.12a5b), then shows what it answered and
    /// updates the title. Nothing without a flow.
    fn project_step(
        &mut self,
        step: impl FnOnce(&mut ProjectFlow) -> Option<ProjectEffect>,
        list: &mut DisplayList,
    ) {
        let effect = self.flow.as_mut().and_then(step);
        if let Some(effect) = effect {
            self.show_effect(effect, list);
        }
        self.refresh_title();
    }

    /// Shows what a step of the flow asked for (see `crate::project`): a question, a chooser, an
    /// error, or closing the window. `open_dialog` and `choose_file` fail only if the interface's
    /// own tree is inconsistent; then nothing is shown, and the flow waits for an answer that
    /// never comes, a state the tests rule out.
    fn show_effect(&mut self, effect: ProjectEffect, list: &mut DisplayList) {
        match effect {
            ProjectEffect::AskToSave { name } => {
                let spec = save_changes_dialog(
                    &self.project_texts,
                    name.as_deref(),
                    DemoMessage::SaveChanges(SaveAnswer::Save),
                    DemoMessage::SaveChanges(SaveAnswer::Discard),
                    DemoMessage::SaveChanges(SaveAnswer::Cancel),
                );
                let _ = self.ui.open_dialog(spec, list);
            }
            ProjectEffect::ChooseOpen => {
                self.choose(ChooserOwner::Project, open_request(&self.files), list);
            }
            ProjectEffect::ChooseSave { name } => {
                let request = save_request(&self.files, name.as_deref());
                self.choose(ChooserOwner::Project, request, list);
            }
            ProjectEffect::ConfirmReplace { path } => {
                let spec = replace_dialog(
                    &self.project_texts,
                    &path,
                    DemoMessage::Replace(true),
                    DemoMessage::Replace(false),
                );
                let _ = self.ui.open_dialog(spec, list);
            }
            ProjectEffect::Failed(error) => {
                let spec = project_error_dialog(
                    &self.project_texts,
                    &error,
                    DemoMessage::ProjectErrorClosed,
                );
                let _ = self.ui.open_dialog(spec, list);
            }
            // The project was put away: the window closes (Quit, or the answer to the question).
            ProjectEffect::Close => self.requests.push_back(WindowRequest::Close),
        }
    }

    /// The shortcut editor, while the Preferences show their Keyboard section (F2.12c1).
    #[must_use]
    pub const fn shortcut_editor(&self) -> Option<&ShortcutEditor> {
        self.shortcut_editor.as_ref()
    }

    /// The open Preferences' panel, while they are open.
    #[must_use]
    pub fn preferences(&self) -> Option<WidgetId> {
        self.preferences.map(|view| view.panel)
    }

    /// A change in the Preferences (DG8): written into the settings (the store writes them half a
    /// second later, DG6) and applied at once: a theme to the window (its colours) and to the
    /// interface (density, text and shape tokens), the cursor's blink to the interface.
    fn preference_changed(&mut self, message: PreferencesMessage, list: &mut DisplayList) {
        if message == PreferencesMessage::Closed {
            self.preferences = None;
            self.playback_labels.clear();
            self.shortcut_editor = None;
            self.theme_editor = None;
            return;
        }
        let chose_theme = matches!(message, PreferencesMessage::Theme(_));
        let now = self.ui.now();
        let mut effect = Effect::None;
        self.settings.update(now, |settings| {
            effect = preferences::apply(settings, message);
        });
        match effect {
            Effect::None => {}
            Effect::Theme(pair, mode) => {
                self.apply_theme(pair, mode, list);
                // Another theme chosen in the drop-down: the theme editor shows it (F2.12b1).
                if chose_theme && let Some(editor) = self.theme_editor.as_mut() {
                    editor.chosen_changed(
                        &mut self.ui,
                        &self.preferences_texts.theme_editor,
                        &self.preferences_texts.pairs,
                        self.settings.settings(),
                        list,
                    );
                }
            }
            Effect::CaretBlink(on) => self.ui.set_caret_blink(on),
            Effect::FineModifier(fine) => self.ui.set_fine_modifier(fine),
            Effect::Section(section) => {
                if let Some(view) = self.preferences {
                    // Fails only if the interface's own tree is inconsistent: the pane then keeps
                    // what it showed.
                    let appearance = self.ui.appearance();
                    let widgets = preferences::show_section(
                        &mut self.ui,
                        view,
                        section,
                        &self.preferences_texts,
                        self.settings.settings(),
                        appearance,
                        list,
                    )
                    .unwrap_or_default();
                    self.playback_labels = widgets.playback_labels;
                    self.shortcut_editor = widgets.keyboard;
                    self.theme_editor = widgets.theme;
                }
            }
            Effect::PlaybackValue(index, n) => {
                // The value's label follows the slider (choice 4 of F2.12a4b2). The performance
                // will take the new tuning when the program plays (F3); there is none yet.
                if let Some(&label) = self.playback_labels.get(index) {
                    let text = preferences::playback_value_text(&self.preferences_texts, index, n);
                    if let Some(widget) = self.ui.widget_mut::<Label>(label) {
                        widget.set_text(&text);
                    }
                    self.ui.request_layout(label);
                    self.ui.frame(list);
                }
            }
        }
    }

    /// Draws with `pair` in `mode` from now on: the window's colours (the request) and the
    /// interface's density, text and shape tokens (the light half's: the same in both halves of
    /// every built-in pair, see `main.rs`; a custom theme changes colours only, F2.12b1).
    fn apply_theme(
        &mut self,
        pair: orchestra_ui::ThemePair,
        mode: orchestra_ui::ThemeMode,
        list: &mut DisplayList,
    ) {
        self.requests
            .push_back(WindowRequest::SetTheme { pair, mode });
        self.ui
            .set_theme(pair.instance(orchestra_ui::Appearance::Light));
        self.ui.frame(list);
    }

    /// A message of the theme editor (F2.12b1): the editor changes the settings (saved half a
    /// second later, DG6) and its widgets; a changed theme is applied at once (the whole window
    /// is the preview), a question opens over the Preferences.
    fn theme_message(&mut self, message: ThemeMessage, list: &mut DisplayList) {
        let Some(editor) = self.theme_editor.as_mut() else {
            return;
        };
        let now = self.ui.now();
        let ui = &mut self.ui;
        let texts = &self.preferences_texts;
        let mut outcome = ThemeOutcome::None;
        self.settings.update(now, |settings| {
            outcome = editor.handle(
                ui,
                &texts.theme_editor,
                &texts.pairs,
                settings,
                message,
                list,
            );
        });
        match outcome {
            ThemeOutcome::None => {}
            ThemeOutcome::Theme => {
                let (pair, mode) = crate::settings::convert::theme_from_settings(
                    &self.settings.settings().theme,
                    &mut Vec::new(),
                );
                self.apply_theme(pair, mode, list);
            }
            // Fails only if the interface's own tree is inconsistent: the question is then not
            // shown, and nothing is deleted.
            ThemeOutcome::Ask(spec) => {
                let _ = self.ui.open_dialog(spec, list);
            }
        }
        self.ui.frame(list);
    }

    /// A message of the shortcut editor (F2.12c1): the editor changes the registry and its
    /// widgets; a change is written into the settings (ADR-0050 DG8, ADR-0048 AM7: only the
    /// differences from the defaults), a conflict opens its question over the Preferences.
    fn keyboard_message(&mut self, message: KeyboardMessage, list: &mut DisplayList) {
        let Some(editor) = self.shortcut_editor.as_mut() else {
            return;
        };
        let outcome = editor.handle(
            &mut self.ui,
            &self.preferences_texts.keyboard,
            message,
            list,
        );
        self.keyboard_outcome(outcome, list);
    }

    /// Carries out what the shortcut editor answered (F2.12c1–c2): the settings written after a
    /// change (only the differences from the defaults, AM7), a dialog opened over the
    /// Preferences, a file chooser opened for the editor.
    fn keyboard_outcome(&mut self, outcome: KeyboardOutcome, list: &mut DisplayList) {
        match outcome {
            KeyboardOutcome::None => {}
            KeyboardOutcome::Changed { report } => {
                if let Some(registry) = self.ui.actions() {
                    let entries = crate::settings::convert::shortcuts_to_settings(registry);
                    let now = self.ui.now();
                    self.settings
                        .update(now, |settings| settings.shortcuts = entries);
                }
                // Fails only if the interface's own tree is inconsistent: then not shown.
                if let Some(spec) = report {
                    let _ = self.ui.open_dialog(spec, list);
                }
            }
            // Fails only if the interface's own tree is inconsistent; the answer then never
            // comes, and the editor keeps its question pending, harmlessly.
            KeyboardOutcome::Ask(spec) => {
                let _ = self.ui.open_dialog(spec, list);
            }
            KeyboardOutcome::Choose(request) => {
                self.choose(ChooserOwner::Shortcuts, request, list);
            }
        }
        self.ui.frame(list);
    }

    /// Opens the system's file chooser with `request` for `owner`, who gets the answer
    /// ([`WindowApp::file_chosen`]). The interface refuses a second chooser while one is open
    /// (its blocking layer keeps one from being asked anyway); the owner then stays as it was.
    fn choose(&mut self, owner: ChooserOwner, request: FileRequest, list: &mut DisplayList) {
        if self.ui.choose_file(request, list) {
            self.chooser = Some(owner);
        }
    }

    /// Acts on the widgets' messages (ADR-0045 WA7): *Restore defaults* puts the options back,
    /// a committed track name is written above the staff.
    fn handle_messages(&mut self, list: &mut DisplayList) {
        let mut restore = false;
        let mut name = None;
        let mut about = false;
        let mut open_preferences = false;
        // The steps of the open project (F2.12a5b), run after the queue is drained (they open
        // dialogs and choosers, which borrow the interface), in the order they came.
        let mut steps: Vec<ProjectInput> = Vec::new();
        // The Preferences' changes, applied after the queue is drained (it borrows the interface).
        let mut changes = Vec::new();
        // The shortcut editor's messages (F2.12c1), likewise.
        let mut keyboard = Vec::new();
        // The theme editor's messages (F2.12b1), likewise.
        let mut theme = Vec::new();
        for message in self.ui.take_messages() {
            match message {
                DemoMessage::RestoreDefaults => restore = true,
                DemoMessage::TrackName(text) => name = Some(text),
                DemoMessage::Preferences(message) => changes.push(message),
                DemoMessage::Keyboard(message) => keyboard.push(message),
                DemoMessage::Theme(message) => theme.push(message),
                DemoMessage::SaveChanges(answer) => steps.push(ProjectInput::SaveChanges(answer)),
                DemoMessage::Replace(replace) => steps.push(ProjectInput::Replace(replace)),
                // Full screen asks the window (F2.11d1), About opens its dialog (F2.12a3b), the
                // file actions and Quit go through the open project (F2.12a5b: Quit asks about
                // unsaved changes first); the other actions are disabled until they exist, so
                // their messages never come.
                DemoMessage::Action(id) => {
                    if let Some(ids) = self.actions {
                        if id == ids.quit {
                            steps.push(ProjectInput::Close);
                        } else if id == ids.full_screen {
                            self.requests.push_back(WindowRequest::ToggleFullScreen);
                        } else if id == ids.about {
                            about = true;
                        } else if id == ids.preferences {
                            open_preferences = true;
                        } else if id == ids.new {
                            steps.push(ProjectInput::New);
                        } else if id == ids.open {
                            steps.push(ProjectInput::Open);
                        } else if id == ids.save {
                            steps.push(ProjectInput::Save);
                        } else if id == ids.save_as {
                            steps.push(ProjectInput::SaveAs);
                        }
                    }
                }
                _ => {}
            }
        }
        for change in changes {
            self.preference_changed(change, list);
        }
        for message in keyboard {
            self.keyboard_message(message, list);
        }
        for message in theme {
            self.theme_message(message, list);
        }
        // A dialog blocks the menus and the shortcuts while it is open (ADR-0050 DG1), so About
        // cannot be asked twice; `open_dialog` fails only if the interface's own tree is
        // inconsistent, and then the dialog is simply not shown.
        if about {
            let _ = self
                .ui
                .open_dialog(about_dialog(&self.about, DemoMessage::AboutClosed), list);
        }
        // The Preferences (F2.12a4a): open once at a time; their own shortcut closes them while
        // they are open (the interface turns it into their Escape), so it never comes here then.
        if open_preferences && self.preferences.is_none() {
            let closed_by = self.actions.map(|ids| ids.preferences);
            let appearance = self.ui.appearance();
            if let Ok((view, widgets)) = preferences::open_preferences(
                &mut self.ui,
                &self.preferences_texts,
                self.settings.settings(),
                closed_by,
                appearance,
                list,
            ) {
                self.preferences = Some(view);
                self.theme_editor = widgets.theme;
            }
        }
        // The flow ignores a command while a question or a chooser is open (and the interface's
        // modal layers keep it from coming), so a batch with two shortcuts runs only the first.
        for input in steps {
            self.project_step(|flow| input.run(flow), list);
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

/// Who asked for the open file chooser (F2.12c2, ADR-0051 SF6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChooserOwner {
    /// The open project: *Open*, *Save*, *Save as* (F2.12a5b).
    Project,
    /// The shortcut editor: *Import…*, *Export…*.
    Shortcuts,
}

/// A step of the open project asked by a message, run once the message queue is drained.
#[derive(Debug, Clone, Copy)]
enum ProjectInput {
    /// *New*.
    New,
    /// *Open*.
    Open,
    /// *Save*.
    Save,
    /// *Save as*.
    SaveAs,
    /// Quit.
    Close,
    /// An answer to "Save the changes?".
    SaveChanges(SaveAnswer),
    /// An answer to "Replace it?".
    Replace(bool),
}

impl ProjectInput {
    /// Runs the step on `flow`.
    fn run(self, flow: &mut ProjectFlow) -> Option<ProjectEffect> {
        match self {
            Self::New => flow.new_project(),
            Self::Open => flow.open(),
            Self::Save => flow.save(),
            Self::SaveAs => flow.save_as(),
            Self::Close => flow.close(),
            Self::SaveChanges(answer) => flow.save_answer(answer),
            Self::Replace(replace) => flow.replace_answer(replace),
        }
    }
}

/// The row of the options' buttons, *Restore defaults* and *OK*, at the end of `options`; returns
/// *Restore defaults* (its tooltip is a screenshot's example, F2.11c). As in [`Demo::new`], `add`
/// fails only for an unknown parent: a failure would leave the row incomplete, never crash.
fn buttons_row(ui: &mut Ui<DemoMessage>, options: WidgetId, texts: &DemoTexts) -> WidgetId {
    let content = LayoutProps::default();
    let buttons = ui
        .add(options, Flex::row(Space::S, Space::None), content)
        .unwrap_or_else(|_| ui.root());
    let restore_defaults = ui
        .add(
            buttons,
            Button::new(texts.restore_defaults.clone(), DemoMessage::RestoreDefaults),
            content,
        )
        .unwrap_or_else(|_| ui.root());
    let _ = ui.add(
        buttons,
        Button::new(texts.ok.clone(), DemoMessage::Ok).kind(ButtonKind::Primary),
        content,
    );
    restore_defaults
}

/// The window's frame (F2.11d2b): the program's actions handed to `ui` first (the text field's
/// context menu and the bar name them, F2.11d1, F2.11d2a), the menu bar along the top, and below it
/// the padded column the demonstration's content goes in. Returns the actions, the bar and that
/// column.
fn frame_window(
    ui: &mut Ui<DemoMessage>,
    frame: &Frame<'_>,
) -> (Option<ActionIds>, Option<WidgetId>, WidgetId) {
    let texts = &frame.texts.actions;
    let actions = attach_actions(ui, texts);
    let menu_bar = add_menu_bar(ui, actions.as_ref(), frame);
    let fill = LayoutProps::default()
        .width(Length::Fill(1.0))
        .height(Length::Fill(1.0));
    let window_root = ui.root();
    // `add` fails only for an unknown parent: the root always exists.
    let root = ui
        .add(window_root, Flex::column(Space::M, Space::L), fill)
        .unwrap_or(window_root);
    (actions, menu_bar, root)
}

/// What frames the demonstration's window: its texts, its title, who draws its frame.
struct Frame<'a> {
    /// The demonstration's texts.
    texts: &'a DemoTexts,
    /// The window's title (the program's name).
    title: &'a str,
    /// Who draws the window's frame (ADR-0049 TB1).
    decorations: Decorations,
    /// GNOME's `button-layout`, if the program read it (F2.11b3).
    button_layout: Option<&'a str>,
}

impl<'a> Frame<'a> {
    /// The frame of a window titled `title` with `texts`, whose frame `decorations` draws, with
    /// the buttons of `button_layout` in our title bar.
    const fn new(
        texts: &'a DemoTexts,
        title: &'a str,
        decorations: Decorations,
        button_layout: Option<&'a str>,
    ) -> Self {
        Self {
            texts,
            title,
            decorations,
            button_layout,
        }
    }
}

/// Adds the menu bar at the top of the window (F2.11d2b), on Linux and Windows only: macOS shows
/// the system's bar through `muda` (F2.11e), and until then the demonstration has none there.
/// `None` without the actions or on macOS.
fn add_menu_bar(
    ui: &mut Ui<DemoMessage>,
    actions: Option<&ActionIds>,
    frame: &Frame<'_>,
) -> Option<WidgetId> {
    if Platform::CURRENT == Platform::MacOs {
        return None;
    }
    let model = actions::menu_bar(actions?, &frame.texts.actions);
    let root = ui.root();
    // Where the system draws no frame, the menus go into our own title bar (ADR-0049 TB5), with
    // the buttons GNOME's `button-layout` asks for (F2.11b3; only Close by default).
    if let Decorations::Own = frame.decorations {
        let spec = TitleBarSpec::from_setting(
            frame.button_layout,
            frame.title.to_owned(),
            frame.texts.window_buttons.clone(),
        );
        return ui.add_title_bar(root, &model, &spec).ok();
    }
    ui.add_menu_bar(
        root,
        &model,
        LayoutProps::default().width(Length::Fill(1.0)),
    )
    .ok()
}

/// Hands the program's actions and their shortcuts to `ui` (F2.11d1), with their names and the
/// keys' names in the interface's language (F2.11d2a), and returns their indices. Their tables
/// are checked by the tests on the three platforms; if they were wrong anyway, the demonstration
/// runs without shortcuts and menus (`None`) rather than not at all.
fn attach_actions(ui: &mut Ui<DemoMessage>, texts: &ActionTexts) -> Option<ActionIds> {
    let (mut registry, ids) = actions::registry(Platform::CURRENT).ok()?;
    texts.apply(&mut registry);
    ui.set_actions(registry, DemoMessage::Action);
    Some(ids)
}

/// The *Track name* row of the options (F2.10b2): a label and a text field filling the rest of
/// the row, with its placeholder, its name for assistive technologies and, when the actions
/// exist, the standard editing context menu (F2.11d2a). Returns the field.
///
/// As in [`Demo::new`], `add` fails only for an unknown parent, and every parent here was just
/// created: a failure would leave the demonstration incomplete, never crash it.
fn track_name_row(
    ui: &mut Ui<DemoMessage>,
    options: WidgetId,
    texts: &DemoTexts,
    actions: Option<&ActionIds>,
) -> WidgetId {
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
    let mut field = TextField::new("", |text| DemoMessage::TrackName(text.to_owned()))
        .placeholder(texts.track_name_placeholder.clone())
        .labelled(texts.track_name.clone());
    if let Some(ids) = actions {
        field = field.with_context_menu(actions::field_menu(ids));
    }
    ui.add(name_row, field, content.width(Length::Fill(1.0)))
        .unwrap_or_else(|_| ui.root())
}

/// The narrowest the options' pane gets, logical pixels (owner's choice after F2.10b2).
///
/// Why 160: the splitter's default minimum (48) let the pane shrink until every label was cut to
/// a few letters, which is a useless state. 160 keeps the switches with the start of their labels
/// and a usable volume slider, while still letting the staff take most of a small window. Labels
/// longer than the pane are shortened with an ellipsis (step F2.09f, ADR-0045 WJ1–WJ7), so the
/// minimum no longer protects anything from being drawn across the divider: it only keeps
/// enough of each label readable.
const OPTIONS_MIN_WIDTH: f32 = 160.0;

/// The splitter under the title, filling the rest of the window (F2.09d, WF8): a vertical scroll
/// area on the left (a little less than half) holding the options' column, and a scroll area in
/// both directions on the right for the staff. Returns (the options' column, the staff's area).
///
/// As in [`Demo::new`], `add` fails only for an unknown parent, and every parent here was just
/// created: a failure would leave the demonstration incomplete, never crash it.
fn panes(ui: &mut Ui<DemoMessage>, root: WidgetId) -> (WidgetId, WidgetId) {
    let fill = LayoutProps::default()
        .width(Length::Fill(1.0))
        .height(Length::Fill(1.0));
    // The splitter under the title, filling the rest of the window: the options on the left
    // (a little less than half), the staff on the right. The options' pane keeps at least
    // OPTIONS_MIN_WIDTH, so its labels are not cut to a few letters (the staff's pane keeps the
    // splitter's default minimum).
    let splitter = ui
        .add(
            root,
            Splitter::new(Axis::Horizontal, 0.45).minimum_sizes(OPTIONS_MIN_WIDTH, 48.0),
            fill,
        )
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
    fn attach_clipboard(&mut self, clipboard: Box<dyn ClipboardSource>) {
        // The system's clipboard, created once by the window (ADR-0047 TF12): the fields of the
        // interface copy to it and paste from it.
        self.ui.set_clipboard(clipboard);
    }

    fn layout(&mut self, bounds: Rect, list: &mut DisplayList) {
        self.ui.layout(bounds, list);
    }

    fn input(&mut self, event: &InputEvent, list: &mut DisplayList) {
        self.ui.input(event, list);
        // The window shows another half (F2.12b1): the theme editor's fields follow it.
        if let InputEvent::Appearance(half) = event
            && let Some(editor) = self.theme_editor.as_mut()
        {
            editor.show_half(
                &mut self.ui,
                &self.preferences_texts.theme_editor,
                self.settings.settings(),
                *half,
                list,
            );
        }
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
        // The field's caret blinks with the interface's wake-ups (F2.10b2); the settings are
        // written when their delay is over (ADR-0050 DG6, F2.12a4a).
        self.ui.advance(now, list);
        self.settings.advance(now);
    }

    fn next_wakeup(&self) -> Option<Instant> {
        // The earlier of the interface's wake-up and the settings' write.
        match (self.ui.next_wakeup(), self.settings.next_wakeup()) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }

    fn text_input(&self) -> Option<TextInput> {
        // The input method is on while the "Track name" field has the focus, with its caret's
        // area for the candidate window (ADR-0047 TF5, TF6; F2.10c2).
        self.ui.text_input()
    }

    fn system_menu(&mut self) -> Option<SystemMenu> {
        // macOS only (the model exists there only): the bar at the top of the screen, with the
        // same actions as the drawn bar of Linux and Windows (F2.11e1), with macOS's own items
        // and Window menu around them (F2.11e2).
        let mac = self.mac_menu.as_ref()?;
        Some(self.ui.system_menu(&mac.model, &mac.app_name, &mac.texts))
    }

    fn run_action(&mut self, action: ActionId, list: &mut DisplayList) {
        // Chosen in the system's menu: the same path as a shortcut, then the app's reaction.
        self.ui.run_action(action, list);
        self.handle_messages(list);
    }

    fn take_menu_updates(&mut self, out: &mut Vec<(ActionId, ActionState)>) {
        self.ui.take_menu_updates(out);
    }

    fn take_file_request(&mut self) -> Option<FileRequest> {
        // Asked by *Open* or *Save as* (F2.12a5a), kept by the interface with its blocking layer.
        self.ui.take_file_request()
    }

    fn file_chosen(&mut self, choice: FileChoice, list: &mut DisplayList) {
        // The blocking layer goes first, so an error dialog opens over the interface itself.
        self.ui.file_chosen(list);
        // The chosen file goes back to who asked (F2.12c2): the open project opens or saves it
        // (F2.12a5b), the shortcut editor imports or exports it. A cancelled chooser, or one that
        // could not open, puts the asker back to idle.
        let path = match choice {
            FileChoice::Chosen(path) => Some(path),
            FileChoice::Cancelled => None,
            // Never silence (ADR-0036 B4): the error says what to install. `open_dialog` fails
            // only if the interface's own tree is inconsistent; then nothing is shown.
            FileChoice::Unavailable => {
                let _ = self.ui.open_dialog(
                    unavailable_dialog(&self.files, DemoMessage::FileErrorClosed),
                    list,
                );
                None
            }
        };
        match self.chooser.take() {
            Some(ChooserOwner::Shortcuts) => {
                if let Some(editor) = self.shortcut_editor.as_mut() {
                    let outcome =
                        editor.file_chosen(&mut self.ui, &self.preferences_texts.keyboard, path);
                    self.keyboard_outcome(outcome, list);
                }
            }
            Some(ChooserOwner::Project) | None => {
                self.project_step(|flow| flow.chosen(path), list);
            }
        }
    }

    fn close_requested(&mut self, list: &mut DisplayList) -> bool {
        // As Quit (F2.12a5b): closing now when there is nothing to save; otherwise the question
        // opens and its answer closes the window later (`ProjectEffect::Close`). While a question or a
        // chooser is open the flow answers nothing, and the window stays.
        let Some(flow) = self.flow.as_mut() else {
            return true;
        };
        match flow.close() {
            Some(ProjectEffect::Close) => true,
            Some(effect) => {
                self.show_effect(effect, list);
                self.refresh_title();
                false
            }
            None => false,
        }
    }

    fn title(&self) -> Option<&str> {
        // The open project's (F2.12a5b), kept by the interface.
        Some(self.ui.title())
    }

    fn take_window_request(&mut self) -> Option<WindowRequest> {
        // Our title bar's requests first (F2.11b2): a drag must follow its press closely. Then
        // Quit and Full screen, in the order their shortcuts were pressed (F2.11d1).
        self.ui
            .take_window_request()
            .or_else(|| self.requests.pop_front())
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
