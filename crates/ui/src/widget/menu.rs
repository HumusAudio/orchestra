// SPDX-License-Identifier: GPL-3.0-or-later

//! Menus (F2.11d2a, ADR-0048 AM9, AM11, AM12): the one menu model, and the two widgets a drawn
//! menu is made of, the panel and its entries.
//!
//! # The model (AM9)
//! A menu is described **once**, as a tree of [`MenuItem`]s: an **action** (its name, shortcut,
//! state and check come from the registry, so a menu never holds a copy that could go stale), a
//! **submenu** (a name and its own items) and a **separator**. Context menus use it today; the
//! drawn menu bar (F2.11d2b) and `muda` on macOS (F2.11e) will be built from the same model.
//! Some items have a [`MenuRole`], which only macOS uses (F2.11e): elsewhere an item stays where
//! the model puts it.
//!
//! # The widgets
//! The interface builds a drawn menu from a model when it opens it
//! ([`Ui::open_menu_model`](crate::Ui::open_menu_model)): a [`MenuPanel`] as the popup's root and
//! one [`MenuEntry`] per item as its children. **Each entry is a widget of its own**, with its
//! own display-list segment, so moving the highlight re-records two entries and nothing else
//! (ADR-0048 AM15). The entries do not handle events: the interface owns what a menu does (the
//! highlight, the keys, choosing, submenus), as it owns the tooltip (F2.11c), and only tells an
//! entry how to look. That keeps the menus' behaviour in one place, next to the routing it
//! changes (a press outside, the focus that stays).
//!
//! # Looks
//! - The panel: `surface-overlay` with a `border` line of `line-width` and the theme's
//!   `corner-radius`, **without a shadow** (owner's choice B in F2.11d2a, as for the tooltip in
//!   F2.11c). Measured on the 200-widget panel, the overlay shadow was the whole cost of a menu:
//!   opening the context menu where none was drawn before took 27.1 M instructions with it and
//!   4.6 M without (the renderer caches a shadow by its exact place, ADR-0043 K2, and a context
//!   menu opens at the pointer), and every move of the highlight 0.82 M against 0.17 M (the
//!   shadow under the opaque panel was drawn again with each redrawn item). Screenshots of both
//!   appearances showed a soft effect in the light half and almost none in the dark one: the
//!   border and the overlay's own colour set the menu apart. A small padding (the density's
//!   extra-small gap) above and below the entries; at least [`MIN_WIDTH`] wide.
//! - An entry: `control-height` high, its name at the left and its shortcut at the right
//!   (`text-muted`), or an arrow for a submenu. The highlighted entry has a rounded
//!   `control-hover` background (the token's own definition: "the highlighted item of a menu")
//!   and its shortcut in `text`, because `text-muted` on `control-hover` is not one of the
//!   contrast pairs every theme is checked against (ADR-0037 T4). A disabled entry is drawn in
//!   `text-disabled` and is never highlighted. A check mark in a column at the left when the menu
//!   has a toggle.
//! - A separator: a `border` line of `line-width` across, with the extra-small gap above and
//!   below.
//!
//! # Accessibility (AM12)
//! The panel is a `Menu`; an entry a `MenuItem`, or a `MenuItemCheckBox` with its check, with its
//! shortcut as `keyboard_shortcut`, its place among the items ("3 of 7") and, for a submenu,
//! that it opens a menu (`has_popup`) and whether it is open. A separator is hidden from assistive technologies, which
//! count the items without it.

use orchestra_text::TextLayout;

use crate::action::registry::{ActionId, ActionState};
use crate::draw::geometry::{Point, Rect, RoundedRect};
use crate::draw::list::Recorder;
use crate::draw::paint::StrokeStyle;
use crate::theme::text::{FamilyToken, SizeToken, Weight};
use crate::theme::token::ColorToken;

use super::access::{AccessInfo, AccessRole};
use super::layout::{Constraints, Size};
use super::tree::{LayoutCx, RecordCx, Widget};

/// The text style of a menu: the interface family, body size, regular weight (the same as the
/// controls', so a menu reads like the window it belongs to).
const STYLE: (FamilyToken, SizeToken, Weight) = (FamilyToken::Ui, SizeToken::Body, Weight::Regular);

/// The narrowest a menu panel is, logical pixels: a menu of two short items ("Cut", "Copy") is
/// still a comfortable target. GNOME's and Windows' menus are about as narrow at their narrowest.
pub(crate) const MIN_WIDTH: f32 = 160.0;

