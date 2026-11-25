// SPDX-License-Identifier: GPL-3.0-or-later

//! The open project (ADR-0050 DG5, SCALETTA F2.12a5b), headless: the new document, the
//! modification mark, *New*, *Open*, *Save*, *Save as* with the extension added and the question
//! about replacing, the question about unsaved changes before *New*, *Open* and closing, the
//! errors and their texts in the six languages, and the demonstration wiring it to its dialogs,
//! its file choosers, its window title and its window's close.
//!
//! The files are written in a folder of the system's temporary directory made for each test
//! (named after the test and the process, so parallel tests never share one) and removed at the
//! end.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test helpers: a failed unwrap or index fails the test"
)]

use std::path::{Path, PathBuf};

use orchestra_app::{
    Demo, NameTemplate, OpenProject, ProjectEffect, ProjectError, ProjectFlow, ProjectTexts,
    SaveAnswer, SceneContext, Step, Then, new_document, project_error_dialog, project_name,
    window_title, with_project_extension,
};
use orchestra_core_model::Command;
use orchestra_core_time::BarIndex;
use orchestra_i18n::{Locale, Localizer};
use orchestra_io::{LoadError, SaveError};
use orchestra_ui::{
    ButtonState, DisplayList, FileChoice, FileRequestKind, InputEvent, Key, Modifiers, NamedKey,
    PhysicalKey, Rect, WindowApp, WindowRequest,
};

/// A fresh, empty folder for the test `name`, removed by [`Scratch`]'s `Drop`.
struct Scratch(PathBuf);

