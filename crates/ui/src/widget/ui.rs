// SPDX-License-Identifier: GPL-3.0-or-later

//! The interface of one window (ADR-0045, choices WB6–WB9, WB11): the widget tree plus the
//! routing state (hover, capture, focus, modal layers) and the application's message queue.
//!
//! # A frame
//! The window calls [`Ui::layout`] when its size changes and [`Ui::input`] for every input
//! event; the application calls [`Ui::frame`] after it changed widgets itself. Each of them ends
//! the same way: the widgets that need it are measured and arranged again (from the root, with
//! the clean subtrees answered from their caches, WA2), and the widgets that need it re-record
//! their segment (WA6), in tree order. The renderer then draws the damage (ADR-0039 D5).
//!
//! # Routing (ADR-0045 WA3, WB7–WB9)
//! - Pointer events go to the topmost widget under the pointer (the deepest in the tree, later
//!   children first), then bubble to its ancestors until one handles them. A widget that handles
//!   a press captures the pointer until the release: moves and the release go to it alone.
//! - Entering and leaving are generated when the widget under the pointer changes (while nothing
//!   captures the pointer).
//! - Keys go to the focused widget and bubble; an unhandled Tab moves the focus (WA4). Typed
//!   text (ADR-0047 TF5) goes the same way.
//! - The wheel goes to the widget under the pointer and bubbles.
//! - A modal layer (the last opened) blocks pointer events outside it and keeps the focus inside.
//! - A widget outside its ancestors' clip (a scroll area's viewport, F2.09d) cannot be hit: the
//!   pointer reaches it only where it can be seen.
//! - The pointer's shape (F2.09e, [`Ui::pointer_shape`]) is asked the same way: the capturing
//!   widget alone during a drag, otherwise the hovered widget and then its ancestors, up to the
//!   modal layer's root, until one answers.
//!
//! # Clipping and scrolling (F2.09d, choice WF1 of the owner: "A today, B only if measured")
//! Rectangles stay in logical pixels of the window (WB5). A scroll area moves its content by
//! arranging it again at the new offset ([`EventCx::request_arrange`]: no measuring), and the
//! recording walk carries each widget's **clip**, the intersection of its ancestors'
//! [`Widget::child_clip`]: every segment is recorded inside its clip, so a scrolled child draws
//! only in the viewport. A widget whose clip changed is recorded again even if it did not move.
//! A widget entirely outside its clip records an empty segment (its old pixels are damaged and
//! cleared, nothing is replayed for it).
//!
//! The rejected alternative (B) kept the content in coordinates of its own with a group offset
//! in the display list: no re-recording when scrolling, but a display list that is no longer flat
//! (ADR-0039 D1) and offsets in hit testing and damage. Both redraw the whole viewport at every
//! scroll step, which is the dominant cost, so B saves little; it stays open if the measures of
//! F2.09d ever ask for it.
//!
//! # Time and wake-ups (F2.10b2, a precision of ADR-0040 W2)
//! The interface has a time of its own, [`Ui::advance`]d by the window with the real clock at the
//! start of each batch of events and by the tests with made-up times; widgets read it
//! ([`EventCx::now`]) and never the clock. A widget may ask to be woken at a time
//! ([`EventCx::request_wakeup`], at most one pending wake-up per widget): [`Ui::next_wakeup`] is
//! the earliest of them, for the window to sleep until, and [`Ui::advance`] sends
//! [`WidgetEvent::WakeUp`] to the widgets whose time has come. Without wake-ups the window waits
//! for input, as before: an idle window costs nothing.
//!
//! # Overlays (F2.10b2, a precision of ADR-0045 WA6)
//! A widget may own a second segment, its overlay ([`Widget::has_overlay`]), recorded after its
//! main one and re-recorded alone on request ([`EventCx::request_overlay`]): the text field's
//! caret, so that a blink damages the caret and not the field.
//!
//! # Popups (F2.11c, ADR-0048 AM11 and AM14)
//! Menus and the tooltip are subtrees with roots of their own in the same arena, laid out at their
//! natural size next to an anchor ([`super::popup::place`], always inside the window) and recorded
//! in display-list layers above the main interface (a precision of ADR-0039 D1): each open menu
//! gets the next layer, the tooltip the highest. The tooltip is kept out of the accessibility
//! tree (it is not for assistive technologies, AM14); the open menus are children of the window
//! node since F2.11d2a.
//!
//! While a menu is open ([`Ui::open_menu`]):
//! - the pointer reaches the open menus (the topmost first) and the widgets that own them
//!   ([`Popup::owner`]), nothing else; a press anywhere else closes every menu, **is consumed**
//!   (it reaches nothing below, WB9 as AM11 decides) and the owners are told
//!   ([`WidgetEvent::MenuClosed`]);
//! - keys go to the topmost menu (its root, bubbling inside it) and never into the main
//!   interface; the focused widget keeps the focus (AM11: an Edit item acts on the field that had
//!   it), but typed text and the input method are off, so typing a menu's letter cannot start a
//!   composition in the field below;
//! - the window losing the focus closes every menu, as the three platforms do.
//!
//! The **tooltip** ([`Widget::tooltip`]) appears when the pointer has rested 500 ms on a control
//! that has one (the timer starts when the pointer comes over it), with no delay on the next such
//! control within 500 ms of the previous tooltip going away by a move; it goes away on any key or
//! button press and on the wheel (and that control's tooltip does not come back until the
//! pointer leaves it), when the pointer leaves the control for something else, or the window. The
//! pointer may move onto the tooltip without hiding it (WCAG 1.4.13, hoverable). A key that hides
//! it is not consumed (choice 4 of F2.11c): Escape hides the tooltip and still closes a dialog.
//! The tooltip's widget is created the first time one shows and kept afterwards, hidden, so
//! showing and hiding allocate nothing once it exists.
//!
//! # Actions and shortcuts (F2.11d1, ADR-0048 AM1–AM4, AM13)
//! Once the application hands in its registry ([`Ui::set_actions`]), a key press that reaches
//! the root unhandled (AM4: a widget always wins over a shortcut) is looked up in the **active
//! contexts**, met from the focused widget (or the open modal layer) up to the root, innermost
//! first ([`Widget::context`]), and in *Global* last (AM3). Each context is tried with the
//! press's logical shortcut, then with its fallback by position (AM5). When an action is found:
//! - the press is consumed, and so are its repeats (they run the action again only for an action
//!   marked *repeatable*) and its release;
//! - a standard editing action (Cut, Copy, Paste, Select all, Undo, Redo) asks the focused widget
//!   first ([`Widget::edit_state`], AM13): a text field carries it out on its own text and local
//!   undo, or refuses it (Undo with an empty local history), and in both cases it never reaches
//!   the document;
//! - otherwise a disabled action does nothing (AM2), and an enabled one becomes the application's
//!   message ([`Ui::set_actions`]'s function), queued with the widgets' messages in the order of
//!   the input.
//!
//! With a menu open the keys are the menu's (AM11) and no shortcut is looked up.
//!
//! # Menus built from a model, context menus (F2.11d2a, ADR-0048 AM9, AM12)
//! What a drawn menu does (the highlight, its keys, the pointer, submenus, choosing, context
//! menus, items kept up to date with their actions) is in the child module `menus`, whose
//! documentation describes it: [`Ui::open_menu_model`] and [`Widget::context_menu`].
//!
//! # Allocation
//! Routing an event allocates nothing (hit testing recurses on the tree, bubbling follows parent
//! links, Tab order walks with a reused stack). Messages go into a queue that keeps its capacity.

use std::time::Instant;

use crate::action::registry::{ActionId, ActionState, Context, EditRole, Registry};
use crate::action::shortcut::{ShortcutKey, candidates};
use crate::draw::geometry::{Point, Rect};
use crate::draw::list::{DisplayList, Recorder, SegmentId};
use crate::platform::clipboard::{ClipboardSource, MemoryClipboard};
use orchestra_text::Faces;

use crate::theme::token::{Shape, Theme};
use crate::window::input::{
    ButtonState, InputEvent, Key, Modifiers, NamedKey, PhysicalKey, PointerButton,
};
use crate::window::pointer::PointerShape;

use super::access::{
    AccessAction, AccessActionKind, AccessInfo, AccessUpdate, WINDOW_NODE, empty_update,
    push_widget_nodes, window_node,
};
use super::layout::{Constraints, LayoutProps, Size};
use super::popup::{self, Popup, PopupSide};

mod menus;

use super::tooltip::Tooltip;
use super::tree::{
    EventCx, Handled, RecordCx, WakeRequest, Widget, WidgetEvent, WidgetId, WidgetTree,
};
use menus::{MenuChain, ModelMenu, SubmenuTimer};

/// Why an operation on the interface was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiError {
    /// No widget has this ID (it was removed, or never existed).
    UnknownWidget(WidgetId),
    /// The root cannot be removed.
    RemoveRoot,
}

impl std::fmt::Display for UiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownWidget(id) => write!(f, "no widget has the ID {}", id.get()),
            Self::RemoveRoot => f.write_str("the root widget cannot be removed"),
        }
    }
}

impl std::error::Error for UiError {}

