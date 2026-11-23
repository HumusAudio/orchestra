// SPDX-License-Identifier: GPL-3.0-or-later

//! Where the settings file lives on each system (ADR-0050 DG7).
//!
//! | System | Directory |
//! |---|---|
//! | Linux and the other Unix systems | `$XDG_CONFIG_HOME/orchestra/`, or `$HOME/.config/orchestra/` |
//! | Windows | `%APPDATA%\Orchestra\` |
//! | macOS | `$HOME/Library/Application Support/Orchestra/` |
//!
//! # Why environment variables only
//! The three answers come from variables every desktop session sets: the XDG Base Directory
//! specification defines `XDG_CONFIG_HOME` and its fallback `~/.config`; Windows sets `APPDATA`
//! to the roaming application data (the folder `FOLDERID_RoamingAppData` names); macOS keeps
//! applications' own files under `~/Library/Application Support` (Apple's *File System
//! Programming Guide*; `~/Library/Preferences` is for the system's preference lists). Reading the
//! variables needs **no `unsafe`** (this crate denies it) and no crate: `dirs` 7.0 would call the
//! Windows known-folder API, which only matters when `APPDATA` is missing, and stays the fallback
//! if a case of that kind ever appears (DG7).
//!
//! # Testable on any system
//! The system is chosen **by value** ([`System`]) and the variables are read through a function
//! passed in, so the tests on Linux check the answers of all three systems with made-up
//! environments, as `orchestra-ui` does with its `Platform`.
//!
//! # Edge cases
//! - A variable that is set but **empty or relative** is ignored, as the XDG specification asks
//!   for `XDG_CONFIG_HOME` ("if … either not set or empty", and relative paths are invalid): a
//!   relative directory would depend on the working directory of the moment.
//! - No usable variable at all (a stripped service environment): `None`, and the program runs
//!   with the defaults without saving (DG7).

use std::ffi::OsString;
use std::path::PathBuf;

/// The system whose conventions decide the directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum System {
    /// Linux and every other Unix system that is not macOS (the XDG conventions).
    Unix,
    /// Windows.
    Windows,
    /// macOS.
    MacOs,
}

impl System {
    /// The system this program was built for.
    pub(crate) const CURRENT: Self = if cfg!(target_os = "macos") {
        Self::MacOs
    } else if cfg!(target_os = "windows") {
        Self::Windows
    } else {
        Self::Unix
    };
}

/// The directory of Orchestra's settings on `system`, with the environment read through `var`
/// (see the module documentation). O(1): at most two variables are read.
pub(crate) fn directory(system: System, var: impl Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    // A variable's value as an absolute path, or nothing when it is unset, empty or relative.
    let absolute = |name: &str| {
        var(name)
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
    };
    match system {
        System::Unix => absolute("XDG_CONFIG_HOME")
            .or_else(|| absolute("HOME").map(|home| home.join(".config")))
            .map(|base| base.join("orchestra")),
        System::Windows => absolute("APPDATA").map(|base| base.join("Orchestra")),
        System::MacOs => absolute("HOME").map(|home| {
            home.join("Library")
                .join("Application Support")
                .join("Orchestra")
        }),
    }
}

#[cfg(test)]
mod tests {
    //! The three systems' answers with made-up environments, and the variables that are ignored.

    use std::path::Path;

    use super::*;

    /// An environment made of `pairs` (name, value).
    fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<OsString> + use<> {
        let pairs: Vec<(String, OsString)> = pairs
            .iter()
            .map(|&(name, value)| (name.to_owned(), OsString::from(value)))
            .collect();
        move |name| {
            pairs
                .iter()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value.clone())
        }
    }

    /// An absolute path on the system the test runs on (Windows needs a drive to be absolute).
    fn abs(unix: &'static str, windows: &'static str) -> &'static str {
        if cfg!(windows) { windows } else { unix }
    }

    /// Linux: `XDG_CONFIG_HOME` first, then `HOME/.config`, ignoring empty and relative values.
    #[test]
    fn unix_follows_xdg() {
        let xdg = abs("/x/config", r"C:\x\config");
        let home = abs("/home/u", r"C:\home\u");
        let in_home = Path::new(home).join(".config").join("orchestra");
        assert_eq!(
            directory(
                System::Unix,
                env(&[("XDG_CONFIG_HOME", xdg), ("HOME", home)])
            ),
            Some(Path::new(xdg).join("orchestra"))
        );
        for ignored in ["relative", ""] {
            assert_eq!(
                directory(
                    System::Unix,
                    env(&[("XDG_CONFIG_HOME", ignored), ("HOME", home)])
                ),
                Some(in_home.clone()),
                "{ignored:?}"
            );
        }
        assert_eq!(directory(System::Unix, env(&[])), None);
    }

    /// Windows: `APPDATA`, and nothing without it.
    #[test]
    fn windows_uses_appdata() {
        let appdata = abs("/roaming", r"C:\Users\u\AppData\Roaming");
        assert_eq!(
            directory(System::Windows, env(&[("APPDATA", appdata)])),
            Some(Path::new(appdata).join("Orchestra"))
        );
        // `HOME` is not a Windows convention: without `APPDATA` there is no directory.
        let home = abs("/home/u", r"C:\home\u");
        assert_eq!(directory(System::Windows, env(&[("HOME", home)])), None);
    }

    /// macOS: under `Library/Application Support`, never `XDG_CONFIG_HOME`.
    #[test]
    fn macos_uses_application_support() {
        let home = abs("/Users/u", r"C:\Users\u");
        let vars = env(&[("HOME", home), ("XDG_CONFIG_HOME", abs("/x", r"C:\x"))]);
        assert_eq!(
            directory(System::MacOs, vars),
            Some(
                Path::new(home)
                    .join("Library")
                    .join("Application Support")
                    .join("Orchestra")
            )
        );
    }
}
