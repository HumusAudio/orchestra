// SPDX-License-Identifier: GPL-3.0-or-later

//! The **settings file** (ADR-0050 DG6–DG7, SCALETTA F2.12a2): the user's choices that belong to
//! the program, not to a project (the theme, the language, the shortcuts, the playback values).
//!
//! # The file
//! One file, `settings.json`, in the directory of [`directory`] (DG7). Plain JSON like the
//! project file (ADR-0029 S1), indented with two spaces and always written in the same order, so
//! that a person can read it and change it by hand. Its first field is the **format version**,
//! `"orchestra_settings": 1`, read before anything else (ADR-0010 §1):
//!
//! ```json
//! {
//!   "orchestra_settings": 1,
//!   "theme": {
//!     "use": "studio",
//!     "mode": "system",
//!     "custom": [
//!       { "name": "Mine", "base": "slate",
//!         "light": [ { "color": "accent", "value": "#3366ccff" } ], "dark": [] }
//!     ]
//!   },
//!   "language": "system",
//!   "caret_blink": true,
//!   "fine_modifier": "primary",
//!   "shortcuts": [ { "action": "file.save", "keys": ["Primary+s"] } ],
//!   "playback": {}
//! }
//! ```
//!
//! # Plain types of its own
//! This crate does not depend on `orchestra-ui`, and `orchestra-ui` does not depend on it (DG6):
//! the schema here is made of identifiers and strings (`"studio"`, `"accent"`,
//! `"Primary+Shift+z"`), numbers and lists, and checks only what it can check without knowing the
//! interface (a colour is `#rrggbbaa`, a custom theme named by `"use"` exists). `orchestra-app`
//! turns these values into the interface's types and reports what *they* refuse (an unknown pair,
//! token or action).
//!
//! # Lenient reading, unlike the project file
//! Settings must never stop the program from starting (DG6). So [`load`] and [`from_bytes`]
//! never fail:
//! - a **missing** file means the defaults;
//! - an **unknown field**, a **wrong type** or a **value out of range** gives the default for that
//!   entry and a [`Warning`], and the rest is kept;
//! - a file that is **not settings at all** (not JSON, no version, larger than
//!   [`MAX_FILE_BYTES`], an old version whose migration fails) is renamed `settings.json.broken`
//!   (replacing an earlier one, choice S6) and the defaults are used;
//! - a file of a **newer** version is read for what this version understands and marked
//!   **read-only** ([`Loaded::writable`] is `false`): it is never written over, so a newer
//!   Orchestra's choices are never lost (ADR-0010's spirit);
//! - a file that **cannot be read** (permissions, a disk error) is also read-only for the session:
//!   writing over a file we could not read could destroy the user's settings.
//!
//! # Order and duplicates
//! The lists that may repeat a key keep the file's order: the **shortcuts** are a list of
//! `{ "action", "keys" }` entries, not an object, so that ADR-0048 AM7's rule "the later entry
//! wins a conflict" has a well-defined "later" (a JSON object read by `serde_json` loses its order
//! and keeps only the last of two equal keys). The overrides of a custom theme are a list for the
//! same reason (ADR-0037 T7: the later override of a token wins). A custom theme name that appears
//! twice keeps the first and warns. The playback values have no order: an object, sorted by name
//! when written.
//!
//! # Playback values (choice S2)
//! The values of ADR-0027 §14–§15 do not have a configurable form yet (F2.12a4 gives them one),
//! so the file keeps them as a map from a **name** to an **integer**. Names are not checked here:
//! an unknown one is kept and written back unchanged, like an unknown action in AM7, so the
//! version-1 format does not change when F2.12a4 fixes the names.
//!
//! # Cost
//! A settings file is a few kilobytes: reading goes through a `serde_json::Value` tree (needed
//! anyway to read entry by entry), O(size of the file); writing is O(size of the settings). Both
//! run at start-up and after a change, never per frame; the benchmarks of `benches/settings.rs`
//! measure them in batches.

mod migrate;
mod read;
mod write;

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub use crate::migration::MigrationError;
use crate::safe_write::IoFailure;

/// The version of the settings file this program writes, and the newest it fully understands.
pub const FORMAT_VERSION: u32 = 1;

