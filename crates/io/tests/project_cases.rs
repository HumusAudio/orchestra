// SPDX-License-Identifier: GPL-3.0-or-later

//! The project file, version 1, end to end (ADR-0029 S3, S5, S8, S12, S13; owner's choices
//! I1–I10 of 2026-11-06).
//!
//! - **Round trips**: the "everything" document, the normal and the heavy score: bytes → document
//!   → bytes gives the same bytes, and the document has the same content, IDs and counter.
//! - **Layout**: deterministic, LF only, one record per line, the version first.
//! - **Refusals**: one case per kind (not a project, newer, compressed, syntax, unknown field,
//!   missing field, non-canonical values, a chord that is also a rest, an invalid document).
//! - **Tolerance**: a BOM and CRLF line endings are accepted.
//! - **Files**: saving over an existing file, no temporary file left, a missing file.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic,
    reason = "test helpers: a failed unwrap, index or panic fails the test"
)]

#[path = "common/everything.rs"]
mod everything;
#[path = "../../core-model/tests/common/score.rs"]
mod score;

use orchestra_core_model::Document;
use orchestra_io::{FORMAT_VERSION, LoadError, from_bytes, load, save, to_bytes};

/// Saves and loads a document, checking that nothing changes: same bytes after a second save,
/// same content, same description (IDs and counter included). Returns the bytes.
fn round_trip(doc: &Document) -> Vec<u8> {
    let bytes = to_bytes(doc).unwrap();
    let back = match from_bytes(&bytes) {
        Ok(back) => back,
        Err(e) => panic!("the saved document did not load: {e}"),
    };
    assert!(back.same_content(doc), "the loaded document differs");
    assert_eq!(back.to_parts(), doc.to_parts(), "the description differs");
    assert_eq!(to_bytes(&back).unwrap(), bytes, "a second save differs");
    bytes
}

/// The text of the "everything" document's file.
fn everything_text() -> String {
    String::from_utf8(to_bytes(&everything::everything()).unwrap()).unwrap()
}

/// Loads a modified text and returns the error.
fn refused(text: &str) -> LoadError {
    match from_bytes(text.as_bytes()) {
        Ok(_) => panic!("a modified file loaded"),
        Err(e) => e,
    }
}

/// Replaces the first occurrence of `from` (which must exist) in `text`.
fn edit(text: &str, from: &str, to: &str) -> String {
    assert!(text.contains(from), "{from:?} not in the file");
    text.replacen(from, to, 1)
}

#[test]
fn everything_round_trips() {
    round_trip(&everything::everything());
}

#[test]
fn the_scores_round_trip() {
    round_trip(&score::score(score::Size::Normal, true));
    round_trip(&score::score(score::Size::Heavy, false));
}

/// The layout of S3 and I9: the version first, LF only, a final newline, one record per line.
#[test]
fn the_layout_is_one_record_per_line() {
    let text = everything_text();
    assert!(text.starts_with(&format!("{{\n  \"orchestra_format\": {FORMAT_VERSION},\n")));
    assert!(!text.contains('\r'));
    assert!(text.ends_with("}\n"));
    // The structural lists are expanded: every cell starts its own block, every list of events
    // opens on its own line.
    assert!(text.lines().any(|l| l.trim() == "\"bar\": 3,"));
    assert!(
        text.lines()
            .filter(|l| l.contains("\"events\""))
            .all(|l| l.trim() == "\"events\": [")
    );
    // Every event is one line: an event's line starts with its ID and holds its notes.
    let event_lines: Vec<&str> = text
        .lines()
        .filter(|l| l.trim_start().starts_with("{\"id\":") && l.contains("\"at\":"))
        .collect();
    assert_ne!(event_lines.len(), 0);
    for line in &event_lines {
        let line = line.trim().trim_end_matches(',');
        assert!(line.starts_with('{') && line.ends_with('}'), "{line}");
    }
    // The values of S6 and I4: pitches, note values, rationals.
    for needle in [
        "\"pitch\":\"E[-1/2]5\"",
        "\"value\":\"4\"",
        "\"rest\":\"bar\"",
        "\"ornament\":\"trill[1]\"",
        "\"ornament\":\"tremolo[3]\"",
        "\"tie\":\"let_ring\"",
        "\"meter\": \"4/4\"",
        "\"tolerance\": \"1/32\"",
        "\"tolerance\":\"25ms\"",
        "\"numbering\": \"same_as_first_lettered\"",
    ] {
        assert!(text.contains(needle), "{needle} missing");
    }
}

/// A BOM and CRLF line endings (a Windows editor or checkout) are accepted.
#[test]
fn a_bom_and_crlf_are_accepted() {
    let text = everything_text().replace('\n', "\r\n");
    let mut bytes = b"\xEF\xBB\xBF".to_vec();
    bytes.extend_from_slice(text.as_bytes());
    let doc = from_bytes(&bytes).unwrap();
    assert!(doc.same_content(&everything::everything()));
}

