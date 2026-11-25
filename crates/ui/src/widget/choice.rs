// SPDX-License-Identifier: GPL-3.0-or-later

//! The widgets that choose one value among several (ADR-0050 DG9, SCALETTA F2.12a4a, owner's
//! choice 3): a **drop-down** ([`DropDown`]), a **group of options** ([`RadioGroup`]) and a
//! **list** ([`ListBox`]). The Preferences use the three: the list of sections, the theme's pair
//! (ten values, a drop-down), the mode *System / Light / Dark* (three values, all visible).
//!
//! # What they share
//! Each holds its values' texts, already localised, and the index of the chosen one; each sends
//! the application its message with the new index when the user changes it (never when the
//! application sets it, [`DropDown::set_selected`] and the others), and is one node for assistive
//! technologies, its chosen value in [`AccessInfo::value_text`].
//!
//! # The drop-down
//! It looks like a plain button with the chosen value's text and a small downward chevron, as wide
//! as its widest value (so it does not jump when the value changes; a narrower room shortens the
//! text with an ellipsis, F2.09f). A click, Enter, Space, Down or Alt+Down open its **list**, a
//! menu of the interface ([`EventCx::open_choices`], the menus of F2.11d2a), below it: the value in
//! use has a check mark, the keys and the pointer are the menus' own, and choosing a value closes
//! the list and comes back as [`WidgetEvent::Chosen`]. Opened from the keyboard, the value in use
//! is highlighted at once, so Enter keeps it. A second click on the drop-down closes the list.
//!
//! # The group of options
//! A row of **segments** in one bordered box, the chosen one filled with the accent (GNOME's linked
//! buttons, macOS's segmented control): every value is visible, so it suits a few short values. It
//! takes the focus as a whole (Tab enters and leaves it in one step, as a group of radio buttons
//! does); the arrows move the choice at once (Left/Up to the previous, Right/Down to the next,
//! Home and End), a click on a segment chooses it.
//!
//! # The list
//! A column of rows, the selected one filled with the accent; Up, Down, Home and End move the
//! selection at once, a click selects. The Preferences' sections are such a list: choosing a
//! section shows its content.
//!
//! Since F2.12c1 (the shortcut editor) a list may have a second, **detail** column
//! ([`ListBox::details`]): a muted text at the right end of each row (an action's shortcuts),
//! and its items may be **replaced** ([`ListBox::set_items`], a search filtering the actions). Its
//! accessible value is then "item, detail". A list may be empty: then nothing is selected, and
//! the arrows do nothing. When the selection moves from the keyboard the list asks the scroll
//! areas around it to show the selected row ([`EventCx::request_reveal`]).
//!
//! # Cost
//! The texts are laid out once per text generation (a change of language or of text tokens) and
//! kept; recording draws a few rectangles and texts. Events allocate nothing, except opening a
//! drop-down's list, which builds the menu's model and widgets once per opening (AM15).

use orchestra_text::TextLayout;

use crate::draw::geometry::{Point, Rect, RoundedRect};
use crate::draw::list::Recorder;
use crate::draw::paint::StrokeStyle;
use crate::theme::text::{FamilyToken, SizeToken, Weight};
use crate::theme::token::ColorToken;
use crate::widget::menu::MenuModel;
use crate::window::input::{ButtonState, Key, NamedKey, PointerButton};

use super::access::{AccessInfo, AccessRole};
use super::elide::ElidedText;
use super::layout::{Constraints, Size};
use super::tree::{EventCx, Handled, LayoutCx, RecordCx, Widget, WidgetEvent};

/// The style of the values' texts: the interface family, body size, regular weight (as buttons).
const STYLE: (FamilyToken, SizeToken, Weight) = (FamilyToken::Ui, SizeToken::Body, Weight::Regular);

/// The values' texts and their layouts, laid out once per text generation.
#[derive(Debug)]
struct Values {
    /// The texts, already localised.
    texts: Vec<String>,
    /// Their layouts, one per text.
    layouts: Vec<TextLayout>,
    /// The text generation they were laid out for; `None` before the first layout.
    laid_out_for: Option<u64>,
}

impl Values {
    /// The values `texts`, not laid out yet.
    fn new(texts: Vec<String>) -> Self {
        let layouts = texts.iter().map(|_| TextLayout::new()).collect();
        Self {
            texts,
            layouts,
            laid_out_for: None,
        }
    }

