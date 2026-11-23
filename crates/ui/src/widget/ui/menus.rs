// SPDX-License-Identifier: GPL-3.0-or-later

//! What a drawn menu does (F2.11d2a, ADR-0048 AM11, AM12, AM13): the highlight, the keyboard,
//! the pointer, submenus, choosing an item, context menus, and keeping the items up to date with
//! their actions' states. A child module of the interface's, because a menu changes the routing
//! itself (the keys of the topmost menu, a press inside it, the focus that stays): the same
//! reason the tooltip lives with the interface (F2.11c).
//!
//! # A menu built from a model
//! [`Ui::open_menu_model`] opens a menu layer (the popup layer of F2.11c) whose root is a
//! [`MenuPanel`] and whose children are one [`MenuEntry`] per item of the [`MenuModel`]. The
//! layer remembers its entries' slots, which one is **highlighted**, and, for a submenu, which
//! entry of the menu below opened it. The menus open at once form one **chain**: a context menu,
//! its submenu, the submenu's submenu, in the stack of open menus.
//!
//! # The keyboard (AM12)
//! With a menu of a model on top, the keys are its own (AM11): Up and Down move the highlight,
//! wrapping and skipping separators and disabled items (which stay visible and readable); Home
//! and End go to the first and last item; Enter or Space chooses (an action runs, a submenu opens
//! with its first item highlighted); Right opens a submenu; Left and Escape close a submenu back
//! to its parent; Escape closes the first menu of the chain. Every other key is consumed, so
//! nothing reaches the window below; the mnemonics' letters come with F2.11d2b. A menu opened from
//! the keyboard (the Menu key, Shift+F10, an assistive technology) starts with its first item
//! highlighted, one opened by the pointer with none.
//!
//! # The pointer (AM12)
//! Moving over an item highlights it; moving over a submenu's item opens its submenu after the
//! pointer rests there [`SUBMENU_DELAY`], and moving to another item of the same menu closes an
//! open submenu after the same delay (so a diagonal move towards the submenu across a neighbour
//! does not lose it). A press on a submenu's item opens it at once. **Choosing happens on the
//! release** over an enabled action, so press-drag-release works; but only once the menu is
//! **armed**: a press inside it, or the pointer having moved since the menu opened. Without that
//! rule a context menu that opens on the press (Linux, macOS) under the pointer would choose its
//! first item at the release of the very press that opened it.
//!
//! # Choosing (AM13)
//! Every menu of the chain closes (silently: the caller knows) and the action runs on the same
//! path as its shortcut ([`Ui::invoke`]): a standard editing action goes to the focused widget,
//! which kept the focus while the menu was open, so Copy in a text field's context menu copies the
//! field's selection.
//!
//! # Context menus (AM12)
//! A widget answers [`Widget::context_menu`]; the interface opens it with the secondary button
//! (on the **press** on Linux and macOS, on the **release** on Windows, their conventions), when
//! no widget handled that press or release; with the Menu key or Shift+F10, left unhandled by the
//! focused widget; and for AccessKit's `ShowContextMenu`. A click opens it under the pointer,
//! rounded to whole logical pixels (as the tooltip, F2.11c: its glyphs then reuse their cached
//! sub-pixel phases); the keyboard at the widget (at the caret of a widget that takes text), so
//! it never opens at a pointer that may be anywhere.
//!
//! # States that change while a menu is open (AM2, choice S3)
//! Every frame drains the registry's marks of changed actions ([`Registry::drain_changed`]) and
//! brings the entries of the open menus up to date: recorded again when their look changed, laid
//! out again when a text changed (a new shortcut, another language). A disabled item loses the
//! highlight.
//!
//! # Accessibility (AM12)
//! The open menus are children of the window node; while one is open, the accessibility focus is
//! its highlighted item (or the menu itself before anything is highlighted), as GTK and Windows
//! report a menu's focus, even though the keyboard focus of the interface stays on the widget
//! that had it (AM11).
//!
//! # Allocation
//! Opening a menu allocates its widgets and their texts (once per opening, well within the 50 ms
//! of an input, AM15). Moving the highlight, the keys and the pointer inside an open menu
//! allocate nothing.

use std::time::{Duration, Instant};

use crate::action::registry::{ActionId, ActionState};
use crate::action::shortcut::Platform;
use crate::draw::geometry::{Point, Rect};
use crate::draw::list::DisplayList;
use crate::widget::layout::LayoutProps;
use crate::widget::menu::{EntryKind, MenuEntry, MenuItem, MenuModel, MenuPanel};
use crate::widget::popup::{Popup, PopupSide};
use crate::widget::tree::{Widget, WidgetId};
use crate::window::input::{ButtonState, Key, Modifiers, NamedKey, PointerButton};

