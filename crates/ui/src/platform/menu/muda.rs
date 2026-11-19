// SPDX-License-Identifier: GPL-3.0-or-later

//! The system's menu bar through `muda`, macOS only (F2.11e1, ADR-0036 B5, ADR-0048 AM9). The
//! only file that knows `muda`, as `window` is the only module that knows `winit`.
//!
//! # From the description to `muda`'s items
//! Every submenu of the [`SystemMenu`] becomes a `Submenu` of the bar; an action item a
//! `MenuItem`, or a `CheckMenuItem` when its action is a toggle, with the action's state and its
//! shortcut as a `KeyAccelerator` (the logical key, see the parent module); a separator `muda`'s
//! predefined separator. Each action item's ID is `orchestra-action-N`, N the action's index, and
//! a table from those IDs to the actions goes with the handler of `muda`'s events, so a choice
//! comes back as an [`ActionId`].
//!
//! # Names and `&`
//! `muda` reads an `&` in an item's text as a mnemonic marker (for Windows and GTK). macOS has no
//! mnemonics (AM10), but a name with an `&` in it ("Rock & Roll") would lose it, so every `&` is
//! written twice, which `muda` shows as one.
//!
//! # Threads
//! `muda`'s items are not `Send`: they are built, kept and updated on the main thread (the event
//! loop's). Its event handler is called there too, by the system's menu.

use std::collections::HashMap;

use muda::accelerator::{Key, KeyAccelerator, Modifiers, NamedKey as MudaKey};
use muda::{CheckMenuItem, Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem, Submenu};

use super::{SystemItem, SystemKey, SystemMenu, SystemMenuHost, SystemShortcut};
use crate::action::registry::{ActionId, ActionState};
use crate::widget::menu::MenuRole;
use crate::window::input::NamedKey;

/// One action item of `muda`'s menu, kept to change its state later.
enum MudaItem {
    /// An ordinary item.
    Plain(MenuItem),
    /// A toggle's item.
    Check(CheckMenuItem),
}

/// `muda`'s menu built from a [`SystemMenu`], not installed yet.
pub(super) struct Built {
    /// The menu bar.
    menu: Menu,
    /// The bar's submenus with a role the system takes (*Window*, *Help*), applied once the
    /// menu is installed (the system's application object must exist).
    roles: Vec<(Submenu, MenuRole)>,
    /// The action items, by action.
    items: Vec<(ActionId, MudaItem)>,
    /// From an item's ID to its action, for the events.
    ids: HashMap<String, ActionId>,
}

impl Built {
    /// How many action items were built.
    pub(super) fn count(&self) -> usize {
        self.items.len()
    }
}

/// The installed menu (see [`super::install`]).
pub(super) struct MudaHost {
    /// The menu bar; kept alive while the program runs (dropping it would empty the bar).
    _menu: Menu,
    /// The action items, by action.
    items: Vec<(ActionId, MudaItem)>,
}

impl SystemMenuHost for MudaHost {
    fn update(&mut self, action: ActionId, state: ActionState) {
        // A handful of items: a linear scan beats a map here, and allocates nothing.
        for (own, item) in &self.items {
            if *own != action {
                continue;
            }
            match item {
                MudaItem::Plain(item) => item.set_enabled(state.enabled),
                MudaItem::Check(item) => {
                    item.set_enabled(state.enabled);
                    item.set_checked(state.checked.unwrap_or(false));
                }
            }
        }
    }
}

/// The text `muda` shows as `name`: every `&` doubled (see the module documentation).
fn text(name: &str) -> String {
    name.replace('&', "&&")
}

/// `shortcut` as `muda`'s logical accelerator (see the parent module).
fn accelerator(shortcut: SystemShortcut) -> Option<KeyAccelerator> {
    let mut modifiers = Modifiers::empty();
    if shortcut.command {
        // `muda` maps META (and the deprecated SUPER) to ⌘.
        modifiers |= Modifiers::META;
    }
    if shortcut.control {
        modifiers |= Modifiers::CONTROL;
    }
    if shortcut.option {
        modifiers |= Modifiers::ALT;
    }
    if shortcut.shift {
        modifiers |= Modifiers::SHIFT;
    }
    let key = match shortcut.key {
        SystemKey::Character(character) => Key::Character(character.to_string()),
        SystemKey::Named(named) => Key::Named(named_key(named)?),
    };
    Some(KeyAccelerator::new(modifiers, key))
}

/// Our named key as `keyboard-types`', which `muda` uses.
fn named_key(key: NamedKey) -> Option<MudaKey> {
    Some(match key {
        NamedKey::Enter => MudaKey::Enter,
        NamedKey::Escape => MudaKey::Escape,
        NamedKey::Tab => MudaKey::Tab,
        NamedKey::Backspace => MudaKey::Backspace,
        NamedKey::Delete => MudaKey::Delete,
        NamedKey::Insert => MudaKey::Insert,
        NamedKey::ArrowUp => MudaKey::ArrowUp,
        NamedKey::ArrowDown => MudaKey::ArrowDown,
        NamedKey::ArrowLeft => MudaKey::ArrowLeft,
        NamedKey::ArrowRight => MudaKey::ArrowRight,
        NamedKey::Home => MudaKey::Home,
        NamedKey::End => MudaKey::End,
        NamedKey::PageUp => MudaKey::PageUp,
        NamedKey::PageDown => MudaKey::PageDown,
        NamedKey::Function(number) => function_key(number)?,
        // Never an action's shortcut (AM8); Space is a character for the system.
        NamedKey::ContextMenu | NamedKey::Alt | NamedKey::Space => return None,
    })
}

