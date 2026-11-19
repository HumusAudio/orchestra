// SPDX-License-Identifier: GPL-3.0-or-later

//! Actions and their shortcuts (ADR-0048, F2.11d1): the generic machinery, which knows nothing of
//! music. The application registers its concrete actions at start-up (AM1); the interface
//! ([`crate::Ui`]) looks up the keys no widget handled (AM4) and tells the application which
//! action to run, with one of its own messages.
//!
//! - [`shortcut`]: what a shortcut is, how a key press becomes the shortcuts it may mean (with
//!   the two fallbacks by position of AM5), the fixed keys of AM8.
//! - [`registry`]: the actions, their contexts, defaults, the user's shortcuts with the conflict
//!   rules of AM7, their states (AM2) and the lookup.
//!
//! The menus built on the same registry come in F2.11d2; `muda` on macOS in F2.11e.

pub(crate) mod registry;
pub(crate) mod shortcut;
#[cfg(test)]
mod tests;
