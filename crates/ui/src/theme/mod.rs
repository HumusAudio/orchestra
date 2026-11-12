// SPDX-License-Identifier: GPL-3.0-or-later

//! The theme system (ADR-0014, ADR-0037): semantic tokens, ten built-in light/dark pairs, custom
//! themes as overrides, the WCAG contrast check.
//!
//! Everything a widget or renderer needs is re-exported at the crate root (ADR-0034 D1); the
//! submodules are private and only split the code by concern:
//!
//! - `color`: the colour value (sRGB, straight alpha) and compositing;
//! - `token`: the colour and shape tokens, and one theme instance;
//! - `density`: the density and its spacing values (F2.09a);
//! - `text`: the text tokens (families and sizes) and the weights (ADR-0044 TX11);
//! - `pair`: a light + dark pair;
//! - `builtin`: the ten built-in pairs, as constants;
//! - `custom`: custom themes, a base pair plus overrides;
//! - `mode`: *System / Light / Dark* and which half it picks;
//! - `contrast`: the WCAG ratio and the table of pairs.
//!
//! No platform code here: the system's preference is read in `platform::appearance`, behind the
//! trait `SystemAppearanceSource` (ADR-0014 §5, ADR-0038), and handed to `ThemeMode::resolve`.

pub(crate) mod builtin;
pub(crate) mod color;
pub(crate) mod contrast;
pub(crate) mod custom;
pub(crate) mod density;
pub(crate) mod mode;
pub(crate) mod pair;
pub(crate) mod text;
pub(crate) mod token;
