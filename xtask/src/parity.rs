// SPDX-License-Identifier: GPL-3.0-or-later

//! Structural parity checks between the Italian and English documents (ADR-0009).
//!
//! Two documents in different languages cannot be compared word by word, but their *structure*
//! can: the same headings at the same levels, in the same order, and the same code blocks with
//! identical content. If an author adds a section or a code snippet to one language only, the
//! structures differ and the check fails, pointing at the first divergence.
//!
//! - Developer documentation: `DOCUMENTAZIONE.md` (it) ⇄ `DOCUMENTATION.md` (en-US). The
//!   signature is exactly the one computed by the Claude Code Stop hook
//!   (`scripts/check-docs-updated.sh`), so the hook and `cargo xtask check` can never disagree.
//! - User guides: `docs/guide/it/` ⇄ `docs/guide/en/` must contain the same files, and each pair
//!   of Typst files the same heading levels, `#include`s and screenshots.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Checks that `DOCUMENTAZIONE.md` and `DOCUMENTATION.md` have the same structure.
///
/// # Errors
/// A message naming the first structural difference, or a missing/unreadable file.
pub(crate) fn check_docs(root: &Path) -> Result<(), String> {
    let it = read(&root.join("DOCUMENTAZIONE.md"))?;
    let en = read(&root.join("DOCUMENTATION.md"))?;
    compare(
        "DOCUMENTAZIONE.md",
        &markdown_signature(&it),
        "DOCUMENTATION.md",
        &markdown_signature(&en),
    )
}

/// Checks that the two guide source trees have the same files and structure.
///
/// # Errors
/// A message listing files present in only one tree, or the first structural difference in a
/// pair of `.typ` files.
pub(crate) fn check_guides(root: &Path) -> Result<(), String> {
    let it_dir = root.join("docs").join("guide").join("it");
    let en_dir = root.join("docs").join("guide").join("en");
    let it_files = relative_files(&it_dir)?;
    let en_files = relative_files(&en_dir)?;
    if it_files != en_files {
        let only_it: Vec<_> = it_files.difference(&en_files).collect();
        let only_en: Vec<_> = en_files.difference(&it_files).collect();
        return Err(format!(
            "the guide trees differ: only in it/: {only_it:?}; only in en/: {only_en:?}"
        ));
    }
    for relative in &it_files {
        if relative.extension().is_some_and(|e| e == "typ") {
            let it = read(&it_dir.join(relative))?;
            let en = read(&en_dir.join(relative))?;
            compare(
                &format!("it/{}", relative.display()),
                &typst_signature(&it),
                &format!("en/{}", relative.display()),
                &typst_signature(&en),
            )?;
        }
    }
    Ok(())
}

/// Reads a UTF-8 file, with the path in the error message.
fn read(path: &Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|e| format!("cannot read {}: {e}", path.display()))
}

/// Structural signature of a Markdown document, one token per line:
/// - `FENCE` for every line starting with three backticks (opening or closing a code block);
/// - the verbatim line for every line inside a code block;
/// - `H<n>` for every heading line (`#` × n followed by a space) outside code blocks.
///
/// This mirrors the `awk` program of `scripts/check-docs-updated.sh` line for line. `lines()`
/// strips `\r`, so CRLF and LF files give the same signature (the awk version sees `\r` too, but
/// `.gitattributes` keeps the repository LF). Complexity: O(n) in the file size.
fn markdown_signature(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut in_fence = false;
    for line in text.lines() {
        if line.starts_with("```") {
            in_fence = !in_fence;
            tokens.push("FENCE".to_owned());
        } else if in_fence {
            tokens.push(line.to_owned());
        } else if let Some(level) = heading_level(line, '#') {
            tokens.push(format!("H{level}"));
        }
    }
    tokens
}

/// Structural signature of a Typst file: heading levels (`=` × n + space), `#include` targets and
/// screenshots (`screenshot(` / `screenshot-placeholder(` calls, with the image path when it is on
/// the same line), ignoring raw blocks. Comments are ignored, so each language can annotate its
/// file freely.
fn typst_signature(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut in_raw = false;
    for line in text.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") {
            in_raw = !in_raw;
            tokens.push("RAW".to_owned());
            continue;
        }
        if in_raw || trimmed.starts_with("//") {
            continue;
        }
        if let Some(level) = heading_level(trimmed, '=') {
            tokens.push(format!("H{level}"));
        } else if trimmed.starts_with("#include") {
            tokens.push(trimmed.to_owned());
        } else if let Some(call) = trimmed
            .strip_prefix('#')
            .filter(|c| c.starts_with("screenshot"))
        {
            // Only a call at the start of a line counts (`#screenshot(…)`,
            // `#screenshot-placeholder(…)`), never the word "screenshot" in the prose. Keep the
            // call name and, for real screenshots, the quoted path: both guides must show the
            // same images in the same places (only captions are translated).
            let name: String = call
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '-')
                .collect();
            let path = call.split('"').nth(1).filter(|p| p.starts_with('/'));
            tokens.push(format!("SHOT {name} {}", path.unwrap_or("")));
        }
    }
    tokens
}