    /// Lays the texts out if the text generation changed; a text that cannot be laid out is
    /// left empty (it then takes no room and draws nothing, instead of failing the layout).
    fn lay_out<M: 'static>(&mut self, cx: &mut LayoutCx<'_, M>) {
        let generation = cx.text_generation();
        if self.laid_out_for == Some(generation) {
            return;
        }
        for (text, layout) in self.texts.iter().zip(&mut self.layouts) {
            if cx.lay_out_text(text, STYLE, None, layout).is_err() {
                *layout = TextLayout::new();
            }
        }
        self.laid_out_for = Some(generation);
    }

    /// The widest text, logical pixels (0 without values).
    fn widest(&self) -> f32 {
        self.layouts
            .iter()
            .map(TextLayout::width)
            .fold(0.0, f32::max)
    }

    /// The tallest text, logical pixels (0 without values).
    fn tallest(&self) -> f32 {
        self.layouts
            .iter()
            .map(TextLayout::height)
            .fold(0.0, f32::max)
    }

    /// The number of values.
    fn len(&self) -> usize {
        self.texts.len()
    }

    /// The text of value `index`, or an empty one.
    fn text(&self, index: usize) -> &str {
        self.texts.get(index).map_or("", String::as_str)
    }
}

/// `index` moved by `key` among `count` values (no wrapping, as GTK's and Windows' lists and
/// groups): the previous for Left or Up, the next for Right or Down, the first for Home, the last
/// for End. `None` for another key, or when the index would not change.
fn stepped(key: &Key, index: usize, count: usize) -> Option<usize> {
    let last = count.checked_sub(1)?;
    let next = match key {
        Key::Named(NamedKey::ArrowLeft | NamedKey::ArrowUp) => index.saturating_sub(1),
        Key::Named(NamedKey::ArrowRight | NamedKey::ArrowDown) => (index + 1).min(last),
        Key::Named(NamedKey::Home) => 0,
        Key::Named(NamedKey::End) => last,
        _ => return None,
    };
    (next != index).then_some(next)
}

/// `true` if `point` is inside `rect`.
fn contains(rect: Rect, point: Point) -> bool {
    point.x >= rect.x0 && point.x < rect.x1 && point.y >= rect.y0 && point.y < rect.y1
}

/// The control's border, inside its rectangle, as buttons draw theirs.
fn border(recorder: &mut Recorder<'_>, cx: &RecordCx, rect: Rect, radius: f32) {
    let width = f32::from(cx.shape.line_width.max(1));
    recorder.stroke_rounded_rect(
        RoundedRect::new(rect.inflate(-width / 2.0), (radius - width / 2.0).max(0.0)),
        StrokeStyle::new(width),
        ColorToken::ControlBorder,
    );
}

/// A drop-down (see the module documentation).
#[derive(Debug)]
pub struct DropDown<M> {
    /// The values.
    values: Values,
    /// The chosen value's index.
    selected: usize,
    /// The message sent with a new index chosen by the user.
    message: fn(usize) -> M,
    /// The accessible name (the label beside it, already localised); `None` for none.
    label: Option<String>,
    /// The pointer is over it.
    hovered: bool,
    /// Its list is open.
    open: bool,
    /// The chosen value's text, shortened with an ellipsis when the room is too narrow.
    shown: ElidedText,
}

impl<M> DropDown<M> {
    /// A drop-down of `values` (already localised) with `selected` chosen (brought inside the
    /// list), sending `message` with the index the user chooses.
    #[must_use]
    pub fn new(values: Vec<String>, selected: usize, message: fn(usize) -> M) -> Self {
        let selected = selected.min(values.len().saturating_sub(1));
        Self {
            values: Values::new(values),
            selected,
            message,
            label: None,
            hovered: false,
            open: false,
            shown: ElidedText::new(),
        }
    }

    /// The same drop-down named `label` for assistive technologies (the label shown beside it).
    #[must_use]
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// The chosen value's index.
    #[must_use]
    pub const fn selected(&self) -> usize {
        self.selected
    }

    /// Chooses value `index` without sending the message (the application's own change); the
    /// caller asks the interface to lay the drop-down out again. Out of the list: nothing.
    pub fn set_selected(&mut self, index: usize) {
        if index < self.values.len() && index != self.selected {
            self.selected = index;
            self.shown.invalidate();
        }
    }