/// The menu's role for macOS (ADR-0048 AM9): where the system wants the item. Ignored on Linux
/// and Windows, where items stay where the model puts them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MenuRole {
    /// *About*: goes into the application menu.
    About,
    /// *Preferences*: goes into the application menu (on Linux and Windows it stays in Edit,
    /// owner's choice in AM9).
    Preferences,
    /// *Quit*: goes into the application menu.
    Quit,
    /// The *Window* submenu: the system adds its own window list to it.
    Window,
    /// The *Help* submenu: the system adds its search field to it.
    Help,
}

/// One item of a menu (ADR-0048 AM9).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuItem {
    /// An action of the registry: its name, shortcut, state and check are the registry's.
    Action {
        /// The action.
        action: ActionId,
        /// Its role on macOS, if any.
        role: Option<MenuRole>,
    },
    /// A submenu.
    Submenu {
        /// Its name, already localised by the application (CLAUDE.md §7).
        name: String,
        /// Its mnemonic (ADR-0048 AM10, F2.11d2b): a letter of `name`, chosen by the translator
        /// (the message's `.mnemonic` attribute); `None` for none.
        mnemonic: Option<char>,
        /// Its role on macOS, if any.
        role: Option<MenuRole>,
        /// Its items.
        items: MenuModel,
    },
    /// A line between groups of items.
    Separator,
}

/// A menu: its items, in order (ADR-0048 AM9). Built by the application, opened by the
/// interface ([`Ui::open_menu_model`](crate::Ui::open_menu_model)) or given by a widget as its
/// context menu ([`Widget::context_menu`]).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MenuModel {
    /// The items.
    pub items: Vec<MenuItem>,
}

impl MenuModel {
    /// An empty menu.
    #[must_use]
    pub const fn new() -> Self {
        Self { items: Vec::new() }
    }

    /// The same menu with the action `action` added at the end.
    #[must_use]
    pub fn action(mut self, action: ActionId) -> Self {
        self.items.push(MenuItem::Action { action, role: None });
        self
    }

    /// The same menu with `role` given to its **last** item (an action or a submenu; nothing for
    /// a separator or an empty menu): where macOS puts it (ADR-0048 AM9, F2.11e1). Elsewhere a role
    /// changes nothing.
    #[must_use]
    pub fn role(mut self, role: MenuRole) -> Self {
        match self.items.last_mut() {
            Some(MenuItem::Action { role: own, .. } | MenuItem::Submenu { role: own, .. }) => {
                *own = Some(role);
            }
            Some(MenuItem::Separator) | None => {}
        }
        self
    }

    /// The same menu with a separator added at the end.
    #[must_use]
    pub fn separator(mut self) -> Self {
        self.items.push(MenuItem::Separator);
        self
    }

    /// The same menu with the submenu `items`, named `name` (already localised) with its
    /// `mnemonic` (F2.11d2b), added at the end. A menu bar is a model whose items are such
    /// submenus, one per title ([`Ui::add_menu_bar`](crate::Ui::add_menu_bar)).
    #[must_use]
    pub fn submenu(mut self, name: impl Into<String>, mnemonic: Option<char>, items: Self) -> Self {
        self.items.push(MenuItem::Submenu {
            name: name.into(),
            mnemonic,
            role: None,
            items,
        });
        self
    }
}

/// The root of a drawn menu: draws the box and stacks its entries (see the module
/// documentation).
#[derive(Debug, Default)]
pub(crate) struct MenuPanel;

/// The panel's padding above the first entry and below the last, and on each side of the
/// entries' highlight, logical pixels: the density's extra-small gap.
fn panel_padding(spacing: crate::theme::density::Spacing) -> f32 {
    spacing.gap_xs
}

