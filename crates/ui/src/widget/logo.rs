// SPDX-License-Identifier: GPL-3.0-or-later

//! The Orchestra logo as a widget (ADR-0050 DG10, SCALETTA F2.12a3b): the shapes our build
//! script converted from `assets/logo/orchestra-logo.svg`, filled in the theme's `text` colour.
//!
//! # The logo's rules (assets/logo/README.md, ADR-0014)
//! - **Never reshaped**: the paths are the file's own, converted at build time without
//!   simplification (`build/logo_svg.rs`), and drawn with one **uniform** scale that keeps the
//!   square canvas and the author's margins. A rectangle that is not square gets the largest
//!   square that fits, centred in it.
//! - **Recoloured** only: the file's single colour becomes [`ColorToken::Text`], so the logo reads
//!   as the texts around it do, in every theme and mode.
//!
//! # Where the shapes come from
//! `crates/ui/build.rs` writes `OUT_DIR/logo_paths.rs` with `LOGO_SIDE` (the canvas's side in
//! the SVG's units) and `LOGO_SHAPES` (each shape a list of [`LogoElement`]s); this module
//! includes it. Nothing is parsed while the program runs.
//!
//! # Cost
//! Recording the logo copies its elements into the display list once (10 shapes, about 860
//! elements today, O(elements)); it is recorded again only when the widget moves or the dialog
//! opens. Rasterising it is `tiny-skia`'s path fill over the logo's square; the About dialog's
//! benchmark measures both.

use crate::draw::geometry::Rect;
use crate::draw::list::Recorder;
use crate::draw::paint::FillRule;
use crate::theme::token::ColorToken;
use crate::widget::access::{AccessInfo, AccessRole};
use crate::widget::layout::{Constraints, Size};
use crate::widget::tree::{LayoutCx, RecordCx, Widget};

/// One element of a logo's path, in the SVG's user units (absolute coordinates, y downwards like
/// the display list's).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum LogoElement {
    /// Starts a sub-path at (x, y).
    MoveTo(f32, f32),
    /// A straight line to (x, y).
    LineTo(f32, f32),
    /// A cubic Bézier: two control points, then the end point.
    CubicTo(f32, f32, f32, f32, f32, f32),
    /// Closes the current sub-path.
    Close,
}

/// The table the build script generated (see the module documentation).
mod generated {
    #![allow(
        clippy::unreadable_literal,
        clippy::excessive_precision,
        clippy::approx_constant,
        reason = "the coordinates are written by the build script in Rust's shortest exact form; \
                  grouping digits or rounding them would only make the table harder to compare \
                  with the SVG, and a value near a mathematical constant is a coincidence"
    )]
    use super::LogoElement;
    include!(concat!(env!("OUT_DIR"), "/logo_paths.rs"));
}

pub(crate) use generated::{LOGO_SHAPES, LOGO_SIDE};

/// The Orchestra logo, `side` logical pixels square, named `label` for assistive technologies
/// (see the module documentation).
#[derive(Debug, Clone)]
pub struct Logo {
    /// The side it asks for, logical pixels.
    side: f32,
    /// Its accessible name, already localised ("Orchestra logo").
    label: String,
}

impl Logo {
    /// The logo at `side` logical pixels (a negative or non-finite side counts as 0), named
    /// `label` for assistive technologies.
    #[must_use]
    pub fn new(side: f32, label: impl Into<String>) -> Self {
        Self {
            side: if side.is_finite() { side.max(0.0) } else { 0.0 },
            label: label.into(),
        }
    }

    /// The square the logo is drawn in inside `rect`: the largest that fits, centred, so the scale
    /// is uniform (never reshaped). Its side divided by [`LOGO_SIDE`] is the scale.
    fn square(rect: Rect) -> Rect {
        let side = rect.width().min(rect.height()).max(0.0);
        let x = rect.x0 + (rect.width() - side) / 2.0;
        let y = rect.y0 + (rect.height() - side) / 2.0;
        Rect::new(x, y, x + side, y + side)
    }
}

