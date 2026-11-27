// SPDX-License-Identifier: GPL-3.0-or-later

//! Accessibility (ADR-0045 WA5, choices WC2–WC5 of F2.09b): how a widget describes itself, in
//! types of our own, and the one place that turns those descriptions into AccessKit's tree.
//!
//! # Why our own types
//! As with `skrifa` behind `orchestra-text` (ADR-0044 TX2), widgets never see AccessKit: they
//! answer an [`AccessRole`] and fill an [`AccessInfo`]. AccessKit publishes a breaking version
//! every few weeks (ADR-0036 B1); with this file in between, an update touches one file, not every
//! widget.
//!
//! # The tree
//! A window node (ID 0: widget IDs start at 1) named after the window's title, whose only child is
//! the root widget; then one node per widget with its role, its bounds (the rectangle of the last
//! layout), its children and what its [`AccessInfo`] says. The first update carries every node
//! and the tree's description; later ones only the nodes marked as changed (WC3), plus the focus.
//!
//! # Text (F2.10b3, ADR-0045 WI1–WI3, ADR-0047 TF11)
//! A widget that holds editable text fills [`AccessInfo::text`] with a [`TextInfo`], and this
//! file turns it into AccessKit's `TextRun` nodes, children of the widget's node: the only nodes
//! of the tree that are not widgets. A run holds at most [`RUN_CHARACTERS`] characters, because
//! AccessKit keeps a run's word starts in bytes; a longer line becomes several runs linked as one
//! line. Their node numbers come from [`WidgetId::text_run_node`]. The requests that come back
//! (set the text, replace the selection, select) name positions by run and character: they are
//! turned into characters of the whole text here, so a widget never sees a run.

use accesskit::{
    Action, HasPopup, Node, NodeId, Orientation, Role, ScrollUnit, TextDirection, TextPosition,
    TextSelection, Toggled, TreeId, TreeInfo, TreeUpdate,
};

use crate::draw::geometry::Rect;

use super::layout::Axis;
use super::tree::{ScrollDirection, WidgetId};

/// How many characters one text run holds at most (ADR-0045 WI2): AccessKit keeps a run's word
/// starts as bytes and reads a character's index as one, so the 256th character of a run could
/// not be named.
pub const RUN_CHARACTERS: usize = 255;

/// The role of a widget, as assistive technologies announce it (WC2). Exhaustive: a new widget
/// kind adds its role here and its translation below.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AccessRole {
    /// A container that only groups its children (rows, columns, grids, stacks).
    Group,
    /// Static text.
    Label,
    /// A push button.
    Button,
    /// A two-state box (F2.09c).
    CheckBox,
    /// An on/off switch (F2.09c).
    Switch,
    /// A slider (F2.09c).
    Slider,
    /// A scroll area: a viewport on content larger than itself (F2.09d).
    ScrollView,
    /// The divider between two panes, which can be moved (F2.09d).
    Splitter,
    /// A one-line text field (F2.10b2), with its text, caret and selection in
    /// [`AccessInfo::text`] (F2.10b3, ADR-0047 TF11).
    TextInput,
    /// A menu: a context menu or a submenu (F2.11d2a, ADR-0048 AM12).
    Menu,
    /// The menu bar (F2.11d2b): its titles are menu items that open menus.
    MenuBar,
    /// Our title bar (F2.11b2, ADR-0049 TB9): it holds the menu bar and the window's buttons.
    TitleBar,
    /// An item of a menu.
    MenuItem,
    /// An item of a menu with a check (a toggle), its state in [`AccessInfo::toggled`].
    MenuItemCheckBox,
    /// A line between groups of items: hidden from assistive technologies, which count the items
    /// without it.
    Separator,
    /// A dialog that asks something (ADR-0050 DG2).
    Dialog,
    /// A dialog that reports an error: assistive technologies announce it at once.
    AlertDialog,
    /// A picture with a name and no text of its own (the Orchestra logo, F2.12a3b).
    Image,
    /// A value in a drop-down's list (F2.12a4a): one of several, its state in
    /// [`AccessInfo::toggled`].
    MenuItemRadio,
    /// A drop-down: shows the value in use and opens the list of values (F2.12a4a).
    ComboBox,
    /// A group of options of which one is chosen (F2.12a4a), the chosen one in
    /// [`AccessInfo::value_text`].
    RadioGroup,
    /// A list of items of which one is selected (the Preferences' sections, F2.12a4a), the
    /// selected one in [`AccessInfo::value_text`].
    ListBox,
}

