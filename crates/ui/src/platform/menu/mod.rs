// SPDX-License-Identifier: GPL-3.0-or-later

//! The system's menu bar (F2.11e1, ADR-0048 AM9, ADR-0036 B5): on macOS the menus live in the bar
//! at the top of the screen, drawn by the system, and `muda` builds them; on Linux and Windows we
//! draw the bar ourselves (`widget::menu`, F2.11d2b) and nothing here runs.
//!
//! # Two layers
//! 1. **A description**, [`SystemMenu`]: the program's menus as the system will show them, built
//!    by the interface ([`Ui::system_menu`](crate::Ui::system_menu)) from the one menu model and
//!    the registry, with every name, shortcut and state already resolved and the items with a
//!    role moved where macOS wants them ([`SystemMenu::for_macos`]). Plain data, built and tested
//!    on every platform, so Linux CI checks what the Mac will show.
//! 2. **The system's menu**, behind the trait [`SystemMenuHost`]: `muda` on macOS (the file `muda`,
//!    compiled there only), nothing elsewhere ([`install`] answers `None`). The window installs it
//!    once, tells it every change of state, and turns what the user chooses in it into the same
//!    "run this action" the drawn menus and the shortcuts produce.
//!
//! # Where the items with a role go (AM9)
//! macOS puts *About*, *Settings* and *Quit* in the **application menu**, the first one, named
//! after the program; the *Window* and *Help* submenus are given to the system, which adds its own
//! items to them (the window list, the help search). [`SystemMenu::for_macos`] takes the action
//! items with those roles out of the menus they are in, puts them into the application menu (in
//! the order About, Settings, Quit, with separators between the groups), and tidies the menus they
//! left: no separator at the start or end of a menu, never two in a row, no empty submenu.
//!
//! # The system's own items (F2.11e2)
//! Every Mac program's application menu has the same items around ours, in Apple's order:
//!
//! ```text
//! About Orchestra            (none yet: it comes with its window, F2.12)
//! ───────────────
//! Settings…            ⌘,    (our Preferences action, named the macOS way)
//! ───────────────
//! Services             ▸
//! ───────────────
//! Hide Orchestra       ⌘H
//! Hide Others         ⌥⌘H
//! Show All
//! ───────────────
//! Quit Orchestra       ⌘Q    (our Quit action, with the program's name)
//! ```
//!
//! and a *Window* menu before *Help*: Minimize (⌘M), Zoom, then Bring All to Front; macOS appends
//! the list of the program's windows to it. Those items are [`SystemItem::System`]: the system
//! does what they do (no action of ours runs), `muda` makes them with its predefined items, and
//! their shortcuts are macOS's own, which the registry refuses to give to actions there (AM8,
//! [`Shortcut::is_fixed`]). Their names are ours, from the catalogues ([`SystemTexts`]): macOS
//! does not translate items a program builds itself, and `muda`'s defaults are English.
//!
//! On Linux and Windows none of this exists: the drawn bar has no application or Window menu.
//!
//! # Shortcuts (AM9)
//! A shortcut is given to the system as its **logical** key, the character it types, exactly as
//! the registry stores it (AM5): ⌘? is the character "?", ⌘, the character ",". `muda`'s
//! `KeyAccelerator` takes a character, and macOS matches a menu's key equivalent by the character
//! the layout produces, so a shortcut works on every layout as it does through the window. The
//! modifiers become macOS's: *Primary* is ⌘, *Secondary* is ⌃, Alt is ⌥ (ADR-0040 W5).
//!
//! # States (AM2)
//! The window asks the application for the actions whose state changed after every batch of
//! events ([`WindowApp::take_menu_updates`](crate::WindowApp::take_menu_updates)) and tells the
//! system's menu ([`SystemMenuHost::update`]); a disabled item cannot be chosen, a toggle shows its
//! check.

#[cfg(target_os = "macos")]
mod muda;

use crate::action::registry::{ActionId, ActionState};
use crate::action::shortcut::{Shortcut, ShortcutKey};
use crate::widget::menu::MenuRole;
use crate::window::input::NamedKey;

/// The program's menus as the system's menu bar shows them (see the module documentation).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SystemMenu {
    /// The menus of the bar, in order: on macOS the application menu first.
    pub items: Vec<SystemItem>,
}

