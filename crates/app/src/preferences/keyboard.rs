// SPDX-License-Identifier: GPL-3.0-or-later

//! The shortcut editor of the Preferences' **Keyboard** section (ADR-0048 AM7, ADR-0050 DG9,
//! SCALETTA F2.12c1, owner's choices 1–8 of 2026-11-25).
//!
//! # What the user sees (choice 1)
//! Under the modifier of fine adjustment, from the top:
//! - the heading **Shortcuts** and a **search field** (choice 4): it filters the commands by
//!   name and by the text of their shortcuts ("ctrl+s" finds Save), ignoring case; a muted line
//!   says so when nothing matches;
//! - the **list of commands**, all of them (also the disabled ones, Help), in the menus' order,
//!   each with its shortcuts at the right of its row; it scrolls, and takes the room left;
//! - for the selected command, *Shortcuts of "Save"* and the **list of its shortcuts** (Redo has
//!   two on Windows; "No shortcut" when it has none), with **Add…**, **Remove** (the selected
//!   one) and **Restore defaults**;
//! - the place where **the capture box** appears after *Add…* (choice 2,
//!   [`ShortcutCapture`]), with a muted line under it saying why a key cannot be assigned;
//! - **Restore all shortcuts**.
//!
//! # Conflicts (choice 3)
//! A shortcut captured that another command has opens our question *"Ctrl+N is already assigned
//! to "New""*, *Assign it to "Open" instead? "New" will no longer have it.*, with *Reassign*
//! (the default button: the user pressed the combination on purpose) and *Cancel*.
//! *Restore defaults* when some defaults belong to other commands now asks *Some default
//! shortcuts of "New" are assigned to other commands*, listing each ("Ctrl+N — Open") as the
//! dialog's details. The registry does the work all or nothing ([`Registry::reassign`],
//! [`Registry::restore_reassigning`]).
//!
//! # Changes apply at once (choice 5, DG8)
//! Every change goes to the registry, so the menus and the shortcuts follow at once, and the
//! demonstration writes the differences from the defaults into the settings
//! ([`KeyboardOutcome::Changed`], `shortcuts_to_settings`), which the store saves half a second
//! later.
//!
//! # Importing and exporting (F2.12c2, ADR-0051)
//! *Import…* and *Export…* beside *Restore all shortcuts* open the system's file chooser for a
//! shortcut file (`orchestra_io::shortcuts`, `.orchestra-shortcuts`). The editor answers
//! [`KeyboardOutcome::Choose`] with the request; the demonstration remembers that the answer is
//! the editor's and hands it back ([`ShortcutEditor::file_chosen`]).
//! - **Export** writes the whole keyboard ([`layout_to_entries`], SF2), adding the extension to a
//!   name that lacks it and asking before replacing a file the chooser did not confirm (SF5).
//! - **Import** reads the file first (an error says why, and nothing changes, SF4), then asks
//!   *Replace your shortcuts with those of "File"?* only when the current shortcuts are not the
//!   defaults, then replaces them all with the file's (`apply_settings_shortcuts`, AM7's rules,
//!   SF3) and reports how many entries were not used, when some were not.
//!
//! # Contexts (a recorded limit)
//! AM7 asks to show when a context's shortcut covers a global one. Every action of the program is
//! global today, so there is nothing to show; it comes with the views' contexts (F2.21).
//!
//! # Where the logic is
//! [`ShortcutEditor`] holds the widgets' IDs and the editor's state (the search, the commands
//! shown, the selection, the question pending). [`ShortcutEditor::handle`] changes the registry
//! and the widgets for one message and answers what the demonstration must do next. The pure
//! parts (the search, the texts of the rows) are functions the tests call directly.

use orchestra_i18n::{I18nError, Localizer};
use orchestra_ui::{
    ActionId, Button, ButtonRole, ColorToken, DialogKind, DialogSpec, DisplayList, FamilyToken,
    Flex, Label, LayoutProps, Length, ListBox, Platform, Registry, ScrollArea, Shortcut,
    ShortcutCapture, SizeToken, Space, TextField, Ui, UiError, Weight, WidgetId,
};

use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use orchestra_io::shortcuts::{self as file, ShortcutFile, ShortcutFileError};
use orchestra_ui::FileRequest;

use crate::demo::DemoMessage;
use crate::project::{file_name, with_extension_added};
use crate::settings::convert::{SettingsProblem, apply_settings_shortcuts, layout_to_entries};
use crate::template::{ISOLATION_MARKS, Template};

/// What the shortcut editor's widgets send (inside [`DemoMessage::Keyboard`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyboardMessage {
    /// The search field's text changed.
    Search(String),
    /// A command was selected, by its index among those shown.
    Command(usize),
    /// A shortcut of the selected command was selected, by its index.
    Shortcut(usize),
    /// *Add…*: the capture box appears.
    Add,
    /// The capture box recorded this shortcut.
    Captured(Shortcut),
    /// The capture box refused this fixed key (AM8) and keeps waiting.
    Refused(Shortcut),
    /// The capture box was cancelled (Escape, or the focus left it).
    CaptureCancelled,
    /// *Remove*: the selected shortcut of the selected command goes.
    Remove,
    /// *Restore defaults* of the selected command.
    Restore,
    /// *Restore all shortcuts*.
    RestoreAll,
    /// The answer to a conflict's question: `true` for *Reassign*.
    Conflict(bool),
    /// *Import…*: the chooser of a shortcut file to read (F2.12c2).
    Import,
    /// *Export…*: the chooser of a shortcut file to write.
    Export,
    /// The answer to "Replace your shortcuts with those of…?": `true` for *Replace*.
    ImportAnswer(bool),
    /// The answer to "… already exists. Replace it?" when exporting: `true` for *Replace*.
    ExportReplace(bool),
    /// *OK* of a report or an error: the dialog closes by itself, nothing else to do.
    Closed,
}