    /// `true` while its list is open.
    #[must_use]
    pub const fn is_open(&self) -> bool {
        self.open
    }

    /// The list of values, the chosen one checked.
    fn model(&self) -> MenuModel {
        self.values
            .texts
            .iter()
            .enumerate()
            .fold(MenuModel::new(), |model, (index, text)| {
                model.choice(text.clone(), index == self.selected, index)
            })
    }

    /// The chevron's width, logical pixels: a third of the control's height.
    fn chevron(spacing: crate::Spacing) -> f32 {
        (spacing.control_height / 3.0).round()
    }
}

impl<M: std::fmt::Debug + 'static> Widget<M> for DropDown<M> {
    /// As wide as the widest value, its padding and the chevron; a control's height.
    fn measure(&mut self, cx: &mut LayoutCx<'_, M>, constraints: Constraints) -> Size {
        self.values.lay_out(cx);
        // The chosen value whole, for `arrange` to fit (or shorten) in the room it gets.
        self.shown
            .measure(cx, self.values.text(self.selected), STYLE, None);
        let spacing = cx.spacing();
        let width = self.values.widest() + 3.0 * spacing.control_padding + Self::chevron(spacing);
        constraints.constrain(Size::new(
            width,
            spacing.control_height.max(self.values.tallest()),
        ))
    }

    /// The chosen text in the room left by the padding and the chevron (shortened if needed).
    fn arrange(&mut self, cx: &mut LayoutCx<'_, M>, rect: Rect) {
        let spacing = cx.spacing();
        let room = rect.width() - 3.0 * spacing.control_padding - Self::chevron(spacing);
        self.shown
            .fit(cx, self.values.text(self.selected), STYLE, room);
    }

    fn record(&self, cx: &RecordCx, recorder: &mut Recorder<'_>) {
        let rect = cx.rect;
        let radius = f32::from(cx.shape.corner_radius);
        let fill = if self.open {
            ColorToken::ControlPressed
        } else if self.hovered {
            ColorToken::ControlHover
        } else {
            ColorToken::Control
        };
        recorder.fill_rounded_rect(RoundedRect::new(rect, radius), fill);
        border(recorder, cx, rect, radius);
        // The value at the left, vertically centred.
        let layout = self.shown.shown();
        let x = rect.x0 + cx.spacing.control_padding;
        let y = rect.y0 + (rect.height() - layout.height()) / 2.0;
        recorder.text(layout, Point::new(x, y), ColorToken::Text);
        // The chevron at the right: two strokes, like "⌄".
        let size = Self::chevron(cx.spacing);
        let right = rect.x1 - cx.spacing.control_padding;
        let middle = f32::midpoint(rect.y0, rect.y1);
        let line = f32::from(cx.shape.line_width.max(1)).max(1.5);
        recorder.stroke_path(StrokeStyle::new(line), ColorToken::Text, |path| {
            path.move_to(right - size, middle - size / 4.0);
            path.line_to(right - size / 2.0, middle + size / 4.0);
            path.line_to(right, middle - size / 4.0);
        });
        if cx.focused {
            super::look::focus_ring(recorder, rect, radius, cx.shape);
        }
    }

    fn event(&mut self, cx: &mut EventCx<'_, M>, event: &WidgetEvent<'_>) -> Handled {
        match event {
            WidgetEvent::PointerEntered | WidgetEvent::PointerLeft => {
                self.hovered = matches!(event, WidgetEvent::PointerEntered);
                cx.request_record();
                Handled::Yes
            }
            // On the press, as menus open (the release then chooses, once the list is armed).
            WidgetEvent::PointerButton {
                button: PointerButton::Left,
                state: ButtonState::Pressed,
                ..
            } => {
                cx.request_focus();
                if self.open {
                    cx.close_choices();
                    self.open = false;
                } else {
                    cx.open_choices(self.model(), false);
                    self.open = true;
                }
                cx.request_record();
                Handled::Yes
            }
            WidgetEvent::PointerButton { .. } | WidgetEvent::PointerMoved { .. } => Handled::Yes,
            WidgetEvent::Key {
                key: Key::Named(NamedKey::Enter | NamedKey::Space | NamedKey::ArrowDown),
                state: ButtonState::Pressed,
                ..
            }
            | WidgetEvent::Activate => {
                if !self.open {
                    cx.open_choices(self.model(), true);
                    self.open = true;
                    cx.request_record();
                }
                Handled::Yes
            }
            WidgetEvent::Chosen(index) => {
                self.open = false;
                if *index < self.values.len() && *index != self.selected {
                    self.selected = *index;
                    self.shown.invalidate();
                    cx.emit((self.message)(*index));
                    cx.request_layout();
                }
                cx.request_record();
                Handled::Yes
            }
            WidgetEvent::MenuClosed(_) => {
                self.open = false;
                cx.request_record();
                Handled::Yes
            }
            _ => Handled::No,
        }
    }

    fn focusable(&self) -> bool {
        true
    }

    /// The whole value while it is drawn shortened, as a button's text (F2.09f).
    fn tooltip(&self) -> Option<&str> {
        self.shown
            .is_elided()
            .then(|| self.values.text(self.selected))
    }

    fn access_role(&self) -> AccessRole {
        AccessRole::ComboBox
    }

    fn access_info(&self, info: &mut AccessInfo) {
        info.label.clone_from(&self.label);
        info.value_text = Some(self.values.text(self.selected).to_owned());
        info.expanded = Some(self.open);
        info.clickable = true;
    }
}