/// One item of a [`SystemMenu`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SystemItem {
    /// An action of the registry.
    Action {
        /// The action, which the item runs.
        action: ActionId,
        /// Its name, as people read it.
        name: String,
        /// Its first shortcut, if any, shown by the system next to the name.
        shortcut: Option<Shortcut>,
        /// Its state as the user sees it now (AM2, AM13).
        state: ActionState,
        /// Its role, if any (AM9).
        role: Option<MenuRole>,
    },
    /// A submenu.
    Submenu {
        /// Its name.
        name: String,
        /// Its role, if any: *Window* and *Help* are given to the system.
        role: Option<MenuRole>,
        /// Its items.
        items: Vec<SystemItem>,
    },
    /// An item the system itself carries out (see the module documentation): no action of ours
    /// runs when it is chosen.
    System {
        /// What it does.
        command: SystemCommand,
        /// Its name, as people read it.
        name: String,
    },
    /// A line between groups of items.
    Separator,
}

/// What a [`SystemItem::System`] does, on macOS (each is one of `muda`'s predefined items).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemCommand {
    /// The *Services* submenu, filled by the system.
    Services,
    /// Hides the program (⌘H).
    Hide,
    /// Hides every other program (⌥⌘H).
    HideOthers,
    /// Shows every program again.
    ShowAll,
    /// Minimizes the front window into the Dock (⌘M).
    Minimize,
    /// Zooms the front window (the green button's resize, not full screen).
    Zoom,
    /// Brings every window of the program to the front.
    BringAllToFront,
}

/// The names macOS's own menus need, already in the user's language (see the module
/// documentation): resolved by the application from its catalogues, so this crate holds no text.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SystemTexts {
    /// "Settings…": the name of the *Preferences* action on macOS (macOS 13 and later).
    pub settings: String,
    /// "Services".
    pub services: String,
    /// "Hide Orchestra", with the program's name.
    pub hide: String,
    /// "Hide Others".
    pub hide_others: String,
    /// "Show All".
    pub show_all: String,
    /// "Quit Orchestra", with the program's name: the name of the *Quit* action on macOS.
    pub quit: String,
    /// "Window": the *Window* menu's title.
    pub window: String,
    /// "Minimize".
    pub minimize: String,
    /// "Zoom".
    pub zoom: String,
    /// "Bring All to Front".
    pub bring_all_to_front: String,
}

impl SystemItem {
    /// `true` for an action item with one of `roles`.
    fn has_action_role(&self, roles: &[MenuRole]) -> bool {
        matches!(self, Self::Action { role: Some(role), .. } if roles.contains(role))
    }
}

/// The roles that go into macOS's application menu, in its order (AM9).
const APP_MENU_ROLES: [MenuRole; 3] = [MenuRole::About, MenuRole::Preferences, MenuRole::Quit];

impl SystemMenu {
    /// The menus as macOS shows them (see the module documentation):
    /// - an application menu named `app_name` first: the action items whose role is *About*,
    ///   then *Preferences* (renamed `texts.settings`), then Services, then Hide, Hide Others and
    ///   Show All, then *Quit* (renamed `texts.quit`), each group apart. The role items are taken
    ///   out of the menus they were in, which are then tidied. The system's items are always
    ///   there, so the menu is never empty, even without role items;
    /// - the other menus, in their order, with a *Window* menu (`texts.window`) inserted before
    ///   the menu whose role is *Help*, or at the end without one; a submenu of the model that
    ///   already has the role *Window* is kept instead, with the system's window items added at
    ///   its start.
    ///
    /// # Complexity
    /// O(items) for the walk, plus a few moves.
    #[must_use]
    pub fn for_macos(menus: Vec<SystemItem>, app_name: &str, texts: &SystemTexts) -> Self {
        let mut taken = Vec::new();
        let mut menus = menus;
        take_roles(&mut menus, &mut taken);
        tidy(&mut menus);
        // The role items with macOS's names (the registry's are Linux's and Windows').
        for item in &mut taken {
            if let SystemItem::Action {
                name,
                role: Some(role),
                ..
            } = item
            {
                match role {
                    MenuRole::Preferences => texts.settings.clone_into(name),
                    MenuRole::Quit => texts.quit.clone_into(name),
                    MenuRole::About | MenuRole::Window | MenuRole::Help => {}
                }
            }
        }
        let group = |role: MenuRole| -> Vec<SystemItem> {
            taken
                .iter()
                .filter(|item| item.has_action_role(&[role]))
                .cloned()
                .collect()
        };
        let system = |command: SystemCommand, name: &str| SystemItem::System {
            command,
            name: name.to_owned(),
        };
        // The application menu, group by group in Apple's order; empty groups leave no line.
        let groups = [
            group(MenuRole::About),
            group(MenuRole::Preferences),
            vec![system(SystemCommand::Services, &texts.services)],
            vec![
                system(SystemCommand::Hide, &texts.hide),
                system(SystemCommand::HideOthers, &texts.hide_others),
                system(SystemCommand::ShowAll, &texts.show_all),
            ],
            group(MenuRole::Quit),
        ];
        let mut app = Vec::new();
        for group in groups {
            if group.is_empty() {
                continue;
            }
            if !app.is_empty() {
                app.push(SystemItem::Separator);
            }
            app.extend(group);
        }
        let mut items = Vec::with_capacity(menus.len() + 2);
        items.push(SystemItem::Submenu {
            name: app_name.to_owned(),
            role: None,
            items: app,
        });
        items.extend(menus);
        add_window_menu(&mut items, texts);
        Self { items }
    }

