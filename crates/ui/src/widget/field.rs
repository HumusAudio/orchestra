// SPDX-License-Identifier: GPL-3.0-or-later

//! The text field (ADR-0047, step F2.10b2): one line of editable text, drawn and driven by the
//! pointer, the keyboard and the clock. The editing itself (text, caret, selection, the
//! platform's keys, the local undo) is the [`TextEditor`] of F2.10b1; this file draws it, turns
//! events into its actions, scrolls the text inside the field, blinks the caret and tells the
//! application when the text is committed.
//!
//! # Behaviour
//! - **Focus.** Tab or a click focuses the field. Focus by the keyboard selects the whole text
//!   (the convention of GTK, Windows and macOS, so typing replaces it); the click that focuses
//!   puts the caret where it lands instead (the press comes right after the focus and replaces
//!   that selection). The text the field had when it got the focus is kept, for Escape.
//! - **Keyboard** (TF3): the editing keys of the platform ([`edit_action`]), fixed inside the
//!   field. Typed text arrives as [`WidgetEvent::Text`] (TF5) after its key, whose plain
//!   character keys the field swallows, so they never reach single-key shortcuts. Cut, Copy and
//!   Paste are recognised and swallowed, but do nothing until the clipboard arrives (F2.10c).
//!   Undo on an empty local history does nothing and never reaches the document's undo (TF7).
//! - **Enter** commits (below) and bubbles, so a dialog around the field can take Enter as its
//!   default action after the commit (precision of TF7, F2.10b2). **Escape** brings back the text
//!   of the focus's start and commits nothing; when there is nothing to bring back it bubbles (a
//!   second Escape closes a dialog, TF7).
//! - **Commit** (TF7): on Enter and when the focus leaves, if the text differs from the last
//!   committed one, the field emits the application's `on_commit` message, which a view turns
//!   into one command of the document's history. A field whose text is not document data (a
//!   search) can also emit `on_change` at every edit.
//! - **Pointer** (TF4): a press puts the caret at the nearest grapheme boundary
//!   ([`TextLayout::hit_test`]); dragging selects (the pointer is captured, ADR-0045 WA3); a
//!   double click selects the word under the pointer and dragging then extends by words; a
//!   triple click selects everything; Shift+press extends the selection from its anchor. Over
//!   the field the pointer is the text beam ([`PointerShape::Text`]).
//! - **Scrolling inside** (TF10): when the text is wider than the field it moves sideways to keep
//!   the caret in view, with a margin, and comes back to fill the field when text is deleted at
//!   the end. Dragging past an edge scrolls with the pointer's moves.
//! - **The caret** (TF8): a 1.5-pixel bar in `text`, the line's height; it blinks 530 ms on,
//!   530 ms off, is shown solid again by every key and click, and stops blinking, visible, after
//!   10 s without input, so an idle window sleeps. Hidden while text is selected and without the
//!   focus. The blink uses the interface's wake-ups ([`EventCx::request_wakeup`]) and redraws only
//!   the caret: it is the field's **overlay** ([`Widget::has_overlay`]).
//!
//! # Looks (TF9)
//! A rounded rectangle of `surface-sunken` (the token of text fields) with a `control-border`
//! line; text in `text`, the placeholder (shown when the text is empty) in `text-muted`, both in
//! `text-disabled` when the field is disabled. The selection is a `selection` rectangle behind
//! the text, and the selected glyphs are drawn again in `on-selection`, clipped to it, so a glyph
//! cut by the selection's edge is right on both sides. Without the focus the selection is kept but
//! not drawn. The focus ring is the one of every control (`look`). Height `control-height`, side
//! padding `control-padding`.
//!
//! # When the text is laid out again
//! An edit marks the layout stale and asks to be **arranged** again
//! ([`EventCx::request_arrange`]): the field's size does not depend on its text, so nothing
//! around it is measured again, and `arrange` (which, unlike `event`, can reach the shaper) lays
//! the text out and recomputes the scroll offset. A caret movement asks the same, for the scroll
//! offset alone. The layout reuses its vectors (ADR-0044 TX9).
//!
//! # Not here yet
//! The clipboard and the input methods (F2.10c), the accessibility text runs (F2.10b3), the
//! multi-line field and the primary selection (postponed, ADR-0047).

use std::ops::Range;
use std::time::{Duration, Instant};

use orchestra_text::{TextLayout, prev_grapheme, word_at};

