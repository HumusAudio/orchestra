// SPDX-License-Identifier: GPL-3.0-or-later

//! The MIDI export, choice by choice (ADR-0032 E1–E11; SCALETTA F1.18): small scores built with
//! the real builder, the file read back with `midly` (the independent oracle of E2), and one case
//! checked byte by byte against bytes written out by hand.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic,
    reason = "test helpers: a failed unwrap, index or panic fails the test"
)]

#[path = "common/midi.rs"]
mod midi;

use midi::{Decoded, Ev, Score, common_time, d, decode, export_doc, p4};
use orchestra_core_model::{
    Alteration, Articulation, ArticulationSet, BaseValue, Document, DynamicLevel, DynamicMark,
    DynamicPlacement, Grace, GraceAttach, GraceKind, GradualMark, Key, Metronome, Mode, NoteValue,
    Pitch, Placement, ScorePoint, Step, TempoMark, TempoMarkChange,
};
use orchestra_core_time::{
    BarIndex, BarKind, IrregularBar, Meter, MeterChange, Position, Rational, Tempo,
};
use orchestra_io::midi::{MidiError, MidiOptions, MidiWarning};
use orchestra_playback::{Performance, Unrolled};

/// Every note start of the performance that falls exactly on a tick must sound, by the file's own
/// clock, within 1 µs (960 units) of the path's time (ADR-0032 E4). Returns how many were checked.
fn assert_timing(doc: &Document, decoded: &Decoded, lead: u64, tolerance: f64) -> usize {
    let path = Unrolled::new(doc).unwrap();
    let performance = Performance::new(doc, &path).unwrap();
    let units = |at: Position| {
        let played = path.played();
        let i = played.partition_point(|b| b.start <= at).saturating_sub(1);
        let offset = at.checked_duration_since(played[i].start).unwrap();
        path.time_at(doc, i, offset).unwrap().to_f64() * 960_000_000.0
    };
    // The time of tick 0: the origin, `lead` ticks before position 0.
    let origin =
        Position::from_rational(Rational::new(-i64::try_from(lead).unwrap(), 3_840).unwrap());
    let zero = units(origin);
    let mut checked = 0;
    for (_, notes) in performance.instruments() {
        for note in notes {
            let ticks = note
                .start
                .as_rational()
                .checked_mul(Rational::from_integer(3_840).unwrap())
                .unwrap();
            if !ticks.is_integer() {
                continue;
            }
            let tick = u64::try_from(ticks.numerator() + i64::try_from(lead).unwrap()).unwrap();
            #[allow(clippy::cast_precision_loss, reason = "test tolerance on f64 seconds")]
            let reader = decoded.units_at(tick) as f64;
            let expected = units(note.start) - zero;
            assert!(
                (reader - expected).abs() <= tolerance,
                "note at tick {tick}: reader {reader}, path {expected}"
            );
            checked += 1;
        }
    }
    checked
}

/// E1–E3, E10: one quarter note C4 at ♩ = 120, byte by byte.
#[test]
fn one_note_byte_by_byte() {
    let mut s = Score::simple(1);
    let v = s.voice();
    s.chord(v, 0, d(0, 1), BaseValue::Quarter, &[p4(Step::C)]);
    let (_, out) = s.export();
    assert_eq!(out.warnings, []);
    let mut expected: Vec<u8> = vec![
        b'M', b'T', b'h', b'd', 0, 0, 0, 6, 0, 1, 0, 2, 0x03, 0xC0, // format 1, 2 tracks, 960
        b'M', b'T', b'r', b'k', 0, 0, 0, 19, // the conductor
        0x00, 0xFF, 0x51, 0x03, 0x07, 0xA1, 0x20, // ♩ = 120: 500 000 µs per quarter
        0x00, 0xFF, 0x58, 0x04, 4, 2, 24, 8, // 4/4
        0x00, 0xFF, 0x2F, 0x00, // end of track
        b'M', b'T', b'r', b'k', 0, 0, 0, 55, // the violin
        0x00, 0xFF, 0x03, 0x06, // track name, 6 bytes
    ];
    expected.extend_from_slice(b"Violin");
    expected.extend_from_slice(&[
        0x00, 0xB0, 101, 0, 0x00, 0xB0, 100, 0, // RPN 0 …
        0x00, 0xB0, 6, 2, 0x00, 0xB0, 38, 0, // … = 2 semitones, 0 cents
        0x00, 0xB0, 101, 127, 0x00, 0xB0, 100, 127, // null RPN
        0x00, 0xB0, 11, 127, // CC 11 at full
        0x00, 0xE0, 0x00, 0x40, // bend at the centre
        0x00, 0x90, 60, 75, // note-on, mf = 75 (ADR-0027 §4)
        0x87, 0x40, 0x80, 60, 64, // a quarter later (960 = 87 40), note-off
        0x00, 0xFF, 0x2F, 0x00,
    ]);
    assert_eq!(out.bytes, expected);
    // The independent reader agrees.
    let decoded = decode(&out.bytes);
    assert_eq!(decoded.division, 960);
    assert_eq!(decoded.tracks[0].name.as_deref(), Some("Violin"));
    assert_eq!(decoded.notes(0), [(0, 960, 0, 60, 75)]);
}

