// SPDX-License-Identifier: GPL-3.0-or-later

//! Projects read and written on a worker thread (ADR-0054 AR1, SCALETTA F2.14a), headless:
//! - the flow's jobs: a save writes the **snapshot** taken when it was asked, and what changed
//!   meanwhile still counts as modified; commands are ignored while a job is out; a close asked
//!   meanwhile **waits** for the answer and then closes, asks, or is dropped after a failure; an
//!   answer that is not the job's changes nothing; a job **stopped** by a mistake of the program
//!   is reported with the operation's title and "a mistake in Orchestra";
//! - the worker: answers in the order of the jobs, the waker called once per answer, the path
//!   taken without a thread, and dropping it finishing what it was sent;
//! - the demonstration: the interface **busy** (blocked, the wait pointer) from the job to its
//!   answer, and the window's close waiting for it.
//!
//! The files are written in a folder of the system's temporary directory made for each test and
//! removed at the end, as in `tests/project.rs`.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test helpers: a failed unwrap or index fails the test"
)]
#![allow(clippy::panic, reason = "test helpers: a panic fails the test")]

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use orchestra_app::{
    Demo, Job, JobAnswer, OpenProject, ProjectEffect, ProjectError, ProjectFlow, ProjectTexts,
    ProjectWorker, SceneContext, Step, project_error_dialog,
};
use orchestra_core_model::Command;
use orchestra_core_time::BarIndex;
use orchestra_i18n::{Locale, Localizer};
use orchestra_io::{LoadError, SaveError};
use orchestra_ui::{
    ButtonState, DisplayList, FileChoice, InputEvent, Key, Modifiers, PhysicalKey, PointerShape,
    Rect, WindowApp, WindowRequest,
};

/// A fresh, empty folder for the test `name`, removed by [`Scratch`]'s `Drop`.
struct Scratch(PathBuf);