/// What a widget tells assistive technologies besides its role (WC2): all optional.
#[derive(Debug, Clone, PartialEq, Default)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent facts a widget reports (disabled, clickable, has a context menu, modal), \
              each read on its own by the AccessKit node builder"
)]
pub struct AccessInfo {
    /// The accessible name: a button's text, a label's text.
    pub label: Option<String>,
    /// On or off, for check boxes and switches.
    pub toggled: Option<bool>,
    /// A numeric value, for sliders.
    pub value: Option<f64>,
    /// The value's range and step (minimum, maximum, step), for sliders: assistive technologies
    /// then offer to increase, decrease and set the value (F2.09c).
    pub range: Option<(f64, f64, f64)>,
    /// The widget cannot be used now.
    pub disabled: bool,
    /// The widget can be activated (it answers [`WidgetEvent::Activate`](crate::WidgetEvent)).
    pub clickable: bool,
    /// How far the content is scrolled, for scroll areas (F2.09d): assistive technologies then
    /// offer to scroll it, and the widget clips its children.
    pub scroll: Option<ScrollInfo>,
    /// The direction the widget works in, for splitters (F2.09d): `Horizontal` for panes side by
    /// side (the divider itself is then vertical, and assistive technologies say so).
    pub orientation: Option<Axis>,
    /// The text the widget holds, with its caret and selection, for text fields (F2.10b3):
    /// assistive technologies then read it by character and word, follow the caret, and offer to
    /// set the text, replace the selection and select.
    pub text: Option<TextInfo>,
    /// The hint shown while a text field is empty, announced then (F2.10b3, ADR-0045 WI4).
    pub placeholder: Option<String>,
    /// The shortcut that does what the widget does, as people read it (a menu item's,
    /// F2.11d2a, ADR-0048 AM12): already written in the platform's style and language.
    pub keyboard_shortcut: Option<String>,
    /// The widget's place among its siblings of the same kind, from 1, and their number (a menu
    /// item's "3 of 7").
    pub position: Option<(usize, usize)>,
    /// For a widget that opens a menu (a submenu's item), whether the menu is open now: the node
    /// then also says it opens a menu (`has_popup`). `None` for a widget that opens nothing.
    pub expanded: Option<bool>,
    /// The widget has a context menu ([`crate::Widget::context_menu`]): assistive technologies
    /// then offer to show it. Filled by the interface, not by the widget.
    pub context_menu: bool,
    /// The letter that chooses the widget inside its menu, or opens its menu from the bar with
    /// Alt (a menu item's mnemonic, F2.11d2b, ADR-0048 AM10).
    pub access_key: Option<char>,
    /// A longer text that says what the widget is about, read after its name (a dialog's message,
    /// ADR-0050 DG2).
    pub description: Option<String>,
    /// The widget is modal: nothing outside it can be used while it is shown (a dialog).
    pub modal: bool,
    /// The value as people read it, for widgets whose value is a word, not a number (F2.12a4a):
    /// a drop-down's, a group of options', a list's chosen item.
    pub value_text: Option<String>,
}

/// One line of text as assistive technologies read it (F2.10b3, ADR-0045 WI1, ADR-0047 TF11).
///
/// # Characters
/// Everything is counted in **characters**: the smallest pieces a caret can stand between, so
/// graphemes, not code points ("é" written as "e" and a combining accent is one). A character is
/// at most 255 bytes long (AccessKit's limit): the widget gives a longer grapheme as several
/// characters.
///
/// # Invariants the widget keeps
/// `character_lengths`, `character_positions` and `character_widths` have one item per character,
/// and the lengths add up to `value`'s length in bytes. A description that breaks them is cut at
/// the first character that does not fit, never a panic.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TextInfo {
    /// The whole text.
    pub value: String,
    /// The length of each character, bytes of `value` (UTF-8).
    pub character_lengths: Vec<u8>,
    /// Where each character starts: logical pixels from the line's left edge (`rect.x0`),
    /// growing to the right (texts are left to right only, ADR-0044 TX5).
    pub character_positions: Vec<f32>,
    /// The advance of each character, logical pixels.
    pub character_widths: Vec<f32>,
    /// The characters at which a word starts, increasing. The line's first character starts a
    /// word whether listed or not; spaces and punctuation belong to the word before them.
    pub word_starts: Vec<usize>,
    /// Where the selection starts, in characters from 0 to their number (the end of the text);
    /// equal to `focus` when nothing is selected.
    pub anchor: usize,
    /// Where the caret is, in characters.
    pub focus: usize,
    /// The whole line's rectangle, logical pixels of the window: it may reach outside the
    /// widget's own rectangle when the text is scrolled inside it (the widget's node then clips).
    pub rect: Rect,
}

