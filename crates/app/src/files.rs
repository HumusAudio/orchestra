// SPDX-License-Identifier: GPL-3.0-or-later

//! The file choosers of *Open* and *Save as* (ADR-0050 DG4, SCALETTA F2.12a5a): their texts from
//! the catalogues, the requests built from them, and the error shown when no chooser can open.
//!
//! # What the requests ask (DG4)
//! - **Open**: the title *Open project*, one filter *Orchestra project* showing `*.orchestra`
//!   files ([`PROJECT_EXTENSION`], ADR-0029 S14).
//! - **Save as**: the title *Save project as*, the same filter, and the name proposed: the
//!   project's own, or the localised *Untitled* for a project never saved, with the extension
//!   added so the chooser shows the whole name (`Untitled.orchestra`).
//!
//! The folder is left to the system (it usually opens the last one used): remembering one of our
//! own comes with the recent files, later.
//!
//! # The open project's texts (F2.12a5b, ADR-0050 DG5)
//! The window title (`Name — Orchestra`, with `• ` in front while modified), the question "Save
//! the changes?", the question "Replace it?" and the errors of *New*, *Open* and *Save*, built
//! from [`ProjectTexts`]. What they ask and when is [`crate::project`]'s.
//!
//! ## Texts with a name in them
//! The texts above hold a name known only when they are shown (the project's, a file's). The
//! demonstration's texts are formatted once, when the language is chosen, and the localizer is
//! not kept; so each of these is formatted once with a **placeholder** in place of the name
//! ([`NameTemplate`]: a private-use character, U+E000, which no catalogue and no file name
//! contains in practice) and the name is put in its place when the text is shown. This is not
//! concatenation (CLAUDE.md §7): Fluent still decides where the name goes in each language, and
//! no message selects on the name (a test checks that the placeholder appears exactly once in
//! every language).
//!
//! ## The errors (choice 6 of F2.12a5b)
//! One title per operation ("«Song.orchestra» can't be opened"), and one message per reason
//! the user can act on: the file is missing, not readable or writable, too large, not a
//! project, made by a newer Orchestra, damaged (a syntax, schema, migration or document error:
//! the place and the cause are for developers, in the error's English `Display`, and are not
//! shown; they will reach the log when there is one, the open decision S4 of F2.12a2), the disk
//! is full, or a mistake of the program. The file's whole path is the dialog's detail, and a failed save adds that the
//! file already there was left as it was (ADR-0029 S13).
//!
//! # The error (ADR-0036 B4)
//! Only Linux can answer [`FileChoice::Unavailable`](orchestra_ui::FileChoice) (no `FileChooser`
//! portal, no `zenity`): the error dialog says so and names the packages to install, with one
//! *OK* button.

use std::io::ErrorKind;
use std::path::Path;

use orchestra_i18n::{FluentArgs, I18nError, Localizer};
use orchestra_io::{LoadError, PROJECT_EXTENSION, SaveError};
use orchestra_ui::{ButtonRole, DialogKind, DialogSpec, FileRequest};

use crate::project::{ProjectError, file_name};

/// The isolation marks Fluent puts around every placeable (U+2068 FIRST STRONG ISOLATE, U+2069
/// POP DIRECTIONAL ISOLATE), dropped from our texts as for the About dialog's (`crate::about`):
/// they would reach the system's title, the chooser and screen readers.
const ISOLATION_MARKS: [char; 2] = ['\u{2068}', '\u{2069}'];

/// The placeholder of a [`NameTemplate`]: a character of the Private Use Area, which no
/// catalogue writes and which file names do not contain in practice.
const SLOT: char = '\u{E000}';

/// The file choosers' texts in one language, already formatted (CLAUDE.md §7).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileTexts {
    /// `file-untitled`: the name of a project never saved.
    pub untitled: String,
    /// `file-filter-project`: the filter's name.
    pub filter: String,
    /// `file-open-title`.
    pub open_title: String,
    /// `file-save-title`.
    pub save_title: String,
    /// `file-chooser-unavailable-title`: the error's title.
    pub unavailable_title: String,
    /// `file-chooser-unavailable-message`: the error's message.
    pub unavailable_message: String,
    /// `dialog-ok`: the error's button.
    pub ok: String,
}

