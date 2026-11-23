// SPDX-License-Identifier: GPL-3.0-or-later

//! The lenient reader of the settings file (ADR-0050 DG6): from a JSON tree to [`Settings`],
//! entry by entry, with the default and a [`Warning`] wherever an entry is not usable.
//!
//! # Why by hand, not with `serde`'s derives
//! A derived `Deserialize` fails the whole value at the first wrong field; here every field must
//! fail **alone**, keeping its siblings. Walking the `serde_json::Value` tree gives that, and each
//! warning names the place with a path (`theme.custom[0].light[2].value`) built while walking.
//!
//! # Paths
//! The path of a field is its keys joined with dots, with `[i]` for list positions. It is built
//! only when a warning is made, from the parent's path: reading a valid file builds none.

use serde_json::{Map, Value};

use super::{
    CUSTOM_PREFIX, CustomTheme, FineModifier, Language, Mode, Settings, ShortcutEntry, ThemeChoice,
    ThemeOverride, ThemeSettings, Warning,
};

/// The key of the version (ADR-0050 DG6).
pub(crate) const KEY: &str = "orchestra_settings";

/// [`KEY`] in quotes, as the version scanner looks for it.
pub(crate) const KEY_QUOTED: &str = "\"orchestra_settings\"";

/// The word of a built-in language choice.
const SYSTEM: &str = "system";

/// Reads the settings from `tree`. Never fails: a tree that is not an object gives the defaults
/// with one warning. O(size of the tree).
pub(crate) fn settings(tree: &Value, warnings: &mut Vec<Warning>) -> Settings {
    let mut settings = Settings::default();
    let Some(map) = tree.as_object() else {
        warnings.push(Warning::WrongType(String::new()));
        return settings;
    };
    for (key, value) in map {
        match key.as_str() {
            KEY => {}
            "theme" => {
                if let Some(theme) = object(value, key, warnings) {
                    settings.theme = read_theme(theme, warnings);
                }
            }
            "language" => {
                if let Some(text) = string(value, key, warnings) {
                    settings.language = match text {
                        "" => {
                            warnings.push(Warning::InvalidValue(key.clone()));
                            Language::System
                        }
                        SYSTEM => Language::System,
                        tag => Language::Tag(tag.to_owned()),
                    };
                }
            }
            "caret_blink" => match value.as_bool() {
                Some(blink) => settings.caret_blink = blink,
                None => warnings.push(Warning::WrongType(key.clone())),
            },
            "fine_modifier" => {
                if let Some(text) = string(value, key, warnings) {
                    match fine_modifier(text) {
                        Some(modifier) => settings.fine_modifier = modifier,
                        None => warnings.push(Warning::InvalidValue(key.clone())),
                    }
                }
            }
            "shortcuts" => {
                if let Some(list) = array(value, key, warnings) {
                    settings.shortcuts = read_shortcuts(list, key, warnings);
                }
            }
            "playback" => {
                if let Some(values) = object(value, key, warnings) {
                    for (name, number) in values {
                        match number.as_i64() {
                            Some(number) => {
                                settings.playback.insert(name.clone(), number);
                            }
                            None => warnings.push(Warning::WrongType(join(key, name))),
                        }
                    }
                }
            }
            _ => warnings.push(Warning::UnknownField(key.clone())),
        }
    }
    settings
}

/// The theme's object: `use`, `mode`, `custom`. The custom themes are read first, because `use`
/// may name one of them.
fn read_theme(map: &Map<String, Value>, warnings: &mut Vec<Warning>) -> ThemeSettings {
    let mut theme = ThemeSettings::default();
    if let Some(list) = map
        .get("custom")
        .and_then(|value| array(value, "theme.custom", warnings))
    {
        theme.custom = read_custom_themes(list, warnings);
    }
    for (key, value) in map {
        let path = join("theme", key);
        match key.as_str() {
            "custom" => {}
            "use" => {
                if let Some(text) = string(value, &path, warnings) {
                    match choice(text, &theme.custom) {
                        Some(choice) => theme.choice = choice,
                        None => warnings.push(Warning::InvalidValue(path)),
                    }
                }
            }
            "mode" => {
                if let Some(text) = string(value, &path, warnings) {
                    match mode(text) {
                        Some(mode) => theme.mode = mode,
                        None => warnings.push(Warning::InvalidValue(path)),
                    }
                }
            }
            _ => warnings.push(Warning::UnknownField(path)),
        }
    }
    theme
}

