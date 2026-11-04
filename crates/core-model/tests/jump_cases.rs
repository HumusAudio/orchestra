// SPDX-License-Identifier: GPL-3.0-or-later

//! Hand-written cases for the form, second part (ADR-0025 §4, §6–§7, SCALETTA F1.10b): markers,
//! jumps, bar repeats and pass rules, rules 10 and 11 of `validate`, and the builder's checks. The
//! cases of ADR-0025 §10 that belong to this part are named after it.
//!
//! Bars are written by their index from 0. Every builder is in 4/4 with a piano (two staves, one
//! voice each) and a trumpet.

// Helper functions (not `#[test]` functions) are outside the scope of clippy.toml's
// `allow-unwrap-in-tests`, so the exemption is stated here, for this test file only.
#![allow(
    clippy::unwrap_used,
    reason = "test helpers: a failed unwrap fails the test"
)]
#![allow(clippy::panic, reason = "test helpers: a panic fails the test")]

use orchestra_core_model::{
    Alteration, BarId, BarRepeatSpan, BaseValue, Clef, DocumentBuilder, DocumentError,
    DynamicScope, InstrumentId, Interval, JumpFires, JumpSpec, JumpTarget, Key, Marker, MarkerKind,
    Metronome, Mode, NoteValue, PassSet, Pitch, Placement, RepeatsAfterJump, ScorePoint, StaffId,
    Step, TempoMark, Violation, VoiceId,
};
use orchestra_core_time::{Duration, Meter, MeterChange, MeterMap, Rational, Tempo};

/// The parts of a test score.
struct Score {
    /// The builder.
    b: DocumentBuilder,
    /// The bars.
    bars: Vec<BarId>,
    /// The piano's staves (upper, lower) and voices (right hand, left hand).
    piano: (InstrumentId, [StaffId; 2], [VoiceId; 2]),
    /// The trumpet and its voice.
    trumpet: (InstrumentId, VoiceId),
}

/// A score of `bars` bars in 4/4.
fn score(bars: usize) -> Score {
    let meters = MeterMap::new(MeterChange::new(Meter::new(&[4], 4).unwrap())).unwrap();
    let tempo = Tempo::new(
        Duration::from_rational(Rational::new(1, 4).unwrap()),
        Rational::from_integer(100).unwrap(),
    )
    .unwrap();
    let mut b = DocumentBuilder::new(
        meters,
        TempoMark::immediate(None, Metronome::shown(tempo)).unwrap(),
        Key::traditional(0, Mode::Major).unwrap(),
    )
    .unwrap();
    let ids = (0..bars).map(|_| b.add_bar().unwrap()).collect();
    let unison = Interval::new(0, Rational::ZERO);
    let piano = b
        .add_instrument_track("Piano", unison, &[Clef::TREBLE, Clef::BASS])
        .unwrap();
    let trumpet = b
        .add_instrument_track("Trumpet", unison, &[Clef::TREBLE])
        .unwrap();
    let doc = b.document();
    let p = doc.track(piano).unwrap().instrument();
    let piano = (
        p.id(),
        [p.staves()[0].id(), p.staves()[1].id()],
        [p.voices()[0].id(), p.voices()[1].id()],
    );
    let t = doc.track(trumpet).unwrap().instrument();
    let trumpet = (t.id(), t.voices()[0].id());
    Score {
        b,
        bars: ids,
        piano,
        trumpet,
    }
}

/// The violation `build` reports.
fn violation(b: DocumentBuilder) -> Violation {
    match b.build() {
        Err(DocumentError::Invalid(v)) => v,
        other => panic!("expected a violation, got {other:?}"),
    }
}

/// The passes `first..=last`.
fn passes(first: u8, last: u8) -> PassSet {
    PassSet::range(first, last).unwrap()
}

