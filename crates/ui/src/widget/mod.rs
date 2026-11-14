// SPDX-License-Identifier: GPL-3.0-or-later

//! The widgets' core (ADR-0045, step F2.09a): a retained tree of widgets, its layout, the
//! routing of events, the keyboard focus and modal layers.
//!
//! - `tree`: the arena, the [`Widget`] trait, the contexts a widget receives;
//! - `layout`: sizes, constraints, size rules, and the containers [`Flex`], [`Stack`], [`Grid`];
//! - `ui`: [`Ui`], the interface of one window: routing, focus, modals, frames;
//! - `access`: how widgets describe themselves to assistive technologies, and AccessKit's tree;
//! - `label`, `button`, `toggle`, `slider`: the controls of F2.09b–c;
//! - `scroll`, `splitter`: the scroll area and the splitter of F2.09d;
//! - `edit`: the text field's editing core (ADR-0047, F2.10b1): text, caret, selection, keys,
//!   undo;
//! - `look`: what every control draws the same way (the focus ring).

pub(crate) mod access;
pub(crate) mod button;
pub(crate) mod edit;
pub(crate) mod label;
pub(crate) mod layout;
pub(crate) mod look;
pub(crate) mod scroll;
pub(crate) mod slider;
pub(crate) mod splitter;
pub(crate) mod toggle;
pub(crate) mod tree;
pub(crate) mod ui;
