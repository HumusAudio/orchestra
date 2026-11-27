// SPDX-License-Identifier: GPL-3.0-or-later

//! The open project (ADR-0050 DG5, SCALETTA F2.12a5b): the one document the program holds, the
//! file it came from, whether it has unsaved changes, and the small state machine of *New*,
//! *Open*, *Save*, *Save as* and closing, with the question "Save the changes?" in between.
//!
//! # What lives here, and what does not
//! This module knows nothing of widgets or windows: it is plain logic, so the tests drive every
//! path of it without an interface. It answers each step with a [`ProjectEffect`], what the
//! interface has to show or ask next (a question, a file chooser, an error, a file to read or
//! write, closing the window); the demonstration ([`crate::Demo`]) turns effects into dialogs,
//! jobs for the worker thread and window requests, and hands the answers back. The files are read
//! and written by a [`Job`], with `orchestra-io` (ADR-0029), on the worker thread of the module
//! `worker` (ADR-0054 AR1).
//!
//! # The open document (DG5)
//! [`OpenProject`] holds the [`Editor`] (ADR-0026: one history for every view) and the path of
//! its file, `None` for a project never saved. **Modified** means the document's revision differs
//! from the revision it had when it was last opened or saved: the revision grows with every
//! transaction, undo and redo (ADR-0026 §5), so changing something and undoing it still counts
//! as modified. This is DG5 as written (choice 4 of F2.12a5b); clearing the mark when undo comes
//! back to the saved state would need the history's position, and is left for when notes can be
//! entered (F2.22), a recorded limit.
//!
//! # A new document (choice 1 of F2.12a5b)
//! The model deliberately has no default meter, tempo or key (`DocumentBuilder::new`): they are
//! the caller's. Until the "new score" dialog exists, *New* makes a small valid document: 4/4,
//! ♩ = 120 (not printed), C major, **one empty bar** and no tracks ([`new_document`]). One bar,
//! not none (owner's choice of 2026-11-25): the model keeps at least one bar everywhere else
//! (deleting bars always leaves one, ADR-0026 §6), and inserting bars into a document without
//! any was the one path that broke validation rule 13, fixed in `orchestra-core-model` in the
//! same step.
//!
//! # The flow, step by step
//! [`ProjectFlow`] is in one of seven steps ([`Step`]):
//! - **idle**: nothing open; every command may start;
//! - **asking**: the question "Save the changes to «Name»?" is open, with what waits for the
//!   answer (*New*, *Open* or closing: a [`Then`]);
//! - **saving**: the *Save as* chooser is open, maybe with something waiting after the save;
//! - **replacing**: the question "«Name.orchestra» already exists. Replace it?" is open (the
//!   extension was added to the chosen name, so the chooser never confirmed that file);
//! - **opening**: the *Open* chooser is open;
//! - **writing**: a [`Job::Write`] is out, maybe with something waiting after the save;
//! - **reading**: a [`Job::Read`] is out.
//!
//! While a step other than idle is open, a new command is ignored (the interface's modal layers
//! keep it from coming anyway). A request to close the window is refused without a second
//! question while a question or a chooser is open; while a file is being **written or read** it is
//! **kept** instead, and runs when the job's answer has come (ADR-0054 AR1): the user who presses
//! the close button during a long save sees the window close once the save is done (or the
//! question, if the project is still modified), never a refusal they could not understand. It is
//! dropped if the job failed (the error is shown, and the user decides again) or if the flow is
//! not idle afterwards (*Open* waiting on the save opened its chooser).
//!
//! What waits for the answer runs only if the project was really put away: *Don't save*, or a
//! save that succeeded. Cancelling the question, the chooser or the replacement, or a save that
//! failed, drops it, and the document stays as it was.
//!
//! # The extension (choice 3 of F2.12a5b)
//! Not every system adds `.orchestra` to a name typed in the *Save as* chooser (GTK's portal does
//! not). [`with_project_extension`] adds it when the name does not already end with it (in any
//! case: `Song.ORCHESTRA` is kept). If the name so made is an existing file other than the one
//! chosen, we ask before replacing it.
//!
//! # Reading and writing on a worker thread (ADR-0054 AR1, F2.14a)
//! Until F2.12a5b opening and saving ran here, on the interface's thread: `orchestra-io`'s
//! benchmarks measured natively about 5.5 ms to save and 11 ms to load the normal reference score
//! (a 0.84 MB file), but 168 ms and 396 ms for the heavy one (25.4 MB), a pause in which the window
//! neither redraws nor answers the compositor. Now the flow only **describes** the work as a
//! [`Job`] in a [`ProjectEffect::Run`], and the interface hands it to the worker thread (the module
//! `worker`), whose answer comes back with [`ProjectFlow::finished`].
//!
//! - **Saving** starts from a **snapshot**: a copy of the document (`Document::clone`, cheap
//!   because the voices' content is shared copy-on-write, ADR-0022 §7; measured in
//!   `orchestra-core-model`'s `benches/parts.rs`) taken with its **revision**. The worker
//!   encodes and writes the copy; the document itself stays on the interface's thread, never
//!   shared, so no edit ever waits for a lock (CLAUDE.md §5). When the answer comes, the project
//!   is "saved at that revision": anything changed meanwhile still counts as modified (DG5).
//! - **Opening** is entirely the worker's: reading, parsing and the full validation (ADR-0029 S8)
//!   build the document there, and the flow only swaps it in.
//! - [`Job::run`] is an ordinary function, so the tests run a job where they are, and the worker
//!   falls back on running it on the interface's thread if the system refuses a thread.
//!
//! The writes are safe as before (a temporary file renamed over the target, ADR-0029 S13).

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use orchestra_core_model::{
    Document, DocumentBuilder, DocumentError, Editor, Key, Metronome, Mode, PitchError, TempoMark,
};
use orchestra_core_time::{
    Duration, Meter, MeterChange, MeterError, MeterMap, MeterMapError, Rational, RationalError,
    Tempo, TempoError,
};
use orchestra_io::{LoadError, PROJECT_EXTENSION, SaveError};