/// ADR-0025 §10, "D.S. al Coda with a repeat inside the segno section": segno at bar 2, a repeated
/// chorus 2–3 whose end carries the D.S. (taken on the last pass), "To Coda" at the end of bar 3,
/// coda at bar 6.
#[test]
fn dal_segno_al_coda_around_a_repeat() {
    let mut s = score(8);
    let b = &mut s.b;
    let bars = &s.bars;
    let segno = b.add_marker(bars[2], MarkerKind::Segno, None).unwrap();
    b.add_repeat_start(bars[2]).unwrap();
    let to_coda = b.add_marker(bars[3], MarkerKind::ToCoda, None).unwrap();
    b.add_repeat_end(bars[4], 2).unwrap();
    let coda = b.add_marker(bars[6], MarkerKind::Coda, None).unwrap();
    let spec = JumpSpec::dal_segno(segno).al_coda(to_coda, coda);
    let jump = b.add_jump(bars[5], spec, None).unwrap();
    let document = s.b.build().unwrap();
    let form = document.form();
    assert_eq!(form.markers().len(), 3);
    assert_eq!(form.jumps()[0].id(), jump);
    let spec = form.jumps()[0].spec();
    assert_eq!(spec.to, JumpTarget::Marker(segno));
    assert_eq!((spec.until, spec.then), (Some(to_coda), Some(coda)));
    assert_eq!(spec.repeats, RepeatsAfterJump::Skip);
    assert_eq!(spec.fires, JumpFires::AfterRepeats);
    assert_eq!(document.marker(coda).unwrap().kind(), MarkerKind::Coda);
}

/// ADR-0025 §10, "D.C. al Fine with `repeats: Play`" and a free-text jump; a jump at the end of a
/// repeated section taken on a chosen pass.
#[test]
fn da_capo_al_fine_with_repeats_and_text() {
    let mut s = score(6);
    let b = &mut s.b;
    let bars = &s.bars;
    b.add_repeat_end(bars[1], 2).unwrap();
    let fine = b.add_marker(bars[1], MarkerKind::Fine, None).unwrap();
    let spec = JumpSpec {
        repeats: RepeatsAfterJump::Play,
        ..JumpSpec::da_capo().al_fine(fine)
    };
    b.add_jump(bars[3], spec, Some("D.C. con ritornelli al Fine"))
        .unwrap();
    // A jump inside a repeated section 4–5, three plays, taken on pass 2.
    b.add_repeat_start(bars[4]).unwrap();
    b.add_repeat_end(bars[5], 3).unwrap();
    let on_second = JumpSpec {
        fires: JumpFires::OnPass(2),
        ..JumpSpec::da_capo()
    };
    b.add_jump(bars[4], on_second, None).unwrap();
    let document = s.b.build().unwrap();
    assert_eq!(
        document.form().jumps()[0].text(),
        Some("D.C. con ritornelli al Fine")
    );
}

/// ADR-0025 §10, "D.S.S. (two segni)": each jump points at its own segno by ID.
#[test]
fn two_segni() {
    let mut s = score(8);
    let b = &mut s.b;
    let bars = &s.bars;
    let first = b.add_marker(bars[1], MarkerKind::Segno, None).unwrap();
    let second = b
        .add_marker(bars[4], MarkerKind::Segno, Some("𝄋𝄋"))
        .unwrap();
    b.add_jump(bars[3], JumpSpec::dal_segno(first), None)
        .unwrap();
    b.add_jump(bars[6], JumpSpec::dal_segno(second), Some("D.S.S."))
        .unwrap();
    let document = s.b.build().unwrap();
    let targets: Vec<_> = document
        .form()
        .jumps()
        .iter()
        .map(|j| j.spec().to)
        .collect();
    assert_eq!(
        targets,
        [JumpTarget::Marker(first), JumpTarget::Marker(second)]
    );
    assert_eq!(document.marker(second).unwrap().text(), Some("𝄋𝄋"));
}

/// ADR-0025 §10, "a jump to a coda": the kinds are checked (rule 10).
#[test]
fn jump_kinds_are_checked() {
    // D.S. to a coda.
    let mut s = score(6);
    let coda = s.b.add_marker(s.bars[1], MarkerKind::Coda, None).unwrap();
    let wrong =
        s.b.add_jump(s.bars[3], JumpSpec::dal_segno(coda), None)
            .unwrap();
    assert_eq!(violation(s.b), Violation::JumpTargetKind(wrong));

    // "Al Coda" without a coda to continue at, and "al Fine" pointing at a segno.
    let mut s = score(6);
    let to_coda = s.b.add_marker(s.bars[1], MarkerKind::ToCoda, None).unwrap();
    let spec = JumpSpec {
        until: Some(to_coda),
        ..JumpSpec::da_capo()
    };
    let missing = s.b.add_jump(s.bars[3], spec, None).unwrap();
    assert_eq!(violation(s.b), Violation::JumpTargetKind(missing));

    let mut s = score(6);
    let segno = s.b.add_marker(s.bars[1], MarkerKind::Segno, None).unwrap();
    let bad =
        s.b.add_jump(s.bars[3], JumpSpec::da_capo().al_fine(segno), None)
            .unwrap();
    assert_eq!(violation(s.b), Violation::JumpTargetKind(bad));
}