/// A group of options drawn as segments (see the module documentation).
#[derive(Debug)]
pub struct RadioGroup<M> {
    /// The values.
    values: Values,
    /// The chosen value's index.
    selected: usize,
    /// The message sent with a new index chosen by the user.
    message: fn(usize) -> M,
    /// The accessible name; `None` for none.
    label: Option<String>,
    /// The segment under the pointer, if any.
    hovered: Option<usize>,
}

impl<M> RadioGroup<M> {
    /// A group of `values` (already localised) with `selected` chosen, sending `message` with the
    /// index the user chooses.
    #[must_use]
    pub fn new(values: Vec<String>, selected: usize, message: fn(usize) -> M) -> Self {
        let selected = selected.min(values.len().saturating_sub(1));
        Self {
            values: Values::new(values),
            selected,
            message,
            label: None,
            hovered: None,
        }
    }

    /// The same group named `label` for assistive technologies.
    #[must_use]
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// The chosen value's index.
    #[must_use]
    pub const fn selected(&self) -> usize {
        self.selected
    }

    /// Chooses value `index` without sending the message; the caller asks the interface to record
    /// the group again. Out of the list: nothing.
    pub const fn set_selected(&mut self, index: usize) {
        if index < self.values.texts.len() {
            self.selected = index;
        }
    }

    /// The segment at `x` in a group drawn in `rect`: equal segments, left to right.
    fn segment_at(&self, rect: Rect, x: f32) -> Option<usize> {
        let count = self.values.len();
        if count == 0 || rect.width() <= 0.0 {
            return None;
        }
        #[allow(
            clippy::cast_precision_loss,
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a handful of segments (far below 2^24); the quotient is clamped to the \
                      segments' range before the conversion back"
        )]
        let index = (((x - rect.x0) / rect.width()) * count as f32)
            .floor()
            .clamp(0.0, (count - 1) as f32) as usize;
        Some(index)
    }

    /// Chooses `index` because the user asked: the message, and a new recording.
    fn choose(&mut self, cx: &mut EventCx<'_, M>, index: usize) {
        if index != self.selected && index < self.values.len() {
            self.selected = index;
            cx.emit((self.message)(index));
            cx.request_record();
        }
    }
}