/// The texts of the shortcut editor in one language (CLAUDE.md §7).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyboardTexts {
    /// `prefs-shortcuts`: the heading.
    pub heading: String,
    /// `prefs-shortcuts-search`: the search field's placeholder and name.
    pub search: String,
    /// `prefs-shortcuts-commands`: the list's name.
    pub commands: String,
    /// `prefs-shortcuts-no-match`, with `$search`.
    pub no_match: Template,
    /// `prefs-shortcuts-of`, with `$command`.
    pub shortcuts_of: Template,
    /// `prefs-shortcuts-none`.
    pub none: String,
    /// `prefs-shortcut-add`.
    pub add: String,
    /// `prefs-shortcut-remove`.
    pub remove: String,
    /// `dialog-restore-defaults`.
    pub restore: String,
    /// `prefs-shortcuts-restore-all`.
    pub restore_all: String,
    /// `prefs-shortcut-capture`: the capture box's prompt.
    pub capture: String,
    /// `prefs-shortcut-capture-name`, with `$command`: the capture box's accessible name.
    pub capture_name: Template,
    /// `prefs-shortcut-fixed`, with `$shortcut`.
    pub fixed: Template,
    /// `prefs-shortcut-conflict-title`, with `$shortcut` and `$owner`.
    pub conflict_title: Template,
    /// `prefs-shortcut-conflict-message`, with `$command` and `$owner`.
    pub conflict_message: Template,
    /// `prefs-shortcut-restore-conflict-title`, with `$command`.
    pub restore_conflict_title: Template,
    /// `prefs-shortcut-restore-conflict-message`, with `$command`.
    pub restore_conflict_message: Template,
    /// `dialog-reassign`.
    pub reassign: String,
    /// `dialog-cancel`.
    pub cancel: String,
    /// `dialog-ok`.
    pub ok: String,
    /// `prefs-shortcuts-import`.
    pub import: String,
    /// `prefs-shortcuts-export`.
    pub export: String,
    /// `file-filter-shortcuts`.
    pub filter: String,
    /// `file-import-shortcuts-title`.
    pub import_title: String,
    /// `file-export-shortcuts-title`.
    pub export_title: String,
    /// `file-shortcuts-name`: the name proposed when exporting, without the extension.
    pub file_name: String,
    /// `prefs-shortcuts-import-question-title`, with `$file`.
    pub import_question: Template,
    /// `prefs-shortcuts-import-question-message`.
    pub import_question_message: String,
    /// `dialog-replace`.
    pub replace: String,
    /// `prefs-shortcuts-imported-title`.
    pub imported: String,
    /// `prefs-shortcuts-imported-unused`, with `$file` and `$count`.
    pub unused: Template,
    /// `prefs-shortcuts-import-error-title`, with `$file`.
    pub import_error: Template,
    /// `prefs-shortcuts-export-error-title`, with `$file`.
    pub export_error: Template,
    /// `prefs-shortcuts-import-error-not-shortcuts`.
    pub not_shortcuts: String,
    /// `prefs-shortcuts-import-error-newer`.
    pub newer: String,
    /// `prefs-shortcuts-import-error-damaged`.
    pub damaged: String,
    /// `project-open-error-not-found` (the project's texts serve every file).
    pub not_found: String,
    /// `project-open-error-permission`.
    pub read_permission: String,
    /// `project-open-error-read`.
    pub unreadable: String,
    /// `project-open-error-too-large`.
    pub too_large: String,
    /// `project-save-error-permission`.
    pub write_permission: String,
    /// `project-save-error-full`.
    pub disk_full: String,
    /// `project-save-error-write`.
    pub unwritable: String,
    /// `project-save-error-internal`.
    pub internal: String,
    /// `project-save-error-kept`.
    pub kept: String,
    /// `project-replace-title`, with `$file`.
    pub replace_title: Template,
    /// `project-replace-message`.
    pub replace_message: String,
}

