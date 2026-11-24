// SPDX-License-Identifier: GPL-3.0-or-later

//! What a file chooser does to the interface while it is open (ADR-0050 DG4, SCALETTA F2.12a5a):
//! an empty modal layer that blocks it, and the request waiting for the window.
//!
//! # The blocking layer
//! [`Ui::choose_file`] pushes a layer of the dialogs' stack (the module `dialogs`) whose panel is
//! an empty widget ([`Blocker`]): nothing is drawn, so the window looks as it was (the system's
//! chooser is what the user looks at), but the layer is **modal**, as a dialog is:
//! - the pointer reaches nothing under it (the dialog's root consumes presses, moves, the wheel);
//! - the keys reach no widget under it (no widget inside can take the focus, so they go to the
//!   layer's root, which ignores them), and Tab finds nothing to move to;
//! - the menu bar and the actions' shortcuts are off, as under any dialog (DG2): Ctrl+O cannot
//!   open a second chooser, Ctrl+Q cannot quit with the chooser open (choice 4 of F2.12a5a);
//! - assistive technologies are not told of a dialog: the window node does not list this layer,
//!   since the dialog they should read is the system's chooser, which they reach by themselves.
//!
//! The focus that was in the interface comes back when the layer goes, as after a dialog.
//!
//! # One chooser at a time
//! While a layer is up, [`Ui::choose_file`] refuses another request (`false`). The window also
//! starts at most one chooser (`crate::platform::file_chooser`).
//!
//! # Cost
//! Opening and closing the layer is the cost of a dialog without content: two widgets inserted,
//! a frame (no pixel damaged, since nothing is drawn). Nothing while it is up.

use crate::draw::list::{DisplayList, Recorder};
use crate::widget::layout::{Constraints, Size};
use crate::widget::tree::{LayoutCx, RecordCx, Widget};

use super::Ui;

/// The empty panel of the blocking layer (see the module documentation): no size, no drawing, no
/// focus. Its root, the dialogs' own, consumes the pointer.
#[derive(Debug)]
pub(crate) struct Blocker;

impl<M: 'static> Widget<M> for Blocker {
    /// No room at all: the layer's root spans the window and is what blocks.
    fn measure(&mut self, _cx: &mut LayoutCx<'_, M>, _constraints: Constraints) -> Size {
        Size::ZERO
    }

    /// Nothing: the window stays as it was under the system's chooser.
    fn record(&self, _cx: &RecordCx, _recorder: &mut Recorder<'_>) {}
}

impl<M: 'static> Ui<M> {
    /// Asks for a file chooser (ADR-0050 DG4): blocks the interface with an empty modal layer
    /// (see the module documentation) and keeps `request` for the window
    /// ([`Ui::take_file_request`]). Answers `false`, changing nothing, while a chooser asked
    /// before has not answered yet ([`Ui::file_chosen`]).
    pub fn choose_file(&mut self, request: crate::FileRequest, list: &mut DisplayList) -> bool {
        if self.file_layer.is_some() {
            return false;
        }
        let (root, _panel) = self.push_dialog(Box::new(Blocker));
        self.open_pushed_dialog(root, None, list);
        self.file_layer = Some(root);
        self.file_request = Some(request);
        true
    }

    /// The request [`Ui::choose_file`] kept, once (the window asks after every batch of events:
    /// [`WindowApp::take_file_request`](crate::WindowApp::take_file_request)).
    pub const fn take_file_request(&mut self) -> Option<crate::FileRequest> {
        self.file_request.take()
    }

    /// `true` from [`Ui::choose_file`] until [`Ui::file_chosen`].
    #[must_use]
    pub const fn choosing_file(&self) -> bool {
        self.file_layer.is_some()
    }

    /// The chooser answered ([`WindowApp::file_chosen`](crate::WindowApp::file_chosen)): the
    /// blocking layer goes, the focus comes back to where it was, and a frame runs. The answer
    /// itself is the application's to act on. Nothing happens without a layer (an answer that
    /// came twice).
    pub fn file_chosen(&mut self, list: &mut DisplayList) {
        // A request the window never took (the window was not open yet) is dropped with it.
        self.file_request = None;
        let Some(root) = self.file_layer.take() else {
            return;
        };
        if let Some(position) = self.dialogs.iter().position(|dialog| dialog.root == root) {
            self.close_dialogs_from(position, list);
        }
        self.frame(list);
    }
}