/// How far a scroll area's content is scrolled (F2.09d), logical pixels: the offset from the
/// content's top left corner to the viewport's, and the largest offset possible in each
/// direction (zero in a direction that does not scroll).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ScrollInfo {
    /// The horizontal offset, from 0 to `max_x`.
    pub x: f32,
    /// The largest horizontal offset.
    pub max_x: f32,
    /// The vertical offset, from 0 to `max_y`.
    pub y: f32,
    /// The largest vertical offset.
    pub max_y: f32,
}

/// What an assistive technology asks a widget to do (WC5), delivered by the window.
///
/// Not `Copy` since F2.10b3: a request to set a text owns that text.
#[derive(Debug, Clone, PartialEq)]
pub struct AccessAction {
    /// The widget.
    pub target: WidgetId,
    /// What to do.
    pub kind: AccessActionKind,
}

/// The actions understood (WC5; the value actions since F2.09c, scrolling since F2.09d, the text
/// actions since F2.10b3).
#[derive(Debug, Clone, PartialEq)]
pub enum AccessActionKind {
    /// Activate the widget, as a click or the Enter key would.
    Click,
    /// Give it the keyboard focus.
    Focus,
    /// Increase the value by one step.
    Increment,
    /// Decrease the value by one step.
    Decrement,
    /// Set the value.
    SetValue(f64),
    /// Scroll the target (a scroll area) by a line or a page (F2.09d).
    Scroll {
        /// Which way the view moves.
        direction: ScrollDirection,
        /// A page (a viewport) instead of a line.
        page: bool,
    },
    /// Scroll the areas around the target until it can be seen (F2.09d).
    ScrollIntoView,
    /// Replace the whole text of the target (a text field) with this one (F2.10b3).
    SetText(String),
    /// Replace the target's selected text with this one, or insert it at the caret.
    ReplaceSelectedText(String),
    /// Select from `anchor` to `focus` in the target's text, both in characters of its
    /// [`TextInfo`] (equal: only the caret moves).
    SetTextSelection {
        /// Where the selection starts.
        anchor: usize,
        /// Where the caret goes.
        focus: usize,
    },
    /// Show the target's context menu (F2.11d2a, ADR-0048 AM12), as the Menu key would.
    ShowContextMenu,
}

/// An update of the accessibility tree, ready for the window's adapter. Opaque: only the window
/// (and tests, through [`AccessUpdate::tree_update`]) look inside.
#[derive(Debug, Clone, PartialEq)]
pub struct AccessUpdate(pub(crate) TreeUpdate);

impl AccessUpdate {
    /// The AccessKit update inside, for the window's adapter and for tests.
    #[must_use]
    pub const fn tree_update(&self) -> &TreeUpdate {
        &self.0
    }

    /// Takes the AccessKit update out.
    #[must_use]
    pub fn into_tree_update(self) -> TreeUpdate {
        self.0
    }
}

/// The ID of the window node (widget IDs start at 1).
pub(crate) const WINDOW_NODE: NodeId = NodeId(0);

/// An update with no nodes yet, focused on `focus` (a widget, or the window), and with the tree's
/// description when `full`.
pub(crate) fn empty_update(focus: Option<WidgetId>, full: bool) -> TreeUpdate {
    TreeUpdate {
        nodes: Vec::new(),
        tree: full.then(|| TreeInfo::new(WINDOW_NODE)),
        tree_id: TreeId::ROOT,
        focus: focus.map_or(WINDOW_NODE, |id| NodeId(id.get())),
    }
}

/// The window node: named after the window, with the root widget as its first child and the
/// open menus after it (F2.11d2a: a menu is drawn above the window's content, and assistive
/// technologies find it as the window's own child, as they find a native popup menu). `busy`
/// marks it as working while the interface's busy layer is up (ADR-0054 AR1, F2.14a).
pub(crate) fn window_node(
    title: &str,
    root: WidgetId,
    menus: impl Iterator<Item = WidgetId>,
    busy: bool,
) -> Node {
    let mut node = Node::new(Role::Window);
    node.set_label(title);
    // AccessKit's `busy` (ARIA's `aria-busy`): the window is working and takes no input for a
    // moment (the busy layer of ADR-0054 AR1).
    if busy {
        node.set_busy();
    }
    node.push_child(NodeId(root.get()));
    for menu in menus {
        node.push_child(NodeId(menu.get()));
    }
    node
}