impl FileTexts {
    /// The texts in `localizer`'s language. The isolation marks Fluent puts around the program's
    /// name are dropped, as for the About dialog's texts (`crate::about`): the name is ours and
    /// left to right, and the marks would reach the system's chooser and screen readers.
    ///
    /// # Errors
    /// The localizer's, if a message is missing (the catalogues' parity tests prevent it).
    pub fn new(localizer: &Localizer) -> Result<Self, I18nError> {
        let text = |key: &str| -> Result<String, I18nError> {
            Ok(localizer.format(key, None)?.replace(ISOLATION_MARKS, ""))
        };
        Ok(Self {
            untitled: text("file-untitled")?,
            filter: text("file-filter-project")?,
            open_title: text("file-open-title")?,
            save_title: text("file-save-title")?,
            unavailable_title: text("file-chooser-unavailable-title")?,
            unavailable_message: text("file-chooser-unavailable-message")?,
            ok: text("dialog-ok")?,
        })
    }
}

/// The request of *Open* (see the module documentation).
#[must_use]
pub fn open_request(texts: &FileTexts) -> FileRequest {
    FileRequest::open(
        texts.open_title.clone(),
        texts.filter.clone(),
        PROJECT_EXTENSION,
    )
}

/// The request of *Save as*, proposing `name` (the project's name without the extension), or
/// *Untitled* when `None` (see the module documentation).
#[must_use]
pub fn save_request(texts: &FileTexts, name: Option<&str>) -> FileRequest {
    let name = name.unwrap_or(&texts.untitled);
    FileRequest::save(
        texts.save_title.clone(),
        texts.filter.clone(),
        PROJECT_EXTENSION,
        format!("{name}.{PROJECT_EXTENSION}"),
    )
}

/// The error shown when no chooser can open (see the module documentation): its *OK* button sends
/// `ok`, which Escape and Enter press too (it is the only button).
#[must_use]
pub fn unavailable_dialog<M>(texts: &FileTexts, ok: M) -> DialogSpec<M> {
    DialogSpec::new(
        DialogKind::Error,
        texts.unavailable_title.clone(),
        texts.unavailable_message.clone(),
    )
    .button(texts.ok.clone(), ButtonRole::Affirmative, ok)
}

/// A text with one name in it, formatted once with a placeholder (see the module documentation).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameTemplate(String);

impl NameTemplate {
    /// The message `key` of `localizer`, with its variable `variable` left as the placeholder.
    ///
    /// # Errors
    /// The localizer's, if the message is missing.
    pub fn new(localizer: &Localizer, key: &str, variable: &str) -> Result<Self, I18nError> {
        let mut args = FluentArgs::new();
        args.set(variable, SLOT.to_string());
        Ok(Self(
            localizer
                .format(key, Some(&args))?
                .replace(ISOLATION_MARKS, ""),
        ))
    }

    /// The text with `name` in place of the placeholder. O(text length + name length).
    #[must_use]
    pub fn fill(&self, name: &str) -> String {
        self.0.replace(SLOT, name)
    }

    /// How many placeholders the text holds: exactly one in every language (the tests check it).
    #[must_use]
    pub fn slots(&self) -> usize {
        self.0.matches(SLOT).count()
    }
}

