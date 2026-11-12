// SPDX-License-Identifier: GPL-3.0-or-later

//! Spike F2.08: the same settings panel built with **retained** widgets and with **immediate**
//! widgets, on our real display list, text crate and renderer, with the AccessKit tree each one
//! produces; and `taffy` against a column layout of our own.
//!
//! # The panel
//! [`ROWS`] rows, each a label (shaped Inter text), a checkbox and a button with a text: 198
//! widgets, the size of a busy preferences page or an inspector. Every widget records into its
//! own display-list segment, as ADR-0039 D1 intends.
//!
//! # The three ways
//! - [`Retained`]: the widgets are objects that live between frames, with their rectangle, their
//!   laid-out text and their state. An event finds the widget under the pointer, changes it,
//!   re-records **its** segment and sends AccessKit **its** node.
//! - [`Immediate`] without a text cache (B1): at every event the UI function runs again from the
//!   top: it lays the texts out, places every widget, re-records every segment and builds the
//!   whole AccessKit tree. This is the pure immediate model (egui's approach, minus its caches).
//! - [`Immediate`] with a text cache (B2): the same, but laid-out texts are kept between runs,
//!   keyed by their content, which is what every immediate toolkit ends up doing.
//!
//! Damage: re-recording a segment damages its old and new bounds, even if nothing changed, so
//! the immediate ways redraw the whole panel at every event; the retained way redraws one
//! widget. (An immediate toolkit could compare what it recorded with the previous run to skip
//! unchanged segments: that comparison is exactly the retained state it claims not to keep.)
//!
//! Throw-away code (spike): clarity of the measurement over polish.

use std::collections::HashMap;

use accesskit::{Node, NodeId, Role, Toggled, TreeId, TreeInfo, TreeUpdate};
use orchestra_text::{Faces, Shaper, TextLayout};
use orchestra_ui::{
    ColorToken, DisplayList, FamilyToken, Point, Rect, RoundedRect, SegmentId, SizeToken,
    StrokeStyle, Typography, Weight,
};

/// Rows of the panel.
pub const ROWS: usize = 66;
/// Height of a row, logical pixels.
const ROW: f32 = 28.0;
/// The panel's top margin.
const TOP: f32 = 10.0;
/// The label's left edge and width.
const LABEL_X: f32 = 10.0;
/// The checkbox's left edge and side.
const CHECK_X: f32 = 320.0;
const CHECK: f32 = 16.0;
/// The button's left edge and size.
const BUTTON_X: f32 = 352.0;
const BUTTON_W: f32 = 140.0;
const BUTTON_H: f32 = 22.0;
/// The panel's size, logical pixels.
pub const WIDTH: f32 = 510.0;
pub const HEIGHT: f32 = TOP * 2.0 + ROWS as f32 * ROW;

/// The texts of the panel, made once (a real UI takes them from the Fluent catalogues).
pub fn texts() -> (Vec<String>, Vec<String>) {
    let labels = (0..ROWS)
        .map(|i| format!("Option {i}: transpose the Viertelnote"))
        .collect();
    let buttons = (0..ROWS).map(|i| format!("Reset {i}")).collect();
    (labels, buttons)
}

/// The three widgets of the panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Label,
    Checkbox,
    Button,
}

/// What an event asks of the panel.
#[derive(Debug, Clone, Copy)]
pub enum Input {
    /// A click (press and release) at a point.
    Click(Point),
    /// The pointer moved to a point.
    Move(Point),
}

/// The rectangle of widget `kind` of `row`: the column layout of our own (O(1) per widget).
fn rect(row: usize, kind: Kind) -> Rect {
    let y = TOP + row as f32 * ROW;
    match kind {
        Kind::Label => Rect::from_origin_size(LABEL_X, y, CHECK_X - LABEL_X - 10.0, ROW),
        Kind::Checkbox => Rect::from_origin_size(CHECK_X, y + (ROW - CHECK) / 2.0, CHECK, CHECK),
        Kind::Button => {
            Rect::from_origin_size(BUTTON_X, y + (ROW - BUTTON_H) / 2.0, BUTTON_W, BUTTON_H)
        }
    }
}