/// Adds to `nodes` the node of the widget `id`, built from its role, its rectangle, its children
/// and its information, followed by the nodes of its text runs if it describes a text
/// ([`AccessInfo::text`]).
pub(crate) fn push_widget_nodes(
    nodes: &mut Vec<(NodeId, Node)>,
    id: WidgetId,
    role: AccessRole,
    rect: Rect,
    children: impl Iterator<Item = WidgetId>,
    info: &AccessInfo,
    focusable: bool,
) {
    let mut node = Node::new(role_of(role));
    if role == AccessRole::Separator {
        node.set_hidden();
    }
    if !rect.is_empty() {
        node.set_bounds(accesskit::Rect {
            x0: f64::from(rect.x0),
            y0: f64::from(rect.y0),
            x1: f64::from(rect.x1),
            y1: f64::from(rect.y1),
        });
    }
    for child in children {
        node.push_child(NodeId(child.get()));
    }
    if let Some(label) = &info.label {
        node.set_label(label.as_str());
    }
    if let Some(description) = &info.description {
        node.set_description(description.as_str());
    }
    if info.modal {
        node.set_modal();
    }
    set_values(&mut node, info);
    if let Some(scroll) = info.scroll {
        // The children outside the viewport are not visible: assistive technologies must not
        // offer them as on screen (they can still ask to scroll them into view).
        node.set_clips_children();
        node.set_scroll_x(f64::from(scroll.x));
        node.set_scroll_x_min(0.0);
        node.set_scroll_x_max(f64::from(scroll.max_x));
        node.set_scroll_y(f64::from(scroll.y));
        node.set_scroll_y_min(0.0);
        node.set_scroll_y_max(f64::from(scroll.max_y));
    }
    if let Some(axis) = info.orientation {
        // AccessKit names a splitter's orientation after the divider: panes side by side are
        // separated by a vertical divider.
        node.set_orientation(match axis {
            Axis::Horizontal => Orientation::Vertical,
            Axis::Vertical => Orientation::Horizontal,
        });
    }
    if let Some(placeholder) = &info.placeholder {
        node.set_placeholder(placeholder.as_str());
    }
    set_menu_properties(&mut node, info);
    if info.text.is_some() {
        // The line may be scrolled inside the widget and reach past its edges (TF11).
        node.set_clips_children();
    }
    if info.disabled {
        node.set_disabled();
    } else {
        if info.text.is_some() {
            // A disabled field keeps its text readable and offers none of these (TF11).
            node.add_action(Action::SetValue);
            node.add_action(Action::ReplaceSelectedText);
            node.add_action(Action::SetTextSelection);
        }
        if info.clickable {
            node.add_action(Action::Click);
        }
        if info.context_menu {
            node.add_action(Action::ShowContextMenu);
        }
        if focusable {
            node.add_action(Action::Focus);
        }
        if info.range.is_some() {
            node.add_action(Action::Increment);
            node.add_action(Action::Decrement);
            node.add_action(Action::SetValue);
        }
        if let Some(scroll) = info.scroll {
            // Offered only in the directions there is somewhere to go.
            if scroll.y > 0.0 {
                node.add_action(Action::ScrollUp);
            }
            if scroll.y < scroll.max_y {
                node.add_action(Action::ScrollDown);
            }
            if scroll.x > 0.0 {
                node.add_action(Action::ScrollLeft);
            }
            if scroll.x < scroll.max_x {
                node.add_action(Action::ScrollRight);
            }
        }
    }
    // Every widget can be asked to come into view: the interface scrolls the areas around it
    // (a widget outside any scroll area is already in view, and nothing happens).
    node.add_action(Action::ScrollIntoView);
    // The widget's node goes first and its runs after it; the runs are pushed as its children
    // while they are built, so the node is put in its place at the end.
    let at = nodes.len();
    if let Some(text) = &info.text {
        push_text_runs(nodes, &mut node, id, text);
    }
    nodes.insert(at, (NodeId(id.get()), node));
}

