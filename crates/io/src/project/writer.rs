// SPDX-License-Identifier: GPL-3.0-or-later

//! The layout of the written file: structure indented, **one record per line** (ADR-0029 S3,
//! owner's choice ADR-0029 I9 of 2026-11-06).
//!
//! # The rule
//! A `serde_json` [`Formatter`] that decides, for every object and array it opens, whether to
//! write it **expanded** (each member on its own line, indented by two spaces per level) or
//! **compact** (on one line, no spaces):
//!
//! - everything inside a compact container is compact;
//! - an object that is an element of an array is a **record**, compact, unless the array is one
//!   of the structural lists ([`STRUCTURAL`]: tracks, staves, voices, cells), whose elements hold
//!   records of their own and stay expanded;
//! - every other object and array (the file, `document`, `meter`, an `events` list…) is expanded.
//!
//! So editing a note changes one line of a diff (its event), inserting a bar one line of the bar
//! list, and the file stays close to the size of fully compact JSON (the spike measured the fully
//! indented form at three times that size).
//!
//! # How the formatter knows which list it is in
//! `serde_json` writes an object key through the same string callbacks as any string, between
//! `begin_object_key` and `end_object_key`; the formatter copies the key's text there, so that an
//! array opened right after knows its key.
//!
//! The output ends with a newline (added by the caller) and uses `\n` only: the file is the same
//! bytes on every platform (CLAUDE.md §6).

use std::io::{self, Write};

use serde_json::ser::Formatter;

/// The lists whose elements are structures, not records: their elements stay expanded.
const STRUCTURAL: [&str; 4] = ["tracks", "staves", "voices", "cells"];

/// One open object or array.
#[derive(Debug)]
struct Frame {
    /// Written on one line.
    compact: bool,
    /// An array (otherwise an object).
    array: bool,
    /// For an array: the key it is the value of (empty for none).
    key: String,
    /// Whether a member has been written (to place the closing bracket).
    non_empty: bool,
}

/// The formatter of the project file (see the module documentation).
#[derive(Debug, Default)]
pub(crate) struct RecordFormatter {
    /// The open containers, outermost first.
    stack: Vec<Frame>,
    /// The text of the key being written, while `in_key`.
    key: String,
    /// Whether the string being written is an object key.
    in_key: bool,
}

impl RecordFormatter {
    /// Whether a container opened now is compact (see the rule above). O(1).
    fn opens_compact(&self, is_object: bool) -> bool {
        match self.stack.last() {
            // Inside a compact container, everything is compact.
            Some(parent) if parent.compact => true,
            // An object in a list is a record, unless the list is structural.
            Some(parent) if parent.array => is_object && !STRUCTURAL.contains(&parent.key.as_str()),
            // The root, and every value of a key in an expanded object.
            _ => false,
        }
    }

    /// A newline and the indentation of the current depth. O(depth).
    fn newline<W: ?Sized + Write>(&self, writer: &mut W) -> io::Result<()> {
        writer.write_all(b"\n")?;
        for _ in 0..self.stack.len() {
            writer.write_all(b"  ")?;
        }
        Ok(())
    }

    /// Opens a container: decides its layout, writes its bracket. O(1).
    fn open<W: ?Sized + Write>(
        &mut self,
        writer: &mut W,
        array: bool,
        bracket: &[u8],
    ) -> io::Result<()> {
        let compact = self.opens_compact(!array);
        let key = if array {
            std::mem::take(&mut self.key)
        } else {
            String::new()
        };
        self.stack.push(Frame {
            compact,
            array,
            key,
            non_empty: false,
        });
        writer.write_all(bracket)
    }

    /// Closes a container: a newline before the bracket if it was expanded and not empty.
    /// O(depth).
    fn close<W: ?Sized + Write>(&mut self, writer: &mut W, bracket: &[u8]) -> io::Result<()> {
        let frame = self.stack.pop();
        if frame.is_some_and(|f| !f.compact && f.non_empty) {
            self.newline(writer)?;
        }
        writer.write_all(bracket)
    }

    /// Starts a member (an element or a key): a comma after the first, then a newline if the
    /// container is expanded. O(depth).
    fn member<W: ?Sized + Write>(&mut self, writer: &mut W, first: bool) -> io::Result<()> {
        if !first {
            writer.write_all(b",")?;
        }
        let expanded = match self.stack.last_mut() {
            Some(frame) => {
                frame.non_empty = true;
                !frame.compact
            }
            None => false,
        };
        if expanded {
            self.newline(writer)?;
        }
        Ok(())
    }
}

impl Formatter for RecordFormatter {
    fn begin_array<W: ?Sized + Write>(&mut self, writer: &mut W) -> io::Result<()> {
        self.open(writer, true, b"[")
    }

    fn end_array<W: ?Sized + Write>(&mut self, writer: &mut W) -> io::Result<()> {
        self.close(writer, b"]")
    }

    fn begin_array_value<W: ?Sized + Write>(
        &mut self,
        writer: &mut W,
        first: bool,
    ) -> io::Result<()> {
        self.member(writer, first)
    }

    fn end_array_value<W: ?Sized + Write>(&mut self, _writer: &mut W) -> io::Result<()> {
        Ok(())
    }

    fn begin_object<W: ?Sized + Write>(&mut self, writer: &mut W) -> io::Result<()> {
        self.open(writer, false, b"{")
    }

    fn end_object<W: ?Sized + Write>(&mut self, writer: &mut W) -> io::Result<()> {
        self.close(writer, b"}")
    }

    fn begin_object_key<W: ?Sized + Write>(
        &mut self,
        writer: &mut W,
        first: bool,
    ) -> io::Result<()> {
        self.member(writer, first)?;
        self.in_key = true;
        self.key.clear();
        Ok(())
    }

    fn end_object_key<W: ?Sized + Write>(&mut self, _writer: &mut W) -> io::Result<()> {
        self.in_key = false;
        Ok(())
    }

    fn begin_object_value<W: ?Sized + Write>(&mut self, writer: &mut W) -> io::Result<()> {
        let compact = self.stack.last().is_some_and(|f| f.compact);
        writer.write_all(if compact { b":" } else { b": " })
    }

    fn end_object_value<W: ?Sized + Write>(&mut self, _writer: &mut W) -> io::Result<()> {
        Ok(())
    }

    fn write_string_fragment<W: ?Sized + Write>(
        &mut self,
        writer: &mut W,
        fragment: &str,
    ) -> io::Result<()> {
        // Our keys are plain ASCII names with no escapes, so the fragments are the whole key.
        if self.in_key {
            self.key.push_str(fragment);
        }
        writer.write_all(fragment.as_bytes())
    }
}