/// The theme named by `use`: `"custom:"` and an existing custom theme's name, or a non-empty
/// identifier of a built-in pair (checked by the application). `None` otherwise.
fn choice(text: &str, custom: &[CustomTheme]) -> Option<ThemeChoice> {
    match text.strip_prefix(CUSTOM_PREFIX) {
        Some(name) => custom
            .iter()
            .any(|theme| theme.name == name)
            .then(|| ThemeChoice::Custom(name.to_owned())),
        None => (!text.is_empty()).then(|| ThemeChoice::BuiltIn(text.to_owned())),
    }
}

/// The mode's word.
fn mode(text: &str) -> Option<Mode> {
    match text {
        "system" => Some(Mode::System),
        "light" => Some(Mode::Light),
        "dark" => Some(Mode::Dark),
        _ => None,
    }
}

/// The fine modifier's word.
fn fine_modifier(text: &str) -> Option<FineModifier> {
    match text {
        "primary" => Some(FineModifier::Primary),
        "shift" => Some(FineModifier::Shift),
        "alt" => Some(FineModifier::Alt),
        _ => None,
    }
}

/// The custom themes: each needs a non-empty, unused `name` and a non-empty `base`; one without
/// is dropped with a warning. O(themes² ) for the uniqueness check, over a few themes.
fn read_custom_themes(list: &[Value], warnings: &mut Vec<Warning>) -> Vec<CustomTheme> {
    let mut themes: Vec<CustomTheme> = Vec::with_capacity(list.len());
    for (index, value) in list.iter().enumerate() {
        let path = format!("theme.custom[{index}]");
        let Some(map) = object(value, &path, warnings) else {
            continue;
        };
        let mut name = None;
        let mut base = None;
        let mut light = Vec::new();
        let mut dark = Vec::new();
        for (key, value) in map {
            let field = join(&path, key);
            match key.as_str() {
                "name" => name = non_empty(value, &field, warnings),
                "base" => base = non_empty(value, &field, warnings),
                "light" => light = read_overrides(value, &field, warnings),
                "dark" => dark = read_overrides(value, &field, warnings),
                _ => warnings.push(Warning::UnknownField(field)),
            }
        }
        let (Some(name), Some(base)) = (name, base) else {
            // Without a name it cannot be chosen, without a base it cannot be resolved: dropped.
            warnings.push(Warning::InvalidValue(path));
            continue;
        };
        if themes.iter().any(|theme| theme.name == name) {
            warnings.push(Warning::Duplicate(join(&path, "name")));
            continue;
        }
        themes.push(CustomTheme {
            name,
            base,
            light,
            dark,
        });
    }
    themes
}

/// One half's overrides; an override that is not usable is dropped with a warning, the others are
/// kept in order.
fn read_overrides(value: &Value, path: &str, warnings: &mut Vec<Warning>) -> Vec<ThemeOverride> {
    let Some(list) = array(value, path, warnings) else {
        return Vec::new();
    };
    let mut overrides = Vec::with_capacity(list.len());
    for (index, value) in list.iter().enumerate() {
        let path = format!("{path}[{index}]");
        let Some(map) = object(value, &path, warnings) else {
            continue;
        };
        match read_override(map) {
            Some(item) => overrides.push(item),
            None => warnings.push(Warning::InvalidValue(path)),
        }
    }
    overrides
}

/// One override: exactly one kind key (`color`, `shape`, `family`, `size`, `density`) with a
/// non-empty token name, and a `value` of the kind's type (none for `density`). `None` when the
/// object is anything else (an extra key included: a half-understood override is not applied).
fn read_override(map: &Map<String, Value>) -> Option<ThemeOverride> {
    let token = |key: &str| {
        map.get(key)
            .and_then(Value::as_str)
            .filter(|t| !t.is_empty())
            .map(str::to_owned)
    };
    let value = map.get("value");
    let byte = || {
        value
            .and_then(Value::as_u64)
            .and_then(|v| u8::try_from(v).ok())
    };
    let item = if map.contains_key("color") {
        ThemeOverride::Color {
            token: token("color")?,
            rgba: value.and_then(Value::as_str).and_then(parse_color)?,
        }
    } else if map.contains_key("shape") {
        ThemeOverride::Shape {
            token: token("shape")?,
            value: byte()?,
        }
    } else if map.contains_key("family") {
        ThemeOverride::Family {
            token: token("family")?,
            family: value
                .and_then(Value::as_str)
                .filter(|f| !f.is_empty())?
                .to_owned(),
        }
    } else if map.contains_key("size") {
        ThemeOverride::TextSize {
            token: token("size")?,
            size: byte()?,
        }
    } else {
        // Density has no `value`: the key itself carries the name, so it has one key only.
        return (map.len() == 1)
            .then(|| token("density").map(ThemeOverride::Density))
            .flatten();
    };
    // The kind key and `value`, nothing else.
    (map.len() == 2 && value.is_some()).then_some(item)
}