/// The "until" marker must lie between the target and the jump (owner's decision of 2026-11-03).
#[test]
fn until_lies_between_target_and_jump() {
    // A Fine after the D.C.: never reached during the jump.
    let mut s = score(6);
    let fine = s.b.add_marker(s.bars[4], MarkerKind::Fine, None).unwrap();
    let late =
        s.b.add_jump(s.bars[2], JumpSpec::da_capo().al_fine(fine), None)
            .unwrap();
    assert_eq!(violation(s.b), Violation::JumpUntilOutOfRange(late));

    // A Fine before the segno.
    let mut s = score(6);
    let fine = s.b.add_marker(s.bars[0], MarkerKind::Fine, None).unwrap();
    let segno = s.b.add_marker(s.bars[2], MarkerKind::Segno, None).unwrap();
    let early =
        s.b.add_jump(s.bars[4], JumpSpec::dal_segno(segno).al_fine(fine), None)
            .unwrap();
    assert_eq!(violation(s.b), Violation::JumpUntilOutOfRange(early));

    // A Fine on the jump's own bar is fine (the jump is at its end, so is the Fine).
    let mut s = score(6);
    let fine = s.b.add_marker(s.bars[4], MarkerKind::Fine, None).unwrap();
    s.b.add_jump(s.bars[4], JumpSpec::da_capo().al_fine(fine), None)
        .unwrap();
    assert!(s.b.build().is_ok());
}

/// `OnPass(n)` only inside a repeated section, with n within its plays.
#[test]
fn on_pass_is_checked() {
    let on = |n| JumpSpec {
        fires: JumpFires::OnPass(n),
        ..JumpSpec::da_capo()
    };
    // Outside any repeated section.
    let mut s = score(6);
    let outside = s.b.add_jump(s.bars[3], on(1), None).unwrap();
    assert_eq!(violation(s.b), Violation::JumpPassOutOfRange(outside));
    // Beyond the plays.
    let mut s = score(6);
    s.b.add_repeat_start(s.bars[1]).unwrap();
    s.b.add_repeat_end(s.bars[3], 2).unwrap();
    let beyond = s.b.add_jump(s.bars[2], on(3), None).unwrap();
    assert_eq!(violation(s.b), Violation::JumpPassOutOfRange(beyond));
}

/// The builder's checks for markers and jumps, all or nothing.
#[test]
fn marker_and_jump_builder_checks() {
    let mut s = score(4);
    let (_, foreign_bars) = {
        let other = score(9);
        (other.b, other.bars)
    };
    let before = s.b.document().clone();
    assert_eq!(
        s.b.add_marker(foreign_bars[8], MarkerKind::Segno, None),
        Err(DocumentError::UnknownBar(foreign_bars[8]))
    );
    assert_eq!(
        s.b.add_marker(s.bars[1], MarkerKind::Fine, Some("")),
        Err(DocumentError::EmptyText)
    );
    // A marker of another document (whose IDs are shifted by its extra bars).
    let mut other = score(9);
    let stranger = other
        .b
        .add_marker(other.bars[8], MarkerKind::Segno, None)
        .unwrap();
    assert_eq!(
        s.b.add_jump(s.bars[2], JumpSpec::dal_segno(stranger), None),
        Err(DocumentError::UnknownMarker(stranger))
    );
    assert_eq!(s.b.document(), &before);
    // One marker per side of a bar, one jump per bar.
    s.b.add_marker(s.bars[1], MarkerKind::Segno, None).unwrap();
    assert_eq!(
        s.b.add_marker(s.bars[1], MarkerKind::Coda, None),
        Err(DocumentError::DuplicateMark)
    );
    s.b.add_marker(s.bars[1], MarkerKind::Fine, None).unwrap();
    assert_eq!(
        s.b.add_marker(s.bars[1], MarkerKind::ToCoda, None),
        Err(DocumentError::DuplicateMark)
    );
    s.b.add_jump(s.bars[3], JumpSpec::da_capo(), None).unwrap();
    assert_eq!(
        s.b.add_jump(s.bars[3], JumpSpec::da_capo(), None),
        Err(DocumentError::DuplicateMark)
    );
    // Start before end at one bar.
    let document = s.b.build().unwrap();
    let kinds: Vec<_> = document.form().markers().iter().map(Marker::kind).collect();
    assert_eq!(kinds, [MarkerKind::Segno, MarkerKind::Fine]);
}

