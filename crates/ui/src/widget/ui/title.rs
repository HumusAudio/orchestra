// SPDX-License-Identifier: GPL-3.0-or-later

//! The interface's side of our own title bar (F2.11b2, ADR-0049): adding it, telling it the
//! window's state, and handing what it asks of the window to the application. A child module of
//! the interface's, next to the menu bar's (`bar`). The bar itself is
//! [`TitleBar`](crate::widget::titlebar) and its buttons.
//!
//! # The window's state
//! The bar shows whether the window has the focus (the title's colour), is maximised (Restore
//! instead of Maximise) or is in full screen (no title, no buttons). The interface learns it from
//! [`InputEvent::Focus`] and [`InputEvent::WindowState`] and passes it to the bar and to each
//! button, recording only the ones whose look changed.
//!
//! [`InputEvent::Focus`]: crate::InputEvent::Focus
//! [`InputEvent::WindowState`]: crate::InputEvent::WindowState

use crate::WindowRequest;
use crate::widget::layout::{LayoutProps, Length};
use crate::widget::menu::MenuModel;
use crate::widget::titlebar::{TitleBar, TitleBarSpec, WindowButtonWidget};
use crate::widget::tree::WidgetId;
use crate::window::input::WindowState;

use super::{Ui, UiError};

/// What the interface keeps about our title bar (see the module documentation).
#[derive(Debug, Clone)]
pub(super) struct TitleBarState {
    /// The bar's slot.
    root: usize,
    /// Its window buttons' slots.
    buttons: Vec<usize>,
    /// The window has the keyboard focus (it starts focused: it opens in front).
    focused: bool,
    /// The window's state as last told.
    window: WindowState,
}

impl<M: 'static> Ui<M> {
    /// Adds our title bar (F2.11b2, ADR-0049 TB4, TB5) as the last child of `parent`, usually the
    /// first row of the window, with the menus of `model` in it (the menu bar of
    /// [`Ui::add_menu_bar`], compact) and the buttons of `spec`. Answers the bar's ID. An
    /// interface has one bar; a second call replaces the one it drives.
    ///
    /// Only where the system draws no frame ([`Decorations::Own`](crate::Decorations::Own));
    /// elsewhere the application adds the plain menu bar under the system's frame.
    ///
    /// # Errors
    /// [`UiError::UnknownWidget`] if `parent` does not exist.
    pub fn add_title_bar(
        &mut self,
        parent: WidgetId,
        model: &MenuModel,
        spec: &TitleBarSpec,
    ) -> Result<WidgetId, UiError> {
        let fill = LayoutProps::default().width(Length::Fill(1.0));
        let bar = self.add(
            parent,
            TitleBar::new(spec.title.clone(), spec.leading.len()),
            fill,
        )?;
        let mut buttons = Vec::with_capacity(spec.leading.len() + spec.trailing.len());
        // The children in the order `TitleBar::arrange` expects: leading buttons, the menu bar,
        // trailing buttons.
        for &kind in &spec.leading {
            let id = self.add(
                bar,
                WindowButtonWidget::new(kind, spec.names.clone()),
                LayoutProps::default(),
            )?;
            buttons.extend(self.tree.slot_of(id));
        }
        self.add_bar(bar, model, LayoutProps::default(), true)?;
        for &kind in &spec.trailing {
            let id = self.add(
                bar,
                WindowButtonWidget::new(kind, spec.names.clone()),
                LayoutProps::default(),
            )?;
            buttons.extend(self.tree.slot_of(id));
        }
        let root = self.tree.slot_of(bar).ok_or(UiError::UnknownWidget(bar))?;
        self.title_bar = Some(TitleBarState {
            root,
            buttons,
            focused: true,
            window: WindowState::default(),
        });
        Ok(bar)
    }

    /// Our title bar's ID, if one was added (tests, screenshots).
    #[must_use]
    pub fn title_bar(&self) -> Option<WidgetId> {
        self.title_bar
            .as_ref()
            .and_then(|state| self.id_of(state.root))
    }

    /// The window buttons of our title bar, in order (tests, screenshots); nothing without a bar.
    pub fn title_bar_buttons(&self) -> impl Iterator<Item = WidgetId> + '_ {
        self.title_bar
            .iter()
            .flat_map(|state| state.buttons.iter())
            .filter_map(|&slot| self.id_of(slot))
    }

    /// The next thing a widget asked of the window (F2.11b2, ADR-0049 TB6), oldest first, or
    /// `None`. An application built on the interface answers
    /// [`WindowApp::take_window_request`](crate::WindowApp::take_window_request) with it (and
    /// then with its own requests, Quit and Full screen).
    pub fn take_window_request(&mut self) -> Option<WindowRequest> {
        self.window_requests.pop_front()
    }

    /// The window gained or lost the focus: the bar and its buttons change colour (TB8).
    pub(super) fn title_bar_focus(&mut self, focused: bool) {
        if let Some(state) = self.title_bar.as_mut() {
            state.focused = focused;
        }
        self.sync_title_bar();
    }

    /// The window's state changed: Restore instead of Maximise, full screen (TB6, TB8).
    pub(super) fn title_bar_window(&mut self, window: WindowState) {
        if let Some(state) = self.title_bar.as_mut() {
            state.window = window;
        }
        self.sync_title_bar();
    }

    /// Hands the window's state to the bar and to each button, recording, laying out again or
    /// rebuilding the accessible node of only those that changed.
    ///
    /// # Cost
    /// O(buttons) (three at most); no allocation.
    fn sync_title_bar(&mut self) {
        let Some(state) = self.title_bar.as_ref() else {
            return;
        };
        let (root, focused, window) = (state.root, state.focused, state.window);
        let changed = self
            .tree
            .slot_mut(root)
            .and_then(|slot| slot.widget.as_deref_mut())
            .and_then(|widget| widget.as_any_mut().downcast_mut::<TitleBar>())
            .map(|bar| bar.set_window(focused, window.fullscreen));
        if let Some((record, layout)) = changed {
            self.mark(root, record, layout, false);
        }
        let count = self
            .title_bar
            .as_ref()
            .map_or(0, |state| state.buttons.len());
        for place in 0..count {
            let Some(slot) = self
                .title_bar
                .as_ref()
                .and_then(|state| state.buttons.get(place).copied())
            else {
                continue;
            };
            let changed = self
                .tree
                .slot_mut(slot)
                .and_then(|slot| slot.widget.as_deref_mut())
                .and_then(|widget| widget.as_any_mut().downcast_mut::<WindowButtonWidget>())
                .map(|button| button.set_window(window.maximized, focused, window.fullscreen));
            if let Some((record, layout, access)) = changed {
                self.mark(slot, record, layout, access);
            }
        }
    }

    /// Asks the tree to record `slot` again, lay it out again, or rebuild its accessible node.
    fn mark(&mut self, slot: usize, record: bool, layout: bool, access: bool) {
        if layout {
            self.tree.invalidate_layout(slot);
        }
        if let Some(entry) = self.tree.slot_mut(slot) {
            entry.needs_record |= record || layout;
            entry.needs_access |= access;
        }
    }
}