/// E10: a chord's note-ons at one tick, by key; exporting twice gives the same bytes.
#[test]
fn a_chord_is_sorted_and_deterministic() {
    let mut s = Score::simple(1);
    let v = s.voice();
    s.chord(
        v,
        0,
        d(0, 1),
        BaseValue::Half,
        &[p4(Step::G), p4(Step::C), p4(Step::E)],
    );
    let doc = s.b.build().unwrap();
    let out = export_doc(&doc, &MidiOptions::new());
    assert_eq!(out.bytes, export_doc(&doc, &MidiOptions::new()).bytes);
    let decoded = decode(&out.bytes);
    let ons: Vec<u8> = decoded.tracks[0]
        .events
        .iter()
        .filter_map(|(_, e)| match e {
            Ev::On { key, .. } => Some(*key),
            _ => None,
        })
        .collect();
    assert_eq!(ons, [60, 64, 67]);
}

/// E4: an immediate tempo change gives a second tempo event at its bar.
#[test]
fn a_tempo_change() {
    let mut s = Score::simple(2);
    let v = s.voice();
    let sixty = Tempo::new(d(1, 4), Rational::from_integer(60).unwrap()).unwrap();
    s.b.add_tempo_mark(
        ScorePoint::new(s.bars[1], d(0, 1)),
        TempoMark::immediate(None, Metronome::shown(sixty)).unwrap(),
    )
    .unwrap();
    s.chord(v, 0, d(0, 1), BaseValue::Whole, &[p4(Step::C)]);
    s.chord(v, 1, d(0, 1), BaseValue::Whole, &[p4(Step::D)]);
    let (doc, out) = s.export();
    let decoded = decode(&out.bytes);
    assert_eq!(decoded.tempos, [(0, 500_000), (3_840, 1_000_000)]);
    assert_eq!(assert_timing(&doc, &decoded, 0, 960.0), 2);
}

/// E4: inside an accelerando the tempo is stepped on a 32nd grid and every note start keeps its
/// time within 1 µs.
#[test]
fn an_accelerando_keeps_the_note_times() {
    let mut s = Score::new(3, common_time(), 60, &["Flute"]);
    let v = s.voice();
    let fast = Tempo::new(d(1, 4), Rational::from_integer(120).unwrap()).unwrap();
    s.b.add_tempo_mark(
        ScorePoint::new(s.bars[1], d(0, 1)),
        TempoMark::new(
            None,
            Some(TempoMarkChange::Gradual(GradualMark {
                from: None,
                to: fast,
                end: ScorePoint::new(s.bars[2], d(0, 1)),
            })),
        )
        .unwrap(),
    )
    .unwrap();
    for bar in 0..3 {
        for k in 0..8 {
            s.chord(v, bar, d(k, 8), BaseValue::Eighth, &[p4(Step::C)]);
        }
    }
    let (doc, out) = s.export();
    let decoded = decode(&out.bytes);
    // Constant before and after, stepped in between: more than one tempo event per 32nd of the
    // accelerando is impossible, fewer than one per eighth would be too coarse.
    assert_eq!(decoded.tempos.first(), Some(&(0, 1_000_000)));
    assert_eq!(decoded.tempos.last().map(|t| t.1), Some(500_000));
    let inside = decoded
        .tempos
        .iter()
        .filter(|(t, _)| (3_840..7_680).contains(t))
        .count();
    assert!(
        (8..=64).contains(&inside),
        "{inside} tempo events in the accelerando"
    );
    assert_eq!(assert_timing(&doc, &decoded, 0, 960.0), 24);
}

