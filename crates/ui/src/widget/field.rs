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
//!   Paste use the interface's clipboard ([`EventCx::clipboard`], TF12, F2.10c1): the system's
//!   in a window, one in memory in tests.
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
//! # Assistive technologies (TF11, F2.10b3)
//! The field describes its text to them ([`AccessInfo::text`]): the value, every **character**
//! with its length, position and width, the word starts, the caret and the selection, and the
//! placeholder. A character is a grapheme, except that one longer than 255 bytes is given in
//! pieces ([`for_each_character`]), because AccessKit keeps a character's length in a byte. They
//! may set the whole text, replace the selection and select
//! ([`WidgetEvent::SetText`], [`WidgetEvent::ReplaceSelectedText`],
//! [`WidgetEvent::SetTextSelection`]): each goes through the same editor as the keys, as a paste
//! or a selection. With the focus these are edits like any other; **without the focus a changed
//! text is committed at once** and the local history cleared, because no Enter and no leaving
//! focus would commit it later.
//!
//! # Input methods (TF6, F2.10c2)
//! While the field has the focus the window keeps the system's input method on
//! ([`Widget::accepts_text`]), and tells it where the caret is ([`Widget::ime_area`]), so its
//! candidate window opens there. A **composition** (the input method's *preedit*: the romaji
//! being turned into kana, the pinyin before its characters are chosen) is shown at the caret,
//! **in place of the selection** if there is one, underlined with a one-pixel line in the text's
//! colour; the segment the input method is converting (when it says which) has a two-pixel line,
//! and the caret is then hidden, as GTK and Qt show it. Otherwise the caret is where the input
//! method puts it inside the composition. The composition is not part of the field's text: it
//! is not selectable, not committed, not in the undo history; assistive technologies get the
//! text as shown, composition included, with the caret inside it.
//!
//! The composed text arrives as a commit and is inserted like typing (it replaces the selection,
//! and is grouped in the undo history as typing is, TF7). Plain keys keep arriving as keys and
//! text even with the input method on: `winit` never reports the same key both ways, so both
//! are inserted (rule (b) of TF6, without a filter).
//!
//! While a composition is shown, the keys belong to the input method (choice D1 of F2.10c2):
//! the field swallows every key, so Enter, Escape, Backspace, the arrows and Tab neither edit
//! the field, nor commit it, nor move the focus, on the platforms that report them anyway. A
//! click inside the field moves the caret and ends the composition (the window resets the input
//! method, [`EventCx::reset_ime`]: most input methods then commit it); losing the focus, being
//! disabled, the input method being turned off and a text set by the application drop it.
//!
//! # The context menu (F2.11d2a, ADR-0048 AM12)
//! A field given a menu ([`TextField::with_context_menu`]: the application's Undo, Redo, Cut,
//! Copy, Paste and Select all) answers it as its [`Widget::context_menu`], and the interface opens
//! it under a secondary click, with the Menu key or Shift+F10 at the caret, and for an assistive
//! technology. Its items act on the field (AM13): the focus stays in the field while the menu is
//! open. A **secondary press** gives the field the focus, as any press does; inside the selection
//! it changes nothing (so Copy copies what was selected), elsewhere it puts the caret where it
//! lands, like a primary press without the drag (the convention of GTK, Firefox and Chrome). It
//! is left unhandled, so the interface opens the menu.
//!
//! # Not here yet
//! The multi-line field and the primary selection (postponed, ADR-0047). A fallback font for the
//! scripts the bundled faces do not cover (ADR-0044 TX4): Japanese or Chinese, composed or
//! committed, shows boxes for now.

use std::ops::Range;
use std::time::{Duration, Instant};

use orchestra_text::{TextLayout, grapheme_boundaries, prev_grapheme, word_at, word_starts};

