// SPDX-License-Identifier: GPL-3.0-or-later

//! The unrolling of the form, case by case (ADR-0025 §5 and §10; SCALETTA F1.12): the hard cases
//! of ADR-0025 §10 and every rule of the walk, each on a small score built with the real builder,
//! checked against the path a musician reads.

#![allow(
    clippy::unwrap_used,
    reason = "test helpers: a failed unwrap fails the test"
)]

use orchestra_core_model::{
    BarId, BarRepeatSpan, ChangeSet, Clef, Command, Document, DocumentBuilder, DynamicScope,
    Editor, GradualMark, Interval, JumpFires, JumpSpec, JumpTarget, Key, MarkerKind, Metronome,
    Mode, PassSet, RepeatsAfterJump, ScorePoint, StaffId, TempoMark, TempoMarkChange, VoiceId,
};
use orchestra_core_time::{
    BarIndex, Duration, Meter, MeterChange, MeterMap, Rational, Tempo, TimeAt,
};
use orchestra_playback::Unrolled;

/// A score being built: the builder, its bars, and its one instrument's voice and staff.
struct Score {
    /// The builder.
    b: DocumentBuilder,
    /// The bars.
    bars: Vec<BarId>,
    /// The flute's voice.
    voice: VoiceId,
    /// The flute's staff.
    staff: StaffId,
    /// The flute.
    instrument: orchestra_core_model::InstrumentId,
}

/// A tempo of `bpm` quarters per minute.
fn tempo(bpm: i64) -> Tempo {
    Tempo::new(
        Duration::from_rational(Rational::new(1, 4).unwrap()),
        Rational::from_integer(bpm).unwrap(),
    )
    .unwrap()
}

/// `n` bars of 4/4 at ♩ = 120 (two seconds a bar), one flute.
fn score(n: usize) -> Score {
    let meters = MeterMap::new(MeterChange::new(Meter::new(&[4], 4).unwrap())).unwrap();
    let mut b = DocumentBuilder::new(
        meters,
        TempoMark::immediate(None, Metronome::shown(tempo(120))).unwrap(),
        Key::traditional(0, Mode::Major).unwrap(),
    )
    .unwrap();
    let bars = (0..n).map(|_| b.add_bar().unwrap()).collect();
    let track = b
        .add_instrument_track("Flute", Interval::new(0, Rational::ZERO), &[Clef::TREBLE])
        .unwrap();
    let instrument = b.document().track(track).unwrap().instrument();
    let (voice, staff, instrument) = (
        instrument.voices()[0].id(),
        instrument.staves()[0].id(),
        instrument.id(),
    );
    Score {
        b,
        bars,
        voice,
        staff,
        instrument,
    }
}

/// The written bars of the path, by index.
fn path(doc: &Document) -> Vec<u32> {
    Unrolled::new(doc)
        .unwrap()
        .played()
        .iter()
        .map(|p| p.index.get())
        .collect()
}

/// A jump to the start or a segno, until a marker, with the given repeats and firing.
fn jump(to: JumpTarget, until: Option<orchestra_core_model::MarkerId>) -> JumpSpec {
    JumpSpec {
        to,
        until,
        then: None,
        repeats: RepeatsAfterJump::Skip,
        fires: JumpFires::AfterRepeats,
    }
}

/// Without form markings the path is the score: every bar once, pass 0, no section.
#[test]
fn a_score_without_form_is_played_in_order() {
    let s = score(4);
    let doc = s.b.build().unwrap();
    let unrolled = Unrolled::new(&doc).unwrap();
    assert_eq!(path(&doc), [0, 1, 2, 3]);
    assert!(
        unrolled
            .played()
            .iter()
            .all(|p| p.pass == 0 && p.section_play.is_none())
    );
    // Two seconds a bar, exactly; the end after four bars.
    assert_eq!(
        unrolled.time_at(&doc, 3, Duration::ZERO).unwrap().to_f64(),
        6.0
    );
    assert_eq!(
        unrolled.end().as_rational(),
        Rational::from_integer(4).unwrap()
    );
}