/// Not a project, a newer format, a compressed container.
#[test]
fn foreign_and_newer_files_are_refused() {
    assert_eq!(refused("hello"), LoadError::NotAProject);
    assert_eq!(refused("{\"written_by\": \"x\"}"), LoadError::NotAProject);
    let text = everything_text();
    assert_eq!(
        refused(&edit(
            &text,
            "\"orchestra_format\": 1",
            "\"orchestra_format\": 2"
        )),
        LoadError::NewerFormat {
            found: Some(2),
            supported: FORMAT_VERSION
        }
    );
    assert_eq!(
        from_bytes(b"PK\x03\x04anything").err(),
        Some(LoadError::NewerFormat {
            found: None,
            supported: FORMAT_VERSION
        })
    );
}

/// Errors of the JSON and of the schema come with their line.
#[test]
fn schema_errors_name_their_place() {
    let text = everything_text();
    // Invalid JSON.
    assert!(matches!(
        refused(&edit(&text, "\"bars\": [", "\"bars\": [,")),
        LoadError::Syntax { .. }
    ));
    // An unknown field, a missing required field, a wrong type, non-canonical values.
    for (from, to) in [
        ("\"pitch\":\"C4\"", "\"pitch\":\"C4\",\"colour\":\"red\""),
        ("\"staff\":16,", "\"staff\":16,\"colour\":\"red\","),
        ("\"bar\": 3,", "\"bar\": 3, \"colour\": 1,"),
        ("\"next_id\"", "\"next_idx\""),
        ("\"closed\":true", "\"closed\":1"),
        ("\"at\":\"1/4\"", "\"at\":\"2/8\""),
        ("\"pitch\":\"C4\"", "\"pitch\":\"B#3x\""),
        ("\"value\":\"4\"", "\"value\":\"quarter\""),
        ("\"meter\": \"4/4\"", "\"meter\": \"4/04\""),
    ] {
        let error = refused(&edit(&text, from, to));
        assert!(
            matches!(error, LoadError::Schema { line: Some(_), .. }),
            "{from} → {to}: {error:?}"
        );
    }
}

/// Content errors name the object: a chord that is also a rest, a meter-map entry on no bar.
#[test]
fn content_errors_name_the_object() {
    let text = everything_text();
    let error = refused(&edit(
        &text,
        "\"value\":\"4\",",
        "\"value\":\"4\",\"rest\":\"4\",",
    ));
    let LoadError::Schema {
        line: None,
        message,
        ..
    } = error
    else {
        panic!("{error:?}");
    };
    assert!(message.starts_with("event "), "{message}");
    let error = refused(&edit(
        &text,
        "\"uncounted\": [\n      ",
        "\"uncounted\": [\n      999999, ",
    ));
    assert!(
        matches!(error, LoadError::Schema { line: None, .. }),
        "{error:?}"
    );
    // Articulations out of the model's order, or repeated; two meter changes at one bar.
    for (from, to) in [
        ("[\"accent\",\"staccato\"]", "[\"staccato\",\"accent\"]"),
        (
            "[\"accent\",\"staccato\"]",
            "[\"accent\",\"accent\",\"staccato\"]",
        ),
        (
            "{\"bar\":9,\"meter\":\"6/8\",\"grouping\":\"3+3/8\"}",
            "{\"bar\":9,\"meter\":\"6/8\",\"grouping\":\"3+3/8\"},{\"bar\":9,\"meter\":\"3/4\"}",
        ),
    ] {
        let error = refused(&edit(&text, from, to));
        assert!(
            matches!(error, LoadError::Schema { line: None, .. }),
            "{from}: {error:?}"
        );
    }
}

/// An invalid document (two events of a voice overlapping) is refused by the model.
#[test]
fn an_invalid_document_is_refused() {
    let text = everything_text();
    // The flute's second note of bar 1 moved onto the first.
    let error = refused(&edit(
        &text,
        "\"at\":\"1/4\",\"staff\"",
        "\"at\":\"1/8\",\"staff\"",
    ));
    assert!(matches!(error, LoadError::Document(_)), "{error:?}");
}

/// Saving over an existing file replaces it, leaves no temporary file, and loads back.
#[test]
fn saving_replaces_the_file_safely() {
    let dir = std::env::temp_dir().join(format!("orchestra-io-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("Song with spaces è.orchestra");
    std::fs::write(&path, b"old content").unwrap();
    let doc = everything::everything();
    save(&doc, &path).unwrap();
    assert!(load(&path).unwrap().same_content(&doc));
    let names: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(names.len(), 1, "a temporary file was left: {names:?}");
    let missing = dir.join("missing.orchestra");
    assert!(matches!(load(&missing), Err(LoadError::Io { .. })));
    std::fs::remove_dir_all(&dir).unwrap();
}
