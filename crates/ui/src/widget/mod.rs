// SPDX-License-Identifier: GPL-3.0-or-later

//! The widgets' core (ADR-0045, step F2.09a): a retained tree of widgets, its layout, the
//! routing of events, the keyboard focus and modal layers.
//!
//! - `tree`: the arena, the [`Widget`] trait, the contexts a widget receives;
//! - `layout`: sizes, constraints, size rules, and the containers [`Flex`], [`Stack`], [`Grid`];
//! - `ui`: [`Ui`], the interface of one window: routing, focus, modals, frames;
//! - `access`: how widgets describe themselves to assistive technologies, and AccessKit's tree;
//! - `label`, `button`: the first widgets.
//!
//! More widgets (checkbox, switch, slider, scroll area, splitter) come in F2.09c–d.

pub(crate) mod access;
pub(crate) mod button;
pub(crate) mod label;
pub(crate) mod layout;
pub(crate) mod look;
pub(crate) mod slider;
pub(crate) mod toggle;
pub(crate) mod tree;
pub(crate) mod ui;
