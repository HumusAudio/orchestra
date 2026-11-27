# ADR-0054 — Projects read and written on a worker thread, automatic saving, recovery after a crash

- **Status:** Accepted (owner, 2026-11-27; recommendations 1–8 of F2.14 presented in the
  briefing, the owner approved every one; then this text, with the details added while writing
  it, approved as it stands).
- **Date:** 2026-11-27
- **Implements:** SCALETTA F2.14 and decision D13 (automatic saving already in Phase 2A, closed
  2026-11-08); ADR-0050 DG5 (opening and saving "move to a worker thread in that step"; choice 5
  of F2.12a5b: "a worker thread comes with the automatic save"); ADR-0029 S13 ("autosave and crash
  recovery stay in F2.14").
- **Builds on:** ADR-0029 (the project file v1 and its safe saving), ADR-0026 (the editor, the
  revision, gestures as open transactions), ADR-0050 DG1–DG4 and DG6–DG7 (dialogs, the blocking
  layer, the settings file and its directory), ADR-0040 (the window's timed wake-ups and the
  proxy), ADR-0045 (pointer shapes, F2.09e), ADR-0010 (versions, migrations, golden files).
- **Amends:** nothing.
- **Steps:** F2.14a (the worker thread) and F2.14b (automatic saving and recovery), see
  Consequences.

## Context

A musician who loses work stops using the program: that is why D13 brought automatic saving
forward to Phase 2A. Two limits recorded in F2.12a5b belong to the same step:

- **Opening and saving run on the interface's thread.** Measured with `orchestra-io`'s benchmarks:
  about 5.5 ms to save and 11 ms to open the normal reference score, but **168 ms and 396 ms** for
  the heavy one. The window freezes for that long: no redraw, no answer to the compositor.
- **Nothing survives a crash.** The last explicit save is all there is.

What exists, verified on 2026-11-27:

- `Document` is `Clone`. A full copy of it costs about the same as `Document::to_parts` (a copy
  into plain parts, measured in `benches/parts.rs`): **1.23 M instructions** for the normal score
  and **35.7 M** for the heavy one, about 0.3 ms and 10 ms here. Encoding the copy into JSON and
  writing it is what costs most, and that can happen anywhere.
- `orchestra_io::project::to_bytes` and `from_bytes` are pure functions; `save` writes with
  `safe_write` (a temporary file in the same directory, sync, atomic rename, directory sync).
- `crates/app/src/settings/mod.rs` already runs a writer thread: a channel of copies, the latest
  one written, the failures sent back, a synchronous fallback if the system refuses a thread.
- The window has a blocking layer for the file chooser (ADR-0050 DG4, precision 4 of F2.12a5a):
  nothing reaches the widgets under it, and requests to close are ignored while it is up.
- The window wakes at the earliest of the interface's and the settings store's wake-ups
  (precision 2 of F2.12a4a).
- Rust 1.99 (the workspace's `rust-version`) has **`std::fs::File::try_lock`**, stable since
  1.89.0: `flock(LOCK_EX | LOCK_NB)` on Unix, `LockFileEx(LOCKFILE_EXCLUSIVE_LOCK |
  LOCKFILE_FAIL_IMMEDIATELY)` on Windows. The system releases such a lock when the process ends,
  however it ends (a crash, `kill -9`, the power button excepted), because the lock belongs to the
  open file handle. No `unsafe`, no crate.

### What others do (checked on 2026-11-27)

- **MuseScore**: an automatic save every **2 minutes** by default (the default in MuseScore 3's
  `preferences.cpp`; MuseScore 4 keeps the option under *Preferences → Save & publish → Auto
  save*), into a copy beside its own data, not over the score; at the next start after a crash it
  offers to restore the unsaved scores.
- **LibreOffice**: *Save AutoRecovery information every* **10 minutes** by default (15 before
  5.1), a timer that starts when the document is first modified and saves while the program is
  idle; a recovery dialog at the next start. Its separate "autosave over the original" is off by
  default.
- **Cubase / Nuendo**: *Auto Save* writes numbered backup projects in the project folder at a
  chosen interval and keeps a chosen number of them (its default interval not checked here).

Notation programs and office suites write a **recovery copy** in the program's own data, never
over the user's file, and offer it after a crash; nobody makes the automatic save overwrite the
original without being asked.

## Decision

### AR1 — One worker thread reads and writes projects (F2.14a)

- `orchestra-app` gets one **project I/O thread**, started at the first request, with the same
  shape as the settings writer: a channel of requests, executed **one at a time, in order**, and a
  channel of answers that wakes the window through its proxy (ADR-0040 W3). If the system refuses
  the thread, the request runs on the interface's thread as today: slower, nothing lost.
- **Saving** (manual): the interface's thread makes a **snapshot** (`Document::clone`, ≈ 0.3 ms
  normal, ≈ 10 ms heavy) and sends it with the path; the worker encodes it and writes it with
  `safe_write`. The revision of the snapshot is sent along, so that when the answer comes the
  project becomes "saved at revision *r*": edits made meanwhile (an automatic save, AR4) still count
  as modified.
- **Opening**: the worker reads the file and builds the document (`from_bytes`, so the full
  validation of ADR-0029 S8 too) and sends back the document or the typed error; the interface's
  thread only swaps the open project.
- While a **manual** save or an opening runs, the window shows the **blocking layer** of DG4 (the
  same layer: nothing reaches the widgets, the menu bar and the shortcuts are off) and the pointer
  takes a new shape, **`PointerShape::Wait`** (winit's `CursorIcon::Wait`). Requests to close the
  window wait: they are kept and run when the answer comes (the question about unsaved changes
  included), never dropped and never answered twice.
- What waited on a save (New, Open, closing, F2.12a5b's `Then`) runs when the save's answer says
  it succeeded, exactly as today after a synchronous save.
- **Why a snapshot and not the document itself**: the document stays editable on the interface's
  thread (and the automatic save must never stop editing, AR4); a shared document would need a
  lock on every edit, against CLAUDE.md §5's spirit. The copy is the cheapest part of the whole
  operation and is measured by a benchmark.

Rejected: a thread per request (two saves of the same file could cross); `async` with an
executor (a dependency for one thread); encoding on the interface's thread and only writing on
the worker (the encoding is the slow part).

### AR2 — Where recovery copies live: the program's own state directory

| System | Directory |
|---|---|
| Linux and other Unix | `$XDG_STATE_HOME/orchestra/recovery/`, or `$HOME/.local/state/orchestra/recovery/` when it is unset, empty or not absolute |
| Windows | `%LOCALAPPDATA%\Orchestra\Recovery\` |
| macOS | `$HOME/Library/Application Support/Orchestra/Recovery/` |

Our own code in `io`'s `platform/`, beside `config_dir` and with its rules (environment variables
only, `std::path`, testable by value, `None` when nothing is usable). Why these: XDG's *state*
directory is defined for "state data that should persist between application restarts … current
state of the application that can be reused on a restart (… open files, undo history …)" (XDG
Base Directory 0.8); on Windows the **local** application data, not the roaming one, because a
recovery copy belongs to this computer's crash and must not travel with the user's profile; on
macOS Apple's place for an application's own files, as for the settings. With no usable directory
the program runs without automatic saving, says so once (an information dialog at start-up) and
the switch in the Preferences shows it is unavailable.

Rejected: **a copy beside the project** (Word's `~$` files, Cubase's backups): it litters the
user's folders and synchronised folders, fails in read-only folders, and has no place for a
project never saved.

### AR3 — What a recovery copy is: the project file v1 and a small description

Each running process owns one **session directory**, `recovery/<session>/`, where `<session>` is
made of the start time and the process ID (unique on this computer, readable when looking at the
folder). In it:

- `session.lock` — an empty file held with an exclusive `try_lock` for the whole life of the
  process (AR5);
- `project.orchestra` — the open project's **whole document in the project format v1** of
  ADR-0029, written with `safe_write` (so a crash in the middle of an automatic save leaves the
  previous copy intact);
- `recovery.json` — a small description in a **new versioned format** (first field
  `"orchestra_recovery": 1`, then the original file's path or `null` for a project never saved,
  the project's name, the time of the copy in UTC seconds, and the program's version as
  information). It is written **after** the copy, also with `safe_write`, so a description always
  names a complete copy.

The undo history is **not** saved (ADR-0029 S7, D08): a recovered project starts with an empty
history, as an opened one does. The description follows CLAUDE.md §9 like every format: version,
migrations through the shared engine of `migration.rs`, a golden file of version 1, strict
reading (an unknown field is a damaged description, AR6).

Rejected: **a journal of commands** replayed on the last save (the history stores edits, not
commands, ADR-0026 §1; the journal would need its own serialisation of every edit, a second format
to keep in step with the model, and one bad entry would lose everything after it); **only the
description, pointing at the original** (a project never saved has nothing to point at).

### AR4 — When the automatic save runs

- A **timer starts at the first change after the last save** (manual or automatic): the revision
  differs from the revision of the last copy. When it expires, if the project is still modified
  and **no gesture is open** (ADR-0026 §4: a drag in progress is not a state to save; the save
  waits for the gesture to end), the interface's thread makes a snapshot and the worker writes
  the copy. **Editing never stops**: no blocking layer, no pointer shape, nothing on screen.
- Default **every 2 minutes** (MuseScore's default; LibreOffice's 10 minutes risk more work).
- The **Preferences** (a new group in an existing section, its place fixed in F2.14b with the
  texts) have a switch *Save automatically* (on by default) and the minutes, **1 to 60**, saved in
  the settings file (DG6). How the new entries enter the settings file (version 1's lenient
  reading, or version 2 with a migration) is checked against ADR-0050 DG6 while writing F2.14b
  and presented to the owner then.
- A manual save that succeeds **removes the recovery copy** (there is nothing left to recover) and
  resets the timer. So does closing the project with *Don't save*, and opening or creating another
  project.
- The automatic save never writes over the user's file.

### AR5 — How a crash is recognised: a lock held by the system

- At start-up the program creates its session directory and **locks** `session.lock`
  (`File::try_lock`). The lock lives as long as the process; the system drops it when the process
  ends, normally or not.
- On a **normal exit** (the loop ends after the project was closed), the program removes its
  session directory.
- At start-up, after creating its own session, the program looks at the other session
  directories. For each one it tries the lock:
  - **lock taken** → another Orchestra is running (two processes of one user are allowed,
    ADR-0050 DG6): left alone;
  - **lock obtained** → that process ended without cleaning up, so it **crashed** (or was killed):
    if the directory holds a complete copy (a description naming an existing copy), it is offered
    (AR6); if not (it crashed before its first automatic save), the directory is removed silently.
  The lock obtained is kept until the question about that copy is answered, so a second Orchestra
  starting at the same moment cannot offer the same copy twice.
- **Why a lock and not a PID file**: process IDs are reused (a PID file can name a living
  stranger), and asking whether a process is alive needs `unsafe` (`kill(pid, 0)`,
  `OpenProcess`) or a crate. The lock answers the right question, "is the owner still there?",
  with the standard library alone, on the three systems.
- Recorded limits: a recovery directory on a network file system whose locks do not work would
  make every copy look orphaned or never orphaned (the directories of AR2 are local by design);
  on Windows a lock is mandatory, on Unix advisory: only Orchestra opens these files, so both
  behave the same for us.

### AR6 — The question at start-up

When a crashed session's copy is found, after the main window opens and before anything else, the
program asks (one of our questions, DG3):

- title *Recover unsaved changes?*; message *Orchestra closed unexpectedly. Do you want to recover
  the unsaved changes to “Name”?*, with the copy's date and time as a detail;
- buttons **Recover** (default), **Discard** (destructive), **Later** (cancel role: Escape presses
  it). *Later* leaves the copy where it is, unlocked, to be offered at the next start.
- **Recover** opens the copy through the worker (AR1) as the open project, **with the original
  file's path** and **marked modified**: the title shows the mark, and the user's file is written
  only by an explicit *Save*. A project never saved comes back as *Untitled*, modified. The copy
  stays where it is (moved under the new session) until the project is saved or closed, so a
  second crash before saving loses nothing.
- **Discard** removes the copy; *Discard* is destructive and never the default, and since it
  loses work it asks no second question (the first question is already that question).
- If **several** crashed sessions hold copies, the **most recent** is offered; the others stay for
  the next start (one window holds one project).
- A copy that **cannot be read** (a damaged description or project file, a newer format): an
  error dialog naming the reason, with **Discard** and **Later**.
- If the original file was changed **after** the copy (its modification time is later), the
  question's detail says so; recovering still opens the copy, and saving it over the original is
  the user's choice.
- Nothing is ever removed automatically without the user's answer.

### AR7 — When the automatic save fails

A failed automatic save (disk full, permission refused, the directory gone) shows an **error
dialog the first time** (*Your work could not be saved automatically*, with the reason the user
can act on, as in choice 6 of F2.12a5b), and then nothing until an automatic save succeeds again;
after a success, the next failure is reported again. A dialog every two minutes would be worse
than the failure. The project stays modified; manual saving is unaffected.

### AR8 — The developers' log stays for later

The open decision S4 of F2.12a2 (where the developers' details of an error go) is **not** settled
here: the errors of this ADR show the translated reason, as every error so far. A log is a subject
of its own and gets its own step, to be placed in SCALETTA.

### AR9 — Tests and measurements

- `orchestra-io`: the recovery directory per system with made-up environments; the description's
  round trip, golden file, strict reading, truncations never panicking.
- `orchestra-app`, on real files in scratch directories, without a window: the worker's order,
  answers and fallback; the snapshot's revision (an edit during a save leaves the project
  modified); the timer (no save while a gesture is open, none when not modified, a reset by a
  manual save); the lock (a session held by a live handle is left alone, a released one is
  offered); recover, discard, later, several copies, a damaged copy; the removal on a manual save,
  on *Don't save*, on a normal exit; the first-failure-only dialog.
- **A crash for real**: a test starts a child process (the test binary itself, in a mode of its
  own) that makes a session and a copy and is then killed; the parent must find the copy offered.
  This proves AR5 on the three systems in CI.
- Benchmarks (ADR-0017): the snapshot (`Document::clone`) of the normal and heavy scores; the
  instructions spent on the interface's thread by an automatic save. Budget: the interface's part
  of an automatic save of the normal score well under one frame (16 ms); the heavy score's is
  recorded.
- Checks by hand by the owner: a heavy score opened and saved without the window freezing; a real
  crash (the process killed) and the question at the next start.

## Alternatives rejected

Listed with each choice: a thread per request, `async`, encoding on the interface's thread (AR1);
a copy beside the project (AR2); a journal of commands, a description only (AR3); a PID file
(AR5); overwriting the user's file automatically (AR4, as nobody does); a dialog at every failure
(AR7); the log now (AR8).

## Consequences

- SCALETTA: F2.14 is split in **F2.14a** — AR1: the worker thread, snapshots, opening and manual
  saving off the interface's thread, the blocking layer and `PointerShape::Wait`, the benchmark of
  the snapshot — and **F2.14b** — AR2–AR7: the recovery directory, the session and its lock, the
  description format, the timer, the Preferences' entries, the question at start-up, the failure
  dialog, the guides' part on automatic saving and recovery.
- `orchestra-io` gains a fourth file format (the recovery description) with the obligations of
  every format, and a second directory function in `platform/`.
- `orchestra-ui` gains `PointerShape::Wait`.
- New texts in the six catalogues, with terminology research (*save automatically*, *recover*,
  *discard*, *later*, *closed unexpectedly*, the failure's message).
- The limits of F2.12a5b about the interface's thread are lifted; the one about "modified after
  change and undo" (F2.22) stays.
- Recorded limits: one recovery copy per process (one window, one project); the undo history does
  not survive a crash; locks on network file systems (AR5).

## Precisions of the implementation (F2.14a, approved by the owner on 2026-11-27)

- **AP1, the snapshot costs much less than estimated.** Measured (`orchestra-core-model`'s
  `benches/parts.rs`, `snapshot`): `Document::clone` costs **0.51 M instructions** for the normal
  score and **2.24 M** for the heavy one, not the 1.2 M and 36 M of `to_parts` assumed in the
  Context. The voices' content lives in shared, copy-on-write chunks (ADR-0022 §7), so the copy
  shares them and an edit afterwards copies only the chunk it touches. The budget of AR9 is met
  with a wide margin, also for the heavy score.
- **AP2, how the window is woken: `WindowWaker`** (a precision of ADR-0040 W3). A new public type
  of `orchestra-ui`, `Clone + Send`, wrapping the event loop's proxy; the window hands it to the
  app once with a new `WindowApp::attach_waker` (default: dropped), before the first layout.
  `wake()` sends an empty event (`ShellEvent::Wake`): the batch it starts calls
  `WindowApp::advance` first, where the app takes its worker's answers, and then the window takes
  the app's requests, pointer and damage as after any event of the app. The answers stay in the
  app's own channel: carrying them in the window's event type would make `orchestra-ui` know the
  app's types.
- **AP3, the busy layer** (a precision of ADR-0045, beside the file chooser's of ADR-0050 DG4).
  `Ui::begin_busy`, `Ui::end_busy`, `Ui::is_busy`: the chooser's empty modal layer, plus the wait
  pointer over the whole window and the window's accessibility node marked **busy** (AccessKit's
  `busy`, ARIA's `aria-busy`); not listed among `Ui::dialogs` nor as a dialog for assistive
  technologies. `end_busy` finds again what is under the pointer, so the shape is right at once.
- **AP4, `PointerShape::Wait`** becomes `winit`'s `CursorIcon::Wait`; on macOS `winit` 0.30.13 gives
  AppKit's "busy but clickable" cursor, the closest the system has.
- **AP5, a job that panics** (a bug) is caught by the worker (`catch_unwind`; the workspace builds
  with unwinding) and answered `JobAnswer::Stopped`, which the flow reports as
  `ProjectError::Stopped`: the operation's title and the existing message "This is a mistake in
  Orchestra: please report it.", so no new text; the interface is never left blocked, and the
  window, whose close waits for the answer, can always close.
- **AP6, what a waiting close does after the answer**: it runs only if the job succeeded and the
  flow is idle (closing, or asking if the project changed meanwhile); after a failure the error is
  shown and the close is dropped, and the user decides again.
- **AP7, the module layout**: `crates/app/src/project.rs` became `project/mod.rs` with
  `project/worker.rs` (`ProjectWorker`); the flow's new public items are `Job`, `JobAnswer`,
  `Step::Writing`, `Step::Reading`, `ProjectEffect::Run`, `ProjectError::Stopped`,
  `ProjectFlow::finished`, `is_busy`, `close_waiting`, `OpenProject::save_job` and `saved_at`
  (`save_to` is gone).