/// The widget's state and value on its node: on or off, a number with its range, a value as a
/// word (F2.12a4a). Split from [`push_widget_nodes`] to keep each function short.
fn set_values(node: &mut Node, info: &AccessInfo) {
    if let Some(on) = info.toggled {
        node.set_toggled(if on { Toggled::True } else { Toggled::False });
    }
    if let Some(value) = info.value {
        node.set_numeric_value(value);
    }
    if let Some(text) = &info.value_text {
        node.set_value(text.clone());
    }
    if let Some((min, max, step)) = info.range {
        node.set_min_numeric_value(min);
        node.set_max_numeric_value(max);
        node.set_numeric_value_step(step);
    }
}

/// AccessKit's role for `role` (WC2).
const fn role_of(role: AccessRole) -> Role {
    match role {
        // AccessKit 0.25 has no separator role; a hidden container is skipped by every screen
        // reader, which is what a menu's separator should be (`push_widget_nodes` hides it).
        AccessRole::Group | AccessRole::Separator => Role::GenericContainer,
        AccessRole::Label => Role::Label,
        AccessRole::Button => Role::Button,
        AccessRole::CheckBox => Role::CheckBox,
        AccessRole::Switch => Role::Switch,
        AccessRole::Slider => Role::Slider,
        AccessRole::ScrollView => Role::ScrollView,
        AccessRole::Splitter => Role::Splitter,
        AccessRole::TextInput => Role::TextInput,
        AccessRole::Menu => Role::Menu,
        AccessRole::MenuBar => Role::MenuBar,
        AccessRole::TitleBar => Role::TitleBar,
        AccessRole::MenuItem => Role::MenuItem,
        AccessRole::MenuItemCheckBox => Role::MenuItemCheckBox,
        AccessRole::Dialog => Role::Dialog,
        AccessRole::AlertDialog => Role::AlertDialog,
        AccessRole::Image => Role::Image,
        AccessRole::MenuItemRadio => Role::MenuItemRadio,
        AccessRole::ComboBox => Role::ComboBox,
        AccessRole::RadioGroup => Role::RadioGroup,
        AccessRole::ListBox => Role::ListBox,
    }
}

/// The properties of a menu's item (F2.11d2a, ADR-0048 AM12): its shortcut, its place among the
/// items, and whether it opens a submenu and that submenu is open. Nothing for other widgets,
/// which leave these empty.
fn set_menu_properties(node: &mut Node, info: &AccessInfo) {
    if let Some(shortcut) = &info.keyboard_shortcut {
        node.set_keyboard_shortcut(shortcut.as_str());
    }
    if let Some((position, size)) = info.position {
        node.set_position_in_set(position);
        node.set_size_of_set(size);
    }
    if let Some(expanded) = info.expanded {
        node.set_has_popup(HasPopup::Menu);
        node.set_expanded(expanded);
    }
    if let Some(letter) = info.access_key {
        // AccessKit takes a string; a letter is at most four bytes, built on the stack.
        let mut buffer = [0; 4];
        node.set_access_key(&*letter.encode_utf8(&mut buffer));
    }
}