/// The open project's texts in one language (see the module documentation), already formatted.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(
    clippy::struct_field_names,
    reason = "the fields are named after their Fluent keys, which share their prefixes"
)]
pub struct ProjectTexts {
    /// `file-untitled`: the name of a project never saved.
    pub untitled: String,
    /// `window-title`, with `$name`.
    pub window_title: NameTemplate,
    /// `window-title-modified`, with `$name`.
    pub window_title_modified: NameTemplate,
    /// `project-save-changes-title`, with `$name`.
    pub save_changes_title: NameTemplate,
    /// `project-save-changes-message`.
    pub save_changes_message: String,
    /// `dialog-save`.
    pub save: String,
    /// `dialog-dont-save`.
    pub dont_save: String,
    /// `dialog-cancel`.
    pub cancel: String,
    /// `project-replace-title`, with `$file`.
    pub replace_title: NameTemplate,
    /// `project-replace-message`.
    pub replace_message: String,
    /// `dialog-replace`.
    pub replace: String,
    /// `dialog-ok`.
    pub ok: String,
    /// `project-new-error-title`.
    pub new_error_title: String,
    /// `project-new-error-message`.
    pub new_error_message: String,
    /// `project-open-error-title`, with `$file`.
    pub open_error_title: NameTemplate,
    /// `project-open-error-not-found`.
    pub open_not_found: String,
    /// `project-open-error-permission`.
    pub open_permission: String,
    /// `project-open-error-read`.
    pub open_read: String,
    /// `project-open-error-too-large`.
    pub open_too_large: String,
    /// `project-open-error-not-project`.
    pub open_not_project: String,
    /// `project-open-error-newer`.
    pub open_newer: String,
    /// `project-open-error-damaged`.
    pub open_damaged: String,
    /// `project-save-error-title`, with `$file`.
    pub save_error_title: NameTemplate,
    /// `project-save-error-permission`.
    pub save_permission: String,
    /// `project-save-error-full`.
    pub save_full: String,
    /// `project-save-error-write`.
    pub save_write: String,
    /// `project-save-error-internal`.
    pub save_internal: String,
    /// `project-save-error-kept`.
    pub save_kept: String,
}

impl ProjectTexts {
    /// The texts in `localizer`'s language.
    ///
    /// # Errors
    /// The localizer's, if a message is missing (the catalogues' parity tests prevent it).
    pub fn new(localizer: &Localizer) -> Result<Self, I18nError> {
        let text = |key: &str| -> Result<String, I18nError> {
            Ok(localizer.format(key, None)?.replace(ISOLATION_MARKS, ""))
        };
        let name = |key: &str| NameTemplate::new(localizer, key, "name");
        let file = |key: &str| NameTemplate::new(localizer, key, "file");
        Ok(Self {
            untitled: text("file-untitled")?,
            window_title: name("window-title")?,
            window_title_modified: name("window-title-modified")?,
            save_changes_title: name("project-save-changes-title")?,
            save_changes_message: text("project-save-changes-message")?,
            save: text("dialog-save")?,
            dont_save: text("dialog-dont-save")?,
            cancel: text("dialog-cancel")?,
            replace_title: file("project-replace-title")?,
            replace_message: text("project-replace-message")?,
            replace: text("dialog-replace")?,
            ok: text("dialog-ok")?,
            new_error_title: text("project-new-error-title")?,
            new_error_message: text("project-new-error-message")?,
            open_error_title: file("project-open-error-title")?,
            open_not_found: text("project-open-error-not-found")?,
            open_permission: text("project-open-error-permission")?,
            open_read: text("project-open-error-read")?,
            open_too_large: text("project-open-error-too-large")?,
            open_not_project: text("project-open-error-not-project")?,
            open_newer: text("project-open-error-newer")?,
            open_damaged: text("project-open-error-damaged")?,
            save_error_title: file("project-save-error-title")?,
            save_permission: text("project-save-error-permission")?,
            save_full: text("project-save-error-full")?,
            save_write: text("project-save-error-write")?,
            save_internal: text("project-save-error-internal")?,
            save_kept: text("project-save-error-kept")?,
        })
    }

    /// Every text with a name in it, for the tests (each must hold exactly one placeholder).
    #[must_use]
    pub fn templates(&self) -> [&NameTemplate; 6] {
        [
            &self.window_title,
            &self.window_title_modified,
            &self.save_changes_title,
            &self.replace_title,
            &self.open_error_title,
            &self.save_error_title,
        ]
    }
}

/// The window title for a project named `name` (`None`: never saved, *Untitled*), with the
/// modification mark when `modified` (ADR-0050 DG5). O(title length).
#[must_use]
pub fn window_title(texts: &ProjectTexts, name: Option<&str>, modified: bool) -> String {
    let name = name.unwrap_or(&texts.untitled);
    if modified {
        texts.window_title_modified.fill(name)
    } else {
        texts.window_title.fill(name)
    }
}