impl Scratch {
    /// The folder of the test `name`.
    fn new(name: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("orchestra-worker-{name}-{}", std::process::id()));
        // Left over from an earlier run that crashed: start empty.
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    /// The path of `file` in the folder.
    fn join(&self, file: &str) -> PathBuf {
        self.0.join(file)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Two bars inserted at the start of `flow`'s document.
fn modify(flow: &mut ProjectFlow) {
    flow.project_mut()
        .editor_mut()
        .execute(&Command::InsertBars {
            at: BarIndex::new(0),
            count: 2,
        })
        .unwrap();
}

/// The job of `effect`, which must be one.
fn job(effect: Option<ProjectEffect>) -> Job {
    match effect {
        Some(ProjectEffect::Run(job)) => job,
        other => panic!("expected a job, got {other:?}"),
    }
}

/// A flow whose project was saved to `name.orchestra` in `scratch` (3 bars: the new document's
/// one and two inserted), not modified.
fn saved_flow(scratch: &Scratch, name: &str) -> ProjectFlow {
    let mut flow = ProjectFlow::new(OpenProject::new().unwrap());
    modify(&mut flow);
    let _ = flow.save_as();
    let write = job(flow.chosen(Some(scratch.join(name))));
    assert_eq!(flow.finished(write.run()), None);
    assert!(!flow.project().is_modified());
    flow
}

/// The bars of the project file at `path`.
fn bars_on_disk(path: &std::path::Path) -> usize {
    orchestra_io::load(path).unwrap().bars().len()
}

#[test]
fn a_save_writes_its_snapshot_and_later_changes_stay_modified() {
    let scratch = Scratch::new("snapshot");
    let mut flow = saved_flow(&scratch, "Song");
    let path = scratch.join("Song.orchestra");
    modify(&mut flow);
    let write = job(flow.save());
    assert_eq!(flow.step(), &Step::Writing(None));
    assert!(flow.is_busy());
    match &write {
        Job::Write {
            path: target,
            revision,
            document,
        } => {
            assert_eq!(target, &path);
            assert_eq!(*revision, flow.project().editor().document().revision());
            assert_eq!(document.bars().len(), 5);
        }
        Job::Read { .. } => panic!("a save reads nothing"),
    }
    // An edit while the job is out (the automatic save of F2.14b will allow it): the snapshot is
    // what gets written, and the project stays modified.
    modify(&mut flow);
    assert_eq!(flow.finished(write.run()), None);
    assert_eq!(flow.step(), &Step::Idle);
    assert_eq!(bars_on_disk(&path), 5);
    assert_eq!(flow.project().editor().document().bars().len(), 7);
    assert!(flow.project().is_modified());
    // Saved again with nothing changing meanwhile: not modified.
    let write = job(flow.save());
    assert_eq!(flow.finished(write.run()), None);
    assert!(!flow.project().is_modified());
    assert_eq!(bars_on_disk(&path), 7);
}

#[test]
fn commands_are_ignored_while_a_job_is_out() {
    let scratch = Scratch::new("ignored");
    let mut flow = saved_flow(&scratch, "Song");
    modify(&mut flow);
    let write = job(flow.save());
    assert_eq!(flow.save(), None);
    assert_eq!(flow.save_as(), None);
    assert_eq!(flow.new_project(), None);
    assert_eq!(flow.open(), None);
    assert_eq!(flow.step(), &Step::Writing(None));
    assert_eq!(flow.finished(write.run()), None);
    // Reading: the same.
    let _ = flow.open();
    let read = job(flow.chosen(Some(scratch.join("Song.orchestra"))));
    assert_eq!(flow.step(), &Step::Reading);
    assert_eq!(flow.save(), None);
    assert_eq!(flow.new_project(), None);
    assert_eq!(flow.finished(read.run()), None);
    assert_eq!(flow.step(), &Step::Idle);
}

#[test]
fn a_close_during_a_save_waits_for_the_answer() {
    let scratch = Scratch::new("close-save");
    // Nothing changed during the save: the window closes after the answer.
    let mut flow = saved_flow(&scratch, "Song");
    modify(&mut flow);
    let write = job(flow.save());
    assert_eq!(flow.close(), None, "kept, not answered yet");
    assert!(flow.close_waiting());
    assert_eq!(flow.close(), None, "a second request is the same request");
    assert_eq!(flow.finished(write.run()), Some(ProjectEffect::Close));
    assert!(!flow.close_waiting());
    // Changed during the save: the close asks about the changes after the answer.
    let mut flow = saved_flow(&scratch, "Other");
    modify(&mut flow);
    let write = job(flow.save());
    let _ = flow.close();
    modify(&mut flow);
    assert_eq!(
        flow.finished(write.run()),
        Some(ProjectEffect::AskToSave {
            name: Some("Other".into())
        })
    );
    // The save fails: its error is said, and the close is dropped (the user decides again).
    let mut flow = ProjectFlow::new(OpenProject::new().unwrap());
    modify(&mut flow);
    let _ = flow.save_as();
    let write = job(flow.chosen(Some(scratch.join("no folder").join("Song"))));
    let _ = flow.close();
    assert!(matches!(
        flow.finished(write.run()),
        Some(ProjectEffect::Failed(ProjectError::Save {
            error: SaveError::Io(_),
            ..
        }))
    ));
    assert!(!flow.close_waiting());
    assert_eq!(flow.step(), &Step::Idle);
}

#[test]
fn a_close_during_an_opening_waits_for_the_answer() {
    let scratch = Scratch::new("close-open");
    let _ = saved_flow(&scratch, "Song");
    let mut flow = ProjectFlow::new(OpenProject::new().unwrap());
    let _ = flow.open();
    let read = job(flow.chosen(Some(scratch.join("Song.orchestra"))));
    assert_eq!(flow.close(), None);
    // The project read replaces the new one, unmodified: the window closes.
    assert_eq!(flow.finished(read.run()), Some(ProjectEffect::Close));
    assert_eq!(flow.project().name().as_deref(), Some("Song"));
    // A failed opening drops the close, as a failed save does.
    let _ = flow.open();
    let read = job(flow.chosen(Some(scratch.join("Missing.orchestra"))));
    let _ = flow.close();
    assert!(matches!(
        flow.finished(read.run()),
        Some(ProjectEffect::Failed(ProjectError::Open {
            error: LoadError::Io(_),
            ..
        }))
    ));
    assert!(!flow.close_waiting());
}

#[test]
fn an_answer_that_is_not_the_jobs_changes_nothing() {
    let scratch = Scratch::new("stray");
    let mut flow = saved_flow(&scratch, "Song");
    let path = scratch.join("Song.orchestra");
    let read_answer = Job::Read { path: path.clone() }.run();
    // Idle: nothing is out.
    assert_eq!(flow.finished(read_answer.clone()), None);
    assert_eq!(flow.step(), &Step::Idle);
    // Writing: a read's answer, or a stopped read, is not the save's.
    modify(&mut flow);
    let write = job(flow.save());
    assert_eq!(flow.finished(read_answer), None);
    assert_eq!(
        flow.finished(JobAnswer::Stopped {
            path: path.clone(),
            writing: false
        }),
        None
    );
    assert_eq!(flow.step(), &Step::Writing(None));
    assert!(flow.project().is_modified());
    assert_eq!(flow.finished(write.run()), None);
    assert!(!flow.project().is_modified());
}

/// The open project's texts in en-US.
fn texts() -> ProjectTexts {
    ProjectTexts::new(&Localizer::new(Locale::EnUs).unwrap()).unwrap()
}

#[test]
fn a_stopped_job_is_reported_as_a_mistake_of_the_program() {
    let scratch = Scratch::new("stopped");
    let mut flow = saved_flow(&scratch, "Song");
    let path = scratch.join("Song.orchestra");
    // A save stopped: the operation's title, the mistake's message, the file left as it was.
    modify(&mut flow);
    let _ = job(flow.save());
    let Some(ProjectEffect::Failed(error)) = flow.finished(JobAnswer::Stopped {
        path: path.clone(),
        writing: true,
    }) else {
        panic!("a stopped save is a failure");
    };
    assert_eq!(
        error,
        ProjectError::Stopped {
            path: path.clone(),
            writing: true
        }
    );
    assert!(flow.project().is_modified(), "nothing was saved");
    let spec = project_error_dialog(&texts(), &error, ());
    assert_eq!(spec.title, "“Song.orchestra” can't be saved");
    assert_eq!(
        spec.message,
        "This is a mistake in Orchestra: please report it."
    );
    assert_eq!(
        spec.details,
        [
            path.display().to_string(),
            "The file that was already there, if any, has been left as it was.".to_owned()
        ]
    );
    // An opening stopped: the project stays.
    let _ = flow.open();
    let _ = flow.save_answer(orchestra_app::SaveAnswer::Discard);
    let _ = job(flow.chosen(Some(path.clone())));
    let Some(ProjectEffect::Failed(error)) = flow.finished(JobAnswer::Stopped {
        path: path.clone(),
        writing: false,
    }) else {
        panic!("a stopped opening is a failure");
    };
    let spec = project_error_dialog(&texts(), &error, ());
    assert_eq!(spec.title, "“Song.orchestra” can't be opened");
    assert_eq!(spec.details, [path.display().to_string()]);
    // "Don't save" before *Open* drops nothing by itself: the project the opening would have
    // replaced stays, with its changes.
    assert_eq!(flow.project().path(), Some(path.as_path()));
    assert!(flow.project().is_modified());
}

#[test]
fn the_worker_answers_in_order_and_wakes_once_per_answer() {
    let scratch = Scratch::new("order");
    let wakes = Arc::new(AtomicUsize::new(0));
    let mut worker = ProjectWorker::new();
    let counter = Arc::clone(&wakes);
    worker.set_waker(move || {
        counter.fetch_add(1, Ordering::SeqCst);
    });
    assert_eq!(worker.wait(), None, "nothing out");
    let path = scratch.join("Song.orchestra");
    let project = OpenProject::new().unwrap();
    // A save, then a read of the same file: the read must see what the save wrote.
    worker.submit(project.save_job(path.clone()));
    worker.submit(Job::Read { path: path.clone() });
    worker.submit(Job::Read {
        path: scratch.join("Missing.orchestra"),
    });
    assert_eq!(worker.pending(), 3);
    assert!(matches!(
        worker.wait(),
        Some(JobAnswer::Written { result: Ok(()), .. })
    ));
    match worker.wait() {
        Some(JobAnswer::Read {
            path: read,
            result: Ok(document),
        }) => {
            assert_eq!(read, path);
            assert!(document.same_content(project.editor().document()));
        }
        other => panic!("expected the document, got {other:?}"),
    }
    assert!(matches!(
        worker.wait(),
        Some(JobAnswer::Read { result: Err(_), .. })
    ));
    assert_eq!(worker.pending(), 0);
    assert_eq!(worker.try_take(), None);
    // The waker runs after each answer is sent: joining the thread makes the count final.
    drop(worker);
    assert_eq!(wakes.load(Ordering::SeqCst), 3);
}

#[test]
fn without_a_thread_the_answer_is_ready_at_once() {
    let scratch = Scratch::new("no-thread");
    let wakes = Arc::new(AtomicUsize::new(0));
    let mut worker = ProjectWorker::on_this_thread();
    let counter = Arc::clone(&wakes);
    worker.set_waker(move || {
        counter.fetch_add(1, Ordering::SeqCst);
    });
    let path = scratch.join("Song.orchestra");
    worker.submit(OpenProject::new().unwrap().save_job(path.clone()));
    assert!(path.exists(), "written before `submit` returned");
    assert_eq!(worker.pending(), 1);
    assert!(matches!(
        worker.try_take(),
        Some(JobAnswer::Written { result: Ok(()), .. })
    ));
    assert_eq!(worker.pending(), 0);
    // Nobody to wake: the caller takes the answer itself, right after sending.
    assert_eq!(wakes.load(Ordering::SeqCst), 0);
}

#[test]
fn dropping_the_worker_finishes_what_it_was_sent() {
    let scratch = Scratch::new("drop");
    let mut worker = ProjectWorker::new();
    let project = OpenProject::new().unwrap();
    let paths: Vec<PathBuf> = (0..4)
        .map(|n| scratch.join(&format!("Song {n}.orchestra")))
        .collect();
    for path in &paths {
        worker.submit(project.save_job(path.clone()));
    }
    drop(worker);
    for path in &paths {
        assert!(path.exists(), "{path:?}");
    }
}

/// The demonstration, laid out, in en-US.
fn demo() -> (Demo, DisplayList) {
    let context = SceneContext::new().unwrap();
    let mut demo = Demo::new(
        &context.faces,
        &orchestra_app::theme(),
        "Orchestra",
        &context.demo,
    );
    let mut list = DisplayList::new();
    demo.layout(Rect::from_origin_size(0.0, 0.0, 640.0, 370.0), &mut list);
    (demo, list)
}

/// *Primary* alone.
const PRIMARY: Modifiers = Modifiers {
    primary: true,
    shift: false,
    alt: false,
    secondary: false,
};

/// Presses and releases `key` with `modifiers`.
fn press(demo: &mut Demo, list: &mut DisplayList, key: &Key, modifiers: Modifiers) {
    for state in [ButtonState::Pressed, ButtonState::Released] {
        demo.input(
            &InputEvent::Key {
                key: key.clone(),
                physical: PhysicalKey::Other,
                state,
                repeat: false,
                modifiers,
            },
            list,
        );
    }
}

#[test]
fn the_interface_is_busy_while_a_project_is_opened_and_the_close_waits() {
    let scratch = Scratch::new("demo-open");
    let _ = saved_flow(&scratch, "Song");
    let (mut demo, mut list) = demo();
    // Ctrl+O with nothing to save: the chooser; its answer sends the job.
    press(&mut demo, &mut list, &Key::Character("o".into()), PRIMARY);
    assert!(demo.take_file_request().is_some());
    demo.file_chosen(
        FileChoice::Chosen(scratch.join("Song.orchestra")),
        &mut list,
    );
    assert!(demo.ui().is_busy());
    assert_eq!(WindowApp::pointer_shape(&demo), PointerShape::Wait);
    assert!(demo.project().unwrap().is_busy());
    // Ctrl+N is blocked by the busy layer; the window's close is kept for the answer.
    press(&mut demo, &mut list, &Key::Character("n".into()), PRIMARY);
    assert!(!demo.close_requested(&mut list));
    assert_eq!(demo.take_window_request(), None);
    // The answer: the project read, the interface back, and the close that waited goes on.
    assert!(demo.wait_for_project(&mut list));
    assert!(!demo.ui().is_busy());
    assert_eq!(WindowApp::pointer_shape(&demo), PointerShape::Default);
    assert_eq!(demo.window_title(), "Song — Orchestra");
    assert_eq!(demo.take_window_request(), Some(WindowRequest::Close));
    assert!(!demo.wait_for_project(&mut list), "nothing more out");
}

#[test]
fn without_a_thread_the_demonstration_saves_at_once() {
    let scratch = Scratch::new("demo-no-thread");
    let (mut demo, mut list) = demo();
    assert!(demo.set_project_worker(ProjectWorker::on_this_thread()));
    press(&mut demo, &mut list, &Key::Character("s".into()), PRIMARY);
    assert!(demo.take_file_request().is_some());
    demo.file_chosen(FileChoice::Chosen(scratch.join("Song")), &mut list);
    // Run, answered and taken in the same call: never busy afterwards.
    assert!(scratch.join("Song.orchestra").exists());
    assert!(!demo.ui().is_busy());
    assert_eq!(demo.window_title(), "Song — Orchestra");
    assert!(!demo.wait_for_project(&mut list));
}

#[test]
fn the_worker_cannot_be_replaced_while_a_job_is_out() {
    let scratch = Scratch::new("demo-replace-worker");
    let (mut demo, mut list) = demo();
    press(&mut demo, &mut list, &Key::Character("s".into()), PRIMARY);
    assert!(demo.take_file_request().is_some());
    demo.file_chosen(FileChoice::Chosen(scratch.join("Song")), &mut list);
    assert!(!demo.set_project_worker(ProjectWorker::on_this_thread()));
    assert!(demo.wait_for_project(&mut list));
    assert!(demo.set_project_worker(ProjectWorker::on_this_thread()));
}