/// `#rrggbbaa` (or `#rrggbb`, opaque) as `0xRRGGBBAA`; hexadecimal digits in either case.
pub(crate) fn parse_color(text: &str) -> Option<u32> {
    let digits = text.strip_prefix('#')?;
    if !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let value = u32::from_str_radix(digits, 16).ok()?;
    match digits.len() {
        8 => Some(value),
        6 => Some(value << 8 | 0xff),
        _ => None,
    }
}

/// The shortcut entries, in the file's order: each needs a non-empty `action` and a `keys` list
/// of strings (an empty list is "no shortcut"). A key that is not a string is dropped alone; an
/// entry without `action` or `keys` is dropped.
fn read_shortcuts(list: &[Value], path: &str, warnings: &mut Vec<Warning>) -> Vec<ShortcutEntry> {
    let mut entries = Vec::with_capacity(list.len());
    for (index, value) in list.iter().enumerate() {
        let path = format!("{path}[{index}]");
        let Some(map) = object(value, &path, warnings) else {
            continue;
        };
        let mut action = None;
        let mut keys = None;
        for (key, value) in map {
            let field = join(&path, key);
            match key.as_str() {
                "action" => action = non_empty(value, &field, warnings),
                "keys" => {
                    if let Some(list) = array(value, &field, warnings) {
                        let mut texts = Vec::with_capacity(list.len());
                        for (k, item) in list.iter().enumerate() {
                            match item.as_str() {
                                Some(text) => texts.push(text.to_owned()),
                                None => warnings.push(Warning::WrongType(format!("{field}[{k}]"))),
                            }
                        }
                        keys = Some(texts);
                    }
                }
                _ => warnings.push(Warning::UnknownField(field)),
            }
        }
        match (action, keys) {
            (Some(action), Some(keys)) => entries.push(ShortcutEntry { action, keys }),
            _ => warnings.push(Warning::InvalidValue(path)),
        }
    }
    entries
}

/// `value` as an object, or a [`Warning::WrongType`] at `path`.
fn object<'v>(
    value: &'v Value,
    path: &str,
    warnings: &mut Vec<Warning>,
) -> Option<&'v Map<String, Value>> {
    let map = value.as_object();
    if map.is_none() {
        warnings.push(Warning::WrongType(path.to_owned()));
    }
    map
}

/// `value` as an array, or a [`Warning::WrongType`] at `path`.
fn array<'v>(value: &'v Value, path: &str, warnings: &mut Vec<Warning>) -> Option<&'v [Value]> {
    let list = value.as_array().map(Vec::as_slice);
    if list.is_none() {
        warnings.push(Warning::WrongType(path.to_owned()));
    }
    list
}

/// `value` as a string, or a [`Warning::WrongType`] at `path`.
fn string<'v>(value: &'v Value, path: &str, warnings: &mut Vec<Warning>) -> Option<&'v str> {
    let text = value.as_str();
    if text.is_none() {
        warnings.push(Warning::WrongType(path.to_owned()));
    }
    text
}

/// `value` as a non-empty string, owned; a warning otherwise (wrong type, or empty).
fn non_empty(value: &Value, path: &str, warnings: &mut Vec<Warning>) -> Option<String> {
    let text = string(value, path, warnings)?;
    if text.is_empty() {
        warnings.push(Warning::InvalidValue(path.to_owned()));
        return None;
    }
    Some(text.to_owned())
}

/// `parent.key`, or `key` alone at the top.
fn join(parent: &str, key: &str) -> String {
    if parent.is_empty() {
        key.to_owned()
    } else {
        format!("{parent}.{key}")
    }
}

#[cfg(test)]
mod tests {
    //! The reader's small parts; the whole file's cases are in `tests/settings_cases.rs`.
    use super::*;

    /// Colours with and without alpha, in both cases; malformed ones refused.
    #[test]
    fn colours() {
        assert_eq!(parse_color("#3366ccff"), Some(0x3366_ccff));
        assert_eq!(parse_color("#3366CC"), Some(0x3366_ccff));
        for bad in ["3366cc", "#36c", "#3366ccf", "#+366ccff", "#3366ccgg", "#"] {
            assert_eq!(parse_color(bad), None, "{bad}");
        }
    }
}
