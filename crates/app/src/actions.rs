// SPDX-License-Identifier: GPL-3.0-or-later

//! The program's actions (ADR-0048 AM1, AM6; F2.11d1): the concrete list, with the default
//! shortcuts of each platform and the Fluent keys of their names. The generic machinery (the
//! registry, the lookup, the contexts) is `orchestra-ui`'s; this module only fills it.
//!
//! # Which actions, and their state today (choice E of F2.11d1)
//! The fourteen of ADR-0048 AM6's table, so that the menus of F2.11d2 have real items, and
//! *About Orchestra* (F2.12a3b, ADR-0050 DG10), fifteen in all:
//! - **working:** Quit and Full screen (they ask the window, [`orchestra_ui::WindowRequest`]),
//!   About (it opens the About dialog, [`crate::about`]), Preferences (F2.12a4a,
//!   [`crate::preferences`]), and the six editing actions on a focused text field (AM13: the
//!   interface sends them to the field itself);
//! - **registered but disabled** until what they do exists: New, Open, Save, Save as (the project
//!   file in the interface), Help (the guides), and the editing actions outside a field (there is
//!   no document view yet to undo in or select from).
//!
//! # Names (AM10)
//! An action's name is the Fluent key of its menu item (`menu-edit-undo`), so the menus of
//! F2.11d2, the search of actions and the tooltips show one text, translated once.
//!
//! # The defaults (AM6)
//! | Action | Linux | Windows | macOS |
//! |---|---|---|---|
//! | New, Open, Save | Ctrl+N, O, S | Ctrl+N, O, S | ⌘N, O, S |
//! | Save as | Ctrl+Shift+S | Ctrl+Shift+S | ⇧⌘S |
//! | Quit | Ctrl+Q | none: Alt+F4 is the system's, a close request | ⌘Q |
//! | Undo | Ctrl+Z | Ctrl+Z | ⌘Z |
//! | Redo | Ctrl+Shift+Z | Ctrl+Y, Ctrl+Shift+Z | ⇧⌘Z |
//! | Cut, Copy, Paste, Select all | Ctrl+X, C, V, A | Ctrl+X, C, V, A | ⌘X, C, V, A |
//! | Preferences | Ctrl+, | Ctrl+, | ⌘, |
//! | Full screen | F11 | F11 | ⌃⌘F |
//! | Help | F1 | F1 | ⌘? |
//! | About Orchestra | none | none | none (as every Mac program) |
//!
//! They are written with the abstract modifiers (*Primary* is Ctrl or ⌘, *Secondary* is the
//! Windows/Super key or ⌃), so one table serves the three platforms, chosen by value: the tests
//! on Linux build the registries of all three.
//!
//! # The texts people read (F2.11d2a, choice S1)
//! [`action_texts`] reads from the catalogues every action's name and every key's name (`key-ctrl`,
//! `key-shift`… researched for the six languages, see `crates/i18n/glossary.toml`), and
//! [`ActionTexts::apply`] hands them to the registry, which writes each shortcut in the platform's
//! style ("Strg+Umschalt+S", "⇧⌘S"). The names of the items that open a window end with an
//! ellipsis in the catalogues themselves (choice S2: "Open…", "Save as…", "Preferences…").
//!
//! # The menu bar and the mnemonics (F2.11d2b)
//! [`menu_bar`] is the drawn bar of Linux and Windows (ADR-0048 AM9): File (New, Open…, Save,
//! Save as…, Quit), Edit (Undo, Redo, Cut, Copy, Paste, Select all, Preferences…), View (Full
//! screen) and Help (User guide, then About Orchestra after a separator, as GNOME, KDE and Windows
//! programs place it). Each title and item has a **mnemonic**, the `.mnemonic`
//! attribute of its message, chosen for each language from the shipped catalogues of a music program and
//! unique among its siblings (tested in the six languages; glossary entries `menu_mnemonics`).
//!
//! The roles of ADR-0048 AM9 are on the model too: About, Preferences and Quit go into macOS's
//! application menu (About first), the Help menu is given to the system (F2.11e1); on Linux and Windows they
//! change nothing.
//!
//! # The field's context menu (F2.11d2a)
//! [`field_menu`] is the standard editing menu of a text field (ADR-0048 AM13): Undo, Redo, then
//! Cut, Copy, Paste, then Select all. Its items act on the focused field.

