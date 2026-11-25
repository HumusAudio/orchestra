// SPDX-License-Identifier: GPL-3.0-or-later

//! The open project (ADR-0050 DG5, SCALETTA F2.12a5b): the one document the program holds, the
//! file it came from, whether it has unsaved changes, and the small state machine of *New*,
//! *Open*, *Save*, *Save as* and closing, with the question "Save the changes?" in between.
//!
//! # What lives here, and what does not
//! This module knows nothing of widgets or windows: it is plain logic, so the tests drive every
//! path of it without an interface. It answers each step with an [`ProjectEffect`], what the interface
//! has to show or ask next (a question, a file chooser, an error, closing the window); the
//! demonstration ([`crate::Demo`]) turns effects into dialogs and window requests, and hands the
//! answers back. The files are read and written here, with `orchestra-io` (ADR-0029).
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
//! [`ProjectFlow`] is in one of five steps ([`Step`]):
//! - **idle**: nothing open; every command may start;
//! - **asking**: the question "Save the changes to «Name»?" is open, with what waits for the
//!   answer (*New*, *Open* or closing: a [`Then`]);
//! - **saving**: the *Save as* chooser is open, maybe with something waiting after the save;
//! - **replacing**: the question "«Name.orchestra» already exists. Replace it?" is open (the
//!   extension was added to the chosen name, so the chooser never confirmed that file);
//! - **opening**: the *Open* chooser is open.
//!
//! While a step other than idle is open, a new command is ignored (the interface's modal layers
//! keep it from coming anyway), and a request to close the window is refused without a second
//! question.
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
//! # Reading and writing on the interface's thread (choice 5 of F2.12a5b)
//! Opening and saving run synchronously here. `orchestra-io`'s benchmarks (`benches/project.rs`)
//! measured natively about 5.5 ms to save and 11 ms to load the normal reference score (a 0.84 MB
//! file), within one frame or two; the heavy score (25.4 MB) takes 168 ms and 396 ms, a visible
//! pause that only a very large project would show. The writes are already safe (a temporary
//! file renamed over the target, ADR-0029 S13). A worker thread comes with the automatic save
//! (F2.14), which needs one anyway; then the interface stays live for large projects too.

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
    let has_it = path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case(PROJECT_EXTENSION));
    if has_it {
        return path;
    }
    // `Path::extension` answers `Some("")` for a name ending with a dot: the dot is already there.
    let ends_with_dot = path.extension().is_some_and(std::ffi::OsStr::is_empty);
    let mut name: OsString = path.into_os_string();
    if !ends_with_dot {
        name.push(".");
    }
    name.push(PROJECT_EXTENSION);
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

    /// Saves the document to `path` (ADR-0029 S13: a failure leaves the file there as it was);
    /// on success the project belongs to `path` from now on and is no longer modified. On a
    /// failure nothing changes: the project keeps its old path and its mark.
    ///
    /// # Errors
    /// [`ProjectError::Save`] with the path and `orchestra-io`'s error.
    ///
    /// # Complexity
    /// O(size of the document) to encode, plus the write.
    pub fn save_to(&mut self, path: PathBuf) -> Result<(), ProjectError> {
        match orchestra_io::save(self.editor.document(), &path) {
            Ok(()) => {
                self.saved = self.editor.document().revision();
                self.path = Some(path);
                Ok(())
            }
            Err(error) => Err(ProjectError::Save { path, error }),
        }
    }
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
}

impl ProjectFlow {
    /// The flow holding `project`, idle.
    #[must_use]
    pub const fn new(project: OpenProject) -> Self {
        Self {
            project,
            step: Step::Idle,
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
    /// any; otherwise [`ProjectEffect::Close`]. Ignored unless idle: a question or a chooser is open,
    /// and the window stays.
    pub fn close(&mut self) -> Option<ProjectEffect> {
        self.start(Then::Close)
    }

    /// *Save*: writes over the project's file, or asks for one like *Save as* when it was never
    /// saved. Ignored unless idle.
    pub fn save(&mut self) -> Option<ProjectEffect> {
        if self.step != Step::Idle {
            return None;
        }
        self.save_then(None)
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
            SaveAnswer::Save => self.save_then(Some(then)),
            SaveAnswer::Discard => self.proceed(then),
            SaveAnswer::Cancel => None,
        }
    }

    /// A chooser's answer: the chosen path, or `None` when it was cancelled or could not open
    /// (the interface has already said why). Ignored unless a chooser was open.
    pub fn chosen(&mut self, path: Option<PathBuf>) -> Option<ProjectEffect> {
        match std::mem::replace(&mut self.step, Step::Idle) {
            Step::Opening => path.and_then(|path| self.open_from(path)),
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
                self.write(with_extension, then)
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
                    self.write(path, then)
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
    fn save_then(&mut self, then: Option<Then>) -> Option<ProjectEffect> {
        if let Some(path) = self.project.path().map(Path::to_path_buf) {
            self.write(path, then)
        } else {
            self.step = Step::Saving(then);
            Some(ProjectEffect::ChooseSave { name: None })
        }
    }

    /// Writes the project to `path`; on success goes on with `then`, on failure says why and
    /// drops it. Leaves the flow idle unless `then` asks for a chooser.
    fn write(&mut self, path: PathBuf, then: Option<Then>) -> Option<ProjectEffect> {
        match self.project.save_to(path) {
            Ok(()) => then.and_then(|then| self.proceed(then)),
            Err(error) => Some(ProjectEffect::Failed(error)),
        }
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

    /// Opens the file at `path`, which replaces the project on success; on failure the project
    /// stays and the error is said.
    fn open_from(&mut self, path: PathBuf) -> Option<ProjectEffect> {
        match orchestra_io::load(&path) {
            Ok(document) => {
                self.project = OpenProject::with_document(document, Some(path));
                None
            }
            Err(error) => Some(ProjectEffect::Failed(ProjectError::Open { path, error })),
        }
    }
}