impl Scratch {
    /// The folder of the test `name`.
    fn new(name: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("orchestra-project-{name}-{}", std::process::id()));
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

/// Changes the document of `flow` (two bars inserted at the start), so that it has unsaved
/// changes.
fn modify(flow: &mut ProjectFlow) {
    flow.project_mut()
        .editor_mut()
        .execute(&Command::InsertBars {
            at: BarIndex::new(0),
            count: 2,
        })
        .unwrap();
    assert!(flow.project().is_modified());
}

/// A flow holding a new project.
fn flow() -> ProjectFlow {
    ProjectFlow::new(OpenProject::new().unwrap())
}

#[test]
fn a_new_document_is_valid_and_saves_and_loads() {
    let scratch = Scratch::new("new");
    let document = new_document().unwrap();
    assert_eq!(document.revision(), 0);
    assert_eq!(document.bars().len(), 1, "one empty bar");
    assert_eq!(document.tracks().len(), 0);
    let path = scratch.join("Empty.orchestra");
    orchestra_io::save(&document, &path).unwrap();
    assert!(orchestra_io::load(&path).unwrap().same_content(&document));
}

#[test]
fn a_new_project_is_untitled_and_not_modified() {
    let mut flow = flow();
    assert_eq!(flow.project().path(), None);
    assert_eq!(flow.project().name(), None);
    assert!(!flow.project().is_modified());
    assert_eq!(flow.step(), &Step::Idle);
    // Nothing to ask: New and closing go ahead at once.
    assert_eq!(flow.new_project(), None);
    assert_eq!(flow.close(), Some(ProjectEffect::Close));
}

#[test]
fn the_extension_is_added_only_when_missing() {
    let cases = [
        ("Song", "Song.orchestra"),
        ("Song.orchestra", "Song.orchestra"),
        ("Song.ORCHESTRA", "Song.ORCHESTRA"),
        ("Song.txt", "Song.txt.orchestra"),
        ("Song.", "Song.orchestra"),
        ("Op. 1 v2.0", "Op. 1 v2.0.orchestra"),
    ];
    for (typed, saved) in cases {
        let folder = Path::new("folder");
        assert_eq!(
            with_project_extension(folder.join(typed)),
            folder.join(saved),
            "{typed}"
        );
    }
    assert_eq!(
        project_name(Path::new("a").join("Song.orchestra").as_path()).as_deref(),
        Some("Song")
    );
}

#[test]
fn save_of_an_untitled_project_asks_for_a_file_and_adds_the_extension() {
    let scratch = Scratch::new("save-untitled");
    let mut flow = flow();
    modify(&mut flow);
    assert_eq!(flow.save(), Some(ProjectEffect::ChooseSave { name: None }));
    assert_eq!(flow.step(), &Step::Saving(None));
    // A second command while the chooser is open is ignored.
    assert_eq!(flow.save(), None);
    assert_eq!(flow.chosen(Some(scratch.join("Song"))), None);
    let path = scratch.join("Song.orchestra");
    assert_eq!(flow.project().path(), Some(path.as_path()));
    assert_eq!(flow.project().name().as_deref(), Some("Song"));
    assert!(!flow.project().is_modified());
    assert!(
        orchestra_io::load(&path)
            .unwrap()
            .same_content(flow.project().editor().document())
    );
    // Saved again, it writes over its own file without asking.
    modify(&mut flow);
    assert_eq!(flow.save(), None);
    assert!(!flow.project().is_modified());
    assert_eq!(orchestra_io::load(&path).unwrap().bars().len(), 5);
    // Save as proposes the project's name.
    assert_eq!(
        flow.save_as(),
        Some(ProjectEffect::ChooseSave {
            name: Some("Song".into())
        })
    );
    assert_eq!(flow.chosen(None), None, "cancelled");
    assert_eq!(flow.step(), &Step::Idle);
}

#[test]
fn an_existing_file_reached_by_adding_the_extension_is_asked_about() {
    let scratch = Scratch::new("replace");
    let existing = scratch.join("Song.orchestra");
    std::fs::write(&existing, b"someone else's file").unwrap();
    let mut flow = flow();
    modify(&mut flow);
    let _ = flow.save_as();
    assert_eq!(
        flow.chosen(Some(scratch.join("Song"))),
        Some(ProjectEffect::ConfirmReplace {
            path: existing.clone()
        })
    );
    // Cancel: nothing written, still modified and untitled.
    assert_eq!(flow.replace_answer(false), None);
    assert_eq!(std::fs::read(&existing).unwrap(), b"someone else's file");
    assert!(flow.project().is_modified());
    assert_eq!(flow.project().path(), None);
    // Replace: written over.
    let _ = flow.save_as();
    let _ = flow.chosen(Some(scratch.join("Song")));
    assert_eq!(flow.replace_answer(true), None);
    assert!(orchestra_io::load(&existing).is_ok());
    assert!(!flow.project().is_modified());
    // The chooser confirmed this very name itself: no second question.
    modify(&mut flow);
    let _ = flow.save_as();
    assert_eq!(flow.chosen(Some(existing.clone())), None);
    assert!(!flow.project().is_modified());
}

#[test]
fn unsaved_changes_are_asked_about_before_new() {
    let scratch = Scratch::new("ask-new");
    let mut flow = flow();
    modify(&mut flow);
    assert_eq!(
        flow.new_project(),
        Some(ProjectEffect::AskToSave { name: None })
    );
    assert_eq!(flow.step(), &Step::Asking(Then::New));
    // Cancel: nothing happens.
    assert_eq!(flow.save_answer(SaveAnswer::Cancel), None);
    assert!(flow.project().is_modified());
    // Don't save: the new project replaces it.
    let _ = flow.new_project();
    assert_eq!(flow.save_answer(SaveAnswer::Discard), None);
    assert!(!flow.project().is_modified());
    assert_eq!(flow.project().editor().document().bars().len(), 1);
    // Save: the chooser first (never saved), then the new project.
    modify(&mut flow);
    let _ = flow.new_project();
    assert_eq!(
        flow.save_answer(SaveAnswer::Save),
        Some(ProjectEffect::ChooseSave { name: None })
    );
    assert_eq!(flow.step(), &Step::Saving(Some(Then::New)));
    assert_eq!(flow.chosen(Some(scratch.join("Kept"))), None);
    assert!(scratch.join("Kept.orchestra").exists());
    assert_eq!(flow.project().path(), None, "the new project");
    assert!(!flow.project().is_modified());
    // A chooser cancelled after "Save" drops the new project too.
    modify(&mut flow);
    let _ = flow.new_project();
    let _ = flow.save_answer(SaveAnswer::Save);
    assert_eq!(flow.chosen(None), None);
    assert!(flow.project().is_modified(), "nothing was put away");
}

#[test]
fn unsaved_changes_are_asked_about_before_open_and_the_file_replaces_the_project() {
    let scratch = Scratch::new("ask-open");
    // A project on disk with three bars.
    let mut saved = flow();
    modify(&mut saved);
    let _ = saved.save_as();
    let _ = saved.chosen(Some(scratch.join("Other")));
    let other = scratch.join("Other.orchestra");
    // A saved project with changes: the question names it, Save writes it, then Open asks.
    let mut flow = flow();
    let _ = flow.save_as();
    let _ = flow.chosen(Some(scratch.join("Mine")));
    modify(&mut flow);
    assert_eq!(
        flow.open(),
        Some(ProjectEffect::AskToSave {
            name: Some("Mine".into())
        })
    );
    assert_eq!(
        flow.save_answer(SaveAnswer::Save),
        Some(ProjectEffect::ChooseOpen)
    );
    assert_eq!(
        orchestra_io::load(&scratch.join("Mine.orchestra"))
            .unwrap()
            .bars()
            .len(),
        3
    );
    assert_eq!(flow.step(), &Step::Opening);
    assert_eq!(flow.chosen(Some(other.clone())), None);
    assert_eq!(flow.project().path(), Some(other.as_path()));
    assert_eq!(flow.project().name().as_deref(), Some("Other"));
    assert!(!flow.project().is_modified());
    assert_eq!(flow.project().editor().document().bars().len(), 3);
}

#[test]
fn closing_asks_about_unsaved_changes_and_a_failed_save_keeps_the_window() {
    let scratch = Scratch::new("close");
    let mut flow = flow();
    modify(&mut flow);
    assert_eq!(flow.close(), Some(ProjectEffect::AskToSave { name: None }));
    // A second close while the question is open: refused, no second question.
    assert_eq!(flow.close(), None);
    assert_eq!(
        flow.save_answer(SaveAnswer::Discard),
        Some(ProjectEffect::Close)
    );
    // Saving into a folder that does not exist fails: the error, and the window stays.
    modify(&mut flow);
    let _ = flow.close();
    let _ = flow.save_answer(SaveAnswer::Save);
    let missing = scratch.join("no folder").join("Song");
    match flow.chosen(Some(missing)) {
        Some(ProjectEffect::Failed(ProjectError::Save {
            path,
            error: SaveError::Io(_),
        })) => assert_eq!(path, scratch.join("no folder").join("Song.orchestra")),
        other => panic!("expected a save error, got {other:?}"),
    }
    assert_eq!(flow.step(), &Step::Idle);
    assert!(flow.project().is_modified());
}

#[test]
fn files_that_cannot_be_opened_are_reported_and_keep_the_project() {
    let scratch = Scratch::new("open-errors");
    let garbage = scratch.join("Notes.orchestra");
    std::fs::write(&garbage, b"just some text").unwrap();
    let damaged = scratch.join("Damaged.orchestra");
    let mut bytes = orchestra_io::to_bytes(&new_document().unwrap()).unwrap();
    bytes.truncate(bytes.len() / 2);
    std::fs::write(&damaged, &bytes).unwrap();
    let missing = scratch.join("Missing.orchestra");
    let mut flow = flow();
    modify(&mut flow);
    for (path, expected) in [
        (
            missing,
            "The file doesn't exist or can't be reached at the moment.",
        ),
        (garbage, "The file isn't an Orchestra project."),
        (
            damaged,
            "The file is damaged: part of it can't be read as an Orchestra project.",
        ),
    ] {
        let _ = flow.open();
        let _ = flow.save_answer(SaveAnswer::Discard);
        let Some(ProjectEffect::Failed(error)) = flow.chosen(Some(path.clone())) else {
            panic!("{path:?} opened");
        };
        assert!(matches!(error, ProjectError::Open { .. }));
        let spec = project_error_dialog(&texts(Locale::EnUs), &error, ());
        assert_eq!(spec.message, expected, "{path:?}");
        assert_eq!(spec.details, [path.display().to_string()]);
        assert_eq!(spec.buttons.len(), 1);
        // The project stays, with its changes.
        assert_eq!(flow.project().path(), None);
        // Discarded by the question each time: a new, unmodified project.
        modify(&mut flow);
    }
}

#[test]
fn every_error_has_its_message() {
    let texts = texts(Locale::EnUs);
    let path = Path::new("dir").join("Song.orchestra");
    let message = |error: ProjectError| project_error_dialog(&texts, &error, ()).message;
    let open = |error: LoadError| ProjectError::Open {
        path: path.clone(),
        error,
    };
    assert_eq!(
        message(open(LoadError::TooLarge { bytes: 1 << 31 })),
        "The file is larger than Orchestra can open."
    );
    assert_eq!(
        message(open(LoadError::NewerFormat {
            found: Some(99),
            supported: 1
        })),
        "The project was saved with a newer version of Orchestra. Install the latest version to \
         open it."
    );
    let spec = project_error_dialog(
        &texts,
        &ProjectError::Save {
            path: path.clone(),
            error: SaveError::Encode("broken".into()),
        },
        (),
    );
    assert_eq!(spec.title, "“Song.orchestra” can't be saved");
    assert_eq!(
        spec.details,
        [
            path.display().to_string(),
            "The file that was already there, if any, has been left as it was.".to_owned()
        ]
    );
}

/// The open project's texts in `locale`.
fn texts(locale: Locale) -> ProjectTexts {
    ProjectTexts::new(&Localizer::new(locale).unwrap()).unwrap()
}

#[test]
fn the_texts_exist_in_six_languages_with_one_name_each() {
    for locale in Locale::ALL {
        let texts = texts(locale);
        let templates: [&NameTemplate; 6] = texts.templates();
        for template in templates {
            assert_eq!(template.slots(0), 1, "{locale:?}: {template:?}");
            let filled = template.fill(&["Song"]);
            assert!(filled.contains("Song"), "{locale:?}");
            assert!(
                !filled.contains(['\u{2068}', '\u{2069}', '\u{E000}']),
                "{locale:?}: {filled}"
            );
        }
        assert_eq!(
            window_title(&texts, Some("Song"), false),
            "Song — Orchestra",
            "{locale:?}"
        );
        assert_eq!(
            window_title(&texts, Some("Song"), true),
            "• Song — Orchestra",
            "{locale:?}"
        );
    }
    let en = texts(Locale::EnUs);
    assert_eq!(window_title(&en, None, false), "Untitled — Orchestra");
    let it = texts(Locale::It);
    assert_eq!(window_title(&it, None, true), "• Senza titolo — Orchestra");
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

/// *Primary* alone.
const PRIMARY: Modifiers = Modifiers {
    primary: true,
    shift: false,
    alt: false,
    secondary: false,
};

#[test]
fn the_window_title_follows_the_project() {
    let scratch = Scratch::new("demo-title");
    let (mut demo, mut list) = demo();
    assert_eq!(demo.window_title(), "Untitled — Orchestra");
    assert_eq!(WindowApp::title(&demo), Some("Untitled — Orchestra"));
    modify(demo.project_mut().unwrap());
    demo.refresh_title();
    assert_eq!(demo.window_title(), "• Untitled — Orchestra");
    // Ctrl+S on a project never saved asks for a file, proposing "Untitled".
    press(&mut demo, &mut list, &Key::Character("s".into()), PRIMARY);
    let request = demo.take_file_request().unwrap();
    assert_eq!(request.kind, FileRequestKind::Save);
    assert_eq!(request.file_name.as_deref(), Some("Untitled.orchestra"));
    demo.file_chosen(FileChoice::Chosen(scratch.join("Song")), &mut list);
    assert_eq!(demo.window_title(), "Song — Orchestra");
    assert!(scratch.join("Song.orchestra").exists());
    // Ctrl+N with nothing to save: a new, untitled project.
    press(&mut demo, &mut list, &Key::Character("n".into()), PRIMARY);
    assert_eq!(demo.window_title(), "Untitled — Orchestra");
    assert_eq!(demo.ui().dialogs().count(), 0);
}

#[test]
fn closing_with_unsaved_changes_asks_and_saving_closes() {
    let scratch = Scratch::new("demo-close");
    let (mut demo, mut list) = demo();
    // Nothing to save: the window closes at once.
    assert!(demo.close_requested(&mut list));
    modify(demo.project_mut().unwrap());
    // Changes: the question opens instead, and a second request is refused without another.
    assert!(!demo.close_requested(&mut list));
    assert_eq!(demo.ui().dialogs().count(), 1);
    assert!(!demo.close_requested(&mut list));
    assert_eq!(demo.ui().dialogs().count(), 1);
    // Escape cancels: the window stays.
    press(
        &mut demo,
        &mut list,
        &Key::Named(NamedKey::Escape),
        Modifiers::default(),
    );
    assert_eq!(demo.ui().dialogs().count(), 0);
    assert_eq!(demo.take_window_request(), None);
    // Quit asks too; Enter answers Save, which asks for a file; once saved, the window closes.
    // (Windows has no Quit shortcut: Alt+F4 is the system's close request.)
    if orchestra_ui::Platform::CURRENT == orchestra_ui::Platform::Windows {
        assert!(!demo.close_requested(&mut list));
    } else {
        press(&mut demo, &mut list, &Key::Character("q".into()), PRIMARY);
    }
    assert_eq!(demo.ui().dialogs().count(), 1);
    press(
        &mut demo,
        &mut list,
        &Key::Named(NamedKey::Enter),
        Modifiers::default(),
    );
    assert_eq!(demo.ui().dialogs().count(), 0);
    assert!(demo.take_file_request().is_some());
    assert_eq!(demo.take_window_request(), None, "not before the save");
    demo.file_chosen(FileChoice::Chosen(scratch.join("Closing")), &mut list);
    assert!(scratch.join("Closing.orchestra").exists());
    assert_eq!(demo.take_window_request(), Some(WindowRequest::Close));
}

#[test]
fn enter_and_escape_never_replace_a_file() {
    let scratch = Scratch::new("demo-replace");
    let existing = scratch.join("Song.orchestra");
    std::fs::write(&existing, b"someone else's file").unwrap();
    let (mut demo, mut list) = demo();
    let shift_primary = Modifiers {
        shift: true,
        ..PRIMARY
    };
    // The focus starts on Cancel (no default button, ADR-0050 C4): Enter presses it.
    for key in [NamedKey::Enter, NamedKey::Escape] {
        press(
            &mut demo,
            &mut list,
            &Key::Character("s".into()),
            shift_primary,
        );
        assert!(demo.take_file_request().is_some());
        demo.file_chosen(FileChoice::Chosen(scratch.join("Song")), &mut list);
        assert_eq!(demo.ui().dialogs().count(), 1, "the question");
        press(&mut demo, &mut list, &Key::Named(key), Modifiers::default());
        assert_eq!(demo.ui().dialogs().count(), 0, "{key:?}");
        assert_eq!(std::fs::read(&existing).unwrap(), b"someone else's file");
        assert_eq!(demo.window_title(), "Untitled — Orchestra");
    }
}
