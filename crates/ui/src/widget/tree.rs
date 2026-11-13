// SPDX-License-Identifier: GPL-3.0-or-later

//! The widget tree (ADR-0045 WA1, choices WB2, WB3, WB6): an arena of retained widgets, the
//! [`Widget`] trait, and the contexts a widget receives when it is measured, arranged, recorded
//! or given an event.
//!
//! # The arena
//! Widgets live in a vector of slots; each slot holds the widget (boxed: widgets are of many
//! types), its parent and children (slot indices), its layout props, its rectangle from the last
//! layout, the cache of its last measure, and two flags: "lay me out again" and "record me
//! again". A [`WidgetId`] comes from one counter and is never reused (as the model's IDs,
//! ADR-0022), so a stale ID can never reach another widget; slots themselves are reused, and a
//! hash map takes an ID to its slot.
//!
//! # Why the widget is taken out of its slot
//! Measuring a container means measuring its children, which live in the same arena: the
//! container's `&mut self` and the arena's `&mut` cannot both be borrowed. So the tree takes the
//! widget out of its slot (`Option::take`), calls it with a context that borrows the arena, and
//! puts it back: one move of a pointer each way, no reference counting, no interior mutability.

use std::any::Any;
use std::collections::HashMap;
use std::hash::BuildHasherDefault;
use std::num::NonZeroU64;

use crate::draw::geometry::{Point, Rect};
use crate::draw::glyph_cache::KeyHasher;
use crate::draw::list::Recorder;
use orchestra_text::{Faces, Shaper, TextError, TextLayout};

use crate::theme::density::Spacing;
use crate::theme::text::{FamilyToken, SizeToken, Typography, Weight};
use crate::theme::token::Shape;
use crate::window::input::{ButtonState, Key, Modifiers, PointerButton, WheelDelta};
use crate::window::pointer::PointerShape;

use super::access::{AccessInfo, AccessRole};
use super::layout::{Constraints, LayoutProps, Size};

/// The stable name of a widget: from one counter, never reused (WB2). It is also the widget's
/// display-list segment (ADR-0045 WA6) and, from F2.09b, its AccessKit node (WA5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WidgetId(NonZeroU64);

impl WidgetId {
    /// The largest ID: never handed out in practice (the counter starts at 1), used for the
    /// impossible error paths.
    pub(crate) const MAX: Self = Self(NonZeroU64::MAX);

    /// The ID with this number, if it is one (not zero). For AccessKit's action requests, which
    /// name their target by the number (WC5); an ID that no widget has is answered "unknown" by
    /// the interface.
    #[must_use]
    pub const fn from_raw(raw: u64) -> Option<Self> {
        match NonZeroU64::new(raw) {
            Some(raw) => Some(Self(raw)),
            None => None,
        }
    }

