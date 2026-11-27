// SPDX-License-Identifier: GPL-3.0-or-later

//! What an open dialog does to the interface (ADR-0050 DG1–DG2, SCALETTA F2.12a3a): its layer,
//! its focus, its closing. A child module of the interface's, like the menus, because a dialog
//! changes the routing itself.
//!
//! # A layer of the popup stack (DG1, C1)
//! A dialog is a subtree with a root of its own (`DialogRoot`, invisible), like a menu (F2.11c),
//! in the **same stack of layers**: it takes the next layer up, so a menu opened from inside it
//! (a field's context menu) goes above it, and a dialog opened from a dialog goes above the first.
//! When the last menu closes, the next menu's layer goes back to just above the topmost dialog,
//! never below it.
//!
//! # Modal (WA4, WB9)
//! The root is also pushed as a **modal layer**: the pointer reaches only the dialog (the root
//! consumes what falls outside the panel), Tab cycles inside it, the keys go to its focused
//! widget and bubble up to the panel (Enter, Escape). The **menu bar and the actions'
//! shortcuts** are off while a dialog is open, except the standard editing actions on a focused
//! field (Copy in a dialog's field still copies; ADR-0048 AM13): Ctrl+S must not save behind a
//! question.
//!
//! # Opening
//! The open menus close first (a dialog is never opened under a menu: choosing an item already
//! closes them; this is a guard) and the tooltip hides. The focus goes to the default button, or
//! the cancel button, or the first button (C4); closing gives it back to where it was (WA4).
//!
//! # Closing
//! [`Ui::close_dialog`] closes one by the ID [`Ui::open_dialog`] answered. A dialog's buttons, and
//! Enter and Escape on its panel, ask for it through `EventCx::close_dialog`: the request is kept
//! and carried out at the start of the next frame (every input ends with one), so the button's
//! message is already queued for the application when the dialog goes.
//!
//! # Cost
//! Opening allocates the dialog's widgets and texts once (well within the 50 ms of an input);
//! routing inside an open dialog allocates nothing (the tests check a key press).

use crate::draw::list::DisplayList;
use crate::theme::text::{FamilyToken, SizeToken, Weight};
use crate::theme::token::ColorToken;
use crate::widget::button::{Button, ButtonKind};
use crate::widget::dialog::{
    DialogPanel, DialogRoot, DialogSpec, LOGO_SIDE, LargeDialog, PanelSize,
};
use crate::widget::label::Label;
use crate::widget::layout::{Align, Constraints, Flex, LayoutProps, Length, Size, Space};
use crate::widget::logo::Logo;
use crate::widget::scroll::ScrollArea;
use crate::widget::tree::{Widget, WidgetId};

use super::{FIRST_MENU_LAYER, TOOLTIP_LAYER, Ui, UiError};

/// One open dialog (see the module documentation).
#[derive(Debug, Clone, Copy)]
pub(super) struct DialogLayer {
    /// The root's slot (`DialogRoot`), the root of the dialog's subtree.
    pub(super) root: usize,
    /// The panel's slot, whose ID the application holds.
    pub(super) panel: usize,
    /// Its display-list layer.
    pub(super) layer: u32,
    /// The action whose shortcut closes it, as Escape does (a large dialog's, F2.12a4a).
    pub(super) closed_by: Option<crate::ActionId>,
}

