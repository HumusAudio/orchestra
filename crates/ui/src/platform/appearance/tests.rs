// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of the appearance machinery (ADR-0038 S6).
//!
//! The rules 1–4 of the module documentation are tested with **scripted fake sources**: the
//! source thread executes commands sent by the test over a channel and acknowledges each one, so
//! the tests decide the order of events exactly and never rely on how long something takes. The
//! only waits are the deliberately tiny timeouts of the "late answer" cases, and the large ones
//! that must *not* be reached.
//!
//! On Linux, the portal's value decoding is tested on hand-built D-Bus values, and opening the
//! real source is checked not to hang or panic on a machine that may have no session bus at all
//! (CI runners have none).

use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::{
    AppearanceCallback, AppearanceWatch, FIRST_READ_TIMEOUT, SystemAppearanceError,
    SystemAppearanceSource, system_appearance_source,
};
use crate::Appearance;

/// A timeout that the tests must never reach: if a test takes this long, the rule it checks is
/// broken (the caller waited when it should not have).
const NEVER: Duration = Duration::from_secs(60);

/// A timeout that the "late answer" tests reach on purpose.
const TINY: Duration = Duration::from_millis(10);

/// One step for the scripted source thread.
enum Command {
    /// Report this answer.
    Answer(Option<Appearance>),
    /// Report this error.
    Fail(SystemAppearanceError),
}

/// The test's side of a scripted source: send commands, wait for each one to be done.
struct Script {
    /// Commands for the source thread.
    commands: Sender<Command>,
    /// One `()` per command executed.
    done: Receiver<()>,
}

impl Script {
    /// Sends `command` and waits until the source thread has executed it.
    fn run(&self, command: Command) {
        self.commands.send(command).unwrap();
        self.done.recv_timeout(NEVER).unwrap();
    }

    /// Sends `command` without waiting: for the first answer, which `observe` itself waits for.
    fn send(&self, command: Command) {
        self.commands.send(command).unwrap();
    }
}

/// The record of a callback that was never called.
const NO_CALLS: Vec<Option<Appearance>> = Vec::new();

/// Collects the values the callback receives.
type Calls = Arc<Mutex<Vec<Option<Appearance>>>>;

/// A callback that records its calls in `calls`.
fn recording(calls: &Calls) -> AppearanceCallback {
    let calls = Arc::clone(calls);
    Box::new(move |value| calls.lock().unwrap().push(value))
}

/// Starts a watch whose source thread executes the script's commands, with `timeout` for the first
/// answer. `first` is sent before `observe` starts waiting (or nothing, for the late cases).
fn scripted(timeout: Duration, first: Option<Command>) -> (AppearanceWatch, Script, Calls) {
    let (commands, inbox) = channel::<Command>();
    let (ack, done) = channel::<()>();
    let script = Script { commands, done };
    if let Some(first) = first {
        script.send(first);
    }
    let calls: Calls = Arc::default();
    let watch = AppearanceWatch::spawn(
        recording(&calls),
        "test-appearance",
        timeout,
        move |reporter| {
            // Execute commands until the test drops its sender.
            for command in inbox {
                match command {
                    Command::Answer(value) => reporter.answer(value),
                    Command::Fail(error) => reporter.fail(error),
                }
                // The test may have stopped listening (first answer): ignore a closed channel.
                let _ = ack.send(());
            }
        },
    );
    (watch, script, calls)
}

/// The values recorded so far.
fn calls_of(calls: &Calls) -> Vec<Option<Appearance>> {
    calls.lock().unwrap().clone()
}

#[test]
fn a_first_answer_in_time_is_the_initial_value_not_a_callback() {
    let started = Instant::now();
    let (watch, script, calls) = scripted(NEVER, Some(Command::Answer(Some(Appearance::Light))));
    // `observe` returned as soon as the answer came, long before the timeout.
    assert!(started.elapsed() < NEVER / 2);
    // The answer is consumed by `observe`; drain its acknowledgement.
    script.done.recv_timeout(NEVER).unwrap();
    assert_eq!(watch.current(), Some(Appearance::Light));
    assert_eq!(watch.last_error(), None);
    assert_eq!(calls_of(&calls), NO_CALLS);
}

#[test]
fn repeats_are_dropped_and_only_transitions_reach_the_callback() {
    let (watch, script, calls) = scripted(NEVER, Some(Command::Answer(Some(Appearance::Dark))));
    script.done.recv_timeout(NEVER).unwrap();
    // The portal sends each change several times (seen three times on the owner's desktop).
    for value in [
        Some(Appearance::Dark),
        Some(Appearance::Light),
        Some(Appearance::Light),
        Some(Appearance::Light),
        Some(Appearance::Dark),
        Some(Appearance::Dark),
    ] {
        script.run(Command::Answer(value));
    }
    assert_eq!(
        calls_of(&calls),
        vec![Some(Appearance::Light), Some(Appearance::Dark)]
    );
    assert_eq!(watch.current(), Some(Appearance::Dark));
}