mod worker;

pub use worker::ProjectWorker;

/// The beats per minute of a new document (choice 1 of F2.12a5b): the usual default of notation
/// programs and DAWs alike, in quarter notes.
const NEW_BPM: i64 = 120;

/// Why a new document could not be made. Every value [`new_document`] uses is a valid constant,
/// so none of these can happen; they are reported instead of a panic (CLAUDE.md §4), and the
/// interface shows them as a mistake of the program.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NewDocumentError {
    /// The beat unit or the tempo as a fraction.
    Rational(RationalError),
    /// The meter 4/4.
    Meter(MeterError),
    /// The meter map made from it.
    MeterMap(MeterMapError),
    /// The tempo ♩ = 120.
    Tempo(TempoError),
    /// The key of C major.
    Key(PitchError),
    /// The document itself (the tempo mark, the builder, its one bar).
    Document(DocumentError),
}

impl std::fmt::Display for NewDocumentError {
    /// A short English description, for developers (the interface shows a translated text).
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Rational(e) => write!(f, "cannot make a new document: {e}"),
            Self::Meter(e) => write!(f, "cannot make a new document: {e}"),
            Self::MeterMap(e) => write!(f, "cannot make a new document: {e}"),
            Self::Tempo(e) => write!(f, "cannot make a new document: {e}"),
            Self::Key(e) => write!(f, "cannot make a new document: {e}"),
            Self::Document(e) => write!(f, "cannot make a new document: {e}"),
        }
    }
}

impl std::error::Error for NewDocumentError {}

