// SPDX-License-Identifier: GPL-3.0-or-later

//! # xtask — project automation
//!
//! Entry point of `cargo xtask <command>` (alias defined in `.cargo/config.toml`). Written in Rust
//! rather than shell so that it behaves identically on Linux, Windows and macOS, and kept
//! dependency-free (std only) so that it compiles in seconds on every CI runner.
//!
//! | Command | What it does |
//! |---|---|
//! | `setup` | Points git at the versioned hooks (`git config core.hooksPath .githooks`). |
//! | `check` | Everything CI checks: fmt, clippy, tests (nextest), cargo-deny, documentation parity, guide parity. Translation parity runs inside the tests. |
//! | `guides` | Builds `dist/GUIDA.pdf` (it) and `dist/GUIDE.pdf` (en-US) with Typst. |
//! | `screenshots` | Placeholder until our UI layer exists (ADR-0012: no toolkit, headless rendering to a pixmap). |
//! | `bench` | Placeholder until the first hot paths exist (ADR-0007). |
//!
//! Every command prints what it runs, stops at the first failure and exits with a non-zero code,
//! so it can be used as-is in CI.

mod parity;

use std::env;
use std::ffi::OsStr;
use std::fmt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

/// Error of an xtask command: a human-readable message for the developer running it.
///
/// A plain `String` wrapper is enough: xtask only reports errors and exits, it never inspects
/// them programmatically.
#[derive(Debug)]
struct TaskError(String);

impl fmt::Display for TaskError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Result type of every task.
type TaskResult = Result<(), TaskError>;

/// Usage text printed by `cargo xtask` without arguments or with an unknown command.
const USAGE: &str = "\
usage: cargo xtask <command>

commands:
  setup        configure git to use the versioned hooks in .githooks/
  check        fmt, clippy, tests, cargo-deny, documentation and guide parity
  guides       build dist/GUIDA.pdf and dist/GUIDE.pdf with Typst
  screenshots  (not implemented yet: needs our UI layer, ADR-0012)
  bench        (not implemented yet: no hot paths yet, ADR-0007)";

/// Program entry point: dispatches the first argument to a task.
fn main() -> ExitCode {
    let task = env::args().nth(1);
    let result = match task.as_deref() {
        Some("setup") => setup(),
        Some("check") => check(),
        Some("guides") => guides(),
        Some("screenshots") => screenshots(),
        Some("bench") => bench(),
        Some("help" | "--help" | "-h") => {
            println!("{USAGE}");
            Ok(())
        }
        Some(other) => Err(TaskError(format!("unknown command `{other}`\n\n{USAGE}"))),
        None => Err(TaskError(USAGE.to_owned())),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("xtask: {e}");
            ExitCode::FAILURE
        }
    }
}

/// The repository root: the parent of this crate's directory (`<root>/xtask`).
///
/// Derived from `CARGO_MANIFEST_DIR` at compile time, so `cargo xtask` works from any
/// subdirectory of the repository.
fn root() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest.parent().unwrap_or(manifest).to_path_buf()
}

/// Runs `program` with `args` in the repository root, echoing the command line.
///
/// # Errors
/// Fails if the program cannot be started (with a hint about installing it) or exits with a
/// non-zero status.
fn run<I, S>(program: &str, args: I) -> TaskResult
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let args: Vec<S> = args.into_iter().collect();
    let shown: Vec<String> = args
        .iter()
        .map(|a| a.as_ref().to_string_lossy().into_owned())
        .collect();
    println!("\n▶ {program} {}", shown.join(" "));
    let status = Command::new(program)
        .args(&args)
        .current_dir(root())
        .status()
        .map_err(|e| {
            TaskError(format!(
                "cannot run `{program}`: {e} (is it installed and on PATH? see DOCUMENTATION.md)"
            ))
        })?;
    if status.success() {
        Ok(())
    } else {
        Err(TaskError(format!(
            "`{program} {}` failed ({status})",
            shown.join(" ")
        )))
    }
}

