// SPDX-License-Identifier: GPL-3.0-or-later

//! The worker thread that reads and writes project files (ADR-0054 AR1, SCALETTA F2.14a), so that
//! the interface's thread never waits for a disk or for the encoding of a large document.
//!
//! # Shape
//! One thread, named `orchestra-projects`, started at the first [`Job`] and kept until the
//! [`ProjectWorker`] is dropped. Two channels join it to the interface's thread:
//! - **jobs** go in, and are run **one at a time, in the order sent**: two saves of the same file
//!   can never cross, and a read sent after a save sees the file the save left;
//! - **answers** come out, one per job, in the same order.
//!
//! After every answer the thread calls the **waker** the interface gave
//! ([`ProjectWorker::set_waker`]: the window's [`WindowWaker`](orchestra_ui::WindowWaker) in the
//! program), so the event loop starts a batch and the application takes the answer in its
//! `advance` ([`ProjectWorker::try_take`]). Nothing on the interface's thread ever blocks on the
//! worker, except [`ProjectWorker::wait`] (the tests, which have no event loop) and dropping the
//! worker (below).
//!
//! # Why not one thread per job, or `async`
//! One thread per job would let two saves of the same file run together and finish in either
//! order. An `async` runtime would be a dependency for what one thread and two standard channels
//! do (CLAUDE.md §5: every dependency is weighed).
//!
//! # When the system refuses a thread
//! [`std::thread::Builder::spawn`] can fail (a limit on threads, no memory). The job then runs on
//! the calling thread at once, as before F2.14a: slower, the interface pauses, but nothing is
//! lost. The next job tries to start the thread again.
//!
//! # A panic inside a job
//! The libraries never panic on purpose (CLAUDE.md §4), but a bug could. If a job panics, the
//! thread catches it (`catch_unwind`; the workspace builds with unwinding) and answers
//! [`JobAnswer::Stopped`] for that job, so the interface is never left blocked waiting for an
//! answer that cannot come (and the window, whose close waits for the answer, can still close).
//! The thread then goes on with the next job: a job shares nothing with the next one but the files.
//!
//! # Dropping
//! Dropping the worker closes the jobs' channel and **joins** the thread, which first finishes the
//! jobs already sent: a save in flight when the program ends is completed, never cut in the middle
//! (`safe_write` would keep the old file intact anyway, ADR-0029 S13, but the user asked for the
//! new one). The window closes only after the flow's job answered (the flow keeps the close
//! waiting), so in practice there is nothing left to finish.
//!
//! # Cost
//! A job and its answer are moved through the channels (the document is boxed, so a move is a
//! pointer); the waker is one event sent to the loop. The thread sleeps in `recv` between jobs.

use std::collections::VecDeque;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::{self, JoinHandle};

use super::{Job, JobAnswer};

/// The name of the thread, as debuggers and system monitors show it.
const THREAD_NAME: &str = "orchestra-projects";

/// What the worker calls after every answer: wakes the event loop. Shared with the thread behind
/// a mutex so that it can be given (or replaced) after the thread started; locked once per answer,
/// never on the interface's hot paths.
type SharedWaker = Arc<Mutex<Option<Box<dyn Fn() + Send>>>>;

/// The worker thread of the project files (see the module documentation).
pub struct ProjectWorker {
    /// The running thread, once a job started it.
    running: Option<Running>,
    /// What the thread calls after every answer.
    waker: SharedWaker,
    /// Whether a thread may be started at all: `false` for [`ProjectWorker::on_this_thread`].
    threads: bool,
    /// Answers of jobs run on the calling thread (no thread could be started, or none may be),
    /// waiting for [`ProjectWorker::try_take`] like the thread's.
    ready: VecDeque<JobAnswer>,
    /// Jobs sent to the thread whose answers have not been taken yet.
    out: usize,
}

/// The thread and its two channels.
struct Running {
    /// Where the jobs go.
    jobs: Sender<Job>,
    /// Where the answers come from.
    answers: Receiver<JobAnswer>,
    /// The thread, joined when the worker is dropped.
    thread: JoinHandle<()>,
}

impl ProjectWorker {
    /// A worker whose thread starts at the first job. Allocates nothing until then.
    #[must_use]
    pub fn new() -> Self {
        Self {
            running: None,
            waker: Arc::new(Mutex::new(None)),
            threads: true,
            ready: VecDeque::new(),
            out: 0,
        }
    }

    /// A worker that never starts a thread and runs every job on the calling thread at once: what
    /// the program does when the system refuses a thread (see the module documentation), made
    /// available so that the tests check that path and the screenshots stay single-threaded.
    #[must_use]
    pub fn on_this_thread() -> Self {
        let mut worker = Self::new();
        worker.threads = false;
        worker
    }