use crate::draw::geometry::{Point, Rect, RoundedRect};
use crate::draw::list::Recorder;
use crate::draw::paint::StrokeStyle;
use crate::theme::text::{FamilyToken, SizeToken, Weight};
use crate::theme::token::ColorToken;
use crate::window::input::{ButtonState, Key, Modifiers, NamedKey, PointerButton};
use crate::window::pointer::PointerShape;

use super::access::{AccessInfo, AccessRole};
use super::edit::keys::{EditConvention, edit_action};
use super::edit::{EditResponse, TextEditor};
use super::layout::{Constraints, Size};
use super::tree::{EventCx, Handled, LayoutCx, RecordCx, Widget, WidgetEvent};

/// How long the caret stays on, and then off (ADR-0047 TF8): Windows' default, close to GTK's
/// 1200 ms period.
pub(crate) const BLINK: Duration = Duration::from_millis(530);

/// After how long without input the caret stops blinking, visible (TF8, as GTK does).
pub(crate) const BLINK_STOP: Duration = Duration::from_secs(10);

/// The caret's width, logical pixels (TF8). The renderer snaps its edges to device pixels
/// (ADR-0039 D7), so it is one pixel at scale 1, two at scale 1.5 and 2, three at scale 2.
const CARET_WIDTH: f32 = 1.5;

/// The width a field asks for when its parent does not size it, logical pixels: about 25
/// characters of the body size, enough for a track's name.
const PREFERRED_WIDTH: f32 = 200.0;

/// The scroll margin (TF10), in units of the text's size (its em): this much text stays in view
/// beside the caret when the text scrolls, about three characters. Capped at a quarter of the
/// field's text area, so a narrow field still has room for the caret's side.
const SCROLL_MARGIN_EMS: f32 = 1.5;

/// The text style of a field: the body text of the interface.
const STYLE: (FamilyToken, SizeToken, Weight) = (FamilyToken::Ui, SizeToken::Body, Weight::Regular);

/// A drag in progress (the pointer is captured).
#[derive(Debug, Clone, PartialEq, Eq)]
enum Drag {
    /// Started by a single press: the selection follows the pointer grapheme by grapheme.
    Graphemes,
    /// Started by a double click on this word: the selection grows by whole words, and always
    /// contains the first one (TF4).
    Words(Range<usize>),
}

/// A one-line text field (ADR-0047), sending `M` messages.
///
/// ```
/// use orchestra_ui::{EditConvention, TextField};
///
/// #[derive(Debug, Clone, PartialEq)]
/// enum Message {
///     Renamed(String),
/// }
///
/// let field = TextField::new("Violin", |text| Message::Renamed(text.to_owned()))
///     .placeholder("Name")
///     .labelled("Track name")
///     .convention(EditConvention::Linux);
/// assert_eq!(field.text(), "Violin");
/// ```
#[derive(Debug)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent states (stale layout, enabled, focused, just focused, caret shown) that \
              change at different events; folding them into an enum of states would multiply it"
)]
pub struct TextField<M> {
    /// The text, the caret, the selection and the local undo (F2.10b1).
    editor: TextEditor,
    /// The text laid out (one line).
    layout: TextLayout,
    /// The placeholder, already localised by the application (empty: none).
    placeholder: String,
    /// The placeholder laid out.
    placeholder_layout: TextLayout,
    /// The text generation both layouts were made for ([`LayoutCx::text_generation`]), if they
    /// are current with the theme.
    laid_out: Option<u64>,
    /// The editor's text changed since `layout` was made.
    text_stale: bool,
    /// The name assistive technologies announce: the text of the label shown next to it.
    name: Option<String>,
    /// Whether it can be used.
    enabled: bool,
    /// It has the keyboard focus (from the focus events).
    focused: bool,
    /// The focus was just gained and no other event came since: the press that gave the focus
    /// places the caret and ignores Shift (it extends nothing, the selection being the one the
    /// focus made).
    fresh_focus: bool,
    /// The last committed text (at the focus's start, then at each commit): what Escape brings
    /// back and what a commit compares with (TF7).
    committed: String,
    /// The message built from the text at each commit (Enter, the focus leaving).
    on_commit: fn(&str) -> M,
    /// The message built from the text at each edit, for fields that filter live (TF7).
    on_change: Option<fn(&str) -> M>,
    /// How far the text is scrolled to the left inside the field, logical pixels, ≥ 0 (TF10).
    scroll: f32,
    /// The field's rectangle from the last arrange, logical pixels of the window.
    rect: Rect,
    /// The text's area inside it: the rectangle without the side padding.
    inner: Rect,
    /// The drag in progress, if any.
    drag: Option<Drag>,
    /// The caret is in its visible half of the blink (TF8).
    caret_on: bool,
    /// The last input (key, text, click, focus): the blink counts its halves from here, and
    /// stops [`BLINK_STOP`] after it.
    blink_from: Instant,
}