/// The largest settings file read (1 MiB). Ordinary files are a few kilobytes; a larger one is not
/// a settings file (or is damaged), and refusing it bounds the work at start-up.
pub const MAX_FILE_BYTES: u64 = 1 << 20;

/// The name of the file inside its directory.
pub const FILE_NAME: &str = "settings.json";

/// The name a broken file is renamed to, beside it (S6).
pub const BROKEN_FILE_NAME: &str = "settings.json.broken";

/// The identifier of the default pair, Studio (ADR-0037 T1). Written here as text because this
/// crate does not know the interface's types; `orchestra-app` checks in a test that it is the
/// interface's default.
pub const DEFAULT_PAIR: &str = "studio";

/// Everything the settings file stores (DG6): only the user's choices, each with a default.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// The theme: the pair or custom theme in use, the mode, the custom themes.
    pub theme: ThemeSettings,
    /// The interface language.
    pub language: Language,
    /// Whether the text caret blinks (ADR-0047 TF8); `true` by default.
    pub caret_blink: bool,
    /// The modifier of fine adjustment (ADR-0046 V3); the primary modifier by default.
    pub fine_modifier: FineModifier,
    /// The user's shortcuts, as differences from the defaults (ADR-0048 AM7), in the file's
    /// order.
    pub shortcuts: Vec<ShortcutEntry>,
    /// The playback values that differ from the defaults, by name (choice S2).
    pub playback: BTreeMap<String, i64>,
}

impl Default for Settings {
    /// The defaults: Studio in the *System* mode, no custom theme, the system's language, a
    /// blinking caret, the primary modifier, no changed shortcut or playback value.
    fn default() -> Self {
        Self {
            theme: ThemeSettings::default(),
            language: Language::System,
            caret_blink: true,
            fine_modifier: FineModifier::Primary,
            shortcuts: Vec::new(),
            playback: BTreeMap::new(),
        }
    }
}

/// The theme's part of the settings (ADR-0037 T1, T7).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThemeSettings {
    /// What is in use: a built-in pair or one of [`custom`](Self::custom).
    pub choice: ThemeChoice,
    /// *System*, *Light* or *Dark*.
    pub mode: Mode,
    /// The user's custom themes, with unique names, in the file's order.
    pub custom: Vec<CustomTheme>,
}

impl Default for ThemeSettings {
    /// Studio, *System*, no custom theme.
    fn default() -> Self {
        Self {
            choice: ThemeChoice::BuiltIn(DEFAULT_PAIR.to_owned()),
            mode: Mode::System,
            custom: Vec::new(),
        }
    }
}

/// The theme in use, as written in `"use"`: a built-in pair's identifier (`"studio"`) or
/// `"custom:"` and a custom theme's name (`"custom:Mine"`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThemeChoice {
    /// A built-in pair, by its stable identifier (checked by the application).
    BuiltIn(String),
    /// A custom theme, by its name (one of [`ThemeSettings::custom`], checked here).
    Custom(String),
}

/// The prefix of a custom theme in `"use"`.
pub(crate) const CUSTOM_PREFIX: &str = "custom:";

/// The theme's mode (ADR-0037 T1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    /// Follow the system (`"system"`).
    #[default]
    System,
    /// Always light (`"light"`).
    Light,
    /// Always dark (`"dark"`).
    Dark,
}

/// A custom theme (ADR-0037 T7): a base pair plus overrides for each half.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomTheme {
    /// The name the user gave it, never empty, unique among the custom themes.
    pub name: String,
    /// The built-in pair it starts from, by identifier.
    pub base: String,
    /// The overrides of the light half, in order (the later one of a token wins).
    pub light: Vec<ThemeOverride>,
    /// The overrides of the dark half, in order.
    pub dark: Vec<ThemeOverride>,
}

