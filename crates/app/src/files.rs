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
//! # What is not here yet
//! The chosen path is not used until F2.12a5b, which connects the open project (DG5): there the
//! extension is also added to a saved name that lacks it (not every system adds it).
//!
//! # The error (ADR-0036 B4)
//! Only Linux can answer [`FileChoice::Unavailable`](orchestra_ui::FileChoice) (no `FileChooser`
//! portal, no `zenity`): the error dialog says so and names the packages to install, with one
//! *OK* button.

use orchestra_i18n::{I18nError, Localizer};
use orchestra_io::PROJECT_EXTENSION;
use orchestra_ui::{ButtonRole, DialogKind, DialogSpec, FileRequest};

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
            Ok(localizer
                .format(key, None)?
                .replace(['\u{2068}', '\u{2069}'], ""))
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