/// The widget under a point: the row by division (O(1)), then the column.
fn hit(point: Point) -> Option<(usize, Kind)> {
    let row = ((point.y - TOP) / ROW).floor();
    if !(0.0..ROWS as f32).contains(&row) {
        return None;
    }
    let row = row as usize;
    [Kind::Checkbox, Kind::Button, Kind::Label]
        .into_iter()
        .find(|&kind| {
            let r = rect(row, kind);
            point.x >= r.x0 && point.x < r.x1 && point.y >= r.y0 && point.y < r.y1
        })
        .map(|kind| (row, kind))
}

/// The segment and the AccessKit node of widget `kind` of `row` (stable ids).
fn id(row: usize, kind: Kind) -> u64 {
    1 + row as u64 * 3
        + match kind {
            Kind::Label => 0,
            Kind::Checkbox => 1,
            Kind::Button => 2,
        }
}

/// The AccessKit root (the panel).
const ROOT: NodeId = NodeId(0);

/// Records one widget into its segment.
fn record(
    list: &mut DisplayList,
    row: usize,
    kind: Kind,
    text: Option<&TextLayout>,
    checked: bool,
    hovered: bool,
) {
    let r = rect(row, kind);
    list.record(SegmentId(id(row, kind)), |rec| match kind {
        Kind::Label => {
            if let Some(text) = text {
                let y = r.y0 + (r.height() - text.height()) / 2.0;
                rec.text(text, Point::new(r.x0, y), ColorToken::Text);
            }
        }
        Kind::Checkbox => {
            let shape = RoundedRect::new(r, 3.0);
            if checked {
                rec.fill_rounded_rect(shape, ColorToken::Accent);
            }
            let border = if hovered {
                ColorToken::Focus
            } else {
                ColorToken::ControlBorder
            };
            rec.stroke_rounded_rect(shape, StrokeStyle::new(1.0), border);
        }
        Kind::Button => {
            let shape = RoundedRect::new(r, 5.0);
            let fill = if hovered {
                ColorToken::ControlHover
            } else {
                ColorToken::Control
            };
            rec.fill_rounded_rect(shape, fill);
            rec.stroke_rounded_rect(shape, StrokeStyle::new(1.0), ColorToken::ControlBorder);
            if let Some(text) = text {
                let x = r.x0 + (r.width() - text.width()) / 2.0;
                let y = r.y0 + (r.height() - text.height()) / 2.0;
                rec.text(text, Point::new(x, y), ColorToken::Text);
            }
        }
    });
}

/// The AccessKit node of one widget.
fn node(row: usize, kind: Kind, label: &str, button: &str, checked: bool) -> Node {
    let r = rect(row, kind);
    let mut node = Node::new(match kind {
        Kind::Label => Role::Label,
        Kind::Checkbox => Role::CheckBox,
        Kind::Button => Role::Button,
    });
    node.set_bounds(accesskit::Rect {
        x0: f64::from(r.x0),
        y0: f64::from(r.y0),
        x1: f64::from(r.x1),
        y1: f64::from(r.y1),
    });
    match kind {
        Kind::Label => node.set_label(label),
        Kind::Checkbox => {
            node.set_toggled(if checked {
                Toggled::True
            } else {
                Toggled::False
            });
        }
        Kind::Button => node.set_label(button),
    }
    node
}

/// The root node, listing every widget.
fn root_node() -> Node {
    let mut root = Node::new(Role::Window);
    for row in 0..ROWS {
        for kind in [Kind::Label, Kind::Checkbox, Kind::Button] {
            root.push_child(NodeId(id(row, kind)));
        }
    }
    root
}

/// An empty update to fill.
pub fn empty_update() -> TreeUpdate {
    TreeUpdate {
        nodes: Vec::new(),
        tree: None,
        tree_id: TreeId::ROOT,
        focus: ROOT,
    }
}

// ------------------------------------------------------------------------------------------------
// A — retained
// ------------------------------------------------------------------------------------------------

/// One retained widget.
#[derive(Debug)]
struct Widget {
    kind: Kind,
    row: usize,
    /// The laid-out text (labels and buttons), kept for the widget's life.
    text: Option<TextLayout>,
    checked: bool,
    hovered: bool,
}

/// The panel as retained widgets.
#[derive(Debug)]
pub struct Retained {
    widgets: Vec<Widget>,
    labels: Vec<String>,
    buttons: Vec<String>,
    hovered: Option<usize>,
}