impl<M: std::fmt::Debug + 'static> Widget<M> for RadioGroup<M> {
    /// Equal segments, each as wide as the widest value and its padding; a control's height.
    fn measure(&mut self, cx: &mut LayoutCx<'_, M>, constraints: Constraints) -> Size {
        self.values.lay_out(cx);
        let spacing = cx.spacing();
        #[allow(
            clippy::cast_precision_loss,
            reason = "a handful of segments, exact in f32"
        )]
        let count = self.values.len() as f32;
        let segment = self.values.widest() + 2.0 * spacing.control_padding;
        constraints.constrain(Size::new(
            segment * count,
            spacing.control_height.max(self.values.tallest()),
        ))
    }

    fn record(&self, cx: &RecordCx, recorder: &mut Recorder<'_>) {
        let rect = cx.rect;
        let radius = f32::from(cx.shape.corner_radius);
        recorder.fill_rounded_rect(RoundedRect::new(rect, radius), ColorToken::Control);
        let count = self.values.len();
        if count == 0 {
            border(recorder, cx, rect, radius);
            return;
        }
        #[allow(
            clippy::cast_precision_loss,
            reason = "a handful of segments, exact in f32"
        )]
        let width = rect.width() / count as f32;
        for (index, layout) in self.values.layouts.iter().enumerate() {
            #[allow(
                clippy::cast_precision_loss,
                reason = "a handful of segments, exact in f32"
            )]
            let x0 = rect.x0 + width * index as f32;
            let segment = Rect::new(x0, rect.y0, x0 + width, rect.y1);
            let chosen = index == self.selected;
            if chosen {
                recorder.fill_rounded_rect(
                    RoundedRect::new(segment.inflate(-2.0), (radius - 2.0).max(0.0)),
                    ColorToken::Accent,
                );
            } else if self.hovered == Some(index) {
                recorder.fill_rounded_rect(
                    RoundedRect::new(segment.inflate(-2.0), (radius - 2.0).max(0.0)),
                    ColorToken::ControlHover,
                );
            }
            // A thin line between two segments that are not chosen.
            if index > 0 && !chosen && index - 1 != self.selected {
                recorder.line(
                    Point::new(x0, rect.y0 + 6.0),
                    Point::new(x0, rect.y1 - 6.0),
                    StrokeStyle::new(f32::from(cx.shape.line_width.max(1))),
                    ColorToken::ControlBorder,
                );
            }
            let text = if chosen {
                ColorToken::OnAccent
            } else {
                ColorToken::Text
            };
            let x = segment.x0 + (segment.width() - layout.width()) / 2.0;
            let y = segment.y0 + (segment.height() - layout.height()) / 2.0;
            recorder.text(layout, Point::new(x, y), text);
        }
        border(recorder, cx, rect, radius);
        if cx.focused {
            super::look::focus_ring(recorder, rect, radius, cx.shape);
        }
    }

    fn event(&mut self, cx: &mut EventCx<'_, M>, event: &WidgetEvent<'_>) -> Handled {
        match event {
            WidgetEvent::PointerMoved { position } => {
                let hovered = self.segment_at(cx.rect(), position.x);
                if hovered != self.hovered {
                    self.hovered = hovered;
                    cx.request_record();
                }
                Handled::Yes
            }
            WidgetEvent::PointerLeft => {
                self.hovered = None;
                cx.request_record();
                Handled::Yes
            }
            WidgetEvent::PointerButton {
                button: PointerButton::Left,
                state: ButtonState::Pressed,
                position,
                ..
            } => {
                cx.request_focus();
                if contains(cx.rect(), *position)
                    && let Some(index) = self.segment_at(cx.rect(), position.x)
                {
                    self.choose(cx, index);
                }
                cx.request_record();
                Handled::Yes
            }
            WidgetEvent::PointerButton { .. } | WidgetEvent::PointerEntered => Handled::Yes,
            WidgetEvent::Key {
                key,
                state: ButtonState::Pressed,
                ..
            } => match stepped(key, self.selected, self.values.len()) {
                Some(index) => {
                    self.choose(cx, index);
                    Handled::Yes
                }
                None => Handled::No,
            },
            _ => Handled::No,
        }
    }

    fn focusable(&self) -> bool {
        true
    }

    fn access_role(&self) -> AccessRole {
        AccessRole::RadioGroup
    }

    fn access_info(&self, info: &mut AccessInfo) {
        info.label.clone_from(&self.label);
        info.value_text = Some(self.values.text(self.selected).to_owned());
    }
}

/// A list of items, one selected (see the module documentation).
#[derive(Debug)]
pub struct ListBox<M> {
    /// The items.
    values: Values,
    /// The detail of each item, at the right end of its row (F2.12c1); empty texts for none,
    /// and no column at all when every text is empty.
    details: Values,
    /// The selected item's index.
    selected: usize,
    /// The message sent with a new index selected by the user.
    message: fn(usize) -> M,
    /// The accessible name; `None` for none.
    label: Option<String>,
}