use orchestra_i18n::{FluentArgs, I18nError, Localizer};
use orchestra_ui::{
    ActionId, ActionSpec, ActionState, Context, Defaults, EditRole, KeyName, KeyNames, MenuModel,
    MenuRole, Modifiers, NamedKey, Platform, Registry, RegistryError, Shortcut, ShortcutError,
    ShortcutKey, SystemTexts,
};

/// The program's actions, by their index in the registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActionIds {
    /// `file.new`.
    pub new: ActionId,
    /// `file.open`.
    pub open: ActionId,
    /// `file.save`.
    pub save: ActionId,
    /// `file.save-as`.
    pub save_as: ActionId,
    /// `file.quit`.
    pub quit: ActionId,
    /// `edit.undo`.
    pub undo: ActionId,
    /// `edit.redo`.
    pub redo: ActionId,
    /// `edit.cut`.
    pub cut: ActionId,
    /// `edit.copy`.
    pub copy: ActionId,
    /// `edit.paste`.
    pub paste: ActionId,
    /// `edit.select-all`.
    pub select_all: ActionId,
    /// `edit.preferences`.
    pub preferences: ActionId,
    /// `view.full-screen`.
    pub full_screen: ActionId,
    /// `help.contents`.
    pub help: ActionId,
    /// `help.about` (F2.12a3b).
    pub about: ActionId,
}

impl ActionIds {
    /// Every action, in registration order.
    #[must_use]
    pub const fn all(&self) -> [ActionId; 15] {
        [
            self.new,
            self.open,
            self.save,
            self.save_as,
            self.quit,
            self.undo,
            self.redo,
            self.cut,
            self.copy,
            self.paste,
            self.select_all,
            self.preferences,
            self.full_screen,
            self.help,
            self.about,
        ]
    }
}

/// Why the program's registry could not be built: its own tables are wrong (the tests of this
/// crate build it for the three platforms, so this never reaches a user).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionsError {
    /// A default shortcut is malformed.
    Shortcut(ShortcutError),
    /// The registry refused an action.
    Registry(RegistryError),
}

impl std::fmt::Display for ActionsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Shortcut(error) => write!(f, "a default shortcut is malformed: {error}"),
            Self::Registry(error) => write!(f, "the registry refused an action: {error}"),
        }
    }
}

impl std::error::Error for ActionsError {}

impl From<ShortcutError> for ActionsError {
    fn from(error: ShortcutError) -> Self {
        Self::Shortcut(error)
    }
}

impl From<RegistryError> for ActionsError {
    fn from(error: RegistryError) -> Self {
        Self::Registry(error)
    }
}

/// No modifier.
const NONE: Modifiers = Modifiers {
    primary: false,
    secondary: false,
    shift: false,
    alt: false,
};

/// *Primary* (Ctrl, ⌘).
const PRIMARY: Modifiers = Modifiers {
    primary: true,
    ..NONE
};

/// *Primary* and Shift.
const PRIMARY_SHIFT: Modifiers = Modifiers {
    shift: true,
    ..PRIMARY
};

/// *Primary* and *Secondary* (⌃⌘ on macOS).
const PRIMARY_SECONDARY: Modifiers = Modifiers {
    secondary: true,
    ..PRIMARY
};

/// The shortcut of a character key.
fn ch(modifiers: Modifiers, character: char) -> Result<Shortcut, ShortcutError> {
    Shortcut::new(modifiers, ShortcutKey::Character(character))
}

/// The shortcut of a function key, without modifiers.
fn function(number: u8) -> Result<Shortcut, ShortcutError> {
    Shortcut::new(NONE, ShortcutKey::Named(NamedKey::Function(number)))
}

