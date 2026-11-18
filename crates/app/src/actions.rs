// SPDX-License-Identifier: GPL-3.0-or-later

//! The program's actions (ADR-0048 AM1, AM6; F2.11d1): the concrete list, with the default
//! shortcuts of each platform and the Fluent keys of their names. The generic machinery (the
//! registry, the lookup, the contexts) is `orchestra-ui`'s; this module only fills it.
//!
//! # Which actions, and their state today (choice E of F2.11d1)
//! The fourteen of ADR-0048 AM6's table, so that the menus of F2.11d2 have real items:
//! - **working:** Quit and Full screen (they ask the window, [`orchestra_ui::WindowRequest`]),
//!   and the six editing actions on a focused text field (AM13: the interface sends them to the
//!   field itself);
//! - **registered but disabled** until what they do exists: New, Open, Save, Save as (the project
//!   file in the interface), Preferences (F2.12), Help (the guides), and the editing actions
//!   outside a field (there is no document view yet to undo in or select from).
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
//!
//! They are written with the abstract modifiers (*Primary* is Ctrl or ⌘, *Secondary* is the
//! Windows/Super key or ⌃), so one table serves the three platforms, chosen by value: the tests
//! on Linux build the registries of all three.

use orchestra_ui::{
    ActionId, ActionSpec, ActionState, Context, Defaults, EditRole, Modifiers, NamedKey, Platform,
    Registry, RegistryError, Shortcut, ShortcutError, ShortcutKey,
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
}

impl ActionIds {
    /// Every action, in registration order.
    #[must_use]
    pub const fn all(&self) -> [ActionId; 14] {
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

/// The fourteen actions, in the order of [`ActionIds::all`], with the defaults of AM6: the File
/// group, the Edit group, then View and Help (three tables, so each stays readable).
///
/// # Errors
/// [`ShortcutError`] when a default shortcut is malformed; the tests prevent it.
fn specs() -> Result<[ActionSpec; 14], ShortcutError> {
    let [new, open, save, save_as, quit] = file_specs()?;
    let [undo, redo, cut, copy, paste, select_all, preferences] = edit_specs()?;
    let [full_screen, help] = view_and_help_specs()?;
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

/// Full screen, Help.
fn view_and_help_specs() -> Result<[ActionSpec; 2], ShortcutError> {
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
    };
    // What does not exist yet is disabled (choice E). The editing actions are enabled by the
    // focused field itself when it has the focus (AM13); outside it there is nothing to edit.
    for id in ids.all() {
        if id != ids.quit && id != ids.full_screen {
            registry.set_state(id, ActionState::DISABLED)?;
        }
    }
    Ok((registry, ids))
}