    /// Gives the function called after every answer of the thread (the window's waker in the
    /// program), replacing the previous one; it applies to the thread already running too.
    pub fn set_waker(&mut self, waker: impl Fn() + Send + 'static) {
        // A poisoned lock only means a waker panicked once; the slot itself is still usable.
        let mut slot = self.waker.lock().unwrap_or_else(PoisonError::into_inner);
        *slot = Some(Box::new(waker));
    }

    /// Sends `job`: to the thread (started now if needed), or, when no thread can run it, runs it
    /// here at once and keeps its answer for [`ProjectWorker::try_take`]. Never blocks on a job
    /// already out.
    pub fn submit(&mut self, job: Job) {
        if self.threads && self.running.is_none() {
            self.running = Running::start(Arc::clone(&self.waker));
        }
        let job = match &self.running {
            Some(running) => match running.jobs.send(job) {
                Ok(()) => {
                    self.out += 1;
                    return;
                }
                // The thread has gone (it cannot, short of the system killing it): take the job
                // back and run it here.
                Err(error) => error.0,
            },
            None => job,
        };
        self.ready.push_back(run_caught(job));
    }

    /// The next answer, if one is ready, in the order the jobs were sent. Never blocks. O(1).
    pub fn try_take(&mut self) -> Option<JobAnswer> {
        if let Some(answer) = self.ready.pop_front() {
            return Some(answer);
        }
        let running = self.running.as_ref()?;
        match running.answers.try_recv() {
            Ok(answer) => {
                self.out = self.out.saturating_sub(1);
                Some(answer)
            }
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => self.lost(),
        }
    }

    /// The next answer, **waiting** for it when a job is out; `None` when no job is out. For the
    /// tests and for whatever has no event loop to wake: the interface never calls it.
    pub fn wait(&mut self) -> Option<JobAnswer> {
        if let Some(answer) = self.ready.pop_front() {
            return Some(answer);
        }
        if self.out == 0 {
            return None;
        }
        let running = self.running.as_ref()?;
        match running.answers.recv() {
            Ok(answer) => {
                self.out = self.out.saturating_sub(1);
                Some(answer)
            }
            Err(_) => self.lost(),
        }
    }

    /// How many jobs have been sent and their answers not taken yet. O(1).
    #[must_use]
    pub fn pending(&self) -> usize {
        self.ready.len() + self.out
    }

    /// The thread ended with jobs out (it cannot: every job's panic is caught, so only the system
    /// killing the thread could do it). Their answers will never come; forgetting them lets the
    /// next job start a new thread. The flow keeps waiting for its job, a state the tests rule
    /// out.
    fn lost(&mut self) -> Option<JobAnswer> {
        self.out = 0;
        if let Some(running) = self.running.take() {
            drop(running.jobs);
            let _ = running.thread.join();
        }
        None
    }
}

impl Default for ProjectWorker {
    /// [`ProjectWorker::new`].
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for ProjectWorker {
    /// The state worth knowing: whether a thread runs, and how many answers are awaited.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProjectWorker")
            .field("running", &self.running.is_some())
            .field("threads", &self.threads)
            .field("pending", &self.pending())
            .finish_non_exhaustive()
    }
}

impl Drop for ProjectWorker {
    /// Finishes the jobs already sent, then ends the thread (see the module documentation).
    fn drop(&mut self) {
        if let Some(running) = self.running.take() {
            // Closing the channel ends the thread's loop once it has run what it holds.
            drop(running.jobs);
            // A thread that panicked outside a job has nothing more to report.
            let _ = running.thread.join();
        }
    }
}

impl Running {
    /// Starts the thread, or `None` if the system refuses one.
    fn start(waker: SharedWaker) -> Option<Self> {
        let (jobs, inbox) = mpsc::channel::<Job>();
        let (outbox, answers) = mpsc::channel::<JobAnswer>();
        let thread = thread::Builder::new()
            .name(THREAD_NAME.to_owned())
            .spawn(move || {
                // One job at a time, in order, until the worker closes the channel.
                while let Ok(job) = inbox.recv() {
                    // The worker may be gone already (at exit): nobody left to answer.
                    if outbox.send(run_caught(job)).is_err() {
                        break;
                    }
                    let slot = waker.lock().unwrap_or_else(PoisonError::into_inner);
                    if let Some(wake) = slot.as_ref() {
                        wake();
                    }
                }
            })
            .ok()?;
        Some(Self {
            jobs,
            answers,
            thread,
        })
    }
}

/// Runs `job`, turning a panic inside it into [`JobAnswer::Stopped`] (see the module
/// documentation). The path is copied first, since the job is consumed by running.
fn run_caught(job: Job) -> JobAnswer {
    let (path, writing): (PathBuf, bool) = match &job {
        Job::Write { path, .. } => (path.clone(), true),
        Job::Read { path } => (path.clone(), false),
    };
    // `AssertUnwindSafe`: after a panic nothing of the job is used again (it was moved into the
    // closure and is dropped with the unwinding), so no broken invariant can be observed.
    catch_unwind(AssertUnwindSafe(|| job.run())).unwrap_or(JobAnswer::Stopped { path, writing })
}
