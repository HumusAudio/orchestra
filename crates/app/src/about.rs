// SPDX-License-Identifier: GPL-3.0-or-later

//! The *About Orchestra* dialog (ADR-0050 DG10, SCALETTA F2.12a3b): its texts from the
//! catalogues and the dialog built from them.
//!
//! # What it shows (DG10, owner's choices 2–4 of F2.12a3b)
//! From top to bottom: the **logo** (recoloured with the theme's text colour, never reshaped:
//! `orchestra_ui::Logo`), the title *About Orchestra*, the **version**, then in the muted colour
//! the **licence** (GNU GPL 3.0 or later), the note that the **logo is not under it** (all rights
//! reserved, `assets/logo/README.md`), the **source repository's address** as plain text (choice
//! 2: no clickable link until a link widget and a way to open the browser exist), and the
//! **bundled fonts** with their licence (ADR-0044: Inter, Bravura, JetBrains Mono, all under the
//! SIL Open Font License 1.1). One button, *Close*, which Escape and Enter on it press.
//!
//! # Where the version and the address come from (choice 3)
//! From Cargo, at compile time: [`VERSION`] is the workspace's `version` (`CARGO_PKG_VERSION`),
//! [`REPOSITORY`] its `repository` (`CARGO_PKG_REPOSITORY`). No commit hash yet: a source archive
//! has no `.git` to read it from, so it comes with packaging (F6).
//!
//! # What is not here
//! The full texts of the licences and the list of every third-party crate's licence: they come
//! with packaging (F6), as DG10 says.

use orchestra_i18n::{FluentArgs, I18nError, Localizer};
use orchestra_ui::{ButtonRole, DialogKind, DialogSpec};

/// The program's version, from the workspace's `Cargo.toml` (choice 3 of F2.12a3b).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The address of the source repository, from the workspace's `Cargo.toml` (choice 2: shown as
/// text).
pub const REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");

/// The texts of the About dialog in one language, already formatted (CLAUDE.md §7).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AboutTexts {
    /// `menu-help-about`: the title ("About Orchestra"), the same text as the menu item.
    pub title: String,
    /// `about-version`, with [`VERSION`].
    pub version: String,
    /// `about-license`.
    pub license: String,
    /// `about-logo-notice`.
    pub logo_notice: String,
    /// `about-source`, with [`REPOSITORY`].
    pub source: String,
    /// `about-fonts`.
    pub fonts: String,
    /// `about-logo`: the logo's accessible name.
    pub logo: String,
    /// `dialog-close`: the button.
    pub close: String,
}

impl AboutTexts {
    /// The texts in `localizer`'s language.
    ///
    /// # Why the isolation marks are removed
    /// Fluent surrounds every placeable (the program's name, the version, the address) with the
    /// invisible marks U+2068 and U+2069. All three are ours and left-to-right, and the texts go
    /// to labels and to the accessibility tree, where a screen reader would meet the marks: they
    /// are dropped, as for the actions' names (`crate::actions`).
    ///
    /// # Errors
    /// The localizer's, if a message is missing (the catalogues' parity tests prevent it).
    pub fn new(localizer: &Localizer) -> Result<Self, I18nError> {
        let text = |key: &str, args: Option<&FluentArgs<'_>>| -> Result<String, I18nError> {
            Ok(localizer
                .format(key, args)?
                .replace(['\u{2068}', '\u{2069}'], ""))
        };
        let mut version = FluentArgs::new();
        version.set("version", VERSION);
        let mut url = FluentArgs::new();
        url.set("url", REPOSITORY);
        Ok(Self {
            title: text("menu-help-about", None)?,
            version: text("about-version", Some(&version))?,
            license: text("about-license", None)?,
            logo_notice: text("about-logo-notice", None)?,
            source: text("about-source", Some(&url))?,
            fonts: text("about-fonts", None)?,
            logo: text("about-logo", None)?,
            close: text("dialog-close", None)?,
        })
    }
}

/// The About dialog (see the module documentation), whose *Close* button sends `close`. Opened
/// with `Ui::open_dialog`; an information dialog (it asks nothing), its one button a *cancel*
/// one so Escape closes it (ADR-0050 DG2, C4) and the focus starts on it.
#[must_use]
pub fn about_dialog<M>(texts: &AboutTexts, close: M) -> DialogSpec<M> {
    DialogSpec::new(
        DialogKind::Information,
        texts.title.clone(),
        texts.version.clone(),
    )
    .logo(texts.logo.clone())
    .detail(texts.license.clone())
    .detail(texts.logo_notice.clone())
    .detail(texts.source.clone())
    .detail(texts.fonts.clone())
    .button(texts.close.clone(), ButtonRole::Cancel, close)
}