/// A repeat ×3 (ADR-0025 §10): the section's bars three times, passes 0–2, plays 1–3.
#[test]
fn a_repeat_three_times() {
    let mut s = score(4);
    s.b.add_repeat_start(s.bars[1]).unwrap();
    s.b.add_repeat_end(s.bars[2], 3).unwrap();
    let doc = s.b.build().unwrap();
    let unrolled = Unrolled::new(&doc).unwrap();
    assert_eq!(path(&doc), [0, 1, 2, 1, 2, 1, 2, 3]);
    let second_plays: Vec<_> = unrolled
        .played()
        .iter()
        .filter(|p| p.index.get() == 2)
        .map(|p| (p.pass, p.section_play))
        .collect();
    assert_eq!(second_plays, [(0, Some(1)), (1, Some(2)), (2, Some(3))]);
    assert_eq!(unrolled.plays_of(BarIndex::new(1)), [1, 3, 5]);
    // The fourth bar starts after seven bars of two seconds: the shift adds the repeats exactly.
    let shift = unrolled.played()[7].shift;
    assert_eq!(shift.to_f64(), 8.0);
    assert!(matches!(shift, TimeAt::Exact(_)));
}

/// An end repeat without a start goes back to the start of the score (ADR-0025 F2).
#[test]
fn an_end_without_start_goes_back_to_bar_0() {
    let mut s = score(4);
    s.b.add_repeat_end(s.bars[1], 2).unwrap();
    let doc = s.b.build().unwrap();
    assert_eq!(path(&doc), [0, 1, 0, 1, 2, 3]);
}

/// First and second ending: bar 2 on pass 1 (with the end repeat), bar 3 on pass 2.
#[test]
fn first_and_second_ending() {
    let mut s = score(5);
    s.b.add_repeat_start(s.bars[1]).unwrap();
    s.b.add_repeat_end(s.bars[2], 2).unwrap();
    s.b.add_volta(s.bars[2], s.bars[2], PassSet::of(&[1]).unwrap(), true, None)
        .unwrap();
    s.b.add_volta(
        s.bars[3],
        s.bars[3],
        PassSet::of(&[2]).unwrap(),
        false,
        None,
    )
    .unwrap();
    let doc = s.b.build().unwrap();
    assert_eq!(path(&doc), [0, 1, 2, 1, 3, 4]);
}

/// "1.–3." and "4.": four plays, the first ending three times.
#[test]
fn a_first_ending_for_three_passes() {
    let mut s = score(5);
    s.b.add_repeat_start(s.bars[1]).unwrap();
    s.b.add_repeat_end(s.bars[2], 4).unwrap();
    s.b.add_volta(
        s.bars[2],
        s.bars[2],
        PassSet::of(&[1, 2, 3]).unwrap(),
        true,
        None,
    )
    .unwrap();
    s.b.add_volta(
        s.bars[3],
        s.bars[3],
        PassSet::of(&[4]).unwrap(),
        false,
        None,
    )
    .unwrap();
    let doc = s.b.build().unwrap();
    assert_eq!(path(&doc), [0, 1, 2, 1, 2, 1, 2, 1, 3, 4]);
}

/// "1. :‖ 2. :‖ 3.": the second ending repeats too (ADR-0025 §8).
#[test]
fn three_endings_each_but_the_last_repeating() {
    let mut s = score(6);
    s.b.add_repeat_start(s.bars[1]).unwrap();
    s.b.add_repeat_end(s.bars[2], 3).unwrap();
    s.b.add_repeat_end(s.bars[3], 3).unwrap();
    for (bar, pass) in [(2, 1), (3, 2), (4, 3)] {
        s.b.add_volta(
            s.bars[bar],
            s.bars[bar],
            PassSet::of(&[pass]).unwrap(),
            pass < 3,
            None,
        )
        .unwrap();
    }
    let doc = s.b.build().unwrap();
    assert_eq!(path(&doc), [0, 1, 2, 1, 3, 1, 4, 5]);
}

/// D.C. al Fine: back to the start after bar 5, stop at the Fine of bar 2.
#[test]
fn da_capo_al_fine() {
    let mut s = score(6);
    let fine = s.b.add_marker(s.bars[2], MarkerKind::Fine, None).unwrap();
    s.b.add_jump(s.bars[5], JumpSpec::da_capo().al_fine(fine), None)
        .unwrap();
    let doc = s.b.build().unwrap();
    assert_eq!(path(&doc), [0, 1, 2, 3, 4, 5, 0, 1, 2]);
    let unrolled = Unrolled::new(&doc).unwrap();
    assert_eq!(unrolled.played()[6].pass, 1);
}

