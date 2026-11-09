// SPDX-License-Identifier: GPL-3.0-or-later

//! Reading the format version **before** the rest of the file (ADR-0010 §1, ADR-0029 S5).
//!
//! # Why a scanner of its own
//! The version decides how the rest is read (straight into the current schema, through
//! migrations, or not at all), so it must be known before any parsing that could fail on a field
//! of another version. The writer always puts `"orchestra_format": N` first; this scanner reads
//! exactly that prefix (whitespace, `{`, the key, `:`, an integer) and nothing more.
//!
//! # Compressed containers
//! A later version may put the document in a zip or gzip container (ADR-0029 S2). The version-1
//! reader already recognises their signatures and answers "made with a newer Orchestra" instead
//! of "not a project" (owner's precision of ADR-0029 S5, 2026-11-06). The check costs a comparison
//! of four bytes.

use super::LoadError;

/// The signature of a zip archive (a local file header).
const ZIP: &[u8] = b"PK\x03\x04";
/// The signature of a gzip stream.
const GZIP: &[u8] = b"\x1F\x8B";
/// The UTF-8 byte order mark, which some editors add and the reader ignores (ADR-0029 S3).
pub(crate) const BOM: &[u8] = b"\xEF\xBB\xBF";
/// The key that must come first.
const KEY: &[u8] = b"\"orchestra_format\"";

/// The format version a file declares, read from its first bytes (after an optional BOM).
///
/// # Errors
/// [`LoadError::NewerFormat`] with `found: None` for a zip or gzip container;
/// [`LoadError::NotAProject`] if the prefix is not `{ "orchestra_format": <integer>`, or the
/// integer is 0 or does not fit 64 bits.
///
/// # Complexity
/// O(prefix): at most a few dozen bytes are looked at.
pub(crate) fn version(bytes: &[u8]) -> Result<u64, LoadError> {
    if bytes.starts_with(ZIP) || bytes.starts_with(GZIP) {
        return Err(LoadError::NewerFormat {
            found: None,
            supported: super::FORMAT_VERSION,
        });
    }
    let rest = bytes.strip_prefix(BOM).unwrap_or(bytes);
    let rest = after_whitespace(rest);
    let rest = rest.strip_prefix(b"{").ok_or(LoadError::NotAProject)?;
    let rest = after_whitespace(rest);
    let rest = rest.strip_prefix(KEY).ok_or(LoadError::NotAProject)?;
    let rest = after_whitespace(rest);
    let rest = rest.strip_prefix(b":").ok_or(LoadError::NotAProject)?;
    let rest = after_whitespace(rest);
    let digits = rest.iter().take_while(|c| c.is_ascii_digit()).count();
    let mut value: u64 = 0;
    for &d in rest.get(..digits).unwrap_or(&[]) {
        value = value
            .checked_mul(10)
            .and_then(|v| v.checked_add(u64::from(d.wrapping_sub(b'0'))))
            .ok_or(LoadError::NotAProject)?;
    }
    if digits == 0 || value == 0 {
        return Err(LoadError::NotAProject);
    }
    Ok(value)
}

/// The bytes after the leading ASCII whitespace (JSON's whitespace is a subset). O(whitespace).
fn after_whitespace(bytes: &[u8]) -> &[u8] {
    let n = bytes.iter().take_while(|c| c.is_ascii_whitespace()).count();
    bytes.get(n..).unwrap_or(&[])
}

#[cfg(test)]
mod tests {
    //! The prefix scanner on every shape it must accept or refuse.
    use super::*;

    /// The versions written by the program and by hand-formatted files.
    #[test]
    fn versions_are_read() {
        assert_eq!(version(b"{\"orchestra_format\": 1,").ok(), Some(1));
        assert_eq!(
            version(b"\xEF\xBB\xBF{ \n \"orchestra_format\" :\t7}").ok(),
            Some(7)
        );
        assert_eq!(
            version(b"\r\n{\"orchestra_format\":12345").ok(),
            Some(12345)
        );
    }

    /// Everything else is not a project, or a newer one in a container.
    #[test]
    fn other_files_are_refused() {
        for bytes in [
            &b""[..],
            b"[",
            b"{\"format\": 1}",
            b"{\"written_by\": \"x\", \"orchestra_format\": 1}",
            b"{\"orchestra_format\": 0}",
            b"{\"orchestra_format\": \"1\"}",
            b"{\"orchestra_format\": 99999999999999999999}",
            b"<?xml",
        ] {
            assert!(
                matches!(version(bytes), Err(LoadError::NotAProject)),
                "{bytes:?}"
            );
        }
        for bytes in [&b"PK\x03\x04rest"[..], b"\x1F\x8B\x08"] {
            assert!(matches!(
                version(bytes),
                Err(LoadError::NewerFormat { found: None, .. })
            ));
        }
    }
}