/// E5: an irregular bar gets a time signature of its real length, then the meter comes back; a
/// meter the format cannot write gets a marker and a warning.
#[test]
fn time_signatures_follow_the_bar_lengths() {
    let mut meters = common_time();
    meters
        .set_irregular_bar(
            BarIndex::new(1),
            IrregularBar {
                length: d(5, 8),
                kind: BarKind::AlignStart,
            },
        )
        .unwrap();
    meters
        .set_meter_change(
            BarIndex::new(3),
            MeterChange::new(Meter::new(&[4], 3).unwrap()),
        )
        .unwrap();
    let mut s = Score::new(4, meters, 120, &["Oboe"]);
    let v = s.voice();
    s.chord(v, 0, d(0, 1), BaseValue::Quarter, &[p4(Step::C)]);
    let (_, out) = s.export();
    let decoded = decode(&out.bytes);
    // 4/4 at 0, 5/8 at bar 1, 4/4 again at bar 2 (3 840 + 2 400 ticks).
    assert_eq!(
        decoded.signatures,
        [(0, 4, 2), (3_840, 5, 3), (6_240, 4, 2)]
    );
    assert_eq!(decoded.markers, [(10_080, "4/3".to_owned())]);
    assert_eq!(out.warnings, [MidiWarning::MeterNotWritable { played: 3 }]);
}

/// E9: a quarter-tone sharp note bends its channel; the next plain note brings it back.
#[test]
fn a_microtone_bends_the_channel() {
    let mut s = Score::simple(1);
    let v = s.voice();
    let quarter_sharp = Alteration::new(Rational::new(1, 2).unwrap()).unwrap();
    s.chord(
        v,
        0,
        d(0, 1),
        BaseValue::Quarter,
        &[Pitch::new(Step::C, quarter_sharp, 4)],
    );
    s.chord(v, 0, d(1, 4), BaseValue::Quarter, &[p4(Step::C)]);
    let (_, out) = s.export();
    assert_eq!(out.warnings, []);
    let decoded = decode(&out.bytes);
    let bends: Vec<(u64, u16)> = decoded.tracks[0]
        .events
        .iter()
        .filter_map(|(t, e)| match e {
            Ev::Bend { value, .. } => Some((*t, *value)),
            _ => None,
        })
        .collect();
    // The set-up's centre, C4 + ½ semitone as C♯4 bent down by ½ (−2 048), then the centre.
    assert_eq!(bends, [(0, 8_192), (0, 6_144), (960, 8_192)]);
    assert_eq!(
        decoded.notes(0),
        [(0, 960, 0, 61, 75), (960, 1_920, 0, 60, 75)]
    );
}

/// E9: two simultaneous notes needing different bends: the second is played on the nearest key
/// under the first one's bend, with a warning.
#[test]
fn conflicting_bends_are_rounded() {
    let mut s = Score::simple(1);
    let v = s.voice();
    let quarter_sharp = Alteration::new(Rational::new(1, 2).unwrap()).unwrap();
    s.chord(
        v,
        0,
        d(0, 1),
        BaseValue::Quarter,
        &[p4(Step::C), Pitch::new(Step::E, quarter_sharp, 4)],
    );
    let (_, out) = s.export();
    assert!(matches!(
        out.warnings[..],
        [MidiWarning::BendRounded { .. }]
    ));
    let decoded = decode(&out.bytes);
    // C4 first (by key, no bend), then E4 + ½, halfway, played as F4 (65, halves up) unbent.
    assert_eq!(decoded.notes(0), [(0, 960, 0, 60, 75), (0, 960, 0, 65, 75)]);
}

/// E10: two voices in unison: the second attack is heard again, and only the later end turns the
/// key off.
#[test]
fn a_unison_does_not_cut_the_other_voice() {
    let mut s = Score::simple(1);
    let (v1, staff) = (s.parts[0].1, s.parts[0].2);
    let v2 = s.b.add_voice(staff).unwrap();
    s.chord(v1, 0, d(0, 1), BaseValue::Whole, &[p4(Step::G)]);
    s.chord(v2, 0, d(1, 4), BaseValue::Quarter, &[p4(Step::G)]);
    let (_, out) = s.export();
    let decoded = decode(&out.bytes);
    let keys: Vec<(u64, &str)> = decoded.tracks[0]
        .events
        .iter()
        .filter_map(|(t, e)| match e {
            Ev::On { key: 67, .. } => Some((*t, "on")),
            Ev::Off { key: 67, .. } => Some((*t, "off")),
            _ => None,
        })
        .collect();
    assert_eq!(keys, [(0, "on"), (960, "off"), (960, "on"), (3_840, "off")]);
}

/// The CC 11 events of the first track: (tick, value), the set-up's first one excluded.
fn expressions(decoded: &Decoded) -> Vec<(u64, u8)> {
    decoded.tracks[0]
        .events
        .iter()
        .filter_map(|(t, e)| match e {
            Ev::Cc { cc: 11, val, .. } => Some((*t, *val)),
            _ => None,
        })
        .skip(1)
        .collect()
}