/// The interface of one window: widgets, routing state, messages (WB11).
#[derive(Debug)]
pub struct Ui<M> {
    /// The widgets.
    tree: WidgetTree<M>,
    /// The window's size, logical pixels.
    bounds: Rect,
    /// Messages emitted by widgets, waiting for the application (WA7).
    messages: Vec<M>,
    /// The widget under the pointer (hovered), if any.
    hovered: Option<usize>,
    /// The widget that captured the pointer (between a handled press and its release).
    captured: Option<usize>,
    /// The widget with the keyboard focus.
    focused: Option<usize>,
    /// Open modal layers, innermost last: (the layer's root, the focus before it opened).
    modals: Vec<(usize, Option<usize>)>,
    /// Where the pointer is.
    pointer: Point,
    /// The modifiers held, from the window's modifier and key events (ADR-0046 V3), given to
    /// every event's context.
    modifiers: Modifiers,
    /// A reused stack for walking the tree (Tab order).
    stack: Vec<usize>,
    /// A reused stack for the recording walk: each slot with the clip its parent gives it
    /// (F2.09d).
    walk: Vec<(usize, Option<Rect>)>,
    /// Widgets that asked to be arranged again in their rectangle (scrolled, divider moved),
    /// handled at the next frame (F2.09d). Reused.
    rearrange: Vec<usize>,
    /// IDs removed by the last removal, to drop their segments (reused).
    removed: Vec<WidgetId>,
    /// The theme's shape tokens, for recording (WC8).
    shape: Shape,
    /// The window's title, the name of the accessibility tree's window node (WC4).
    title: String,
    /// `false` until the whole accessibility tree has been handed out (and again when an
    /// assistive technology asks for it from the start).
    access_sent: bool,
    /// The interface's time: the last one given to [`Ui::advance`] (never going back), or the
    /// moment the interface was created (see the module documentation).
    now: Instant,
    /// The clipboard widgets copy to and paste from (ADR-0047 TF12): one in memory until the
    /// application hands in the system's ([`Ui::set_clipboard`]).
    clipboard: Box<dyn ClipboardSource>,
    /// The pending wake-ups, one at most per widget, in no order. By ID, not by slot: a slot
    /// freed and reused must not inherit a wake-up. A handful at most (a blinking caret), so a
    /// linear scan beats any ordered structure.
    wakeups: Vec<(WidgetId, Instant)>,
    /// The widgets whose wake-up came, collected by [`Ui::advance`] before they are called
    /// (reused).
    due: Vec<WidgetId>,
    /// How many times a widget asked to end the input method's composition
    /// ([`EventCx::reset_ime`]), wrapping: the window compares it with the count it last saw
    /// ([`TextInput::reset`]).
    ime_resets: u64,
    /// The open menus, lowest first (F2.11c, ADR-0048 AM11; see the module documentation).
    menus: Vec<MenuLayer>,
    /// The display-list layer the next menu opens in: one above the last, back to the first when
    /// none is open.
    next_layer: u32,
    /// The tooltip's state (ADR-0048 AM14).
    tooltip: TooltipState,
    /// A press closed the menus and was consumed: its release is consumed too, so the widget
    /// under it does not get a release without a press.
    swallow_release: bool,
    /// The registry of actions and how an action becomes a message, once the application handed
    /// them in ([`Ui::set_actions`]; ADR-0048 AM1, AM4).
    actions: Option<Actions<M>>,
    /// The key of the last press that matched an action: its release is consumed too (AM4).
    /// Only the key, compared without the modifiers, which may be released first.
    action_key: Option<ShortcutKey>,
    /// Whether the open chain of menus is armed, and where it opened (F2.11d2a, see the module
    /// `menus`).
    chain: MenuChain,
    /// A submenu to open or close after the pointer rests (F2.11d2a, `menus::SUBMENU_DELAY`).
    submenu_timer: Option<SubmenuTimer>,
    /// The actions marked as changed by the registry at this frame (reused, F2.11d2a).
    changed_actions: Vec<ActionId>,
    /// The window's accessibility node must be sent again: the open menus, its children after
    /// the root, changed (F2.11d2a).
    access_window: bool,
}

/// The registry of actions and the application's message for running one (F2.11d1).
#[derive(Debug)]
struct Actions<M> {
    /// The registry.
    registry: Registry,
    /// The message that tells the application to run an action.
    message: fn(ActionId) -> M,
}

/// The most contexts collected for one key, *Global* included (AM3): views nest a few levels at
/// most (a notation view inside the window); deeper ones are ignored, which keeps the collection
/// in a fixed array, without allocating.
const MAX_CONTEXTS: usize = 8;

/// One open menu (F2.11c).
#[derive(Debug, Clone)]
struct MenuLayer {
    /// Its root's slot.
    root: usize,
    /// The slot of the widget it belongs to ([`Popup::owner`]), if any.
    owner: Option<usize>,
    /// Where it opens.
    popup: Popup,
    /// What the interface knows of a menu built from a model ([`Ui::open_menu_model`],
    /// F2.11d2a); `None` for a menu whose widgets the application added itself
    /// ([`Ui::open_menu`]).
    model: Option<ModelMenu>,
}

/// What the interface knows about the tooltip (ADR-0048 AM14; see the module documentation).
/// Slots, not IDs, like the hover: they are dropped when their widget is removed.
#[derive(Debug, Default)]
struct TooltipState {
    /// The tooltip widget's slot, once one has been shown; kept, hidden, afterwards.
    root: Option<usize>,
    /// The control whose tooltip is shown, while one is.
    shown_for: Option<usize>,
    /// Where the shown tooltip opened ([`popup::tooltip_anchor`] at the pointer then).
    anchor: Rect,
    /// A control the pointer is resting on, and when its tooltip is due.
    pending: Option<(usize, Instant)>,
    /// Until when the next tooltip shows at once: 500 ms after one went away by a move.
    warm_until: Option<Instant>,
    /// A control whose tooltip a press, a key or the wheel dismissed: it does not come back until
    /// the pointer is over something else.
    suppressed: Option<usize>,
}

/// How long the pointer must rest on a control before its tooltip shows (ADR-0048 AM14): GTK's
/// default and close to Windows' (`SPI_GETMOUSEHOVERTIME`, 400 ms).
const TOOLTIP_DELAY: std::time::Duration = std::time::Duration::from_millis(500);

/// For how long after a tooltip went away by a move the next one shows at once (AM14).
const TOOLTIP_WARM: std::time::Duration = std::time::Duration::from_millis(500);

/// The display-list layer of the first menu: one above the main interface's 0.
const FIRST_MENU_LAYER: u32 = 1;

/// The display-list layer of the tooltip: above every menu.
const TOOLTIP_LAYER: u32 = u32::MAX;

/// What the window must do with the system's input method (ADR-0047 TF5, TF6; F2.10c2): the
/// answer of [`Ui::text_input`] while a widget that takes text has the focus.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextInput {
    /// Where the text being typed is, logical pixels of the window: the caret, or the
    /// composition ([`Widget::ime_area`]). The input method opens its candidates beside it.
    pub area: Rect,
    /// A count that changes each time a widget asks to end the composition in progress
    /// ([`EventCx::reset_ime`]). Only a change has a meaning: the window resets the input
    /// method when the count differs from the one it last saw.
    pub reset: u64,
}

/// The root slot (always 0).
const ROOT: usize = 0;

