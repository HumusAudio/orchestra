// SPDX-License-Identifier: GPL-3.0-or-later

//! The widgets' core (ADR-0045, step F2.09a): a retained tree of widgets, its layout, the
//! routing of events, the keyboard focus and modal layers.
//!
//! - `tree`: the arena, the [`Widget`] trait, the contexts a widget receives;
//! - `layout`: sizes, constraints, size rules, and the containers [`Flex`], [`Stack`], [`Grid`];
//! - `ui`: [`Ui`], the interface of one window: routing, focus, modals, frames.
//!
//! The first concrete widgets (label, button…) come in F2.09b and F2.09c, AccessKit in F2.09b.

pub(crate) mod layout;
pub(crate) mod tree;
pub(crate) mod ui;
