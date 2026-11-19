// SPDX-License-Identifier: GPL-3.0-or-later

//! The drawn menu bar and the mnemonics (F2.11d2b, ADR-0048 AM9, AM10, AM12): what the bar does,
//! a child module of the interface's like the menus' (`menus`), because the bar changes the
//! routing of keys and presses itself.
//!
//! # The bar
//! [`Ui::add_menu_bar`] adds a [`MenuBar`] with one [`BarTitle`] per submenu of a [`MenuModel`]
//! (File, Edit, View, Help). Its menus are the menus of `menus`, opened below their title with
//! the bar as their **owner** ([`Popup::owner`]): while one is open the pointer still reaches the
//! titles. Linux and Windows only: on macOS the application gives the model to `muda` (F2.11e).
//!
//! # The keyboard (AM12)
//! - **F10**, or **Alt pressed and released alone** (nothing else pressed or clicked in between,
//!   so Alt+Tab and Alt+drag never open it), puts the keyboard on the first title: the bar is
//!   **active**. Left and Right move between the titles, Down, Up, Enter or Space open the
//!   title's menu with its first item highlighted, a title's letter opens it, Escape, F10, Tab or
//!   Alt again leave the bar. Every other key is consumed while the bar is active.
//! - **Alt+letter** opens the title with that mnemonic at once, **before the focused widget sees
//!   the key** (a precision of AM4 and AM8 approved in F2.11d2b): a text field would otherwise
//!   take Alt+F as typing. `AltGr` is not Alt (on Windows it arrives as Ctrl+Alt, on Linux as a key
//!   of its own), so it still types "@" and "€". Only with neither *Primary* nor *Secondary*
//!   held.
//! - In a menu opened from the bar, Left and Right (on an item without a submenu) open the
//!   neighbouring title's menu; Escape closes the menu back to its active title, and a second
//!   Escape leaves the bar.
//!
//! # The pointer (AM12)
//! A press on a title opens its menu (a second press on the open title closes it); while a menu
//! of the bar is open, moving over another title opens that one. Choosing works as in every menu
//! (press-drag-release from the title too). A press anywhere else leaves the bar.
//!
//! # Mnemonics (AM10)
//! Every title and item may have a letter ([`MenuItem::Submenu`]'s, the registry's
//! [`Registry::mnemonic`](crate::Registry::mnemonic)). Inside an open menu the letter alone chooses
//! its item (and Alt+letter too); on the bar it opens its title. The letters are **underlined
//! only while Alt is held, or while the bar is reached from the keyboard** (F10, Alt, Alt+letter),
//! as Windows and KDE do by default; never on macOS, which has no mnemonics. Assistive
//! technologies get the letter as the node's `access_key`.
//!
//! # Cost
//! Opening a menu of the bar costs what opening any menu costs (`menus`). Keeping the
//! underlines up to date walks the bar's titles and the open menus' entries once per event (a
//! few tens of flag tests), and records only the ones that changed. No allocation after the bar
//! exists.

use crate::action::shortcut::Platform;
use crate::draw::geometry::{Point, Rect};
use crate::draw::list::DisplayList;
use crate::widget::layout::LayoutProps;
use crate::widget::menu::{BarTitle, MenuBar, MenuEntry, MenuItem, MenuModel};
use crate::widget::popup::{Popup, PopupSide};
use crate::widget::tree::WidgetId;
use crate::window::input::{ButtonState, Key, Modifiers, NamedKey};

use super::{Ui, UiError};

/// What the interface knows about its menu bar (see the module documentation).
#[derive(Debug, Clone)]
pub(super) struct BarState {
    /// The bar's slot.
    pub(super) root: usize,
    /// Its titles' slots, in order.
    pub(super) titles: Vec<usize>,
    /// Each title's menu.
    pub(super) menus: Vec<MenuModel>,
    /// The title the keyboard is on (the bar is active), if any.
    pub(super) active: Option<usize>,
    /// The title whose menu is open, if any.
    pub(super) open: Option<usize>,
    /// The bar was reached from the keyboard (F10, Alt, Alt+letter): the mnemonics stay underlined
    /// until it is left.
    pub(super) keyboard: bool,
}

/// The single letter of a character key, in lower case, if it is one: what a mnemonic is compared
/// with.
fn letter_of(key: &Key) -> Option<char> {
    let Key::Character(text) = key else {
        return None;
    };
    let mut characters = text.chars();
    let (Some(character), None) = (characters.next(), characters.next()) else {
        return None;
    };
    let mut lowered = character.to_lowercase();
    match (lowered.next(), lowered.next()) {
        (Some(single), None) => Some(single),
        _ => Some(character),
    }
}