impl<M> ListBox<M> {
    /// A list of `items` (already localised) with `selected` selected, sending `message` with the
    /// index the user selects.
    #[must_use]
    pub fn new(items: Vec<String>, selected: usize, message: fn(usize) -> M) -> Self {
        let selected = selected.min(items.len().saturating_sub(1));
        let details = Values::new(vec![String::new(); items.len()]);
        Self {
            values: Values::new(items),
            details,
            selected,
            message,
            label: None,
        }
    }

    /// The same list named `label` for assistive technologies.
    #[must_use]
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// The same list with `details` at the right end of the rows, one per item in order (F2.12c1;
    /// missing ones are empty, extra ones ignored).
    #[must_use]
    pub fn details(mut self, details: Vec<String>) -> Self {
        self.details = Values::new(fitted(details, self.values.len()));
        self
    }

    /// The selected item's index (0 for an empty list).
    #[must_use]
    pub const fn selected(&self) -> usize {
        self.selected
    }

    /// The number of items.
    #[must_use]
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Whether the list has no item.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.values.len() == 0
    }

    /// Replaces the items and their details (F2.12c1: a search filtering the list), selecting
    /// `selected` (clamped). No message is sent: the application chose. The caller asks for a new
    /// layout ([`Ui::request_layout`](crate::Ui::request_layout)): the list's height and width
    /// change with its items.
    pub fn set_items(&mut self, items: Vec<String>, details: Vec<String>, selected: usize) {
        let count = items.len();
        self.selected = selected.min(count.saturating_sub(1));
        self.values = Values::new(items);
        self.details = Values::new(fitted(details, count));
    }

    /// Selects `index` (clamped) without sending the message: the application chose. The caller
    /// asks for a new recording ([`Ui::request_record`](crate::Ui::request_record)).
    pub fn set_selected(&mut self, index: usize) {
        self.selected = index.min(self.values.len().saturating_sub(1));
    }

    /// The width of the details' column with its gap before it (0 without details).
    fn detail_room(&self, spacing: crate::Spacing) -> f32 {
        let widest = self.details.widest();
        if widest > 0.0 {
            widest + 2.0 * spacing.control_padding
        } else {
            0.0
        }
    }

    /// The height of one row: a control's height.
    const fn row(spacing: crate::Spacing) -> f32 {
        spacing.control_height
    }

    /// Selects `index` because the user asked: the message, and a new recording.
    fn select(&mut self, cx: &mut EventCx<'_, M>, index: usize) {
        if index != self.selected && index < self.values.len() {
            self.selected = index;
            cx.emit((self.message)(index));
            cx.request_record();
        }
    }

    /// The rectangle of row `index` inside `rect` (the list's).
    fn row_rect(rect: Rect, row: f32, index: usize) -> Rect {
        #[allow(
            clippy::cast_precision_loss,
            reason = "a few hundred rows at most, exact in f32"
        )]
        let y0 = rect.y0 + row * index as f32;
        Rect::new(rect.x0, y0, rect.x1, y0 + row)
    }
}

/// `texts` made exactly `count` long: cut, or completed with empty texts. O(count).
fn fitted(mut texts: Vec<String>, count: usize) -> Vec<String> {
    texts.resize(count, String::new());
    texts
}

