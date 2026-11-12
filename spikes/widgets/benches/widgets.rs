// SPDX-License-Identifier: GPL-3.0-or-later

//! Spike F2.08: instruction counts of retained and immediate widgets (see `src/lib.rs`).
//!
//! Each case starts from a panel already shown once (fonts parsed, glyph cache warm, the first
//! frame drawn) and measures one event end to end: the UI's work, the AccessKit update and the
//! drawing of the damage into the 510 × 1868 image. The `_logic` cases leave the drawing out.
//! Counts, not times: the project's rule is 1 instruction ≈ 1 ns on the reference laptop.

use std::hint::black_box;

use accesskit::TreeUpdate;
use gungraun::prelude::*;
use orchestra_text::{Faces, Shaper};
use orchestra_ui::{
    Appearance, BuiltInTheme, CpuRenderer, DisplayList, Image, Point, Renderer, Theme,
};
use widgets::{
    HEIGHT, Immediate, Input, Retained, WIDTH, empty_update, own_layout, taffy_compute, taffy_tree,
    texts,
};

// A click on the checkbox of row 33 and a move onto the button of row 40.
fn click() -> Input {
    Input::Click(Point::new(328.0, 10.0 + 33.0 * 28.0 + 14.0))
}
fn hover() -> Input {
    Input::Move(Point::new(420.0, 10.0 + 40.0 * 28.0 + 14.0))
}

struct Common {
    faces: Faces,
    shaper: Shaper,
    list: DisplayList,
    renderer: CpuRenderer,
    image: Image,
    theme: Theme,
    update: TreeUpdate,
}

fn common() -> Common {
    let faces = Faces::bundled().unwrap();
    Common {
        renderer: CpuRenderer::new(faces.clone()),
        faces,
        shaper: Shaper::new(),
        list: DisplayList::new(),
        image: Image::new(WIDTH as u32, HEIGHT as u32).unwrap(),
        theme: *BuiltInTheme::Studio.pair().instance(Appearance::Light),
        update: empty_update(),
    }
}

// Draws the list's damage and clears the update: the end of a frame.
fn present(c: &mut Common) {
    let damage = c.list.take_damage();
    c.renderer
        .render(&c.list, &c.theme, &damage, 1.0, &mut c.image)
        .unwrap();
}

fn retained() -> (Common, Retained) {
    let mut c = common();
    let (labels, buttons) = texts();
    let panel = Retained::build(
        &c.faces,
        &mut c.shaper,
        labels,
        buttons,
        &mut c.list,
        &mut c.update,
    );
    present(&mut c);
    c.update.nodes.clear();
    (c, panel)
}

fn immediate(cached: bool) -> (Common, Immediate) {
    let mut c = common();
    let (labels, buttons) = texts();
    let mut panel = Immediate::new(labels, buttons, cached);
    panel.run(&c.faces, &mut c.shaper, None, &mut c.list, &mut c.update);
    present(&mut c);
    c.update.nodes.clear();
    // A second run, so B2's cache and every vector have their final capacity.
    panel.run(&c.faces, &mut c.shaper, None, &mut c.list, &mut c.update);
    present(&mut c);
    c.update.nodes.clear();
    (c, panel)
}

// The first frame of the retained panel from scratch (fonts parsed, nothing else).
fn fresh() -> (Common, Vec<String>, Vec<String>) {
    let (labels, buttons) = texts();
    (common(), labels, buttons)
}

#[library_benchmark]
#[bench::retained(fresh())]
fn first_frame_retained(input: (Common, Vec<String>, Vec<String>)) -> (Common, Retained) {
    let (mut c, labels, buttons) = input;
    let panel = Retained::build(
        &c.faces,
        &mut c.shaper,
        labels,
        buttons,
        &mut c.list,
        &mut c.update,
    );
    present(&mut c);
    (c, panel)
}

#[library_benchmark]
#[bench::click(retained(), click())]
#[bench::hover(retained(), hover())]
fn event_retained(state: (Common, Retained), event: Input) -> (Common, Retained) {
    let (mut c, mut panel) = state;
    panel.handle(black_box(event), &mut c.list, &mut c.update);
    present(&mut c);
    (c, panel)
}

#[library_benchmark]
#[bench::click(retained(), click())]
fn event_retained_logic(state: (Common, Retained), event: Input) -> (Common, Retained) {
    let (mut c, mut panel) = state;
    panel.handle(black_box(event), &mut c.list, &mut c.update);
    (c, panel)
}

#[library_benchmark]
#[bench::pure_click(immediate(false), click())]
#[bench::cached_click(immediate(true), click())]
#[bench::cached_hover(immediate(true), hover())]
fn event_immediate(state: (Common, Immediate), event: Input) -> (Common, Immediate) {
    let (mut c, mut panel) = state;
    panel.run(
        &c.faces,
        &mut c.shaper,
        Some(black_box(event)),
        &mut c.list,
        &mut c.update,
    );
    present(&mut c);
    (c, panel)
}

#[library_benchmark]
#[bench::pure_click(immediate(false), click())]
#[bench::cached_click(immediate(true), click())]
fn event_immediate_logic(state: (Common, Immediate), event: Input) -> (Common, Immediate) {
    let (mut c, mut panel) = state;
    panel.run(
        &c.faces,
        &mut c.shaper,
        Some(black_box(event)),
        &mut c.list,
        &mut c.update,
    );
    (c, panel)
}

#[library_benchmark]
#[bench::own(Vec::with_capacity(256))]
fn layout_own(mut out: Vec<orchestra_ui::Rect>) -> Vec<orchestra_ui::Rect> {
    own_layout(black_box(&mut out));
    out
}

#[library_benchmark]
#[bench::first(taffy_tree())]
fn layout_taffy(
    input: (taffy::TaffyTree<()>, taffy::NodeId, Vec<taffy::NodeId>),
) -> taffy::TaffyTree<()> {
    let (mut tree, root, _) = input;
    taffy_compute(black_box(&mut tree), root);
    tree
}

// Taffy after one leaf changed (marked dirty): its cache spares the clean subtrees.
fn taffy_dirty() -> (taffy::TaffyTree<()>, taffy::NodeId, Vec<taffy::NodeId>) {
    let (mut tree, root, leaves) = taffy_tree();
    taffy_compute(&mut tree, root);
    tree.mark_dirty(leaves[100]).unwrap();
    (tree, root, leaves)
}

#[library_benchmark]
#[bench::one_dirty(taffy_dirty())]
fn relayout_taffy(
    input: (taffy::TaffyTree<()>, taffy::NodeId, Vec<taffy::NodeId>),
) -> taffy::TaffyTree<()> {
    let (mut tree, root, _) = input;
    taffy_compute(black_box(&mut tree), root);
    tree
}

library_benchmark_group!(
    name = panel,
    benchmarks = [
        first_frame_retained,
        event_retained,
        event_retained_logic,
        event_immediate,
        event_immediate_logic,
        layout_own,
        layout_taffy,
        relayout_taffy
    ]
);

main!(library_benchmark_groups = panel);