impl<M: 'static> Ui<M> {
    /// An interface whose root is `root` (usually a container), laid out with `theme`'s density
    /// and text tokens, its texts laid out with `faces` (the application's copy, ADR-0044 TB2).
    pub fn new(root: impl Widget<M>, theme: &Theme, faces: Faces) -> Self {
        Self {
            tree: WidgetTree::new(Box::new(root), theme.spacing(), faces, theme.typography()),
            shape: theme.shape(),
            title: String::new(),
            access_sent: false,
            now: Instant::now(),
            clipboard: Box::new(MemoryClipboard::default()),
            wakeups: Vec::new(),
            due: Vec::new(),
            ime_resets: 0,
            menus: Vec::new(),
            next_layer: FIRST_MENU_LAYER,
            tooltip: TooltipState::default(),
            swallow_release: false,
            actions: None,
            action_key: None,
            chain: MenuChain::default(),
            submenu_timer: None,
            changed_actions: Vec::new(),
            access_window: false,
            bounds: Rect::EMPTY,
            messages: Vec::new(),
            hovered: None,
            captured: None,
            focused: None,
            modals: Vec::new(),
            pointer: Point::default(),
            modifiers: Modifiers::default(),
            stack: Vec::new(),
            walk: Vec::new(),
            rearrange: Vec::new(),
            removed: Vec::new(),
        }
    }

    /// Gives the interface the clipboard its widgets copy to and paste from (ADR-0047 TF12): the
    /// system's, which the window hands to the application once
    /// ([`WindowApp::attach_clipboard`](crate::WindowApp::attach_clipboard)). Until then the
    /// interface has one in memory ([`MemoryClipboard`]), which works inside the program only.
    pub fn set_clipboard(&mut self, clipboard: Box<dyn ClipboardSource>) {
        self.clipboard = clipboard;
    }

    /// The interface's clipboard: for an application's own Copy and Paste commands, and for
    /// tests, which read what a widget copied and set what it will paste.
    pub fn clipboard(&mut self) -> &mut dyn ClipboardSource {
        self.clipboard.as_mut()
    }

    /// The root widget.
    #[must_use]
    pub fn root(&self) -> WidgetId {
        self.id_of(ROOT).unwrap_or_else(unreachable_root)
    }

    /// The ID of the widget in `slot`.
    fn id_of(&self, slot: usize) -> Option<WidgetId> {
        self.tree.slot(slot).map(|slot| slot.id)
    }

    /// Adds `widget` as the last child of `parent`, laid out by `props`.
    ///
    /// # Errors
    /// [`UiError::UnknownWidget`] if `parent` does not exist.
    pub fn add(
        &mut self,
        parent: WidgetId,
        widget: impl Widget<M>,
        props: LayoutProps,
    ) -> Result<WidgetId, UiError> {
        let parent = self
            .tree
            .slot_of(parent)
            .ok_or(UiError::UnknownWidget(parent))?;
        let slot = self.tree.insert(Some(parent), Box::new(widget), props);
        self.id_of(slot)
            .ok_or(UiError::UnknownWidget(WidgetId::placeholder()))
    }

    /// Removes `id` and its subtree, and their segments from `list`. The focus, the hover and the
    /// capture are dropped if they were inside.
    ///
    /// # Errors
    /// [`UiError::UnknownWidget`], or [`UiError::RemoveRoot`] for the root.
    pub fn remove(&mut self, id: WidgetId, list: &mut DisplayList) -> Result<(), UiError> {
        let slot = self.tree.slot_of(id).ok_or(UiError::UnknownWidget(id))?;
        if slot == ROOT {
            return Err(UiError::RemoveRoot);
        }
        self.remove_slot(slot, list);
        Ok(())
    }

    /// [`Ui::remove`] for a slot known to exist and not to be the root.
    fn remove_slot(&mut self, slot: usize, list: &mut DisplayList) {
        self.removed.clear();
        let mut removed = std::mem::take(&mut self.removed);
        self.tree.remove(slot, &mut removed);
        for gone in &removed {
            list.remove(SegmentId(gone.get()));
            // An overlay segment exists only for widgets with an overlay; removing an unknown id
            // is one hash lookup.
            list.remove(gone.overlay_segment());
        }
        self.wakeups.retain(|(id, _)| !removed.contains(id));
        self.removed = removed;
        // Routing state pointing into the removed subtree is dropped.
        let alive =
            |slot: Option<usize>, tree: &WidgetTree<M>| slot.filter(|&s| tree.slot(s).is_some());
        self.hovered = alive(self.hovered, &self.tree);
        self.captured = alive(self.captured, &self.tree);
        self.focused = alive(self.focused, &self.tree);
        self.modals
            .retain(|&(layer, _)| self.tree.slot(layer).is_some());
        // A removed menu is no longer open; a menu whose owner went keeps open without one.
        let tree = &self.tree;
        self.menus.retain(|menu| tree.slot(menu.root).is_some());
        for menu in &mut self.menus {
            menu.owner = alive(menu.owner, tree);
        }
        if self.menus.is_empty() {
            self.next_layer = FIRST_MENU_LAYER;
        }
        // The tooltip forgets the controls that went; one shown for a removed control is hidden.
        self.tooltip.root = alive(self.tooltip.root, tree);
        self.tooltip.suppressed = alive(self.tooltip.suppressed, tree);
        self.tooltip.pending = self
            .tooltip
            .pending
            .filter(|&(slot, _)| tree.slot(slot).is_some());
        if self.tooltip.shown_for.is_some() && alive(self.tooltip.shown_for, tree).is_none() {
            self.hide_tooltip();
        }
        // Slots freed by the removal may be reused by the next insertion: forget their requests.
        let tree = &self.tree;
        self.rearrange.retain(|&slot| tree.slot(slot).is_some());
    }

    /// The widget `id` as its concrete type, to read its state.
    #[must_use]
    pub fn widget<T: Widget<M>>(&self, id: WidgetId) -> Option<&T> {
        let slot = self.tree.slot_of(id)?;
        self.tree
            .slot(slot)?
            .widget
            .as_deref()?
            .as_any()
            .downcast_ref::<T>()
    }

    /// The widget `id` as its concrete type, to change it. The caller then asks for what the
    /// change needs: [`Ui::request_layout`] or [`Ui::request_record`].
    pub fn widget_mut<T: Widget<M>>(&mut self, id: WidgetId) -> Option<&mut T> {
        let slot = self.tree.slot_of(id)?;
        self.tree
            .slot_mut(slot)?
            .widget
            .as_deref_mut()?
            .as_any_mut()
            .downcast_mut::<T>()
    }

    /// Asks for `id` to be laid out (and recorded) again at the next frame.
    pub fn request_layout(&mut self, id: WidgetId) {
        if let Some(slot) = self.tree.slot_of(id) {
            self.tree.invalidate_layout(slot);
            if let Some(slot) = self.tree.slot_mut(slot) {
                slot.needs_record = true;
            }
        }
    }

    /// Asks for `id` to be recorded again at the next frame.
    pub fn request_record(&mut self, id: WidgetId) {
        if let Some(slot) = self
            .tree
            .slot_of(id)
            .and_then(|slot| self.tree.slot_mut(slot))
        {
            slot.needs_record = true;
        }
    }

    /// The rectangle of `id` from the last layout.
    #[must_use]
    pub fn rect(&self, id: WidgetId) -> Option<Rect> {
        let slot = self.tree.slot_of(id)?;
        self.tree.slot(slot).map(|slot| slot.rect)
    }

    /// The widget with the keyboard focus.
    #[must_use]
    pub fn focused(&self) -> Option<WidgetId> {
        self.focused.and_then(|slot| self.id_of(slot))
    }

    /// The widget under the pointer.
    #[must_use]
    pub fn hovered(&self) -> Option<WidgetId> {
        self.hovered.and_then(|slot| self.id_of(slot))
    }

    /// The shape the pointer should have now (F2.09e, a precision of ADR-0045 WA3), for the
    /// window to show ([`WindowApp::pointer_shape`](crate::WindowApp)).
    ///
    /// # Who decides
    /// - During a capture (a drag), the capturing widget alone, at the pointer's last position,
    ///   even outside its rectangle: dragging a divider past its limit keeps the resize shape.
    ///   Its `None` means the arrow; the widgets under the pointer are not asked, because they
    ///   will not receive the release.
    /// - Otherwise the hovered widget, then its parent, and so on, until one answers (the
    ///   bubbling of WB7). The walk stops at the innermost modal layer's root: what lies behind
    ///   a dialog has no say. Over the area a modal layer blocks there is no hovered widget
    ///   (hit testing stays inside the layer, WB9), so the answer is the arrow.
    /// - When the pointer is outside the window, nothing is hovered: the arrow (the system owns
    ///   the pointer there anyway).
    ///
    /// # Why it is asked, not stored
    /// The answer depends on the pointer's position inside a widget (the divider is a band inside
    /// the splitter, not a child) and on state that changes with layout (a divider moved by the
    /// keyboard leaves the pointer over a pane). Asking at the moment, with the current
    /// rectangles, is always right; a stored shape would need every widget to tell the interface
    /// when it changes.
    ///
    /// # Cost
    /// O(depth of the tree) calls to [`Widget::pointer_shape`], each a rectangle test in the
    /// widgets we have; no allocation. The window asks once per event and touches the system's
    /// pointer only when the answer changes.
    #[must_use]
    pub fn pointer_shape(&self) -> PointerShape {
        // Asks the widget in `index` alone.
        let ask = |index: usize| {
            self.tree
                .slot(index)
                .and_then(|slot| slot.widget.as_deref())
                .and_then(|widget| widget.pointer_shape(self.pointer))
        };
        if let Some(captured) = self.captured {
            return ask(captured).unwrap_or_default();
        }
        let layer = self.modals.last().map(|&(layer, _)| layer);
        let mut current = self.hovered;
        while let Some(index) = current {
            if let Some(shape) = ask(index) {
                return shape;
            }
            if Some(index) == layer {
                break;
            }
            current = self.tree.slot(index).and_then(|slot| slot.parent);
        }
        PointerShape::Default
    }

    /// What the window must do with the system's input method now (ADR-0047 TF5, TF6; F2.10c2):
    /// `Some` while the focused widget [accepts text](Widget::accepts_text), with where its
    /// caret is; `None` otherwise, and the input method is then off.
    ///
    /// # Why the window asks, rather than being told
    /// The same pattern as the pointer's shape ([`Ui::pointer_shape`]): the window asks after
    /// every event and calls the system only when the answer changed, so the interface never
    /// has to track who must be told what. It replaces the `Ui::wants_text_input` named in
    /// ADR-0047 TF5, adding the caret's area and the reset requests to the same answer
    /// (precision written in F2.10c2).
    ///
    /// The focused widget is asked even inside an open modal layer: focus never leaves the layer
    /// (WB9), so the focused widget is always one the user can type into.
    ///
    /// # Cost
    /// Two calls to the focused widget, each answering from stored state; no allocation.
    #[must_use]
    pub fn text_input(&self) -> Option<TextInput> {
        // While a menu is open the keys are the menu's (ADR-0048 AM11): the input method is off,
        // so a menu's letter cannot start a composition in the field that keeps the focus.
        if !self.menus.is_empty() {
            return None;
        }
        let slot = self.tree.slot(self.focused?)?;
        let widget = slot.widget.as_deref()?;
        widget.accepts_text().then(|| TextInput {
            area: widget.ime_area().unwrap_or(slot.rect),
            reset: self.ime_resets,
        })
    }

    /// Takes the messages the widgets emitted since the last call (WA7), keeping the queue's
    /// capacity.
    pub fn take_messages(&mut self) -> std::vec::Drain<'_, M> {
        self.messages.drain(..)
    }

    /// Uses `theme`'s density from now on: everything is laid out again (WB10).
    ///
    /// A change of density or of text tokens lays everything out again; a change of shape tokens
    /// records everything again; colours need nothing here (the renderer resolves them, ADR-0039
    /// D3).
    pub fn set_theme(&mut self, theme: &Theme) {
        let spacing = theme.spacing();
        let typography = theme.typography();
        if spacing != self.tree.spacing || typography != self.tree.typography {
            if typography != self.tree.typography {
                self.tree.text_generation = self.tree.text_generation.wrapping_add(1);
            }
            self.tree.spacing = spacing;
            self.tree.typography = typography;
            self.invalidate_all();
        }
        if theme.shape() != self.shape {
            self.shape = theme.shape();
            for slot in self.tree.slots.iter_mut().flatten() {
                slot.needs_record = true;
            }
        }
    }

    /// Names the window for assistive technologies (its title, already localised).
    pub fn set_title(&mut self, title: &str) {
        if title != self.title {
            title.clone_into(&mut self.title);
            // The window node is rebuilt with every full tree; a changed title needs one.
            self.access_sent = false;
        }
    }

    /// Marks every widget for layout and recording.
    fn invalidate_all(&mut self) {
        for slot in self.tree.slots.iter_mut().flatten() {
            slot.needs_layout = true;
            slot.needs_record = true;
        }
    }

    /// Lays the interface out for a window of `bounds` and records it (the window calls it on
    /// every size or scale change).
    pub fn layout(&mut self, bounds: Rect, list: &mut DisplayList) {
        if bounds != self.bounds {
            self.bounds = bounds;
            self.tree.invalidate_layout(ROOT);
        }
        self.frame(list);
    }

    /// Lays out what needs it and records what needs it (see the module documentation).
    ///
    /// # Order
    /// 1. The root's layout, if anything below it changed size (measure and arrange, WA2).
    /// 2. The widgets that asked only to be arranged again (scroll areas that scrolled, splitters
    ///    whose divider moved, F2.09d), in their current rectangles. After step 1, so they use
    ///    the rectangles of this frame's layout.
    /// 3. The recording walk, in tree order, carrying each widget's clip.
    ///
    /// # Complexity
    /// Measure and arrange visit the invalidated paths (the rest answer from their caches);
    /// re-arranging is O(descendants that moved); recording walks the tree once, O(widgets),
    /// recording only the marked ones (and those whose clip changed).
    pub fn frame(&mut self, list: &mut DisplayList) {
        // The open menus' items follow their actions' states first (F2.11d2a): a changed text
        // lays an entry out again in this same frame.
        self.refresh_menus();
        if self.tree.slot(ROOT).is_some_and(|root| root.needs_layout) {
            let size = Size::new(self.bounds.width(), self.bounds.height());
            self.tree.measure(ROOT, Constraints::tight(size));
            self.tree.arrange(ROOT, self.bounds);
        }
        // Drained in order of request; a slot asked twice is arranged twice, which is harmless
        // (the second time nothing moves) and rarer than the cost of deduplicating.
        while let Some(index) = self.rearrange.pop() {
            self.tree.rearrange(index);
        }
        self.lay_out_popups();
        self.record(list);
    }

    /// Lays the open popups out at their natural size and places them (F2.11c, ADR-0048 AM11):
    /// each is measured within the window (from its cache when nothing changed) and arranged at
    /// the rectangle [`popup::place`] gives, which arranging skips when it did not move.
    ///
    /// # Complexity
    /// O(open popups) when nothing changed (one cached measure and one skipped arrange each),
    /// plus the layout of what did change.
    fn lay_out_popups(&mut self) {
        for index in 0..self.menus.len() {
            if let Some(menu) = self.menus.get(index) {
                let (root, popup) = (menu.root, menu.popup);
                self.place_popup(root, popup.anchor, popup.side);
            }
        }
        if let (Some(root), Some(_)) = (self.tooltip.root, self.tooltip.shown_for) {
            self.place_popup(root, self.tooltip.anchor, PopupSide::Below);
        }
    }

    /// Measures the popup whose root is `root` within the window and arranges it next to
    /// `anchor`, inside the window.
    fn place_popup(&mut self, root: usize, anchor: Rect, side: PopupSide) {
        let window = Size::new(self.bounds.width(), self.bounds.height());
        let size = self.tree.measure(root, Constraints::loose(window));
        let rect = popup::place(anchor, size, side, self.bounds);
        self.tree.arrange(root, rect);
    }

    /// The recording walk (WA6, F2.09d): pre-order, children in order, so a segment recorded for
    /// the first time lands above its parent's. Each slot receives the clip its ancestors give
    /// it; a slot whose clip changed is recorded again even if nothing else changed (the part of
    /// it that can be seen did).
    ///
    /// The main interface is walked first, in layer 0, then each open menu in its layer and the
    /// tooltip in the highest (F2.11c): the display list keeps each layer above the lower ones
    /// whatever the order segments are first recorded in (a precision of ADR-0039 D1).
    fn record(&mut self, list: &mut DisplayList) {
        self.record_from(ROOT, 0, list);
        for index in 0..self.menus.len() {
            if let Some(menu) = self.menus.get(index) {
                let root = menu.root;
                let layer = self.tree.slot(root).map_or(0, |slot| slot.layer);
                self.record_from(root, layer, list);
            }
        }
        if let Some(root) = self.tooltip.root {
            self.record_from(root, TOOLTIP_LAYER, list);
        }
        list.set_layer(0);
    }

    /// The recording walk below `root`, whose new segments go into `layer`.
    fn record_from(&mut self, root: usize, layer: u32, list: &mut DisplayList) {
        list.set_layer(layer);
        let spacing = self.tree.spacing;
        let shape = self.shape;
        let focused = self.focused;
        self.walk.clear();
        self.walk.push((root, None));
        while let Some((index, clip)) = self.walk.pop() {
            let Some(slot) = self.tree.slot_mut(index) else {
                continue;
            };
            if slot.clip != clip {
                slot.clip = clip;
                slot.needs_record = true;
            }
            if !slot.children.is_empty() {
                // The clip of the children: this one's, narrowed by what the widget clips to.
                // Asked only of widgets with children (a leaf's answer would serve nobody): the
                // walk visits every widget at every frame, and most are leaves (measured in
                // F2.09d: asking all 241 widgets of the core benchmark's panel cost a click
                // 18 000 instructions, +29%).
                let own = slot.widget.as_deref().and_then(Widget::child_clip);
                let inner = match (clip, own) {
                    (Some(outer), Some(own)) => Some(outer.intersect(&own)),
                    (outer, None) => outer,
                    (None, own) => own,
                };
                // Children pushed in reverse so they pop in order.
                self.walk
                    .extend(slot.children.iter().rev().map(|&child| (child, inner)));
            }
            // The main segment and the overlay together, or the overlay alone (a blink).
            let main = slot.needs_record;
            if !main && !slot.needs_overlay {
                continue;
            }
            slot.needs_record = false;
            slot.needs_overlay = false;
            if main {
                // A widget that looks different (or moved) may describe itself differently: its
                // accessibility node is sent again (WC3). A new overlay alone changes nothing an
                // assistive technology sees.
                slot.needs_access = true;
            }
            let cx = RecordCx {
                rect: slot.rect,
                focused: focused == Some(index),
                spacing,
                shape,
            };
            let Some(widget) = slot.widget.as_deref() else {
                continue;
            };
            let reach = slot.rect.inflate(DRAW_MARGIN);
            if main {
                record_clipped(list, SegmentId(slot.id.get()), clip, reach, |recorder| {
                    widget.record(&cx, recorder);
                });
            }
            // Asked only of a widget being recorded, so the many widgets without an overlay pay
            // one call per recording, never one per frame.
            if widget.has_overlay() {
                record_clipped(list, slot.id.overlay_segment(), clip, reach, |recorder| {
                    widget.record_overlay(&cx, recorder);
                });
            }
        }
    }

    /// The interface's time ([`Ui::advance`]).
    #[must_use]
    pub const fn now(&self) -> Instant {
        self.now
    }

    /// Moves the interface's time to `now` (it never goes back: an earlier `now` is ignored) and
    /// wakes the widgets whose wake-up has come ([`WidgetEvent::WakeUp`], each alone, without
    /// bubbling), then runs a frame if any was woken (F2.10b2, a precision of ADR-0040 W2).
    ///
    /// The window calls it with the real clock at the start of every batch of events, before
    /// handing them over, so the events of a batch share one time; tests call it with made-up
    /// times.
    ///
    /// # Complexity
    /// O(pending wake-ups) (a handful), plus the woken widgets' work and, only then, a frame. No
    /// allocation once the reused vectors have their capacity.
    pub fn advance(&mut self, now: Instant, list: &mut DisplayList) {
        if now > self.now {
            self.now = now;
        }
        let now = self.now;
        // Collected first and called after: a woken widget asks for its next wake-up while it is
        // called, which changes `wakeups`; and a widget asking for a time already past is woken
        // at the next advance, not again in this one (no endless loop).
        let mut due = std::mem::take(&mut self.due);
        due.clear();
        self.wakeups.retain(|&(id, at)| {
            if at <= now {
                due.push(id);
                false
            } else {
                true
            }
        });
        for &id in &due {
            if let Some(slot) = self.tree.slot_of(id) {
                self.deliver(slot, &WidgetEvent::WakeUp, false);
            }
        }
        let mut woken = !due.is_empty();
        self.due = due;
        // The tooltip's own timer (ADR-0048 AM14): it shows if the pointer is still resting on
        // the control it was started for.
        if let Some((control, at)) = self.tooltip.pending
            && at <= now
        {
            self.tooltip.pending = None;
            if self.tooltip_candidate() == Some(control) {
                self.show_tooltip(control);
                woken = true;
            }
        }
        // A submenu to open or close after the pointer rested (F2.11d2a).
        if let Some(timer) = self.submenu_timer
            && timer.at <= now
        {
            self.submenu_timer = None;
            self.submenu_due(timer, list);
            woken = true;
        }
        if woken {
            self.frame(list);
        }
    }

    /// The earliest pending wake-up, if any: the window sleeps until then
    /// (`ControlFlow::WaitUntil`), or until the next input event when `None`.
    ///
    /// # Complexity
    /// O(pending wake-ups), no allocation.
    #[must_use]
    pub fn next_wakeup(&self) -> Option<Instant> {
        let widgets = self.wakeups.iter().map(|&(_, at)| at).min();
        // The tooltip's and the submenus' timers count too: the window must wake for them.
        let tooltip = self.tooltip.pending.map(|(_, at)| at);
        let submenu = self.submenu_timer.map(|timer| timer.at);
        [widgets, tooltip, submenu].into_iter().flatten().min()
    }

    /// Handles one input event of the window (WA3), then runs a frame.
    pub fn input(&mut self, event: &InputEvent, list: &mut DisplayList) {
        match event {
            InputEvent::PointerMoved { position } => {
                self.pointer = *position;
                if let Some(captured) = self.captured {
                    self.deliver(
                        captured,
                        &WidgetEvent::PointerMoved {
                            position: *position,
                        },
                        false,
                    );
                } else {
                    let target = self.hit(*position);
                    self.set_hover(target);
                    if let Some(target) = target {
                        self.deliver(
                            target,
                            &WidgetEvent::PointerMoved {
                                position: *position,
                            },
                            true,
                        );
                    }
                    // A drawn menu follows the pointer with its highlight (F2.11d2a).
                    if !self.menus.is_empty() {
                        self.menu_pointer_moved(*position);
                    }
                    // Not during a drag: a tooltip would cover what is being dragged.
                    self.follow_tooltip();
                }
            }
            InputEvent::PointerLeft => {
                if self.captured.is_none() {
                    self.set_hover(None);
                }
                // Out of the window: no tooltip, and none waiting.
                self.hide_tooltip();
                self.tooltip.pending = None;
                self.tooltip.suppressed = None;
            }
            InputEvent::PointerButton {
                button,
                state,
                position,
                clicks,
            } => self.pointer_button(*button, *state, *position, *clicks, list),
            InputEvent::Wheel { delta } => {
                self.dismiss_tooltip();
                if let Some(target) = self.hit(self.pointer) {
                    let event = WidgetEvent::Wheel {
                        delta: *delta,
                        position: self.pointer,
                    };
                    self.deliver(target, &event, true);
                }
            }
            InputEvent::Key {
                key,
                physical,
                state,
                repeat,
                modifiers,
            } => self.key(key, *physical, *state, *repeat, *modifiers, list),
            // With a menu open, typed text and the input method's events reach nothing: the
            // menu takes its letters as keys, and the input method is off (`text_input`).
            InputEvent::Text(_) | InputEvent::Ime(_) if !self.menus.is_empty() => {}
            InputEvent::Text(text) => {
                // Like a key (WA3): to the focused widget, or the open modal layer, bubbling.
                let start = self
                    .focused
                    .or_else(|| self.modals.last().map(|&(layer, _)| layer));
                if let Some(start) = start {
                    self.deliver(start, &WidgetEvent::Text(text), true);
                }
            }
            InputEvent::Ime(event) => {
                // Like typed text (ADR-0047 TF6): to the focused widget, or the open modal layer,
                // bubbling. The window sends these only while the focused widget accepts text.
                let start = self
                    .focused
                    .or_else(|| self.modals.last().map(|&(layer, _)| layer));
                if let Some(start) = start {
                    self.deliver(start, &WidgetEvent::Ime(event), true);
                }
            }
            InputEvent::ModifiersChanged { modifiers } => self.modifiers = *modifiers,
            // A window losing the focus misses the releases of the keys held: forget them, or the
            // fine adjustment would stay on after an Alt+Tab with Ctrl held.
            // It also closes the menus and hides the tooltip, as the three platforms do.
            InputEvent::Focus(false) => {
                self.modifiers = Modifiers::default();
                // The release of a shortcut's key will not come either.
                self.action_key = None;
                self.close_menus_from(0, list, true);
                self.hide_tooltip();
                self.tooltip.pending = None;
            }
            InputEvent::Focus(true) => {}
        }
        self.frame(list);
    }

    /// A key (WA3): to the focused widget (or the open modal layer), bubbling; an unhandled Tab
    /// moves the focus (WA4), any other unhandled press is looked up among the actions
    /// (ADR-0048 AM4). With a menu open, to the topmost menu instead (ADR-0048 AM11).
    fn key(
        &mut self,
        key: &Key,
        physical: PhysicalKey,
        state: ButtonState,
        repeat: bool,
        modifiers: Modifiers,
        list: &mut DisplayList,
    ) {
        self.modifiers = modifiers;
        let event = WidgetEvent::Key {
            key,
            state,
            repeat,
            modifiers,
        };
        // Any key hides the tooltip, and still does what it does (choice 4 of F2.11c).
        if state == ButtonState::Pressed {
            self.dismiss_tooltip();
        }
        // The release of a key that matched an action is consumed like its press (AM4).
        if state == ButtonState::Released
            && let Some(held) = self.action_key
            && candidates(key, physical, modifiers)
                .iter()
                .flatten()
                .any(|candidate| candidate.key() == held)
        {
            self.action_key = None;
            return;
        }
        // With a menu open the keys are the topmost menu's, bubbling inside it only, and Tab moves
        // nothing in the main interface (ADR-0048 AM11).
        if let Some(menu) = self.menus.last() {
            // A menu built from a model has its own keys (F2.11d2a); another menu gets them in
            // its root, as F2.11c routed them.
            if menu.model.is_some() {
                self.menu_key(key, state, repeat, list);
            } else {
                let root = menu.root;
                self.deliver(root, &event, true);
            }
            return;
        }
        let start = self
            .focused
            .or_else(|| self.modals.last().map(|&(layer, _)| layer));
        let handled = start.and_then(|start| self.deliver(start, &event, true));
        if handled.is_some() || state != ButtonState::Pressed {
            return;
        }
        if *key == Key::Named(NamedKey::Tab) {
            self.move_focus(!modifiers.shift);
        } else if !self.context_menu_key(start, key, modifiers) {
            self.shortcut(start, key, physical, repeat, modifiers);
        }
    }

    /// Gives the interface the application's actions (ADR-0048 AM1): the registry, and the
    /// message that tells the application to run one (`message(id)` is queued with the widgets'
    /// messages, [`Ui::take_messages`]). From then on, keys no widget handles run the actions
    /// whose shortcuts they are (see the module documentation).
    pub fn set_actions(&mut self, registry: Registry, message: fn(ActionId) -> M) {
        self.actions = Some(Actions { registry, message });
        self.action_key = None;
    }

    /// The registry, once handed in.
    #[must_use]
    pub fn actions(&self) -> Option<&Registry> {
        self.actions.as_ref().map(|actions| &actions.registry)
    }

    /// The registry, to change states (AM2) and shortcuts (AM7).
    pub fn actions_mut(&mut self) -> Option<&mut Registry> {
        self.actions.as_mut().map(|actions| &mut actions.registry)
    }

    /// The state of `id` as the user sees it now (AM2): the registry's, except for a standard
    /// editing action that the focused widget takes (AM13), whose *enabled* is the widget's
    /// answer (Undo in a field with an empty local history is disabled). `None` for an unknown
    /// action or without a registry.
    #[must_use]
    pub fn action_state(&self, id: ActionId) -> Option<ActionState> {
        let registry = self.actions()?;
        let state = registry.state(id)?;
        let taken = registry
            .spec(id)
            .and_then(|spec| spec.edit)
            .and_then(|role| self.focused_edit_state(role));
        Some(taken.map_or(state, |enabled| ActionState {
            enabled,
            checked: state.checked,
        }))
    }

    /// Looks the unhandled press up among the actions (AM3, AM4, AM5) and runs the one found.
    /// `start` is where the key was delivered (the focused widget or the modal layer).
    ///
    /// # Cost
    /// O(depth) to collect the contexts, then at most two probes per context (O(1) each); no
    /// allocation.
    fn shortcut(
        &mut self,
        start: Option<usize>,
        key: &Key,
        physical: PhysicalKey,
        repeat: bool,
        modifiers: Modifiers,
    ) {
        let Some(actions) = self.actions.as_ref() else {
            return;
        };
        let candidates = candidates(key, physical, modifiers);
        let (contexts, count) = self.active_contexts(start);
        // Innermost context first, and in each the logical shortcut before the fallback.
        let found = contexts.iter().take(count).find_map(|&context| {
            candidates.iter().flatten().find_map(|&candidate| {
                actions
                    .registry
                    .lookup(context, candidate)
                    .map(|id| (id, candidate.key()))
            })
        });
        let Some((id, matched)) = found else {
            return;
        };
        self.action_key = Some(matched);
        let repeatable = actions
            .registry
            .spec(id)
            .is_some_and(|spec| spec.repeatable);
        // A repeat runs the action again only when it is repeatable; otherwise it is consumed.
        if repeat && !repeatable {
            return;
        }
        self.invoke(id);
    }

    /// Runs `id` as a shortcut or a menu item does: a standard editing action the focused widget
    /// takes goes to it (AM13); otherwise, when enabled, the application's message is queued.
    /// `true` when something ran.
    pub(crate) fn invoke(&mut self, id: ActionId) -> bool {
        let Some(actions) = self.actions.as_ref() else {
            return false;
        };
        let role = actions.registry.spec(id).and_then(|spec| spec.edit);
        if let Some(role) = role
            && let Some(focused) = self.focused
            && let Some(enabled) = self.focused_edit_state(role)
        {
            if enabled {
                self.call(focused, &WidgetEvent::Edit(role));
            }
            return enabled;
        }
        if !actions
            .registry
            .state(id)
            .is_some_and(|state| state.enabled)
        {
            return false;
        }
        let message = (actions.message)(id);
        self.messages.push(message);
        true
    }

    /// The focused widget's answer to a standard editing action (AM13), if it takes it.
    fn focused_edit_state(&self, role: EditRole) -> Option<bool> {
        let slot = self.tree.slot(self.focused?)?;
        slot.widget.as_ref()?.edit_state(role)
    }

    /// The active contexts for a key delivered at `start` (AM3): those declared on the way from
    /// `start` to the root, innermost first, then *Global*; and how many there are.
    fn active_contexts(&self, start: Option<usize>) -> ([Context; MAX_CONTEXTS], usize) {
        let mut contexts = [Context::GLOBAL; MAX_CONTEXTS];
        let mut count = 0;
        let mut current = start;
        // The last place is kept for *Global* (already in the array).
        while let Some(index) = current
            && count < MAX_CONTEXTS - 1
        {
            let Some(slot) = self.tree.slot(index) else {
                break;
            };
            if let Some(context) = slot.widget.as_ref().and_then(|widget| widget.context())
                && let Some(place) = contexts.get_mut(count)
            {
                *place = context;
                count += 1;
            }
            current = slot.parent;
        }
        // *Global* last: the array is already filled with it after the views' contexts.
        (contexts, count + 1)
    }

    /// A pointer button (WA3): a press goes to the widget under the pointer, which captures the
    /// pointer if it handles it; the release goes to the capturing widget.
    ///
    /// While a menu is open (F2.11c, ADR-0048 AM11) a press outside the menus and their owners
    /// closes every menu and is consumed, and a press inside moves no focus: the focused widget
    /// keeps it for the Edit items.
    fn pointer_button(
        &mut self,
        button: PointerButton,
        state: ButtonState,
        position: Point,
        clicks: u8,
        list: &mut DisplayList,
    ) {
        self.pointer = position;
        if state == ButtonState::Pressed {
            self.dismiss_tooltip();
        }
        let widget_event = WidgetEvent::PointerButton {
            button,
            state,
            position,
            clicks,
        };
        match (state, self.captured) {
            (ButtonState::Released, Some(captured)) => {
                self.captured = None;
                self.deliver(captured, &widget_event, false);
                // The pointer may be over another widget now.
                let target = self.hit(position);
                self.set_hover(target);
            }
            (ButtonState::Pressed, _) if !self.menus.is_empty() => match self.hit(position) {
                None => {
                    self.close_menus_from(0, list, true);
                    self.swallow_release = true;
                }
                // A press inside a drawn menu is the menu's (F2.11d2a).
                Some(target) if self.menu_press(target, list) => {}
                Some(target) => {
                    if let Some(handler) = self.deliver(target, &widget_event, true) {
                        self.captured = Some(handler);
                    }
                }
            },
            (ButtonState::Pressed, _) => {
                if let Some(target) = self.hit(position) {
                    self.focus_on_press(target);
                    if let Some(handler) = self.deliver(target, &widget_event, true) {
                        self.captured = Some(handler);
                    } else {
                        // Unhandled: a secondary press may open a context menu (AM12).
                        self.secondary_click(button, false, target, position);
                    }
                }
            }
            // The release of a press that closed the menus: consumed like its press.
            (ButtonState::Released, None) if self.swallow_release => self.swallow_release = false,
            // A release inside a drawn menu may choose an item (F2.11d2a).
            (ButtonState::Released, None)
                if !self.menus.is_empty() && self.menu_release(position, list) => {}
            (ButtonState::Released, None) => {
                if let Some(target) = self.hit(position)
                    && self.deliver(target, &widget_event, true).is_none()
                    && self.menus.is_empty()
                {
                    // Unhandled: on Windows a secondary release opens a context menu (AM12).
                    self.secondary_click(button, true, target, position);
                }
            }
        }
    }

    /// Gives `event` to the widget at `start` and, if `bubble`, to its ancestors until one
    /// handles it (WB7); stops at the modal layer's root, which is the top for events inside it.
    /// Returns the slot that handled it.
    fn deliver(&mut self, start: usize, event: &WidgetEvent<'_>, bubble: bool) -> Option<usize> {
        let mut current = Some(start);
        while let Some(index) = current {
            let (handled, parent) = self.call(index, event);
            if handled == Handled::Yes {
                return Some(index);
            }
            if !bubble {
                return None;
            }
            current = parent;
        }
        None
    }

    /// Calls the widget at `index` with `event` and applies what it asked for (WB6).
    fn call(&mut self, index: usize, event: &WidgetEvent<'_>) -> (Handled, Option<usize>) {
        let Some(slot) = self.tree.slot_mut(index) else {
            return (Handled::No, None);
        };
        let (id, parent, rect) = (slot.id, slot.parent, slot.rect);
        let Some(mut widget) = slot.widget.take() else {
            return (Handled::No, parent);
        };
        let mut cx = EventCx {
            id,
            rect,
            messages: &mut self.messages,
            relayout: false,
            rerecord: false,
            focus: false,
            rearrange: false,
            modifiers: self.modifiers,
            overlay: false,
            now: self.now,
            wakeup: WakeRequest::Keep,
            clipboard: self.clipboard.as_mut(),
            ime_reset: false,
        };
        let handled = widget.event(&mut cx, event);
        if cx.ime_reset {
            // A new request: the window sees the count change and resets the input method
            // ([`Ui::text_input`]). Wrapping, because only a change matters, never the value.
            self.ime_resets = self.ime_resets.wrapping_add(1);
        }
        let (relayout, rerecord, focus, rearrange, overlay, wakeup) = (
            cx.relayout,
            cx.rerecord,
            cx.focus,
            cx.rearrange,
            cx.overlay,
            cx.wakeup,
        );
        if let Some(slot) = self.tree.slot_mut(index) {
            slot.widget = Some(widget);
            slot.needs_record |= rerecord;
            slot.needs_overlay |= overlay;
        }
        match wakeup {
            WakeRequest::Keep => {}
            WakeRequest::At(at) => {
                // One pending wake-up per widget: a new request replaces the old one.
                if let Some(entry) = self.wakeups.iter_mut().find(|(owner, _)| *owner == id) {
                    entry.1 = at;
                } else {
                    self.wakeups.push((id, at));
                }
            }
            WakeRequest::Cancel => self.wakeups.retain(|(owner, _)| *owner != id),
        }
        if relayout {
            self.tree.invalidate_layout(index);
        } else if rearrange {
            // A full layout of the widget would arrange it anyway; only without one is it queued.
            self.rearrange.push(index);
        }
        if focus {
            self.set_focus_slot(Some(index));
        }
        (handled, parent)
    }

    /// The topmost widget under `point` (WA3): inside the innermost modal layer only, if one is
    /// open (WB9). Later children are tested first, as they are drawn on top.
    ///
    /// With menus open (F2.11c) only the menus, topmost first, and then their owners can be hit;
    /// the tooltip never is (it takes no input).
    fn hit(&self, point: Point) -> Option<usize> {
        if !self.menus.is_empty() {
            let menus = self.menus.iter().rev();
            return menus
                .clone()
                .find_map(|menu| self.hit_in(menu.root, point))
                .or_else(|| {
                    menus
                        .filter_map(|menu| menu.owner)
                        .find_map(|owner| self.hit_in(owner, point))
                });
        }
        let top = self.modals.last().map_or(ROOT, |&(layer, _)| layer);
        self.hit_in(top, point)
    }

    /// [`Ui::hit`] below `index`. Recursion depth is the tree's depth (a few tens at most).
    ///
    /// A widget is hit only where it can be seen: inside its rectangle **and** inside its clip
    /// (F2.09d), so the part of a scrolled content under a scroll bar, or outside the viewport,
    /// is not reachable.
    fn hit_in(&self, index: usize, point: Point) -> Option<usize> {
        let slot = self.tree.slot(index)?;
        if !contains(slot.rect, point) || slot.clip.is_some_and(|clip| !contains(clip, point)) {
            return None;
        }
        for &child in slot.children.iter().rev() {
            if let Some(found) = self.hit_in(child, point) {
                return Some(found);
            }
        }
        Some(index)
    }

    /// Changes the hovered widget, telling the old one it was left and the new one it was
    /// entered (WB8).
    fn set_hover(&mut self, target: Option<usize>) {
        if target == self.hovered {
            return;
        }
        if let Some(old) = self.hovered {
            self.call(old, &WidgetEvent::PointerLeft);
        }
        self.hovered = target;
        if let Some(new) = target {
            self.call(new, &WidgetEvent::PointerEntered);
        }
    }

    /// A press focuses the first focusable widget from `target` up (WA4); a press on nothing
    /// focusable leaves the focus where it is.
    fn focus_on_press(&mut self, target: usize) {
        let mut current = Some(target);
        while let Some(index) = current {
            let Some(slot) = self.tree.slot(index) else {
                return;
            };
            if slot.widget.as_deref().is_some_and(Widget::focusable) {
                self.set_focus_slot(Some(index));
                return;
            }
            current = slot.parent;
        }
    }

    /// Gives the focus to `id` (or to nobody).
    ///
    /// # Errors
    /// [`UiError::UnknownWidget`] if `id` does not exist.
    pub fn set_focus(
        &mut self,
        id: Option<WidgetId>,
        list: &mut DisplayList,
    ) -> Result<(), UiError> {
        let slot = match id {
            Some(id) => Some(self.tree.slot_of(id).ok_or(UiError::UnknownWidget(id))?),
            None => None,
        };
        self.set_focus_slot(slot);
        if let Some(slot) = slot {
            // Focus moved by the application: bring the widget into view, as Tab does (WF5).
            self.reveal(slot);
        }
        self.frame(list);
        Ok(())
    }

    /// Moves the focus, telling both widgets and marking them for recording (the ring).
    fn set_focus_slot(&mut self, target: Option<usize>) {
        if target == self.focused {
            return;
        }
        if let Some(old) = self.focused {
            if let Some(slot) = self.tree.slot_mut(old) {
                slot.needs_record = true;
            }
            self.call(old, &WidgetEvent::FocusLost);
        }
        self.focused = target;
        if let Some(new) = target {
            if let Some(slot) = self.tree.slot_mut(new) {
                slot.needs_record = true;
            }
            self.call(new, &WidgetEvent::FocusGained);
        }
    }

    /// Moves the focus to the next (or previous) focusable widget in tree order, wrapping
    /// around, inside the innermost modal layer if one is open (WA4).
    ///
    /// # Complexity
    /// O(widgets of the layer): one pre-order walk with the reused stack.
    fn move_focus(&mut self, forward: bool) {
        let top = self.modals.last().map_or(ROOT, |&(layer, _)| layer);
        let mut first = None;
        let mut last = None;
        let mut before = None;
        let mut after = None;
        let mut seen_current = false;
        self.stack.clear();
        self.stack.push(top);
        while let Some(entry) = self.stack.pop() {
            // An entry with the DEFERRED bit is a container coming back for its own turn, after
            // the children it asked to follow ([`Widget::tab_after_child`]).
            let (index, deferred) = (entry & !DEFERRED, entry & DEFERRED != 0);
            let Some(slot) = self.tree.slot(index) else {
                continue;
            };
            let widget = slot.widget.as_deref();
            if !deferred {
                match widget.and_then(Widget::tab_after_child) {
                    Some(after) if after < slot.children.len() => {
                        // Pushed in reverse so they pop as: children 0..=after, itself, the rest.
                        let (before, rest) = slot.children.split_at(after + 1);
                        self.stack.extend(rest.iter().rev().copied());
                        self.stack.push(index | DEFERRED);
                        self.stack.extend(before.iter().rev().copied());
                        continue;
                    }
                    _ => self.stack.extend(slot.children.iter().rev().copied()),
                }
            }
            if !widget.is_some_and(Widget::focusable) {
                continue;
            }
            first.get_or_insert(index);
            last = Some(index);
            if Some(index) == self.focused {
                seen_current = true;
            } else if seen_current {
                after.get_or_insert(index);
            } else {
                before = Some(index);
            }
        }
        let target = if forward {
            after.or(first)
        } else {
            before.or(last)
        };
        if target.is_some() {
            self.set_focus_slot(target);
            // A widget reached with the keyboard is brought into view (WF5).
            if let Some(target) = target {
                self.reveal(target);
            }
        }
    }

    /// Scrolls the scroll areas around `index` so that it can be seen (WF5): each ancestor,
    /// from the innermost out, is asked to bring the widget's rectangle (with room for its focus
    /// ring) into its viewport; an ancestor that moves its content is arranged again at the next
    /// frame, and the rectangle moves with it before the next ancestor is asked.
    ///
    /// # Complexity
    /// O(depth) calls, plus the re-arranging of the areas that moved.
    fn reveal(&mut self, index: usize) {
        let Some(slot) = self.tree.slot(index) else {
            return;
        };
        let mut target = slot.rect.inflate(DRAW_MARGIN);
        let mut current = slot.parent;
        while let Some(at) = current {
            let Some(slot) = self.tree.slot_mut(at) else {
                return;
            };
            current = slot.parent;
            let Some(widget) = slot.widget.as_deref_mut() else {
                continue;
            };
            let moved = widget.scroll_into_view(target);
            if moved != Point::default() {
                slot.needs_record = true;
                self.rearrange.push(at);
                target = Rect::new(
                    target.x0 + moved.x,
                    target.y0 + moved.y,
                    target.x1 + moved.x,
                    target.y1 + moved.y,
                );
            }
        }
    }

    /// Opens `id` as a modal layer (WB9): pointer events outside it are blocked and the focus
    /// moves to its first focusable widget, until [`Ui::close_modal`].
    ///
    /// # Errors
    /// [`UiError::UnknownWidget`] if `id` does not exist.
    pub fn open_modal(&mut self, id: WidgetId, list: &mut DisplayList) -> Result<(), UiError> {
        let layer = self.tree.slot_of(id).ok_or(UiError::UnknownWidget(id))?;
        self.modals.push((layer, self.focused));
        self.focused_inside(layer);
        if self
            .hovered
            .is_some_and(|hovered| !self.inside(hovered, layer))
        {
            self.set_hover(None);
        }
        self.frame(list);
        Ok(())
    }

    /// Closes the modal layer `id`, giving the focus back to where it was before it opened.
    ///
    /// # Errors
    /// [`UiError::UnknownWidget`] if `id` is not an open modal layer.
    pub fn close_modal(&mut self, id: WidgetId, list: &mut DisplayList) -> Result<(), UiError> {
        let layer = self.tree.slot_of(id).ok_or(UiError::UnknownWidget(id))?;
        let position = self
            .modals
            .iter()
            .rposition(|&(open, _)| open == layer)
            .ok_or(UiError::UnknownWidget(id))?;
        let (_, previous) = self.modals.remove(position);
        let previous = previous.filter(|&slot| self.tree.slot(slot).is_some());
        self.set_focus_slot(previous);
        self.frame(list);
        Ok(())
    }

    /// Focuses the first focusable widget inside `layer`, or nobody.
    fn focused_inside(&mut self, layer: usize) {
        self.set_focus_slot(None);
        self.move_focus(true);
        if self
            .focused
            .is_some_and(|focused| !self.inside(focused, layer))
        {
            self.set_focus_slot(None);
        }
    }

    /// `true` if `index` is `layer` or below it.
    fn inside(&self, index: usize, layer: usize) -> bool {
        let mut current = Some(index);
        while let Some(at) = current {
            if at == layer {
                return true;
            }
            current = self.tree.slot(at).and_then(|slot| slot.parent);
        }
        false
    }
}

