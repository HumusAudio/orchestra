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
//! # Allocation
//! Routing an event allocates nothing (hit testing recurses on the tree, bubbling follows parent
//! links, Tab order walks with a reused stack). Messages go into a queue that keeps its capacity.

use std::time::Instant;

use crate::draw::geometry::{Point, Rect};
use crate::draw::list::{DisplayList, Recorder, SegmentId};
use orchestra_text::Faces;

use crate::theme::token::{Shape, Theme};
use crate::window::input::{ButtonState, InputEvent, Key, Modifiers, NamedKey, PointerButton};
use crate::window::pointer::PointerShape;

use super::access::{
    AccessAction, AccessActionKind, AccessInfo, AccessUpdate, WINDOW_NODE, empty_update,
    widget_node, window_node,
};
use super::layout::{Constraints, LayoutProps, Size};
use super::tree::{
    EventCx, Handled, RecordCx, WakeRequest, Widget, WidgetEvent, WidgetId, WidgetTree,
};

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
    /// The pending wake-ups, one at most per widget, in no order. By ID, not by slot: a slot
    /// freed and reused must not inherit a wake-up. A handful at most (a blinking caret), so a
    /// linear scan beats any ordered structure.
    wakeups: Vec<(WidgetId, Instant)>,
    /// The widgets whose wake-up came, collected by [`Ui::advance`] before they are called
    /// (reused).
    due: Vec<WidgetId>,
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
            wakeups: Vec::new(),
            due: Vec::new(),
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
        // Slots freed by the removal may be reused by the next insertion: forget their requests.
        let tree = &self.tree;
        self.rearrange.retain(|&slot| tree.slot(slot).is_some());
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
        self.record(list);
    }

    /// The recording walk (WA6, F2.09d): pre-order, children in order, so a segment recorded for
    /// the first time lands above its parent's. Each slot receives the clip its ancestors give
    /// it; a slot whose clip changed is recorded again even if nothing else changed (the part of
    /// it that can be seen did).
    fn record(&mut self, list: &mut DisplayList) {
        let spacing = self.tree.spacing;
        let shape = self.shape;
        let focused = self.focused;
        self.walk.clear();
        self.walk.push((ROOT, None));
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
        let woken = !due.is_empty();
        self.due = due;
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
        self.wakeups.iter().map(|&(_, at)| at).min()
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
                clicks,
            } => self.pointer_button(*button, *state, *position, *clicks),
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
                self.modifiers = *modifiers;
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
            InputEvent::Text(text) => {
                // Like a key (WA3): to the focused widget, or the open modal layer, bubbling.
                let start = self
                    .focused
                    .or_else(|| self.modals.last().map(|&(layer, _)| layer));
                if let Some(start) = start {
                    self.deliver(start, &WidgetEvent::Text(text), true);
                }
            }
            InputEvent::ModifiersChanged { modifiers } => self.modifiers = *modifiers,
            // A window losing the focus misses the releases of the keys held: forget them, or the
            // fine adjustment would stay on after an Alt+Tab with Ctrl held.
            InputEvent::Focus(false) => self.modifiers = Modifiers::default(),
            InputEvent::Focus(true) => {}
        }
        self.frame(list);
    }

    /// A pointer button (WA3): a press goes to the widget under the pointer, which captures the
    /// pointer if it handles it; the release goes to the capturing widget.
    fn pointer_button(
        &mut self,
        button: PointerButton,
        state: ButtonState,
        position: Point,
        clicks: u8,
    ) {
        self.pointer = position;
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
            (ButtonState::Pressed, _) => {
                if let Some(target) = self.hit(position) {
                    self.focus_on_press(target);
                    if let Some(handler) = self.deliver(target, &widget_event, true) {
                        self.captured = Some(handler);
                    }
                }
            }
            (ButtonState::Released, None) => {
                if let Some(target) = self.hit(position) {
                    self.deliver(target, &widget_event, true);
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
        };
        let handled = widget.event(&mut cx, event);
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
    fn hit(&self, point: Point) -> Option<usize> {
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
    /// The accessibility update to send (ADR-0045 WA5, WC3): the whole tree the first time (or
    /// after [`Ui::reset_access`]), then only the widgets whose node changed, always with the
    /// focus. Call it after [`Ui::frame`] (directly or through `layout` and `input`).
    ///
    /// # Cost
    /// One pass over the widgets to find the marked ones (O(widgets), a flag test each), plus the
    /// nodes built, which allocate (AccessKit's nodes, WC10).
    pub fn take_access_update(&mut self) -> AccessUpdate {
        let full = !self.access_sent;
        let mut update = empty_update(self.focused(), full);
        if full {
            update
                .nodes
                .push((WINDOW_NODE, window_node(&self.title, self.root())));
        }
        for index in 0..self.tree.slots.len() {
            let Some(slot) = self.tree.slot(index) else {
                continue;
            };
            if !(full || slot.needs_access) {
                continue;
            }
            let Some(widget) = slot.widget.as_deref() else {
                continue;
            };
            let mut info = AccessInfo::default();
            widget.access_info(&mut info);
            let children = slot
                .children
                .iter()
                .filter_map(|&child| self.tree.slot(child).map(|child| child.id));
            let node = widget_node(
                widget.access_role(),
                slot.rect,
                children,
                &info,
                widget.focusable(),
            );
            update.nodes.push((accesskit::NodeId(slot.id.get()), node));
        }
        for slot in self.tree.slots.iter_mut().flatten() {
            slot.needs_access = false;
        }
        self.access_sent = true;
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
            AccessActionKind::Click => {
                self.deliver(slot, &WidgetEvent::Activate, false);
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