/// F1 to F24 as `keyboard-types`' keys.
const fn function_key(number: u8) -> Option<MudaKey> {
    Some(match number {
        1 => MudaKey::F1,
        2 => MudaKey::F2,
        3 => MudaKey::F3,
        4 => MudaKey::F4,
        5 => MudaKey::F5,
        6 => MudaKey::F6,
        7 => MudaKey::F7,
        8 => MudaKey::F8,
        9 => MudaKey::F9,
        10 => MudaKey::F10,
        11 => MudaKey::F11,
        12 => MudaKey::F12,
        13 => MudaKey::F13,
        14 => MudaKey::F14,
        15 => MudaKey::F15,
        16 => MudaKey::F16,
        17 => MudaKey::F17,
        18 => MudaKey::F18,
        19 => MudaKey::F19,
        20 => MudaKey::F20,
        21 => MudaKey::F21,
        22 => MudaKey::F22,
        23 => MudaKey::F23,
        24 => MudaKey::F24,
        _ => return None,
    })
}

/// Builds `muda`'s menu from `menu` (see the module documentation). Only the submenus of the bar
/// are built: macOS's bar holds nothing else.
///
/// # Errors
/// `muda`'s message when it refuses an item.
pub(super) fn build(menu: &SystemMenu) -> Result<Built, String> {
    let mut built = Built {
        menu: Menu::new(),
        roles: Vec::new(),
        items: Vec::new(),
        ids: HashMap::new(),
    };
    for item in &menu.items {
        let SystemItem::Submenu { name, role, items } = item else {
            continue;
        };
        let submenu = Submenu::new(text(name), true);
        fill(&submenu, items, &mut built)?;
        built
            .menu
            .append(&submenu)
            .map_err(|error| error.to_string())?;
        if let Some(role @ (MenuRole::Window | MenuRole::Help)) = role {
            built.roles.push((submenu, *role));
        }
    }
    Ok(built)
}

/// Appends `items` to `submenu`, recording the action items in `built`.
fn fill(submenu: &Submenu, items: &[SystemItem], built: &mut Built) -> Result<(), String> {
    for item in items {
        match item {
            SystemItem::Action {
                action,
                name,
                shortcut,
                state,
                ..
            } => {
                let id = format!("orchestra-action-{}", action.get());
                let accelerator = (*shortcut)
                    .and_then(SystemShortcut::from_shortcut)
                    .and_then(accelerator);
                let made = if let Some(checked) = state.checked {
                    let check = CheckMenuItem::with_id(
                        id.as_str(),
                        text(name),
                        state.enabled,
                        checked,
                        None,
                    );
                    check
                        .set_key_accelerator(accelerator)
                        .map_err(|error| error.to_string())?;
                    submenu.append(&check).map_err(|error| error.to_string())?;
                    MudaItem::Check(check)
                } else {
                    let plain = MenuItem::with_id(id.as_str(), text(name), state.enabled, None);
                    plain
                        .set_key_accelerator(accelerator)
                        .map_err(|error| error.to_string())?;
                    submenu.append(&plain).map_err(|error| error.to_string())?;
                    MudaItem::Plain(plain)
                };
                built.ids.insert(id, *action);
                built.items.push((*action, made));
            }
            SystemItem::Submenu { name, items, .. } => {
                let inner = Submenu::new(text(name), true);
                fill(&inner, items, built)?;
                submenu.append(&inner).map_err(|error| error.to_string())?;
            }
            SystemItem::Separator => {
                submenu
                    .append(&PredefinedMenuItem::separator())
                    .map_err(|error| error.to_string())?;
            }
        }
    }
    Ok(())
}

/// Builds `menu`, installs it as the application's menu bar, gives the system its *Window* and
/// *Help* menus, and routes `muda`'s events to `chosen` (see the parent module's [`install`]).
///
/// # Errors
/// `muda`'s message when it refuses an item.
///
/// [`install`]: super::install
pub(super) fn install(
    menu: &SystemMenu,
    chosen: impl Fn(ActionId) + Send + Sync + 'static,
) -> Result<MudaHost, String> {
    let built = build(menu)?;
    built.menu.init_for_nsapp();
    for (submenu, role) in &built.roles {
        match role {
            MenuRole::Window => submenu.set_as_windows_menu_for_nsapp(),
            MenuRole::Help => submenu.set_as_help_menu_for_nsapp(),
            MenuRole::About | MenuRole::Preferences | MenuRole::Quit => {}
        }
    }
    let ids = built.ids;
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        let id: &MenuId = event.id();
        if let Some(&action) = ids.get(id.as_ref()) {
            chosen(action);
        }
    }));
    Ok(MudaHost {
        _menu: built.menu,
        items: built.items,
    })
}
