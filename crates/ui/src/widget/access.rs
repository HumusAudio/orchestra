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

use accesskit::{
    Action, Node, NodeId, Orientation, Role, ScrollUnit, Toggled, TreeId, TreeInfo, TreeUpdate,
};

use crate::draw::geometry::Rect;

use super::layout::Axis;
use super::tree::{ScrollDirection, WidgetId};

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
}

/// What a widget tells assistive technologies besides its role (WC2): all optional.
#[derive(Debug, Clone, PartialEq, Default)]
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
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AccessAction {
    /// The widget.
    pub target: WidgetId,
    /// What to do.
    pub kind: AccessActionKind,
}

/// The actions understood (WC5; the value actions since F2.09c, scrolling since F2.09d).
#[derive(Debug, Clone, Copy, PartialEq)]
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

/// The window node: named after the window, with the root widget as its only child.
pub(crate) fn window_node(title: &str, root: WidgetId) -> Node {
    let mut node = Node::new(Role::Window);
    node.set_label(title);
    node.push_child(NodeId(root.get()));
    node
}

/// The node of one widget from its role, its rectangle, its children and its information.
pub(crate) fn widget_node(
    role: AccessRole,
    rect: Rect,
    children: impl Iterator<Item = WidgetId>,
    info: &AccessInfo,
    focusable: bool,
) -> Node {
    let mut node = Node::new(match role {
        AccessRole::Group => Role::GenericContainer,
        AccessRole::Label => Role::Label,
        AccessRole::Button => Role::Button,
        AccessRole::CheckBox => Role::CheckBox,
        AccessRole::Switch => Role::Switch,
        AccessRole::Slider => Role::Slider,
        AccessRole::ScrollView => Role::ScrollView,
        AccessRole::Splitter => Role::Splitter,
    });
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
    if let Some(on) = info.toggled {
        node.set_toggled(if on { Toggled::True } else { Toggled::False });
    }
    if let Some(value) = info.value {
        node.set_numeric_value(value);
    }
    if let Some((min, max, step)) = info.range {
        node.set_min_numeric_value(min);
        node.set_max_numeric_value(max);
        node.set_numeric_value_step(step);
    }
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
    if info.disabled {
        node.set_disabled();
    } else {
        if info.clickable {
            node.add_action(Action::Click);
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
    node
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
        Action::SetValue => match request.data {
            Some(accesskit::ActionData::NumericValue(value)) if value.is_finite() => {
                AccessActionKind::SetValue(value)
            }
            _ => return None,
        },
        Action::ScrollUp => scroll(ScrollDirection::Up, request),
        Action::ScrollDown => scroll(ScrollDirection::Down, request),
        Action::ScrollLeft => scroll(ScrollDirection::Left, request),
        Action::ScrollRight => scroll(ScrollDirection::Right, request),
        Action::ScrollIntoView => AccessActionKind::ScrollIntoView,
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

    use accesskit::{Action, ActionData, ActionRequest, NodeId, ScrollUnit, TreeId};

    use super::{AccessActionKind, ScrollDirection, action_from};

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
}