impl<M: Clone + std::fmt::Debug + 'static> Ui<M> {
    /// Opens a question or an error (ADR-0050 DG2, DG3) above everything, and answers the ID of
    /// its panel, for [`Ui::close_dialog`]. The buttons are placed in the order of
    /// [`Ui::button_order`]; each sends its message and closes the dialog; Enter presses the
    /// default button, Escape the cancel button (see `crate::widget::dialog`).
    ///
    /// # Errors
    /// [`UiError::UnknownWidget`] only if the interface's own tree is inconsistent (never
    /// expected).
    pub fn open_dialog(
        &mut self,
        spec: DialogSpec<M>,
        list: &mut DisplayList,
    ) -> Result<WidgetId, UiError> {
        let order = spec.display_order(self.button_order);
        let default = spec.default_button();
        let cancel = spec.cancel_button();
        let message_of = |index: Option<usize>| {
            index
                .and_then(|i| spec.buttons.get(i))
                .map(|button| button.message.clone())
        };
        let panel = DialogPanel::new(
            spec.kind,
            spec.title.clone(),
            spec.message.clone(),
            message_of(default),
            message_of(cancel),
        );
        let (root, panel) = self.push_dialog(Box::new(panel));
        // The texts, in a vertical scroll area, so a low window scrolls them and keeps the
        // buttons (C2).
        let scroll = self.tree.insert(
            Some(panel),
            Box::new(ScrollArea::vertical()),
            LayoutProps::default(),
        );
        let column = self.tree.insert(
            Some(scroll),
            Box::new(Flex::column(Space::M, Space::None)),
            LayoutProps::default().width(Length::Fill(1.0)),
        );
        let fill = LayoutProps::default().width(Length::Fill(1.0));
        // The logo first, centred above the title (the About dialog, ADR-0050 DG10).
        if let Some(label) = spec.logo {
            self.tree.insert(
                Some(column),
                Box::new(Logo::new(LOGO_SIDE, label)),
                LayoutProps::default().align(Align::Center, Align::Start),
            );
        }
        let title = Label::new(spec.title)
            .styled(FamilyToken::Ui, SizeToken::Subheading, Weight::SemiBold)
            .wrapping();
        self.tree.insert(Some(column), Box::new(title), fill);
        self.tree.insert(
            Some(column),
            Box::new(Label::new(spec.message).wrapping()),
            fill,
        );
        for detail in spec.details {
            let detail = Label::new(detail).color(ColorToken::TextMuted).wrapping();
            self.tree.insert(Some(column), Box::new(detail), fill);
        }
        // The buttons in their display order; the default one in the accent.
        let mut focus = None;
        // Each button taken once, by its index in the specification.
        let mut taken: Vec<Option<_>> = spec.buttons.into_iter().map(Some).collect();
        for &index in &order {
            let Some(button) = taken.get_mut(index).and_then(Option::take) else {
                continue;
            };
            let kind = if Some(index) == default {
                ButtonKind::Primary
            } else {
                ButtonKind::Normal
            };
            let widget = Button::new(button.text, button.message)
                .kind(kind)
                .closing_dialog();
            let slot = self
                .tree
                .insert(Some(panel), Box::new(widget), LayoutProps::default());
            // The default first, then the cancel button, then the first one drawn (C4).
            let rank = if Some(index) == default {
                0
            } else if Some(index) == cancel {
                1
            } else {
                2
            };
            if focus.is_none_or(|(best, _)| rank < best) {
                focus = Some((rank, slot));
            }
        }
        self.open_pushed_dialog(root, focus.map(|(_, slot)| slot), list);
        self.id_of(panel)
            .ok_or(UiError::UnknownWidget(WidgetId::placeholder()))
    }

    /// Opens a large dialog (the Preferences, ADR-0050 DG8–DG9, F2.12a4a) and answers the IDs
    /// of its panel (for [`Ui::close_dialog`]) and of its **content area**, an empty column that
    /// fills the room between the title and the *Close* button: the application adds its widgets
    /// there with [`Ui::add`], then calls [`Ui::focus_first`] to put the focus on the first one.
    /// The panel is [`LARGE_SIZE`](crate::DIALOG_LARGE_SIZE) within the window (see
    /// `crate::widget::dialog`); Escape, *Close* and the shortcut of `spec.closed_by` close it,
    /// each sending `spec.close_message`.
    ///
    /// # Errors
    /// As [`Ui::open_dialog`].
    pub fn open_large_dialog(
        &mut self,
        spec: LargeDialog<M>,
        list: &mut DisplayList,
    ) -> Result<(WidgetId, WidgetId), UiError> {
        let panel = DialogPanel::new(
            crate::DialogKind::Information,
            spec.title.clone(),
            String::new(),
            None,
            Some(spec.close_message.clone()),
        )
        .sized(PanelSize::Large);
        let (root, panel) = self.push_dialog(Box::new(panel));
        if let Some(layer) = self.dialogs.last_mut() {
            layer.closed_by = spec.closed_by;
        }
        let fill = LayoutProps::default()
            .width(Length::Fill(1.0))
            .height(Length::Fill(1.0));
        // The title above the content, as a question's (its heading style).
        let column = self.tree.insert(
            Some(panel),
            Box::new(Flex::column(Space::M, Space::None)),
            fill,
        );
        let title =
            Label::new(spec.title).styled(FamilyToken::Ui, SizeToken::Subheading, Weight::SemiBold);
        self.tree
            .insert(Some(column), Box::new(title), LayoutProps::default());
        let content = self.tree.insert(
            Some(column),
            Box::new(Flex::column(Space::M, Space::None)),
            fill,
        );
        let close = Button::new(spec.close_text, spec.close_message).closing_dialog();
        let close = self
            .tree
            .insert(Some(panel), Box::new(close), LayoutProps::default());
        self.open_pushed_dialog(root, Some(close), list);
        let panel = self
            .id_of(panel)
            .ok_or(UiError::UnknownWidget(WidgetId::placeholder()))?;
        let content = self
            .id_of(content)
            .ok_or(UiError::UnknownWidget(WidgetId::placeholder()))?;
        Ok((panel, content))
    }

    /// Opens a dialog whose panel is `panel`, with no content yet, and answers the panel's ID:
    /// the application adds the content with [`Ui::add`] (the Preferences, F2.12a4). The panel
    /// is centred in the window like a question's; the focus goes to its first focusable widget at
    /// the next [`Ui::frame`] the application asks for after adding it ([`Ui::focus_first`]).
    ///
    /// # Errors
    /// As [`Ui::open_dialog`].
    pub fn open_dialog_with(
        &mut self,
        panel: impl Widget<M>,
        list: &mut DisplayList,
    ) -> Result<WidgetId, UiError> {
        let (root, panel) = self.push_dialog(Box::new(panel));
        self.open_pushed_dialog(root, None, list);
        self.id_of(panel)
            .ok_or(UiError::UnknownWidget(WidgetId::placeholder()))
    }
}