/// The document *New* makes (see the module documentation): 4/4, ♩ = 120 not printed, C major,
/// one empty bar, no tracks. O(1) plus the builder's few small allocations.
///
/// # Errors
/// [`NewDocumentError`], which the constants above never cause.
pub fn new_document() -> Result<Document, NewDocumentError> {
    let meter = Meter::new(&[4], 4).map_err(NewDocumentError::Meter)?;
    let meters = MeterMap::new(MeterChange::new(meter)).map_err(NewDocumentError::MeterMap)?;
    let quarter = Duration::from_rational(Rational::new(1, 4).map_err(NewDocumentError::Rational)?);
    let bpm = Rational::from_integer(NEW_BPM).map_err(NewDocumentError::Rational)?;
    let tempo = Tempo::new(quarter, bpm).map_err(NewDocumentError::Tempo)?;
    // Not printed: a new score shows no "♩ = 120" until the user writes one.
    let mark = TempoMark::immediate(
        None,
        Metronome {
            tempo,
            shown: false,
        },
    )
    .map_err(NewDocumentError::Document)?;
    let key = Key::traditional(0, Mode::Major).map_err(NewDocumentError::Key)?;
    let mut builder =
        DocumentBuilder::new(meters, mark, key).map_err(NewDocumentError::Document)?;
    let _first_bar = builder.add_bar().map_err(NewDocumentError::Document)?;
    builder.build().map_err(NewDocumentError::Document)
}

/// `path` with `.orchestra` at its end (see the module documentation): unchanged when its
/// extension already is `orchestra` in any case, otherwise with `.orchestra` appended to the
/// whole name, so `Song.txt` becomes `Song.txt.orchestra` (the user's own dots are never taken
/// for an extension to replace). A name ending with a dot gets only `orchestra`, so `Song.` gives
/// `Song.orchestra`, not `Song..orchestra`.
///
/// Works on the `OsStr` of the path, never on a lossy string, so names that are not valid UTF-8
/// (possible on Linux) are kept byte for byte. O(path length).
#[must_use]
pub fn with_project_extension(path: PathBuf) -> PathBuf {
    with_extension_added(path, PROJECT_EXTENSION)
}

/// `path` with `.extension` at its end, by [`with_project_extension`]'s rules for any extension
/// (F2.12c2: the shortcut files' `orchestra-shortcuts`, ADR-0051 SF5). O(path length).
#[must_use]
pub fn with_extension_added(path: PathBuf, extension: &str) -> PathBuf {
    let has_it = path
        .extension()
        .is_some_and(|own| own.eq_ignore_ascii_case(extension));
    if has_it {
        return path;
    }
    // `Path::extension` answers `Some("")` for a name ending with a dot: the dot is already there.
    let ends_with_dot = path.extension().is_some_and(std::ffi::OsStr::is_empty);
    let mut name: OsString = path.into_os_string();
    if !ends_with_dot {
        name.push(".");
    }
    name.push(extension);
    PathBuf::from(name)
}

/// The name of a project saved at `path`: the file's name without its extension, as the window
/// title and the question show it (`Song` for `…/Song.orchestra`). Lossy only for a name that is
/// not valid UTF-8, which is shown with replacement characters (it is never used to find the
/// file again). `None` for a path without a file name (never chosen by a chooser).
#[must_use]
pub fn project_name(path: &Path) -> Option<String> {
    path.file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
}

/// The whole name of the file at `path` (`Song.orchestra`), for the error messages and the
/// replacement question; the whole path if it has no file name. Lossy as [`project_name`].
#[must_use]
pub fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

/// The open project (see the module documentation).
#[derive(Debug)]
pub struct OpenProject {
    /// The document and its history.
    editor: Editor,
    /// Where it was opened from or last saved to; `None` for a project never saved.
    path: Option<PathBuf>,
    /// The document's revision when it was opened or last saved (DG5).
    saved: u64,
}

impl OpenProject {
    /// A new project ([`new_document`]), never saved, not modified.
    ///
    /// # Errors
    /// [`NewDocumentError`], as [`new_document`].
    pub fn new() -> Result<Self, NewDocumentError> {
        Ok(Self::with_document(new_document()?, None))
    }

    /// The project holding `document`, which came from `path` (`None`: never saved); not
    /// modified. O(1).
    #[must_use]
    pub fn with_document(document: Document, path: Option<PathBuf>) -> Self {
        let saved = document.revision();
        Self {
            editor: Editor::new(document),
            path,
            saved,
        }
    }

    /// The editor: every change to the document goes through it (ADR-0026).
    #[must_use]
    pub const fn editor(&self) -> &Editor {
        &self.editor
    }