impl<M> TextField<M> {
    /// A field holding `text` (cleaned as a paste would be: one line), sending
    /// `on_commit(text)` when its text is committed (Enter, the focus leaving) after a change.
    /// It follows the platform's editing keys ([`EditConvention::current`]).
    #[must_use]
    pub fn new(text: &str, on_commit: fn(&str) -> M) -> Self {
        let editor = TextEditor::new(text, EditConvention::current());
        let committed = editor.text().to_owned();
        Self {
            editor,
            layout: TextLayout::new(),
            placeholder: String::new(),
            placeholder_layout: TextLayout::new(),
            laid_out: None,
            text_stale: true,
            name: None,
            enabled: true,
            focused: false,
            fresh_focus: false,
            committed,
            on_commit,
            on_change: None,
            scroll: 0.0,
            rect: Rect::EMPTY,
            inner: Rect::EMPTY,
            drag: None,
            caret_on: true,
            // Replaced at the first focus by the interface's time; the clock is only read here
            // because a field must start with some instant.
            blink_from: Instant::now(),
        }
    }

    /// The same field, showing `placeholder` (already localised) while its text is empty.
    #[must_use]
    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = placeholder.into();
        self.laid_out = None;
        self
    }

    /// The same field, named for assistive technologies with the text of the label shown next to
    /// it (already localised).
    #[must_use]
    pub fn labelled(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// The same field, also sending `on_change(text)` at every edit (live filtering, TF7).
    #[must_use]
    pub fn on_change(mut self, on_change: fn(&str) -> M) -> Self {
        self.on_change = Some(on_change);
        self
    }

    /// The same field, following another platform's editing keys (tests, and the screenshots
    /// that show one platform's behaviour on another's machine).
    #[must_use]
    pub fn convention(mut self, convention: EditConvention) -> Self {
        let text = self.editor.text().to_owned();
        self.editor = TextEditor::new(&text, convention);
        self
    }

    /// The text.
    #[must_use]
    pub fn text(&self) -> &str {
        self.editor.text()
    }

    /// The selected bytes, in text order (empty when nothing is selected); the caret is at
    /// [`TextField::caret`].
    #[must_use]
    pub fn selection(&self) -> Range<usize> {
        self.editor.selection()
    }

    /// Where the caret is, bytes into the text.
    #[must_use]
    pub const fn caret(&self) -> usize {
        self.editor.focus()
    }

    /// `true` while the caret is in the visible half of its blink (it is drawn only with the
    /// focus and nothing selected, see the module documentation).
    #[must_use]
    pub const fn caret_on(&self) -> bool {
        self.caret_on
    }

    /// How far the text is scrolled inside the field, logical pixels (TF10).
    #[must_use]
    pub const fn scroll(&self) -> f32 {
        self.scroll
    }

    /// The editor, to reserve its capacity ([`TextEditor::reserve`]) or read its history.
    #[must_use]
    pub const fn editor(&self) -> &TextEditor {
        &self.editor
    }

    /// Reserves room for a text of `text_bytes` and for an undo history of `history_bytes` in
    /// `history_edits` edits ([`TextEditor::reserve`]), so editing within them allocates nothing.
    pub fn reserve(&mut self, text_bytes: usize, history_bytes: usize, history_edits: usize) {
        self.editor
            .reserve(text_bytes, history_bytes, history_edits);
    }

    /// Replaces the text (the application changed it: the document was edited elsewhere, a
    /// command was undone): not an edit, so nothing to undo and nothing committed; it becomes the
    /// committed text. The caller asks the interface to lay the field out again
    /// ([`Ui::request_layout`](crate::Ui::request_layout)).
    pub fn set_text(&mut self, text: &str) {
        self.editor.set_text(text);
        self.editor.text().clone_into(&mut self.committed);
        self.text_stale = true;
    }

    /// Enables or disables the field; the caller asks the interface to record it again. A
    /// disabled field takes no focus and shows the arrow (TF4).
    pub const fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.drag = None;
        }
    }

    /// Lays out what is stale: the text after an edit, both texts after a theme change.
    fn lay_out(&mut self, cx: &mut LayoutCx<'_, M>)
    where
        M: 'static,
    {
        let generation = cx.text_generation();
        let theme_changed = self.laid_out != Some(generation);
        if theme_changed || self.text_stale {
            if cx
                .lay_out_text(self.editor.text(), STYLE, None, &mut self.layout)
                .is_err()
            {
                // Impossible with the bundled faces; an empty layout keeps the window usable.
                self.layout = TextLayout::new();
            }
            self.text_stale = false;
        }
        if theme_changed {
            if cx
                .lay_out_text(&self.placeholder, STYLE, None, &mut self.placeholder_layout)
                .is_err()
            {
                self.placeholder_layout = TextLayout::new();
            }
            self.laid_out = Some(generation);
        }
    }

    /// The y of the text's top: the line centred in the field.
    fn text_top(&self) -> f32 {
        self.rect.y0 + (self.rect.height() - self.layout.height()) / 2.0
    }

    /// The x where the text's left edge is drawn: the text area's left, moved by the scroll.
    fn text_left(&self) -> f32 {
        self.inner.x0 - self.scroll
    }

    /// The width of everything that must fit in view at the end of the text: the line's content,
    /// or the caret after a trailing space if that is further (spaces hang past the content's
    /// width, ADR-0044 TX10), plus the caret itself.
    fn content_width(&self) -> f32 {
        let text = self.editor.text();
        self.layout
            .width()
            .max(self.layout.caret_x(text, text.len()))
            + CARET_WIDTH
    }

    /// Brings the caret into view (TF10), after a layout or a movement.
    ///
    /// # The rule
    /// When the content fits in the text area, nothing scrolls. Otherwise the caret keeps a
    /// margin from both edges (about three characters, at most a quarter of the area), and the
    /// offset stays between 0 and the content's overflow: so deleting at the end lets the text
    /// come back to fill the field instead of leaving empty room at its right.
    ///
    /// # Complexity
    /// Two `caret_x` (O(log stops)), no allocation.
    fn scroll_to_caret(&mut self) {
        let width = self.inner.width().max(0.0);
        let content = self.content_width();
        if content <= width {
            self.scroll = 0.0;
            return;
        }
        let margin = (self.layout.size() * SCROLL_MARGIN_EMS).min(width / 4.0);
        let caret = self.layout.caret_x(self.editor.text(), self.editor.focus());
        if caret - self.scroll < margin {
            self.scroll = caret - margin;
        }
        if caret - self.scroll > width - margin {
            self.scroll = caret - (width - margin);
        }
        self.scroll = self.scroll.clamp(0.0, content - width);
    }

    /// The caret offset nearest to `point` (window coordinates): where a press puts the caret.
    fn offset_at(&self, point: Point) -> usize {
        let x = point.x - self.text_left();
        // One line: the y only has to land in it.
        let y = (point.y - self.text_top()).clamp(0.0, self.layout.height().max(0.0) / 2.0);
        self.layout.hit_test(self.editor.text(), x, y)
    }

    /// The grapheme **under** `point` (its start), rather than the nearest boundary: a double
    /// click on the right half of a word's last letter selects that word, not the space after it.
    fn grapheme_under(&self, point: Point) -> usize {
        let offset = self.offset_at(point);
        let text = self.editor.text();
        let x = point.x - self.text_left();
        if offset > 0 && self.layout.caret_x(text, offset) > x {
            prev_grapheme(text, offset)
        } else {
            offset
        }
    }

    /// Shows the caret solid and starts its blink again from now (TF8): every key, text, click
    /// and focus does this, so the caret never disappears under the user's hand.
    fn restart_blink(&mut self, cx: &mut EventCx<'_, M>) {
        let now = cx.now();
        self.blink_from = now;
        if !self.caret_on {
            self.caret_on = true;
        }
        cx.request_overlay();
        cx.request_wakeup(now + BLINK);
    }

    /// The wake-up came (TF8): the caret takes the half of the blink the time is in, and the next
    /// wake-up is asked at the next half's start, unless the blink has stopped.
    ///
    /// # Why the halves are counted from the last input, not toggled
    /// A wake-up can come late (a busy machine, a suspended laptop): counting where the time is
    /// keeps the rhythm and the 10 s stop exact whatever the delays, where toggling would drift.
    fn blink(&mut self, cx: &mut EventCx<'_, M>) {
        if !self.focused {
            return;
        }
        let elapsed = cx.now().saturating_duration_since(self.blink_from);
        if elapsed >= BLINK_STOP {
            // Stopped, visible: no more wake-ups, so the window can sleep.
            if !self.caret_on {
                self.caret_on = true;
                cx.request_overlay();
            }
            return;
        }
        // The number of whole halves since the last input: below 19 (10 s / 530 ms), so the
        // conversion never fails; a failure would only stop the blink, visible.
        let halves = u32::try_from(elapsed.as_nanos() / BLINK.as_nanos()).unwrap_or(u32::MAX);
        let on = halves % 2 == 0;
        if on != self.caret_on {
            self.caret_on = on;
            cx.request_overlay();
        }
        // The next half's start, or the stop if it comes first (the caret is then shown).
        let next_half = BLINK
            .checked_mul(halves.saturating_add(1))
            .map_or(BLINK_STOP, |at| at.min(BLINK_STOP));
        cx.request_wakeup(self.blink_from + next_half);
    }

    /// Commits the text (TF7): if it differs from the last committed one, the application is told
    /// and it becomes the committed one.
    fn commit(&mut self, cx: &mut EventCx<'_, M>) {
        if self.editor.text() != self.committed {
            self.editor.text().clone_into(&mut self.committed);
            cx.emit((self.on_commit)(self.editor.text()));
        }
    }

    /// What follows an editing action: a changed text is laid out again (and announced to a
    /// live filter), a moved caret scrolled into view; in every case the caret shows solid.
    fn after(&mut self, cx: &mut EventCx<'_, M>, response: EditResponse) {
        match response {
            EditResponse::TextChanged => {
                self.text_stale = true;
                cx.request_arrange();
                if let Some(on_change) = self.on_change {
                    cx.emit(on_change(self.editor.text()));
                }
            }
            EditResponse::SelectionChanged => cx.request_arrange(),
            // Nothing moved, or a clipboard action, which does nothing until F2.10c.
            EditResponse::Unchanged | EditResponse::Clipboard(_) => {}
        }
        self.restart_blink(cx);
    }

    /// A key pressed while focused.
    fn key(&mut self, cx: &mut EventCx<'_, M>, key: &Key, modifiers: Modifiers) -> Handled {
        if let Some(action) = edit_action(key, modifiers, self.editor.convention()) {
            let response = self.editor.apply(action);
            self.after(cx, response);
            return Handled::Yes;
        }
        match key {
            Key::Named(NamedKey::Enter) => {
                // Committed, and left to bubble: a dialog's default action may follow.
                self.commit(cx);
                Handled::No
            }
            Key::Named(NamedKey::Escape) => {
                if self.editor.text() == self.committed {
                    // Nothing to bring back: the second Escape goes on (to close a dialog).
                    return Handled::No;
                }
                let committed = std::mem::take(&mut self.committed);
                self.editor.set_text(&committed);
                self.committed = committed;
                self.after(cx, EditResponse::TextChanged);
                Handled::Yes
            }
            // The text it types follows as a text event; the key itself must not reach a
            // single-key shortcut. With the primary modifier alone it is a shortcut (Ctrl+K):
            // it bubbles. With Alt (⌥ on macOS types special characters) it types.
            Key::Named(NamedKey::Space) | Key::Character(_)
                if !modifiers.primary || modifiers.alt =>
            {
                Handled::Yes
            }
            _ => Handled::No,
        }
    }

    /// A left press at `point` with `clicks` (TF4).
    fn press(&mut self, cx: &mut EventCx<'_, M>, point: Point, clicks: u8) {
        let extend = cx.modifiers().shift && !self.fresh_focus;
        self.fresh_focus = false;
        let response = match clicks {
            0 | 1 => {
                let offset = self.offset_at(point);
                self.drag = Some(Drag::Graphemes);
                let anchor = if extend { self.editor.anchor() } else { offset };
                self.editor.select(anchor, offset)
            }
            2 => {
                let word = word_at(self.editor.text(), self.grapheme_under(point));
                let response = self.editor.select(word.start, word.end);
                self.drag = Some(Drag::Words(word));
                response
            }
            _ => {
                self.drag = None;
                self.editor.select(0, self.editor.text().len())
            }
        };
        self.after(cx, response);
    }

    /// The pointer moved to `point` during a drag (TF4).
    fn drag_to(&mut self, cx: &mut EventCx<'_, M>, point: Point) {
        let response = match &self.drag {
            None => return,
            Some(Drag::Graphemes) => {
                let offset = self.offset_at(point);
                self.editor.select(self.editor.anchor(), offset)
            }
            Some(Drag::Words(first)) => {
                let word = word_at(self.editor.text(), self.grapheme_under(point));
                // The selection always holds the first word, and grows from it by whole words
                // towards the pointer; the caret is at the end towards the pointer.
                if word.start < first.start {
                    self.editor.select(first.end, word.start)
                } else {
                    self.editor.select(first.start, word.end.max(first.end))
                }
            }
        };
        self.after(cx, response);
    }

    /// The colours of the text and of the placeholder.
    const fn text_colors(&self) -> (ColorToken, ColorToken) {
        if self.enabled {
            (ColorToken::Text, ColorToken::TextMuted)
        } else {
            (ColorToken::TextDisabled, ColorToken::TextDisabled)
        }
    }
}

