// SPDX-License-Identifier: GPL-3.0-or-later

//! The theme editor's **Import…** and **Export…** (ADR-0052, SCALETTA F2.12b2b): the texts, what
//! importing does to the settings, and the dialogs of a theme file.
//!
//! # Importing (TF3, TF4)
//! Always a new theme, never a replacement: [`import`] takes the file's theme, gives it a name
//! (the file's own, else the file's name without the extension), made unique among the custom
//! themes ("Night (2)"), and chooses it. On the way it applies the application's rules:
//! - a base this Orchestra does not know becomes Studio;
//! - text, lines and density are made the same in both halves, the light half's winning
//!   (ADR-0037's precision of F2.12b2a), so a file written by hand behaves like one written here;
//! - overrides of unknown tokens are kept (a newer Orchestra finds them again) but counted as not
//!   used, with the entries the file reader dropped.
//!
//! The count goes into a short report after the import, only when it is not zero.
//!
//! # Exporting (TF2)
//! The chosen custom theme as the settings hold it; the editor proposes its name as the file's.

use std::io::ErrorKind;
use std::path::Path;

use orchestra_i18n::{I18nError, Localizer};
use orchestra_io::settings::{CustomTheme, Settings, ThemeChoice, ThemeOverride};
use orchestra_io::theme_file::{ThemeFile, ThemeFileError};
use orchestra_ui::{BuiltInTheme, ButtonRole, DialogKind, DialogSpec};

use super::{ThemeMessage, unique_name};
use crate::demo::DemoMessage;
use crate::settings::convert::custom_theme_from_settings;
use crate::template::{ISOLATION_MARKS, Template};

/// The texts of a theme file in one language (CLAUDE.md §7). The system's reasons for a failed
/// read or write are the project's texts, as for the shortcut files (F2.12c2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FileTexts {
    /// `prefs-theme-import`.
    pub(crate) import: String,
    /// `prefs-theme-export`.
    pub(crate) export: String,
    /// `file-filter-themes`.
    pub(crate) filter: String,
    /// `file-import-theme-title`.
    pub(crate) import_title: String,
    /// `file-export-theme-title`.
    pub(crate) export_title: String,
    /// `prefs-theme-imported-title`.
    pub(crate) imported: String,
    /// `prefs-theme-imported-unused`, with `$file` and `$count`.
    pub(crate) unused: Template,
    /// `prefs-theme-import-error-title`, with `$file`.
    pub(crate) import_error: Template,
    /// `prefs-theme-export-error-title`, with `$file`.
    pub(crate) export_error: Template,
    /// `prefs-theme-import-error-not-theme`.
    pub(crate) not_theme: String,
    /// `prefs-theme-import-error-newer`.
    pub(crate) newer: String,
    /// `prefs-theme-import-error-damaged`.
    pub(crate) damaged: String,
    /// `project-open-error-not-found`.
    pub(crate) not_found: String,
    /// `project-open-error-permission`.
    pub(crate) read_permission: String,
    /// `project-open-error-read`.
    pub(crate) unreadable: String,
    /// `project-open-error-too-large`.
    pub(crate) too_large: String,
    /// `project-save-error-permission`.
    pub(crate) write_permission: String,
    /// `project-save-error-full`.
    pub(crate) disk_full: String,
    /// `project-save-error-write`.
    pub(crate) unwritable: String,
    /// `project-save-error-internal`.
    pub(crate) internal: String,
    /// `project-save-error-kept`.
    pub(crate) kept: String,
    /// `project-replace-title`, with `$file`.
    pub(crate) replace_title: Template,
    /// `project-replace-message`.
    pub(crate) replace_message: String,
    /// `dialog-replace`.
    pub(crate) replace: String,
    /// `dialog-ok`.
    pub(crate) ok: String,
}