/// E8: an accent at ♩ = 60 on a half note: the note-on at the accented velocity, then CC 11 down
/// to the dynamic's level in 0.25 s (ADR-0027 H15, §15), in 10 ms steps.
#[test]
fn an_accent_fades_back_with_cc11() {
    let mut s = Score::new(1, common_time(), 60, &["Horn"]);
    let v = s.voice();
    let e = s.chord(v, 0, d(0, 1), BaseValue::Half, &[p4(Step::C)]);
    s.b.set_articulations(e, ArticulationSet::EMPTY.with(Articulation::Accent))
        .unwrap();
    let (_, out) = s.export();
    let decoded = decode(&out.bytes);
    // mf 75 + 15 = 90 at the attack (H15).
    assert_eq!(decoded.notes(0), [(0, 1_920, 0, 60, 90)]);
    let cc = expressions(&decoded);
    // Decreasing to round(127 × 75/90) = 106, the last point at 0.25 s (0.25 quarters = 240
    // ticks).
    assert!(
        cc.windows(2).all(|w| w[0].0 < w[1].0 && w[0].1 > w[1].1),
        "{cc:?}"
    );
    assert_eq!(cc.last(), Some(&(240, 106)));
    assert!(cc.len() >= 20, "about one step per 10 ms: {cc:?}");
}

/// E8: *pf*: the note-on at the *f* velocity, CC 11 starting at the *p* ratio and rising to 127;
/// *niente* never writes a 0 velocity.
#[test]
fn pf_rises_and_niente_is_not_silent() {
    let mut s = Score::simple(2);
    let (instrument, v, staff) = s.parts[0];
    s.chord(v, 0, d(0, 1), BaseValue::Whole, &[p4(Step::C)]);
    s.chord(v, 1, d(0, 1), BaseValue::Whole, &[p4(Step::D)]);
    s.b.add_dynamic(
        instrument,
        ScorePoint::new(s.bars[0], d(0, 1)),
        DynamicPlacement::instrument(staff),
        DynamicMark::Pf,
    )
    .unwrap();
    s.b.add_dynamic(
        instrument,
        ScorePoint::new(s.bars[1], d(0, 1)),
        DynamicPlacement::instrument(staff),
        DynamicMark::Niente,
    )
    .unwrap();
    let (_, out) = s.export();
    let decoded = decode(&out.bytes);
    let notes = decoded.notes(0);
    assert_eq!(notes[0].4, 88, "pf: the note-on at f");
    assert_eq!(notes[1].4, 1, "niente: velocity 1, not a note-off");
    let cc = expressions(&decoded);
    // p/f = 49/88 → 71, then up to 127 after the 0.5 s hold.
    assert_eq!(cc.first(), Some(&(0, 71)));
    assert!(cc.iter().any(|(_, v)| *v == 127));
    let _ = DynamicLevel::P;
}

/// E5: an acciaccatura before the very first note delays the file by a lead-in bar of one quarter,
/// with the caller's marker; no note is cut.
#[test]
fn a_lead_in_for_an_early_grace_note() {
    let mut s = Score::simple(1);
    let v = s.voice();
    s.b.add_grace_chord(
        Placement::new(v, s.bars[0], d(0, 1)),
        Grace {
            kind: GraceKind::Acciaccatura,
            attach: GraceAttach::Before,
        },
        NoteValue::plain(BaseValue::Eighth),
        &[p4(Step::D)],
    )
    .unwrap();
    s.chord(v, 0, d(0, 1), BaseValue::Quarter, &[p4(Step::C)]);
    let mut options = MidiOptions::new();
    options.set_lead_in_marker(Some("lead-in".to_owned()));
    let (doc, out) = s.export_with(&options);
    assert_eq!(out.warnings, [MidiWarning::LeadIn { quarters: 1 }]);
    let decoded = decode(&out.bytes);
    assert_eq!(decoded.signatures, [(0, 1, 2), (960, 4, 2)]);
    assert_eq!(decoded.markers, [(0, "lead-in".to_owned())]);
    let notes = decoded.notes(0);
    assert_eq!(notes.len(), 2);
    assert!(
        notes[0].0 < 960 && notes[0].1 <= 960,
        "the grace note in the lead-in: {notes:?}"
    );
    assert_eq!((notes[1].0, notes[1].3), (960, 60));
    assert_eq!(assert_timing(&doc, &decoded, 960, 960.0), 2);
}