/// An action of the global context.
const fn global(
    identifier: &'static str,
    name: &'static str,
    defaults: Defaults,
    edit: Option<EditRole>,
) -> ActionSpec {
    ActionSpec {
        identifier,
        name,
        context: Context::GLOBAL,
        defaults,
        repeatable: false,
        edit,
    }
}

/// The fifteen actions, in the order of [`ActionIds::all`], with the defaults of AM6: the File
/// group, the Edit group, then View and Help (three tables, so each stays readable).
///
/// # Errors
/// [`ShortcutError`] when a default shortcut is malformed; the tests prevent it.
fn specs() -> Result<[ActionSpec; 15], ShortcutError> {
    let [new, open, save, save_as, quit] = file_specs()?;
    let [undo, redo, cut, copy, paste, select_all, preferences] = edit_specs()?;
    let [full_screen, help, about] = view_and_help_specs()?;
    Ok([
        new,
        open,
        save,
        save_as,
        quit,
        undo,
        redo,
        cut,
        copy,
        paste,
        select_all,
        preferences,
        full_screen,
        help,
        about,
    ])
}

/// The same shortcut on the three platforms.
fn everywhere(shortcut: Shortcut) -> Defaults {
    Defaults::everywhere(&[shortcut])
}

/// New, Open, Save, Save as, Quit.
fn file_specs() -> Result<[ActionSpec; 5], ShortcutError> {
    Ok([
        global(
            "file.new",
            "menu-file-new",
            everywhere(ch(PRIMARY, 'n')?),
            None,
        ),
        global(
            "file.open",
            "menu-file-open",
            everywhere(ch(PRIMARY, 'o')?),
            None,
        ),
        global(
            "file.save",
            "menu-file-save",
            everywhere(ch(PRIMARY, 's')?),
            None,
        ),
        global(
            "file.save-as",
            "menu-file-save-as",
            everywhere(ch(PRIMARY_SHIFT, 's')?),
            None,
        ),
        // On Windows a window is closed with Alt+F4, which `winit` reports as a close request.
        global(
            "file.quit",
            "menu-file-quit",
            Defaults {
                linux: vec![ch(PRIMARY, 'q')?],
                windows: Vec::new(),
                macos: vec![ch(PRIMARY, 'q')?],
            },
            None,
        ),
    ])
}

/// Undo, Redo, Cut, Copy, Paste, Select all, Preferences.
fn edit_specs() -> Result<[ActionSpec; 7], ShortcutError> {
    Ok([
        global(
            "edit.undo",
            "menu-edit-undo",
            everywhere(ch(PRIMARY, 'z')?),
            Some(EditRole::Undo),
        ),
        global(
            "edit.redo",
            "menu-edit-redo",
            Defaults {
                linux: vec![ch(PRIMARY_SHIFT, 'z')?],
                windows: vec![ch(PRIMARY, 'y')?, ch(PRIMARY_SHIFT, 'z')?],
                macos: vec![ch(PRIMARY_SHIFT, 'z')?],
            },
            Some(EditRole::Redo),
        ),
        global(
            "edit.cut",
            "menu-edit-cut",
            everywhere(ch(PRIMARY, 'x')?),
            Some(EditRole::Cut),
        ),
        global(
            "edit.copy",
            "menu-edit-copy",
            everywhere(ch(PRIMARY, 'c')?),
            Some(EditRole::Copy),
        ),
        global(
            "edit.paste",
            "menu-edit-paste",
            everywhere(ch(PRIMARY, 'v')?),
            Some(EditRole::Paste),
        ),
        global(
            "edit.select-all",
            "menu-edit-select-all",
            everywhere(ch(PRIMARY, 'a')?),
            Some(EditRole::SelectAll),
        ),
        global(
            "edit.preferences",
            "menu-edit-preferences",
            everywhere(ch(PRIMARY, ',')?),
            None,
        ),
    ])
}

