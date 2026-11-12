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
//! - Keys go to the focused widget and bubble; an unhandled Tab moves the focus (WA4).
//! - The wheel goes to the widget under the pointer and bubbles.
//! - A modal layer (the last opened) blocks pointer events outside it and keeps the focus inside.
//!
//! # Allocation
//! Routing an event allocates nothing (hit testing recurses on the tree, bubbling follows parent
//! links, Tab order walks with a reused stack). Messages go into a queue that keeps its capacity.

use crate::draw::geometry::{Point, Rect};
use crate::draw::list::{DisplayList, SegmentId};
use crate::theme::token::Theme;
use crate::window::input::{ButtonState, InputEvent, Key, NamedKey};

use super::layout::{Constraints, LayoutProps, Size};
use super::tree::{EventCx, Handled, RecordCx, Widget, WidgetEvent, WidgetId, WidgetTree};

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
    /// A reused stack for walking the tree (Tab order, recording).
    stack: Vec<usize>,
    /// IDs removed by the last removal, to drop their segments (reused).
    removed: Vec<WidgetId>,
}

/// The root slot (always 0).
const ROOT: usize = 0;

impl<M: 'static> Ui<M> {
    /// An interface whose root is `root` (usually a container), laid out with `theme`'s density.
    pub fn new(root: impl Widget<M>, theme: &Theme) -> Self {
        Self {
            tree: WidgetTree::new(Box::new(root), theme.spacing()),
            bounds: Rect::EMPTY,
            messages: Vec::new(),
            hovered: None,
            captured: None,
            focused: None,
            modals: Vec::new(),
            pointer: Point::default(),
            stack: Vec::new(),
            removed: Vec::new(),
        }
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
        self.removed.clear();
        let mut removed = std::mem::take(&mut self.removed);
        self.tree.remove(slot, &mut removed);
        for gone in &removed {
            list.remove(SegmentId(gone.get()));
        }
        self.removed = removed;
        // Routing state pointing into the removed subtree is dropped.
        let alive =
            |slot: Option<usize>, tree: &WidgetTree<M>| slot.filter(|&s| tree.slot(s).is_some());
        self.hovered = alive(self.hovered, &self.tree);
        self.captured = alive(self.captured, &self.tree);
        self.focused = alive(self.focused, &self.tree);
        self.modals
            .retain(|&(layer, _)| self.tree.slot(layer).is_some());
        Ok(())
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

    /// Takes the messages the widgets emitted since the last call (WA7), keeping the queue's
    /// capacity.
    pub fn take_messages(&mut self) -> std::vec::Drain<'_, M> {
        self.messages.drain(..)
    }

    /// Uses `theme`'s density from now on: everything is laid out again (WB10).
    pub fn set_theme(&mut self, theme: &Theme) {
        let spacing = theme.spacing();
        if spacing != self.tree.spacing {
            self.tree.spacing = spacing;
            self.invalidate_all();
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
    /// # Complexity
    /// Measure and arrange visit the invalidated paths (the rest answer from their caches);
    /// recording walks the tree once, O(widgets), recording only the marked ones.
    pub fn frame(&mut self, list: &mut DisplayList) {
        if self.tree.slot(ROOT).is_some_and(|root| root.needs_layout) {
            let size = Size::new(self.bounds.width(), self.bounds.height());
            self.tree.measure(ROOT, Constraints::tight(size));
            self.tree.arrange(ROOT, self.bounds);
        }
        // Record in tree order (pre-order, children in order), so a segment recorded for the
        // first time lands above its parent's.
        let spacing = self.tree.spacing;
        let focused = self.focused;
        self.stack.clear();
        self.stack.push(ROOT);
        while let Some(index) = self.stack.pop() {
            let Some(slot) = self.tree.slot_mut(index) else {
                continue;
            };
            // Children pushed in reverse so they pop in order.
            self.stack.extend(slot.children.iter().rev().copied());
            if !slot.needs_record {
                continue;
            }
            slot.needs_record = false;
            let cx = RecordCx {
                rect: slot.rect,
                focused: focused == Some(index),
                spacing,
            };
            if let Some(widget) = slot.widget.as_deref() {
                list.record(SegmentId(slot.id.get()), |recorder| {
                    widget.record(&cx, recorder);
                });
            }
        }
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
                }
            }
            InputEvent::PointerLeft => {
                if self.captured.is_none() {
                    self.set_hover(None);
                }
            }
            InputEvent::PointerButton {
                button,
                state,
                position,
            } => {
                self.pointer = *position;
                let widget_event = WidgetEvent::PointerButton {
                    button: *button,
                    state: *state,
                    position: *position,
                };
                match (*state, self.captured) {
                    (ButtonState::Released, Some(captured)) => {
                        self.captured = None;
                        self.deliver(captured, &widget_event, false);
                        // The pointer may be over another widget now.
                        let target = self.hit(*position);
                        self.set_hover(target);
                    }
                    (ButtonState::Pressed, _) => {
                        if let Some(target) = self.hit(*position) {
                            self.focus_on_press(target);
                            if let Some(handler) = self.deliver(target, &widget_event, true) {
                                self.captured = Some(handler);
                            }
                        }
                    }
                    (ButtonState::Released, None) => {
                        if let Some(target) = self.hit(*position) {
                            self.deliver(target, &widget_event, true);
                        }
                    }
                }
            }
            InputEvent::Wheel { delta } => {
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
                state,
                repeat,
                modifiers,
            } => {
                let event = WidgetEvent::Key {
                    key,
                    state: *state,
                    repeat: *repeat,
                    modifiers: *modifiers,
                };
                let start = self
                    .focused
                    .or_else(|| self.modals.last().map(|&(layer, _)| layer));
                let handled = start.and_then(|start| self.deliver(start, &event, true));
                if handled.is_none()
                    && *state == ButtonState::Pressed
                    && *key == Key::Named(NamedKey::Tab)
                {
                    self.move_focus(!modifiers.shift);
                }
            }
            InputEvent::ModifiersChanged { .. } | InputEvent::Focus(_) => {}
        }
        self.frame(list);
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
        let (id, parent) = (slot.id, slot.parent);
        let Some(mut widget) = slot.widget.take() else {
            return (Handled::No, parent);
        };
        let mut cx = EventCx {
            id,
            messages: &mut self.messages,
            relayout: false,
            rerecord: false,
            focus: false,
        };
        let handled = widget.event(&mut cx, event);
        let (relayout, rerecord, focus) = (cx.relayout, cx.rerecord, cx.focus);
        if let Some(slot) = self.tree.slot_mut(index) {
            slot.widget = Some(widget);
            slot.needs_record |= rerecord;
        }
        if relayout {
            self.tree.invalidate_layout(index);
        }
        if focus {
            self.set_focus_slot(Some(index));
        }
        (handled, parent)
    }

    /// The topmost widget under `point` (WA3): inside the innermost modal layer only, if one is
    /// open (WB9). Later children are tested first, as they are drawn on top.
    fn hit(&self, point: Point) -> Option<usize> {
        let top = self.modals.last().map_or(ROOT, |&(layer, _)| layer);
        self.hit_in(top, point)
    }

    /// [`Ui::hit`] below `index`. Recursion depth is the tree's depth (a few tens at most).
    fn hit_in(&self, index: usize, point: Point) -> Option<usize> {
        let slot = self.tree.slot(index)?;
        let rect = slot.rect;
        if !(point.x >= rect.x0 && point.x < rect.x1 && point.y >= rect.y0 && point.y < rect.y1) {
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
        while let Some(index) = self.stack.pop() {
            let Some(slot) = self.tree.slot(index) else {
                continue;
            };
            self.stack.extend(slot.children.iter().rev().copied());
            if !slot.widget.as_deref().is_some_and(Widget::focusable) {
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