/// ADR-0025 §10, "Bar repeat over two bars on the piano's lower staff": the bars hold no events
/// there (the right hand plays on), the source exists; an overlap is rejected.
#[test]
fn bar_repeats() {
    let mut s = score(8);
    let (_, [upper, lower], [right, left]) = s.piano;
    let whole_note = NoteValue::plain(BaseValue::Whole);
    let c4 = Pitch::new(Step::C, Alteration::NATURAL, 4);
    for k in 0..6 {
        s.b.add_chord(
            Placement::new(right, s.bars[k], Duration::ZERO),
            whole_note,
            &[c4],
        )
        .unwrap();
        if k < 2 {
            s.b.add_chord(
                Placement::new(left, s.bars[k], Duration::ZERO),
                whole_note,
                &[c4],
            )
            .unwrap();
        }
    }
    let two =
        s.b.add_bar_repeat(lower, s.bars[2], 4, BarRepeatSpan::Two)
            .unwrap();
    let mut overlapping = s.b.clone();
    assert!(s.b.clone().build().is_ok());
    let document = s.b.build().unwrap();
    let piano = document.instrument(s.piano.0).unwrap();
    assert_eq!(piano.bar_repeats()[0].id(), two);
    assert_eq!(piano.bar_repeats()[0].count(), 4);
    assert_eq!(piano.bar_repeats()[0].span().bars(), 2);

    // A second bar repeat on the same staff overlapping the first.
    let again = overlapping
        .add_bar_repeat(lower, s.bars[4], 1, BarRepeatSpan::One)
        .unwrap();
    assert_eq!(violation(overlapping), Violation::BarRepeatOverlap(again));

    // On the upper staff, where the right hand has notes.
    let mut s2 = score(4);
    let (_, [upper2, _], [right2, _]) = s2.piano;
    s2.b.add_chord(
        Placement::new(right2, s2.bars[1], Duration::ZERO),
        whole_note,
        &[c4],
    )
    .unwrap();
    let full =
        s2.b.add_bar_repeat(upper2, s2.bars[1], 1, BarRepeatSpan::One)
            .unwrap();
    assert_eq!(violation(s2.b), Violation::BarRepeatNotEmpty(full));
    let _ = upper;
}

/// A bar repeat needs its source bars, and no form marking between the source and its end.
#[test]
fn bar_repeat_rules() {
    // A two-bar repeat starting at bar 1: only one bar before it.
    let mut s = score(4);
    let lower = s.piano.1[1];
    let early =
        s.b.add_bar_repeat(lower, s.bars[1], 2, BarRepeatSpan::Two)
            .unwrap();
    assert_eq!(violation(s.b), Violation::BarRepeatNoSource(early));

    // A repeat barline between the source and the repeated bar (‖: at bar 2, % at bar 2 repeating
    // bar 1): the start of the source's edge is fine, an inner barline is not.
    let mut s = score(4);
    s.b.add_repeat_start(s.bars[2]).unwrap();
    s.b.add_repeat_end(s.bars[3], 2).unwrap();
    let across =
        s.b.add_bar_repeat(lower, s.bars[2], 1, BarRepeatSpan::One)
            .unwrap();
    assert_eq!(violation(s.b), Violation::BarRepeatAcrossForm(across));

    // A repeat barline at the outer edges only: fine.
    let mut s = score(5);
    s.b.add_repeat_start(s.bars[1]).unwrap();
    s.b.add_repeat_end(s.bars[2], 2).unwrap();
    s.b.add_bar_repeat(lower, s.bars[2], 1, BarRepeatSpan::One)
        .unwrap();
    assert!(s.b.build().is_ok());

    // Builder checks: a count that is not a multiple of the span, a staff of nobody.
    let mut s = score(4);
    assert_eq!(
        s.b.add_bar_repeat(lower, s.bars[2], 3, BarRepeatSpan::Two),
        Err(DocumentError::BarRepeatCount)
    );
    assert_eq!(
        s.b.add_bar_repeat(lower, s.bars[2], 0, BarRepeatSpan::One),
        Err(DocumentError::BarRepeatCount)
    );
    // A staff of another document with many more bars, so that its ID is beyond every ID here.
    let other = score(40);
    let stranger = other.piano.1[1];
    assert_eq!(
        s.b.add_bar_repeat(stranger, s.bars[2], 1, BarRepeatSpan::One),
        Err(DocumentError::UnknownStaff(stranger))
    );
}