/// Full screen, Help, About.
fn view_and_help_specs() -> Result<[ActionSpec; 3], ShortcutError> {
    Ok([
        global(
            "view.full-screen",
            "menu-view-full-screen",
            Defaults {
                linux: vec![function(11)?],
                windows: vec![function(11)?],
                macos: vec![ch(PRIMARY_SECONDARY, 'f')?],
            },
            None,
        ),
        // ⌘? is Shift+⌘/ on the US layout; the shortcut is the character (precision A).
        global(
            "help.contents",
            "menu-help-contents",
            Defaults {
                linux: vec![function(1)?],
                windows: vec![function(1)?],
                macos: vec![ch(PRIMARY, '?')?],
            },
            None,
        ),
        // No shortcut anywhere: no platform gives one to About (ADR-0050 DG10).
        global(
            "help.about",
            "menu-help-about",
            Defaults::everywhere(&[]),
            None,
        ),
    ])
}

/// The program's registry for `platform`, with the initial states of choice E (see the module
/// documentation), and the actions' indices.
///
/// # Errors
/// [`ActionsError`] when the tables above are wrong; the tests prevent it.
pub fn registry(platform: Platform) -> Result<(Registry, ActionIds), ActionsError> {
    let mut registry = Registry::new(platform);
    // Registered in the order of `specs`, which is the order of the fields below.
    let [
        new,
        open,
        save,
        save_as,
        quit,
        undo,
        redo,
        cut,
        copy,
        paste,
        select_all,
        preferences,
        full_screen,
        help,
        about,
    ] = specs()?.map(|spec| registry.register(spec));
    let ids = ActionIds {
        new: new?,
        open: open?,
        save: save?,
        save_as: save_as?,
        quit: quit?,
        undo: undo?,
        redo: redo?,
        cut: cut?,
        copy: copy?,
        paste: paste?,
        select_all: select_all?,
        preferences: preferences?,
        full_screen: full_screen?,
        help: help?,
        about: about?,
    };
    // What does not exist yet is disabled (choice E). The editing actions are enabled by the
    // focused field itself when it has the focus (AM13); outside it there is nothing to edit.
    for id in ids.all() {
        if id != ids.quit && id != ids.full_screen && id != ids.about && id != ids.preferences {
            registry.set_state(id, ActionState::DISABLED)?;
        }
    }
    Ok((registry, ids))
}

/// The texts of the actions in one language (F2.11d2a): the actions' names by their Fluent key,
/// and the keys' names for the shortcuts.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ActionTexts {
    /// Each action's Fluent key ([`ActionSpec::name`]), its text and its mnemonic.
    names: Vec<(&'static str, String, Option<char>)>,
    /// The keys' names.
    keys: KeyNames,
    /// The menu bar's titles ([`TITLES`]): their names and mnemonics (F2.11d2b).
    titles: Vec<(String, Option<char>)>,
}

impl ActionTexts {
    /// Hands the texts to `registry` ([`Registry::set_texts`], [`Registry::set_mnemonics`]):
    /// every action's name, its shortcut written in the registry's platform's style, and its
    /// menu item's mnemonic.
    pub fn apply(&self, registry: &mut Registry) {
        registry.set_texts(self.keys.clone(), |key| {
            self.names
                .iter()
                .find(|(name, _, _)| *name == key)
                .map_or_else(String::new, |(_, text, _)| text.clone())
        });
        registry.set_mnemonics(|key| {
            self.names
                .iter()
                .find(|(name, _, _)| *name == key)
                .and_then(|(_, _, mnemonic)| *mnemonic)
        });
    }

    /// The keys' names, for writing modifiers as the platform does (the Preferences' Keyboard
    /// section, F2.12a4b1).
    #[must_use]
    pub const fn keys(&self) -> &KeyNames {
        &self.keys
    }

    /// The name and the mnemonic of the bar's title number `title` ([`TITLES`]).
    #[must_use]
    pub fn title(&self, title: usize) -> (&str, Option<char>) {
        self.titles
            .get(title)
            .map_or(("", None), |(name, mnemonic)| (name.as_str(), *mnemonic))
    }
}

/// The Fluent keys of the menu bar's titles, in order: File, Edit, View, Help (F2.11d2b).
pub const TITLES: [&str; 4] = ["menu-file", "menu-edit", "menu-view", "menu-help"];