    /// The number, for the display list's segment and AccessKit's node.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

/// Lets a boxed widget be looked at as its concrete type (to read or change its state from the
/// application). Implemented for every `'static` type.
pub trait AsAny: Any {
    /// `self` as `&dyn Any`.
    fn as_any(&self) -> &dyn Any;
    /// `self` as `&mut dyn Any`.
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

impl<T: Any> AsAny for T {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// Whether a widget handled an event (WB6): `No` lets the event bubble to the parent (WB7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Handled {
    /// The widget used the event: it goes no further.
    Yes,
    /// The widget ignored it: the parent gets it.
    No,
}

/// An event as a widget receives it: the window's input (ADR-0040 W5) after routing (WB7, WB8),
/// plus what the routing itself produces (entering, leaving, focus). Borrowed, so routing never
/// copies a key's text.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WidgetEvent<'a> {
    /// The pointer came over this widget.
    PointerEntered,
    /// The pointer left this widget.
    PointerLeft,
    /// The pointer moved (over this widget, or anywhere while it captures the pointer).
    PointerMoved {
        /// Where, logical pixels of the window.
        position: Point,
    },
    /// A pointer button. A widget that handles a press captures the pointer until the release.
    PointerButton {
        /// Which button.
        button: PointerButton,
        /// Pressed or released.
        state: ButtonState,
        /// Where.
        position: Point,
        /// Which click of a series (1 single, 2 double…, ADR-0046 V4); a release carries its
        /// press's count.
        clicks: u8,
    },
    /// The wheel or the touchpad.
    Wheel {
        /// How far.
        delta: WheelDelta,
        /// Where the pointer is.
        position: Point,
    },
    /// A key, to the focused widget (bubbling up).
    Key {
        /// The key.
        key: &'a Key,
        /// Pressed or released.
        state: ButtonState,
        /// An automatic repeat.
        repeat: bool,
        /// The modifiers held.
        modifiers: Modifiers,
    },
    /// This widget got the keyboard focus.
    FocusGained,
    /// This widget lost the keyboard focus.
    FocusLost,
    /// An assistive technology asked to activate this widget (AccessKit's click, WC5): a button
    /// acts as if clicked.
    Activate,
    /// An assistive technology asked to increase the value by one step (F2.09c).
    Increment,
    /// An assistive technology asked to decrease the value by one step.
    Decrement,
    /// An assistive technology asked to set the value.
    SetValue(f64),
    /// An assistive technology asked to scroll (AccessKit's scroll actions, F2.09d): a scroll
    /// area moves its content by a line (`page == false`) or by a viewport (`page == true`).
    Scroll {
        /// Which way the *view* moves (`Down` shows what is below, as a scroll bar's arrow).
        direction: ScrollDirection,
        /// A whole viewport instead of a line.
        page: bool,
    },
}

/// Which way a view moves when it scrolls (F2.09d): `Down` shows what is further down, so the
/// content itself moves up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollDirection {
    /// Towards the top of the content.
    Up,
    /// Towards the bottom of the content.
    Down,
    /// Towards the left edge of the content.
    Left,
    /// Towards the right edge of the content.
    Right,
}

/// What a widget needs to record its segment: its rectangle, its focus, the theme's spacing.
#[derive(Debug, Clone, Copy)]
pub struct RecordCx {
    /// The widget's rectangle from the last layout, logical pixels of the window.
    pub rect: Rect,
    /// `true` when the widget has the keyboard focus (it draws its focus ring, ADR-0045 WA4).
    pub focused: bool,
    /// The spacing of the theme's density.
    pub spacing: Spacing,
    /// The theme's shape tokens (corner radius, line width, focus width).
    pub shape: Shape,
}