impl<M: 'static> Ui<M> {
    /// Opens `widget` as a menu next to `popup.anchor` (F2.11c, ADR-0048 AM11) and answers its
    /// ID, under which the application adds the menu's items ([`Ui::add`]). The menu is laid out
    /// at its natural size and placed inside the window ([`super::popup::place`]), above the
    /// main interface and above the menus already open (a submenu is opened as another menu).
    ///
    /// While it is open the pointer reaches only the open menus and their owners, a press
    /// elsewhere closes them all, keys go to the topmost menu and the focus stays where it was
    /// (see the module documentation). A tooltip showing is hidden.
    ///
    /// # Errors
    /// [`UiError::UnknownWidget`] if `popup.owner` does not exist.
    pub fn open_menu(
        &mut self,
        widget: impl Widget<M>,
        popup: Popup,
        list: &mut DisplayList,
    ) -> Result<WidgetId, UiError> {
        let owner = match popup.owner {
            Some(id) => Some(self.tree.slot_of(id).ok_or(UiError::UnknownWidget(id))?),
            None => None,
        };
        let root = self.push_menu(Box::new(widget), popup, owner);
        self.frame(list);
        self.id_of(root)
            .ok_or(UiError::UnknownWidget(WidgetId::placeholder()))
    }

    /// Closes the open menu `id` and every menu opened after it (its submenus), removing their
    /// widgets and segments. Their owners are not told ([`WidgetEvent::MenuClosed`] is for the
    /// menus the interface closes by itself): the caller already knows.
    ///
    /// # Errors
    /// [`UiError::UnknownWidget`] if `id` is not an open menu.
    pub fn close_menu(&mut self, id: WidgetId, list: &mut DisplayList) -> Result<(), UiError> {
        let root = self.tree.slot_of(id).ok_or(UiError::UnknownWidget(id))?;
        let position = self
            .menus
            .iter()
            .position(|menu| menu.root == root)
            .ok_or(UiError::UnknownWidget(id))?;
        self.close_menus_from(position, list, false);
        self.frame(list);
        Ok(())
    }

    /// The open menus' IDs, lowest first.
    pub fn menus(&self) -> impl Iterator<Item = WidgetId> + '_ {
        self.menus.iter().filter_map(|menu| self.id_of(menu.root))
    }

    /// Closes the menus from position `from` of the stack upwards, topmost first, removing their
    /// subtrees; with `notify`, each owner still alive gets [`WidgetEvent::MenuClosed`].
    fn close_menus_from(&mut self, from: usize, list: &mut DisplayList, notify: bool) {
        if self.menus.len() > from {
            // The window node lists the open menus.
            self.access_window = true;
        }
        // A pending submenu change for a menu that goes is dropped.
        if self.submenu_timer.is_some_and(|timer| timer.menu >= from) {
            self.submenu_timer = None;
        }
        while self.menus.len() > from {
            let Some(menu) = self.menus.pop() else {
                break;
            };
            let id = self.id_of(menu.root);
            if self.tree.slot(menu.root).is_some() {
                self.remove_slot(menu.root, list);
            }
            // A submenu closing: the entry it hung from is no longer open (F2.11d2a).
            self.submenu_closed(menu.model.as_ref());
            if notify
                && let (Some(id), Some(owner)) = (id, menu.owner)
                && self.tree.slot(owner).is_some()
            {
                self.call(owner, &WidgetEvent::MenuClosed(id));
            }
        }
        if self.menus.is_empty() {
            self.next_layer = FIRST_MENU_LAYER;
        }
        // The pointer may be over the main interface again.
        if self.captured.is_none() {
            let hovered = self.hit(self.pointer);
            self.set_hover(hovered);
        }
    }

    /// The control whose tooltip the pointer would show now: the hovered widget or its nearest
    /// ancestor with a [`Widget::tooltip`]. None while a menu is open (menus explain themselves).
    ///
    /// # Cost
    /// O(depth) calls answering from stored state; no allocation.
    fn tooltip_candidate(&self) -> Option<usize> {
        if !self.menus.is_empty() {
            return None;
        }
        let mut current = self.hovered;
        while let Some(index) = current {
            let slot = self.tree.slot(index)?;
            if slot.widget.as_deref().and_then(Widget::tooltip).is_some() {
                return Some(index);
            }
            current = slot.parent;
        }
        None
    }

    /// Follows the pointer for the tooltip, after a move with nothing captured (ADR-0048 AM14):
    /// keeps the tooltip shown while the pointer is over its control or over the tooltip itself,
    /// hides it (starting the 500 ms in which the next one shows at once) when the pointer is over
    /// something else, and starts the timer of the control the pointer is now over.
    fn follow_tooltip(&mut self) {
        let candidate = self.tooltip_candidate();
        if let Some(shown) = self.tooltip.shown_for {
            if candidate == Some(shown) || self.pointer_over_tooltip() {
                return;
            }
            self.hide_tooltip();
            self.tooltip.warm_until = Some(self.now + TOOLTIP_WARM);
        }
        // A dismissed control stays without a tooltip until the pointer is over something else.
        if candidate != self.tooltip.suppressed {
            self.tooltip.suppressed = None;
        }
        match candidate {
            Some(control) if Some(control) != self.tooltip.suppressed => {
                if self
                    .tooltip
                    .pending
                    .is_some_and(|(waiting, _)| waiting == control)
                {
                    // Still resting on the same control: its timer runs on.
                    return;
                }
                if self
                    .tooltip
                    .warm_until
                    .is_some_and(|until| self.now < until)
                {
                    self.show_tooltip(control);
                } else {
                    self.tooltip.pending = Some((control, self.now + TOOLTIP_DELAY));
                }
            }
            _ => self.tooltip.pending = None,
        }
    }

    /// `true` while a tooltip is shown and the pointer is over it.
    fn pointer_over_tooltip(&self) -> bool {
        self.tooltip.shown_for.is_some()
            && self
                .tooltip
                .root
                .and_then(|root| self.tree.slot(root))
                .is_some_and(|slot| contains(slot.rect, self.pointer))
    }

    /// Shows the tooltip of `control` below the pointer (ADR-0048 AM14). The tooltip's widget is
    /// created the first time and reused afterwards; it is laid out and recorded at the next
    /// frame.
    fn show_tooltip(&mut self, control: usize) {
        let Some(text) = self
            .tree
            .slot(control)
            .and_then(|slot| slot.widget.as_deref())
            .and_then(Widget::tooltip)
        else {
            return;
        };
        let root = if let Some(root) = self.tooltip.root {
            // The tooltip's widget is taken out of its slot while the control's text (borrowed
            // from the same arena) is copied into it, so no temporary copy of the text is made:
            // the tooltip's own string keeps its capacity, and showing allocates nothing once
            // the text has fitted in it before. The downcast cannot fail (the slot holds the
            // interface's own `Tooltip`); if it did, the tooltip would simply keep its old text.
            let mut widget = self.tree.slot_mut(root).and_then(|slot| slot.widget.take());
            if let (Some(tooltip), Some(text)) = (
                widget
                    .as_deref_mut()
                    .and_then(|widget| widget.as_any_mut().downcast_mut::<Tooltip>()),
                self.tree
                    .slot(control)
                    .and_then(|slot| slot.widget.as_deref())
                    .and_then(Widget::tooltip),
            ) {
                tooltip.set_text(text);
                tooltip.set_visible(true);
            }
            if let Some(slot) = self.tree.slot_mut(root) {
                slot.widget = widget;
            }
            root
        } else {
            let tooltip = Tooltip::new(text);
            let root = self
                .tree
                .insert(None, Box::new(tooltip), LayoutProps::default());
            if let Some(slot) = self.tree.slot_mut(root) {
                slot.layer = TOOLTIP_LAYER;
            }
            self.tooltip.root = Some(root);
            root
        };
        self.tree.invalidate_layout(root);
        if let Some(slot) = self.tree.slot_mut(root) {
            slot.needs_record = true;
        }
        self.tooltip.anchor = popup::tooltip_anchor(self.pointer);
        self.tooltip.shown_for = Some(control);
        self.tooltip.pending = None;
    }

    /// Hides the tooltip, if one is shown: its segment is recorded empty at the next frame.
    fn hide_tooltip(&mut self) {
        if self.tooltip.shown_for.take().is_none() {
            return;
        }
        if let Some(root) = self.tooltip.root {
            if let Some(tooltip) = self
                .tree
                .slot_mut(root)
                .and_then(|slot| slot.widget.as_deref_mut())
                .and_then(|widget| widget.as_any_mut().downcast_mut::<Tooltip>())
            {
                tooltip.set_visible(false);
            }
            if let Some(slot) = self.tree.slot_mut(root) {
                slot.needs_record = true;
            }
        }
    }

    /// A press, a key or the wheel (ADR-0048 AM14): the tooltip goes away and its control's
    /// does not come back, nor start again, until the pointer is over something else.
    fn dismiss_tooltip(&mut self) {
        let control = self
            .tooltip
            .shown_for
            .or(self.tooltip.pending.map(|(control, _)| control));
        if control.is_some() {
            self.tooltip.suppressed = control;
        }
        self.hide_tooltip();
        self.tooltip.pending = None;
        self.tooltip.warm_until = None;
    }

    /// The text of the tooltip shown now, if any: for tests and for the screenshots' scenes.
    #[must_use]
    pub fn tooltip_text(&self) -> Option<&str> {
        self.tooltip.shown_for?;
        let root = self.tooltip.root?;
        self.tree
            .slot(root)?
            .widget
            .as_deref()?
            .as_any()
            .downcast_ref::<Tooltip>()
            .map(Tooltip::text)
    }

    /// The rectangle of the tooltip shown now, if any, logical pixels of the window, from the
    /// last frame.
    #[must_use]
    pub fn tooltip_rect(&self) -> Option<Rect> {
        self.tooltip.shown_for?;
        self.tree.slot(self.tooltip.root?).map(|slot| slot.rect)
    }
}