/// Adds to `nodes` the text runs of `text`, held by the widget `owner`, and to `parent` (the
/// widget's node) those runs as children and the selection.
///
/// # The runs
/// One run per [`RUN_CHARACTERS`] characters, in order, and one empty run for an empty text (a
/// caret needs a run to stand in). Each has its part of the value, its characters' lengths,
/// their positions from the run's own left edge and their widths, the word starts that fall in
/// it, and its rectangle: its part of the line. Runs are linked to their neighbours as parts of
/// one line, so "end of line" is the text's end and not a run's.
///
/// # Complexity
/// O(characters + word starts); allocates the nodes and their lists (WC10: not a hot path, it
/// runs only while an assistive technology listens).
fn push_text_runs(
    nodes: &mut Vec<(NodeId, Node)>,
    parent: &mut Node,
    owner: WidgetId,
    text: &TextInfo,
) {
    // The characters fully described: a description that breaks its invariants is cut at the
    // shortest list.
    let described = text
        .character_lengths
        .len()
        .min(text.character_positions.len())
        .min(text.character_widths.len());
    // The run each character index belongs to is its index divided by the run size, so the
    // number of runs is the division rounded up, and at least one. The names may allow fewer
    // (`WidgetId::text_run_node`): the loop stops there.
    let wanted = described.div_ceil(RUN_CHARACTERS).max(1);
    // Where the run being built starts: in characters, and in bytes of the value.
    let mut first = 0_usize;
    let mut byte = 0_usize;
    // The word starts not yet given to a run (they are sorted).
    let mut words = text.word_starts.iter().copied().peekable();
    // The node number of the previous run, to link the two.
    let mut previous: Option<NodeId> = None;
    // How many characters and runs were really built, for the selection.
    let mut built_characters = 0_usize;
    let mut built_runs = 0_usize;
    for run in 0..wanted {
        let Some(id) = owner.text_run_node(run).map(NodeId) else {
            // Past the last name (ADR-0045 WI2): the text is cut short here.
            break;
        };
        let last = first.saturating_add(RUN_CHARACTERS).min(described);
        let lengths = text.character_lengths.get(first..last).unwrap_or_default();
        let positions = text
            .character_positions
            .get(first..last)
            .unwrap_or_default();
        let widths = text.character_widths.get(first..last).unwrap_or_default();
        let bytes: usize = lengths.iter().map(|&length| usize::from(length)).sum();
        let Some(value) = text.value.get(byte..byte.saturating_add(bytes)) else {
            // The lengths do not describe the value (they end inside a code point or past the
            // end): nothing from here on can be trusted.
            break;
        };
        let mut node = Node::new(Role::TextRun);
        node.set_value(value);
        node.set_character_lengths(lengths);
        node.set_text_direction(TextDirection::LeftToRight);
        // The run's left edge: where its first character starts (an empty run sits at the
        // line's start). Positions inside the run are counted from it.
        let left = positions.first().copied().unwrap_or(0.0);
        let right = positions
            .last()
            .zip(widths.last())
            .map_or(left, |(position, width)| position + width);
        node.set_character_positions(
            positions
                .iter()
                .map(|position| position - left)
                .collect::<Vec<f32>>(),
        );
        node.set_character_widths(widths);
        node.set_bounds(accesskit::Rect {
            x0: f64::from(text.rect.x0 + left),
            y0: f64::from(text.rect.y0),
            x1: f64::from(text.rect.x0 + right),
            y1: f64::from(text.rect.y1),
        });
        // The word starts inside this run, as indices from its first character: below 255, so
        // they fit AccessKit's bytes. Starts before the run (a list out of order) are dropped.
        let mut starts = Vec::new();
        while let Some(&start) = words.peek() {
            if start >= last {
                break;
            }
            words.next();
            if let Some(index) = start
                .checked_sub(first)
                .and_then(|index| u8::try_from(index).ok())
                && starts.last() != Some(&index)
            {
                starts.push(index);
            }
        }
        node.set_word_starts(starts);
        if let Some(previous) = previous {
            node.set_previous_on_line(previous);
            // The previous run is the last node pushed: it learns its successor now.
            if let Some((_, previous_node)) = nodes.last_mut() {
                previous_node.set_next_on_line(id);
            }
        }
        parent.push_child(id);
        nodes.push((id, node));
        previous = Some(id);
        built_characters = last;
        built_runs = run.saturating_add(1);
        first = last;
        byte = byte.saturating_add(bytes);
    }
    if built_runs == 0 {
        return;
    }
    let position = |character: usize| run_position(owner, character, built_characters, built_runs);
    if let (Some(anchor), Some(focus)) = (position(text.anchor), position(text.focus)) {
        parent.set_text_selection(TextSelection { anchor, focus });
    }
}

/// The position AccessKit names `character` by, in a text of `characters` characters split in
/// `runs` runs of [`RUN_CHARACTERS`]: the run and the index inside it.
///
/// A character on the border of two runs is the first of the later one; only the end of the
/// whole text is "after the last character" of the last run.
fn run_position(
    owner: WidgetId,
    character: usize,
    characters: usize,
    runs: usize,
) -> Option<TextPosition> {
    let character = character.min(characters);
    let last_run = runs.checked_sub(1)?;
    let run = (character / RUN_CHARACTERS).min(last_run);
    // Counted from the run's first character: 0 to 254 inside a run, up to 255 at the text's end.
    let index = character.saturating_sub(run.saturating_mul(RUN_CHARACTERS));
    Some(TextPosition {
        node: NodeId(owner.text_run_node(run)?),
        character_index: index,
    })
}