/// The question "Save the changes to «`name`»?" (`None`: *Untitled*) with its three answers,
/// sending `save`, `discard` and `cancel`; the interface places them in the system's order
/// (ADR-0050 DG2). *Save* is the default (Enter), *Cancel* answers Escape, *Don't save* is never
/// the default (it throws work away).
#[must_use]
pub fn save_changes_dialog<M>(
    texts: &ProjectTexts,
    name: Option<&str>,
    save: M,
    discard: M,
    cancel: M,
) -> DialogSpec<M> {
    let name = name.unwrap_or(&texts.untitled);
    DialogSpec::new(
        DialogKind::Question,
        texts.save_changes_title.fill(name),
        texts.save_changes_message.clone(),
    )
    .button(texts.save.clone(), ButtonRole::Affirmative, save)
    .button(texts.dont_save.clone(), ButtonRole::Destructive, discard)
    .button(texts.cancel.clone(), ButtonRole::Cancel, cancel)
}

/// The question "«file» already exists. Replace it?" for `path`, sending `replace` or `cancel`.
/// *Replace* goes ahead by throwing the old file away, so it is destructive and never the
/// default; with no default the focus starts on *Cancel* (ADR-0050 C4), so Enter and Escape both
/// keep the file, and replacing it takes a deliberate click or Tab.
#[must_use]
pub fn replace_dialog<M>(
    texts: &ProjectTexts,
    path: &Path,
    replace: M,
    cancel: M,
) -> DialogSpec<M> {
    DialogSpec::new(
        DialogKind::Question,
        texts.replace_title.fill(&file_name(path)),
        texts.replace_message.clone(),
    )
    .detail(path.display().to_string())
    .button(texts.replace.clone(), ButtonRole::Destructive, replace)
    .button(texts.cancel.clone(), ButtonRole::Cancel, cancel)
}

/// The error dialog of `error` (see the module documentation), with one *OK* sending `ok`.
#[must_use]
pub fn project_error_dialog<M>(texts: &ProjectTexts, error: &ProjectError, ok: M) -> DialogSpec<M> {
    let spec = match error {
        ProjectError::New(_) => DialogSpec::new(
            DialogKind::Error,
            texts.new_error_title.clone(),
            texts.new_error_message.clone(),
        ),
        ProjectError::Open { path, error } => DialogSpec::new(
            DialogKind::Error,
            texts.open_error_title.fill(&file_name(path)),
            open_message(texts, error),
        )
        .detail(path.display().to_string()),
        ProjectError::Save { path, error } => DialogSpec::new(
            DialogKind::Error,
            texts.save_error_title.fill(&file_name(path)),
            save_message(texts, error),
        )
        .detail(path.display().to_string())
        .detail(texts.save_kept.clone()),
    };
    spec.button(texts.ok.clone(), ButtonRole::Affirmative, ok)
}

/// The message of a failed open: one per reason the user can act on (module documentation).
fn open_message(texts: &ProjectTexts, error: &LoadError) -> String {
    match error {
        LoadError::Io(failure) => match failure.kind() {
            ErrorKind::NotFound => texts.open_not_found.clone(),
            ErrorKind::PermissionDenied => texts.open_permission.clone(),
            _ => texts.open_read.clone(),
        },
        LoadError::TooLarge { .. } => texts.open_too_large.clone(),
        LoadError::NotAProject => texts.open_not_project.clone(),
        LoadError::NewerFormat { .. } => texts.open_newer.clone(),
        // The place and the cause are for developers (`Display`), not shown (module documentation).
        LoadError::Syntax { .. }
        | LoadError::Schema { .. }
        | LoadError::Migration { .. }
        | LoadError::Document(_) => texts.open_damaged.clone(),
    }
}

/// The message of a failed save (module documentation).
fn save_message(texts: &ProjectTexts, error: &SaveError) -> String {
    match error {
        SaveError::Io(failure) => match failure.kind() {
            ErrorKind::PermissionDenied | ErrorKind::ReadOnlyFilesystem => {
                texts.save_permission.clone()
            }
            ErrorKind::StorageFull | ErrorKind::QuotaExceeded => texts.save_full.clone(),
            _ => texts.save_write.clone(),
        },
        SaveError::Encode(_) => texts.save_internal.clone(),
    }
}