/// One widget of the interface (ADR-0045 WA1, WB3). `M` is the application's message type
/// (WA7). Containers are widgets too.
pub trait Widget<M>: AsAny + std::fmt::Debug {
    /// The size the widget wants within `constraints` (the answer is brought inside them by the
    /// caller anyway). A container measures its children through `cx`.
    fn measure(&mut self, cx: &mut LayoutCx<'_, M>, constraints: Constraints) -> Size;

    /// Places the children inside `rect` (the widget's own rectangle, already stored). Leaves do
    /// nothing.
    fn arrange(&mut self, cx: &mut LayoutCx<'_, M>, rect: Rect) {
        let _ = (cx, rect);
    }

    /// Records the widget's segment: its own drawing, not its children's (they have their own
    /// segments, WA6). A widget that draws nothing records nothing.
    fn record(&self, cx: &RecordCx, recorder: &mut Recorder<'_>);

    /// Handles an event; `Handled::No` lets it bubble to the parent (WB7). The default ignores
    /// everything.
    fn event(&mut self, cx: &mut EventCx<'_, M>, event: &WidgetEvent<'_>) -> Handled {
        let _ = (cx, event);
        Handled::No
    }

    /// `true` if the widget takes the keyboard focus (Tab stops at it, a click focuses it).
    fn focusable(&self) -> bool {
        false
    }

    /// The widget's role for assistive technologies (ADR-0045 WA5, WC2); a container by default.
    fn access_role(&self) -> AccessRole {
        AccessRole::Group
    }

    /// What the widget tells assistive technologies besides its role: its name, its state, its
    /// value. Nothing by default.
    fn access_info(&self, info: &mut AccessInfo) {
        let _ = info;
    }

    /// Where a focusable container sits in the Tab order among its own children (F2.09d): `None`
    /// (the default) puts it before them, as tree order does; `Some(k)` puts it after its child
    /// `k`. A splitter answers `Some(0)`, so Tab goes from the first pane to the divider and then
    /// into the second pane, as the reader sees them.
    fn tab_after_child(&self) -> Option<usize> {
        None
    }

    /// The rectangle the widget's **children** may be seen in, logical pixels of the window, or
    /// `None` when they are not clipped (F2.09d, choice WF1). A scroll area answers its viewport:
    /// the interface then clips its descendants' drawing to it and ignores the pointer outside
    /// it. Read after every layout, so a widget answers from what its last `arrange` stored.
    fn child_clip(&self) -> Option<Rect> {
        None
    }

    /// Scrolls so that `target` (a descendant's rectangle, logical pixels of the window) comes
    /// into view, and answers how far the content moved (positive: right and down); zero when it
    /// did not move or the widget does not scroll (the default). Called by the interface when the
    /// keyboard focus moves to a hidden widget (WF5), from the innermost scroll area outwards. A
    /// widget that moves its content must lay it out again: the interface arranges it again when
    /// the answer is not zero.
    fn scroll_into_view(&mut self, target: Rect) -> Point {
        let _ = target;
        Point::default()
    }

    /// The shape the pointer should take at `point` (logical pixels of the window), or `None`
    /// (the default) to let the parent decide (F2.09e, a precision of ADR-0045 WA3).
    ///
    /// # How the interface asks
    /// - While nothing captures the pointer, it asks the **hovered** widget first and then its
    ///   ancestors, up to the root (or to the open modal layer's root), until one answers
    ///   `Some`: the same bubbling as the pointer's events (WB7). Nobody answering means the
    ///   ordinary arrow. So a widget answers only for the parts of itself that want a shape: the
    ///   splitter answers over its divider's band and `None` elsewhere, and a pane's widgets
    ///   that answer nothing get the arrow, not the splitter's resize shape.
    /// - While a widget **captures** the pointer (a drag), only that widget is asked, wherever
    ///   the pointer is, even outside the widget or the window: the shape of a drag stays the
    ///   same from press to release. `None` then means the arrow.
    /// - Over the part of the window that an open modal layer blocks (WB9), nobody is asked.
    ///
    /// # Rules for implementers
    /// Answer from what the last `arrange` stored (rectangles, state), with no allocation: the
    /// question comes after every input event. A disabled widget answers `None`, whatever it
    /// would ask for when enabled, so the parent's shape (usually the arrow) shows: a shape
    /// promises an action, and a disabled widget has none to offer.
    fn pointer_shape(&self, point: Point) -> Option<PointerShape> {
        let _ = point;
        None
    }
}

/// What a widget can do while handling an event (WB6): send messages to the application and ask
/// the tree for work. Collected by the router, which applies it after the call.
#[derive(Debug)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "four independent requests a widget may make in any combination (lay out, record, \
              focus, arrange); a bit set would only hide them"
)]
pub struct EventCx<'a, M> {
    /// The widget receiving the event.
    pub(crate) id: WidgetId,
    /// Its rectangle from the last layout.
    pub(crate) rect: Rect,
    /// The application's message queue.
    pub(crate) messages: &'a mut Vec<M>,
    /// Lay this widget out again (its size may have changed).
    pub(crate) relayout: bool,
    /// Record this widget again (its look changed).
    pub(crate) rerecord: bool,
    /// Give this widget the focus.
    pub(crate) focus: bool,
    /// Arrange this widget's children again, without measuring anything (F2.09d: a scroll area
    /// whose offset changed, a splitter whose divider moved; their own size is the same).
    pub(crate) rearrange: bool,
    /// The modifiers held when the event arrived (ADR-0046 V3: the fine adjustment).
    pub(crate) modifiers: Modifiers,
}

impl<M> EventCx<'_, M> {
    /// The widget receiving the event.
    #[must_use]
    pub const fn id(&self) -> WidgetId {
        self.id
    }

    /// The widget's rectangle from the last layout (to know whether a release happened over it).
    #[must_use]
    pub const fn rect(&self) -> Rect {
        self.rect
    }

    /// The modifiers held when the event arrived: a value control adjusts finely while the
    /// primary modifier is held (ADR-0046 V3), also for pointer events, which carry no modifiers.
    #[must_use]
    pub const fn modifiers(&self) -> Modifiers {
        self.modifiers
    }

    /// Sends `message` to the application (WA7), after the event.
    pub fn emit(&mut self, message: M) {
        self.messages.push(message);
    }

    /// Asks the tree to lay this widget out again (and record it).
    pub const fn request_layout(&mut self) {
        self.relayout = true;
        self.rerecord = true;
    }

    /// Asks the tree to record this widget again (its look changed, not its size).
    pub const fn request_record(&mut self) {
        self.rerecord = true;
    }

    /// Asks the tree to **arrange** this widget again in the rectangle it already has (and to
    /// record it): its children move, but its own size did not change, so nothing above it is
    /// measured again (F2.09d). Cheaper than [`EventCx::request_layout`], which measures the
    /// widget and its ancestors again: a scroll area that scrolls, a splitter whose divider moves.
    pub const fn request_arrange(&mut self) {
        self.rearrange = true;
        self.rerecord = true;
    }

    /// Asks for the keyboard focus.
    pub const fn request_focus(&mut self) {
        self.focus = true;
    }
}