/// The character of the whole text that `position` names, if it is in a run of `target`: the
/// inverse of [`run_position`]. The widget brings an index past its text back to the end.
fn character_of(target: WidgetId, position: TextPosition) -> Option<usize> {
    let (owner, run) = WidgetId::text_run_of(position.node.0)?;
    (owner == target).then(|| {
        run.saturating_mul(RUN_CHARACTERS)
            .saturating_add(position.character_index.min(RUN_CHARACTERS))
    })
}

/// A tree with only the window in it, for an app that describes nothing (WC6): assistive
/// technologies still see a named window.
pub(crate) fn window_only(title: &str) -> TreeUpdate {
    let mut window = Node::new(Role::Window);
    window.set_label(title);
    let mut update = empty_update(None, true);
    update.nodes.push((WINDOW_NODE, window));
    update
}

/// The action an AccessKit request asks for, if it is one we understand.
pub(crate) fn action_from(request: &accesskit::ActionRequest) -> Option<AccessAction> {
    let target = WidgetId::from_raw(request.target_node.0)?;
    let kind = match request.action {
        Action::Click => AccessActionKind::Click,
        Action::Focus => AccessActionKind::Focus,
        Action::Increment => AccessActionKind::Increment,
        Action::Decrement => AccessActionKind::Decrement,
        Action::SetValue => match &request.data {
            Some(accesskit::ActionData::NumericValue(value)) if value.is_finite() => {
                AccessActionKind::SetValue(*value)
            }
            // The same action with a text is for a text field (F2.10b3).
            Some(accesskit::ActionData::Value(text)) => {
                AccessActionKind::SetText(text.as_ref().to_owned())
            }
            _ => return None,
        },
        Action::ReplaceSelectedText => match &request.data {
            Some(accesskit::ActionData::Value(text)) => {
                AccessActionKind::ReplaceSelectedText(text.as_ref().to_owned())
            }
            _ => return None,
        },
        Action::SetTextSelection => match &request.data {
            // Both ends must be in the target's own runs; anything else is not understood.
            Some(accesskit::ActionData::SetTextSelection(selection)) => {
                AccessActionKind::SetTextSelection {
                    anchor: character_of(target, selection.anchor)?,
                    focus: character_of(target, selection.focus)?,
                }
            }
            _ => return None,
        },
        Action::ScrollUp => scroll(ScrollDirection::Up, request),
        Action::ScrollDown => scroll(ScrollDirection::Down, request),
        Action::ScrollLeft => scroll(ScrollDirection::Left, request),
        Action::ScrollRight => scroll(ScrollDirection::Right, request),
        Action::ScrollIntoView => AccessActionKind::ScrollIntoView,
        Action::ShowContextMenu => AccessActionKind::ShowContextMenu,
        _ => return None,
    };
    Some(AccessAction { target, kind })
}

/// A scroll action towards `direction`: by a page when the request says so, by a line otherwise
/// (AccessKit's unit is optional, and a line is the smaller, safer default).
fn scroll(direction: ScrollDirection, request: &accesskit::ActionRequest) -> AccessActionKind {
    let page = matches!(
        request.data,
        Some(accesskit::ActionData::ScrollUnit(ScrollUnit::Page))
    );
    AccessActionKind::Scroll { direction, page }
}

#[cfg(test)]
mod tests {
    //! The translation of AccessKit's scroll requests (F2.09d), which only the window's adapter
    //! reaches otherwise.

    use accesskit::{
        Action, ActionData, ActionRequest, NodeId, ScrollUnit, TextPosition, TextSelection, TreeId,
    };

    use super::{
        AccessActionKind, RUN_CHARACTERS, ScrollDirection, WidgetId, action_from, character_of,
        run_position,
    };

    /// A request for `action` on widget 7, with `data`.
    fn request(action: Action, data: Option<ActionData>) -> ActionRequest {
        ActionRequest {
            action,
            target_tree: TreeId::ROOT,
            target_node: NodeId(7),
            data,
        }
    }

    #[test]
    fn scroll_requests_become_lines_pages_and_reveals() {
        let page = Some(ActionData::ScrollUnit(ScrollUnit::Page));
        let item = Some(ActionData::ScrollUnit(ScrollUnit::Item));
        let cases = [
            (
                Action::ScrollDown,
                page.clone(),
                ScrollDirection::Down,
                true,
            ),
            (Action::ScrollUp, item, ScrollDirection::Up, false),
            (Action::ScrollLeft, None, ScrollDirection::Left, false),
            (Action::ScrollRight, page, ScrollDirection::Right, true),
        ];
        for (action, data, direction, is_page) in cases {
            let kind = action_from(&request(action, data)).map(|action| action.kind);
            assert_eq!(
                kind,
                Some(AccessActionKind::Scroll {
                    direction,
                    page: is_page
                }),
                "{action:?}"
            );
        }
        let reveal = action_from(&request(Action::ScrollIntoView, None)).map(|a| a.kind);
        assert_eq!(reveal, Some(AccessActionKind::ScrollIntoView));
        // Scrolling to a point is not offered, so it is not understood.
        assert_eq!(action_from(&request(Action::ScrollToPoint, None)), None);
    }