impl<M: 'static> Widget<M> for MenuPanel {
    fn measure(&mut self, cx: &mut LayoutCx<'_, M>, constraints: Constraints) -> Size {
        let pad = panel_padding(cx.spacing());
        // Each entry at its natural size; the panel is as wide as the widest one (so the
        // shortcuts line up at the right edge) and as high as all of them together.
        let mut width = MIN_WIDTH;
        let mut height = 2.0 * pad;
        for child in 0..cx.child_count() {
            let size = cx.measure_child(child, Constraints::loose(constraints.max));
            width = width.max(size.width);
            height += size.height;
        }
        constraints.constrain(Size::new(width, height))
    }

    fn arrange(&mut self, cx: &mut LayoutCx<'_, M>, rect: Rect) {
        let pad = panel_padding(cx.spacing());
        let mut y = rect.y0 + pad;
        for child in 0..cx.child_count() {
            let height = cx.child_size(child).height;
            // Every entry spans the panel's whole width: its highlight and its shortcut reach
            // the same right edge.
            cx.place_child(child, Rect::new(rect.x0, y, rect.x1, y + height));
            y += height;
        }
    }

    fn record(&self, cx: &RecordCx, recorder: &mut Recorder<'_>) {
        let rect = cx.rect;
        let radius = f32::from(cx.shape.corner_radius);
        // No shadow (owner's choice B in F2.11d2a, after the measures and the screenshots of
        // both appearances): see the module documentation.
        recorder.fill_rounded_rect(RoundedRect::new(rect, radius), ColorToken::SurfaceOverlay);
        let width = f32::from(cx.shape.line_width.max(1));
        recorder.stroke_rounded_rect(
            RoundedRect::new(rect.inflate(-width / 2.0), (radius - width / 2.0).max(0.0)),
            StrokeStyle::new(width),
            ColorToken::Border,
        );
    }

    fn access_role(&self) -> AccessRole {
        AccessRole::Menu
    }
}

/// What an entry is (see [`MenuEntry`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum EntryKind {
    /// An action of the registry, with its state as the user sees it (ADR-0048 AM13).
    Action {
        /// The action.
        action: ActionId,
        /// Its state when the entry was last brought up to date.
        state: ActionState,
    },
    /// A submenu: its items, opened when the entry is chosen.
    Submenu {
        /// The submenu's items.
        items: MenuModel,
        /// `true` while its submenu is open.
        open: bool,
    },
    /// A separator.
    Separator,
}

/// One item of a drawn menu (see the module documentation). Built and changed by the interface
/// only.
#[derive(Debug)]
pub(crate) struct MenuEntry {
    /// What it is.
    kind: EntryKind,
    /// Its name, as people read it (from the registry or the model).
    name: String,
    /// Its shortcut, as people read it (empty for none, for a submenu or a separator).
    shortcut: String,
    /// The laid-out name.
    name_layout: TextLayout,
    /// The laid-out shortcut.
    shortcut_layout: TextLayout,
    /// The text generation the layouts were made for; `None` after a text changed.
    laid_out_for: Option<u64>,
    /// `true` while the entry is highlighted (the pointer is over it, or the keyboard moved to
    /// it).
    highlighted: bool,
    /// `true` when the menu has a toggle: every entry then keeps a column for the check mark, so
    /// the names line up.
    check_column: bool,
    /// Its place among the menu's items that are not separators, from 1, and their number: for
    /// assistive technologies ("3 of 7").
    position: (usize, usize),
    /// Its mnemonic, in lower case (F2.11d2b, ADR-0048 AM10).
    mnemonic: Option<char>,
    /// `true` while the mnemonic is underlined: Alt is held, or the menus were reached from the
    /// keyboard (AM10). The interface sets it on every open entry when it changes.
    show_mnemonic: bool,
}

impl MenuEntry {
    /// An entry of `kind` named `name`, showing `shortcut`, chosen with `mnemonic`.
    pub(crate) fn new(
        kind: EntryKind,
        name: String,
        shortcut: String,
        mnemonic: Option<char>,
    ) -> Self {
        Self {
            kind,
            name,
            shortcut,
            mnemonic,
            show_mnemonic: false,
            name_layout: TextLayout::new(),
            shortcut_layout: TextLayout::new(),
            laid_out_for: None,
            highlighted: false,
            check_column: false,
            position: (0, 0),
        }
    }

    /// What it is.
    pub(crate) const fn kind(&self) -> &EntryKind {
        &self.kind
    }

    /// Its mnemonic, in lower case (F2.11d2b).
    pub(crate) const fn mnemonic(&self) -> Option<char> {
        self.mnemonic
    }

    /// Underlines its mnemonic or not; `true` when that changed (the caller records it again).
    pub(crate) const fn set_show_mnemonic(&mut self, show: bool) -> bool {
        let changed = self.show_mnemonic != show && self.mnemonic.is_some();
        self.show_mnemonic = show;
        changed
    }

