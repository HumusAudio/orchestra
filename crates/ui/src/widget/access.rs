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

use accesskit::{Action, Node, NodeId, Role, Toggled, TreeId, TreeInfo, TreeUpdate};

use crate::draw::geometry::Rect;

use super::tree::WidgetId;

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
}

/// What an assistive technology asks a widget to do (WC5), delivered by the window.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AccessAction {
    /// The widget.
    pub target: WidgetId,
    /// What to do.
    pub kind: AccessActionKind,
}

/// The actions understood (WC5; the value actions since F2.09c); scrolling comes with the scroll
/// area.
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
    }
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
        _ => return None,
    };
    Some(AccessAction { target, kind })
}