#[test]
fn a_late_first_answer_starts_with_none_and_arrives_through_the_callback() {
    let started = Instant::now();
    let (watch, script, calls) = scripted(TINY, None);
    // `observe` gave up after the tiny timeout, with "no answer" and the reason.
    assert!(started.elapsed() >= TINY);
    assert_eq!(watch.current(), None);
    assert_eq!(watch.last_error(), Some(SystemAppearanceError::Timeout));
    // The answer comes later and is news compared with the assumed `None`.
    script.run(Command::Answer(Some(Appearance::Light)));
    assert_eq!(calls_of(&calls), vec![Some(Appearance::Light)]);
    assert_eq!(watch.current(), Some(Appearance::Light));
}

#[test]
fn a_late_none_is_not_a_change() {
    let (watch, script, calls) = scripted(TINY, None);
    assert_eq!(watch.current(), None);
    script.run(Command::Answer(None));
    assert_eq!(calls_of(&calls), NO_CALLS);
}

#[test]
fn a_failure_before_the_first_answer_releases_the_caller_at_once() {
    let started = Instant::now();
    let error = SystemAppearanceError::Unreachable {
        detail: "no bus".to_owned(),
    };
    let (watch, script, calls) = scripted(NEVER, Some(Command::Fail(error.clone())));
    // No waiting for the 60 s timeout: the failure is the answer.
    assert!(started.elapsed() < NEVER / 2);
    script.done.recv_timeout(NEVER).unwrap();
    assert_eq!(watch.current(), None);
    assert_eq!(watch.last_error(), Some(error));
    assert_eq!(calls_of(&calls), NO_CALLS);
}

#[test]
fn a_failure_while_observing_turns_to_no_answer_and_is_recorded() {
    let (watch, script, calls) = scripted(NEVER, Some(Command::Answer(Some(Appearance::Light))));
    script.done.recv_timeout(NEVER).unwrap();
    let error = SystemAppearanceError::Observe {
        detail: "bus gone".to_owned(),
    };
    script.run(Command::Fail(error.clone()));
    assert_eq!(calls_of(&calls), vec![None]);
    assert_eq!(watch.current(), None);
    assert_eq!(watch.last_error(), Some(error));
    // A second failure is not a second change.
    script.run(Command::Fail(SystemAppearanceError::UnknownValue {
        value: 7,
    }));
    assert_eq!(calls_of(&calls), vec![None]);
    // A later success keeps the last error (it says what went wrong last).
    script.run(Command::Answer(Some(Appearance::Dark)));
    assert_eq!(calls_of(&calls), vec![None, Some(Appearance::Dark)]);
    assert_eq!(
        watch.last_error(),
        Some(SystemAppearanceError::UnknownValue { value: 7 })
    );
}

#[test]
fn after_the_watch_is_dropped_the_callback_is_never_called() {
    let (watch, script, calls) = scripted(NEVER, Some(Command::Answer(Some(Appearance::Light))));
    script.done.recv_timeout(NEVER).unwrap();
    script.run(Command::Answer(Some(Appearance::Dark)));
    drop(watch);
    // The source thread is still alive and reporting; nothing reaches the callback any more.
    script.run(Command::Answer(Some(Appearance::Light)));
    script.run(Command::Fail(SystemAppearanceError::Timeout));
    assert_eq!(calls_of(&calls), vec![Some(Appearance::Dark)]);
}

#[test]
fn a_panicking_callback_does_not_break_the_watch() {
    // A callback that panics poisons the lock; the watch must keep answering (`Shared::lock`).
    let (commands, inbox) = channel::<Option<Appearance>>();
    // Both answers are queued before `spawn`, which waits for the first one: Light is the
    // initial value (no callback), Dark is a transition, so it calls the callback, which panics
    // while holding the lock and ends the source thread.
    commands.send(Some(Appearance::Light)).unwrap();
    commands.send(Some(Appearance::Dark)).unwrap();
    let watch = AppearanceWatch::spawn(
        Box::new(|_| panic!("a buggy callback")),
        "test-appearance",
        NEVER,
        move |reporter| {
            for value in inbox {
                reporter.answer(value);
            }
        },
    );
    // Wait until the thread has gone: sending fails once its receiver is dropped.
    let started = Instant::now();
    while commands.send(None).is_ok() {
        assert!(started.elapsed() < NEVER, "the source thread never ended");
        std::thread::yield_now();
    }
    assert_eq!(watch.current(), Some(Appearance::Dark));
    drop(watch);
}

