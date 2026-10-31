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
//! | `check` | Everything CI checks: fmt, clippy, tests (nextest), cargo-deny, documentation parity, guide parity, test-only crates kept out of normal dependencies. Translation parity runs inside the tests. |
//! | `guides` | Builds `dist/GUIDA.pdf` (it) and `dist/GUIDE.pdf` (en-US) with Typst. |
//! | `screenshots` | Placeholder until our UI layer exists (ADR-0012: no toolkit, headless rendering to a pixmap). |
//! | `bench` | Instruction-count benchmarks with Gungraun under Valgrind on Linux; compile-only elsewhere (ADR-0016). With `--compare <rev>`, also the regression gate against the merge base with `<rev>` (ADR-0017). |
//!
//! Every command prints what it runs, stops at the first failure and exits with a non-zero code,
//! so it can be used as-is in CI.

mod bench;
mod parity;

use std::env;
use std::ffi::OsStr;
use std::fmt;
use std::fmt::Write as _;
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
  check        fmt, clippy, tests, cargo-deny, documentation and guide parity,
               test-only crates kept out of normal dependencies
  guides       build dist/GUIDA.pdf and dist/GUIDE.pdf with Typst
  screenshots  (not implemented yet: needs our UI layer, ADR-0012)
  bench        instruction-count benchmarks (Gungraun + Valgrind, Linux);
               on Windows and macOS the benchmarks are only compiled
  bench --compare <rev>
               also measure the merge base of HEAD and <rev> and fail if an
               instruction count grows by more than the limit (ADR-0017)";

/// Program entry point: dispatches the first argument to a task.
fn main() -> ExitCode {
    let task = env::args().nth(1);
    let result = match task.as_deref() {
        Some("setup") => setup(),
        Some("check") => check(),
        Some("guides") => guides(),
        Some("screenshots") => screenshots(),
        // Everything after `bench` is that task's own options (`--compare <rev>`).
        Some("bench") => bench::bench(&env::args().skip(2).collect::<Vec<_>>()),
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
    run_with_env(program, args, &[])
}

/// Like [`run`], but also sets the environment variables `envs` (name, value) for the child
/// process, on top of the inherited environment. The variables are echoed before the command
/// line, as a shell would show them.
///
/// # Errors
/// Same as [`run`].
fn run_with_env<I, S>(program: &str, args: I, envs: &[(&str, &OsStr)]) -> TaskResult
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    run_in(&root(), program, args, envs)
}

/// Like [`run_with_env`], but in the directory `dir` instead of the repository root.
///
/// Needed by the benchmark comparison, which builds an exported copy of an older commit in its
/// own directory (see `bench.rs`).
///
/// # Errors
/// Same as [`run`].
fn run_in<I, S>(dir: &Path, program: &str, args: I, envs: &[(&str, &OsStr)]) -> TaskResult
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let args: Vec<S> = args.into_iter().collect();
    let shown: Vec<String> = args
        .iter()
        .map(|a| a.as_ref().to_string_lossy().into_owned())
        .collect();
    let mut shown_envs = String::new();
    for (name, value) in envs {
        // Writing into a `String` cannot fail: the `fmt::Result` is always `Ok`.
        let _ = write!(shown_envs, "{name}={} ", value.to_string_lossy());
    }
    // The directory is shown only when it is not the repository root, the usual case.
    let shown_dir = if dir == root() {
        String::new()
    } else {
        format!("(in {}) ", dir.display())
    };
    println!("\n▶ {shown_dir}{shown_envs}{program} {}", shown.join(" "));
    let status = Command::new(program)
        .args(&args)
        .envs(envs.iter().copied())
        .current_dir(dir)
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

/// Runs `program` with `args` in the repository root and returns its standard output, trimmed.
///
/// For short queries whose answer the task needs (`git rev-parse`, `git merge-base`, …); unlike
/// [`run`], the command line is not echoed, because the output is not shown either. The
/// program's standard error is inherited, so its own error message reaches the developer.
///
/// # Errors
/// Fails if the program cannot be started, exits with a non-zero status or prints something that
/// is not UTF-8.
fn capture(program: &str, args: &[&str]) -> Result<String, TaskError> {
    let output = Command::new(program)
        .args(args)
        .current_dir(root())
        .stderr(std::process::Stdio::inherit())
        .output()
        .map_err(|e| TaskError(format!("cannot run `{program}`: {e}")))?;
    if !output.status.success() {
        return Err(TaskError(format!(
            "`{program} {}` failed ({})",
            args.join(" "),
            output.status
        )));
    }
    String::from_utf8(output.stdout)
        .map(|s| s.trim().to_owned())
        .map_err(|_| {
            TaskError(format!(
                "`{program} {}` printed non-UTF-8 output",
                args.join(" ")
            ))
        })
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
    check_test_only_crates()?;
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

/// Workspace crates that may appear only in `[dev-dependencies]` (ADR-0016 §6).
///
/// They contain test machinery (for example a counting global allocator) that must never end up
/// in the shipped program.
const TEST_ONLY_CRATES: [&str; 1] = ["orchestra-test-support"];

/// Part of `cargo xtask check`: fails if a crate listed in [`TEST_ONLY_CRATES`] is a normal or
/// build dependency of any workspace crate, on any target platform.
///
/// # How
/// `cargo tree --invert <crate> --edges normal,build --target all` prints the crate followed by
/// everything that depends on it through normal or build edges, one package per line with
/// `--prefix none`. Dev-dependency edges are excluded by `--edges`, so for a correctly used
/// test-only crate the output is exactly one line: the crate itself. Any further line names an
/// offender. `--target all` makes platform-specific dependencies (`[target.'cfg(…)'.dependencies]`)
/// count as well, so a Windows-only misuse is caught on Linux. Cargo resolves this from the
/// lockfile and the manifests: no compilation, it takes well under a second.
///
/// # Errors
/// Fails if `cargo tree` cannot run, or lists the offending dependents.
fn check_test_only_crates() -> TaskResult {
    for krate in TEST_ONLY_CRATES {
        println!("\n▶ {krate} only in [dev-dependencies]");
        let output = Command::new("cargo")
            .args([
                "tree",
                "--workspace",
                "--locked",
                "--edges",
                "normal,build",
                "--target",
                "all",
                "--prefix",
                "none",
                "--invert",
                krate,
            ])
            .current_dir(root())
            .output()
            .map_err(|e| TaskError(format!("cannot run `cargo tree`: {e}")))?;
        if !output.status.success() {
            return Err(TaskError(format!(
                "`cargo tree --invert {krate}` failed ({}):\n{}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            )));
        }
        // `lines()` handles both `\n` and `\r\n` (Windows), so no assumption on line endings.
        // The first line is the crate itself; duplicates are possible when a dependent appears
        // through several paths, and harmless since we only report them.
        let stdout = String::from_utf8_lossy(&output.stdout);
        let dependents: Vec<&str> = stdout
            .lines()
            .skip(1)
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect();
        if !dependents.is_empty() {
            return Err(TaskError(format!(
                "{krate} is test-only (ADR-0016 §6) but is a normal or build dependency of:\n  {}\n\
                 move it to [dev-dependencies]",
                dependents.join("\n  ")
            )));
        }
        println!("  ok");
    }
    Ok(())
}