    /// `true` when it can be highlighted and chosen: an enabled action, or a submenu. Separators
    /// and disabled actions are skipped by the keyboard and ignored by the pointer (AM12).
    pub(crate) const fn selectable(&self) -> bool {
        match &self.kind {
            EntryKind::Action { state, .. } => state.enabled,
            EntryKind::Submenu { .. } => true,
            EntryKind::Separator => false,
        }
    }

    /// The action it runs, if it is an action.
    pub(crate) const fn action(&self) -> Option<ActionId> {
        match &self.kind {
            EntryKind::Action { action, .. } => Some(*action),
            _ => None,
        }
    }

    /// Highlights it or not; `true` when that changed (the caller records it again).
    pub(crate) const fn set_highlighted(&mut self, highlighted: bool) -> bool {
        let changed = self.highlighted != highlighted;
        self.highlighted = highlighted;
        changed
    }

    /// Marks a submenu entry as open or closed (assistive technologies say so); `true` when that
    /// changed.
    pub(crate) const fn set_open(&mut self, open: bool) -> bool {
        match &mut self.kind {
            EntryKind::Submenu { open: was, .. } if *was != open => {
                *was = open;
                true
            }
            _ => false,
        }
    }

    /// Keeps a column for the check mark, and sets its place among the items.
    pub(crate) const fn set_place(&mut self, check_column: bool, position: (usize, usize)) {
        self.check_column = check_column;
        self.position = position;
    }

    /// Brings an action entry up to date with its action's `state`, `name` and `shortcut` (AM2:
    /// the registry marked it as changed). Answers what the interface must redo: `Some(true)` to
    /// lay it out again (a text changed), `Some(false)` to record it again (only its look), `None`
    /// when nothing changed.
    pub(crate) fn update(
        &mut self,
        state: ActionState,
        name: &str,
        shortcut: &str,
        mnemonic: Option<char>,
    ) -> Option<bool> {
        let EntryKind::Action { state: own, .. } = &mut self.kind else {
            return None;
        };
        let look = *own != state || self.mnemonic != mnemonic;
        *own = state;
        self.mnemonic = mnemonic;
        let text = name != self.name || shortcut != self.shortcut;
        if text {
            name.clone_into(&mut self.name);
            shortcut.clone_into(&mut self.shortcut);
            self.laid_out_for = None;
        }
        // A disabled entry is never highlighted (AM12).
        if !state.enabled {
            self.highlighted = false;
        }
        match (text, look) {
            (true, _) => Some(true),
            (false, true) => Some(false),
            (false, false) => None,
        }
    }

    /// The width of the check-mark column, logical pixels: a square as tall as the text's line,
    /// plus the small gap after it.
    fn check_width(&self, spacing: crate::theme::density::Spacing) -> f32 {
        if self.check_column {
            self.name_layout.height() + spacing.gap_s
        } else {
            0.0
        }
    }

    /// The width of a submenu's arrow, logical pixels: half the text's line.
    fn arrow_width(&self) -> f32 {
        self.name_layout.height() / 2.0
    }
}