    /// The editor, to change the document (the tests; the views from F2.21).
    pub const fn editor_mut(&mut self) -> &mut Editor {
        &mut self.editor
    }

    /// Where the project was opened from or last saved to.
    #[must_use]
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// The project's name ([`project_name`] of its path), `None` while it was never saved (the
    /// interface writes *Untitled*).
    #[must_use]
    pub fn name(&self) -> Option<String> {
        self.path.as_deref().and_then(project_name)
    }

    /// Whether the document changed since it was opened or last saved (DG5; see the module
    /// documentation for undo). O(1).
    #[must_use]
    pub const fn is_modified(&self) -> bool {
        self.editor.document().revision() != self.saved
    }

    /// A job that saves a snapshot of the document to `path` (see the module documentation):
    /// the copy and its revision. Nothing changes here until the answer comes
    /// ([`OpenProject::saved_at`]).
    ///
    /// # Complexity
    /// A copy that **shares** the voices' content (copy-on-write chunks, ADR-0022 §7) and
    /// duplicates the rest: measured about 0.51 M instructions for the normal reference score and
    /// 2.24 M for the heavy one (`orchestra-core-model`'s `benches/parts.rs`, `snapshot`), well
    /// within a frame. An edit made while the job is out copies only the chunk it touches. The
    /// encoding and the write are the worker's.
    #[must_use]
    pub fn save_job(&self, path: PathBuf) -> Job {
        let document = self.editor.document();
        Job::Write {
            path,
            revision: document.revision(),
            document: Box::new(document.clone()),
        }
    }

    /// A save of the snapshot taken at `revision` succeeded: the project belongs to `path` from
    /// now on, and is no longer modified unless it changed since that snapshot. O(1).
    pub fn saved_at(&mut self, path: PathBuf, revision: u64) {
        self.saved = revision;
        self.path = Some(path);
    }
}

/// Work on a project file that the interface hands to the worker thread (see the module
/// documentation), from a [`ProjectEffect::Run`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Job {
    /// Write `document` to `path` (ADR-0029 S13: a failure leaves the file there as it was).
    Write {
        /// The file.
        path: PathBuf,
        /// The document's revision when the snapshot was taken.
        revision: u64,
        /// The snapshot. Boxed: the jobs travel through a channel, and the other variant is a
        /// path.
        document: Box<Document>,
    },
    /// Read the project at `path`, with the full validation of ADR-0029 S8.
    Read {
        /// The file.
        path: PathBuf,
    },
}

impl Job {
    /// Does the work, on the calling thread: the worker's, or the interface's in the tests and
    /// when no thread could be started.
    ///
    /// # Complexity
    /// O(size of the document) to encode, or to parse and validate, plus the file's reading or
    /// writing: for the heavy reference score natively about 168 ms to save and 396 ms to open.
    #[must_use]
    pub fn run(self) -> JobAnswer {
        match self {
            Self::Write {
                path,
                revision,
                document,
            } => {
                let result = orchestra_io::save(&document, &path);
                JobAnswer::Written {
                    path,
                    revision,
                    result,
                }
            }
            Self::Read { path } => {
                let result = orchestra_io::load(&path).map(Box::new);
                JobAnswer::Read { path, result }
            }
        }
    }
}

/// What a [`Job`] answered, for [`ProjectFlow::finished`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobAnswer {
    /// The answer of a [`Job::Write`].
    Written {
        /// The file.
        path: PathBuf,
        /// The revision of the snapshot written.
        revision: u64,
        /// Whether it was written.
        result: Result<(), SaveError>,
    },
    /// The answer of a [`Job::Read`].
    Read {
        /// The file.
        path: PathBuf,
        /// The document read (boxed as in [`Job::Write`]), or why it could not be.
        result: Result<Box<Document>, LoadError>,
    },
    /// The job stopped in the middle, by a mistake of the program (a panic the worker caught,
    /// module `worker`): nothing is known of its result.
    Stopped {
        /// The job's file.
        path: PathBuf,
        /// `true` for a [`Job::Write`], `false` for a [`Job::Read`].
        writing: bool,
    },
}