/// D.S. al Coda with a repeat inside the segno section (ADR-0025 §10): the jump on the last pass,
/// the repeat skipped after it, then the coda.
#[test]
fn dal_segno_al_coda_with_a_repeat() {
    let mut s = score(8);
    let segno = s.b.add_marker(s.bars[1], MarkerKind::Segno, None).unwrap();
    s.b.add_repeat_start(s.bars[1]).unwrap();
    s.b.add_repeat_end(s.bars[2], 2).unwrap();
    let to_coda = s.b.add_marker(s.bars[3], MarkerKind::ToCoda, None).unwrap();
    let coda = s.b.add_marker(s.bars[6], MarkerKind::Coda, None).unwrap();
    s.b.add_jump(
        s.bars[5],
        JumpSpec::dal_segno(segno).al_coda(to_coda, coda),
        None,
    )
    .unwrap();
    let doc = s.b.build().unwrap();
    assert_eq!(path(&doc), [0, 1, 2, 1, 2, 3, 4, 5, 1, 2, 3, 6, 7]);
    // After the jump the repeat is skipped: its section is played once, as play 1.
    let unrolled = Unrolled::new(&doc).unwrap();
    assert_eq!(unrolled.played()[8].section_play, Some(1));
    assert_eq!(unrolled.played()[8].pass, 2);
}

/// D.C. al Fine with `repeats: Play`: the repeat is played again after the jump.
#[test]
fn da_capo_playing_the_repeats_again() {
    let mut s = score(5);
    s.b.add_repeat_start(s.bars[0]).unwrap();
    s.b.add_repeat_end(s.bars[1], 2).unwrap();
    let fine = s.b.add_marker(s.bars[2], MarkerKind::Fine, None).unwrap();
    let mut spec = JumpSpec::da_capo().al_fine(fine);
    spec.repeats = RepeatsAfterJump::Play;
    s.b.add_jump(s.bars[4], spec, None).unwrap();
    let doc = s.b.build().unwrap();
    assert_eq!(path(&doc), [0, 1, 0, 1, 2, 3, 4, 0, 1, 0, 1, 2]);
}

/// D.S.S.: two segni, each jump to its own, each taken once.
#[test]
fn two_segni() {
    let mut s = score(8);
    let first = s.b.add_marker(s.bars[1], MarkerKind::Segno, None).unwrap();
    let second = s.b.add_marker(s.bars[4], MarkerKind::Segno, None).unwrap();
    s.b.add_jump(s.bars[3], jump(JumpTarget::Marker(first), None), None)
        .unwrap();
    s.b.add_jump(s.bars[5], jump(JumpTarget::Marker(second), None), None)
        .unwrap();
    let doc = s.b.build().unwrap();
    assert_eq!(path(&doc), [0, 1, 2, 3, 1, 2, 3, 4, 5, 4, 5, 6, 7]);
}

/// A jump inside a repeated section fires after the last pass by default, or on the pass named.
#[test]
fn a_jump_inside_a_repeat() {
    for (fires, expected) in [
        (JumpFires::AfterRepeats, vec![0, 1, 2, 1, 2, 0, 1, 2, 3]),
        (JumpFires::OnPass(1), vec![0, 1, 2, 0, 1, 2, 3]),
    ] {
        let mut s = score(4);
        s.b.add_repeat_start(s.bars[1]).unwrap();
        s.b.add_repeat_end(s.bars[2], 2).unwrap();
        let mut spec = JumpSpec::da_capo();
        spec.fires = fires;
        s.b.add_jump(s.bars[2], spec, None).unwrap();
        let doc = s.b.build().unwrap();
        assert_eq!(path(&doc), expected, "{fires:?}");
    }
}

/// A two-bar repeat sign on the flute (bars 3–4 repeat bars 1–2): those bars take their notes from
/// the source bars, with the pass of those notes (P3).
#[test]
fn bar_repeats_play_their_source() {
    let mut s = score(6);
    s.b.add_bar_repeat(s.staff, s.bars[3], 2, BarRepeatSpan::Two)
        .unwrap();
    let staff = s.staff;
    let bars = s.bars.clone();
    let doc = s.b.build().unwrap();
    let unrolled = Unrolled::new(&doc).unwrap();
    let content = |k| unrolled.content(k, staff).unwrap();
    assert_eq!((content(3).bar, content(3).pass), (bars[1], 1));
    assert_eq!((content(4).bar, content(4).pass), (bars[2], 1));
    assert_eq!((content(5).bar, content(5).pass), (bars[5], 0));
    // The played bar itself is still bar 3, played for the first time.
    assert_eq!(unrolled.played()[3].pass, 0);
}