impl Retained {
    /// Creates the widgets, lays their texts out, records every segment and fills `update`
    /// with the whole tree: the first frame.
    pub fn build(
        faces: &Faces,
        shaper: &mut Shaper,
        labels: Vec<String>,
        buttons: Vec<String>,
        list: &mut DisplayList,
        update: &mut TreeUpdate,
    ) -> Self {
        let typography = Typography::DEFAULT;
        let style = typography.text_style(FamilyToken::Ui, SizeToken::Body, Weight::Regular);
        let mut widgets = Vec::with_capacity(ROWS * 3);
        for row in 0..ROWS {
            for kind in [Kind::Label, Kind::Checkbox, Kind::Button] {
                let text = match kind {
                    Kind::Label => Some(&labels[row]),
                    Kind::Button => Some(&buttons[row]),
                    Kind::Checkbox => None,
                }
                .map(|text| {
                    let mut layout = TextLayout::new();
                    shaper
                        .layout(faces, text, style, None, &mut layout)
                        .unwrap();
                    layout
                });
                widgets.push(Widget {
                    kind,
                    row,
                    text,
                    checked: false,
                    hovered: false,
                });
            }
        }
        let panel = Self {
            widgets,
            labels,
            buttons,
            hovered: None,
        };
        for index in 0..panel.widgets.len() {
            panel.record(index, list);
        }
        update.nodes.push((ROOT, root_node()));
        for index in 0..panel.widgets.len() {
            update.nodes.push(panel.node(index));
        }
        update.tree = Some(TreeInfo::new(ROOT));
        panel
    }

    /// Records widget `index`.
    fn record(&self, index: usize, list: &mut DisplayList) {
        let w = &self.widgets[index];
        record(list, w.row, w.kind, w.text.as_ref(), w.checked, w.hovered);
    }

    /// The AccessKit node of widget `index`.
    fn node(&self, index: usize) -> (NodeId, Node) {
        let w = &self.widgets[index];
        (
            NodeId(id(w.row, w.kind)),
            node(
                w.row,
                w.kind,
                &self.labels[w.row],
                &self.buttons[w.row],
                w.checked,
            ),
        )
    }

    /// Handles one event: finds the widget, changes it, re-records only what changed and puts
    /// only the changed nodes in `update`.
    pub fn handle(&mut self, input: Input, list: &mut DisplayList, update: &mut TreeUpdate) {
        match input {
            Input::Click(point) => {
                if let Some((row, Kind::Checkbox)) = hit(point) {
                    let index = row * 3 + 1;
                    self.widgets[index].checked ^= true;
                    self.record(index, list);
                    update.nodes.push(self.node(index));
                }
            }
            Input::Move(point) => {
                let now = hit(point)
                    .filter(|&(_, kind)| kind != Kind::Label)
                    .map(|(row, kind)| row * 3 + if kind == Kind::Checkbox { 1 } else { 2 });
                if now != self.hovered {
                    for index in [self.hovered, now].into_iter().flatten() {
                        let w = &mut self.widgets[index];
                        w.hovered = Some(index) == now;
                        self.record(index, list);
                    }
                    self.hovered = now;
                }
            }
        }
    }
}

// ------------------------------------------------------------------------------------------------
// B — immediate
// ------------------------------------------------------------------------------------------------

/// The panel as an immediate UI: only the application's state lives between runs (and, in B2,
/// the text cache).
#[derive(Debug)]
pub struct Immediate {
    checked: Vec<bool>,
    hovered: Option<(usize, Kind)>,
    labels: Vec<String>,
    buttons: Vec<String>,
    /// B2: laid-out texts by content; `None` in B1.
    cache: Option<HashMap<String, TextLayout>>,
    /// B1's scratch layout, reused (the fairest pure immediate: no allocation for it).
    scratch: TextLayout,
}

impl Immediate {
    /// An immediate panel, with (`cached`) or without a text cache.
    pub fn new(labels: Vec<String>, buttons: Vec<String>, cached: bool) -> Self {
        Self {
            checked: vec![false; ROWS],
            hovered: None,
            labels,
            buttons,
            cache: cached.then(HashMap::new),
            scratch: TextLayout::new(),
        }
    }