impl KeyboardTexts {
    /// The texts in `localizer`'s language.
    ///
    /// # Errors
    /// The localizer's, if a message is missing (the catalogues' parity tests prevent it).
    pub fn new(localizer: &Localizer) -> Result<Self, I18nError> {
        let text = |key: &str| -> Result<String, I18nError> {
            Ok(localizer.format(key, None)?.replace(ISOLATION_MARKS, ""))
        };
        let template = |key: &str, variables: &[&str]| Template::new(localizer, key, variables);
        Ok(Self {
            heading: text("prefs-shortcuts")?,
            search: text("prefs-shortcuts-search")?,
            commands: text("prefs-shortcuts-commands")?,
            no_match: template("prefs-shortcuts-no-match", &["search"])?,
            shortcuts_of: template("prefs-shortcuts-of", &["command"])?,
            none: text("prefs-shortcuts-none")?,
            add: text("prefs-shortcut-add")?,
            remove: text("prefs-shortcut-remove")?,
            restore: text("dialog-restore-defaults")?,
            restore_all: text("prefs-shortcuts-restore-all")?,
            capture: text("prefs-shortcut-capture")?,
            capture_name: template("prefs-shortcut-capture-name", &["command"])?,
            fixed: template("prefs-shortcut-fixed", &["shortcut"])?,
            conflict_title: template("prefs-shortcut-conflict-title", &["shortcut", "owner"])?,
            conflict_message: template("prefs-shortcut-conflict-message", &["command", "owner"])?,
            restore_conflict_title: template(
                "prefs-shortcut-restore-conflict-title",
                &["command"],
            )?,
            restore_conflict_message: template(
                "prefs-shortcut-restore-conflict-message",
                &["command"],
            )?,
            reassign: text("dialog-reassign")?,
            cancel: text("dialog-cancel")?,
            ok: text("dialog-ok")?,
            import: text("prefs-shortcuts-import")?,
            export: text("prefs-shortcuts-export")?,
            filter: text("file-filter-shortcuts")?,
            import_title: text("file-import-shortcuts-title")?,
            export_title: text("file-export-shortcuts-title")?,
            file_name: text("file-shortcuts-name")?,
            import_question: template("prefs-shortcuts-import-question-title", &["file"])?,
            import_question_message: text("prefs-shortcuts-import-question-message")?,
            replace: text("dialog-replace")?,
            imported: text("prefs-shortcuts-imported-title")?,
            unused: template("prefs-shortcuts-imported-unused", &["file", "count"])?,
            import_error: template("prefs-shortcuts-import-error-title", &["file"])?,
            export_error: template("prefs-shortcuts-export-error-title", &["file"])?,
            not_shortcuts: text("prefs-shortcuts-import-error-not-shortcuts")?,
            newer: text("prefs-shortcuts-import-error-newer")?,
            damaged: text("prefs-shortcuts-import-error-damaged")?,
            not_found: text("project-open-error-not-found")?,
            read_permission: text("project-open-error-permission")?,
            unreadable: text("project-open-error-read")?,
            too_large: text("project-open-error-too-large")?,
            write_permission: text("project-save-error-permission")?,
            disk_full: text("project-save-error-full")?,
            unwritable: text("project-save-error-write")?,
            internal: text("project-save-error-internal")?,
            kept: text("project-save-error-kept")?,
            replace_title: template("project-replace-title", &["file"])?,
            replace_message: text("project-replace-message")?,
        })
    }

    /// Every text with values in it, each with its number of variables, for the tests (each
    /// variable must appear exactly once in every language).
    #[must_use]
    pub fn templates(&self) -> [(&Template, usize); 13] {
        [
            (&self.import_question, 1),
            (&self.unused, 2),
            (&self.import_error, 1),
            (&self.export_error, 1),
            (&self.replace_title, 1),
            (&self.no_match, 1),
            (&self.shortcuts_of, 1),
            (&self.capture_name, 1),
            (&self.fixed, 1),
            (&self.conflict_title, 2),
            (&self.conflict_message, 2),
            (&self.restore_conflict_title, 1),
            (&self.restore_conflict_message, 1),
        ]
    }
}

/// The shortcuts of `id` written for people, joined by ", " (the detail of its row). O(their
/// length).
#[must_use]
pub fn shortcuts_text(registry: &Registry, id: ActionId) -> String {
    registry
        .shortcuts(id)
        .iter()
        .map(|&shortcut| registry.shortcut_label(shortcut))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Whether a command named `name` with the shortcuts' text `shortcuts` matches the search
/// `query` (choice 4): the query, trimmed, is empty or found in either, ignoring case (Unicode's
/// lower case, without folding accents). O(lengths).
#[must_use]
pub fn matches(name: &str, shortcuts: &str, query: &str) -> bool {
    let query = query.trim().to_lowercase();
    query.is_empty()
        || name.to_lowercase().contains(&query)
        || shortcuts.to_lowercase().contains(&query)
}

/// The commands of `registry` matching `query`, in registration order (the menus').
/// O(commands × text lengths).
#[must_use]
pub fn shown_commands(registry: &Registry, query: &str) -> Vec<ActionId> {
    registry
        .ids()
        .filter(|&id| matches(registry.name_text(id), &shortcuts_text(registry, id), query))
        .collect()
}

/// A question waiting for its answer.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Pending {
    /// Give `shortcut` to `action`, taking it from its owner.
    Reassign {
        /// The command that gets it.
        action: ActionId,
        /// The shortcut.
        shortcut: Shortcut,
    },
    /// Restore the defaults of `action`, taking them from their owners.
    Restore {
        /// The command whose defaults come back.
        action: ActionId,
    },
    /// Replace every shortcut with those of `file`, read from the file named `name`.
    Import {
        /// The file's whole name, for the report.
        name: String,
        /// What was read.
        file: ShortcutFile,
    },
    /// Write the keyboard to `path`, which exists and the chooser did not confirm.
    Export {
        /// The file.
        path: PathBuf,
    },
}

/// Which file chooser the editor is waiting for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Choosing {
    /// A file to import.
    Import,
    /// A file to export to.
    Export,
}

/// What the demonstration does after a message ([`ShortcutEditor::handle`]).
#[derive(Debug)]
pub enum KeyboardOutcome {
    /// Nothing more.
    None,
    /// The registry's shortcuts changed: write the differences into the settings (DG8); then
    /// show `report` if there is one (an import with entries not used, F2.12c2).
    Changed {
        /// What to tell after the change.
        report: Option<DialogSpec<DemoMessage>>,
    },
    /// Open this dialog (a conflict, choice 3; a question, a report or an error of a shortcut
    /// file, F2.12c2); its answer comes back as a [`KeyboardMessage`].
    Ask(DialogSpec<DemoMessage>),
    /// Open the system's file chooser with this request (F2.12c2); its answer goes back to
    /// [`ShortcutEditor::file_chosen`].
    Choose(FileRequest),
}

impl KeyboardOutcome {
    /// [`KeyboardOutcome::Changed`] with nothing to tell.
    const CHANGED: Self = Self::Changed { report: None };
}