    /// Every action item, with its place, depth first (for the host's table and for tests).
    pub fn actions(&self) -> impl Iterator<Item = &SystemItem> {
        let mut stack: Vec<&SystemItem> = self.items.iter().rev().collect();
        std::iter::from_fn(move || {
            while let Some(item) = stack.pop() {
                match item {
                    SystemItem::Submenu { items, .. } => stack.extend(items.iter().rev()),
                    SystemItem::Action { .. } => return Some(item),
                    SystemItem::System { .. } | SystemItem::Separator => {}
                }
            }
            None
        })
    }
}

/// Gives the bar `items` its *Window* menu (see [`SystemMenu::for_macos`]): the system's window
/// items, a line, then whatever the model's own *Window* menu holds, if it has one.
fn add_window_menu(items: &mut Vec<SystemItem>, texts: &SystemTexts) {
    let mut window = vec![
        SystemItem::System {
            command: SystemCommand::Minimize,
            name: texts.minimize.clone(),
        },
        SystemItem::System {
            command: SystemCommand::Zoom,
            name: texts.zoom.clone(),
        },
        SystemItem::Separator,
        SystemItem::System {
            command: SystemCommand::BringAllToFront,
            name: texts.bring_all_to_front.clone(),
        },
    ];
    let has_role = |item: &SystemItem, wanted: MenuRole| matches!(item, SystemItem::Submenu { role: Some(role), .. } if *role == wanted);
    // The model's own Window menu, if any: the system's items go first, its own after a line.
    if let Some(SystemItem::Submenu { items: own, .. }) = items
        .iter_mut()
        .find(|item| has_role(item, MenuRole::Window))
    {
        window.push(SystemItem::Separator);
        window.append(own);
        tidy(&mut window);
        *own = window;
        return;
    }
    // Otherwise a new one, before Help (where every Mac program has it) or last.
    let at = items
        .iter()
        .position(|item| has_role(item, MenuRole::Help))
        .unwrap_or(items.len());
    items.insert(
        at,
        SystemItem::Submenu {
            name: texts.window.clone(),
            role: Some(MenuRole::Window),
            items: window,
        },
    );
}

/// Moves out of `items` (and of their submenus, at any depth) the action items whose role goes
/// into the application menu, appending them to `taken` in the order met.
fn take_roles(items: &mut Vec<SystemItem>, taken: &mut Vec<SystemItem>) {
    let mut kept = Vec::with_capacity(items.len());
    for mut item in items.drain(..) {
        if item.has_action_role(&APP_MENU_ROLES) {
            taken.push(item);
            continue;
        }
        if let SystemItem::Submenu { items: inner, .. } = &mut item {
            take_roles(inner, taken);
        }
        kept.push(item);
    }
    *items = kept;
}

/// Tidies a list of items after some were taken out: no separator at its start or end, never two
/// in a row, no empty submenu; the same inside every submenu.
fn tidy(items: &mut Vec<SystemItem>) {
    for item in items.iter_mut() {
        if let SystemItem::Submenu { items: inner, .. } = item {
            tidy(inner);
        }
    }
    items.retain(|item| !matches!(item, SystemItem::Submenu { items, .. } if items.is_empty()));
    let mut tidied: Vec<SystemItem> = Vec::with_capacity(items.len());
    for item in items.drain(..) {
        let separator = matches!(item, SystemItem::Separator);
        let after_separator = matches!(tidied.last(), None | Some(SystemItem::Separator));
        if separator && after_separator {
            continue;
        }
        tidied.push(item);
    }
    if matches!(tidied.last(), Some(SystemItem::Separator)) {
        tidied.pop();
    }
    *items = tidied;
}

