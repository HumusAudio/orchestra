// SPDX-License-Identifier: GPL-3.0-or-later

//! Guard against user-visible text written directly in the code (CLAUDE.md §7).
//!
//! # What it checks
//! The crates that produce user-visible text (`ui` and `app`) are scanned for string literals
//! that *look like* interface text: they start with an uppercase letter and contain a space
//! between two words, e.g. `"Save project"` or `"Measures selected"`. Such text must be a Fluent
//! key resolved through `orchestra-i18n` instead.
//!
//! # Why a heuristic
//! Proving that a string is never shown to users would need the type information of our UI layer,
//! which does not exist yet (ADR-0012: no toolkit, our own widgets). The heuristic catches the
//! *obvious* cases, which is what the rule asks for, and has an explicit escape hatch: a line
//! containing the marker `i18n-ignore`
//! (e.g. `// i18n-ignore: log message`) is skipped. Developer-facing texts (logs, panics in
//! tests, error `Display` impls) that trigger it must carry that marker and a reason.
//!
//! # Known limits
//! Single-word labels (`"Undo"`) and lower-case text are not detected; raw strings `r"…"` and
//! multi-line literals are not parsed. When our widgets exist, this test should be
//! complemented by making their label APIs accept only message keys (a type-level guarantee).

use std::fs;
use std::path::{Path, PathBuf};

/// Crates whose code may produce user-visible text, relative to the `crates/` directory.
const SCANNED_CRATES: [&str; 2] = ["ui", "app"];

/// Marker that exempts a line from the check.
const IGNORE_MARKER: &str = "i18n-ignore";

/// Returns the string literals on `line` that look like interface text.
///
/// The scan understands `\"` escapes inside literals and ignores everything after a `//`
/// comment that starts outside a literal. Complexity: O(n) in the line length.
fn suspicious_literals(line: &str) -> Vec<String> {
    let trimmed = line.trim_start();
    // Doc comments and ordinary comments are prose for developers, not UI text.
    if trimmed.starts_with("//") || line.contains(IGNORE_MARKER) {
        return Vec::new();
    }
    let mut found = Vec::new();
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            // A comment outside a literal ends the code part of the line.
            '/' if chars.peek() == Some(&'/') => break,
            // A char literal like '"' must not open a string: skip `'x'` and `'\x'` forms.
            '\'' => {
                let mut lookahead = chars.clone();
                let first = lookahead.next();
                let closing = if first == Some('\\') {
                    lookahead.nth(1)
                } else {
                    lookahead.next()
                };
                if closing == Some('\'') {
                    chars = lookahead;
                }
            }
            '"' => {
                let mut literal = String::new();
                while let Some(inner) = chars.next() {
                    match inner {
                        '\\' => {
                            // Keep escaped characters (including `\"`) inside the literal.
                            if let Some(escaped) = chars.next() {
                                literal.push(escaped);
                            }
                        }
                        '"' => break,
                        other => literal.push(other),
                    }
                }
                if looks_like_ui_text(&literal) {
                    found.push(literal);
                }
            }
            _ => {}
        }
    }
    found
}

/// Heuristic for "interface text": starts with an uppercase letter and contains at least two
/// words made of letters (so identifiers, paths, format strings like `"{}"`, keys like
/// `"menu-edit"` and single words are not flagged).
fn looks_like_ui_text(literal: &str) -> bool {
    let starts_upper = literal.chars().next().is_some_and(char::is_uppercase);
    let words = literal
        .split_whitespace()
        .filter(|w| w.chars().filter(|c| c.is_alphabetic()).count() >= 2)
        .count();
    starts_upper && words >= 2
}

/// Recursively collects the `.rs` files under `dir`.
fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

/// The scanned crates contain no obvious hard-coded interface text.
#[test]
fn ui_crates_have_no_hardcoded_text() {
    // `CARGO_MANIFEST_DIR` is `crates/i18n`; its parent is `crates/`. Built with `Path::join`
    // so separators are right on every platform.
    let crates_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates/i18n has a parent directory")
        .to_path_buf();
    let mut problems = Vec::new();
    for name in SCANNED_CRATES {
        let mut files = Vec::new();
        rust_files(&crates_dir.join(name).join("src"), &mut files);
        assert!(!files.is_empty(), "no sources found for crate `{name}`");
        for file in files {
            let text = fs::read_to_string(&file).unwrap();
            // `lines()` accepts both `\n` and `\r\n`, so a CRLF checkout works too.
            for (number, line) in text.lines().enumerate() {
                for literal in suspicious_literals(line) {
                    problems.push(format!(
                        "{}:{}: \"{literal}\" looks like interface text; use a Fluent key \
                         (or add `// {IGNORE_MARKER}: <reason>`)",
                        file.display(),
                        number + 1
                    ));
                }
            }
        }
    }
    assert!(problems.is_empty(), "\n{}", problems.join("\n"));
}

/// The heuristic itself: flags the obvious cases and leaves code-like strings alone.
#[test]
fn heuristic_examples() {
    // `assert_eq!` against an empty array (rather than `assert!(….is_empty())`) prints what was
    // wrongly flagged when the assertion fails.
    const NONE: [&str; 0] = [];
    assert_eq!(
        suspicious_literals(r#"button.label("Save project");"#),
        ["Save project"]
    );
    assert_eq!(
        suspicious_literals(r#"let s = "Measures \"selected\" now";"#),
        [r#"Measures "selected" now"#]
    );
    // Keys, identifiers, format strings, single words, comments and ignored lines pass.
    assert_eq!(
        suspicious_literals(r#"l.format("menu-edit-undo", None)"#),
        NONE
    );
    assert_eq!(suspicious_literals(r#"println!("{} {}", a, b)"#), NONE);
    assert_eq!(suspicious_literals(r#"let w = "Undo";"#), NONE);
    assert_eq!(suspicious_literals(r#"// "This is a comment""#), NONE);
    assert_eq!(
        suspicious_literals(r#"log("Audio device lost"); // i18n-ignore: log"#),
        NONE
    );
    assert_eq!(suspicious_literals(r#"let q = '"'; let k = "key";"#), NONE);
    assert_eq!(
        suspicious_literals(r#"f("a"); // "Not code anymore""#),
        NONE
    );
}