/// ADR-0025 §10, "Tacet 1st time on the trumpet": pass set {2}, inside one repeated section; the
/// rule-11 violations of pass rules.
#[test]
fn pass_rules() {
    let mut s = score(8);
    let (trumpet, voice) = s.trumpet;
    s.b.add_repeat_start(s.bars[1]).unwrap();
    s.b.add_repeat_end(s.bars[4], 2).unwrap();
    let whole = DynamicScope::Instrument;
    let tacet =
        s.b.add_pass_rule(
            trumpet,
            ScorePoint::start_of(s.bars[1]),
            ScorePoint::start_of(s.bars[5]),
            whole,
            passes(2, 2),
            Some("1st time tacet"),
        )
        .unwrap();
    // A voice rule over the same span is another scope.
    s.b.add_pass_rule(
        trumpet,
        ScorePoint::start_of(s.bars[2]),
        ScorePoint::start_of(s.bars[3]),
        DynamicScope::Voice(voice),
        passes(1, 1),
        None,
    )
    .unwrap();
    let base = s.b.clone();
    let document = s.b.build().unwrap();
    let rule = &document.instrument(trumpet).unwrap().pass_rules()[0];
    assert_eq!(rule.id(), tacet);
    assert_eq!(rule.plays(), passes(2, 2));
    assert_eq!(rule.text(), Some("1st time tacet"));

    // Overlap in one scope.
    let mut b = base.clone();
    let over = b
        .add_pass_rule(
            trumpet,
            ScorePoint::start_of(s.bars[3]),
            ScorePoint::start_of(s.bars[4]),
            whole,
            passes(1, 1),
            None,
        )
        .unwrap();
    assert_eq!(violation(b), Violation::PassRuleOverlap(over));

    // Outside a repeated section, past its end, beyond its plays.
    for (from, to, plays, expected) in [
        (6, 7, passes(1, 1), "outside"),
        (3, 6, passes(1, 1), "outside"),
        (2, 3, passes(3, 3), "passes"),
    ] {
        let mut s = score(8);
        s.b.add_repeat_start(s.bars[1]).unwrap();
        s.b.add_repeat_end(s.bars[4], 2).unwrap();
        let id =
            s.b.add_pass_rule(
                trumpet,
                ScorePoint::start_of(s.bars[from]),
                ScorePoint::start_of(s.bars[to]),
                whole,
                plays,
                None,
            )
            .unwrap();
        let want = if expected == "outside" {
            Violation::PassRuleOutsideRepeat(id)
        } else {
            Violation::PassRulePasses(id)
        };
        assert_eq!(violation(s.b), want);
    }

    // Builder checks.
    let mut s = score(4);
    let trumpet = s.trumpet.0;
    let start = ScorePoint::start_of(s.bars[2]);
    assert_eq!(
        s.b.add_pass_rule(trumpet, start, start, whole, passes(1, 1), None),
        Err(DocumentError::PassRuleNotForward)
    );
    let end = ScorePoint::start_of(s.bars[3]);
    assert_eq!(
        s.b.add_pass_rule(trumpet, start, end, whole, PassSet::EMPTY, None),
        Err(DocumentError::EmptyPassSet)
    );
    let piano_voice = s.piano.2[0];
    assert_eq!(
        s.b.add_pass_rule(
            trumpet,
            start,
            end,
            DynamicScope::Voice(piano_voice),
            passes(1, 1),
            None
        ),
        Err(DocumentError::VoiceOfAnotherInstrument(piano_voice))
    );
}

/// A section with "1. :‖ 2. :‖ 3." reaches the end of its second volta: a jump there may fire on
/// pass 2, and a pass rule there lies inside the section.
#[test]
fn a_section_with_three_voltas_reaches_its_second_volta() {
    let mut s = score(8);
    let (trumpet, _) = s.trumpet;
    let bars = s.bars.clone();
    s.b.add_repeat_start(bars[0]).unwrap();
    s.b.add_volta(bars[2], bars[2], passes(1, 1), true, None)
        .unwrap();
    s.b.add_repeat_end(bars[2], 3).unwrap();
    s.b.add_volta(bars[3], bars[3], passes(2, 2), true, None)
        .unwrap();
    s.b.add_repeat_end(bars[3], 3).unwrap();
    s.b.add_volta(bars[4], bars[4], passes(3, 3), false, None)
        .unwrap();
    let on_second = JumpSpec {
        fires: JumpFires::OnPass(2),
        ..JumpSpec::da_capo()
    };
    s.b.add_jump(bars[3], on_second, None).unwrap();
    s.b.add_pass_rule(
        trumpet,
        ScorePoint::start_of(bars[3]),
        ScorePoint::start_of(bars[4]),
        DynamicScope::Instrument,
        passes(2, 3),
        None,
    )
    .unwrap();
    assert!(s.b.build().is_ok());
}