/// One slot of the arena.
#[derive(Debug)]
pub(crate) struct Slot<M> {
    /// The widget's ID.
    pub(crate) id: WidgetId,
    /// The widget; `None` only while it is being called (see the module documentation).
    pub(crate) widget: Option<Box<dyn Widget<M>>>,
    /// The parent's slot; `None` for the root.
    pub(crate) parent: Option<usize>,
    /// The children's slots, in order (later ones are drawn on top and hit first).
    pub(crate) children: Vec<usize>,
    /// How the parent lays this widget out.
    pub(crate) props: LayoutProps,
    /// The rectangle from the last layout.
    pub(crate) rect: Rect,
    /// Where the widget can be seen: the intersection of its ancestors' [`Widget::child_clip`],
    /// logical pixels of the window, `None` when no ancestor clips (F2.09d). Brought up to date
    /// by every frame's recording walk; its drawing is clipped to it and the pointer outside it
    /// does not reach the widget.
    pub(crate) clip: Option<Rect>,
    /// The last measure: its constraints and the answer.
    pub(crate) measured: Option<(Constraints, Size)>,
    /// Must be measured and arranged again.
    pub(crate) needs_layout: bool,
    /// Must be recorded again.
    pub(crate) needs_record: bool,
    /// Its accessibility node must be sent again (WC3).
    pub(crate) needs_access: bool,
}

/// The arena (WB2).
#[derive(Debug)]
pub(crate) struct WidgetTree<M> {
    /// The slots; `None` for a free one.
    pub(crate) slots: Vec<Option<Slot<M>>>,
    /// Free slots to reuse.
    free: Vec<usize>,
    /// From an ID to its slot.
    index: HashMap<WidgetId, usize, BuildHasherDefault<KeyHasher>>,
    /// The next ID's number.
    next: u64,
    /// The spacing of the theme's density, for layout.
    pub(crate) spacing: Spacing,
    /// The bundled faces, for the widgets that lay text out (WC7).
    pub(crate) faces: Faces,
    /// The shaper those widgets share (its working memory is reused, ADR-0044 TX9).
    pub(crate) shaper: Shaper,
    /// The theme's text tokens.
    pub(crate) typography: Typography,
    /// Grows each time the text tokens change, so widgets know their laid-out texts are stale.
    pub(crate) text_generation: u64,
}