/// Why a step of the flow failed, for the interface to say in the user's language.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectError {
    /// *New* could not make a document (never happens, see [`NewDocumentError`]).
    New(NewDocumentError),
    /// The file at `path` could not be opened.
    Open {
        /// The file.
        path: PathBuf,
        /// Why.
        error: LoadError,
    },
    /// The project could not be saved to `path`; the file there, if any, is as it was.
    Save {
        /// The file.
        path: PathBuf,
        /// Why.
        error: SaveError,
    },
    /// Opening or saving `path` stopped in the middle by a mistake of the program
    /// ([`JobAnswer::Stopped`]). A save leaves the file there, if any, as it was (ADR-0029 S13:
    /// the new file only replaces it by a rename, at the very end).
    Stopped {
        /// The file.
        path: PathBuf,
        /// Whether it was a save (`false`: an opening).
        writing: bool,
    },
}

/// What waits for the project to be put away (saved, or its changes discarded).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Then {
    /// Make a new project.
    New,
    /// Ask for a project to open.
    Open,
    /// Close the window.
    Close,
}

/// The answer to "Save the changes?".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveAnswer {
    /// Save, then go on.
    Save,
    /// Throw the changes away and go on.
    Discard,
    /// Go back: nothing happens.
    Cancel,
}

/// The step the flow is in (see the module documentation).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// Nothing open.
    Idle,
    /// "Save the changes?" is open; the answer decides about the [`Then`].
    Asking(Then),
    /// The *Save as* chooser is open; after a save that succeeds, the [`Then`] if any.
    Saving(Option<Then>),
    /// "Replace it?" is open for `path`; after a save that succeeds, the [`Then`] if any.
    Replacing {
        /// The file that would be replaced (the chosen name with the extension added).
        path: PathBuf,
        /// What waits for the save.
        then: Option<Then>,
    },
    /// The *Open* chooser is open.
    Opening,
    /// A [`Job::Write`] is out; after an answer that says it succeeded, the [`Then`] if any.
    Writing(Option<Then>),
    /// A [`Job::Read`] is out.
    Reading,
}

/// What the interface has to do next (see the module documentation).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectEffect {
    /// Ask "Save the changes to «`name`»?" (`None`: the project was never saved, *Untitled*),
    /// and answer with [`ProjectFlow::save_answer`].
    AskToSave {
        /// The project's name.
        name: Option<String>,
    },
    /// Open the *Open* chooser, and answer with [`ProjectFlow::chosen`].
    ChooseOpen,
    /// Open the *Save as* chooser proposing `name` (`None`: *Untitled*), and answer with
    /// [`ProjectFlow::chosen`].
    ChooseSave {
        /// The name proposed, without the extension.
        name: Option<String>,
    },
    /// Ask "«file» already exists. Replace it?" for `path`, and answer with
    /// [`ProjectFlow::replace_answer`].
    ConfirmReplace {
        /// The file that would be replaced.
        path: PathBuf,
    },
    /// Hand this job to the worker thread, block the interface while it runs (ADR-0054 AR1), and
    /// answer with [`ProjectFlow::finished`].
    Run(Job),
    /// Say that something failed.
    Failed(ProjectError),
    /// Close the window: the project was put away.
    Close,
}

/// The open project and the step it is in (see the module documentation).
#[derive(Debug)]
pub struct ProjectFlow {
    /// The project.
    project: OpenProject,
    /// The step.
    step: Step,
    /// A request to close the window that came while a job was out (see the module
    /// documentation): it runs after the answer.
    close_waiting: bool,
}

impl ProjectFlow {
    /// The flow holding `project`, idle.
    #[must_use]
    pub const fn new(project: OpenProject) -> Self {
        Self {
            project,
            step: Step::Idle,
            close_waiting: false,
        }
    }

    /// The open project.
    #[must_use]
    pub const fn project(&self) -> &OpenProject {
        &self.project
    }

    /// The open project, to change its document (the tests; the views from F2.21).
    pub const fn project_mut(&mut self) -> &mut OpenProject {
        &mut self.project
    }