/// The shortcut editor while the Keyboard section is shown (see the module documentation).
#[derive(Debug)]
pub struct ShortcutEditor {
    /// The search field.
    search: WidgetId,
    /// *Restore all shortcuts*.
    restore_all: WidgetId,
    /// *Import…* (F2.12c2).
    import: WidgetId,
    /// *Export…* (F2.12c2).
    export: WidgetId,
    /// The muted line under the search field ("No command matches…").
    no_match: WidgetId,
    /// The list of commands.
    commands: WidgetId,
    /// *Shortcuts of "…"*.
    of: WidgetId,
    /// The list of the selected command's shortcuts.
    shortcuts: WidgetId,
    /// "No shortcut", or nothing.
    none: WidgetId,
    /// *Add…*.
    add: WidgetId,
    /// *Remove*.
    remove: WidgetId,
    /// *Restore defaults*.
    restore: WidgetId,
    /// The holder of the capture box, at the start of its row.
    slot: WidgetId,
    /// The capture box, while it is there.
    capture: Option<WidgetId>,
    /// The muted line under the capture box (a fixed key's reason), or empty.
    notice: WidgetId,
    /// The search's text.
    query: String,
    /// The commands shown, in the list's order.
    shown: Vec<ActionId>,
    /// The selected command, if any is shown.
    action: Option<ActionId>,
    /// The selected shortcut's index in the selected command's list.
    shortcut: usize,
    /// The question waiting for its answer.
    pending: Option<Pending>,
    /// The file chooser waiting for its answer (F2.12c2).
    choosing: Option<Choosing>,
    /// Whose fixed keys apply (the capture box's).
    platform: Platform,
}

/// The message of the search field (a plain function, ADR-0045 WA7).
fn search_message(text: &str) -> DemoMessage {
    DemoMessage::Keyboard(KeyboardMessage::Search(text.to_owned()))
}

impl ShortcutEditor {
    /// Builds the editor at the end of `pane` (the Keyboard section's column) from `ui`'s
    /// registry, the first command selected, everything shown. Without a registry (never in the
    /// program) the lists are empty.
    ///
    /// # Errors
    /// The interface's, if its own tree is inconsistent (never expected).
    pub fn build(
        ui: &mut Ui<DemoMessage>,
        pane: WidgetId,
        texts: &KeyboardTexts,
        list: &mut DisplayList,
    ) -> Result<Self, UiError> {
        let wide = LayoutProps::default().width(Length::Fill(1.0));
        let muted = |text: String| Label::new(text).color(ColorToken::TextMuted).wrapping();
        ui.add(
            pane,
            Label::new(texts.heading.clone()).styled(
                FamilyToken::Ui,
                SizeToken::Body,
                Weight::SemiBold,
            ),
            LayoutProps::default(),
        )?;
        let search = ui.add(
            pane,
            TextField::new("", search_message)
                .on_change(search_message)
                .placeholder(texts.search.clone())
                .labelled(texts.search.clone()),
            wide,
        )?;
        let no_match = ui.add(pane, muted(String::new()), wide)?;
        // The commands take the room left, and scroll.
        let scroll = ui.add(pane, ScrollArea::vertical(), wide.height(Length::Fill(1.0)))?;
        let commands = ui.add(
            scroll,
            ListBox::new(Vec::new(), 0, |index| {
                DemoMessage::Keyboard(KeyboardMessage::Command(index))
            })
            .label(texts.commands.clone()),
            wide,
        )?;
        let of = ui.add(pane, Label::new(String::new()), wide)?;
        let row = ui.add(pane, Flex::row(Space::M, Space::None), wide)?;
        let shortcuts = ui.add(
            row,
            ListBox::new(Vec::new(), 0, |index| {
                DemoMessage::Keyboard(KeyboardMessage::Shortcut(index))
            }),
            LayoutProps::default(),
        )?;
        let none = ui.add(
            row,
            Label::new(String::new()).color(ColorToken::TextMuted),
            LayoutProps::default(),
        )?;
        let buttons = ui.add(
            row,
            Flex::row(Space::S, Space::None),
            LayoutProps::default(),
        )?;
        let button = |text: &str, message: KeyboardMessage| {
            Button::new(text.to_owned(), DemoMessage::Keyboard(message))
        };
        let add = ui.add(
            buttons,
            button(&texts.add, KeyboardMessage::Add),
            LayoutProps::default(),
        )?;
        let remove = ui.add(
            buttons,
            button(&texts.remove, KeyboardMessage::Remove),
            LayoutProps::default(),
        )?;
        let restore = ui.add(
            buttons,
            button(&texts.restore, KeyboardMessage::Restore),
            LayoutProps::default(),
        )?;
        // One row: the capture box (in its holder, empty until *Add…*) and, beside it, the reason
        // a key cannot be assigned, which only ever shows while the box is there. Neither takes a
        // line of its own while it is not shown.
        let capture_row = ui.add(pane, Flex::row(Space::M, Space::None), wide)?;
        let slot = ui.add(capture_row, Flex::row(Space::None, Space::None), wide)?;
        let notice = ui.add(capture_row, muted(String::new()), wide)?;
        let (restore_all, import, export) = last_row(ui, pane, texts)?;
        let platform = ui.actions().map_or(Platform::CURRENT, Registry::platform);
        let mut editor = Self {
            search,
            restore_all,
            import,
            export,
            no_match,
            commands,
            of,
            shortcuts,
            none,
            add,
            remove,
            restore,
            slot,
            capture: None,
            notice,
            query: String::new(),
            shown: Vec::new(),
            action: None,
            shortcut: 0,
            pending: None,
            choosing: None,
            platform,
        };
        editor.refresh_commands(ui, texts);
        ui.frame(list);
        Ok(editor)
    }