impl FileTexts {
    /// The texts in `localizer`'s language.
    ///
    /// # Errors
    /// The localizer's, if a message is missing (the catalogues' parity tests prevent it).
    pub(crate) fn new(localizer: &Localizer) -> Result<Self, I18nError> {
        let text = |key: &str| -> Result<String, I18nError> {
            Ok(localizer.format(key, None)?.replace(ISOLATION_MARKS, ""))
        };
        let template = |key: &str, variables: &[&str]| Template::new(localizer, key, variables);
        Ok(Self {
            import: text("prefs-theme-import")?,
            export: text("prefs-theme-export")?,
            filter: text("file-filter-themes")?,
            import_title: text("file-import-theme-title")?,
            export_title: text("file-export-theme-title")?,
            imported: text("prefs-theme-imported-title")?,
            unused: template("prefs-theme-imported-unused", &["file", "count"])?,
            import_error: template("prefs-theme-import-error-title", &["file"])?,
            export_error: template("prefs-theme-export-error-title", &["file"])?,
            not_theme: text("prefs-theme-import-error-not-theme")?,
            newer: text("prefs-theme-import-error-newer")?,
            damaged: text("prefs-theme-import-error-damaged")?,
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
            replace: text("dialog-replace")?,
            ok: text("dialog-ok")?,
        })
    }

    /// Every text with values, with its number of variables (tests: each in place once).
    pub(crate) fn templates(&self) -> [(&Template, usize); 4] {
        [
            (&self.unused, 2),
            (&self.import_error, 1),
            (&self.export_error, 1),
            (&self.replace_title, 1),
        ]
    }
}

/// Whether `item` is one of the values shared by both halves (text, lines, density), not a
/// colour.
const fn is_shared(item: &ThemeOverride) -> bool {
    !matches!(item, ThemeOverride::Color { .. })
}

/// Adds `file`'s theme to `settings`' custom themes and chooses it (see the module
/// documentation); `stem` is the file's name without the extension, the name of a theme without
/// one. Answers how many of the file's entries are not used here (the report's count).
/// O(overrides + themes × tries for the name).
pub(crate) fn import(settings: &mut Settings, file: ThemeFile, stem: &str) -> usize {
    let ThemeFile { theme, dropped } = file;
    let mut unused = dropped;
    let mut theme = CustomTheme {
        name: if theme.name.trim().is_empty() {
            stem.to_owned()
        } else {
            theme.name.trim().to_owned()
        },
        ..theme
    };
    // An unknown base (or none): Studio, and one entry not used.
    if BuiltInTheme::from_id(&theme.base).is_none() {
        BuiltInTheme::default().id().clone_into(&mut theme.base);
        unused += 1;
    }
    // Text, lines and density: the light half's in both (ADR-0037, F2.12b2a). The dark half's
    // own values that differ are not used.
    let light_shared: Vec<ThemeOverride> = theme
        .light
        .iter()
        .filter(|i| is_shared(i))
        .cloned()
        .collect();
    unused += theme
        .dark
        .iter()
        .filter(|item| is_shared(item) && !light_shared.contains(item))
        .count();
    theme.dark.retain(|item| !is_shared(item));
    theme.dark.extend(light_shared);
    // Unknown tokens and values: kept, counted. The conversion the window uses lists them.
    let mut problems = Vec::new();
    let _ = custom_theme_from_settings(&theme, &mut problems);
    unused += problems.len();
    theme.name = unique_name(settings, &theme.name);
    settings.theme.choice = ThemeChoice::Custom(theme.name.clone());
    settings.theme.custom.push(theme);
    unused
}

/// The report after an import with `unused` entries not used, `name` the file's whole name; none
/// when every entry was used.
pub(crate) fn report(
    texts: &FileTexts,
    name: &str,
    unused: usize,
) -> Option<DialogSpec<DemoMessage>> {
    (unused > 0).then(|| {
        DialogSpec::new(
            DialogKind::Information,
            texts.imported.clone(),
            texts.unused.fill(&[name, &unused.to_string()]),
        )
        .button(
            texts.ok.clone(),
            ButtonRole::Affirmative,
            DemoMessage::Theme(ThemeMessage::Closed),
        )
    })
}

/// The question before replacing `target`, a file the chooser did not confirm (TF2, as *Save
/// as*), `cancel` the *Cancel* button's text.
pub(crate) fn replace_question(
    texts: &FileTexts,
    cancel: &str,
    target: &Path,
) -> DialogSpec<DemoMessage> {
    DialogSpec::new(
        DialogKind::Question,
        texts
            .replace_title
            .fill(&[&crate::project::file_name(target)]),
        texts.replace_message.clone(),
    )
    .detail(target.display().to_string())
    .button(
        texts.replace.clone(),
        ButtonRole::Destructive,
        DemoMessage::Theme(ThemeMessage::ExportReplace(true)),
    )
    .button(
        cancel.to_owned(),
        ButtonRole::Cancel,
        DemoMessage::Theme(ThemeMessage::ExportReplace(false)),
    )
}