impl<M: 'static> WidgetTree<M> {
    /// A tree whose root is `root` (slot 0).
    pub(crate) fn new(
        root: Box<dyn Widget<M>>,
        spacing: Spacing,
        faces: Faces,
        typography: Typography,
    ) -> Self {
        let mut tree = Self {
            slots: Vec::new(),
            free: Vec::new(),
            index: HashMap::default(),
            next: 1,
            spacing,
            faces,
            shaper: Shaper::new(),
            typography,
            text_generation: 0,
        };
        tree.insert(None, root, LayoutProps::default());
        tree
    }

    /// The slot of `id`.
    pub(crate) fn slot_of(&self, id: WidgetId) -> Option<usize> {
        self.index.get(&id).copied()
    }

    /// The slot at `index`.
    pub(crate) fn slot(&self, index: usize) -> Option<&Slot<M>> {
        self.slots.get(index).and_then(Option::as_ref)
    }

    /// The slot at `index`, mutably.
    pub(crate) fn slot_mut(&mut self, index: usize) -> Option<&mut Slot<M>> {
        self.slots.get_mut(index).and_then(Option::as_mut)
    }

    /// Inserts a widget as the last child of `parent` (or as the root); returns its slot.
    pub(crate) fn insert(
        &mut self,
        parent: Option<usize>,
        widget: Box<dyn Widget<M>>,
        props: LayoutProps,
    ) -> usize {
        // IDs never repeat: the counter only grows. u64 cannot run out in practice (a new widget
        // every nanosecond for 584 years); saturating keeps the code panic-free regardless.
        let id = WidgetId(NonZeroU64::new(self.next).unwrap_or(NonZeroU64::MIN));
        self.next = self.next.saturating_add(1);
        let slot = Slot {
            id,
            widget: Some(widget),
            parent,
            children: Vec::new(),
            props,
            rect: Rect::EMPTY,
            clip: None,
            measured: None,
            needs_layout: true,
            needs_record: true,
            needs_access: true,
        };
        let index = if let Some(index) = self.free.pop() {
            if let Some(place) = self.slots.get_mut(index) {
                *place = Some(slot);
            }
            index
        } else {
            self.slots.push(Some(slot));
            self.slots.len() - 1
        };
        self.index.insert(id, index);
        if let Some(parent) = parent
            && let Some(parent) = self.slot_mut(parent)
        {
            parent.children.push(index);
            // Its list of children changed: its accessibility node too (WC3).
            parent.needs_access = true;
        }
        self.invalidate_layout(index);
        index
    }

    /// Removes the subtree at `index`; returns the IDs removed (their segments must go too).
    pub(crate) fn remove(&mut self, index: usize, removed: &mut Vec<WidgetId>) {
        let parent = self.slot(index).and_then(|slot| slot.parent);
        if let Some(parent) = parent {
            if let Some(slot) = self.slot_mut(parent) {
                slot.children.retain(|&child| child != index);
                slot.needs_access = true;
            }
            self.invalidate_layout(parent);
        }
        // Depth first over the subtree, with an explicit stack (no recursion depth limit).
        let mut stack = vec![index];
        while let Some(current) = stack.pop() {
            if let Some(slot) = self.slots.get_mut(current).and_then(Option::take) {
                stack.extend(slot.children.iter().copied());
                self.index.remove(&slot.id);
                removed.push(slot.id);
                self.free.push(current);
            }
        }
    }

    /// Marks `index` and its ancestors as needing layout (WA2's upward invalidation).
    pub(crate) fn invalidate_layout(&mut self, index: usize) {
        let mut current = Some(index);
        while let Some(at) = current {
            let Some(slot) = self.slot_mut(at) else {
                break;
            };
            if slot.needs_layout && at != index {
                // Already marked from here up by an earlier invalidation.
                break;
            }
            slot.needs_layout = true;
            current = slot.parent;
        }
    }

    /// Measures the widget at `index` (WA2's first pass), from the cache when it is clean and the
    /// constraints did not change.
    pub(crate) fn measure(&mut self, index: usize, constraints: Constraints) -> Size {
        let Some(slot) = self.slot_mut(index) else {
            return Size::ZERO;
        };
        if !slot.needs_layout
            && let Some((last, size)) = slot.measured
            && last == constraints
        {
            return size;
        }
        let Some(mut widget) = slot.widget.take() else {
            return Size::ZERO;
        };
        let answer = {
            let mut cx = LayoutCx { tree: self, index };
            widget.measure(&mut cx, constraints)
        };
        let size = constraints.constrain(answer);
        if let Some(slot) = self.slot_mut(index) {
            slot.widget = Some(widget);
            if slot.measured.map(|(_, old)| old) != Some(size) {
                slot.needs_record = true;
            }
            slot.measured = Some((constraints, size));
            // Measured again: the next arrange must run even if the rectangle is the same.
            slot.needs_layout = true;
        }
        size
    }

    /// Arranges the widget at `index` in `rect` (WA2's second pass): skipped when it is clean and
    /// its rectangle did not change.
    pub(crate) fn arrange(&mut self, index: usize, rect: Rect) {
        let Some(slot) = self.slot_mut(index) else {
            return;
        };
        if !slot.needs_layout && slot.rect == rect {
            return;
        }
        if slot.rect != rect {
            slot.rect = rect;
            slot.needs_record = true;
        }
        let Some(mut widget) = slot.widget.take() else {
            return;
        };
        {
            let mut cx = LayoutCx { tree: self, index };
            widget.arrange(&mut cx, rect);
        }
        if let Some(slot) = self.slot_mut(index) {
            slot.widget = Some(widget);
            slot.needs_layout = false;
        }
    }

    /// Arranges the widget at `index` again in the rectangle it already has (F2.09d,
    /// [`EventCx::request_arrange`]): its `arrange` runs even though the rectangle did not
    /// change, so it can move its children (a new scroll offset, a moved divider).
    ///
    /// # Why the flag is set on this slot alone
    /// The slot is marked as needing layout only for the duration of the call (`arrange` clears
    /// it at the end), and its ancestors are not marked: its own size did not change, so they
    /// have nothing to redo. The invariant `invalidate_layout` relies on ("a marked slot has its
    /// ancestors marked") therefore holds again as soon as this returns.
    ///
    /// # Complexity
    /// O(descendants whose rectangle changes): each one is arranged and marked for recording.
    pub(crate) fn rearrange(&mut self, index: usize) {
        let Some(slot) = self.slot_mut(index) else {
            return;
        };
        let rect = slot.rect;
        slot.needs_layout = true;
        slot.needs_record = true;
        self.arrange(index, rect);
    }

    /// The child slot `child` of `index` (its position among the children).
    fn child(&self, index: usize, child: usize) -> Option<usize> {
        self.slot(index)
            .and_then(|slot| slot.children.get(child))
            .copied()
    }
}