    /// The commands shown, in the list's order (tests).
    #[must_use]
    pub fn shown(&self) -> &[ActionId] {
        &self.shown
    }

    /// The selected command (tests).
    #[must_use]
    pub const fn selected(&self) -> Option<ActionId> {
        self.action
    }

    /// The capture box, while it is there (tests).
    #[must_use]
    pub const fn capture(&self) -> Option<WidgetId> {
        self.capture
    }

    /// The commands' list (tests, screenshots).
    #[must_use]
    pub const fn commands(&self) -> WidgetId {
        self.commands
    }

    /// *Add…* (tests, screenshots).
    #[must_use]
    pub const fn add_button(&self) -> WidgetId {
        self.add
    }

    /// The search field (tests, screenshots).
    #[must_use]
    pub const fn search(&self) -> WidgetId {
        self.search
    }

    /// The list of the selected command's shortcuts (tests).
    #[must_use]
    pub const fn shortcuts(&self) -> WidgetId {
        self.shortcuts
    }

    /// *Remove* (tests).
    #[must_use]
    pub const fn remove_button(&self) -> WidgetId {
        self.remove
    }

    /// *Restore defaults* (tests).
    #[must_use]
    pub const fn restore_button(&self) -> WidgetId {
        self.restore
    }

    /// *Restore all shortcuts* (tests).
    #[must_use]
    pub const fn restore_all_button(&self) -> WidgetId {
        self.restore_all
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

    /// The muted line under the capture box (tests).
    #[must_use]
    pub const fn notice(&self) -> WidgetId {
        self.notice
    }

    /// Acts on `message` (see the module documentation) and answers what the demonstration must
    /// do next. The widgets follow at the next frame, which the caller runs.
    pub fn handle(
        &mut self,
        ui: &mut Ui<DemoMessage>,
        texts: &KeyboardTexts,
        message: KeyboardMessage,
        list: &mut DisplayList,
    ) -> KeyboardOutcome {
        let outcome = match message {
            KeyboardMessage::Search(query) => {
                self.query = query;
                self.refresh_commands(ui, texts);
                KeyboardOutcome::None
            }
            KeyboardMessage::Command(index) => {
                self.end_capture(ui, list);
                self.action = self.shown.get(index).copied();
                self.shortcut = 0;
                self.refresh_detail(ui, texts);
                KeyboardOutcome::None
            }
            KeyboardMessage::Shortcut(index) => {
                self.shortcut = index;
                KeyboardOutcome::None
            }
            KeyboardMessage::Add => {
                self.start_capture(ui, texts, list);
                KeyboardOutcome::None
            }
            KeyboardMessage::Refused(shortcut) => {
                let label = ui
                    .actions()
                    .map(|registry| registry.shortcut_label(shortcut))
                    .unwrap_or_default();
                set_label(ui, self.notice, &texts.fixed.fill(&[&label]));
                KeyboardOutcome::None
            }
            KeyboardMessage::CaptureCancelled => {
                self.end_capture(ui, list);
                KeyboardOutcome::None
            }
            KeyboardMessage::Captured(shortcut) => {
                self.end_capture(ui, list);
                self.captured(ui, texts, shortcut)
            }
            KeyboardMessage::Remove => self.remove_selected(ui),
            KeyboardMessage::Restore => self.restore_selected(ui, texts),
            KeyboardMessage::RestoreAll => {
                if let Some(registry) = ui.actions_mut() {
                    registry.restore_all();
                }
                KeyboardOutcome::CHANGED
            }
            KeyboardMessage::Conflict(reassign) => self.answer(ui, reassign),
            KeyboardMessage::Import => {
                self.choosing = Some(Choosing::Import);
                KeyboardOutcome::Choose(FileRequest::open(
                    texts.import_title.clone(),
                    texts.filter.clone(),
                    file::EXTENSION,
                ))
            }
            KeyboardMessage::Export => {
                self.choosing = Some(Choosing::Export);
                KeyboardOutcome::Choose(FileRequest::save(
                    texts.export_title.clone(),
                    texts.filter.clone(),
                    file::EXTENSION,
                    format!("{}.{}", texts.file_name, file::EXTENSION),
                ))
            }
            KeyboardMessage::ImportAnswer(replace) => match self.pending.take() {
                Some(Pending::Import { name, file }) if replace => {
                    Self::import(ui, texts, &name, &file)
                }
                _ => KeyboardOutcome::None,
            },
            KeyboardMessage::ExportReplace(replace) => match self.pending.take() {
                Some(Pending::Export { path }) if replace => Self::export(ui, texts, &path),
                _ => KeyboardOutcome::None,
            },
            KeyboardMessage::Closed => KeyboardOutcome::None,
        };
        if matches!(outcome, KeyboardOutcome::Changed { .. }) {
            self.refresh_commands(ui, texts);
        }
        outcome
    }

    /// The answer of the file chooser the editor asked for (F2.12c2): the chosen path, or
    /// `None` when it was cancelled or could not open (the demonstration has said why). Answers
    /// what to show next, as [`ShortcutEditor::handle`] does.
    pub fn file_chosen(
        &mut self,
        ui: &mut Ui<DemoMessage>,
        texts: &KeyboardTexts,
        path: Option<PathBuf>,
    ) -> KeyboardOutcome {
        let (Some(choosing), Some(path)) = (self.choosing.take(), path) else {
            return KeyboardOutcome::None;
        };
        let outcome = match choosing {
            Choosing::Import => self.read_import(ui, texts, &path),
            Choosing::Export => {
                // The extension added when the typed name lacks it; a file the chooser did not
                // confirm is asked about first (SF5, as *Save as*, F2.12a5b).
                let target = with_extension_added(path.clone(), file::EXTENSION);
                if target != path && target.try_exists().unwrap_or(false) {
                    let spec = DialogSpec::new(
                        DialogKind::Question,
                        texts.replace_title.fill(&[&file_name(&target)]),
                        texts.replace_message.clone(),
                    )
                    .detail(target.display().to_string())
                    .button(
                        texts.replace.clone(),
                        ButtonRole::Destructive,
                        DemoMessage::Keyboard(KeyboardMessage::ExportReplace(true)),
                    )
                    .button(
                        texts.cancel.clone(),
                        ButtonRole::Cancel,
                        DemoMessage::Keyboard(KeyboardMessage::ExportReplace(false)),
                    );
                    self.pending = Some(Pending::Export { path: target });
                    KeyboardOutcome::Ask(spec)
                } else {
                    Self::export(ui, texts, &target)
                }
            }
        };
        if matches!(outcome, KeyboardOutcome::Changed { .. }) {
            self.refresh_commands(ui, texts);
        }
        outcome
    }

    /// Reads the file at `path` to import it: an error says why and changes nothing; otherwise
    /// the question when there is something to lose (SF3), or the import at once.
    fn read_import(
        &mut self,
        ui: &mut Ui<DemoMessage>,
        texts: &KeyboardTexts,
        path: &Path,
    ) -> KeyboardOutcome {
        let name = file_name(path);
        let file = match file::load(path) {
            Ok(file) => file,
            Err(error) => {
                return KeyboardOutcome::Ask(error_dialog(
                    texts,
                    texts.import_error.fill(&[&name]),
                    &error,
                    path,
                    false,
                ));
            }
        };
        let customised = ui
            .actions()
            .is_some_and(|registry| registry.overrides().next().is_some());
        if customised {
            let spec = DialogSpec::new(
                DialogKind::Question,
                texts.import_question.fill(&[&name]),
                texts.import_question_message.clone(),
            )
            .button(
                texts.replace.clone(),
                ButtonRole::Destructive,
                DemoMessage::Keyboard(KeyboardMessage::ImportAnswer(true)),
            )
            .button(
                texts.cancel.clone(),
                ButtonRole::Cancel,
                DemoMessage::Keyboard(KeyboardMessage::ImportAnswer(false)),
            );
            self.pending = Some(Pending::Import { name, file });
            return KeyboardOutcome::Ask(spec);
        }
        Self::import(ui, texts, &name, &file)
    }

    /// Replaces every shortcut with `file`'s (SF3: from the defaults, AM7's rules), and reports
    /// the entries not used when there are some: those the file reader dropped, the keys that
    /// are not shortcuts, the fixed keys and the conflicts lost (unknown actions are kept, not
    /// "not used").
    fn import(
        ui: &mut Ui<DemoMessage>,
        texts: &KeyboardTexts,
        name: &str,
        file: &ShortcutFile,
    ) -> KeyboardOutcome {
        let Some(registry) = ui.actions_mut() else {
            return KeyboardOutcome::None;
        };
        let mut problems = Vec::new();
        apply_settings_shortcuts(&file.entries, registry, &mut problems);
        let unused = file.dropped
            + problems
                .iter()
                .filter(|problem| {
                    !matches!(
                        problem,
                        SettingsProblem::Registry(orchestra_ui::LoadWarning::UnknownAction(_))
                    )
                })
                .count();
        let report = (unused > 0).then(|| {
            DialogSpec::new(
                DialogKind::Information,
                texts.imported.clone(),
                texts.unused.fill(&[name, &unused.to_string()]),
            )
            .button(
                texts.ok.clone(),
                ButtonRole::Affirmative,
                DemoMessage::Keyboard(KeyboardMessage::Closed),
            )
        });
        KeyboardOutcome::Changed { report }
    }

    /// Writes the whole keyboard to `path` (SF2); an error says why.
    fn export(ui: &mut Ui<DemoMessage>, texts: &KeyboardTexts, path: &Path) -> KeyboardOutcome {
        let Some(registry) = ui.actions() else {
            return KeyboardOutcome::None;
        };
        match file::save(&layout_to_entries(registry), path) {
            Ok(()) => KeyboardOutcome::None,
            Err(error) => KeyboardOutcome::Ask(error_dialog(
                texts,
                texts.export_error.fill(&[&file_name(path)]),
                &error,
                path,
                true,
            )),
        }
    }

    /// A shortcut captured for the selected command: added at once, or the question when another
    /// command has it.
    fn captured(
        &mut self,
        ui: &mut Ui<DemoMessage>,
        texts: &KeyboardTexts,
        shortcut: Shortcut,
    ) -> KeyboardOutcome {
        let Some(action) = self.action else {
            return KeyboardOutcome::None;
        };
        let Some(registry) = ui.actions_mut() else {
            return KeyboardOutcome::None;
        };
        if let Some(owner) = registry.owner_of(action, shortcut) {
            let label = registry.shortcut_label(shortcut);
            let (command, owner) = (registry.name_text(action), registry.name_text(owner));
            self.pending = Some(Pending::Reassign { action, shortcut });
            return KeyboardOutcome::Ask(question(
                texts,
                texts.conflict_title.fill(&[&label, owner]),
                texts.conflict_message.fill(&[command, owner]),
            ));
        }
        // Nobody has it: the reassignment takes nothing from anyone. A fixed key never comes
        // here (the capture box refuses it), so the error is not expected.
        match registry.reassign(action, shortcut) {
            Ok(_) => KeyboardOutcome::CHANGED,
            Err(_) => KeyboardOutcome::None,
        }
    }

    /// *Remove*: the selected shortcut of the selected command goes.
    fn remove_selected(&mut self, ui: &mut Ui<DemoMessage>) -> KeyboardOutcome {
        let Some(action) = self.action else {
            return KeyboardOutcome::None;
        };
        let Some(registry) = ui.actions_mut() else {
            return KeyboardOutcome::None;
        };
        let mut kept = registry.shortcuts(action).to_vec();
        if self.shortcut >= kept.len() {
            return KeyboardOutcome::None;
        }
        kept.remove(self.shortcut);
        // Fewer shortcuts never conflict and never hold a fixed key the list did not hold.
        match registry.set_shortcuts(action, &kept) {
            Ok(()) => {
                self.shortcut = self.shortcut.min(kept.len().saturating_sub(1));
                KeyboardOutcome::CHANGED
            }
            Err(_) => KeyboardOutcome::None,
        }
    }

    /// *Restore defaults*: at once, or the question when other commands have some defaults now.
    fn restore_selected(
        &mut self,
        ui: &mut Ui<DemoMessage>,
        texts: &KeyboardTexts,
    ) -> KeyboardOutcome {
        let Some(action) = self.action else {
            return KeyboardOutcome::None;
        };
        let Some(registry) = ui.actions_mut() else {
            return KeyboardOutcome::None;
        };
        let conflicts = registry.restore_conflicts(action);
        if conflicts.is_empty() {
            return match registry.restore(action) {
                Ok(()) => KeyboardOutcome::CHANGED,
                Err(_) => KeyboardOutcome::None,
            };
        }
        let command = registry.name_text(action);
        let mut spec = question(
            texts,
            texts.restore_conflict_title.fill(&[command]),
            texts.restore_conflict_message.fill(&[command]),
        );
        // One detail per default taken: "Ctrl+N — Open" (a shortcut and a name, nothing to
        // translate between them).
        for (shortcut, owner) in conflicts {
            spec = spec.detail(format!(
                "{} — {}",
                registry.shortcut_label(shortcut),
                registry.name_text(owner)
            ));
        }
        self.pending = Some(Pending::Restore { action });
        KeyboardOutcome::Ask(spec)
    }

    /// The answer to the question pending: *Reassign* does what it asked, *Cancel* nothing.
    fn answer(&mut self, ui: &mut Ui<DemoMessage>, reassign: bool) -> KeyboardOutcome {
        let Some(pending) = self.pending.take() else {
            return KeyboardOutcome::None;
        };
        let Some(registry) = ui.actions_mut() else {
            return KeyboardOutcome::None;
        };
        if !reassign {
            return KeyboardOutcome::None;
        }
        let done = match pending {
            Pending::Reassign { action, shortcut } => registry.reassign(action, shortcut).is_ok(),
            Pending::Restore { action } => registry.restore_reassigning(action).is_ok(),
            // The files' questions have answers of their own (`ImportAnswer`, `ExportReplace`).
            Pending::Import { .. } | Pending::Export { .. } => false,
        };
        if done {
            KeyboardOutcome::CHANGED
        } else {
            KeyboardOutcome::None
        }
    }

    /// Shows the capture box for the selected command, with the focus in it.
    fn start_capture(
        &mut self,
        ui: &mut Ui<DemoMessage>,
        texts: &KeyboardTexts,
        list: &mut DisplayList,
    ) {
        let Some(action) = self.action else {
            return;
        };
        if self.capture.is_some() {
            return;
        }
        let command = ui
            .actions()
            .map(|registry| registry.name_text(action).to_owned())
            .unwrap_or_default();
        let capture = ShortcutCapture::new(
            texts.capture.clone(),
            self.platform,
            |shortcut| DemoMessage::Keyboard(KeyboardMessage::Captured(shortcut)),
            |shortcut| DemoMessage::Keyboard(KeyboardMessage::Refused(shortcut)),
            || DemoMessage::Keyboard(KeyboardMessage::CaptureCancelled),
        )
        .labelled(texts.capture_name.fill(&[&command]));
        if let Ok(id) = ui.add(
            self.slot,
            capture,
            LayoutProps::default().width(Length::Fill(1.0)),
        ) {
            self.capture = Some(id);
            ui.frame(list);
            // Fails only for an unknown widget: the box was just added.
            let _ = ui.set_focus(Some(id), list);
        }
    }

    /// Removes the capture box, if it is there, and its notice; the focus goes back to *Add…*
    /// (it was in the box, which goes).
    fn end_capture(&mut self, ui: &mut Ui<DemoMessage>, list: &mut DisplayList) {
        set_label(ui, self.notice, "");
        let Some(capture) = self.capture.take() else {
            return;
        };
        let had_focus = ui.focused() == Some(capture);
        // Fails only if the interface's own tree is inconsistent: the box then stays, inert.
        let _ = ui.remove(capture, list);
        if had_focus {
            // Fails only for an unknown widget: *Add…* lives as long as the editor.
            let _ = ui.set_focus(Some(self.add), list);
        }
    }

    /// Fills the commands' list from the search, keeping the selected command if it is still
    /// shown (otherwise the first), then the detail below.
    fn refresh_commands(&mut self, ui: &mut Ui<DemoMessage>, texts: &KeyboardTexts) {
        let (names, details, shown) = match ui.actions() {
            Some(registry) => {
                let shown = shown_commands(registry, &self.query);
                let names = shown
                    .iter()
                    .map(|&id| registry.name_text(id).to_owned())
                    .collect();
                let details = shown
                    .iter()
                    .map(|&id| shortcuts_text(registry, id))
                    .collect();
                (names, details, shown)
            }
            None => (Vec::new(), Vec::new(), Vec::new()),
        };
        let selected = self
            .action
            .and_then(|action| shown.iter().position(|&id| id == action))
            .unwrap_or(0);
        self.action = shown.get(selected).copied();
        self.shown = shown;
        if let Some(widget) = ui.widget_mut::<ListBox<DemoMessage>>(self.commands) {
            widget.set_items(names, details, selected);
        }
        ui.request_layout(self.commands);
        let no_match = if self.shown.is_empty() {
            texts.no_match.fill(&[self.query.trim()])
        } else {
            String::new()
        };
        set_label(ui, self.no_match, &no_match);
        self.refresh_detail(ui, texts);
    }

    /// Fills the part below the commands for the selected one: its name, its shortcuts, the
    /// buttons' states.
    fn refresh_detail(&mut self, ui: &mut Ui<DemoMessage>, texts: &KeyboardTexts) {
        let (of, labels, modified) = match (ui.actions(), self.action) {
            (Some(registry), Some(action)) => (
                texts.shortcuts_of.fill(&[registry.name_text(action)]),
                registry
                    .shortcuts(action)
                    .iter()
                    .map(|&shortcut| registry.shortcut_label(shortcut))
                    .collect::<Vec<_>>(),
                registry.shortcuts(action) != registry.defaults(action),
            ),
            _ => (String::new(), Vec::new(), false),
        };
        self.shortcut = self.shortcut.min(labels.len().saturating_sub(1));
        let empty = labels.is_empty();
        set_label(ui, self.of, &of);
        set_label(
            ui,
            self.none,
            if empty && self.action.is_some() {
                &texts.none
            } else {
                ""
            },
        );
        if let Some(widget) = ui.widget_mut::<ListBox<DemoMessage>>(self.shortcuts) {
            widget.set_items(labels, Vec::new(), self.shortcut);
        }
        ui.request_layout(self.shortcuts);
        for (button, enabled) in [
            (self.add, self.action.is_some()),
            (self.remove, !empty),
            (self.restore, modified),
        ] {
            if let Some(widget) = ui.widget_mut::<Button<DemoMessage>>(button) {
                widget.set_enabled(enabled);
            }
            ui.request_record(button);
        }
    }
}

/// The last row of the editor: *Restore all shortcuts*, *Import…* and *Export…* (F2.12c2), as
/// `(restore all, import, export)`.
///
/// # Errors
/// The interface's, if its own tree is inconsistent (never expected).
fn last_row(
    ui: &mut Ui<DemoMessage>,
    pane: WidgetId,
    texts: &KeyboardTexts,
) -> Result<(WidgetId, WidgetId, WidgetId), UiError> {
    let button = |text: &str, message: KeyboardMessage| {
        Button::new(text.to_owned(), DemoMessage::Keyboard(message))
    };
    // *Restore all shortcuts*, *Import…* and *Export…* in one row at the end (F2.12c2).
    let last = ui.add(
        pane,
        Flex::row(Space::S, Space::None),
        LayoutProps::default(),
    )?;
    let restore_all = ui.add(
        last,
        button(&texts.restore_all, KeyboardMessage::RestoreAll),
        LayoutProps::default(),
    )?;
    let import = ui.add(
        last,
        button(&texts.import, KeyboardMessage::Import),
        LayoutProps::default(),
    )?;
    let export = ui.add(
        last,
        button(&texts.export, KeyboardMessage::Export),
        LayoutProps::default(),
    )?;
    Ok((restore_all, import, export))
}

/// The error dialog of a shortcut file (SF4): `title`, one message per reason the user can act
/// on (the project's texts for the system's reasons), the path as the detail, and for a failed
/// export (`writing`) that the file already there was left as it was. One *OK*.
fn error_dialog(
    texts: &KeyboardTexts,
    title: String,
    error: &ShortcutFileError,
    path: &Path,
    writing: bool,
) -> DialogSpec<DemoMessage> {
    let message = match error {
        ShortcutFileError::Io(failure) => match (failure.kind(), writing) {
            (ErrorKind::NotFound, false) => &texts.not_found,
            (ErrorKind::PermissionDenied, false) => &texts.read_permission,
            (_, false) => &texts.unreadable,
            (ErrorKind::PermissionDenied | ErrorKind::ReadOnlyFilesystem, true) => {
                &texts.write_permission
            }
            (ErrorKind::StorageFull | ErrorKind::QuotaExceeded, true) => &texts.disk_full,
            (_, true) => &texts.unwritable,
        },
        ShortcutFileError::TooLarge { .. } => &texts.too_large,
        ShortcutFileError::NotShortcuts => &texts.not_shortcuts,
        ShortcutFileError::NewerFormat { .. } => &texts.newer,
        ShortcutFileError::Syntax { .. } | ShortcutFileError::Migration { .. } => &texts.damaged,
        ShortcutFileError::Encode(_) => &texts.internal,
    };
    let mut spec = DialogSpec::new(DialogKind::Error, title, message.clone())
        .detail(path.display().to_string());
    if writing {
        spec = spec.detail(texts.kept.clone());
    }
    spec.button(
        texts.ok.clone(),
        ButtonRole::Affirmative,
        DemoMessage::Keyboard(KeyboardMessage::Closed),
    )
}

/// A conflict's question (choice 3): *Reassign* the default, *Cancel*.
fn question(texts: &KeyboardTexts, title: String, message: String) -> DialogSpec<DemoMessage> {
    DialogSpec::new(DialogKind::Question, title, message)
        .button(
            texts.reassign.clone(),
            ButtonRole::Affirmative,
            DemoMessage::Keyboard(KeyboardMessage::Conflict(true)),
        )
        .button(
            texts.cancel.clone(),
            ButtonRole::Cancel,
            DemoMessage::Keyboard(KeyboardMessage::Conflict(false)),
        )
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