/// One override of a custom theme, by the token's stable name (ADR-0037 T7; the interface's
/// `Override`). Each is written as a small object whose key says the kind:
/// `{ "color": "accent", "value": "#3366ccff" }`, `{ "shape": "corner-radius", "value": 4 }`,
/// `{ "family": "text-body", "value": "inter" }`, `{ "size": "text-body", "value": 14 }`,
/// `{ "density": "compact" }`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThemeOverride {
    /// A colour token and its colour, `0xRRGGBBAA` (written `#rrggbbaa`).
    Color {
        /// The colour token's name.
        token: String,
        /// The colour, red in the high byte, alpha in the low byte.
        rgba: u32,
    },
    /// A shape token and its value in logical pixels.
    Shape {
        /// The shape token's name.
        token: String,
        /// The value.
        value: u8,
    },
    /// A family token and its bundled family.
    Family {
        /// The family token's name.
        token: String,
        /// The family's name.
        family: String,
    },
    /// A size token and its size in logical pixels.
    TextSize {
        /// The size token's name.
        token: String,
        /// The size.
        size: u8,
    },
    /// The theme's density, by name.
    Density(String),
}

/// The interface language (DG9): the system's, or a tag of the six catalogues (`"it"`,
/// `"en-GB"`, checked by the application).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Language {
    /// The system's language (`"system"`); until F2.13 the program uses en-US for it.
    #[default]
    System,
    /// A language tag.
    Tag(String),
}

/// The modifier of fine adjustment (ADR-0046 V3, DG9).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FineModifier {
    /// The primary modifier, Ctrl or ⌘ (`"primary"`).
    #[default]
    Primary,
    /// Shift (`"shift"`).
    Shift,
    /// Alt or ⌥ Option (`"alt"`).
    Alt,
}

/// One action's shortcuts in the settings (ADR-0048 AM7): its whole list, which replaces the
/// defaults; an empty list means "no shortcut".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShortcutEntry {
    /// The action's stable identifier (`"file.save"`).
    pub action: String,
    /// The shortcuts in their settings text (`"Primary+s"`), read by the application.
    pub keys: Vec<String>,
}

/// Something in the settings file that was not used as written (DG6). Never an error: each one
/// is logged by the application, and the default is used in its place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Warning {
    /// The file could not be read; the defaults are used and the file is not written over this
    /// session.
    Unreadable(IoFailure),
    /// The file is not a settings file this program can read; it was renamed to `kept_as` (when
    /// the rename worked) and the defaults are used.
    Broken {
        /// Why it is not readable.
        reason: Broken,
        /// Where the broken file now is, or `None` if renaming it failed (the file is then not
        /// written over this session).
        kept_as: Option<PathBuf>,
    },
    /// The file was written by a newer Orchestra: it is read for what this version understands
    /// and is not written over.
    NewerFormat {
        /// The file's version.
        found: u64,
        /// The newest this program knows.
        supported: u32,
    },
    /// A field this version does not know; ignored. The path names it (`theme.colour`).
    UnknownField(String),
    /// A field of the wrong JSON type; its default is used.
    WrongType(String),
    /// A field whose value is not allowed (an unknown word, a malformed colour, a number out of
    /// range, a custom theme that does not exist); its default is used.
    InvalidValue(String),
    /// A second custom theme with a name already used; it is dropped.
    Duplicate(String),
}

/// Why a file is not a settings file this program can read (DG6, S6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Broken {
    /// Larger than [`MAX_FILE_BYTES`].
    TooLarge {
        /// Its size.
        bytes: u64,
    },
    /// It does not begin with `{ "orchestra_settings": <version>`.
    NotSettings,
    /// It is not valid JSON.
    Syntax {
        /// The line of the error, from 1.
        line: usize,
        /// The column of the error, from 1.
        column: usize,
    },
    /// An older version whose migration failed.
    Migration {
        /// The version at which the migration stopped.
        from: u64,
        /// Why.
        reason: MigrationError,
    },
}

impl fmt::Display for Warning {
    /// A short English description for developers and the log (ADR-0008: never shown on screen).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unreadable(failure) => {
                write!(f, "cannot read {failure}; the defaults are used")
            }
            Self::Broken {
                reason,
                kept_as: Some(path),
            } => write!(
                f,
                "{reason}; kept as {} and the defaults are used",
                path.display()
            ),
            Self::Broken {
                reason,
                kept_as: None,
            } => write!(f, "{reason}; the defaults are used"),
            Self::NewerFormat { found, supported } => write!(
                f,
                "written by a newer Orchestra (version {found}; this one knows up to {supported}): \
                 read in part and not written over"
            ),
            Self::UnknownField(path) => write!(f, "unknown field `{path}` ignored"),
            Self::WrongType(path) => write!(f, "`{path}` has the wrong type; the default is used"),
            Self::InvalidValue(path) => {
                write!(f, "`{path}` has a value not allowed; the default is used")
            }
            Self::Duplicate(path) => write!(f, "`{path}` repeats a name; dropped"),
        }
    }
}

