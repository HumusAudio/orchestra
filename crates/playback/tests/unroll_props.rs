// SPDX-License-Identifier: GPL-3.0-or-later

//! Property test of the unrolling (SCALETTA F1.12, P8): random forms built with the real builder
//! (the invalid ones are refused by `validate` and skipped: about 70% of them, so 4 096 cases give
//! about a thousand scores with repeats, voltas, jumps or bar repeats, measured on 2026-11-05), and
//! the invariants every path must keep, whatever the form:
//! - it is built, and is finite (no more than `bars × MAX_PLAYS × (jumps + 1)` bars);
//! - each played bar's pass index is the number of earlier plays of the same written bar, and the
//!   same holds for the notes a staff with bar repeats plays;
//! - the bars follow each other on the unrolled timeline without gaps, and the times never go
//!   backwards;
//! - "where is this bar played" agrees with the path;
//! - a section's play is between 1 and `MAX_PLAYS`;
//! - without any form marking the path is the score, bar by bar.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test helpers over their own small tables: a failure fails the test"
)]

use orchestra_core_model::{
    BarRepeatSpan, Clef, Document, DocumentBuilder, DynamicScope, Interval, JumpFires, JumpSpec,
    JumpTarget, Key, MAX_PLAYS, MarkerKind, Metronome, Mode, PassSet, RepeatsAfterJump, ScorePoint,
    TempoMark,
};
use orchestra_core_time::{BarIndex, Duration, Meter, MeterChange, MeterMap, Rational, Tempo};
use orchestra_playback::Unrolled;
use proptest::prelude::*;