/// The mnemonic of the message `key`: the single character of its `.mnemonic` attribute
/// (F2.11d2b, ADR-0048 AM10), or `None` when the attribute is empty or longer than one character
/// (the catalogues' test prevents both).
///
/// # Errors
/// The localizer's, if the message or its attribute is missing (the catalogues' parity tests and
/// the mnemonics' test prevent it).
fn mnemonic_of(localizer: &Localizer, key: &str) -> Result<Option<char>, I18nError> {
    let text = localizer.format_attribute(key, "mnemonic", None)?;
    let mut characters = text.chars();
    Ok(match (characters.next(), characters.next()) {
        (Some(letter), None) => Some(letter),
        _ => None,
    })
}

/// The menu bar of Linux and Windows (F2.11d2b, ADR-0048 AM9; see the module documentation),
/// with the titles' names and mnemonics of `texts`. The actions' own names and letters come from
/// the registry.
#[must_use]
pub fn menu_bar(ids: &ActionIds, texts: &ActionTexts) -> MenuModel {
    let file = MenuModel::new()
        .action(ids.new)
        .action(ids.open)
        .action(ids.save)
        .action(ids.save_as)
        .separator()
        .action(ids.quit)
        .role(MenuRole::Quit);
    let edit = MenuModel::new()
        .action(ids.undo)
        .action(ids.redo)
        .separator()
        .action(ids.cut)
        .action(ids.copy)
        .action(ids.paste)
        .separator()
        .action(ids.select_all)
        .separator()
        .action(ids.preferences)
        .role(MenuRole::Preferences);
    let view = MenuModel::new().action(ids.full_screen);
    let help = MenuModel::new()
        .action(ids.help)
        .separator()
        .action(ids.about)
        .role(MenuRole::About);
    let title = |index: usize| {
        let (name, mnemonic) = texts.title(index);
        (name.to_owned(), mnemonic)
    };
    let mut bar = MenuModel::new();
    for (index, items) in [file, edit, view, help].into_iter().enumerate() {
        let (name, mnemonic) = title(index);
        bar = bar.submenu(name, mnemonic, items);
    }
    // macOS gives the Help menu its search field (AM9, F2.11e1).
    bar.role(MenuRole::Help)
}

/// The Fluent message of a key's name (choice S1 of F2.11d2a); the function keys and the keypad
/// take their number as an argument.
const fn key_message(key: KeyName) -> &'static str {
    match key {
        KeyName::Ctrl => "key-ctrl",
        KeyName::Shift => "key-shift",
        KeyName::Alt => "key-alt",
        KeyName::Super => "key-super",
        KeyName::Windows => "key-windows",
        KeyName::Enter => "key-enter",
        KeyName::Escape => "key-escape",
        KeyName::Tab => "key-tab",
        KeyName::Space => "key-space",
        KeyName::Backspace => "key-backspace",
        KeyName::Delete => "key-delete",
        KeyName::Insert => "key-insert",
        KeyName::Home => "key-home",
        KeyName::End => "key-end",
        KeyName::PageUp => "key-page-up",
        KeyName::PageDown => "key-page-down",
        KeyName::Left => "key-left",
        KeyName::Right => "key-right",
        KeyName::Up => "key-up",
        KeyName::Down => "key-down",
        KeyName::Menu => "key-menu",
        KeyName::Function(_) => "key-function",
        KeyName::Numpad(_) => "key-numpad",
    }
}

/// The name of `key` from `localizer`.
///
/// # Why the isolation marks are removed
/// Fluent surrounds an argument with the invisible marks U+2068 and U+2069 (our `Localizer` keeps
/// them on purpose, for right-to-left text mixed into a message). A key's name holds only a
/// number from us, never the user's text, and it ends up in the accessibility tree's
/// `keyboard_shortcut`, where a screen reader would meet the marks: they are dropped here.
///
/// # Errors
/// The localizer's, if the message is missing (the catalogues' parity tests prevent it).
fn key_name(localizer: &Localizer, key: KeyName) -> Result<String, I18nError> {
    let number = match key {
        KeyName::Function(number) => Some(("number", number)),
        KeyName::Numpad(digit) => Some(("digit", digit)),
        _ => None,
    };
    let text = match number {
        Some((name, value)) => {
            let mut args = FluentArgs::new();
            args.set(name, u32::from(value));
            localizer.format(key_message(key), Some(&args))?
        }
        None => localizer.format(key_message(key), None)?,
    };
    Ok(text.replace(['\u{2068}', '\u{2069}'], ""))
}