    /// The step the flow is in.
    #[must_use]
    pub const fn step(&self) -> &Step {
        &self.step
    }

    /// *New*: asks about the changes first, if any; otherwise a new project replaces this one
    /// (or the error, if it could not be made). Ignored unless idle.
    pub fn new_project(&mut self) -> Option<ProjectEffect> {
        self.start(Then::New)
    }

    /// *Open*: asks about the changes first, if any; otherwise asks for a file. Ignored unless
    /// idle.
    pub fn open(&mut self) -> Option<ProjectEffect> {
        self.start(Then::Open)
    }

    /// Closing the window (Quit, the system's close button): asks about the changes first, if
    /// any; otherwise [`ProjectEffect::Close`]. Ignored while a question or a chooser is open (the
    /// window stays); **kept** while a job is out, and run after its answer (see the module
    /// documentation).
    pub fn close(&mut self) -> Option<ProjectEffect> {
        if self.is_busy() {
            self.close_waiting = true;
            return None;
        }
        self.start(Then::Close)
    }

    /// Whether a job is out (writing or reading): the interface is blocked until its answer.
    #[must_use]
    pub const fn is_busy(&self) -> bool {
        matches!(self.step, Step::Writing(_) | Step::Reading)
    }

    /// Whether a request to close the window is waiting for the job's answer.
    #[must_use]
    pub const fn close_waiting(&self) -> bool {
        self.close_waiting
    }

    /// The answer of the job of the last [`ProjectEffect::Run`]: a save that succeeded makes the
    /// project saved at its snapshot's revision and goes on with what waited; a document read
    /// replaces the project; a failure is said, and what waited is dropped. Then a close that
    /// waited runs, if the flow is idle and nothing failed. Ignored unless that job is out (an
    /// answer that does not match the step changes nothing).
    pub fn finished(&mut self, answer: JobAnswer) -> Option<ProjectEffect> {
        let effect = match (std::mem::replace(&mut self.step, Step::Idle), answer) {
            (
                Step::Writing(then),
                JobAnswer::Written {
                    path,
                    revision,
                    result,
                },
            ) => match result {
                Ok(()) => {
                    self.project.saved_at(path, revision);
                    then.and_then(|then| self.proceed(then))
                }
                Err(error) => Some(ProjectEffect::Failed(ProjectError::Save { path, error })),
            },
            // A mistake of the program stopped the job that is out: said like any failure, and
            // nothing changed (what waited on a save is dropped, as after a failed save).
            (step @ (Step::Writing(_) | Step::Reading), JobAnswer::Stopped { path, writing })
                if writing == matches!(step, Step::Writing(_)) =>
            {
                Some(ProjectEffect::Failed(ProjectError::Stopped {
                    path,
                    writing,
                }))
            }
            (Step::Reading, JobAnswer::Read { path, result }) => match result {
                Ok(document) => {
                    self.project = OpenProject::with_document(*document, Some(path));
                    None
                }
                Err(error) => Some(ProjectEffect::Failed(ProjectError::Open { path, error })),
            },
            // Not the job that is out: the step is put back as it was.
            (other, _) => {
                self.step = other;
                return None;
            }
        };
        let close = std::mem::take(&mut self.close_waiting);
        match effect {
            // Nothing else to show and nothing open: the close that waited goes on now.
            None if close && self.step == Step::Idle => self.close(),
            // A failure, a chooser, the window closing anyway: the waiting close is dropped.
            effect => effect,
        }
    }

    /// *Save*: writes over the project's file, or asks for one like *Save as* when it was never
    /// saved. Ignored unless idle.
    pub fn save(&mut self) -> Option<ProjectEffect> {
        if self.step != Step::Idle {
            return None;
        }
        Some(self.save_then(None))
    }

    /// *Save as*: asks for a file, proposing the project's name. Ignored unless idle.
    pub fn save_as(&mut self) -> Option<ProjectEffect> {
        if self.step != Step::Idle {
            return None;
        }
        self.step = Step::Saving(None);
        Some(ProjectEffect::ChooseSave {
            name: self.project.name(),
        })
    }