/// The error of a theme file at `path` (TF5): `title` names the file, the message says why in
/// plain words, the path is the detail; `writing` adds that the file there is intact.
pub(crate) fn error_dialog(
    texts: &FileTexts,
    title: String,
    error: &ThemeFileError,
    path: &Path,
    writing: bool,
) -> DialogSpec<DemoMessage> {
    let message = match error {
        ThemeFileError::Io(failure) => match (failure.kind(), writing) {
            (ErrorKind::NotFound, false) => &texts.not_found,
            (ErrorKind::PermissionDenied, false) => &texts.read_permission,
            (_, false) => &texts.unreadable,
            (ErrorKind::PermissionDenied | ErrorKind::ReadOnlyFilesystem, true) => {
                &texts.write_permission
            }
            (ErrorKind::StorageFull | ErrorKind::QuotaExceeded, true) => &texts.disk_full,
            (_, true) => &texts.unwritable,
        },
        ThemeFileError::TooLarge { .. } => &texts.too_large,
        ThemeFileError::NotTheme => &texts.not_theme,
        ThemeFileError::NewerFormat { .. } => &texts.newer,
        ThemeFileError::Syntax { .. } | ThemeFileError::Migration { .. } => &texts.damaged,
        ThemeFileError::Encode(_) => &texts.internal,
    };
    let mut spec = DialogSpec::new(DialogKind::Error, title, message.clone())
        .detail(path.display().to_string());
    if writing {
        spec = spec.detail(texts.kept.clone());
    }
    spec.button(
        texts.ok.clone(),
        ButtonRole::Affirmative,
        DemoMessage::Theme(ThemeMessage::Closed),
    )
}

#[cfg(test)]
mod tests {
    //! Importing: the name, the base, the halves aligned, the count.
    use super::*;
    use orchestra_io::theme_file;

    /// A theme file's content read from `text`.
    fn read(text: &str) -> ThemeFile {
        theme_file::from_bytes(text.as_bytes()).unwrap()
    }

    /// A theme is added, chosen, named after the file when it has no name, and made unique.
    #[test]
    fn an_import_adds_a_theme_and_chooses_it() {
        let mut settings = Settings::default();
        let file = read(r#"{ "orchestra_theme": 1, "theme": { "base": "slate" } }"#);
        assert_eq!(import(&mut settings, file.clone(), "Night"), 0);
        assert_eq!(
            settings.theme.choice,
            ThemeChoice::Custom("Night".to_owned())
        );
        assert_eq!(import(&mut settings, file, "Night"), 0);
        assert_eq!(
            settings.theme.choice,
            ThemeChoice::Custom("Night (2)".to_owned()),
            "never a replacement"
        );
        assert_eq!(settings.theme.custom.len(), 2);
    }

    /// An unknown base becomes Studio; dark-only text, lines and density give way to the light
    /// half's; unknown tokens are kept; each counts as not used.
    #[test]
    fn unknown_and_mismatched_entries_are_counted() {
        let mut settings = Settings::default();
        let file = read(
            r##"{ "orchestra_theme": 1, "theme": { "name": "Odd", "base": "from-the-future",
                "light": [ { "shape": "corner-radius", "value": 4 } ],
                "dark": [ { "shape": "corner-radius", "value": 9 },
                          { "color": "future-token", "value": "#000000ff" } ] } }"##,
        );
        // The base (1), the dark corners differing (1), the unknown colour token (1).
        assert_eq!(import(&mut settings, file, "x"), 3);
        let theme = &settings.theme.custom[0];
        assert_eq!(theme.base, BuiltInTheme::default().id());
        let shared = |list: &[ThemeOverride]| {
            list.iter()
                .filter(|i| is_shared(i))
                .cloned()
                .collect::<Vec<_>>()
        };
        assert_eq!(
            shared(&theme.dark),
            shared(&theme.light),
            "the light half's"
        );
        assert!(
            theme.dark.iter().any(
                |i| matches!(i, ThemeOverride::Color { token, .. } if token == "future-token")
            ),
            "kept for a newer Orchestra"
        );
    }
}