impl<M: 'static> Ui<M> {
    /// The accessibility update to send (ADR-0045 WA5, WC3): the whole tree the first time (or
    /// after [`Ui::reset_access`]), then only the widgets whose node changed, always with the
    /// focus. Call it after [`Ui::frame`] (directly or through `layout` and `input`).
    ///
    /// # Cost
    /// One pass over the widgets to find the marked ones (O(widgets), a flag test each), plus the
    /// nodes built, which allocate (AccessKit's nodes, WC10).
    pub fn take_access_update(&mut self) -> AccessUpdate {
        let full = !self.access_sent;
        let mut update = empty_update(self.access_focus(), full);
        if full || self.access_window {
            let menus = self.menus.iter().filter_map(|menu| self.id_of(menu.root));
            update
                .nodes
                .push((WINDOW_NODE, window_node(&self.title, self.root(), menus)));
        }
        for index in 0..self.tree.slots.len() {
            let Some(slot) = self.tree.slot(index) else {
                continue;
            };
            if !(full || slot.needs_access) {
                continue;
            }
            // The tooltip is not for assistive technologies (ADR-0048 AM14); the open menus are
            // children of the window node (F2.11d2a).
            if slot.layer == TOOLTIP_LAYER {
                continue;
            }
            let Some(widget) = slot.widget.as_deref() else {
                continue;
            };
            let mut info = AccessInfo::default();
            widget.access_info(&mut info);
            // The interface opens context menus, so it says which widgets have one.
            info.context_menu = widget.context_menu().is_some();
            let children = slot
                .children
                .iter()
                .filter_map(|&child| self.tree.slot(child).map(|child| child.id));
            push_widget_nodes(
                &mut update.nodes,
                slot.id,
                widget.access_role(),
                slot.rect,
                children,
                &info,
                widget.focusable(),
            );
        }
        for slot in self.tree.slots.iter_mut().flatten() {
            slot.needs_access = false;
        }
        self.access_sent = true;
        self.access_window = false;
        AccessUpdate(update)
    }

    /// Makes the next [`Ui::take_access_update`] carry the whole tree (an assistive technology
    /// just connected and asked for it).
    pub const fn reset_access(&mut self) {
        self.access_sent = false;
    }

    /// Carries out an assistive technology's request (WC5): a click activates the widget (it
    /// receives [`WidgetEvent::Activate`]), a focus request moves the focus. Then runs a frame.
    ///
    /// # Errors
    /// [`UiError::UnknownWidget`] if the target does not exist.
    pub fn access_action(
        &mut self,
        action: AccessAction,
        list: &mut DisplayList,
    ) -> Result<(), UiError> {
        let slot = self
            .tree
            .slot_of(action.target)
            .ok_or(UiError::UnknownWidget(action.target))?;
        match action.kind {
            // An item of an open menu is chosen as Enter chooses it (F2.11d2a).
            AccessActionKind::Click if self.menu_access_click(slot, list) => {}
            AccessActionKind::Click => {
                self.deliver(slot, &WidgetEvent::Activate, false);
            }
            AccessActionKind::ShowContextMenu => {
                if self.menus.is_empty() {
                    self.open_context_menu(slot, None);
                }
            }
            AccessActionKind::Focus => {
                self.set_focus_slot(Some(slot));
                self.reveal(slot);
            }
            AccessActionKind::ScrollIntoView => self.reveal(slot),
            AccessActionKind::Scroll { direction, page } => {
                self.deliver(slot, &WidgetEvent::Scroll { direction, page }, false);
            }
            AccessActionKind::Increment => {
                self.deliver(slot, &WidgetEvent::Increment, false);
            }
            AccessActionKind::Decrement => {
                self.deliver(slot, &WidgetEvent::Decrement, false);
            }
            AccessActionKind::SetValue(value) => {
                self.deliver(slot, &WidgetEvent::SetValue(value), false);
            }
            AccessActionKind::SetText(text) => {
                self.deliver(slot, &WidgetEvent::SetText(&text), false);
            }
            AccessActionKind::ReplaceSelectedText(text) => {
                self.deliver(slot, &WidgetEvent::ReplaceSelectedText(&text), false);
            }
            AccessActionKind::SetTextSelection { anchor, focus } => {
                self.deliver(
                    slot,
                    &WidgetEvent::SetTextSelection { anchor, focus },
                    false,
                );
            }
        }
        self.frame(list);
        Ok(())
    }
}