    /// The answer to "Save the changes?" (only while asking; ignored otherwise).
    pub fn save_answer(&mut self, answer: SaveAnswer) -> Option<ProjectEffect> {
        let Step::Asking(then) = self.step else {
            return None;
        };
        self.step = Step::Idle;
        match answer {
            SaveAnswer::Save => Some(self.save_then(Some(then))),
            SaveAnswer::Discard => self.proceed(then),
            SaveAnswer::Cancel => None,
        }
    }

    /// A chooser's answer: the chosen path, or `None` when it was cancelled or could not open
    /// (the interface has already said why). Ignored unless a chooser was open.
    pub fn chosen(&mut self, path: Option<PathBuf>) -> Option<ProjectEffect> {
        match std::mem::replace(&mut self.step, Step::Idle) {
            Step::Opening => path.map(|path| self.open_from(path)),
            Step::Saving(then) => {
                let path = path?;
                let with_extension = with_project_extension(path.clone());
                // The chooser confirmed `path` only: a different file that exists is asked
                // about. `try_exists` answers `false` when it cannot tell (no permission to look
                // in the folder): the save then fails with the system's own reason.
                if with_extension != path && with_extension.try_exists().unwrap_or(false) {
                    self.step = Step::Replacing {
                        path: with_extension.clone(),
                        then,
                    };
                    return Some(ProjectEffect::ConfirmReplace {
                        path: with_extension,
                    });
                }
                Some(self.write(with_extension, then))
            }
            // Not waiting for a chooser: the step is put back as it was.
            other => {
                self.step = other;
                None
            }
        }
    }

    /// The answer to "Replace it?": `true` saves over that file (then goes on), `false` drops the
    /// save and what waited for it. Ignored unless that question was open.
    pub fn replace_answer(&mut self, replace: bool) -> Option<ProjectEffect> {
        match std::mem::replace(&mut self.step, Step::Idle) {
            Step::Replacing { path, then } => {
                if replace {
                    Some(self.write(path, then))
                } else {
                    None
                }
            }
            other => {
                self.step = other;
                None
            }
        }
    }

    /// Starts *New*, *Open* or closing from idle: the question first when there are changes.
    fn start(&mut self, then: Then) -> Option<ProjectEffect> {
        if self.step != Step::Idle {
            return None;
        }
        if self.project.is_modified() {
            self.step = Step::Asking(then);
            return Some(ProjectEffect::AskToSave {
                name: self.project.name(),
            });
        }
        self.proceed(then)
    }

    /// Saves to the project's own file (then goes on), or asks for one when it has none.
    fn save_then(&mut self, then: Option<Then>) -> ProjectEffect {
        if let Some(path) = self.project.path().map(Path::to_path_buf) {
            self.write(path, then)
        } else {
            self.step = Step::Saving(then);
            ProjectEffect::ChooseSave { name: None }
        }
    }

    /// Sends the job that writes the project to `path`; `then` waits for its answer
    /// ([`ProjectFlow::finished`]).
    fn write(&mut self, path: PathBuf, then: Option<Then>) -> ProjectEffect {
        self.step = Step::Writing(then);
        ProjectEffect::Run(self.project.save_job(path))
    }

    /// Does what waited, the project now being put away.
    fn proceed(&mut self, then: Then) -> Option<ProjectEffect> {
        match then {
            Then::New => match OpenProject::new() {
                Ok(project) => {
                    self.project = project;
                    None
                }
                Err(error) => Some(ProjectEffect::Failed(ProjectError::New(error))),
            },
            Then::Open => {
                self.step = Step::Opening;
                Some(ProjectEffect::ChooseOpen)
            }
            Then::Close => Some(ProjectEffect::Close),
        }
    }

    /// Sends the job that reads the file at `path`; its answer replaces the project, or says why
    /// it could not ([`ProjectFlow::finished`]): until then the project stays.
    fn open_from(&mut self, path: PathBuf) -> ProjectEffect {
        self.step = Step::Reading;
        ProjectEffect::Run(Job::Read { path })
    }
}
