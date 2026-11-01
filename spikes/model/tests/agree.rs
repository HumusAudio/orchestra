// SPDX-License-Identifier: GPL-3.0-or-later

//! The three candidates must hold the same music after every measured operation: otherwise the
//! measurements would compare different work. Every test runs the operation on P1, P2 and P3 and
//! compares the sorted listings `(voice, absolute position, id)`.
//!
//! The keep-bars meter change is checked only between P2 and P3 against a hand-computed
//! expectation, because P1 cannot represent overfull bars (see `p1.rs`): it produces the same
//! positions, which is what is checked, but loses which bar the overflowing events belong to.

use model::p1::P1;
use model::p2::P2;
use model::p3::P3;
use model::*;

/// Runs `op` on the three candidates built from the normal score and compares their listings
/// in `meter` (the map after the edit).
fn agree(
    meter_after: impl Fn(&Spec) -> orchestra_core_time::MeterMap,
    op: impl Fn(&mut dyn Op, &Spec, &orchestra_core_time::MeterMap),
) {
    let spec = generate(Size::Normal);
    let after = meter_after(&spec);
    let mut p1 = P1::build(&spec);
    let mut p2 = P2::build(&spec);
    let mut p3 = P3::build(&spec);
    op(&mut p1, &spec, &after);
    op(&mut p2, &spec, &after);
    op(&mut p3, &spec, &after);
    let l1 = listing(&p1, &after);
    let l2 = listing(&p2, &after);
    let l3 = listing(&p3, &after);
    assert_eq!(l1, l2, "P1 and P2 differ");
    assert_eq!(l2, l3, "P2 and P3 differ");
}

/// Object-safe wrapper over [`Model`], so one closure can drive the three candidates.
trait Op {
    fn run(&mut self, name: &str, spec: &Spec, after: &orchestra_core_time::MeterMap);
}

impl<M: Model> Op for M {
    fn run(&mut self, name: &str, spec: &Spec, after: &orchestra_core_time::MeterMap) {
        let before = &spec.meter;
        match name {
            "none" => {}
            "insert" => {
                for (v, at, e) in probes(spec).inserts {
                    self.insert(before, v, at, e);
                }
            }
            "delete" => {
                for id in probes(spec).ids {
                    assert!(self.delete(id).is_some());
                }
            }
            "insert_bar" => self.insert_bar(before, after, INSERT_BAR),
            "lengthen" => self.lengthen_bar(before, after, FREE_BAR),
            "keep" => self.meter_keep_bars(before, after, METER_BAR),
            "rewrite" => {
                self.meter_rewrite(before, after, METER_BAR, METER_UNTIL, METER_UNTIL_REWRITTEN)
            }
            _ => unreachable!(),
        }
    }
}

#[test]
fn score_sizes() {
    let (events, notes) = generate(Size::Normal).counts();
    println!("normal: {events} events, {notes} notes");
    assert!(
        (8_000..=10_500).contains(&notes),
        "normal score: {notes} notes"
    );
    let (events, notes) = generate(Size::Heavy).counts();
    println!("heavy: {events} events, {notes} notes");
    assert!(
        (220_000..=300_000).contains(&notes),
        "heavy score: {notes} notes"
    );
}

#[test]
fn build() {
    agree(|s| s.meter.clone(), |m, s, a| m.run("none", s, a));
}

#[test]
fn insert_and_delete() {
    agree(|s| s.meter.clone(), |m, s, a| m.run("insert", s, a));
    agree(|s| s.meter.clone(), |m, s, a| m.run("delete", s, a));
}

#[test]
fn insert_bar() {
    agree(
        |s| after_insert_bar(&s.meter),
        |m, s, a| m.run("insert_bar", s, a),
    );
}

#[test]
fn lengthen_free_bar() {
    agree(
        |s| after_lengthen(&s.meter),
        |m, s, a| m.run("lengthen", s, a),
    );
}

#[test]
fn meter_keep_bars() {
    agree(|s| after_keep_bars(&s.meter), |m, s, a| m.run("keep", s, a));
}

#[test]
fn meter_rewrite() {
    agree(
        |s| after_rewrite(&s.meter),
        |m, s, a| m.run("rewrite", s, a),
    );
}

#[test]
fn find_and_collect() {
    let spec = generate(Size::Normal);
    let p1 = P1::build(&spec);
    let p2 = P2::build(&spec);
    let p3 = P3::build(&spec);
    for id in probes(&spec).ids {
        let f1 = p1.find(&spec.meter, id);
        assert!(f1.is_some());
        assert_eq!(f1, p2.find(&spec.meter, id));
        assert_eq!(f1, p3.find(&spec.meter, id));
    }
    let collect = |m: &dyn Fn(&mut Vec<_>)| {
        let mut out = Vec::new();
        m(&mut out);
        out.sort();
        out
    };
    let c1 = collect(&|o| p1.collect(&spec.meter, COLLECT, o));
    assert!(!c1.is_empty());
    assert_eq!(c1, collect(&|o| p2.collect(&spec.meter, COLLECT, o)));
    assert_eq!(c1, collect(&|o| p3.collect(&spec.meter, COLLECT, o)));
}

#[test]
fn snapshot_is_independent() {
    let spec = generate(Size::Normal);
    let mut p3 = P3::build(&spec);
    let snap = p3.snapshot();
    let before: usize = snap.iter().flatten().flatten().map(|c| c.len()).sum();
    for (v, at, e) in probes(&spec).inserts {
        p3.insert(&spec.meter, v, at, e);
    }
    let after: usize = snap.iter().flatten().flatten().map(|c| c.len()).sum();
    assert_eq!(before, after, "an edit leaked into the snapshot");
}
