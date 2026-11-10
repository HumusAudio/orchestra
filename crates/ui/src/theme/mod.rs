// SPDX-License-Identifier: GPL-3.0-or-later

//! The theme system (ADR-0014, ADR-0037): semantic tokens, ten built-in light/dark pairs, custom
//! themes as overrides, the WCAG contrast check.
//!
//! Everything a widget or renderer needs is re-exported at the crate root (ADR-0034 D1); the
//! submodules are private and only split the code by concern:
//!
//! - `color`: the colour value (sRGB, straight alpha) and compositing;
//! - `token`: the colour and shape tokens, and one theme instance;
//! - `pair`: a light + dark pair;
//! - `builtin`: the ten built-in pairs, as constants;
//! - `custom`: custom themes, a base pair plus overrides;
//! - `mode`: *System / Light / Dark* and which half it picks;
//! - `contrast`: the WCAG ratio and the table of pairs.
//!
//! No platform code here: reading the system's preference is step F2.02b, in `platform/`
//! behind a trait (ADR-0014 §5).

pub(crate) mod builtin;
pub(crate) mod color;
pub(crate) mod contrast;
pub(crate) mod custom;
pub(crate) mod mode;
pub(crate) mod pair;
pub(crate) mod token;