/// A shortcut as the system's menu takes it (see the module documentation): macOS's modifiers and
/// the logical key.
#[cfg_attr(
    not(target_os = "macos"),
    allow(
        dead_code,
        reason = "read by the macOS host (`muda`) and by the tests; Linux and Windows draw their bar"
    )
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "the four modifier keys of macOS, each held or not; a bit set would only hide them"
)]
pub(crate) struct SystemShortcut {
    /// ⌘, the *Primary* modifier.
    pub(crate) command: bool,
    /// ⌃, the *Secondary* modifier.
    pub(crate) control: bool,
    /// ⌥, Alt.
    pub(crate) option: bool,
    /// ⇧.
    pub(crate) shift: bool,
    /// The key.
    pub(crate) key: SystemKey,
}

/// The key of a [`SystemShortcut`]: a character, or a named key.
#[cfg_attr(
    not(target_os = "macos"),
    allow(
        dead_code,
        reason = "read by the macOS host (`muda`) and by the tests; Linux and Windows draw their bar"
    )
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SystemKey {
    /// The character the key types (a letter in lower case, as the registry keeps it; macOS adds
    /// ⇧ itself from the modifiers).
    Character(char),
    /// A named key.
    Named(NamedKey),
}

#[cfg_attr(
    not(target_os = "macos"),
    allow(
        dead_code,
        reason = "read by the macOS host (`muda`) and by the tests; Linux and Windows draw their bar"
    )
)]
impl SystemShortcut {
    /// `shortcut` for the system's menu. A digit of the keypad becomes the digit (macOS's menus
    /// cannot tell the keypad apart); the Menu key and Alt alone, which are never an action's
    /// shortcut (AM8), give `None`.
    pub(crate) fn from_shortcut(shortcut: Shortcut) -> Option<Self> {
        let modifiers = shortcut.modifiers();
        let key = match shortcut.key() {
            ShortcutKey::Character(character) => SystemKey::Character(character),
            ShortcutKey::Digit(digit) | ShortcutKey::Numpad(digit) => {
                SystemKey::Character(char::from(b'0'.saturating_add(digit)))
            }
            ShortcutKey::Named(NamedKey::ContextMenu | NamedKey::Alt) => return None,
            ShortcutKey::Named(NamedKey::Space) => SystemKey::Character(' '),
            ShortcutKey::Named(named) => SystemKey::Named(named),
        };
        Some(Self {
            command: modifiers.primary,
            control: modifiers.secondary,
            option: modifiers.alt,
            shift: modifiers.shift,
            key,
        })
    }
}

/// The system's menu once installed (see the module documentation): `muda`'s on macOS.
pub(crate) trait SystemMenuHost {
    /// Shows `action`'s new state: enabled or not, checked or not.
    fn update(&mut self, action: ActionId, state: ActionState);
}

/// Installs `menu` as the system's menu bar and answers its host, on macOS; `None` elsewhere
/// (the drawn bar is used there) and when `muda` refuses the menu. `chosen` is called, on the main
/// thread, with the action of every item the user chooses.
///
/// Must be called on the main thread once the event loop runs (macOS's application object
/// exists then).
#[cfg(target_os = "macos")]
pub(crate) fn install(
    menu: &SystemMenu,
    chosen: impl Fn(ActionId) + Send + Sync + 'static,
) -> Option<Box<dyn SystemMenuHost>> {
    muda::install(menu, chosen)
        .ok()
        .map(|host| Box::new(host) as Box<dyn SystemMenuHost>)
}

/// See the macOS version: nothing to install on Linux and Windows.
#[cfg(not(target_os = "macos"))]
pub(crate) fn install(
    menu: &SystemMenu,
    chosen: impl Fn(ActionId) + Send + Sync + 'static,
) -> Option<Box<dyn SystemMenuHost>> {
    let _ = (menu, chosen);
    None
}

/// Builds `menu` with `muda` without installing it, and answers how many action items it made:
/// for the CI test on macOS that checks the menu is accepted by the system's own classes
/// (ADR-0048 AM15). Must run on the main thread.
///
/// # Errors
/// `muda`'s message when it refuses an item.
#[cfg(target_os = "macos")]
pub fn build_system_menu(menu: &SystemMenu) -> Result<usize, String> {
    muda::build(menu).map(|built| built.count())
}

#[cfg(test)]
mod tests;