/// What a widget can reach while it is measured or arranged: its children (by position), their
/// props and sizes, and the theme's spacing.
#[derive(Debug)]
pub struct LayoutCx<'a, M> {
    /// The arena.
    tree: &'a mut WidgetTree<M>,
    /// The widget's slot.
    index: usize,
}

impl<M: 'static> LayoutCx<'_, M> {
    /// How many children the widget has.
    #[must_use]
    pub fn child_count(&self) -> usize {
        self.tree
            .slot(self.index)
            .map_or(0, |slot| slot.children.len())
    }

    /// The layout props of child `child` (defaults if there is no such child).
    #[must_use]
    pub fn child_props(&self, child: usize) -> LayoutProps {
        self.tree
            .child(self.index, child)
            .and_then(|slot| self.tree.slot(slot))
            .map_or_else(LayoutProps::default, |slot| slot.props)
    }

    /// Measures child `child` within `constraints`.
    pub fn measure_child(&mut self, child: usize, constraints: Constraints) -> Size {
        match self.tree.child(self.index, child) {
            Some(slot) => self.tree.measure(slot, constraints),
            None => Size::ZERO,
        }
    }

    /// The size child `child` answered at its last measure.
    #[must_use]
    pub fn child_size(&self, child: usize) -> Size {
        self.tree
            .child(self.index, child)
            .and_then(|slot| self.tree.slot(slot))
            .and_then(|slot| slot.measured)
            .map_or(Size::ZERO, |(_, size)| size)
    }

    /// Places child `child` in `rect` and arranges it.
    pub fn place_child(&mut self, child: usize, rect: Rect) {
        if let Some(slot) = self.tree.child(self.index, child) {
            self.tree.arrange(slot, rect);
        }
    }

    /// The spacing of the theme's density (WB10).
    #[must_use]
    pub const fn spacing(&self) -> Spacing {
        self.tree.spacing
    }

    /// A number that changes whenever the theme's text tokens change: a widget keeps the number
    /// its text was laid out with, and lays it out again when it differs (ADR-0044 TX9).
    #[must_use]
    pub const fn text_generation(&self) -> u64 {
        self.tree.text_generation
    }

    /// Lays `text` out into `layout` in the theme's style for `family`, `size` and `weight`,
    /// wrapping at `max_width` if given (WC7). The faces and the shaper are the interface's own,
    /// shared by every widget, so laying out allocates nothing once the layout has its capacity.
    ///
    /// # Errors
    /// As [`Shaper::layout`].
    pub fn lay_out_text(
        &mut self,
        text: &str,
        style: (FamilyToken, SizeToken, Weight),
        max_width: Option<f32>,
        layout: &mut TextLayout,
    ) -> Result<(), TextError> {
        let (family, size, weight) = style;
        let style = self.tree.typography.text_style(family, size, weight);
        let tree = &mut *self.tree;
        tree.shaper
            .layout(&tree.faces, text, style, max_width, layout)
    }
}
