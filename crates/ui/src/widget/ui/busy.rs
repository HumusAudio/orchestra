// SPDX-License-Identifier: GPL-3.0-or-later

//! The interface while the program is busy for a moment (ADR-0054 AR1, SCALETTA F2.14a): a project
//! is being opened or saved by the application's worker thread, and nothing may change it
//! meanwhile.
//!
//! # The busy layer
//! [`Ui::begin_busy`] pushes the same empty modal layer as the file chooser's (the module
//! `files`, ADR-0050 DG4): nothing is drawn, the pointer and the keys reach nothing under it, the
//! menu bar and the shortcuts are off, as under any dialog. Two differences:
//! - the pointer takes the **wait** shape ([`PointerShape::Wait`](crate::PointerShape)) over the
//!   whole window, so the user sees why nothing answers (the system's chooser showed itself; a
//!   worker does not);
//! - the window's accessibility node is marked **busy** (AccessKit's `busy` state, ARIA's
//!   `aria-busy`), so a screen reader can say that the window is working instead of seeming
//!   dead; the layer itself is not listed as a dialog, there being nothing in it to read.
//!
//! # Why not simply ignore input in the application
//! The application could drop the events itself, but then the menu bar would still open, the
//! focus would still move and the pointer would still hover: one blocking mechanism for every
//! modal situation (dialogs, choosers, this) keeps those rules in one place (ADR-0045 WA4, WB9).
//!
//! # One at a time
//! The application reads or writes one project at a time (AR1), so one layer is enough:
//! [`Ui::begin_busy`] answers `false` while one is up. The file chooser's layer and this one are
//! never up together in practice (the application starts reading only after the chooser
//! answered), but nothing breaks if they were: they are two layers of the same stack.
//!
//! # Cost
//! As a dialog without content (two widgets inserted, a frame without damaged pixels). Nothing
//! while it is up; input behind it is consumed by the layer's root, as behind the chooser's.

use crate::draw::list::DisplayList;

use super::Ui;
use super::files::Blocker;

impl<M: 'static> Ui<M> {
    /// Blocks the interface while the application works (see the module documentation): an
    /// empty modal layer, the wait pointer, the window marked busy for assistive technologies.
    /// Answers `false`, changing nothing, while it is already busy.
    pub fn begin_busy(&mut self, list: &mut DisplayList) -> bool {
        if self.busy_layer.is_some() {
            return false;
        }
        let (root, _panel) = self.push_dialog(Box::new(Blocker));
        self.open_pushed_dialog(root, None, list);
        self.busy_layer = Some(root);
        // The window node carries the busy state: rebuilt at the next accessibility update.
        self.access_window = true;
        true
    }

    /// `true` from [`Ui::begin_busy`] until [`Ui::end_busy`].
    #[must_use]
    pub const fn is_busy(&self) -> bool {
        self.busy_layer.is_some()
    }

    /// The work is over: the busy layer goes, the focus comes back to where it was, the pointer
    /// takes the shape of what is under it again, and a frame runs. Nothing happens when the
    /// interface was not busy (an answer that came twice).
    pub fn end_busy(&mut self, list: &mut DisplayList) {
        let Some(root) = self.busy_layer.take() else {
            return;
        };
        if let Some(position) = self.dialogs.iter().position(|dialog| dialog.root == root) {
            self.close_dialogs_from(position, list);
        }
        // What is under the pointer now: the layer that was hovered is gone, and without this the
        // window would keep the arrow over a text field until the pointer moved (the window asks
        // for the shape after this batch, `PointerShape::Wait` no longer answers). Not needed after
        // a dialog, whose own panel the pointer was over: here the whole window changes shape at
        // once. A walk down the tree, O(depth × children).
        let target = self.hit(self.pointer);
        self.set_hover(target);
        self.access_window = true;
        self.frame(list);
    }
}