impl fmt::Display for Broken {
    /// A short English description for developers and the log.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLarge { bytes } => write!(f, "the settings file has {bytes} bytes, too many"),
            Self::NotSettings => f.write_str("not an Orchestra settings file"),
            Self::Syntax { line, column } => {
                write!(f, "invalid JSON at line {line}, column {column}")
            }
            Self::Migration { from, reason } => {
                write!(
                    f,
                    "cannot migrate the settings from version {from}: {reason}"
                )
            }
        }
    }
}

/// The settings read from bytes, with what was not used as written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parsed {
    /// The settings: the file's values where they were usable, the defaults elsewhere.
    pub settings: Settings,
    /// What was not used as written, in the order met.
    pub warnings: Vec<Warning>,
    /// `true` when the file was written by a newer version (also in `warnings`).
    pub newer: bool,
}

/// The settings read from a file, ready for the session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Loaded {
    /// The settings to use.
    pub settings: Settings,
    /// What was not used as written, in the order met (the application logs them).
    pub warnings: Vec<Warning>,
    /// `false` when the file must not be written over this session: it is newer, it could not be
    /// read, or a broken file could not be moved aside (DG6).
    pub writable: bool,
}

/// Why the settings could not be saved. The application logs it and keeps going: the settings
/// still apply to the session (DG6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveError {
    /// Creating the directory or writing the file failed.
    Io(IoFailure),
    /// The settings could not be encoded (never expected: every value is a plain string, number
    /// or list).
    Encode(String),
}

impl fmt::Display for SaveError {
    /// A short English description for developers and the log.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(failure) => write!(f, "cannot write the settings: {failure}"),
            Self::Encode(message) => write!(f, "cannot encode the settings: {message}"),
        }
    }
}

impl std::error::Error for SaveError {
    /// The I/O failure, when there is one.
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(failure) => Some(failure),
            Self::Encode(_) => None,
        }
    }
}

/// The directory of the settings file on this system (DG7), or `None` when the environment names
/// none (the program then runs with the defaults and does not save). O(1).
#[must_use]
pub fn directory() -> Option<PathBuf> {
    crate::platform::config_dir::directory(crate::platform::config_dir::System::CURRENT, |name| {
        std::env::var_os(name)
    })
}

/// The settings file's path on this system: [`directory`] joined with [`FILE_NAME`].
#[must_use]
pub fn default_path() -> Option<PathBuf> {
    directory().map(|directory| directory.join(FILE_NAME))
}

/// The settings encoded as the file writes them (see the module documentation): version first,
/// two-space indentation, a final newline. O(size of the settings).
///
/// # Errors
/// [`SaveError::Encode`] if `serde_json` fails (not expected for these plain values).
pub fn to_bytes(settings: &Settings) -> Result<Vec<u8>, SaveError> {
    write::to_bytes(settings)
}

/// Reads settings from the bytes of a file, leniently (see the module documentation).
/// O(size of the file).
///
/// # Errors
/// [`Broken`] when the bytes are not a settings file this program can read at all; the caller
/// then uses the defaults (and [`load`] moves the file aside).
pub fn from_bytes(bytes: &[u8]) -> Result<Parsed, Broken> {
    let size = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    if size > MAX_FILE_BYTES {
        return Err(Broken::TooLarge { bytes: size });
    }
    let found = crate::migration::leading_version(bytes, read::KEY_QUOTED.as_bytes())
        .ok_or(Broken::NotSettings)?;
    let body = bytes.strip_prefix(crate::migration::BOM).unwrap_or(bytes);
    let tree: serde_json::Value = serde_json::from_slice(body).map_err(|e| Broken::Syntax {
        line: e.line(),
        column: e.column(),
    })?;
    let current = u64::from(FORMAT_VERSION);
    let mut warnings = Vec::new();
    let tree = if found < current {
        migrate::run(tree, found, current)
            .map_err(|(from, reason)| Broken::Migration { from, reason })?
    } else {
        if found > current {
            warnings.push(Warning::NewerFormat {
                found,
                supported: FORMAT_VERSION,
            });
        }
        tree
    };
    let settings = read::settings(&tree, &mut warnings);
    Ok(Parsed {
        settings,
        warnings,
        newer: found > current,
    })
}