impl<M: 'static> Widget<M> for Logo {
    /// Its side in both directions, within the constraints.
    fn measure(&mut self, _cx: &mut LayoutCx<'_, M>, constraints: Constraints) -> Size {
        constraints.constrain(Size::new(self.side, self.side))
    }

    /// Each shape filled on its own with the non-zero rule (the SVG's), in the text colour, mapped
    /// from the canvas to the widget's square by one scale and one offset.
    fn record(&self, cx: &RecordCx, recorder: &mut Recorder<'_>) {
        let square = Self::square(cx.rect);
        if square.width() <= 0.0 {
            return;
        }
        let scale = square.width() / LOGO_SIDE;
        let (x0, y0) = (square.x0, square.y0);
        // From the SVG's units to logical pixels: the same affine map for every point.
        let map = |x: f32, y: f32| (x0 + x * scale, y0 + y * scale);
        for shape in LOGO_SHAPES {
            recorder.fill_path(FillRule::NonZero, ColorToken::Text, |path| {
                for &element in *shape {
                    match element {
                        LogoElement::MoveTo(x, y) => {
                            let (x, y) = map(x, y);
                            path.move_to(x, y);
                        }
                        LogoElement::LineTo(x, y) => {
                            let (x, y) = map(x, y);
                            path.line_to(x, y);
                        }
                        LogoElement::CubicTo(x1, y1, x2, y2, x, y) => {
                            let (x1, y1) = map(x1, y1);
                            let (x2, y2) = map(x2, y2);
                            let (x, y) = map(x, y);
                            path.cubic_to(x1, y1, x2, y2, x, y);
                        }
                        LogoElement::Close => path.close(),
                    }
                }
            });
        }
    }

    fn access_role(&self) -> AccessRole {
        AccessRole::Image
    }

    fn access_info(&self, info: &mut AccessInfo) {
        info.label = Some(self.label.clone());
    }
}

/// The converter, compiled here for the tests below (the build script compiles it too). Declared
/// at this level, not inside `tests`, because a `#[path]` is resolved from this file's directory
/// only here (inside an inline module it would start from `logo/tests/`, which does not exist).
#[cfg(test)]
#[allow(
    dead_code,
    reason = "the build script uses every item; the tests use most of them"
)]
#[path = "../../build/logo_svg.rs"]
mod logo_svg;

#[cfg(test)]
mod tests {
    //! The generated table against the converter, and the converter on small inputs.

    use super::*;

    use super::logo_svg::{self, Element, LogoError, parse};

    /// The real logo, read as the build script reads it.
    const SVG: &str = include_str!("../../../../assets/logo/orchestra-logo.svg");

    /// The file included by this module is exactly what the converter makes of today's logo:
    /// the build script ran on this very file.
    #[test]
    fn generated_table_matches_the_logo() {
        let logo = parse(SVG).unwrap();
        let generated = include_str!(concat!(env!("OUT_DIR"), "/logo_paths.rs"));
        assert_eq!(logo_svg::to_rust(&logo), generated);
        // One drawing and nine placed letters (O R C H E S T R A, the R used twice).
        assert_eq!(logo.shapes.len(), 10);
        assert_eq!(LOGO_SHAPES.len(), 10);
        assert!((LOGO_SIDE - 2384.0).abs() < f32::EPSILON);
        let elements: usize = LOGO_SHAPES.iter().map(|shape| shape.len()).sum();
        assert_eq!(elements, logo.shapes.iter().map(Vec::len).sum::<usize>());
        // Every shape starts a sub-path and stays inside the canvas.
        for shape in LOGO_SHAPES {
            assert!(matches!(shape.first(), Some(LogoElement::MoveTo(..))));
        }
        let (x0, y0, x1, y1) = logo.bounds().unwrap();
        assert!(x0 >= 0.0 && y0 >= 0.0 && x1 <= logo.side && y1 <= logo.side);
    }

