// SPDX-License-Identifier: GPL-3.0-or-later

//! Windows and macOS, until step F2.04: a source that never answers (ADR-0038 S3).
//!
//! On both systems the right source is `winit` (`ActiveEventLoop::system_theme()` for the
//! initial value, `WindowEvent::ThemeChanged` for the changes): it uses the registry value
//! `AppsUseLightTheme` on Windows and `NSApp.effectiveAppearance` on macOS, exactly as ADR-0014
//! §5 asks, without polling and without new dependencies. `winit` enters the workspace in F2.04,
//! together with the first window; this file is replaced there.
//!
//! Until then *System* gives dark (ADR-0037 T9). Nothing is visible yet, so the fallback costs
//! nothing, and no error is recorded: this is not a failure, just a part not built yet.

use super::{AppearanceCallback, AppearanceWatch, SystemAppearanceSource};

/// The source of the platforms without an implementation yet: always "no answer".
pub(crate) struct NoAnswerSource;

impl SystemAppearanceSource for NoAnswerSource {
    /// Returns at once with `None`; the callback is dropped unused, because nothing ever changes.
    fn observe(&self, on_change: AppearanceCallback) -> AppearanceWatch {
        drop(on_change);
        AppearanceWatch::no_answer()
    }
}
