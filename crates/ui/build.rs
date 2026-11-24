// SPDX-License-Identifier: GPL-3.0-or-later

//! The build script of `orchestra-ui`: converts the Orchestra logo into paths for the About
//! dialog (ADR-0050 DG10, SCALETTA F2.12a3b, owner's choice 1).
//!
//! # What it does
//! Reads `assets/logo/orchestra-logo.svg`, converts it with our own small converter
//! (`build/logo_svg.rs`, also tested by the library's tests) and writes the result as Rust into
//! `OUT_DIR/logo_paths.rs`, which `src/widget/logo.rs` includes.
//!
//! # Why at build time, into `OUT_DIR`
//! - No cost when the program runs: the paths are a static table in the binary, nothing is parsed.
//! - No generated file in the repository: the logo is "all rights reserved", not GPL
//!   (`assets/logo/README.md`), so its shape stays only in `assets/logo/`, never in a source file
//!   that carries the GPL's SPDX header (CLAUDE.md §10).
//! - Always in step with the asset: Cargo runs the script again when the logo or the converter
//!   changes (the `rerun-if-changed` lines), and never otherwise.
//!
//! # Cost
//! The script has no dependencies; compiling it and converting the 52 KB logo add well under a
//! second to a clean build of the crate, and nothing to an incremental one.
//!
//! # Failure
//! A logo the converter cannot read, or whose shapes leave its canvas, fails the build with an
//! English message for the developer (never a user: a release is built from a working tree).

#![allow(
    clippy::print_stdout,
    reason = "a build script talks to Cargo through its standard output (`cargo::` lines)"
)]

use std::path::{Path, PathBuf};

/// The converter, shared with the library's tests.
#[path = "build/logo_svg.rs"]
mod logo_svg;

/// Converts the logo and writes `OUT_DIR/logo_paths.rs`.
///
/// # Errors
/// An English message when the logo cannot be read, converted or written.
fn main() -> Result<(), String> {
    let manifest = std::env::var_os("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .ok_or("CARGO_MANIFEST_DIR is not set (not run by Cargo?)")?;
    let out_dir = std::env::var_os("OUT_DIR")
        .map(PathBuf::from)
        .ok_or("OUT_DIR is not set (not run by Cargo?)")?;
    // The crate lives at `crates/ui`, the asset at the workspace's `assets/logo/`.
    let svg_path: PathBuf = [
        manifest.as_path(),
        Path::new(".."),
        Path::new(".."),
        Path::new("assets"),
        Path::new("logo"),
        Path::new("orchestra-logo.svg"),
    ]
    .iter()
    .collect();
    let converter: PathBuf = [
        manifest.as_path(),
        Path::new("build"),
        Path::new("logo_svg.rs"),
    ]
    .iter()
    .collect();
    println!("cargo::rerun-if-changed={}", svg_path.display());
    println!("cargo::rerun-if-changed={}", converter.display());
    println!("cargo::rerun-if-changed=build.rs");

    let svg = std::fs::read_to_string(&svg_path)
        .map_err(|error| format!("cannot read {}: {error}", svg_path.display()))?;
    let logo = logo_svg::parse(&svg).map_err(|error| error.to_string())?;
    // Every point inside the square canvas: a shape outside it would be cut off in the dialog,
    // which is a change of the logo's composition (assets/logo/README.md rule 3).
    let inside = logo.bounds().is_some_and(|(x0, y0, x1, y1)| {
        x0 >= 0.0 && y0 >= 0.0 && x1 <= logo.side && y1 <= logo.side
    });
    if !inside {
        return Err("the logo's shapes leave its square canvas".to_owned());
    }
    let out = out_dir.join("logo_paths.rs");
    std::fs::write(&out, logo_svg::to_rust(&logo))
        .map_err(|error| format!("cannot write {}: {error}", out.display()))
}