/// The actions' and the keys' names in `localizer`'s language (F2.11d2a).
///
/// # Errors
/// The localizer's, if a message is missing (the catalogues' parity tests prevent it).
pub fn action_texts(localizer: &Localizer) -> Result<ActionTexts, I18nError> {
    // The names come from the same table the registry is built from; if that table were wrong,
    // there would be no registry to name either (`registry` fails the same way).
    let keys = specs()
        .map(|specs| specs.map(|spec| spec.name).to_vec())
        .unwrap_or_default();
    let mut names = Vec::with_capacity(keys.len());
    for key in keys {
        // A name may hold the program's name (`About { app-name }`, F2.12a3b): Fluent's isolation
        // marks around it are dropped, as for the system's names below (the program's name is
        // never right-to-left, and the name reaches menus and the accessibility tree).
        let name = localizer
            .format(key, None)?
            .replace(['\u{2068}', '\u{2069}'], "");
        names.push((key, name, mnemonic_of(localizer, key)?));
    }
    let mut titles = Vec::with_capacity(TITLES.len());
    for key in TITLES {
        titles.push((localizer.format(key, None)?, mnemonic_of(localizer, key)?));
    }
    // Every key's name, asked once; the first missing message is reported.
    let mut error = None;
    let keys = KeyNames::new(|key| {
        key_name(localizer, key).unwrap_or_else(|failure| {
            error.get_or_insert(failure);
            String::new()
        })
    });
    match error {
        Some(error) => Err(error),
        None => Ok(ActionTexts {
            names,
            keys,
            titles,
        }),
    }
}

/// The names macOS's own menus need (F2.11e2, ADR-0048 AM9), in `localizer`'s language: the
/// application menu's Settings, Services, Hide, Hide Others, Show All and Quit, and the *Window*
/// menu with Minimize, Zoom and Bring All to Front. The messages name the program through
/// `{ app-name }` ("Quit Orchestra", "Esci da Orchestra", "Orchestra beenden"), so the word order
/// is each language's. Fluent's isolation marks around that reference are dropped, as for the
/// keys' names: the program's name is never right-to-left, and the system's menu shows the text
/// as given.
///
/// # Errors
/// The localizer's, if a message is missing (the catalogues' parity tests prevent it).
pub fn system_texts(localizer: &Localizer) -> Result<SystemTexts, I18nError> {
    let text = |key: &str| -> Result<String, I18nError> {
        Ok(localizer
            .format(key, None)?
            .replace(['\u{2068}', '\u{2069}'], ""))
    };
    Ok(SystemTexts {
        settings: text("menu-mac-settings")?,
        services: text("menu-mac-services")?,
        hide: text("menu-mac-hide")?,
        hide_others: text("menu-mac-hide-others")?,
        show_all: text("menu-mac-show-all")?,
        quit: text("menu-mac-quit")?,
        window: text("menu-mac-window")?,
        minimize: text("menu-mac-minimize")?,
        zoom: text("menu-mac-zoom")?,
        bring_all_to_front: text("menu-mac-bring-all-to-front")?,
    })
}

/// The context menu of a text field (F2.11d2a, ADR-0048 AM13): Undo and Redo, then Cut, Copy and
/// Paste, then Select all, the order and grouping of GTK's, Qt's and Windows' own field menus.
#[must_use]
pub fn field_menu(ids: &ActionIds) -> MenuModel {
    MenuModel::new()
        .action(ids.undo)
        .action(ids.redo)
        .separator()
        .action(ids.cut)
        .action(ids.copy)
        .action(ids.paste)
        .separator()
        .action(ids.select_all)
}