impl<M: 'static> Ui<M> {
    /// Closes the dialog whose panel is `id` (or whose root is), with everything above it, and
    /// gives the focus back to where it was before it opened.
    ///
    /// # Errors
    /// [`UiError::UnknownWidget`] if `id` is not an open dialog.
    pub fn close_dialog(&mut self, id: WidgetId, list: &mut DisplayList) -> Result<(), UiError> {
        let slot = self.tree.slot_of(id).ok_or(UiError::UnknownWidget(id))?;
        let position = self
            .dialogs
            .iter()
            .position(|dialog| dialog.panel == slot || dialog.root == slot)
            .ok_or(UiError::UnknownWidget(id))?;
        self.close_dialogs_from(position, list);
        self.frame(list);
        Ok(())
    }

    /// The open dialogs' panel IDs, lowest first. The layers that block the interface while a
    /// file chooser is open ([`Ui::choose_file`]) or while the application is busy
    /// ([`Ui::begin_busy`]) are not among them: they have no panel to show.
    pub fn dialogs(&self) -> impl Iterator<Item = WidgetId> + '_ {
        self.dialogs
            .iter()
            .filter(|dialog| Some(dialog.root) != self.file_layer)
            .filter(|dialog| Some(dialog.root) != self.busy_layer)
            .filter_map(|dialog| self.id_of(dialog.panel))
    }

    /// The order of the dialogs' buttons (DG2): the application sets the system's once at
    /// start-up ([`system_button_order`](crate::system_button_order)); affirmative last until
    /// then.
    pub const fn set_button_order(&mut self, order: crate::ButtonOrder) {
        self.button_order = order;
    }

    /// The order of the dialogs' buttons in use.
    #[must_use]
    pub const fn button_order(&self) -> crate::ButtonOrder {
        self.button_order
    }

    /// Moves the focus to the first focusable widget of the innermost modal layer (or of the
    /// interface): for a dialog opened with [`Ui::open_dialog_with`] once its content is added.
    pub fn focus_first(&mut self, list: &mut DisplayList) {
        self.set_focus_slot(None);
        self.move_focus(true);
        self.frame(list);
    }

    /// The layer the next menu opens in when none is open: just above the topmost dialog, or the
    /// first menu layer (see the module documentation).
    pub(super) fn layer_floor(&self) -> u32 {
        self.dialogs
            .last()
            .map_or(FIRST_MENU_LAYER, |dialog| dialog.layer.saturating_add(1))
    }

    /// Carries out the closings the widgets asked for since the last frame (a dialog's button,
    /// Enter or Escape on its panel): each closes the innermost open dialog that contains the
    /// widget, if it is still open.
    pub(super) fn close_requested(&mut self, list: &mut DisplayList) {
        while let Some(index) = self.dialogs_to_close.pop() {
            if self.tree.slot(index).is_none() {
                continue;
            }
            let position = self
                .dialogs
                .iter()
                .rposition(|dialog| self.inside(index, dialog.root));
            if let Some(position) = position {
                self.close_dialogs_from(position, list);
            }
        }
    }

    /// Lays the open dialogs out: each root spans the whole window and centres its panel.
    /// O(open dialogs) when nothing changed (cached measures; arranging skips what did not move).
    pub(super) fn lay_out_dialogs(&mut self) {
        let window = Size::new(self.bounds.width(), self.bounds.height());
        for index in 0..self.dialogs.len() {
            if let Some(dialog) = self.dialogs.get(index) {
                let root = dialog.root;
                self.tree.measure(root, Constraints::tight(window));
                self.tree.arrange(root, self.bounds);
            }
        }
    }

    /// Records the open dialogs, lowest first, each in its layer.
    pub(super) fn record_dialogs(&mut self, list: &mut DisplayList) {
        for index in 0..self.dialogs.len() {
            if let Some(dialog) = self.dialogs.get(index) {
                let (root, layer) = (dialog.root, dialog.layer);
                self.record_from(root, layer, list);
            }
        }
    }

    /// Inserts a root with `panel` in it as a new layer, above the open menus' layers, and
    /// answers the two slots. Not yet modal: [`Ui::open_pushed_dialog`] does the rest.
    pub(super) fn push_dialog(&mut self, panel: Box<dyn Widget<M>>) -> (usize, usize) {
        let root = self
            .tree
            .insert(None, Box::new(DialogRoot), LayoutProps::default());
        let panel = self.tree.insert(Some(root), panel, LayoutProps::default());
        // The next layer up, as a menu takes it; never the tooltip's.
        let layer = self.next_layer.min(TOOLTIP_LAYER - 1);
        self.next_layer = self.next_layer.saturating_add(1);
        if let Some(slot) = self.tree.slot_mut(root) {
            slot.layer = layer;
        }
        self.dialogs.push(DialogLayer {
            root,
            panel,
            layer,
            closed_by: None,
        });
        (root, panel)
    }

    /// Makes the pushed dialog `root` modal: the menus close, the tooltip hides, the focus moves
    /// to `focus` (or to the first focusable widget inside), and a frame runs.
    pub(super) fn open_pushed_dialog(
        &mut self,
        root: usize,
        focus: Option<usize>,
        list: &mut DisplayList,
    ) {
        self.close_menus_from(0, list, true);
        self.hide_tooltip();
        self.tooltip.pending = None;
        self.modals.push((root, self.focused));
        match focus {
            Some(slot) => self.set_focus_slot(Some(slot)),
            None => self.focused_inside(root),
        }
        // The pointer may be over what the dialog now covers.
        self.set_hover(None);
        let hovered = self.hit(self.pointer);
        if self.captured.is_none() {
            self.set_hover(hovered);
        }
        // The window node lists the open dialogs (assistive technologies find them there).
        self.access_window = true;
        self.frame(list);
    }

    /// Closes the dialogs from `position` upwards, topmost first: the menus above them close too,
    /// each dialog's focus goes back, and its subtree and segments are removed.
    pub(super) fn close_dialogs_from(&mut self, position: usize, list: &mut DisplayList) {
        self.close_menus_from(0, list, true);
        while self.dialogs.len() > position {
            let Some(dialog) = self.dialogs.pop() else {
                break;
            };
            // Its modal layer, and the focus it saved.
            if let Some(at) = self
                .modals
                .iter()
                .rposition(|&(layer, _)| layer == dialog.root)
            {
                let (_, previous) = self.modals.remove(at);
                let previous = previous.filter(|&slot| {
                    self.tree.slot(slot).is_some() && !self.inside(slot, dialog.root)
                });
                self.set_focus_slot(previous);
            }
            self.remove_slot(dialog.root, list);
        }
        self.next_layer = self.layer_floor();
        self.access_window = true;
    }
}