/// Reads the settings file at `path` for the session (see the module documentation). Never fails:
/// a missing file gives the defaults, a broken one is renamed to [`BROKEN_FILE_NAME`] beside it,
/// and every problem becomes a [`Warning`].
///
/// O(size of the file), plus one rename for a broken file.
#[must_use]
pub fn load(path: &Path) -> Loaded {
    let defaults = |warnings, writable| Loaded {
        settings: Settings::default(),
        warnings,
        writable,
    };
    let bytes = match read_bounded(path) {
        Ok(bytes) => bytes,
        // No file yet: the first start, nothing to warn about, and the first save creates it.
        Err(Read::Io(error)) if error.kind() == io::ErrorKind::NotFound => {
            return defaults(Vec::new(), true);
        }
        Err(Read::Io(error)) => {
            return defaults(
                vec![Warning::Unreadable(IoFailure::new(&error, path))],
                false,
            );
        }
        Err(Read::TooLarge(bytes)) => return set_aside(path, Broken::TooLarge { bytes }),
    };
    match from_bytes(&bytes) {
        Ok(parsed) => Loaded {
            settings: parsed.settings,
            warnings: parsed.warnings,
            writable: !parsed.newer,
        },
        Err(reason) => set_aside(path, reason),
    }
}

/// Writes `settings` to `path` safely (ADR-0029 S13, through `crate::safe_write`), creating its
/// directory first when it does not exist. O(size of the settings), plus a constant number of
/// system calls.
///
/// # Errors
/// [`SaveError`]: the directory could not be created, or the file could not be written (the
/// previous file is then intact).
pub fn save(path: &Path, settings: &Settings) -> Result<(), SaveError> {
    let bytes = to_bytes(settings)?;
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent).map_err(|e| SaveError::Io(IoFailure::new(&e, parent)))?;
    }
    crate::safe_write::write(path, &bytes).map_err(SaveError::Io)
}

/// Why [`read_bounded`] gave no bytes.
enum Read {
    /// The operating system's error.
    Io(io::Error),
    /// The file is larger than [`MAX_FILE_BYTES`] (its size, or what was read of it).
    TooLarge(u64),
}

/// The bytes of the file at `path`, reading at most one byte more than [`MAX_FILE_BYTES`], so a
/// huge file costs no more than the limit. O(size of the file, bounded).
fn read_bounded(path: &Path) -> Result<Vec<u8>, Read> {
    use std::io::Read as _;
    let file = fs::File::open(path).map_err(Read::Io)?;
    let size = file.metadata().map_err(Read::Io)?.len();
    if size > MAX_FILE_BYTES {
        return Err(Read::TooLarge(size));
    }
    // The size sizes the buffer; it fits `usize` because it is at most 1 MiB.
    let mut bytes = Vec::with_capacity(usize::try_from(size).unwrap_or(0));
    file.take(MAX_FILE_BYTES.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(Read::Io)?;
    Ok(bytes)
}

/// Moves a broken file aside to [`BROKEN_FILE_NAME`] (replacing an earlier one, S6) and gives the
/// defaults. If the rename fails, the file stays where it is and is not written over this session.
fn set_aside(path: &Path, reason: Broken) -> Loaded {
    let kept = path.with_file_name(BROKEN_FILE_NAME);
    // `fs::rename` replaces an existing target on Unix and Windows alike (`MoveFileExW` with
    // `MOVEFILE_REPLACE_EXISTING` in the standard library).
    let kept_as = fs::rename(path, &kept).ok().map(|()| kept);
    let writable = kept_as.is_some();
    Loaded {
        settings: Settings::default(),
        warnings: vec![Warning::Broken { reason, kept_as }],
        writable,
    }
}
