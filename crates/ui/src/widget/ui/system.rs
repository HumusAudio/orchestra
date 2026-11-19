// SPDX-License-Identifier: GPL-3.0-or-later

//! The interface's side of the system's menu bar (F2.11e1, ADR-0048 AM2, AM9, AM13): describing
//! the menus for the system, telling the window which items changed, running what the user chose
//! there. A child module of the interface's, next to `menus` and `bar`.
//!
//! # Who reads the changed actions (choice 3a of F2.11e1)
//! The registry's marks of changed actions have **one** reader, the interface, which drains them
//! at every frame for its drawn menus (`menus::refresh_menus`). Once the application asked for a
//! system menu ([`Ui::system_menu`]), the interface also **keeps** the drained actions until the
//! window takes them ([`Ui::take_menu_updates`], after every batch of events), with their state as
//! the user sees it then. Two readers of the registry would each have to clear the marks only
//! when both had seen them; one reader handing them on keeps the registry simple.
//!
//! # The editing actions (AM13)
//! The state of Cut, Copy, Paste, Select all, Undo and Redo is the focused text field's while one
//! has the focus (Copy needs a selection), and that changes without the registry knowing. So when
//! the window takes the updates, the interface also compares those actions' states with the ones
//! the system's menu last got, and hands on the ones that differ: a handful of comparisons per
//! batch, no allocation.

use crate::action::registry::{ActionId, ActionState};
use crate::draw::list::DisplayList;
use crate::platform::menu::{SystemItem, SystemMenu};
use crate::widget::menu::{MenuItem, MenuModel};

use super::Ui;

/// What the interface keeps for the system's menu (see the module documentation).
#[derive(Debug, Default)]
pub(super) struct SystemTracking {
    /// The actions marked as changed since the window last took the updates, each once.
    pub(super) changed: Vec<ActionId>,
    /// The editing actions of the system's menu and the state it last got.
    pub(super) editing: Vec<(ActionId, ActionState)>,
}

impl<M: 'static> Ui<M> {
    /// The menus of `model` as the system's menu bar shows them on macOS (F2.11e1, ADR-0048 AM9):
    /// every action's name, first shortcut and state resolved from the registry, the items with
    /// a role moved into the application menu named `app_name` ([`SystemMenu::for_macos`]). From
    /// then on the interface keeps the changed actions for the window
    /// ([`Ui::take_menu_updates`]).
    ///
    /// The application calls it when the window asks for the system's menu
    /// ([`WindowApp::system_menu`](crate::WindowApp::system_menu)), on macOS only; on Linux and
    /// Windows it draws its bar instead ([`Ui::add_menu_bar`]).
    pub fn system_menu(&mut self, model: &MenuModel, app_name: &str) -> SystemMenu {
        let menus = self.system_items(&model.items);
        let menu = SystemMenu::for_macos(menus, app_name);
        // The editing actions the menu shows, with the state it was built with.
        let mut editing = Vec::new();
        for item in menu.actions() {
            if let SystemItem::Action { action, state, .. } = item
                && self
                    .actions()
                    .and_then(|registry| registry.spec(*action))
                    .is_some_and(|spec| spec.edit.is_some())
            {
                editing.push((*action, *state));
            }
        }
        self.system = Some(SystemTracking {
            changed: Vec::new(),
            editing,
        });
        menu
    }

    /// `items` of the model as the system's items, resolved now.
    fn system_items(&self, items: &[MenuItem]) -> Vec<SystemItem> {
        items
            .iter()
            .map(|item| match item {
                MenuItem::Action { action, role } => SystemItem::Action {
                    action: *action,
                    name: self
                        .actions()
                        .map(|registry| registry.name_text(*action).to_owned())
                        .unwrap_or_default(),
                    shortcut: self
                        .actions()
                        .and_then(|registry| registry.shortcuts(*action).first().copied()),
                    state: self.action_state(*action).unwrap_or(ActionState::DISABLED),
                    role: *role,
                },
                MenuItem::Submenu {
                    name, role, items, ..
                } => SystemItem::Submenu {
                    name: name.clone(),
                    role: *role,
                    items: self.system_items(&items.items),
                },
                MenuItem::Separator => SystemItem::Separator,
            })
            .collect()
    }

    /// Hands to `out` every action of the system's menu whose state changed since the last call,
    /// with its state as the user sees it now (see the module documentation); nothing before
    /// [`Ui::system_menu`]. The window calls it after every batch of events.
    ///
    /// # Cost
    /// O(changed + editing actions); no allocation once the vectors have their capacity.
    pub fn take_menu_updates(&mut self, out: &mut Vec<(ActionId, ActionState)>) {
        let Some(mut tracking) = self.system.take() else {
            return;
        };
        for &action in &tracking.changed {
            if let Some(state) = self.action_state(action) {
                out.push((action, state));
            }
        }
        tracking.changed.clear();
        for (action, last) in &mut tracking.editing {
            let Some(state) = self.action_state(*action) else {
                continue;
            };
            if state != *last {
                *last = state;
                // Not twice: the registry may have marked it too.
                if !out.iter().any(|(own, _)| own == action) {
                    out.push((*action, state));
                }
            }
        }
        self.system = Some(tracking);
    }

    /// Keeps the actions the registry marked at this frame for the system's menu (called by
    /// `refresh_menus` with what it drained), once each.
    pub(super) fn keep_for_system(&mut self, changed: &[ActionId]) {
        let Some(tracking) = self.system.as_mut() else {
            return;
        };
        for action in changed {
            if !tracking.changed.contains(action) {
                tracking.changed.push(*action);
            }
        }
    }

    /// Runs `action` as the user chose it in the system's menu (F2.11e1), on the same path as a
    /// shortcut and the drawn menus (AM13: an editing action goes to the focused field), then runs
    /// a frame. A drawn menu still open (a context menu) closes first, as the system's menu takes
    /// over.
    pub fn run_action(&mut self, action: ActionId, list: &mut DisplayList) {
        self.close_menus_from(0, list, true);
        self.bar_reset();
        self.invoke(action);
        self.frame(list);
    }
}