use super::{MenuLayer, Ui, UiError};

/// How long the pointer must rest on a submenu's item before the submenu opens, and on another
/// item before an open submenu closes (ADR-0048 AM12): about the platforms' own delay (GTK 225 ms,
/// Windows' `SPI_GETMENUSHOWDELAY` 400 ms, macOS shorter).
pub(crate) const SUBMENU_DELAY: Duration = Duration::from_millis(250);

/// What the interface knows about a menu built from a model (see the module documentation).
#[derive(Debug, Clone, Default)]
pub(super) struct ModelMenu {
    /// The entries' slots, in the model's order (separators included).
    pub(super) entries: Vec<usize>,
    /// The highlighted entry, by its place in `entries`.
    pub(super) highlighted: Option<usize>,
    /// For a submenu, the entry of the menu below in the stack that opened it.
    pub(super) parent: Option<usize>,
}

/// A pending change of submenus, after the pointer rested [`SUBMENU_DELAY`] (see the module
/// documentation).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct SubmenuTimer {
    /// The menu, by its place in the stack of open menus.
    pub(super) menu: usize,
    /// The entry the pointer rested on, by its place in the menu's entries; `None` for none.
    pub(super) entry: Option<usize>,
    /// When the change happens.
    pub(super) at: Instant,
}

/// The state of the open chain of menus (see the module documentation).
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct MenuChain {
    /// The chain is **armed**: a press happened inside it, or the pointer moved since its first
    /// menu opened; only then does a release choose an item.
    pub(super) armed: bool,
    /// Where the pointer was when the first menu of the chain opened.
    pub(super) opened_at: Point,
}

/// Where the keyboard moves a menu's highlight.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    /// To the next selectable item, wrapping.
    Next,
    /// To the previous one, wrapping.
    Previous,
    /// To the first.
    First,
    /// To the last.
    Last,
}

/// `true` for the keys that open the focused widget's context menu (ADR-0048 AM8, AM12): the
/// Menu key whatever is held, and Shift+F10 (Shift alone).
fn opens_context_menu(key: &Key, modifiers: Modifiers) -> bool {
    match key {
        Key::Named(NamedKey::ContextMenu) => true,
        Key::Named(NamedKey::Function(10)) => {
            modifiers.shift && !modifiers.primary && !modifiers.secondary && !modifiers.alt
        }
        _ => false,
    }
}