/// The bit marking a deferred entry of the Tab-order walk (see [`Ui::move_focus`]): slot indices
/// never reach it (a vector cannot hold that many slots).
const DEFERRED: usize = 1 << (usize::BITS - 1);

/// How far around a widget's rectangle its drawing may reach, logical pixels: the focus ring
/// (one pixel out plus a line of up to a few pixels, `look::focus_ring`) and anti-aliasing. Used
/// to decide that a widget is entirely outside its clip, and to bring a focused widget into view
/// with its ring (F2.09d).
const DRAW_MARGIN: f32 = 6.0;

/// Records `segment` with `draw`, inside `clip` (the widget's clip, see the module
/// documentation), for a widget whose rectangle, with room for the focus ring, is `reach`. Shared
/// by the main segment and the overlay, which have the same clip.
///
/// # Why the clip is avoided where it can be
/// Measured in F2.09d: the renderer builds a clip mask the size of the region it draws for every
/// clip it replays, so clipping the 200 widgets of a scrolled panel cost 397 M instructions per
/// wheel notch; only the widgets whose drawing crosses the viewport's edge need it.
///
/// # The rectangle is a guess, the drawing decides
/// A widget whose `reach` crosses the clip's edge is clipped, and one entirely outside records
/// nothing, both decided from the rectangle alone (cheap). A widget whose `reach` lies inside is
/// recorded through [`DisplayList::record_inside`], which looks at what it actually drew: a widget
/// may draw outside its rectangle (a label narrowed below its text's width by a splitter still
/// draws the whole text), and that drawing must not cross into the next pane. Until this fix the
/// rectangle alone decided, and such text overlapped the pane beside it.
fn record_clipped(
    list: &mut DisplayList,
    segment: SegmentId,
    clip: Option<Rect>,
    reach: Rect,
    draw: impl Fn(&mut Recorder<'_>),
) {
    match clip {
        // Not clipped: recorded as it is.
        None => list.record(segment, draw),
        // Inside its clip as far as its rectangle says: recorded without the clip, unless what it
        // drew reaches out.
        Some(clip) if encloses(clip, reach) => list.record_inside(segment, clip, draw),
        // Entirely outside its clip, focus ring included: an empty segment. Its old area is still
        // damaged (it may have been visible before this scroll step), but nothing is replayed for
        // it, and recording nothing is cheaper than recording commands the clip would drop one
        // by one.
        Some(clip) if !reach.intersects(&clip) => list.record(segment, |_| {}),
        Some(clip) => list.record(segment, |recorder| {
            recorder.push_clip_rect(clip);
            draw(recorder);
            recorder.pop_clip();
        }),
    }
}

/// `true` when `inner` lies entirely within `outer` (edges may touch).
fn encloses(outer: Rect, inner: Rect) -> bool {
    inner.x0 >= outer.x0 && inner.y0 >= outer.y0 && inner.x1 <= outer.x1 && inner.y1 <= outer.y1
}

/// `true` when `point` is inside `rect` (left and top edges in, right and bottom out, so two
/// rectangles that touch never both contain a point).
fn contains(rect: Rect, point: Point) -> bool {
    point.x >= rect.x0 && point.x < rect.x1 && point.y >= rect.y0 && point.y < rect.y1
}

impl WidgetId {
    /// An ID that no widget has (the counter starts at 1 and never wraps in practice), for the
    /// impossible error paths.
    const fn placeholder() -> Self {
        Self::MAX
    }
}

/// The root always exists (slot 0 is created with the tree and cannot be removed); this is the
/// panic-free answer for the impossible case.
const fn unreachable_root() -> WidgetId {
    WidgetId::MAX
}