impl<M: std::fmt::Debug + 'static> Widget<M> for ListBox<M> {
    /// As wide as the widest item, the widest detail and their padding; one control's height per
    /// item.
    fn measure(&mut self, cx: &mut LayoutCx<'_, M>, constraints: Constraints) -> Size {
        self.values.lay_out(cx);
        self.details.lay_out(cx);
        let spacing = cx.spacing();
        #[allow(
            clippy::cast_precision_loss,
            reason = "a few hundred items at most, exact in f32"
        )]
        let rows = self.values.len() as f32;
        constraints.constrain(Size::new(
            self.values.widest() + 2.0 * spacing.control_padding + self.detail_room(spacing),
            Self::row(spacing) * rows,
        ))
    }

    fn record(&self, cx: &RecordCx, recorder: &mut Recorder<'_>) {
        let rect = cx.rect;
        let radius = f32::from(cx.shape.corner_radius);
        let row = Self::row(cx.spacing);
        for (index, layout) in self.values.layouts.iter().enumerate() {
            let line = Self::row_rect(rect, row, index);
            let selected = index == self.selected;
            if selected {
                recorder.fill_rounded_rect(RoundedRect::new(line, radius), ColorToken::Accent);
            }
            let text = if selected {
                ColorToken::OnAccent
            } else {
                ColorToken::Text
            };
            let y = line.y0 + (row - layout.height()) / 2.0;
            recorder.text(
                layout,
                Point::new(line.x0 + cx.spacing.control_padding, y),
                text,
            );
            // The detail at the right end, muted (on the accent: the accent's text colour).
            if let Some(detail) = self.details.layouts.get(index)
                && detail.width() > 0.0
            {
                let paint = if selected {
                    ColorToken::OnAccent
                } else {
                    ColorToken::TextMuted
                };
                let x = line.x1 - cx.spacing.control_padding - detail.width();
                let y = line.y0 + (row - detail.height()) / 2.0;
                recorder.text(detail, Point::new(x, y), paint);
            }
        }
        if cx.focused {
            super::look::focus_ring(recorder, rect, radius, cx.shape);
        }
    }

    fn event(&mut self, cx: &mut EventCx<'_, M>, event: &WidgetEvent<'_>) -> Handled {
        match event {
            WidgetEvent::PointerButton {
                button: PointerButton::Left,
                state: ButtonState::Pressed,
                position,
                ..
            } => {
                cx.request_focus();
                #[allow(
                    clippy::cast_precision_loss,
                    reason = "a handful of items, exact in f32"
                )]
                let row = cx.rect().height() / self.values.len().max(1) as f32;
                if row > 0.0 && contains(cx.rect(), *position) {
                    #[allow(
                        clippy::cast_possible_truncation,
                        clippy::cast_sign_loss,
                        reason = "a non-negative row number inside the list (checked above), far \
                                  below usize's range"
                    )]
                    let index = ((position.y - cx.rect().y0) / row).floor() as usize;
                    self.select(cx, index);
                }
                cx.request_record();
                Handled::Yes
            }
            WidgetEvent::PointerButton { .. }
            | WidgetEvent::PointerMoved { .. }
            | WidgetEvent::PointerEntered
            | WidgetEvent::PointerLeft => Handled::Yes,
            WidgetEvent::Key {
                key,
                state: ButtonState::Pressed,
                ..
            } => match stepped(key, self.selected, self.values.len()) {
                Some(index) => {
                    self.select(cx, index);
                    // The selected row in view, when the list scrolls (F2.12c1). The row's
                    // height from the last layout, as the click finds its row.
                    #[allow(
                        clippy::cast_precision_loss,
                        reason = "a few hundred items at most, exact in f32"
                    )]
                    let row = cx.rect().height() / self.values.len().max(1) as f32;
                    cx.request_reveal(Self::row_rect(cx.rect(), row, index));
                    Handled::Yes
                }
                None => Handled::No,
            },
            _ => Handled::No,
        }
    }

    fn focusable(&self) -> bool {
        true
    }

    fn access_role(&self) -> AccessRole {
        AccessRole::ListBox
    }

    fn access_info(&self, info: &mut AccessInfo) {
        info.label.clone_from(&self.label);
        if self.values.len() == 0 {
            return;
        }
        let item = self.values.text(self.selected);
        let detail = self.details.text(self.selected);
        info.value_text = Some(if detail.is_empty() {
            item.to_owned()
        } else {
            format!("{item}, {detail}")
        });
    }
}

#[cfg(test)]
mod tests {
    //! The keys' steps; the widgets' behaviour is tested through the interface in
    //! `tests/choices.rs`.
    use super::*;

    /// Arrows move by one without wrapping, Home and End jump, other keys do nothing.
    #[test]
    fn steps_without_wrapping() {
        let key = |named| Key::Named(named);
        assert_eq!(stepped(&key(NamedKey::ArrowRight), 0, 3), Some(1));
        assert_eq!(stepped(&key(NamedKey::ArrowDown), 2, 3), None);
        assert_eq!(stepped(&key(NamedKey::ArrowLeft), 0, 3), None);
        assert_eq!(stepped(&key(NamedKey::ArrowUp), 2, 3), Some(1));
        assert_eq!(stepped(&key(NamedKey::Home), 2, 3), Some(0));
        assert_eq!(stepped(&key(NamedKey::End), 0, 3), Some(2));
        assert_eq!(stepped(&key(NamedKey::Enter), 0, 3), None);
        assert_eq!(stepped(&key(NamedKey::End), 0, 0), None);
    }
}