impl<M: 'static> Widget<M> for MenuEntry {
    fn measure(&mut self, cx: &mut LayoutCx<'_, M>, constraints: Constraints) -> Size {
        let spacing = cx.spacing();
        if self.kind == EntryKind::Separator {
            // A line with the extra-small gap above and below it; as narrow as anything.
            return constraints.constrain(Size::new(0.0, 2.0 * spacing.gap_xs + 1.0));
        }
        let generation = cx.text_generation();
        if self.laid_out_for != Some(generation) {
            for (text, layout) in [
                (&self.name, &mut self.name_layout),
                (&self.shortcut, &mut self.shortcut_layout),
            ] {
                if cx.lay_out_text(text, STYLE, None, layout).is_err() {
                    // Impossible with the bundled faces; an empty text keeps the menu usable.
                    *layout = TextLayout::new();
                }
            }
            self.laid_out_for = Some(generation);
        }
        let pad = panel_padding(spacing);
        // The right side: the shortcut, or a submenu's arrow, after a large gap from the name.
        let right = match self.kind {
            EntryKind::Submenu { .. } => spacing.gap_l + self.arrow_width(),
            _ if self.shortcut.is_empty() => 0.0,
            _ => spacing.gap_l + self.shortcut_layout.width(),
        };
        let width = 2.0 * (pad + spacing.control_padding)
            + self.check_width(spacing)
            + self.name_layout.width()
            + right;
        let height = spacing.control_height.max(self.name_layout.height());
        constraints.constrain(Size::new(width, height))
    }

    fn record(&self, cx: &RecordCx, recorder: &mut Recorder<'_>) {
        let rect = cx.rect;
        let spacing = cx.spacing;
        let pad = panel_padding(spacing);
        let line = f32::from(cx.shape.line_width.max(1));
        if self.kind == EntryKind::Separator {
            let y = f32::midpoint(rect.y0, rect.y1);
            recorder.line(
                Point::new(rect.x0 + pad, y),
                Point::new(rect.x1 - pad, y),
                StrokeStyle::new(line),
                ColorToken::Border,
            );
            return;
        }
        let enabled = self.selectable();
        if self.highlighted && enabled {
            // Inside the panel's padding on both sides, with the theme's corners, as GNOME and
            // Windows 11 draw a highlighted item.
            let radius = f32::from(cx.shape.corner_radius);
            recorder.fill_rounded_rect(
                RoundedRect::new(
                    Rect::new(rect.x0 + pad, rect.y0, rect.x1 - pad, rect.y1),
                    radius,
                ),
                ColorToken::ControlHover,
            );
        }
        let (text, muted) = match (enabled, self.highlighted) {
            (false, _) => (ColorToken::TextDisabled, ColorToken::TextDisabled),
            // `text-muted` on `control-hover` is not a checked contrast pair (ADR-0037 T4).
            (true, true) => (ColorToken::Text, ColorToken::Text),
            (true, false) => (ColorToken::Text, ColorToken::TextMuted),
        };
        let left = rect.x0 + pad + spacing.control_padding;
        let right = rect.x1 - pad - spacing.control_padding;
        let middle = f32::midpoint(rect.y0, rect.y1);
        let text_top = middle - self.name_layout.height() / 2.0;
        if let EntryKind::Action {
            state:
                ActionState {
                    checked: Some(true),
                    ..
                },
            ..
        } = self.kind
        {
            // A check mark in its column: two strokes, like "✓", inside a square as tall as
            // the text's line.
            let size = self.name_layout.height();
            let (x, y) = (left, text_top);
            recorder.stroke_path(StrokeStyle::new(line.max(1.5)), text, |path| {
                path.move_to(x + size * 0.2, y + size * 0.52);
                path.line_to(x + size * 0.42, y + size * 0.74);
                path.line_to(x + size * 0.82, y + size * 0.28);
            });
        }
        let name_x = left + self.check_width(spacing);
        recorder.text(&self.name_layout, Point::new(name_x, text_top), text);
        if self.show_mnemonic
            && let Some(letter) = self.mnemonic
        {
            underline(
                recorder,
                &self.name,
                &self.name_layout,
                letter,
                Point::new(name_x, text_top),
                text,
            );
        }
        match self.kind {
            EntryKind::Submenu { .. } => {
                // A small triangle pointing right, centred on the text's middle.
                let half = self.arrow_width() / 2.0;
                let x = right - self.arrow_width();
                recorder.fill_path(crate::draw::paint::FillRule::NonZero, muted, |path| {
                    path.move_to(x + half * 0.5, middle - half);
                    path.line_to(x + half * 1.5, middle);
                    path.line_to(x + half * 0.5, middle + half);
                    path.close();
                });
            }
            _ if !self.shortcut.is_empty() => {
                let x = right - self.shortcut_layout.width();
                let y = middle - self.shortcut_layout.height() / 2.0;
                recorder.text(&self.shortcut_layout, Point::new(x, y), muted);
            }
            _ => {}
        }
    }

    fn access_role(&self) -> AccessRole {
        match self.kind {
            EntryKind::Action {
                state: ActionState {
                    checked: Some(_), ..
                },
                ..
            } => AccessRole::MenuItemCheckBox,
            EntryKind::Action { .. } | EntryKind::Submenu { .. } => AccessRole::MenuItem,
            EntryKind::Separator => AccessRole::Separator,
        }
    }

    fn access_info(&self, info: &mut AccessInfo) {
        if self.kind == EntryKind::Separator {
            return;
        }
        info.label = Some(self.name.clone());
        info.disabled = !self.selectable();
        info.clickable = true;
        if !self.shortcut.is_empty() {
            info.keyboard_shortcut = Some(self.shortcut.clone());
        }
        info.position = Some(self.position);
        info.access_key = self.mnemonic;
        match &self.kind {
            EntryKind::Action { state, .. } => info.toggled = state.checked,
            EntryKind::Submenu { open, .. } => info.expanded = Some(*open),
            EntryKind::Separator => {}
        }
    }
}