    /// The widget the requests of [`request`] aim at.
    fn seven() -> WidgetId {
        WidgetId::from_raw(7).unwrap_or(WidgetId::MAX)
    }

    /// The text requests (F2.10b3): a text value, a replacement, and a selection whose ends are
    /// named by run and character.
    #[test]
    fn text_requests_become_texts_and_characters() {
        let text = || Some(ActionData::Value("Cello".into()));
        assert_eq!(
            action_from(&request(Action::SetValue, text())).map(|action| action.kind),
            Some(AccessActionKind::SetText("Cello".to_owned()))
        );
        assert_eq!(
            action_from(&request(Action::ReplaceSelectedText, text())).map(|action| action.kind),
            Some(AccessActionKind::ReplaceSelectedText("Cello".to_owned()))
        );
        // A replacement without a text is not understood.
        assert_eq!(
            action_from(&request(Action::ReplaceSelectedText, None)),
            None
        );
        // Run 0 character 3 to run 2 character 10 of widget 7: characters 3 and 520.
        let node = |run: u64| NodeId((1 << 63) | (run << 48) | 7);
        let selection = |anchor: NodeId, focus: NodeId| {
            Some(ActionData::SetTextSelection(TextSelection {
                anchor: TextPosition {
                    node: anchor,
                    character_index: 3,
                },
                focus: TextPosition {
                    node: focus,
                    character_index: 10,
                },
            }))
        };
        assert_eq!(
            action_from(&request(
                Action::SetTextSelection,
                selection(node(0), node(2))
            ))
            .map(|action| action.kind),
            Some(AccessActionKind::SetTextSelection {
                anchor: 3,
                focus: 520
            })
        );
        // An end in another widget's run (widget 8), or on a widget's own node: not understood.
        let foreign = NodeId((1 << 63) | 8);
        assert_eq!(
            action_from(&request(
                Action::SetTextSelection,
                selection(node(0), foreign)
            )),
            None
        );
        assert_eq!(
            action_from(&request(
                Action::SetTextSelection,
                selection(NodeId(7), node(0))
            )),
            None
        );
    }

    /// Every character of texts around the run size is named by a position that names it back,
    /// inside the run it belongs to; the text's end is after the last character of the last run.
    #[test]
    fn positions_and_characters_are_inverse() {
        for characters in [0_usize, 1, 254, 255, 256, 509, 510, 511, 1000] {
            let runs = usize::max(characters.div_ceil(RUN_CHARACTERS), 1);
            for character in 0..=characters {
                let position = run_position(seven(), character, characters, runs);
                let Some(position) = position else {
                    panic!("no position for {character} of {characters}");
                };
                assert_eq!(character_of(seven(), position), Some(character));
                let (owner, run) = WidgetId::text_run_of(position.node.0).unwrap_or((seven(), 99));
                assert_eq!(owner, seven());
                assert!(run < runs, "{character} of {characters}: run {run}");
                // Inside a run, except at the very end of the text.
                let in_run = characters.min((run + 1) * RUN_CHARACTERS) - run * RUN_CHARACTERS;
                if character < characters {
                    assert!(position.character_index < in_run);
                } else {
                    assert_eq!(position.character_index, in_run);
                }
            }
            // Past the end: the end.
            assert_eq!(
                run_position(seven(), characters + 5, characters, runs),
                run_position(seven(), characters, characters, runs)
            );
        }
        // Names that do not fit: a run past the last one, a widget number of 2⁴⁸.
        assert_eq!(seven().text_run_node(1 << 15), None);
        assert!(seven().text_run_node((1 << 15) - 1).is_some());
        let huge = WidgetId::from_raw(1 << 48).unwrap_or(seven());
        assert_eq!(huge.text_run_node(0), None);
        // A widget's own number names no run.
        assert_eq!(WidgetId::text_run_of(7), None);
    }
}