/// Returns `Some(n)` if `line` starts with `n` repetitions of `marker` followed by a space.
fn heading_level(line: &str, marker: char) -> Option<usize> {
    let level = line.chars().take_while(|c| *c == marker).count();
    (level > 0 && line[level..].starts_with(' ')).then_some(level)
}

/// Compares two signatures and describes the first difference.
fn compare(a_name: &str, a: &[String], b_name: &str, b: &[String]) -> Result<(), String> {
    if a == b {
        return Ok(());
    }
    // Position of the first differing token (or the end of the shorter signature).
    let index = a
        .iter()
        .zip(b)
        .position(|(x, y)| x != y)
        .unwrap_or_else(|| a.len().min(b.len()));
    let show = |s: &[String]| {
        s.get(index)
            .map_or("<end of file>", String::as_str)
            .to_owned()
    };
    Err(format!(
        "{a_name} and {b_name} have a different structure: element #{} is `{}` in {a_name} but \
         `{}` in {b_name} (headings: same levels in the same order; code blocks: identical)",
        index + 1,
        show(a),
        show(b),
    ))
}

/// Lists the files under `dir` as paths relative to it, recursively, sorted.
fn relative_files(dir: &Path) -> Result<BTreeSet<PathBuf>, String> {
    /// Recursive walk; `base` stays the top directory so paths can be made relative.
    fn walk(base: &Path, dir: &Path, out: &mut BTreeSet<PathBuf>) -> Result<(), String> {
        let entries =
            fs::read_dir(dir).map_err(|e| format!("cannot list {}: {e}", dir.display()))?;
        for entry in entries {
            let path = entry
                .map_err(|e| format!("cannot list {}: {e}", dir.display()))?
                .path();
            if path.is_dir() {
                walk(base, &path, out)?;
            } else if let Ok(relative) = path.strip_prefix(base) {
                out.insert(relative.to_path_buf());
            }
        }
        Ok(())
    }
    let mut files = BTreeSet::new();
    walk(dir, dir, &mut files)?;
    Ok(files)
}

#[cfg(test)]
mod tests {
    //! Unit tests of the signatures and of the comparison messages.

    use super::*;

    /// Headings and code blocks count; prose does not.
    #[test]
    fn markdown_signature_ignores_prose() {
        let it = "# Titolo\nTesto.\n## Parte\n```rust\nlet x = 1;\n```\n";
        let en = "# Title\nSome text.\n\nMore.\n## Part\n```rust\nlet x = 1;\n```\n";
        assert_eq!(markdown_signature(it), markdown_signature(en));
    }

    /// A `#` inside a code block is code, not a heading; changed code is a difference.
    #[test]
    fn code_blocks_must_be_identical() {
        let a = "```sh\n# comment\n```\n";
        let b = "```sh\n# commento\n```\n";
        assert_eq!(markdown_signature(a), ["FENCE", "# comment", "FENCE"]);
        assert!(compare("a", &markdown_signature(a), "b", &markdown_signature(b)).is_err());
    }

    /// A missing section is reported with its position.
    #[test]
    fn missing_heading_is_reported() {
        let a = markdown_signature("# A\n## B\n");
        let b = markdown_signature("# A\n");
        let error = compare("it", &a, "en", &b).unwrap_err();
        assert!(error.contains("element #2"), "{error}");
        assert!(error.contains("<end of file>"), "{error}");
    }

    /// Typst: headings, includes and screenshots form the structure; captions do not.
    #[test]
    fn typst_signature_tracks_structure() {
        let it = "// commento\n= Introduzione\n#include \"a.typ\"\n#screenshot(\"/x.png\", [Didascalia])\n";
        let en = "= Introduction\nSee the screenshots below.\n#include \"a.typ\"\n#screenshot(\"/x.png\", [Caption])\n";
        assert_eq!(typst_signature(it), typst_signature(en));
        assert_eq!(
            typst_signature(en),
            ["H1", "#include \"a.typ\"", "SHOT screenshot /x.png"]
        );
    }

    /// `#` without a following space (e.g. `#![attr]`) is not a heading.
    #[test]
    fn heading_needs_a_space() {
        assert_eq!(heading_level("## Title", '#'), Some(2));
        assert_eq!(heading_level("#!/bin/sh", '#'), None);
        assert_eq!(heading_level("== Sezione", '='), Some(2));
    }
}