/// `cargo xtask setup`: makes git use the hooks versioned in `.githooks/`.
///
/// Git does not version `.git/config`, so every clone needs this once. The `commit-msg` hook
/// enforces the `User-visible` trailer (ADR-0009).
fn setup() -> TaskResult {
    run("git", ["config", "core.hooksPath", ".githooks"])?;
    println!("\ngit hooks enabled from .githooks/");
    Ok(())
}

/// `cargo xtask check`: everything that must pass before a commit is pushed and that CI runs.
///
/// Order: cheapest and most common failures first (formatting, then the pure-file parity checks,
/// then lints, tests and the dependency audit), so feedback is fast.
fn check() -> TaskResult {
    run("cargo", ["fmt", "--all", "--check"])?;

    println!("\n▶ documentation parity (DOCUMENTAZIONE.md ⇄ DOCUMENTATION.md)");
    parity::check_docs(&root()).map_err(TaskError)?;
    println!("  ok");

    println!("\n▶ guide parity (docs/guide/it ⇄ docs/guide/en)");
    parity::check_guides(&root()).map_err(TaskError)?;
    println!("  ok");

    run(
        "cargo",
        [
            "clippy",
            "--workspace",
            "--all-targets",
            "--locked",
            "--",
            "-D",
            "warnings",
        ],
    )?;
    // nextest does not run doctests: run them separately so that documentation examples stay
    // correct. Translation parity lives in `orchestra-i18n`'s tests and runs here.
    run("cargo", ["nextest", "run", "--workspace", "--locked"])?;
    run("cargo", ["test", "--workspace", "--doc", "--locked"])?;
    run("cargo", ["deny", "--locked", "check"])?;
    println!("\nall checks passed");
    Ok(())
}

/// `cargo xtask guides`: builds both user guides into `dist/`.
///
/// Typst is run with the repository as root (so chapters can use absolute paths like
/// `/docs/guide/template.typ`) and with `--ignore-system-fonts`, so only the fonts bundled in
/// `assets/fonts` are used and the PDFs are identical on every machine (ADR-0009).
fn guides() -> TaskResult {
    let root = root();
    let dist = root.join("dist");
    std::fs::create_dir_all(&dist)
        .map_err(|e| TaskError(format!("cannot create {}: {e}", dist.display())))?;
    for (language, output) in [("it", "GUIDA.pdf"), ("en", "GUIDE.pdf")] {
        let source = root
            .join("docs")
            .join("guide")
            .join(language)
            .join("main.typ");
        let out = dist.join(output);
        run(
            "typst",
            [
                OsStr::new("compile"),
                OsStr::new("--root"),
                root.as_os_str(),
                OsStr::new("--ignore-system-fonts"),
                OsStr::new("--font-path"),
                root.join("assets").join("fonts").as_os_str(),
                source.as_os_str(),
                out.as_os_str(),
            ],
        )?;
    }
    println!("\nguides written to {}", dist.display());
    Ok(())
}

/// `cargo xtask screenshots`: not implemented yet.
///
/// Screenshots for the guides are rendered headless by the application itself, always in en-US
/// (ADR-0009). That needs our own UI layer (ADR-0012: no toolkit), which will render the interface
/// into an off-screen pixmap with the same renderer it uses on screen.
/// It fails rather than silently succeeding, so nobody mistakes it for a working step.
fn screenshots() -> TaskResult {
    Err(TaskError(
        "screenshots: not implemented yet — headless rendering arrives with our UI layer \
         (ADR-0012); guides use screenshot placeholders until then"
            .to_owned(),
    ))
}

/// `cargo xtask bench`: not implemented yet.
///
/// Benchmarks (preferably deterministic: instruction or allocation counts) will be added with the
/// first hot paths, starting with `orchestra-core-time` conversions in Phase 1 (ADR-0007).
fn bench() -> TaskResult {
    Err(TaskError(
        "bench: not implemented yet — the first benchmarks arrive with core-time in Phase 1 \
         (ADR-0007)"
            .to_owned(),
    ))
}