#[test]
fn the_no_answer_watch_says_nothing_and_records_nothing() {
    let watch = AppearanceWatch::no_answer();
    assert_eq!(watch.current(), None);
    assert_eq!(watch.last_error(), None);
    // Debug output names the state, for diagnostics.
    assert!(format!("{watch:?}").contains("current: None"));
}

#[test]
fn error_messages_are_one_line_and_name_the_detail() {
    let errors = [
        SystemAppearanceError::Unreachable {
            detail: "d1".to_owned(),
        },
        SystemAppearanceError::Read {
            detail: "d2".to_owned(),
        },
        SystemAppearanceError::UnexpectedType {
            signature: "s".to_owned(),
        },
        SystemAppearanceError::UnknownValue { value: 9 },
        SystemAppearanceError::Observe {
            detail: "d3".to_owned(),
        },
        SystemAppearanceError::Timeout,
        SystemAppearanceError::Thread {
            detail: "d4".to_owned(),
        },
    ];
    for error in &errors {
        let text = error.to_string();
        assert!(!text.is_empty() && !text.contains('\n'), "{text:?}");
    }
    assert!(errors[1].to_string().contains("d2"));
    assert!(errors[2].to_string().contains("`s`"));
    assert!(errors[3].to_string().contains('9'));
    assert!(errors[5].to_string().contains("250 ms"));
}

#[test]
fn the_platform_source_never_hangs_or_panics() {
    // On a CI runner there is no session bus (Linux) or no implementation yet (Windows, macOS):
    // the answer is `None`. On a developer's desktop the portal may answer: any value is fine.
    // What must hold everywhere is the bounded wait of ADR-0038 S5.
    let started = Instant::now();
    let watch = system_appearance_source().observe(Box::new(|_| {}));
    let elapsed = started.elapsed();
    // The bound plus a generous margin for a loaded runner creating a thread.
    assert!(
        elapsed < FIRST_READ_TIMEOUT + Duration::from_secs(5),
        "{elapsed:?}"
    );
    if watch.current().is_none() {
        // No answer: on Linux there must be a reason; elsewhere none is recorded (not a failure).
        assert_eq!(watch.last_error().is_some(), cfg!(target_os = "linux"));
    }
}

/// The portal's value decoding (ADR-0038 S4), on hand-built D-Bus values: no bus needed.
#[cfg(target_os = "linux")]
mod portal_values {
    use zbus::zvariant::Value;

    use super::super::SystemAppearanceError;
    use super::super::portal::{appearance_from_portal, decode};
    use crate::Appearance;

    #[test]
    fn the_three_documented_numbers() {
        assert_eq!(appearance_from_portal(1), Ok(Some(Appearance::Dark)));
        assert_eq!(appearance_from_portal(2), Ok(Some(Appearance::Light)));
        // "No preference" is light (GNOME's default style), owner's choice of 2026-11-10.
        assert_eq!(appearance_from_portal(0), Ok(Some(Appearance::Light)));
    }

    #[test]
    fn other_numbers_are_no_answer_with_a_reason() {
        for value in [3, 4, 255, u32::MAX] {
            assert_eq!(
                appearance_from_portal(value),
                Err(SystemAppearanceError::UnknownValue { value })
            );
        }
    }

    #[test]
    fn read_one_gives_a_plain_number() {
        assert_eq!(decode(&Value::U32(1)), Ok(Some(Appearance::Dark)));
    }

    #[test]
    fn the_old_read_wraps_the_number_in_one_more_variant() {
        let wrapped = Value::Value(Box::new(Value::U32(2)));
        assert_eq!(decode(&wrapped), Ok(Some(Appearance::Light)));
        // Two extra layers are still accepted (the margin of `MAX_VARIANT_DEPTH`).
        let twice = Value::Value(Box::new(wrapped));
        assert_eq!(decode(&twice), Ok(Some(Appearance::Light)));
    }

    #[test]
    fn too_many_layers_or_another_type_is_unexpected() {
        let thrice = Value::Value(Box::new(Value::Value(Box::new(Value::Value(Box::new(
            Value::U32(1),
        ))))));
        assert_eq!(
            decode(&thrice),
            Err(SystemAppearanceError::UnexpectedType {
                signature: "v".to_owned()
            })
        );
        assert_eq!(
            decode(&Value::from("dark")),
            Err(SystemAppearanceError::UnexpectedType {
                signature: "s".to_owned()
            })
        );
        assert_eq!(
            decode(&Value::I32(1)),
            Err(SystemAppearanceError::UnexpectedType {
                signature: "i".to_owned()
            })
        );
    }
}