/// `true` when `point` is inside `rect` (edges included on the top and left, as hit testing).
fn contains(rect: Rect, point: Point) -> bool {
    point.x >= rect.x0 && point.x < rect.x1 && point.y >= rect.y0 && point.y < rect.y1
}

impl<M: std::fmt::Debug + 'static> Widget<M> for TextField<M> {
    fn measure(&mut self, cx: &mut LayoutCx<'_, M>, constraints: Constraints) -> Size {
        self.lay_out(cx);
        let spacing = cx.spacing();
        let width = if constraints.max.width.is_finite() {
            PREFERRED_WIDTH.min(constraints.max.width)
        } else {
            PREFERRED_WIDTH
        };
        constraints.constrain(Size::new(
            width,
            spacing.control_height.max(self.layout.height()),
        ))
    }

    fn arrange(&mut self, cx: &mut LayoutCx<'_, M>, rect: Rect) {
        self.lay_out(cx);
        let padding = cx.spacing().control_padding;
        self.rect = rect;
        self.inner = Rect::new(
            rect.x0 + padding,
            rect.y0,
            (rect.x1 - padding).max(rect.x0 + padding),
            rect.y1,
        );
        self.scroll_to_caret();
    }

    fn record(&self, cx: &RecordCx, recorder: &mut Recorder<'_>) {
        let rect = cx.rect;
        let radius = f32::from(cx.shape.corner_radius);
        recorder.fill_rounded_rect(RoundedRect::new(rect, radius), ColorToken::SurfaceSunken);
        let line = f32::from(cx.shape.line_width.max(1));
        recorder.stroke_rounded_rect(
            RoundedRect::new(rect.inflate(-line / 2.0), (radius - line / 2.0).max(0.0)),
            StrokeStyle::new(line),
            ColorToken::ControlBorder,
        );
        let (text_color, placeholder_color) = self.text_colors();
        let origin = Point::new(self.text_left(), self.text_top());
        // The text is clipped to its area only when it overflows it: a clip costs a mask of the
        // redrawn region (F2.09d), and a short name never needs one.
        let area = Rect::new(self.inner.x0, rect.y0, self.inner.x1, rect.y1);
        let clipped = self.scroll > 0.0 || self.content_width() > self.inner.width();
        if clipped {
            recorder.push_clip_rect(area);
        }
        let text = self.editor.text();
        if text.is_empty() {
            recorder.text(&self.placeholder_layout, origin, placeholder_color);
        } else {
            let selection = self.editor.selection();
            if cx.focused && !selection.is_empty() {
                // The selection's band, behind the text, then the selected glyphs again in
                // `on-selection`, clipped to the band (TF9).
                let x0 = origin.x + self.layout.caret_x(text, selection.start);
                let x1 = origin.x + self.layout.caret_x(text, selection.end);
                let band =
                    Rect::new(x0, origin.y, x1, origin.y + self.layout.height()).intersect(&area);
                recorder.fill_rect(band, ColorToken::Selection);
                recorder.text(&self.layout, origin, text_color);
                if !band.is_empty() {
                    recorder.push_clip_rect(band);
                    recorder.text(&self.layout, origin, ColorToken::OnSelection);
                    recorder.pop_clip();
                }
            } else {
                recorder.text(&self.layout, origin, text_color);
            }
        }
        if clipped {
            recorder.pop_clip();
        }
        if cx.focused && self.enabled {
            super::look::focus_ring(recorder, rect, radius, cx.shape);
        }
    }

    fn has_overlay(&self) -> bool {
        true
    }

    fn record_overlay(&self, cx: &RecordCx, recorder: &mut Recorder<'_>) {
        // The caret: only with the focus, in its visible half, and with nothing selected.
        if !(cx.focused && self.enabled && self.caret_on && self.editor.selection().is_empty()) {
            return;
        }
        let x = self.text_left() + self.layout.caret_x(self.editor.text(), self.editor.focus());
        // Kept inside the text area: at the very end of a scrolled text the caret's bar would
        // otherwise touch the padding.
        let x = x.clamp(
            self.inner.x0,
            (self.inner.x1 - CARET_WIDTH).max(self.inner.x0),
        );
        let top = self.text_top();
        recorder.fill_rect(
            Rect::new(x, top, x + CARET_WIDTH, top + self.layout.height()),
            ColorToken::Text,
        );
    }

    fn event(&mut self, cx: &mut EventCx<'_, M>, event: &WidgetEvent<'_>) -> Handled {
        if !self.enabled {
            return Handled::No;
        }
        // Any event but the focus itself, the press that may follow it and the pointer coming
        // over the field ends the moment "just focused".
        if !matches!(
            event,
            WidgetEvent::FocusGained
                | WidgetEvent::PointerEntered
                | WidgetEvent::PointerLeft
                | WidgetEvent::PointerButton {
                    state: ButtonState::Pressed,
                    ..
                }
        ) {
            self.fresh_focus = false;
        }
        match *event {
            WidgetEvent::FocusGained => {
                self.focused = true;
                self.fresh_focus = true;
                self.editor.text().clone_into(&mut self.committed);
                // Focus by the keyboard selects everything (a click replaces it at once).
                let _ = self.editor.select(0, self.editor.text().len());
                cx.request_arrange();
                self.restart_blink(cx);
                Handled::Yes
            }
            WidgetEvent::FocusLost => {
                self.focused = false;
                self.fresh_focus = false;
                self.drag = None;
                self.commit(cx);
                // The local history ends with the focus (TF7).
                self.editor.clear_history();
                cx.cancel_wakeup();
                cx.request_record();
                Handled::Yes
            }
            WidgetEvent::WakeUp => {
                self.blink(cx);
                Handled::Yes
            }
            WidgetEvent::PointerButton {
                button: PointerButton::Left,
                state: ButtonState::Pressed,
                position,
                clicks,
            } => {
                self.press(cx, position, clicks);
                Handled::Yes
            }
            WidgetEvent::PointerMoved { position } if self.drag.is_some() => {
                self.drag_to(cx, position);
                Handled::Yes
            }
            WidgetEvent::PointerButton {
                button: PointerButton::Left,
                state: ButtonState::Released,
                ..
            } => {
                self.drag = None;
                Handled::Yes
            }
            WidgetEvent::Key {
                key,
                state: ButtonState::Pressed,
                modifiers,
                ..
            } => self.key(cx, key, modifiers),
            // The release of a key the field swallowed when pressed is swallowed too.
            WidgetEvent::Key {
                key: Key::Character(_) | Key::Named(NamedKey::Space),
                state: ButtonState::Released,
                ..
            } => Handled::Yes,
            WidgetEvent::Text(text) => {
                let response = self.editor.type_text(text);
                self.after(cx, response);
                Handled::Yes
            }
            _ => Handled::No,
        }
    }

    fn focusable(&self) -> bool {
        self.enabled
    }

    fn pointer_shape(&self, point: Point) -> Option<PointerShape> {
        // During a drag (captured) the beam stays wherever the pointer goes.
        (self.enabled && (self.drag.is_some() || contains(self.rect, point)))
            .then_some(PointerShape::Text)
    }

    fn access_role(&self) -> AccessRole {
        AccessRole::TextInput
    }

    fn access_info(&self, info: &mut AccessInfo) {
        info.label.clone_from(&self.name);
        info.disabled = !self.enabled;
    }
}