/// A random score: `n` bars and random form markings, or `None` if `validate` refuses them.
#[allow(
    clippy::too_many_lines,
    reason = "one short arm per kind of random marking"
)]
fn random(n: usize, ops: &[(u8, usize, usize)]) -> Option<Document> {
    let meters = MeterMap::new(MeterChange::new(Meter::new(&[4], 4).unwrap())).unwrap();
    let tempo = Tempo::new(
        Duration::from_rational(Rational::new(1, 4).unwrap()),
        Rational::from_integer(120).unwrap(),
    )
    .unwrap();
    let mut b = DocumentBuilder::new(
        meters,
        TempoMark::immediate(None, Metronome::shown(tempo)).unwrap(),
        Key::traditional(0, Mode::Major).unwrap(),
    )
    .unwrap();
    let bars: Vec<_> = (0..n).map(|_| b.add_bar().unwrap()).collect();
    let track = b
        .add_instrument_track("Flute", Interval::new(0, Rational::ZERO), &[Clef::TREBLE])
        .unwrap();
    let (instrument, staff) = {
        let i = b.document().track(track).unwrap().instrument();
        (i.id(), i.staves()[0].id())
    };
    let mut markers = Vec::new();
    for &(op, a, c) in ops {
        let bar = bars[a % n];
        // Every failure is simply skipped: `build` refuses the invalid combinations.
        match op % 9 {
            0 => {
                let _ = b.add_repeat_start(bar);
            }
            1 => {
                let _ = b.add_repeat_end(bar, u8::try_from(2 + c % 3).unwrap());
            }
            2 => {
                // A volta group: "1." on `bar` with its end repeat, "2." on the next bar.
                let next = bars[(a + 1) % n];
                let _ = b.add_repeat_end(bar, 2);
                let _ = b.add_volta(bar, bar, PassSet::of(&[1]).unwrap(), true, None);
                let _ = b.add_volta(next, next, PassSet::of(&[2]).unwrap(), false, None);
            }
            3 => {
                let kind = [
                    MarkerKind::Segno,
                    MarkerKind::Fine,
                    MarkerKind::ToCoda,
                    MarkerKind::Coda,
                ][c % 4];
                if let Ok(id) = b.add_marker(bar, kind, None) {
                    markers.push((id, kind));
                }
            }
            4 | 5 => {
                // A D.C. or D.S., maybe al Fine or al Coda, with random repeats and firing.
                let pick = |kind| {
                    markers
                        .iter()
                        .filter(|(_, k)| *k == kind)
                        .map(|(id, _)| *id)
                        .nth(c % 2)
                };
                let to = if op % 9 == 4 {
                    JumpTarget::Start
                } else {
                    match pick(MarkerKind::Segno) {
                        Some(segno) => JumpTarget::Marker(segno),
                        None => continue,
                    }
                };
                let mut spec = JumpSpec {
                    to,
                    ..JumpSpec::da_capo()
                };
                match c % 3 {
                    1 => {
                        if let Some(fine) = pick(MarkerKind::Fine) {
                            spec = spec.al_fine(fine);
                        }
                    }
                    2 => {
                        if let (Some(t), Some(coda)) =
                            (pick(MarkerKind::ToCoda), pick(MarkerKind::Coda))
                        {
                            spec = spec.al_coda(t, coda);
                        }
                    }
                    _ => {}
                }
                if c % 5 == 0 {
                    spec.repeats = RepeatsAfterJump::Play;
                }
                if c % 7 == 0 {
                    spec.fires = JumpFires::OnPass(1);
                }
                let _ = b.add_jump(bar, spec, None);
            }
            6 => {
                let _ = b.add_bar_repeat(staff, bar, 1, BarRepeatSpan::One);
            }
            7 => {
                let end = bars[(a + 1 + c % 2) % n];
                let _ = b.add_pass_rule(
                    instrument,
                    ScorePoint::start_of(bar),
                    ScorePoint::start_of(end),
                    DynamicScope::Instrument,
                    PassSet::of(&[u8::try_from(1 + c % 2).unwrap()]).unwrap(),
                    None,
                );
            }
            _ => {}
        }
    }
    b.build().ok()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(4096))]

    /// Every path keeps the invariants of the module documentation.
    #[test]
    fn every_path_keeps_its_invariants(
        n in 3_usize..12,
        ops in prop::collection::vec((0_u8..9, 0_usize..32, 0_usize..32), 0..14),
    ) {
        let Some(doc) = random(n, &ops) else { return Ok(()) };
        let unrolled = Unrolled::new(&doc).unwrap();
        let played = unrolled.played();
        let form = doc.form();
        let jumps = form.jumps().len();
        let bound = n * usize::from(MAX_PLAYS) * (jumps + 1);
        prop_assert!(played.len() <= bound);
        let meter = doc.meter_map();
        let mut counts = vec![0_u32; n];
        let mut last_time = f64::NEG_INFINITY;
        for (k, p) in played.iter().enumerate() {
            let i = usize::try_from(p.index.get()).unwrap();
            prop_assert_eq!(p.pass, counts[i]);
            counts[i] += 1;
            prop_assert_eq!(doc.bars().id_at(p.index), Some(p.bar));
            if let Some(play) = p.section_play {
                prop_assert!((1..=MAX_PLAYS).contains(&play));
            }
            if k > 0 {
                let before = &played[k - 1];
                let length = meter.bar_length(before.index).unwrap();
                prop_assert_eq!(p.start, before.start.checked_add(length).unwrap());
            }
            let time = unrolled.time_at(&doc, k, Duration::ZERO).unwrap().to_f64();
            prop_assert!(time >= last_time - 1e-9, "time went back at {}", k);
            last_time = time;
            prop_assert!(unrolled.plays_of(p.index).contains(&u32::try_from(k).unwrap()));
        }
        for (i, &count) in counts.iter().enumerate() {
            let index = BarIndex::new(u32::try_from(i).unwrap());
            prop_assert_eq!(unrolled.plays_of(index).len(), usize::try_from(count).unwrap());
        }
        // The staves with bar repeats: the passes of the notes they play are counts too.
        for track in doc.tracks() {
            for staff in track.instrument().staves() {
                let mut seen = std::collections::HashMap::new();
                for k in 0..played.len() {
                    let content = unrolled.content(k, staff.id()).unwrap();
                    let count = seen.entry(content.bar).or_insert(0_u32);
                    prop_assert_eq!(content.pass, *count);
                    *count += 1;
                }
            }
        }
        let has_form = !form.repeat_starts().is_empty()
            || !form.repeat_ends().is_empty()
            || jumps > 0;
        if !has_form {
            let path: Vec<u32> = played.iter().map(|p| p.index.get()).collect();
            let expected: Vec<u32> = (0..u32::try_from(n).unwrap()).collect();
            prop_assert_eq!(path, expected);
        }
    }
}
