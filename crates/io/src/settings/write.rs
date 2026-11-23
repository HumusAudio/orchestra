// SPDX-License-Identifier: GPL-3.0-or-later

//! The writer of the settings file (ADR-0050 DG6): [`Settings`] → the bytes of the file.
//!
//! # Why `serde` structs that borrow
//! The order of the fields is part of the format (the version first, so the scanner finds it;
//! then a fixed order, so a person reads the same file every time). A `serde_json::Value` object
//! would sort its keys, putting `caret_blink` before `orchestra_settings`; structs with
//! `#[derive(Serialize)]` keep the declared order. They borrow from [`Settings`], so writing copies
//! nothing but the few texts made here (the colours and the `use` value).
//!
//! The output is `serde_json`'s pretty form (two spaces per level) and a final newline, the same
//! on every system: `\n` only (ADR-0029 S3 for the project file; `serde_json` never writes
//! `\r\n`).

use std::collections::BTreeMap;

use serde::Serialize;

use super::{
    CUSTOM_PREFIX, FORMAT_VERSION, FineModifier, Language, Mode, SaveError, Settings, ThemeChoice,
    ThemeOverride,
};

/// The whole file, in its written order.
#[derive(Serialize)]
struct File<'a> {
    /// The format version, first (DG6).
    orchestra_settings: u32,
    /// The theme.
    theme: Theme<'a>,
    /// `"system"` or a language tag.
    language: &'a str,
    /// Whether the caret blinks.
    caret_blink: bool,
    /// `"primary"`, `"shift"` or `"alt"`.
    fine_modifier: &'static str,
    /// The changed shortcuts, in order.
    shortcuts: Vec<Shortcut<'a>>,
    /// The changed playback values, sorted by name (a `BTreeMap`).
    playback: &'a BTreeMap<String, i64>,
}

/// The theme's object.
#[derive(Serialize)]
struct Theme<'a> {
    /// The pair's identifier, or `custom:` and a name.
    #[serde(rename = "use")]
    choice: String,
    /// `"system"`, `"light"` or `"dark"`.
    mode: &'static str,
    /// The custom themes.
    custom: Vec<Custom<'a>>,
}

/// One custom theme.
#[derive(Serialize)]
struct Custom<'a> {
    /// Its name.
    name: &'a str,
    /// Its base pair.
    base: &'a str,
    /// The light half's overrides.
    light: Vec<Override<'a>>,
    /// The dark half's overrides.
    dark: Vec<Override<'a>>,
}

/// One override, as a small object whose first key says the kind (`untagged`: the variant's
/// fields are written as they are, with no tag around them).
#[derive(Serialize)]
#[serde(untagged)]
enum Override<'a> {
    /// `{ "color": token, "value": "#rrggbbaa" }`.
    Color {
        /// The token.
        color: &'a str,
        /// The colour in hexadecimal.
        value: String,
    },
    /// `{ "shape": token, "value": n }`.
    Shape {
        /// The token.
        shape: &'a str,
        /// The value.
        value: u8,
    },
    /// `{ "family": token, "value": family }`.
    Family {
        /// The token.
        family: &'a str,
        /// The family.
        value: &'a str,
    },
    /// `{ "size": token, "value": n }`.
    Size {
        /// The token.
        size: &'a str,
        /// The size.
        value: u8,
    },
    /// `{ "density": name }`.
    Density {
        /// The density's name.
        density: &'a str,
    },
}

/// One shortcut entry.
#[derive(Serialize)]
struct Shortcut<'a> {
    /// The action.
    action: &'a str,
    /// Its shortcuts' texts.
    keys: &'a [String],
}

/// Encodes `settings` (see the module documentation). O(size of the settings).
///
/// # Errors
/// [`SaveError::Encode`] if `serde_json` fails (not expected for these values).
pub(crate) fn to_bytes(settings: &Settings) -> Result<Vec<u8>, SaveError> {
    let theme = &settings.theme;
    let file = File {
        orchestra_settings: FORMAT_VERSION,
        theme: Theme {
            choice: match &theme.choice {
                ThemeChoice::BuiltIn(id) => id.clone(),
                ThemeChoice::Custom(name) => format!("{CUSTOM_PREFIX}{name}"),
            },
            mode: match theme.mode {
                Mode::System => "system",
                Mode::Light => "light",
                Mode::Dark => "dark",
            },
            custom: theme
                .custom
                .iter()
                .map(|custom| Custom {
                    name: &custom.name,
                    base: &custom.base,
                    light: custom.light.iter().map(to_override).collect(),
                    dark: custom.dark.iter().map(to_override).collect(),
                })
                .collect(),
        },
        language: match &settings.language {
            Language::System => "system",
            Language::Tag(tag) => tag,
        },
        caret_blink: settings.caret_blink,
        fine_modifier: match settings.fine_modifier {
            FineModifier::Primary => "primary",
            FineModifier::Shift => "shift",
            FineModifier::Alt => "alt",
        },
        shortcuts: settings
            .shortcuts
            .iter()
            .map(|entry| Shortcut {
                action: &entry.action,
                keys: &entry.keys,
            })
            .collect(),
        playback: &settings.playback,
    };
    let mut bytes =
        serde_json::to_vec_pretty(&file).map_err(|e| SaveError::Encode(e.to_string()))?;
    bytes.push(b'\n');
    Ok(bytes)
}

/// The written form of one override.
fn to_override(item: &ThemeOverride) -> Override<'_> {
    match item {
        ThemeOverride::Color { token, rgba } => Override::Color {
            color: token,
            value: format!("#{rgba:08x}"),
        },
        ThemeOverride::Shape { token, value } => Override::Shape {
            shape: token,
            value: *value,
        },
        ThemeOverride::Family { token, family } => Override::Family {
            family: token,
            value: family,
        },
        ThemeOverride::TextSize { token, size } => Override::Size {
            size: token,
            value: *size,
        },
        ThemeOverride::Density(name) => Override::Density { density: name },
    }
}