    /// One run of the UI function: handles `input` while walking the widgets, lays the texts
    /// out (or takes them from the cache), records every segment and fills `update` with the
    /// whole tree.
    pub fn run(
        &mut self,
        faces: &Faces,
        shaper: &mut Shaper,
        input: Option<Input>,
        list: &mut DisplayList,
        update: &mut TreeUpdate,
    ) {
        let typography = Typography::DEFAULT;
        let style = typography.text_style(FamilyToken::Ui, SizeToken::Body, Weight::Regular);
        let target = input.map(|input| match input {
            Input::Click(p) | Input::Move(p) => (hit(p), matches!(input, Input::Click(_))),
        });
        update.nodes.push((ROOT, root_node()));
        for row in 0..ROWS {
            for kind in [Kind::Label, Kind::Checkbox, Kind::Button] {
                // The widget "asks" whether the input concerns it, as immediate widgets do.
                if let Some((Some((r, k)), click)) = target
                    && r == row
                    && k == kind
                {
                    if click && kind == Kind::Checkbox {
                        self.checked[row] ^= true;
                    }
                    if !click {
                        self.hovered = Some((row, kind));
                    }
                }
                let hovered = self.hovered == Some((row, kind));
                let text = match kind {
                    Kind::Label => Some(self.labels[row].as_str()),
                    Kind::Button => Some(self.buttons[row].as_str()),
                    Kind::Checkbox => None,
                };
                let layout = text.map(|text| match &mut self.cache {
                    Some(cache) => {
                        if !cache.contains_key(text) {
                            let mut layout = TextLayout::new();
                            shaper
                                .layout(faces, text, style, None, &mut layout)
                                .unwrap();
                            cache.insert(text.to_owned(), layout);
                        }
                        &cache[text]
                    }
                    None => {
                        shaper
                            .layout(faces, text, style, None, &mut self.scratch)
                            .unwrap();
                        &self.scratch
                    }
                });
                record(list, row, kind, layout, self.checked[row], hovered);
                update.nodes.push((
                    NodeId(id(row, kind)),
                    node(
                        row,
                        kind,
                        &self.labels[row],
                        &self.buttons[row],
                        self.checked[row],
                    ),
                ));
            }
        }
        update.tree = Some(TreeInfo::new(ROOT));
    }
}

// ------------------------------------------------------------------------------------------------
// Layout: taffy against our column
// ------------------------------------------------------------------------------------------------

/// Our column layout: the rectangle of every widget, O(n) arithmetic.
pub fn own_layout(out: &mut Vec<Rect>) {
    out.clear();
    for row in 0..ROWS {
        for kind in [Kind::Label, Kind::Checkbox, Kind::Button] {
            out.push(rect(row, kind));
        }
    }
}

/// The same panel as a taffy tree: a flex column of rows, each a flex row of three fixed-size
/// leaves (the label's width given, as a measured text would be).
pub fn taffy_tree() -> (taffy::TaffyTree<()>, taffy::NodeId, Vec<taffy::NodeId>) {
    use taffy::prelude::*;
    let mut tree: TaffyTree<()> = TaffyTree::new();
    let leaf = |tree: &mut TaffyTree<()>, w: f32, h: f32| {
        tree.new_leaf(Style {
            size: Size {
                width: length(w),
                height: length(h),
            },
            margin: Rect {
                left: length(0.0),
                right: length(10.0),
                top: length(0.0),
                bottom: length(0.0),
            },
            ..Style::default()
        })
        .unwrap()
    };
    let mut leaves = Vec::new();
    let mut rows = Vec::new();
    for _ in 0..ROWS {
        let label = leaf(&mut tree, CHECK_X - LABEL_X - 10.0, ROW);
        let check = leaf(&mut tree, CHECK, CHECK);
        let button = leaf(&mut tree, BUTTON_W, BUTTON_H);
        leaves.extend([label, check, button]);
        let row = tree
            .new_with_children(
                Style {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::CENTER,
                    size: Size {
                        width: auto(),
                        height: length(ROW),
                    },
                    ..Style::default()
                },
                &[label, check, button],
            )
            .unwrap();
        rows.push(row);
    }
    let root = tree
        .new_with_children(
            Style {
                flex_direction: FlexDirection::Column,
                padding: Rect {
                    left: length(LABEL_X),
                    right: length(0.0),
                    top: length(TOP),
                    bottom: length(TOP),
                },
                ..Style::default()
            },
            &rows,
        )
        .unwrap();
    (tree, root, leaves)
}

/// Lays the taffy tree out for the panel's width.
pub fn taffy_compute(tree: &mut taffy::TaffyTree<()>, root: taffy::NodeId) {
    use taffy::prelude::*;
    tree.compute_layout(
        root,
        Size {
            width: AvailableSpace::Definite(WIDTH),
            height: AvailableSpace::MaxContent,
        },
    )
    .unwrap();
}