/// `true` for F10 with no modifier: the key that reaches or leaves the bar (AM12).
fn is_f10(key: &Key, modifiers: Modifiers) -> bool {
    *key == Key::Named(NamedKey::Function(10)) && modifiers == Modifiers::default()
}

impl<M: 'static> Ui<M> {
    /// Adds the menu bar (F2.11d2b, ADR-0048 AM9) as the last child of `parent`, laid out by
    /// `props` (usually the first row of the window, its width filled), and answers its ID. Each
    /// submenu of `model` becomes a title (its name and mnemonic) and its items that title's menu;
    /// other items of `model` are ignored. The interface then drives the bar as the module
    /// documentation of its `bar` says: an interface has one bar, and a second call replaces the
    /// one it drives.
    ///
    /// A bar is for Linux and Windows (AM9): on macOS the application gives the same model to the
    /// system's menu bar instead (F2.11e).
    ///
    /// # Errors
    /// [`UiError::UnknownWidget`] if `parent` does not exist.
    pub fn add_menu_bar(
        &mut self,
        parent: WidgetId,
        model: &MenuModel,
        props: LayoutProps,
    ) -> Result<WidgetId, UiError> {
        let bar = self.add(parent, MenuBar, props)?;
        let root = self.tree.slot_of(bar).ok_or(UiError::UnknownWidget(bar))?;
        let mut titles = Vec::new();
        let mut menus = Vec::new();
        for item in &model.items {
            if let MenuItem::Submenu {
                name,
                mnemonic,
                items,
                ..
            } = item
            {
                let title = BarTitle::new(name.clone(), mnemonic.map(lower));
                titles.push(
                    self.tree
                        .insert(Some(root), Box::new(title), LayoutProps::default()),
                );
                menus.push(items.clone());
            }
        }
        self.bar = Some(BarState {
            root,
            titles,
            menus,
            active: None,
            open: None,
            keyboard: false,
        });
        Ok(bar)
    }

    /// The titles of the menu bar, in order (tests, screenshots); nothing without a bar.
    pub fn menu_bar_titles(&self) -> impl Iterator<Item = WidgetId> + '_ {
        let titles = self
            .bar
            .as_ref()
            .map_or(&[][..], |bar| bar.titles.as_slice());
        titles.iter().filter_map(|&slot| self.id_of(slot))
    }

    /// The title the keyboard is on while the bar is active (F10, Alt), by its place; `None`
    /// otherwise.
    #[must_use]
    pub fn menu_bar_active(&self) -> Option<usize> {
        self.bar.as_ref()?.active
    }

    /// The title whose menu is open, by its place; `None` when no menu of the bar is open.
    #[must_use]
    pub fn menu_bar_open(&self) -> Option<usize> {
        self.bar.as_ref()?.open
    }

    /// `true` while the mnemonics are underlined (AM10): Alt is held, or the bar was reached from
    /// the keyboard; never on macOS. For tests and the screenshots' scenes.
    #[must_use]
    pub fn mnemonics_underlined(&self) -> bool {
        self.platform() != Platform::MacOs
            && (self.modifiers.alt
                || self
                    .bar
                    .as_ref()
                    .is_some_and(|bar| bar.keyboard || bar.active.is_some()))
    }

    /// `true` when the bar can be used here: it exists, and the platform is not macOS.
    fn bar_usable(&self) -> bool {
        self.bar.is_some() && self.platform() != Platform::MacOs
    }

    /// The place of the title in `slot`, if it is one of the bar's.
    pub(super) fn bar_title_of(&self, slot: usize) -> Option<usize> {
        self.bar
            .as_ref()?
            .titles
            .iter()
            .position(|&own| own == slot)
    }

    /// `true` when the open menus were opened from the bar (the first is owned by it).
    pub(super) fn bar_chain(&self) -> bool {
        match (self.bar.as_ref(), self.menus.first()) {
            (Some(bar), Some(first)) => first.owner == Some(bar.root),
            _ => false,
        }
    }

    /// Opens the menu of title `title` below it, closing every open menu first (AM12); from the
    /// keyboard its first item is highlighted and the bar stays active on that title.
    pub(super) fn open_bar_menu(
        &mut self,
        title: usize,
        from_keyboard: bool,
        list: &mut DisplayList,
    ) {
        self.close_menus_from(0, list, false);
        let Some(bar) = self.bar.as_ref() else {
            return;
        };
        let (Some(&slot), Some(model)) = (bar.titles.get(title), bar.menus.get(title)) else {
            return;
        };
        let model = model.clone();
        let owner = bar.root;
        let anchor = self.tree.slot(slot).map_or(Rect::EMPTY, |slot| slot.rect);
        let popup = Popup {
            anchor,
            side: PopupSide::Below,
            owner: self.id_of(owner),
        };
        self.push_model_menu(&model, popup, Some(owner), from_keyboard, None);
        if let Some(bar) = self.bar.as_mut() {
            bar.open = Some(title);
            bar.active = from_keyboard.then_some(title);
            bar.keyboard = from_keyboard;
        }
    }

    /// Leaves the bar: every menu closes, the keyboard leaves the titles.
    pub(super) fn leave_bar(&mut self, list: &mut DisplayList) {
        self.close_menus_from(0, list, false);
        if let Some(bar) = self.bar.as_mut() {
            bar.active = None;
            bar.open = None;
            bar.keyboard = false;
        }
    }

    /// Puts the keyboard on the bar's first title (F10, Alt alone), closing any open menu.
    fn enter_bar(&mut self, list: &mut DisplayList) {
        self.close_menus_from(0, list, false);
        if let Some(bar) = self.bar.as_mut()
            && !bar.titles.is_empty()
        {
            bar.active = Some(0);
            bar.open = None;
            bar.keyboard = true;
        }
    }

    /// The number of titles.
    fn title_count(&self) -> usize {
        self.bar.as_ref().map_or(0, |bar| bar.titles.len())
    }

    /// The title whose mnemonic is `letter`, if any.
    fn title_with(&self, letter: char) -> Option<usize> {
        let bar = self.bar.as_ref()?;
        bar.titles.iter().position(|&slot| {
            self.tree
                .slot(slot)
                .and_then(|slot| slot.widget.as_deref())
                .and_then(|widget| widget.as_any().downcast_ref::<BarTitle>())
                .and_then(BarTitle::mnemonic)
                == Some(letter)
        })
    }

    /// The keys the bar takes **before** anything else (AM12): Alt pressed and released alone,
    /// and Alt+letter on a title's mnemonic. `true` when the key was the bar's.
    pub(super) fn bar_key_first(
        &mut self,
        key: &Key,
        state: ButtonState,
        modifiers: Modifiers,
        list: &mut DisplayList,
    ) -> bool {
        // Alt alone: armed by its press with no other modifier, disarmed by any other key.
        let alt = *key == Key::Named(NamedKey::Alt);
        if state == ButtonState::Pressed {
            self.alt_alone = alt && !modifiers.primary && !modifiers.secondary && !modifiers.shift;
        } else if alt && std::mem::take(&mut self.alt_alone) && self.bar_usable() {
            let engaged = self
                .bar
                .as_ref()
                .is_some_and(|bar| bar.active.is_some() || bar.open.is_some());
            if engaged {
                self.leave_bar(list);
            } else {
                self.enter_bar(list);
            }
            return true;
        }
        // Alt+letter: the title with that mnemonic, before the focused widget (precision of AM4).
        if state == ButtonState::Pressed
            && modifiers.alt
            && !modifiers.primary
            && !modifiers.secondary
            && self.bar_usable()
            && let Some(letter) = letter_of(key)
            && let Some(title) = self.title_with(letter)
        {
            self.open_bar_menu(title, true, list);
            return true;
        }
        false
    }

    /// A key while the bar is active and no menu is open (AM12; see the module documentation).
    /// `true` when the bar took it (every key, while it is active).
    pub(super) fn bar_key(
        &mut self,
        key: &Key,
        state: ButtonState,
        list: &mut DisplayList,
    ) -> bool {
        let Some(active) = self.bar.as_ref().and_then(|bar| bar.active) else {
            return false;
        };
        if !self.menus.is_empty() {
            return false;
        }
        if state != ButtonState::Pressed {
            return true;
        }
        let count = self.title_count().max(1);
        match key {
            Key::Named(NamedKey::ArrowRight) => self.set_bar_active((active + 1) % count),
            Key::Named(NamedKey::ArrowLeft) => self.set_bar_active((active + count - 1) % count),
            Key::Named(
                NamedKey::ArrowDown | NamedKey::ArrowUp | NamedKey::Enter | NamedKey::Space,
            ) => self.open_bar_menu(active, true, list),
            Key::Named(NamedKey::Escape | NamedKey::Tab | NamedKey::Function(10)) => {
                self.leave_bar(list);
            }
            _ => {
                if let Some(title) = letter_of(key).and_then(|letter| self.title_with(letter)) {
                    self.open_bar_menu(title, true, list);
                }
            }
        }
        true
    }

    /// Moves the keyboard to title `title`.
    fn set_bar_active(&mut self, title: usize) {
        if let Some(bar) = self.bar.as_mut() {
            bar.active = Some(title);
        }
    }

    /// F10 left unhandled with no menu open (AM12): it reaches the bar. `true` when it did.
    pub(super) fn bar_f10(
        &mut self,
        key: &Key,
        modifiers: Modifiers,
        list: &mut DisplayList,
    ) -> bool {
        if !is_f10(key, modifiers) || !self.bar_usable() {
            return false;
        }
        self.enter_bar(list);
        true
    }

    /// The keys a menu of the bar adds to a menu's own (AM12): F10 leaves everything; Left and
    /// Right go to the neighbouring title (`step` −1 or +1); Escape on the first menu goes back to
    /// its title. `true` when the key was handled here.
    pub(super) fn bar_menu_key(
        &mut self,
        key: &Key,
        modifiers: Modifiers,
        list: &mut DisplayList,
    ) -> bool {
        if !self.bar_chain() {
            return false;
        }
        if is_f10(key, modifiers) {
            self.leave_bar(list);
            return true;
        }
        let Some(open) = self.bar.as_ref().and_then(|bar| bar.open) else {
            return false;
        };
        let count = self.title_count().max(1);
        match key {
            Key::Named(NamedKey::ArrowLeft) if self.menus.len() == 1 => {
                self.open_bar_menu((open + count - 1) % count, true, list);
                true
            }
            Key::Named(NamedKey::ArrowRight) if !self.highlighted_opens_submenu() => {
                self.open_bar_menu((open + 1) % count, true, list);
                true
            }
            Key::Named(NamedKey::Escape) if self.menus.len() == 1 => {
                self.close_menus_from(0, list, false);
                if let Some(bar) = self.bar.as_mut() {
                    bar.active = Some(open);
                    bar.keyboard = true;
                }
                true
            }
            _ => false,
        }
    }

    /// A press on `target` concerning the bar (AM12): on a title it opens that title's menu (or
    /// closes it, pressed again) and is consumed (`true`); anywhere else it leaves an active bar
    /// and goes on (`false`).
    pub(super) fn bar_press(&mut self, target: Option<usize>, list: &mut DisplayList) -> bool {
        self.alt_alone = false;
        if self.bar.is_none() {
            return false;
        }
        match target.and_then(|slot| self.bar_title_of(slot)) {
            Some(title) if self.bar.as_ref().and_then(|bar| bar.open) == Some(title) => {
                self.leave_bar(list);
                true
            }
            Some(title) => {
                self.open_bar_menu(title, false, list);
                true
            }
            None => {
                if self.bar.as_ref().is_some_and(|bar| bar.active.is_some())
                    && self.menus.is_empty()
                {
                    self.leave_bar(list);
                }
                false
            }
        }
    }

    /// The pointer moved while a menu of the bar is open (AM12): over another title, that title's
    /// menu opens. `true` when it did.
    pub(super) fn bar_pointer_moved(&mut self, list: &mut DisplayList) -> bool {
        let Some(open) = self.bar.as_ref().and_then(|bar| bar.open) else {
            return false;
        };
        let Some(title) = self.hovered.and_then(|slot| self.bar_title_of(slot)) else {
            return false;
        };
        if title == open {
            return false;
        }
        let keyboard = self.bar.as_ref().is_some_and(|bar| bar.keyboard);
        self.open_bar_menu(title, false, list);
        // Reached from the keyboard, the mnemonics stay underlined while the pointer moves on.
        if let Some(bar) = self.bar.as_mut() {
            bar.keyboard = keyboard;
        }
        true
    }

    /// An assistive technology clicked `slot`: a title opens its menu as Enter would (`true`).
    pub(super) fn bar_access_click(&mut self, slot: usize, list: &mut DisplayList) -> bool {
        let Some(title) = self.bar_title_of(slot) else {
            return false;
        };
        self.open_bar_menu(title, true, list);
        true
    }

    /// The title the accessibility focus is on while the bar is active with no menu open.
    pub(super) fn bar_access_focus(&self) -> Option<WidgetId> {
        let bar = self.bar.as_ref()?;
        if !self.menus.is_empty() {
            return None;
        }
        self.id_of(*bar.titles.get(bar.active?)?)
    }

    /// The window lost the focus, or the bar's menus all closed by a press outside: the bar is
    /// left (its menus are already closed).
    pub(super) fn bar_reset(&mut self) {
        self.alt_alone = false;
        if let Some(bar) = self.bar.as_mut() {
            bar.active = None;
            bar.open = None;
            bar.keyboard = false;
        }
    }

    /// The last menu closed: no title is open any more; the bar stays active only if the keyboard
    /// is still on it (Escape back to the title).
    pub(super) fn bar_menus_closed(&mut self) {
        if let Some(bar) = self.bar.as_mut() {
            bar.open = None;
            if bar.active.is_none() {
                bar.keyboard = false;
            }
        }
    }

    /// Brings the titles and the open menus' entries up to date with the bar's state and with
    /// whether the mnemonics are shown (see the module documentation): records only what changed.
    /// Called at the end of every event, before the frame.
    ///
    /// # Cost
    /// O(titles + open entries) flag tests; no allocation.
    pub(super) fn sync_menu_looks(&mut self) {
        let shown = self.mnemonics_underlined();
        if let Some(bar) = self.bar.as_ref() {
            let (active, open) = (bar.active, bar.open);
            for place in 0..bar.titles.len() {
                let Some(&slot) = self.bar.as_ref().and_then(|bar| bar.titles.get(place)) else {
                    continue;
                };
                let changed = self
                    .tree
                    .slot_mut(slot)
                    .and_then(|slot| slot.widget.as_deref_mut())
                    .and_then(|widget| widget.as_any_mut().downcast_mut::<BarTitle>())
                    .is_some_and(|title| {
                        title.set_state(active == Some(place), open == Some(place), shown)
                    });
                if changed && let Some(slot) = self.tree.slot_mut(slot) {
                    slot.needs_record = true;
                    slot.needs_access = true;
                }
            }
        }
        for menu in 0..self.menus.len() {
            let count = self
                .menus
                .get(menu)
                .and_then(|layer| layer.model.as_ref())
                .map_or(0, |model| model.entries.len());
            for place in 0..count {
                let Some(slot) = self
                    .menus
                    .get(menu)
                    .and_then(|layer| layer.model.as_ref())
                    .and_then(|model| model.entries.get(place).copied())
                else {
                    continue;
                };
                let changed = self
                    .tree
                    .slot_mut(slot)
                    .and_then(|slot| slot.widget.as_deref_mut())
                    .and_then(|widget| widget.as_any_mut().downcast_mut::<MenuEntry>())
                    .is_some_and(|entry| entry.set_show_mnemonic(shown));
                if changed && let Some(slot) = self.tree.slot_mut(slot) {
                    slot.needs_record = true;
                }
            }
        }
    }

    /// The item of the topmost menu with the mnemonic `letter` that can be chosen, after the
    /// highlighted one (so two items with one letter, which the catalogues' test forbids, would
    /// still both be reachable), or `None`.
    pub(super) fn item_with(&self, letter: char) -> Option<usize> {
        let model = self.menus.last()?.model.as_ref()?;
        let count = model.entries.len();
        let start = model.highlighted.map_or(0, |at| at + 1);
        (0..count)
            .map(|offset| (start + offset) % count)
            .find(|&place| {
                model
                    .entries
                    .get(place)
                    .and_then(|&slot| self.tree.slot(slot))
                    .and_then(|slot| slot.widget.as_deref())
                    .and_then(|widget| widget.as_any().downcast_ref::<MenuEntry>())
                    .is_some_and(|entry| entry.selectable() && entry.mnemonic() == Some(letter))
            })
    }

    /// `letter_of` for the module `menus`.
    pub(super) fn key_letter(key: &Key) -> Option<char> {
        letter_of(key)
    }

    /// Where the bar starts, for tests that click its titles: the first title's middle.
    #[must_use]
    pub fn menu_bar_title_point(&self, title: usize) -> Option<Point> {
        let slot = *self.bar.as_ref()?.titles.get(title)?;
        let rect = self.tree.slot(slot)?.rect;
        Some(Point::new(
            f32::midpoint(rect.x0, rect.x1),
            f32::midpoint(rect.y0, rect.y1),
        ))
    }
}

/// The lower case of `letter` when it is a single character (a mnemonic is compared in lower
/// case, ADR-0048 AM10).
fn lower(letter: char) -> char {
    let mut lowered = letter.to_lowercase();
    match (lowered.next(), lowered.next()) {
        (Some(single), None) => single,
        _ => letter,
    }
}