    /// A glyph placed twice with `<use>`, implicit repetitions of `M` (as lines) and of `C`.
    #[test]
    fn glyphs_uses_and_repetitions() {
        let svg = r##"<svg viewBox="0 0 10 10"><defs><g><g id="a">
            <path d="M 1 1 2 2 C 1 1 2 2 3 3 4 4 5 5 6 6 Z"/></g></g></defs>
            <path fill-rule="nonzero" d="M0,0L1-1Z"/>
            <g><use xlink:href="#a" x="1" y="2"/><use href="#a" x="0.5" y="-1e-1"/></g></svg>"##;
        let logo = parse(svg).unwrap();
        assert!((logo.side - 10.0).abs() < f64::EPSILON);
        assert_eq!(logo.shapes.len(), 3);
        assert_eq!(
            logo.shapes[0],
            vec![
                Element::MoveTo(0.0, 0.0),
                Element::LineTo(1.0, -1.0),
                Element::Close
            ]
        );
        assert_eq!(
            logo.shapes[1],
            vec![
                Element::MoveTo(2.0, 3.0),
                Element::LineTo(3.0, 4.0),
                Element::CubicTo(2.0, 3.0, 3.0, 4.0, 4.0, 5.0),
                Element::CubicTo(5.0, 6.0, 6.0, 7.0, 7.0, 8.0),
                Element::Close,
            ]
        );
        assert_eq!(logo.shapes[2][0], Element::MoveTo(1.5, 0.9));
    }

    /// What the converter does not support fails with a message, never with another shape.
    #[test]
    fn unsupported_svg_is_refused() {
        let refused = |svg: &str| parse(svg).unwrap_err();
        assert_eq!(
            refused(r#"<svg viewBox="0 0 1 1"><path d="m 0 0 1 1"/></svg>"#),
            LogoError::UnsupportedCommand('m')
        );
        assert_eq!(
            refused(r#"<svg viewBox="0 0 1 1"><path d="M 0 0 A 1 1 0 0 0 1 1"/></svg>"#),
            LogoError::UnsupportedCommand('A')
        );
        assert_eq!(
            refused(r#"<svg viewBox="0 0 1 1"><g transform="scale(2)"></g></svg>"#),
            LogoError::UnsupportedAttribute("transform")
        );
        assert_eq!(
            refused(r#"<svg viewBox="0 0 1 1"><path fill-rule="evenodd" d="M 0 0"/></svg>"#),
            LogoError::UnsupportedAttribute("fill-rule")
        );
        assert_eq!(
            refused(r#"<svg viewBox="0 0 1 1"><circle r="1"/></svg>"#),
            LogoError::UnsupportedElement("circle".to_owned())
        );
        assert_eq!(
            refused(r#"<svg viewBox="0 0 2 1"><path d="M 0 0"/></svg>"#),
            LogoError::BadViewBox("0 0 2 1".to_owned())
        );
        assert_eq!(
            refused(r##"<svg viewBox="0 0 1 1"><use xlink:href="#x"/></svg>"##),
            LogoError::UnknownGlyph("x".to_owned())
        );
        assert!(matches!(
            refused(r#"<svg viewBox="0 0 1 1"><path d="M 0 0 C 1 1"/></svg>"#),
            LogoError::BadPathData(_)
        ));
        assert!(matches!(
            refused(r#"<svg viewBox="0 0 1 1"><path d="L 0 0"/></svg>"#),
            LogoError::BadPathData(_)
        ));
        assert_eq!(
            refused(r#"<svg viewBox="0 0 1 1"><path d="M 0 0 1 # 2"/></svg>"#),
            LogoError::BadNumber("#".to_owned())
        );
        assert_eq!(
            refused(r#"<svg viewBox="0 0 1 1"></svg>"#),
            LogoError::Empty
        );
        assert_eq!(
            refused("<svg viewBox=\"0 0 1 1\"><!-- open"),
            LogoError::Unterminated("comment")
        );
    }

    /// The square is the largest that fits, centred: the scale is uniform in any rectangle.
    #[test]
    fn square_is_centred_and_uniform() {
        let square = Logo::square(Rect::new(10.0, 20.0, 110.0, 70.0));
        assert_eq!(square, Rect::new(35.0, 20.0, 85.0, 70.0));
        let logo = Logo::new(f32::NAN, "logo");
        assert!(logo.side.abs() < f32::EPSILON);
    }
}