/// A two-bar repeat sign written twice (bars 3–6 repeat bars 1–2, then 1–2 again): the third and
/// fourth repeated bars take their notes from bars 1 and 2 again, a pass later (P3).
#[test]
fn a_bar_repeat_over_several_spans() {
    let mut s = score(8);
    s.b.add_bar_repeat(s.staff, s.bars[3], 4, BarRepeatSpan::Two)
        .unwrap();
    let staff = s.staff;
    let bars = s.bars.clone();
    let doc = s.b.build().unwrap();
    let unrolled = Unrolled::new(&doc).unwrap();
    let content = |k| {
        let c = unrolled.content(k, staff).unwrap();
        (c.bar, c.pass)
    };
    assert_eq!(
        (3..7).map(content).collect::<Vec<_>>(),
        [(bars[1], 1), (bars[2], 1), (bars[1], 2), (bars[2], 2)]
    );
}

/// "Tacet 1st time" on the flute over bars 1–2 of a repeated section (P4).
#[test]
fn a_pass_rule_silences_the_first_time() {
    let mut s = score(4);
    s.b.add_repeat_start(s.bars[1]).unwrap();
    s.b.add_repeat_end(s.bars[2], 2).unwrap();
    s.b.add_pass_rule(
        s.instrument,
        ScorePoint::start_of(s.bars[1]),
        ScorePoint::start_of(s.bars[3]),
        DynamicScope::Instrument,
        PassSet::of(&[2]).unwrap(),
        None,
    )
    .unwrap();
    let voice = s.voice;
    let doc = s.b.build().unwrap();
    let unrolled = Unrolled::new(&doc).unwrap();
    let plays = |k| unrolled.plays(&doc, voice, k, Duration::ZERO).unwrap();
    // Path 0, 1, 2, 1, 2, 3.
    assert_eq!(
        (0..6).map(plays).collect::<Vec<_>>(),
        [true, false, false, true, true, true]
    );
}

/// Inside a gradual change the times are approximate, but a path that does not jump there keeps
/// an exact shift (see the module documentation).
#[test]
fn shifts_stay_exact_outside_gradual_changes() {
    let mut s = score(6);
    s.b.add_tempo_mark(
        ScorePoint::start_of(s.bars[3]),
        TempoMark::new(
            Some("accel."),
            Some(TempoMarkChange::Gradual(GradualMark {
                from: None,
                to: tempo(160),
                end: ScorePoint::start_of(s.bars[5]),
            })),
        )
        .unwrap(),
    )
    .unwrap();
    s.b.add_repeat_start(s.bars[0]).unwrap();
    s.b.add_repeat_end(s.bars[1], 2).unwrap();
    let doc = s.b.build().unwrap();
    let unrolled = Unrolled::new(&doc).unwrap();
    assert!(
        unrolled
            .played()
            .iter()
            .all(|p| matches!(p.shift, TimeAt::Exact(_)))
    );
    // Bar 4 (inside the accelerando) is approximate, the shift of its played bar exact.
    assert!(matches!(
        unrolled.time_at(&doc, 6, Duration::ZERO).unwrap(),
        TimeAt::Approximate(_)
    ));
}

/// The path is rebuilt only when the form, the bars or the maps change (P7).
#[test]
fn update_rebuilds_only_when_needed() {
    let mut s = score(4);
    s.b.add_repeat_start(s.bars[1]).unwrap();
    s.b.add_repeat_end(s.bars[2], 2).unwrap();
    let (voice, bars) = (s.voice, s.bars.clone());
    let mut editor = Editor::new(s.b.build().unwrap());
    let mut unrolled = Unrolled::new(editor.document()).unwrap();
    let note = Command::EnterEvent(orchestra_core_model::NewEvent {
        at: orchestra_core_model::Placement::new(voice, bars[0], Duration::ZERO),
        body: orchestra_core_model::NewBody::Rest(orchestra_core_model::RestValue::WholeBar),
    });
    let changes = editor.execute(&note).unwrap();
    assert!(!unrolled.update(editor.document(), &changes).unwrap());
    assert!(
        !unrolled
            .update(editor.document(), &ChangeSet::default())
            .unwrap()
    );
    let changes = editor
        .execute(&Command::RemoveMark(
            orchestra_core_model::ObjectRef::RepeatEnd(
                editor.document().form().repeat_ends()[0].id(),
            ),
        ))
        .unwrap();
    assert!(unrolled.update(editor.document(), &changes).unwrap());
    assert_eq!(unrolled.played().len(), 4);
}
