// SPDX-License-Identifier: GPL-3.0-or-later

//! # orchestra-screenshots — the guides' screenshots, rendered headless
//!
//! A developer tool, never distributed to users (ADR-0041 S1). `cargo xtask screenshots` runs it
//! as `orchestra-screenshots <OUT_DIR>`: it renders every scene of [`orchestra_app::SCENES`] with
//! the program's own views and renderer, and writes `<OUT_DIR>/<name>.png` for each.
//!
//! No window, no display and no system setting is involved (CLAUDE.md §3): the theme, the scale
//! and the language are fixed by the catalogue (ADR-0041 S3), so the files depend only on the code.
//! The messages are English, for developers, like `xtask`'s.

#![forbid(unsafe_code)]
#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "a command-line tool: reporting to the terminal is its job"
)]

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use orchestra_app::{SCENES, render};
use orchestra_ui::CpuRenderer;

/// Usage line printed for wrong arguments.
const USAGE: &str = "usage: orchestra-screenshots <OUT_DIR>";

/// Entry point: one argument, the output directory (created if missing).
fn main() -> ExitCode {
    let mut args = std::env::args_os().skip(1);
    let (Some(out_dir), None) = (args.next(), args.next()) else {
        eprintln!("{USAGE}");
        return ExitCode::FAILURE;
    };
    match write_all(Path::new(&out_dir)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("orchestra-screenshots: {message}");
            ExitCode::FAILURE
        }
    }
}

/// Renders every scene and writes its PNG into `out_dir`, stopping at the first failure.
///
/// # Why a plain `fs::write`
/// The files are generated output in an ignored directory (`dist/`), rebuilt on every run: a
/// half-written file after a crash is simply written again next time, so the atomic save of
/// `orchestra-io` (meant for the user's projects) would add nothing.
///
/// # Errors
/// A message naming the scene or the file that failed.
fn write_all(out_dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(out_dir)
        .map_err(|e| format!("cannot create {}: {e}", out_dir.display()))?;
    // One renderer for the whole catalogue: it reuses its scratch buffers and its glyph cache
    // between scenes.
    let faces = orchestra_text::Faces::bundled().map_err(|e| e.to_string())?;
    let mut renderer = CpuRenderer::new(faces);
    for scene in SCENES {
        let image = render(scene, &mut renderer).map_err(|e| e.to_string())?;
        let png = image
            .encode_png()
            .map_err(|e| format!("scene `{}`: {e}", scene.name))?;
        let path: PathBuf = out_dir.join(format!("{}.png", scene.name));
        std::fs::write(&path, &png).map_err(|e| format!("cannot write {}: {e}", path.display()))?;
        println!(
            "{} ({} × {} px, {} bytes)",
            path.display(),
            image.width(),
            image.height(),
            png.len()
        );
    }
    Ok(())
}