/// E6, E7: seventeen instruments: channel 10 never used, the last two share channels 1 and 2 with
/// a warning each; a program change where asked.
#[test]
fn channels_and_programs() {
    let names: Vec<String> = (0..17).map(|i| format!("I{i}")).collect();
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    let mut s = Score::new(1, common_time(), 120, &names);
    // A different key for each instrument, so that the two sharing a channel do not merge
    // into one attack (E10).
    for (i, part) in s.parts.clone().into_iter().enumerate() {
        let pitch = Pitch::spell_midi(
            48 + u8::try_from(i).unwrap(),
            Key::traditional(0, Mode::Major).unwrap(),
        );
        s.chord(part.1, 0, d(0, 1), BaseValue::Quarter, &[pitch]);
    }
    let mut options = MidiOptions::new();
    options.set_program(s.parts[0].0, 40).unwrap();
    assert_eq!(
        options.set_program(s.parts[0].0, 128),
        Err(MidiError::InvalidProgram(128))
    );
    let shared: Vec<_> = s.parts[15..].iter().map(|p| p.0).collect();
    let (_, out) = s.export_with(&options);
    assert_eq!(
        out.warnings,
        [
            MidiWarning::ChannelShared {
                instrument: shared[0],
                channel: 0
            },
            MidiWarning::ChannelShared {
                instrument: shared[1],
                channel: 1
            },
        ]
    );
    let decoded = decode(&out.bytes);
    assert_eq!(decoded.tracks.len(), 17);
    let channels: Vec<u8> = (0..17).map(|t| decoded.notes(t)[0].2).collect();
    assert_eq!(
        channels,
        [0, 1, 2, 3, 4, 5, 6, 7, 8, 10, 11, 12, 13, 14, 15, 0, 1]
    );
    assert!(
        decoded.tracks[0]
            .events
            .contains(&(0, Ev::Program { ch: 0, program: 40 }))
    );
}

/// E11: saving writes the same bytes, safely.
#[test]
fn save_writes_the_bytes() {
    let mut s = Score::simple(1);
    let v = s.voice();
    s.chord(v, 0, d(0, 1), BaseValue::Quarter, &[p4(Step::C)]);
    let (_, out) = s.export();
    let dir = std::env::temp_dir().join(format!("orchestra-midi-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("one.mid");
    orchestra_io::midi::save(&out, &file).unwrap();
    assert_eq!(std::fs::read(&file).unwrap(), out.bytes);
    std::fs::remove_dir_all(&dir).unwrap();
}

/// The full preset (normal size), the reference score: the reader accepts it, every note-on has
/// its note-off, every played note is there, the times hold, no velocity is 0, the bytes are
/// deterministic.
#[test]
fn the_full_reference_score() {
    let doc = orchestra_test_scores::full(orchestra_test_scores::Size::Normal);
    let out = export_doc(&doc, &MidiOptions::new());
    assert_eq!(out.bytes, export_doc(&doc, &MidiOptions::new()).bytes);
    let decoded = decode(&out.bytes);
    let path = Unrolled::new(&doc).unwrap();
    let performance = Performance::new(&doc, &path).unwrap();
    // Every attack is written once: two notes of one key starting at the same tick on one channel
    // are one attack (E10). The instruments are fewer than 16, so a channel is an instrument.
    assert!(performance.instruments().count() <= 15);
    let mut attacks = std::collections::BTreeSet::new();
    for (track, (_, notes)) in performance.instruments().enumerate() {
        for n in notes {
            let tick = n
                .start
                .as_rational()
                .checked_mul(Rational::from_integer(3_840).unwrap())
                .unwrap();
            let key = n
                .pitch
                .checked_add(Rational::new(1, 2).unwrap())
                .unwrap()
                .floor();
            attacks.insert((track, key, tick));
        }
    }
    assert!(!out.warnings.iter().any(|w| matches!(
        w,
        MidiWarning::PitchOutOfRange { .. } | MidiWarning::BendRounded { .. }
    )));
    let written: usize = (0..decoded.tracks.len())
        .map(|t| decoded.notes(t).len())
        .sum();
    assert_eq!(written, attacks.len());
    for t in 0..decoded.tracks.len() {
        assert!(decoded.notes(t).iter().all(|n| n.4 >= 1));
    }
    let lead = out
        .warnings
        .iter()
        .find_map(|w| match w {
            MidiWarning::LeadIn { quarters } => Some(u64::from(*quarters) * 960),
            _ => None,
        })
        .unwrap_or(0);
    assert!(assert_timing(&doc, &decoded, lead, 96_000.0) > 1_000);
}