/// Underlines the mnemonic `letter` in `text`, laid out as `layout` and drawn with its top-left
/// corner at `origin`, in `paint` (F2.11d2b, ADR-0048 AM10): a line of one logical pixel, one
/// pixel below the baseline, under the character of `text` that is `letter` in either case.
///
/// # Which occurrence
/// The first one **at the start of a word**, otherwise the first one: "Save **a**s…" and not
/// "S**a**ve as…", "Salva **c**ome…", "Speichern **u**nter…", "Plein **é**cran", as Windows and
/// the translated catalogues of other music programs place it (the glossary's `menu_mnemonics`).
/// Nothing is drawn when the text has no such character (the catalogues' test prevents it).
///
/// # Cost
/// O(characters of the text) to find the letter, O(log stops) for its two edges; no allocation.
fn underline(
    recorder: &mut Recorder<'_>,
    text: &str,
    layout: &TextLayout,
    letter: char,
    origin: Point,
    paint: ColorToken,
) {
    let matches =
        |character: char| character == letter || character.to_lowercase().eq(letter.to_lowercase());
    // Each character with whether it starts a word (the text's start, or after a character
    // that is not alphanumeric).
    let mut previous: Option<char> = None;
    let mut first = None;
    let mut at_word = None;
    for (index, character) in text.char_indices() {
        let starts_word = previous.is_none_or(|before| !before.is_alphanumeric());
        previous = Some(character);
        if !matches(character) {
            continue;
        }
        first.get_or_insert((index, character));
        if starts_word {
            at_word = Some((index, character));
            break;
        }
    }
    let Some((start, character)) = at_word.or(first) else {
        return;
    };
    let end = start + character.len_utf8();
    let (x0, x1) = (layout.caret_x(text, start), layout.caret_x(text, end));
    let baseline = layout.lines().first().map_or(0.0, |line| line.baseline);
    let y = (origin.y + baseline + 1.5).round() - 0.5;
    recorder.line(
        Point::new(origin.x + x0, y),
        Point::new(origin.x + x1, y),
        StrokeStyle::new(1.0),
        paint,
    );
}

/// The drawn menu bar (F2.11d2b, ADR-0048 AM9, AM12): a row of [`BarTitle`]s across the top of
/// the window, on Linux and Windows (macOS has its own bar through `muda`, F2.11e). Like a menu,
/// it handles no events itself: the interface opens and switches its menus
/// ([`Ui::add_menu_bar`](crate::Ui::add_menu_bar)).
///
/// # Looks
/// `control-height` high and as wide as it is given, on `surface` with a `border` line along its
/// bottom edge; the titles start after the density's extra-small gap.
#[derive(Debug, Default)]
pub(crate) struct MenuBar;

impl<M: 'static> Widget<M> for MenuBar {
    fn measure(&mut self, cx: &mut LayoutCx<'_, M>, constraints: Constraints) -> Size {
        let spacing = cx.spacing();
        let mut height = spacing.control_height;
        for child in 0..cx.child_count() {
            let size = cx.measure_child(child, Constraints::loose(constraints.max));
            height = height.max(size.height);
        }
        // As wide as it may be (it spans the window); as high as a control.
        constraints.constrain(Size::new(constraints.max.width, height))
    }

    fn arrange(&mut self, cx: &mut LayoutCx<'_, M>, rect: Rect) {
        let mut x = rect.x0 + cx.spacing().gap_xs;
        for child in 0..cx.child_count() {
            let width = cx.child_size(child).width;
            cx.place_child(child, Rect::new(x, rect.y0, x + width, rect.y1));
            x += width;
        }
    }

    fn record(&self, cx: &RecordCx, recorder: &mut Recorder<'_>) {
        let rect = cx.rect;
        recorder.fill_rect(rect, ColorToken::Surface);
        let width = f32::from(cx.shape.line_width.max(1));
        let y = rect.y1 - width / 2.0;
        recorder.line(
            Point::new(rect.x0, y),
            Point::new(rect.x1, y),
            StrokeStyle::new(width),
            ColorToken::Border,
        );
    }

    fn access_role(&self) -> AccessRole {
        AccessRole::MenuBar
    }
}