impl<M: 'static> Ui<M> {
    /// Opens the menu `model` next to `popup.anchor` (F2.11d2a, ADR-0048 AM9, AM12) and answers
    /// its ID. Its items come from the model and the registry ([`Ui::set_actions`]): an action's
    /// name, shortcut and state are the registry's, and the menu then behaves as the module
    /// documentation of [`Ui`]'s menus says. With `from_keyboard` its first item is highlighted
    /// at once, as when the keyboard opens a menu.
    ///
    /// The interface opens context menus by itself ([`Widget::context_menu`]); this is for
    /// menus the application opens (the menu bar's, F2.11d2b, and tests).
    ///
    /// # Errors
    /// [`UiError::UnknownWidget`] if `popup.owner` does not exist.
    pub fn open_menu_model(
        &mut self,
        model: &MenuModel,
        popup: Popup,
        from_keyboard: bool,
        list: &mut DisplayList,
    ) -> Result<WidgetId, UiError> {
        let owner = match popup.owner {
            Some(id) => Some(self.tree.slot_of(id).ok_or(UiError::UnknownWidget(id))?),
            None => None,
        };
        let root = self.push_model_menu(model, popup, owner, from_keyboard, None);
        self.frame(list);
        self.id_of(root)
            .ok_or(UiError::UnknownWidget(WidgetId::placeholder()))
    }

    /// The items of the open menu `menu` built from a model, in order (separators included), for
    /// tests and the screenshots' scenes; nothing for another widget.
    pub fn menu_items(&self, menu: WidgetId) -> impl Iterator<Item = WidgetId> + '_ {
        let entries = self
            .tree
            .slot_of(menu)
            .and_then(|root| self.menus.iter().find(|layer| layer.root == root))
            .and_then(|layer| layer.model.as_ref())
            .map_or(&[][..], |model| model.entries.as_slice());
        entries.iter().filter_map(|&slot| self.id_of(slot))
    }

    /// The highlighted item of the topmost open menu, if it was built from a model and something
    /// is highlighted.
    #[must_use]
    pub fn highlighted_menu_item(&self) -> Option<WidgetId> {
        let model = self.menus.last()?.model.as_ref()?;
        let slot = *model.entries.get(model.highlighted?)?;
        self.id_of(slot)
    }

    /// The platform whose conventions the menus follow: the registry's, or the one the program
    /// was built for before the application handed a registry in.
    pub(super) fn platform(&self) -> Platform {
        self.actions
            .as_ref()
            .map_or(Platform::CURRENT, |actions| actions.registry.platform())
    }

    /// Opens `model` as the next menu of the stack (without running a frame) and answers its
    /// root's slot; `parent` is the entry of the menu below that opened it, for a submenu.
    pub(super) fn push_model_menu(
        &mut self,
        model: &MenuModel,
        popup: Popup,
        owner: Option<usize>,
        from_keyboard: bool,
        parent: Option<usize>,
    ) -> usize {
        let root = self.push_menu(Box::new(MenuPanel), popup, owner);
        // A column for the check mark when any action of the menu is a toggle, so the names of
        // all the entries line up.
        let checks = model.items.iter().any(|item| match item {
            MenuItem::Action { action, .. } => self
                .action_state(*action)
                .is_some_and(|state| state.checked.is_some()),
            _ => false,
        });
        let count = model
            .items
            .iter()
            .filter(|item| !matches!(item, MenuItem::Separator))
            .count();
        let mut entries = Vec::with_capacity(model.items.len());
        let mut position = 0;
        for item in &model.items {
            let mut entry = self.entry_for(item);
            if !matches!(item, MenuItem::Separator) {
                position += 1;
            }
            entry.set_place(checks, (position, count));
            let slot = self
                .tree
                .insert(Some(root), Box::new(entry), LayoutProps::default());
            entries.push(slot);
        }
        if let Some(layer) = self.menus.last_mut() {
            layer.model = Some(ModelMenu {
                entries,
                highlighted: None,
                parent,
            });
        }
        if from_keyboard {
            let top = self.menus.len().saturating_sub(1);
            self.step(top, Step::First);
        }
        root
    }

    /// The entry of `item`, with its texts and state as they are now.
    fn entry_for(&self, item: &MenuItem) -> MenuEntry {
        match item {
            MenuItem::Action { action, .. } => {
                let state = self.action_state(*action).unwrap_or(ActionState::DISABLED);
                let (name, shortcut, mnemonic) = self.actions().map_or_else(
                    || (String::new(), String::new(), None),
                    |registry| {
                        (
                            registry.name_text(*action).to_owned(),
                            registry.shortcut_text(*action).to_owned(),
                            registry.mnemonic(*action),
                        )
                    },
                );
                MenuEntry::new(
                    EntryKind::Action {
                        action: *action,
                        state,
                    },
                    name,
                    shortcut,
                    mnemonic,
                )
            }
            MenuItem::Submenu {
                name,
                mnemonic,
                items,
                ..
            } => MenuEntry::new(
                EntryKind::Submenu {
                    items: items.clone(),
                    open: false,
                },
                name.clone(),
                String::new(),
                // Compared in lower case, as the registry keeps the actions' letters.
                mnemonic.and_then(|letter| letter.to_lowercase().next()),
            ),
            MenuItem::Separator => {
                MenuEntry::new(EntryKind::Separator, String::new(), String::new(), None)
            }
        }
    }

    /// The menu entry in `slot`, if it is one.
    fn entry(&self, slot: usize) -> Option<&MenuEntry> {
        self.tree
            .slot(slot)?
            .widget
            .as_deref()?
            .as_any()
            .downcast_ref::<MenuEntry>()
    }

    /// The menu entry in `slot`, mutably, if it is one.
    fn entry_mut(&mut self, slot: usize) -> Option<&mut MenuEntry> {
        self.tree
            .slot_mut(slot)?
            .widget
            .as_deref_mut()?
            .as_any_mut()
            .downcast_mut::<MenuEntry>()
    }

    /// The slot of entry `entry` of the open menu `menu` (both by place).
    fn entry_slot(&self, menu: usize, entry: usize) -> Option<usize> {
        self.menus
            .get(menu)?
            .model
            .as_ref()?
            .entries
            .get(entry)
            .copied()
    }

    /// The highlighted entry of the open menu `menu`.
    fn highlighted(&self, menu: usize) -> Option<usize> {
        self.menus.get(menu)?.model.as_ref()?.highlighted
    }

    /// Which open menu built from a model `slot` belongs to, by place in the stack, and which of
    /// its entries it is (`None` for the panel itself).
    ///
    /// # Cost
    /// O(open menus × their entries): a handful of each.
    pub(super) fn model_menu_of(&self, slot: usize) -> Option<(usize, Option<usize>)> {
        let parent = self.tree.slot(slot)?.parent;
        self.menus.iter().enumerate().find_map(|(index, layer)| {
            let model = layer.model.as_ref()?;
            if layer.root == slot {
                Some((index, None))
            } else if parent == Some(layer.root) {
                let entry = model.entries.iter().position(|&own| own == slot);
                Some((index, entry))
            } else {
                None
            }
        })
    }

    /// Highlights `entry` of the open menu `menu` (or nothing), recording the two entries that
    /// changed and nothing else (AM15).
    fn set_highlight(&mut self, menu: usize, entry: Option<usize>) {
        let Some(old) = self.menus.get(menu).and_then(|layer| layer.model.as_ref()) else {
            return;
        };
        let old = old.highlighted;
        if old == entry {
            return;
        }
        for (place, on) in [(old, false), (entry, true)] {
            let Some(slot) = place.and_then(|place| self.entry_slot(menu, place)) else {
                continue;
            };
            if self
                .entry_mut(slot)
                .is_some_and(|widget| widget.set_highlighted(on))
                && let Some(slot) = self.tree.slot_mut(slot)
            {
                slot.needs_record = true;
            }
        }
        if let Some(model) = self
            .menus
            .get_mut(menu)
            .and_then(|layer| layer.model.as_mut())
        {
            model.highlighted = entry;
        }
    }

    /// Moves the highlight of the open menu `menu` as the keyboard asks (AM12), skipping what
    /// cannot be chosen. The keys always reach the topmost menu (AM11), so no submenu hangs from
    /// the menu whose highlight moves.
    fn step(&mut self, menu: usize, step: Step) {
        let Some(model) = self.menus.get(menu).and_then(|layer| layer.model.as_ref()) else {
            return;
        };
        let count = model.entries.len();
        let current = model.highlighted;
        // The candidates in the order the step walks them: forwards from after the current one
        // (or from the start), backwards from before it (or from the end), each once.
        let start = match (step, current) {
            (Step::Next, Some(at)) => at + 1,
            (Step::Previous, Some(at)) => at + count - 1,
            (Step::Next | Step::First, _) => 0,
            (Step::Previous | Step::Last, _) => count.saturating_sub(1),
        };
        let forward = matches!(step, Step::Next | Step::First);
        let found = (0..count)
            .map(|offset| {
                if forward {
                    (start + offset) % count
                } else {
                    (start + count - offset % count) % count
                }
            })
            .find(|&place| {
                self.entry_slot(menu, place)
                    .and_then(|slot| self.entry(slot))
                    .is_some_and(MenuEntry::selectable)
            });
        if let Some(found) = found {
            self.set_highlight(menu, Some(found));
        }
    }

    /// Chooses entry `entry` of the open menu `menu` (AM12, AM13): an enabled action closes the
    /// whole chain and runs; a submenu opens (with its first item highlighted when
    /// `from_keyboard`). A disabled action or a separator does nothing.
    fn activate(&mut self, menu: usize, entry: usize, from_keyboard: bool, list: &mut DisplayList) {
        let Some(slot) = self.entry_slot(menu, entry) else {
            return;
        };
        let Some(widget) = self.entry(slot) else {
            return;
        };
        if !widget.selectable() {
            return;
        }
        match widget.kind() {
            EntryKind::Action { action, .. } => {
                let action = *action;
                self.close_menus_from(0, list, false);
                // Choosing also leaves the bar, when the menu was one of its (F2.11d2b).
                self.bar_reset();
                self.invoke(action);
            }
            EntryKind::Submenu { .. } => self.open_submenu(menu, entry, from_keyboard, list),
            EntryKind::Separator => {}
        }
    }

    /// Opens the submenu of entry `entry` of the open menu `menu`, beside it (AM11: on the right,
    /// on the left without room), closing any other submenu of that menu first. Already open: only
    /// the keyboard's highlight of its first item is added.
    fn open_submenu(
        &mut self,
        menu: usize,
        entry: usize,
        from_keyboard: bool,
        list: &mut DisplayList,
    ) {
        let open_here = self
            .menus
            .get(menu + 1)
            .and_then(|layer| layer.model.as_ref())
            .is_some_and(|model| model.parent == Some(entry));
        if open_here {
            if from_keyboard && self.highlighted(menu + 1).is_none() {
                self.step(menu + 1, Step::First);
            }
            return;
        }
        self.close_menus_from(menu + 1, list, false);
        let Some(slot) = self.entry_slot(menu, entry) else {
            return;
        };
        let Some(EntryKind::Submenu { items, .. }) = self.entry(slot).map(MenuEntry::kind) else {
            return;
        };
        let items = items.clone();
        let rect = self.tree.slot(slot).map_or(Rect::EMPTY, |slot| slot.rect);
        // The submenu's first item level with this entry: its panel starts its padding higher.
        let pad = self.tree.spacing.gap_xs;
        let anchor = Rect::new(rect.x0, rect.y0 - pad, rect.x1, rect.y1);
        let popup = Popup {
            anchor,
            side: PopupSide::Right,
            owner: None,
        };
        self.push_model_menu(&items, popup, None, from_keyboard, Some(entry));
        if self
            .entry_mut(slot)
            .is_some_and(|widget| widget.set_open(true))
            && let Some(slot) = self.tree.slot_mut(slot)
        {
            slot.needs_access = true;
        }
    }

    /// A key while a menu built from a model is the topmost (AM11, AM12; see the module
    /// documentation). Releases and every key it does not use are consumed.
    pub(super) fn menu_key(
        &mut self,
        key: &Key,
        state: ButtonState,
        repeat: bool,
        modifiers: Modifiers,
        list: &mut DisplayList,
    ) {
        if state != ButtonState::Pressed {
            return;
        }
        // A menu of the bar: F10, Left and Right towards the other titles, Escape to the title
        // (F2.11d2b, the module `bar`).
        if self.bar_menu_key(key, modifiers, list) {
            return;
        }
        let top = self.menus.len().saturating_sub(1);
        let Key::Named(named) = key else {
            // A letter chooses the item with that mnemonic (AM10, F2.11d2b), alone or with Alt;
            // never with *Primary* or *Secondary* (a shortcut's key, consumed here).
            if !modifiers.primary
                && !modifiers.secondary
                && !repeat
                && let Some(item) = Self::key_letter(key).and_then(|letter| self.item_with(letter))
            {
                self.set_highlight(top, Some(item));
                self.activate(top, item, true, list);
            }
            return;
        };
        let is_submenu = self
            .menus
            .get(top)
            .and_then(|layer| layer.model.as_ref())
            .is_some_and(|model| model.parent.is_some());
        match named {
            NamedKey::ArrowDown => self.step(top, Step::Next),
            NamedKey::ArrowUp => self.step(top, Step::Previous),
            NamedKey::Home => self.step(top, Step::First),
            NamedKey::End => self.step(top, Step::Last),
            NamedKey::Enter | NamedKey::Space if !repeat => {
                if let Some(entry) = self.highlighted(top) {
                    self.activate(top, entry, true, list);
                }
            }
            NamedKey::ArrowRight => {
                if let Some(entry) = self.highlighted(top)
                    && self
                        .entry_slot(top, entry)
                        .and_then(|slot| self.entry(slot))
                        .is_some_and(|widget| matches!(widget.kind(), EntryKind::Submenu { .. }))
                {
                    self.open_submenu(top, entry, true, list);
                }
            }
            NamedKey::ArrowLeft if is_submenu => self.close_menus_from(top, list, false),
            // A submenu closes back to its parent; the first menu of a chain closes, and its
            // owner is told (the interface closed it by itself).
            NamedKey::Escape => self.close_menus_from(top, list, !is_submenu),
            _ => {}
        }
    }

    /// `true` when the highlighted item of the topmost menu opens a submenu (Right then opens it,
    /// instead of moving to the bar's next title).
    pub(super) fn highlighted_opens_submenu(&self) -> bool {
        let top = self.menus.len().saturating_sub(1);
        self.highlighted(top)
            .and_then(|place| self.entry_slot(top, place))
            .and_then(|slot| self.entry(slot))
            .is_some_and(|widget| matches!(widget.kind(), EntryKind::Submenu { .. }))
    }

    /// The pointer moved to `position` while menus are open and nothing captures it (AM12): the
    /// item under it is highlighted, and a submenu's opening or closing is scheduled. The hover
    /// is already up to date.
    pub(super) fn menu_pointer_moved(&mut self, position: Point) {
        if position != self.chain.opened_at {
            self.chain.armed = true;
        }
        let found = self.hovered.and_then(|slot| self.model_menu_of(slot));
        let top = self.menus.len().saturating_sub(1);
        let Some((menu, entry)) = found else {
            // Outside every menu: the topmost loses its highlight unless a submenu hangs from it,
            // and nothing is scheduled.
            if self
                .menus
                .get(top)
                .is_some_and(|layer| layer.model.is_some())
            {
                self.set_highlight(top, None);
            }
            self.submenu_timer = None;
            return;
        };
        // Inside a submenu, the chain below keeps pointing at it: each menu highlights the entry
        // whose submenu is open, even if the pointer crossed a neighbour on its way here.
        for below in (0..menu).rev() {
            let parent = self
                .menus
                .get(below + 1)
                .and_then(|layer| layer.model.as_ref())
                .and_then(|model| model.parent);
            if parent.is_some() {
                self.set_highlight(below, parent);
            }
        }
        if self.submenu_timer.is_some_and(|timer| timer.menu < menu) {
            self.submenu_timer = None;
        }
        let selectable = entry.filter(|&place| {
            self.entry_slot(menu, place)
                .and_then(|slot| self.entry(slot))
                .is_some_and(MenuEntry::selectable)
        });
        if selectable == self.highlighted(menu) {
            return;
        }
        // Over a separator, a disabled item or the padding: no highlight there, but a submenu
        // open from this menu keeps its entry highlighted until the delay closes it.
        let submenu_open = self.menus.len() > menu + 1;
        if selectable.is_some() || !submenu_open {
            self.set_highlight(menu, selectable);
        }
        let opens_submenu = selectable
            .and_then(|place| self.entry_slot(menu, place))
            .and_then(|slot| self.entry(slot))
            .is_some_and(|widget| matches!(widget.kind(), EntryKind::Submenu { .. }));
        // Something to do after the delay: open this entry's submenu, or close the one open.
        self.submenu_timer = (opens_submenu || submenu_open).then(|| SubmenuTimer {
            menu,
            entry: selectable,
            at: self.now + SUBMENU_DELAY,
        });
    }

    /// The submenu delay came (from [`Ui::advance`]): if the pointer still rests where it did,
    /// its entry's submenu opens, or the submenu open from that menu closes.
    pub(super) fn submenu_due(&mut self, timer: SubmenuTimer, list: &mut DisplayList) {
        if self.highlighted(timer.menu) != timer.entry && timer.entry.is_some() {
            return;
        }
        let opens = timer
            .entry
            .and_then(|place| self.entry_slot(timer.menu, place))
            .and_then(|slot| self.entry(slot))
            .is_some_and(|widget| matches!(widget.kind(), EntryKind::Submenu { .. }));
        match timer.entry {
            Some(entry) if opens => self.open_submenu(timer.menu, entry, false, list),
            _ => {
                self.close_menus_from(timer.menu + 1, list, false);
                // The entry whose submenu just closed is no longer under the pointer.
                if timer.entry.is_none() {
                    self.set_highlight(timer.menu, None);
                }
            }
        }
    }

    /// A press on `target` while menus are open (AM12): inside a menu built from a model it arms
    /// the menu, opens a submenu at once, and is consumed (`true`); anywhere else it is not this
    /// module's (`false`).
    pub(super) fn menu_press(&mut self, target: usize, list: &mut DisplayList) -> bool {
        let Some((menu, entry)) = self.model_menu_of(target) else {
            return false;
        };
        self.chain.armed = true;
        if let Some(entry) = entry
            && self
                .entry_slot(menu, entry)
                .and_then(|slot| self.entry(slot))
                .is_some_and(|widget| matches!(widget.kind(), EntryKind::Submenu { .. }))
        {
            self.submenu_timer = None;
            self.open_submenu(menu, entry, false, list);
        }
        true
    }

    /// A release at `position` while menus are open and nothing captured the pointer (AM12): over
    /// an enabled action of an armed menu it chooses it; anywhere inside a menu built from a model
    /// it is consumed (`true`).
    pub(super) fn menu_release(&mut self, position: Point, list: &mut DisplayList) -> bool {
        let Some((menu, entry)) = self.hit(position).and_then(|slot| self.model_menu_of(slot))
        else {
            return false;
        };
        if let Some(entry) = entry
            && self.chain.armed
            && self
                .entry_slot(menu, entry)
                .and_then(|slot| self.entry(slot))
                .is_some_and(|widget| widget.action().is_some())
        {
            self.activate(menu, entry, false, list);
        }
        true
    }

    /// An assistive technology clicked `slot` (AccessKit's click, WC5): an entry of an open menu
    /// is chosen as Enter would choose it (`true`); anything else is not this module's.
    pub(super) fn menu_access_click(&mut self, slot: usize, list: &mut DisplayList) -> bool {
        let Some((menu, Some(entry))) = self.model_menu_of(slot) else {
            return false;
        };
        self.activate(menu, entry, true, list);
        true
    }

    /// The pointer button `button` was pressed (`released == false`) or released on `target`
    /// and no widget handled it, with no menu open: on the platform's moment for it, the context
    /// menu of `target` or its nearest ancestor that has one opens under the pointer (AM12).
    pub(super) fn secondary_click(
        &mut self,
        button: PointerButton,
        released: bool,
        target: usize,
        position: Point,
    ) {
        // Windows opens a context menu on the release, Linux and macOS on the press.
        let on_release = self.platform() == Platform::Windows;
        if button == PointerButton::Right && released == on_release {
            self.open_context_menu(target, Some(position));
        }
    }

    /// A key pressed and left unhandled from `start` (the focused widget or the modal layer): the
    /// Menu key and Shift+F10 open the context menu at the widget (AM12) and answer `true`.
    pub(super) fn context_menu_key(
        &mut self,
        start: Option<usize>,
        key: &Key,
        modifiers: Modifiers,
    ) -> bool {
        if !opens_context_menu(key, modifiers) {
            return false;
        }
        if let Some(start) = start {
            self.open_context_menu(start, None);
        }
        // Consumed even without a menu to open: the key is fixed (AM8), never a shortcut.
        true
    }

    /// Opens the context menu of `start` or of its nearest ancestor that has one (AM12): under
    /// `at` (the pointer) rounded to whole logical pixels, or at the widget for the keyboard
    /// (`None`), with its first item highlighted then. `true` when a menu opened.
    pub(super) fn open_context_menu(&mut self, start: usize, at: Option<Point>) -> bool {
        let mut current = Some(start);
        let mut found = None;
        while let Some(index) = current {
            let Some(slot) = self.tree.slot(index) else {
                break;
            };
            if let Some(model) = slot.widget.as_deref().and_then(Widget::context_menu) {
                // Copied: the menu's entries are built into the same arena the widget lives in.
                found = Some((index, model.clone()));
                break;
            }
            current = slot.parent;
        }
        let Some((widget, model)) = found else {
            return false;
        };
        let point = at.map_or_else(
            || self.keyboard_anchor(widget),
            |point| Point::new(point.x.round(), point.y.round()),
        );
        let popup = Popup {
            anchor: Rect::new(point.x, point.y, point.x, point.y),
            side: PopupSide::Below,
            owner: None,
        };
        self.push_model_menu(&model, popup, None, at.is_none(), None);
        true
    }

    /// Where a context menu opened from the keyboard goes for the widget in `slot`: below the
    /// caret of a widget that takes text ([`Widget::ime_area`]), below the widget's left edge
    /// otherwise.
    fn keyboard_anchor(&self, slot: usize) -> Point {
        let Some(slot) = self.tree.slot(slot) else {
            return Point::default();
        };
        let area = slot
            .widget
            .as_deref()
            .filter(|widget| widget.accepts_text())
            .and_then(Widget::ime_area)
            .unwrap_or(slot.rect);
        Point::new(area.x0.round(), area.y1.round())
    }

    /// Brings the entries of the open menus up to date with the actions that changed (AM2,
    /// choice S3; see the module documentation). Called at the start of every frame.
    ///
    /// # Cost
    /// O(actions / 64) to drain the marks; with marks and menus open, O(entries) per changed
    /// action. No allocation once the reused vector has its capacity.
    pub(super) fn refresh_menus(&mut self) {
        let Some(actions) = self.actions.as_mut() else {
            return;
        };
        let mut changed = std::mem::take(&mut self.changed_actions);
        changed.clear();
        actions.registry.drain_changed(|id| changed.push(id));
        // The system's menu gets them too, through the window (F2.11e1, the module `system`).
        self.keep_for_system(&changed);
        if !changed.is_empty() {
            for menu in 0..self.menus.len() {
                let count = self
                    .menus
                    .get(menu)
                    .and_then(|layer| layer.model.as_ref())
                    .map_or(0, |model| model.entries.len());
                for place in 0..count {
                    self.refresh_entry(menu, place, &changed);
                }
            }
        }
        self.changed_actions = changed;
    }

    /// Brings entry `place` of the open menu `menu` up to date if its action is in `changed`.
    fn refresh_entry(&mut self, menu: usize, place: usize, changed: &[ActionId]) {
        let Some(slot) = self.entry_slot(menu, place) else {
            return;
        };
        let Some(action) = self
            .entry(slot)
            .and_then(MenuEntry::action)
            .filter(|action| changed.contains(action))
        else {
            return;
        };
        let state = self.action_state(action).unwrap_or(ActionState::DISABLED);
        let Some(registry) = self.actions.as_ref().map(|actions| &actions.registry) else {
            return;
        };
        let (name, shortcut, mnemonic) = (
            registry.name_text(action),
            registry.shortcut_text(action),
            registry.mnemonic(action),
        );
        // The registry and the arena are separate fields: the texts are read while the entry
        // changes, with no copy.
        let redo = self
            .tree
            .slot_mut(slot)
            .and_then(|slot| slot.widget.as_deref_mut())
            .and_then(|widget| widget.as_any_mut().downcast_mut::<MenuEntry>())
            .and_then(|entry| entry.update(state, name, shortcut, mnemonic));
        match redo {
            Some(true) => self.tree.invalidate_layout(slot),
            Some(false) => {}
            None => return,
        }
        if let Some(slot) = self.tree.slot_mut(slot) {
            slot.needs_record = true;
            slot.needs_access = true;
        }
        if !state.enabled
            && let Some(model) = self
                .menus
                .get_mut(menu)
                .and_then(|layer| layer.model.as_mut())
            && model.highlighted == Some(place)
        {
            model.highlighted = None;
        }
    }

    /// A menu just left the stack (from [`Ui::close_menus_from`]): if it was a submenu, the entry
    /// of the menu now on top that opened it is no longer open (assistive technologies say so).
    /// When the last menu went, the chain's state goes with it.
    pub(super) fn submenu_closed(&mut self, closed: Option<&ModelMenu>) {
        if self.menus.is_empty() {
            self.chain.armed = false;
            self.submenu_timer = None;
            // No title of the bar is open any more (F2.11d2b).
            self.bar_menus_closed();
            return;
        }
        let Some(entry) = closed.and_then(|model| model.parent) else {
            return;
        };
        let top = self.menus.len() - 1;
        if let Some(slot) = self.entry_slot(top, entry)
            && self
                .entry_mut(slot)
                .is_some_and(|widget| widget.set_open(false))
            && let Some(slot) = self.tree.slot_mut(slot)
        {
            slot.needs_access = true;
        }
    }

    /// The node assistive technologies should see as focused (see the module documentation):
    /// with a menu open, its highlighted item or the menu itself; otherwise the focused widget.
    pub(super) fn access_focus(&self) -> Option<WidgetId> {
        if let Some(layer) = self.menus.last() {
            let item = layer
                .model
                .as_ref()
                .and_then(|model| model.entries.get(model.highlighted?).copied());
            return self.id_of(item.unwrap_or(layer.root));
        }
        // The bar's title while the keyboard is on the bar (F2.11d2b).
        self.bar_access_focus().or_else(|| self.focused())
    }

    /// Opens a menu layer whose root is `widget` (without running a frame) and answers its root's
    /// slot: the part of [`Ui::open_menu`] shared with the menus built from a model.
    pub(super) fn push_menu(
        &mut self,
        widget: Box<dyn Widget<M>>,
        popup: Popup,
        owner: Option<usize>,
    ) -> usize {
        if self.menus.is_empty() {
            // A new chain: not armed until a press inside it or a move of the pointer.
            self.chain.armed = false;
            self.chain.opened_at = self.pointer;
        }
        let root = self.tree.insert(None, widget, LayoutProps::default());
        // The next layer up; it never reaches the tooltip's (four billion menus open at once
        // cannot happen, and saturating keeps the order right regardless).
        let layer = self.next_layer.min(super::TOOLTIP_LAYER - 1);
        self.next_layer = self.next_layer.saturating_add(1);
        if let Some(slot) = self.tree.slot_mut(root) {
            slot.layer = layer;
        }
        self.menus.push(MenuLayer {
            root,
            owner,
            popup,
            model: None,
        });
        // The window node lists the open menus (see the module documentation).
        self.access_window = true;
        self.hide_tooltip();
        self.tooltip.pending = None;
        // The pointer may be over what the menu now blocks.
        let hovered = self.hit(self.pointer);
        if self.captured.is_none() {
            self.set_hover(hovered);
        }
        root
    }
}