use crate::action::registry::EditRole;
use crate::draw::geometry::{Point, Rect, RoundedRect};
use crate::draw::list::Recorder;
use crate::draw::paint::StrokeStyle;
use crate::theme::text::{FamilyToken, SizeToken, Weight};
use crate::theme::token::ColorToken;
use crate::window::input::{ButtonState, ImeEvent, Key, Modifiers, NamedKey, PointerButton};
use crate::window::pointer::PointerShape;

use super::access::{AccessInfo, AccessRole, TextInfo};
use super::edit::keys::{EditAction, EditConvention, edit_action};
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

/// The thickness of the line under a composition (TF6), logical pixels: the renderer snaps it to
/// device pixels, so it is one pixel at scale 1 (as TF6 asks) and two at scale 2.
const PREEDIT_LINE: f32 = 1.0;

/// The thickness of the line under the segment the input method is converting, logical pixels:
/// twice the composition's, so the segment stands out as in GTK and Qt (choice D6 of F2.10c2).
const PREEDIT_SEGMENT_LINE: f32 = 2.0;

/// How far below the baseline the composition's line is drawn, logical pixels: clear of the
/// letters' bottoms and inside the line's descent (about 3 px at the body size), so it never
/// touches the field's edge.
const PREEDIT_LINE_GAP: f32 = 1.0;

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
    /// The input method's composition (TF6), empty when there is none. Not part of the text.
    preedit: String,
    /// The input method's cursor inside [`TextField::preedit`], bytes (see
    /// [`ImeEvent::Preedit`]).
    preedit_cursor: Option<Range<usize>>,
    /// The text as shown while composing: the editor's text with the composition in place of
    /// the selection. Rebuilt when either changes, keeping its capacity (so a composition
    /// allocates nothing once warm); unused, and possibly stale, while not composing:
    /// [`TextField::shown`] is the only way to read it.
    composed: String,
    /// The field's context menu, if the application gave it one (F2.11d2a).
    context_menu: Option<crate::MenuModel>,
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
            preedit: String::new(),
            preedit_cursor: None,
            composed: String::new(),
            context_menu: None,
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

    /// The same field with `menu` as its context menu (F2.11d2a): usually the application's
    /// standard editing items, which act on the field while it has the focus (ADR-0048 AM13).
    #[must_use]
    pub fn with_context_menu(mut self, menu: crate::MenuModel) -> Self {
        self.context_menu = Some(menu);
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
    ///
    /// A composition in progress is dropped: it belonged to the old text. The input method may
    /// still commit it later; it is then inserted into the new text, as typing would be.
    pub fn set_text(&mut self, text: &str) {
        self.drop_preedit();
        self.editor.set_text(text);
        self.editor.text().clone_into(&mut self.committed);
        self.text_stale = true;
    }

    /// Enables or disables the field; the caller asks the interface to record it again. A
    /// disabled field takes no focus and shows the arrow (TF4).
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.drag = None;
            // No longer accepting text: the window turns the input method off.
            self.drop_preedit();
        }
    }

    /// `true` while the input method's composition is shown (TF6).
    #[must_use]
    pub fn composing(&self) -> bool {
        !self.preedit.is_empty()
    }

    /// The composition being shown (empty: none), and the input method's cursor inside it.
    #[must_use]
    pub fn preedit(&self) -> (&str, Option<Range<usize>>) {
        (&self.preedit, self.preedit_cursor.clone())
    }

    /// The text as shown: the editor's, with the composition in place of the selection while
    /// composing. Everything drawn and measured reads this; editing reads the editor's.
    fn shown(&self) -> &str {
        if self.composing() {
            &self.composed
        } else {
            self.editor.text()
        }
    }

    /// The composition's bytes inside [`TextField::shown`]: from the selection's start, as long
    /// as the composition; empty while not composing.
    fn preedit_span(&self) -> Range<usize> {
        let start = self.editor.selection().start;
        start..start.saturating_add(self.preedit.len())
    }

    /// Where the caret is drawn and kept in view, bytes into [`TextField::shown`], or `None`
    /// when the input method hides it: the editor's caret, or the input method's caret inside the
    /// composition (its end when the input method gives no caret, which is only used to scroll).
    fn shown_caret(&self) -> usize {
        if self.composing() {
            let span = self.preedit_span();
            span.start.saturating_add(
                self.preedit_cursor
                    .as_ref()
                    .map_or(self.preedit.len(), |c| c.end),
            )
        } else {
            self.editor.focus()
        }
    }

    /// `true` when the caret is drawn: always outside a composition, and inside one only when
    /// the input method gives an empty cursor (a non-empty one is a segment, underlined instead).
    fn caret_shown(&self) -> bool {
        !self.composing() || self.preedit_cursor.as_ref().is_some_and(Range::is_empty)
    }

    /// An offset of [`TextField::shown`] as an offset of the editor's text: identical outside a
    /// composition; inside it, the composition's start (where it would be inserted); after it,
    /// moved back by the composition and forward by the selection it hides.
    fn to_text_offset(&self, offset: usize) -> usize {
        if !self.composing() {
            return offset;
        }
        let span = self.preedit_span();
        if offset <= span.start {
            offset
        } else if offset < span.end {
            span.start
        } else {
            offset - span.end + self.editor.selection().end
        }
    }

    /// Rebuilds [`TextField::composed`] from the editor's text and the composition: the text
    /// before the selection, the composition, the text after it. O(n), no allocation once its
    /// capacity is reached.
    fn compose(&mut self) {
        let text = self.editor.text();
        let selection = self.editor.selection();
        self.composed.clear();
        // The selection's ends are always grapheme boundaries of the text (the editor's
        // invariant), so both `get`s succeed; an empty piece would only show less text.
        self.composed
            .push_str(text.get(..selection.start).unwrap_or_default());
        self.composed.push_str(&self.preedit);
        self.composed
            .push_str(text.get(selection.end..).unwrap_or_default());
    }

    /// Shows the input method's composition `text` with its cursor (TF6): the shown text is
    /// rebuilt and laid out again. An empty `text` ends the composition. Answers `false` when
    /// nothing changed (input methods repeat themselves).
    fn set_preedit(&mut self, text: &str, cursor: Option<Range<usize>>) -> bool {
        if self.preedit == text && self.preedit_cursor == cursor {
            return false;
        }
        self.preedit.clear();
        self.preedit.push_str(text);
        self.preedit_cursor = if text.is_empty() { None } else { cursor };
        self.text_stale = true;
        true
    }

    /// Drops the composition, if any, answering whether there was one; the text is laid out
    /// again (the caller asks for the arrange when it has a context).
    fn drop_preedit(&mut self) -> bool {
        let had = self.composing();
        if had {
            self.preedit.clear();
            self.preedit_cursor = None;
            self.text_stale = true;
        }
        had
    }

    /// An input method's event (TF6).
    fn ime(&mut self, cx: &mut EventCx<'_, M>, event: &ImeEvent) {
        match event {
            ImeEvent::Enabled => {}
            ImeEvent::Preedit { text, cursor } => {
                if self.set_preedit(text, cursor.clone()) {
                    cx.request_arrange();
                    self.restart_blink(cx);
                }
            }
            ImeEvent::Commit(text) => {
                // The composition ends with its commit (`winit` sends an empty preedit first,
                // but a missing one must not leave it shown over the inserted text).
                if self.drop_preedit() {
                    cx.request_arrange();
                }
                let response = self.editor.type_text(text);
                self.after(cx, response);
            }
            ImeEvent::Disabled => {
                if self.drop_preedit() {
                    cx.request_arrange();
                }
            }
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
            if self.composing() {
                self.compose();
            }
            // `shown` borrows the whole field; the two texts are read field by field here so the
            // layout can be borrowed mutably beside them.
            let shown = if self.composing() {
                &self.composed
            } else {
                self.editor.text()
            };
            if cx
                .lay_out_text(shown, STYLE, None, &mut self.layout)
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
        let text = self.shown();
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
        let caret = self.layout.caret_x(self.shown(), self.shown_caret());
        if caret - self.scroll < margin {
            self.scroll = caret - margin;
        }
        if caret - self.scroll > width - margin {
            self.scroll = caret - (width - margin);
        }
        self.scroll = self.scroll.clamp(0.0, content - width);
    }

    /// The caret offset nearest to `point` (window coordinates): where a press puts the caret.
    /// An offset of the editor's text: a point over a composition gives the composition's start.
    fn offset_at(&self, point: Point) -> usize {
        let x = point.x - self.text_left();
        // One line: the y only has to land in it.
        let y = (point.y - self.text_top()).clamp(0.0, self.layout.height().max(0.0) / 2.0);
        self.to_text_offset(self.layout.hit_test(self.shown(), x, y))
    }

    /// The grapheme **under** `point` (its start), rather than the nearest boundary: a double
    /// click on the right half of a word's last letter selects that word, not the space after it.
    fn grapheme_under(&self, point: Point) -> usize {
        let x = point.x - self.text_left();
        let y = (point.y - self.text_top()).clamp(0.0, self.layout.height().max(0.0) / 2.0);
        let shown = self.shown();
        let offset = self.layout.hit_test(shown, x, y);
        let under = if offset > 0 && self.layout.caret_x(shown, offset) > x {
            prev_grapheme(shown, offset)
        } else {
            offset
        };
        self.to_text_offset(under)
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

    /// What follows an editing action of the user's: what [`TextField::changed`] does, and the
    /// caret shows solid.
    fn after(&mut self, cx: &mut EventCx<'_, M>, response: EditResponse) {
        self.changed(cx, response);
        self.restart_blink(cx);
    }

    /// What follows an editing action of an assistive technology's (TF11): with the focus, the
    /// same as the user's own; without it there is no caret to show, and a changed text is
    /// committed at once, with the local history cleared as when the focus leaves (nothing else
    /// would ever commit it).
    fn assisted(&mut self, cx: &mut EventCx<'_, M>, response: EditResponse) {
        if self.focused {
            self.after(cx, response);
            return;
        }
        self.changed(cx, response);
        if response == EditResponse::TextChanged {
            self.commit(cx);
            self.editor.clear_history();
        }
    }

    /// The byte offset where the character number `character` of the accessibility description
    /// starts ([`for_each_character`]); the text's length at or past their number.
    ///
    /// # Complexity
    /// O(n): one walk over the graphemes, no allocation.
    fn character_offset(&self, character: usize) -> usize {
        let text = self.editor.text();
        let mut offset = text.len();
        let mut index = 0_usize;
        for_each_character(text, |piece, _| {
            if index == character {
                offset = piece.start;
            }
            index = index.saturating_add(1);
        });
        offset
    }

    /// The text as assistive technologies read it (TF11): see [`TextInfo`].
    ///
    /// # Complexity
    /// O(n log n) for n characters: one walk over the graphemes and the word segments, and a
    /// [`TextLayout::caret_x`] (a binary search) per character. It allocates the description: it
    /// runs only when the field's node is sent to a listening assistive technology.
    fn text_info(&self) -> TextInfo {
        // The text as shown, composition included (choice D5 of F2.10c2): what is on screen, with
        // the caret where it is drawn; a composition hides the selection, so there is none.
        let text = self.shown();
        let (anchor, focus) = if self.composing() {
            let caret = self.shown_caret();
            (caret, caret)
        } else {
            (self.editor.anchor(), self.editor.focus())
        };
        let left = self.text_left();
        let top = self.text_top();
        let mut info = TextInfo {
            value: text.to_owned(),
            rect: Rect::new(
                left,
                top,
                left + self.layout.caret_x(text, text.len()),
                top + self.layout.height(),
            ),
            ..TextInfo::default()
        };
        // The word starts (bytes, increasing) are turned into characters while walking: each is
        // the first character that starts at or after it.
        let mut words = word_starts(text).peekable();
        // The selection's ends likewise; `None` until their character is met (the text's end,
        // which no character starts at, is filled in after the walk).
        let mut anchor_at = None;
        let mut focus_at = None;
        // Where the grapheme being described starts: the end of the one before it.
        let mut pen = self.layout.caret_x(text, 0);
        for_each_character(text, |piece, grapheme| {
            let index = info.character_lengths.len();
            while words.next_if(|&start| start <= piece.start).is_some() {
                if info.word_starts.last() != Some(&index) {
                    info.word_starts.push(index);
                }
            }
            if anchor_at.is_none() && piece.start >= anchor {
                anchor_at = Some(index);
            }
            if focus_at.is_none() && piece.start >= focus {
                focus_at = Some(index);
            }
            // A piece is at most 255 bytes by construction; the fallback is never taken.
            info.character_lengths
                .push(u8::try_from(piece.len()).unwrap_or(u8::MAX));
            if piece.start == grapheme.start {
                // The grapheme's first (usually only) piece has its whole advance. It starts
                // where the grapheme before it ended, so one `caret_x` per grapheme is enough
                // (asking for both ends cost a third more per key with a reader listening).
                let start = pen;
                pen = self.layout.caret_x(text, grapheme.end);
                info.character_positions.push(start);
                info.character_widths.push(pen - start);
            } else {
                // The later pieces of a giant grapheme: nothing to show, at its end.
                info.character_positions.push(pen);
                info.character_widths.push(0.0);
            }
        });
        let characters = info.character_lengths.len();
        info.anchor = anchor_at.unwrap_or(characters);
        info.focus = focus_at.unwrap_or(characters);
        info
    }

    /// The part of an editing action common to the user and to assistive technologies: a changed
    /// text is laid out again (and announced to a live filter), a moved caret scrolled into view.
    fn changed(&mut self, cx: &mut EventCx<'_, M>, response: EditResponse) {
        match response {
            EditResponse::TextChanged => {
                self.text_stale = true;
                cx.request_arrange();
                if let Some(on_change) = self.on_change {
                    cx.emit(on_change(self.editor.text()));
                }
            }
            EditResponse::SelectionChanged => {
                // A composition is shown in place of the selection: it moved with it.
                if self.composing() {
                    self.text_stale = true;
                }
                cx.request_arrange();
            }
            // Nothing moved. (A clipboard action never arrives here: `clipboard` carried it out
            // and answered what it changed.)
            EditResponse::Unchanged | EditResponse::Clipboard(_) => {}
        }
    }

    /// Carries out Cut, Copy or Paste with the interface's clipboard (TF12, F2.10c1), and
    /// answers what changed in the field.
    ///
    /// - **Copy** writes the selected text; with nothing selected it does nothing, and the
    ///   clipboard keeps what it had.
    /// - **Cut** copies and then deletes the selection (one undo step). If the clipboard could
    ///   not be written the selection is **not** deleted: the text would be lost.
    /// - **Paste** replaces the selection with the clipboard's text, cleaned to one line (one
    ///   undo step, [`TextEditor::paste`]). An empty clipboard, or one holding no text, pastes
    ///   nothing.
    ///
    /// A clipboard that fails is ignored: nothing is shown to the user for now (precision of
    /// ADR-0047, F2.10c1).
    ///
    /// # Cost
    /// A paste allocates the clipboard's text and may wait for the program that owns it (an X11
    /// round trip); a copy allocates inside the clipboard. Neither is a hot path: they happen at
    /// a user's command.
    fn clipboard(&mut self, cx: &mut EventCx<'_, M>, action: EditAction) -> EditResponse {
        match action {
            EditAction::Copy | EditAction::Cut => {
                let selected = self.editor.selected_text();
                if selected.is_empty() || cx.clipboard().write_text(selected).is_err() {
                    return EditResponse::Unchanged;
                }
                if action == EditAction::Cut {
                    self.editor.delete_selection()
                } else {
                    EditResponse::Unchanged
                }
            }
            EditAction::Paste => match cx.clipboard().read_text() {
                Ok(Some(text)) => self.editor.paste(&text),
                Ok(None) | Err(_) => EditResponse::Unchanged,
            },
            // Every other action was carried out by the editor itself.
            _ => EditResponse::Unchanged,
        }
    }

    /// A key pressed while focused.
    fn key(&mut self, cx: &mut EventCx<'_, M>, key: &Key, modifiers: Modifiers) -> Handled {
        if let Some(action) = edit_action(key, modifiers, self.editor.convention()) {
            self.edit(cx, action);
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

    /// Carries out an editing action, from a key (TF3) or from an action of the registry
    /// (ADR-0048 AM13, [`WidgetEvent::Edit`]): the same path for both, so a menu's Undo and
    /// Ctrl+Z do exactly the same.
    fn edit(&mut self, cx: &mut EventCx<'_, M>, action: EditAction) {
        let response = match self.editor.apply(action) {
            // The editor has no clipboard: it hands these three back (F2.10b1).
            EditResponse::Clipboard(action) => self.clipboard(cx, action),
            response => response,
        };
        self.after(cx, response);
    }

    /// A left press at `point` with `clicks` (TF4).
    fn press(&mut self, cx: &mut EventCx<'_, M>, point: Point, clicks: u8) {
        let extend = cx.modifiers().shift && !self.fresh_focus;
        self.fresh_focus = false;
        // Where the press lands, read while the layout still shows a composition, if any.
        let offset = self.offset_at(point);
        let under = self.grapheme_under(point);
        // A press during a composition ends it (choice D3 of F2.10c2): the caret moves, and the
        // composition belonged to the old place. The window resets the input method, which
        // usually commits it at the new caret.
        if self.drop_preedit() {
            cx.reset_ime();
        }
        let response = match clicks {
            0 | 1 => {
                self.drag = Some(Drag::Graphemes);
                let anchor = if extend { self.editor.anchor() } else { offset };
                self.editor.select(anchor, offset)
            }
            2 => {
                let word = word_at(self.editor.text(), under);
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

    /// A secondary press at `point` (F2.11d2a, see the module documentation): inside the
    /// selection nothing changes; elsewhere, or right after the press gave the focus (whose
    /// "select everything" belongs to the keyboard), the caret goes where the press lands.
    /// Answers [`Handled::No`], always: the interface opens the context menu after it.
    fn secondary_press(&mut self, cx: &mut EventCx<'_, M>, point: Point) -> Handled {
        let fresh = std::mem::take(&mut self.fresh_focus);
        let offset = self.offset_at(point);
        let selection = self.editor.selection();
        // Inside, edges included: a press on the selection's edge keeps it too.
        let inside = !selection.is_empty() && (selection.start..=selection.end).contains(&offset);
        if !inside || fresh {
            if self.drop_preedit() {
                cx.reset_ime();
            }
            let response = self.editor.select(offset, offset);
            self.after(cx, response);
        }
        Handled::No
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

    /// A request of an assistive technology to change the text or the selection (TF11): each
    /// goes through the editor as a paste or a selection would, then [`TextField::assisted`].
    fn assistive(&mut self, cx: &mut EventCx<'_, M>, event: &WidgetEvent<'_>) -> Handled {
        match *event {
            WidgetEvent::SetText(text) => {
                // A paste over everything: one line, one undo step (TF11).
                let selected = self.editor.select(0, self.editor.text().len());
                let pasted = self.editor.paste(text);
                // If the paste changed nothing (an empty text set on an empty field), the
                // selection may still have moved.
                let response = if pasted == EditResponse::TextChanged {
                    pasted
                } else {
                    selected
                };
                self.assisted(cx, response);
                Handled::Yes
            }
            WidgetEvent::ReplaceSelectedText(text) => {
                let response = self.editor.paste(text);
                self.assisted(cx, response);
                Handled::Yes
            }
            WidgetEvent::SetTextSelection { anchor, focus } => {
                let response = self
                    .editor
                    .select(self.character_offset(anchor), self.character_offset(focus));
                self.assisted(cx, response);
                Handled::Yes
            }
            _ => Handled::No,
        }
    }

    /// Records the lines under a composition (TF6, choice D6 of F2.10c2): one pixel under all of
    /// it, two under the segment the input method converts, in the text's colour, below the
    /// baseline and clipped to the text's `area`.
    fn record_preedit_lines(
        &self,
        origin: Point,
        area: Rect,
        color: ColorToken,
        recorder: &mut Recorder<'_>,
    ) {
        let text = self.shown();
        let span = self.preedit_span();
        let baseline = self
            .layout
            .lines()
            .first()
            .map_or(0.0, |line| line.baseline);
        let y = origin.y + baseline + PREEDIT_LINE_GAP;
        let x_at = |offset: usize| origin.x + self.layout.caret_x(text, offset);
        let line =
            Rect::new(x_at(span.start), y, x_at(span.end), y + PREEDIT_LINE).intersect(&area);
        if !line.is_empty() {
            recorder.fill_rect(line, color);
        }
        if let Some(cursor) = self.preedit_cursor.as_ref().filter(|c| !c.is_empty()) {
            let segment = Rect::new(
                x_at(span.start + cursor.start),
                y,
                x_at(span.start + cursor.end),
                y + PREEDIT_SEGMENT_LINE,
            )
            .intersect(&area);
            if !segment.is_empty() {
                recorder.fill_rect(segment, color);
            }
        }
    }

    /// The caret's rectangle, logical pixels of the window, kept inside the text's area: at the
    /// very end of a scrolled text the caret's bar would otherwise touch the padding.
    fn caret_rect(&self) -> Rect {
        let x = self.text_left() + self.layout.caret_x(self.shown(), self.shown_caret());
        let x = x.clamp(
            self.inner.x0,
            (self.inner.x1 - CARET_WIDTH).max(self.inner.x0),
        );
        let top = self.text_top();
        Rect::new(x, top, x + CARET_WIDTH, top + self.layout.height())
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

/// The longest character AccessKit can describe, bytes: it keeps a character's length in a `u8`.
const MAX_CHARACTER_BYTES: usize = u8::MAX as usize;

/// Calls `f` for every **character** of `text` as assistive technologies count them (TF11,
/// precision of F2.10b3), in order, with the character's bytes and the bytes of the grapheme it
/// belongs to.
///
/// A character is a grapheme, and then both ranges are equal. A grapheme longer than
/// [`MAX_CHARACTER_BYTES`] (a letter under a pile of combining marks) is given as several
/// characters of at most that length, cut at code points: the first starts where the grapheme
/// does. Consecutive characters cover the whole text; an empty text has none.
///
/// # Complexity
/// O(n), one pass of the grapheme segmenter, no allocation.
fn for_each_character(text: &str, mut f: impl FnMut(Range<usize>, Range<usize>)) {
    let mut start = 0_usize;
    // The boundaries start with 0, which ends no grapheme.
    for end in grapheme_boundaries(text).skip(1) {
        let grapheme = start..end;
        let mut piece_start = start;
        while piece_start < end {
            // The furthest code-point boundary within the limit. A code point is at most four
            // bytes, so it is always past `piece_start`; the `max` only guards the loop's end.
            let limit = piece_start.saturating_add(MAX_CHARACTER_BYTES).min(end);
            let piece_end = text
                .floor_char_boundary(limit)
                .max(piece_start.saturating_add(1));
            f(piece_start..piece_end, grapheme.clone());
            piece_start = piece_end;
        }
        start = end;
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
        let text = self.shown();
        if text.is_empty() {
            recorder.text(&self.placeholder_layout, origin, placeholder_color);
        } else if self.composing() {
            // A composition, shown in place of the selection: the text, then its lines (TF6).
            recorder.text(&self.layout, origin, text_color);
            self.record_preedit_lines(origin, area, text_color, recorder);
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
        // The caret: only with the focus, in its visible half, and with nothing selected; during
        // a composition the selection is hidden, and the caret is the input method's, if any.
        let unselected = self.composing() || self.editor.selection().is_empty();
        if !(cx.focused && self.enabled && self.caret_on && unselected && self.caret_shown()) {
            return;
        }
        recorder.fill_rect(self.caret_rect(), ColorToken::Text);
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
        // While a composition is shown, the keys belong to the input method (choice D1 of
        // F2.10c2): none edits, commits or moves the focus, pressed or released.
        if self.composing() && matches!(event, WidgetEvent::Key { .. }) {
            return Handled::Yes;
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
                // A composition ends with the focus (choice D4 of F2.10c2); the window turns the
                // input method off, and a commit that still comes goes to the new focus.
                self.drop_preedit();
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
            // Left unhandled after moving the caret: the interface then opens the context menu.
            WidgetEvent::PointerButton {
                button: PointerButton::Right,
                state: ButtonState::Pressed,
                position,
                ..
            } => self.secondary_press(cx, position),
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
            WidgetEvent::Ime(ime) => {
                self.ime(cx, ime);
                Handled::Yes
            }
            WidgetEvent::SetText(_)
            | WidgetEvent::ReplaceSelectedText(_)
            | WidgetEvent::SetTextSelection { .. } => self.assistive(cx, event),
            // A standard editing action of the registry (ADR-0048 AM13), sent only after
            // `edit_state` said it can be done now.
            WidgetEvent::Edit(role) => {
                self.edit(cx, edit_action_of(role));
                Handled::Yes
            }
            _ => Handled::No,
        }
    }

    fn focusable(&self) -> bool {
        self.enabled
    }

    fn accepts_text(&self) -> bool {
        self.enabled
    }

    fn edit_state(&self, role: EditRole) -> Option<bool> {
        // A disabled field never has the focus, but answers nothing then, to be safe: the action
        // is not its business.
        if !self.enabled {
            return None;
        }
        // With the focus in the field, the six act on its text and its local undo only (AM13,
        // ADR-0047 TF7): Undo with an empty local history is disabled, never the document's.
        Some(match role {
            EditRole::Cut | EditRole::Copy => !self.editor.selection().is_empty(),
            // Whether the clipboard has text is not known without asking the system (which
            // would mean a round trip for every state read); an empty paste does nothing.
            EditRole::Paste | EditRole::SelectAll => true,
            EditRole::Undo => self.editor.can_undo(),
            EditRole::Redo => self.editor.can_redo(),
        })
    }

    fn context_menu(&self) -> Option<&crate::MenuModel> {
        // A disabled field takes no focus, so its editing items would have nothing to act on.
        self.context_menu.as_ref().filter(|_| self.enabled)
    }

    fn ime_area(&self) -> Option<Rect> {
        // The caret, even where it is not drawn (selected text, a segment, the blink's off
        // half): the candidates open beside where the text is being typed.
        Some(self.caret_rect())
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
        info.text = Some(self.text_info());
        if !self.placeholder.is_empty() {
            info.placeholder = Some(self.placeholder.clone());
        }
    }
}

/// The field's editing action for a standard editing action of the registry (AM13).
const fn edit_action_of(role: EditRole) -> EditAction {
    match role {
        EditRole::Cut => EditAction::Cut,
        EditRole::Copy => EditAction::Copy,
        EditRole::Paste => EditAction::Paste,
        EditRole::SelectAll => EditAction::SelectAll,
        EditRole::Undo => EditAction::Undo,
        EditRole::Redo => EditAction::Redo,
    }
}