/// One title of the menu bar (F2.11d2b): its name, with the mnemonic underlined while it is
/// shown; a rounded `control-hover` background while the pointer is over it, the keyboard is on
/// it, or its menu is open.
#[derive(Debug)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "four independent looks (hovered, highlighted, open, mnemonic shown) set by different \
              events; an enum would have to list their combinations"
)]
pub(crate) struct BarTitle {
    /// The name, already localised.
    name: String,
    /// The laid-out name.
    layout: TextLayout,
    /// The text generation the layout was made for.
    laid_out_for: Option<u64>,
    /// Its mnemonic, in lower case.
    mnemonic: Option<char>,
    /// The pointer is over it.
    hovered: bool,
    /// The keyboard is on it (the bar was reached with F10 or Alt).
    highlighted: bool,
    /// Its menu is open.
    open: bool,
    /// The mnemonic is underlined.
    show_mnemonic: bool,
}

impl BarTitle {
    /// A title named `name`, opened with `mnemonic`.
    pub(crate) const fn new(name: String, mnemonic: Option<char>) -> Self {
        Self {
            name,
            layout: TextLayout::new(),
            laid_out_for: None,
            mnemonic,
            hovered: false,
            highlighted: false,
            open: false,
            show_mnemonic: false,
        }
    }

    /// Its mnemonic, in lower case.
    pub(crate) const fn mnemonic(&self) -> Option<char> {
        self.mnemonic
    }

    /// Sets what the interface decides about it: the keyboard on it, its menu open, its mnemonic
    /// shown. `true` when its look changed (the caller records it again).
    pub(crate) const fn set_state(&mut self, highlighted: bool, open: bool, show: bool) -> bool {
        let show = show && self.mnemonic.is_some();
        let changed =
            self.highlighted != highlighted || self.open != open || self.show_mnemonic != show;
        self.highlighted = highlighted;
        self.open = open;
        self.show_mnemonic = show;
        changed
    }
}

impl<M: 'static> Widget<M> for BarTitle {
    fn measure(&mut self, cx: &mut LayoutCx<'_, M>, constraints: Constraints) -> Size {
        let generation = cx.text_generation();
        if self.laid_out_for != Some(generation) {
            if cx
                .lay_out_text(&self.name, STYLE, None, &mut self.layout)
                .is_err()
            {
                self.layout = TextLayout::new();
            }
            self.laid_out_for = Some(generation);
        }
        let spacing = cx.spacing();
        constraints.constrain(Size::new(
            self.layout.width() + 2.0 * spacing.control_padding,
            spacing.control_height.max(self.layout.height()),
        ))
    }

    fn record(&self, cx: &RecordCx, recorder: &mut Recorder<'_>) {
        let rect = cx.rect;
        let pad = cx.spacing.gap_xs;
        if self.hovered || self.highlighted || self.open {
            let radius = f32::from(cx.shape.corner_radius);
            recorder.fill_rounded_rect(
                RoundedRect::new(
                    Rect::new(rect.x0, rect.y0 + pad, rect.x1, rect.y1 - pad),
                    radius,
                ),
                ColorToken::ControlHover,
            );
        }
        let origin = Point::new(
            rect.x0 + (rect.width() - self.layout.width()) / 2.0,
            rect.y0 + (rect.height() - self.layout.height()) / 2.0,
        );
        recorder.text(&self.layout, origin, ColorToken::Text);
        if self.show_mnemonic
            && let Some(letter) = self.mnemonic
        {
            underline(
                recorder,
                &self.name,
                &self.layout,
                letter,
                origin,
                ColorToken::Text,
            );
        }
    }

    fn event(
        &mut self,
        cx: &mut super::tree::EventCx<'_, M>,
        event: &super::tree::WidgetEvent<'_>,
    ) -> super::tree::Handled {
        use super::tree::{Handled, WidgetEvent};
        // The hover is the title's own look; everything else is the interface's.
        match event {
            WidgetEvent::PointerEntered | WidgetEvent::PointerLeft => {
                self.hovered = matches!(event, WidgetEvent::PointerEntered);
                cx.request_record();
                Handled::Yes
            }
            _ => Handled::No,
        }
    }

    fn access_role(&self) -> AccessRole {
        AccessRole::MenuItem
    }

    fn access_info(&self, info: &mut AccessInfo) {
        info.label = Some(self.name.clone());
        info.clickable = true;
        info.access_key = self.mnemonic;
        info.expanded = Some(self.open);
    }
}
