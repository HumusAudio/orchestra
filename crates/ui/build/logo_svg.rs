// SPDX-License-Identifier: GPL-3.0-or-later

//! Our small converter of the Orchestra logo's SVG into paths (ADR-0050 DG10, SCALETTA
//! F2.12a3b, owner's choice 1: run at build time by `crates/ui/build.rs`, written into `OUT_DIR`,
//! never committed).
//!
//! # Why our own converter, and why so small
//! DG10 asks for the logo in the About dialog without adding an SVG renderer as a dependency
//! (`resvg` and its tree would add hundreds of kilobytes and seconds of build time for one
//! picture). The logo file (`assets/logo/orchestra-logo.svg`) was produced by `pdftocairo -svg`
//! (see `assets/logo/README.md`) and uses a tiny part of SVG:
//! - one `viewBox="0 0 S S"` on the root, square;
//! - inside `<defs>`, one `<g id="glyph-…">` per letter of the wordmark, each holding one `<path>`;
//! - outside `<defs>`, one `<path>` (the drawing) and one `<use xlink:href="#glyph-…" x y>` per
//!   letter placed (a translation, nothing else);
//! - path data with the absolute commands `M`, `L`, `C`, `Z` only, one colour, the non-zero rule.
//!
//! This module reads exactly that and **refuses everything else** with a typed error: a
//! `transform`, another element, a relative or another command, the even-odd rule. A future
//! edit of the logo that uses more of SVG therefore fails the build with a message naming what
//! is missing, instead of drawing a different shape (assets/logo/README.md rule 1: never
//! reshape the logo).
//!
//! # Where it runs
//! The same file is compiled twice: by the build script (`#[path]` module), which writes the
//! generated Rust, and by the library's tests (`#[cfg(test)]`), which check the parser on small
//! inputs and check that the generated file is what the parser makes of the real logo. It is not
//! part of the shipped library: the program only carries the generated paths.
//!
//! # Complexity
//! O(n) in the file's length: one pass over the tags, one pass over each path's data. The logo
//! is 52 KB; converting it takes well under a millisecond of build time.

use std::fmt::{self, Write as _};

/// One element of a path, in the SVG's user units (the `viewBox`'s units), absolute.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Element {
    /// Starts a sub-path at (x, y).
    MoveTo(f64, f64),
    /// A straight line to (x, y).
    LineTo(f64, f64),
    /// A cubic Bézier: two control points, then the end point.
    CubicTo(f64, f64, f64, f64, f64, f64),
    /// Closes the current sub-path.
    Close,
}

impl Element {
    /// The same element moved by (dx, dy): how a `<use x y>` places a glyph.
    fn translated(self, dx: f64, dy: f64) -> Self {
        match self {
            Self::MoveTo(x, y) => Self::MoveTo(x + dx, y + dy),
            Self::LineTo(x, y) => Self::LineTo(x + dx, y + dy),
            Self::CubicTo(x1, y1, x2, y2, x, y) => {
                Self::CubicTo(x1 + dx, y1 + dy, x2 + dx, y2 + dy, x + dx, y + dy)
            }
            Self::Close => Self::Close,
        }
    }
}

/// The logo as paths: the side of its square canvas and its shapes, each filled on its own with
/// the non-zero rule, in document order.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Logo {
    /// The side of the square `viewBox`, in user units (2384 for today's file).
    pub(crate) side: f64,
    /// The shapes: the drawing's path, then one per placed letter, already translated.
    pub(crate) shapes: Vec<Vec<Element>>,
}

impl Logo {
    /// The smallest rectangle holding every shape **as drawn**, as (x0, y0, x1, y1); `None` for
    /// a logo without points.
    ///
    /// # Why the curves' true extent, not their control points
    /// A cubic Bézier stays inside the hull of its control points, but its control points may lie
    /// far outside the curve: today's logo has one at x ≈ 2642 on a 2384 canvas, while the curve
    /// itself reaches only x ≈ 2228. Checking control points would refuse a logo that is drawn
    /// entirely inside its canvas. So each curve contributes its end points and its extrema,
    /// where the derivative of x(t) or y(t) is zero for t in (0, 1).
    ///
    /// # Complexity
    /// O(elements): each curve solves two quadratics.
    pub(crate) fn bounds(&self) -> Option<(f64, f64, f64, f64)> {
        let mut bounds: Option<(f64, f64, f64, f64)> = None;
        let mut include = |x: f64, y: f64| {
            bounds = Some(match bounds {
                None => (x, y, x, y),
                Some((x0, y0, x1, y1)) => (x0.min(x), y0.min(y), x1.max(x), y1.max(y)),
            });
        };
        for shape in &self.shapes {
            // Where the pen is, and where the sub-path started (a `Z` goes back there).
            let mut current = (0.0, 0.0);
            let mut start = (0.0, 0.0);
            for &element in shape {
                match element {
                    Element::MoveTo(x, y) => {
                        include(x, y);
                        current = (x, y);
                        start = current;
                    }
                    Element::LineTo(x, y) => {
                        include(x, y);
                        current = (x, y);
                    }
                    Element::CubicTo(x1, y1, x2, y2, x, y) => {
                        let xs = [current.0, x1, x2, x];
                        let ys = [current.1, y1, y2, y];
                        for t in extrema(xs).into_iter().chain(extrema(ys)) {
                            include(cubic_at(xs, t), cubic_at(ys, t));
                        }
                        include(x, y);
                        current = (x, y);
                    }
                    Element::Close => current = start,
                }
            }
        }
        bounds
    }
}

/// The value at `t` of the one-dimensional cubic Bézier with coefficients `p` (Bernstein form).
fn cubic_at(p: [f64; 4], t: f64) -> f64 {
    let u = 1.0 - t;
    u * u * u * p[0] + 3.0 * u * u * t * p[1] + 3.0 * u * t * t * p[2] + t * t * t * p[3]
}

/// The parameters t in (0, 1) where the one-dimensional cubic Bézier `p` has a zero derivative.
///
/// The derivative is 3 · (a t² + b t + c) with a = −p0 + 3p1 − 3p2 + p3, b = 2 (p0 − 2p1 + p2),
/// c = p1 − p0; the roots of that quadratic (or of the linear equation when a is zero) inside
/// (0, 1) are the candidates for the curve's extent along this axis.
fn extrema(p: [f64; 4]) -> Vec<f64> {
    let a = -p[0] + 3.0 * p[1] - 3.0 * p[2] + p[3];
    let b = 2.0 * (p[0] - 2.0 * p[1] + p[2]);
    let c = p[1] - p[0];
    let roots = if a.abs() < 1e-12 {
        // Degenerate to a straight derivative: b t + c = 0 (no root at all when b is zero too).
        if b.abs() < 1e-12 {
            Vec::new()
        } else {
            vec![-c / b]
        }
    } else {
        let discriminant = b * b - 4.0 * a * c;
        if discriminant < 0.0 {
            Vec::new()
        } else {
            let root = discriminant.sqrt();
            vec![(-b + root) / (2.0 * a), (-b - root) / (2.0 * a)]
        }
    };
    roots.into_iter().filter(|t| *t > 0.0 && *t < 1.0).collect()
}

/// Why the logo's SVG could not be converted. The texts are for developers (they end a failed
/// build), so they are English and not translated.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum LogoError {
    /// A tag, a comment or an attribute value never ends.
    Unterminated(&'static str),
    /// An element this converter does not support (its name).
    UnsupportedElement(String),
    /// An attribute this converter does not support (`transform`, the even-odd rule).
    UnsupportedAttribute(&'static str),
    /// A required attribute is missing (its name).
    MissingAttribute(&'static str),
    /// The `viewBox` is not `0 0 S S` with a positive S.
    BadViewBox(String),
    /// A number in the path data or an attribute could not be read.
    BadNumber(String),
    /// A path command this converter does not support (its letter).
    UnsupportedCommand(char),
    /// Numbers in the path data with no command before them, or too few for their command.
    BadPathData(String),
    /// A `<path>` inside `<defs>` that is not in a `<g id="…">`.
    PathOutsideGlyph,
    /// A `<use>` naming a glyph that was not defined before it.
    UnknownGlyph(String),
    /// The file has no `viewBox`, or no shape.
    Empty,
}

impl fmt::Display for LogoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unterminated(what) => write!(f, "the logo's SVG has an unterminated {what}"),
            Self::UnsupportedElement(name) => {
                write!(f, "the logo's SVG uses the element <{name}>, not supported")
            }
            Self::UnsupportedAttribute(name) => {
                write!(f, "the logo's SVG uses `{name}`, not supported")
            }
            Self::MissingAttribute(name) => {
                write!(
                    f,
                    "an element of the logo's SVG lacks the attribute `{name}`"
                )
            }
            Self::BadViewBox(value) => {
                write!(
                    f,
                    "the logo's viewBox `{value}` is not a square starting at 0 0"
                )
            }
            Self::BadNumber(text) => write!(f, "the logo's SVG has the bad number `{text}`"),
            Self::UnsupportedCommand(letter) => {
                write!(
                    f,
                    "the logo's path data uses the command `{letter}`, not supported"
                )
            }
            Self::BadPathData(detail) => write!(f, "the logo's path data is malformed: {detail}"),
            Self::PathOutsideGlyph => write!(f, "a path in the logo's <defs> is not in a glyph"),
            Self::UnknownGlyph(id) => write!(f, "the logo's SVG uses the undefined glyph `{id}`"),
            Self::Empty => write!(f, "the logo's SVG has no viewBox or no shape"),
        }
    }
}

/// Reads the logo's SVG text (see the module documentation for what is supported).
///
/// # Errors
/// A [`LogoError`] naming the first thing that is not supported or not well formed.
pub(crate) fn parse(svg: &str) -> Result<Logo, LogoError> {
    // The glyphs defined so far, by ID: a handful (8 today), so a vector searched linearly is
    // simpler and as fast as a map.
    let mut glyphs: Vec<(String, Vec<Element>)> = Vec::new();
    // The ID of the `<g id="…">` most recently opened inside `<defs>`, waiting for its path.
    let mut pending_glyph: Option<String> = None;
    let mut in_defs = false;
    let mut side = None;
    let mut shapes = Vec::new();
    let mut rest = svg;
    while let Some((_, after)) = rest.split_once('<') {
        // A comment may contain anything but its end mark: skip it whole.
        if let Some(comment) = after.strip_prefix("!--") {
            let (_, tail) = comment
                .split_once("-->")
                .ok_or(LogoError::Unterminated("comment"))?;
            rest = tail;
            continue;
        }
        // No attribute value of this file contains `>`, so the first one ends the tag (a value
        // that did would end it early and fail below as a malformed attribute or element).
        let (tag, tail) = after
            .split_once('>')
            .ok_or(LogoError::Unterminated("tag"))?;
        rest = tail;
        // A translation is the only placement supported, and only through `<use x y>`.
        if attribute(tag, "transform").is_some() {
            return Err(LogoError::UnsupportedAttribute("transform"));
        }
        // The element's name: up to the first white space, without the `/` of a tag that
        // closes itself (`<path …/>` has a space first, `<g/>` does not); an end tag keeps its
        // leading `/` (`/g`).
        let word = tag.split(char::is_whitespace).next().unwrap_or_default();
        let name = word.strip_suffix('/').unwrap_or(word);
        match name {
            // The XML declaration says nothing about the shapes; a group's end changes nothing
            // either: the colour is the same everywhere and groups carry no placement (a
            // `transform` was refused above).
            "?xml" | "/g" | "/svg" => {}
            "svg" => side = Some(view_box(tag)?),
            "defs" => in_defs = true,
            "/defs" => in_defs = false,
            "g" => {
                if in_defs && let Some(id) = attribute(tag, "id") {
                    pending_glyph = Some(id.to_owned());
                }
            }
            "path" => {
                if attribute(tag, "fill-rule").is_some_and(|rule| rule != "nonzero") {
                    return Err(LogoError::UnsupportedAttribute("fill-rule"));
                }
                let data = attribute(tag, "d").ok_or(LogoError::MissingAttribute("d"))?;
                let elements = path_data(data)?;
                if in_defs {
                    let id = pending_glyph.take().ok_or(LogoError::PathOutsideGlyph)?;
                    glyphs.push((id, elements));
                } else {
                    shapes.push(elements);
                }
            }
            "use" => {
                let href = attribute(tag, "xlink:href")
                    .or_else(|| attribute(tag, "href"))
                    .ok_or(LogoError::MissingAttribute("xlink:href"))?;
                let id = href.strip_prefix('#').unwrap_or(href);
                let x = number_attribute(tag, "x")?;
                let y = number_attribute(tag, "y")?;
                let glyph = glyphs
                    .iter()
                    .find(|(glyph, _)| glyph == id)
                    .ok_or_else(|| LogoError::UnknownGlyph(id.to_owned()))?;
                shapes.push(glyph.1.iter().map(|e| e.translated(x, y)).collect());
            }
            other => return Err(LogoError::UnsupportedElement(other.to_owned())),
        }
    }
    match side {
        Some(side) if !shapes.is_empty() => Ok(Logo { side, shapes }),
        _ => Err(LogoError::Empty),
    }
}

/// The value of the attribute `name` in the tag's text (everything between `<` and `>`), if
/// present: `name="value"` preceded by white space. O(tag length).
fn attribute<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let key = format!("{name}=\"");
    let mut search = tag;
    while let Some((before, after)) = search.split_once(key.as_str()) {
        // `x="` must not match the end of `xlink:href="…x="`-like names: the key starts a word.
        if before.ends_with(char::is_whitespace) {
            return after.split_once('"').map(|(value, _)| value);
        }
        search = after;
    }
    None
}

/// The attribute `name` read as a number, 0 when absent (SVG's default for `<use>`'s x and y).
///
/// # Errors
/// [`LogoError::BadNumber`] when present but not a number.
fn number_attribute(tag: &str, name: &str) -> Result<f64, LogoError> {
    attribute(tag, name).map_or(Ok(0.0), |value| {
        value
            .trim()
            .parse()
            .map_err(|_| LogoError::BadNumber(value.to_owned()))
    })
}

/// The side of the root's `viewBox`, which must be `0 0 S S` with S positive.
///
/// # Errors
/// [`LogoError::MissingAttribute`] or [`LogoError::BadViewBox`].
fn view_box(tag: &str) -> Result<f64, LogoError> {
    let value = attribute(tag, "viewBox").ok_or(LogoError::MissingAttribute("viewBox"))?;
    let numbers: Vec<f64> = value
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|part| !part.is_empty())
        .map(str::parse)
        .collect::<Result<_, _>>()
        .map_err(|_| LogoError::BadViewBox(value.to_owned()))?;
    match numbers.as_slice() {
        // Exact comparisons on purpose: the origin is written as the integer 0, and a square
        // canvas has two identical numbers in the file (not two numbers that are merely close).
        #[allow(
            clippy::float_cmp,
            reason = "the file writes these numbers exactly, see above"
        )]
        &[x, y, w, h] if x == 0.0 && y == 0.0 && w == h && w > 0.0 => Ok(w),
        _ => Err(LogoError::BadViewBox(value.to_owned())),
    }
}

/// One token of path data: a command letter or a number.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Token {
    /// A command letter.
    Command(char),
    /// A number.
    Number(f64),
}

/// Splits path data into tokens. Numbers follow SVG's grammar as far as this file needs it:
/// an optional sign, digits with at most one decimal point, an optional exponent; a sign or a
/// second point starts the next number (`1.5-2` is two numbers). O(data length).
///
/// # Errors
/// [`LogoError::BadNumber`] for a number that does not parse.
fn tokens(data: &str) -> Result<Vec<Token>, LogoError> {
    let mut out = Vec::new();
    let mut chars = data.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() || c == ',' {
            chars.next();
        } else if c.is_ascii_alphabetic() {
            out.push(Token::Command(c));
            chars.next();
        } else {
            let mut text = String::new();
            let mut seen_point = false;
            let mut seen_exponent = false;
            while let Some(&c) = chars.peek() {
                let starts_number = text.is_empty();
                let after_exponent = text.ends_with(['e', 'E']);
                let accepted = match c {
                    '+' | '-' => starts_number || after_exponent,
                    '.' => !seen_point && !seen_exponent,
                    'e' | 'E' => !seen_exponent && !starts_number,
                    _ => c.is_ascii_digit(),
                };
                if !accepted {
                    break;
                }
                seen_point |= c == '.';
                seen_exponent |= matches!(c, 'e' | 'E');
                text.push(c);
                chars.next();
            }
            // A character that starts nothing (a stray symbol) would loop forever: refuse it.
            if text.is_empty() {
                return Err(LogoError::BadNumber(c.to_string()));
            }
            let value = text
                .parse()
                .map_err(|_| LogoError::BadNumber(text.clone()))?;
            out.push(Token::Number(value));
        }
    }
    Ok(out)
}

/// Reads path data into elements: the absolute `M`, `L`, `C` and `Z`, with SVG's implicit
/// repetition (more number groups after a command repeat it; after `M` they are lines).
///
/// # Errors
/// [`LogoError::UnsupportedCommand`] for any other letter, [`LogoError::BadPathData`] for numbers
/// without a command, a group cut short, or a path that does not start with `M`.
fn path_data(data: &str) -> Result<Vec<Element>, LogoError> {
    let tokens = tokens(data)?;
    let mut elements = Vec::new();
    let mut command: Option<char> = None;
    let mut index = 0;
    // The next `count` numbers, or an error naming the command they belong to.
    let take = |index: &mut usize, count: usize, letter: char| -> Result<Vec<f64>, LogoError> {
        let mut numbers = Vec::with_capacity(count);
        for _ in 0..count {
            match tokens.get(*index) {
                Some(&Token::Number(value)) => numbers.push(value),
                _ => {
                    return Err(LogoError::BadPathData(format!(
                        "`{letter}` needs {count} numbers"
                    )));
                }
            }
            *index += 1;
        }
        Ok(numbers)
    };
    while let Some(&token) = tokens.get(index) {
        let letter = match token {
            Token::Command(letter) => {
                index += 1;
                command = Some(letter);
                letter
            }
            // A number repeats the current command (after `M`, as a line; after `Z`, nothing
            // can be repeated).
            Token::Number(_) => match command {
                Some('M') => 'L',
                Some(letter) if letter != 'Z' => letter,
                _ => {
                    return Err(LogoError::BadPathData(
                        "numbers without a command".to_owned(),
                    ));
                }
            },
        };
        let element = match letter {
            'M' => {
                let n = take(&mut index, 2, letter)?;
                Element::MoveTo(n[0], n[1])
            }
            'L' => {
                let n = take(&mut index, 2, letter)?;
                Element::LineTo(n[0], n[1])
            }
            'C' => {
                let n = take(&mut index, 6, letter)?;
                Element::CubicTo(n[0], n[1], n[2], n[3], n[4], n[5])
            }
            'Z' => Element::Close,
            other => return Err(LogoError::UnsupportedCommand(other)),
        };
        if elements.is_empty() && !matches!(element, Element::MoveTo(..)) {
            return Err(LogoError::BadPathData(
                "a path must start with `M`".to_owned(),
            ));
        }
        elements.push(element);
    }
    Ok(elements)
}

/// The Rust source the build script writes into `OUT_DIR/logo_paths.rs`: the canvas's side and
/// the shapes as a static table of `LogoElement`s (the enum is defined in
/// `crates/ui/src/widget/logo.rs`, where the file is included).
///
/// # Precision
/// The coordinates are written as `f32` (the display list's type) with Rust's shortest exact
/// representation (`{:?}`), so the table holds exactly the `f32` nearest to each number of the
/// file. At the About dialog's size one user unit is about 0.04 logical pixels, and `f32` keeps
/// about seven significant digits: the rounding is a ten-thousandth of a pixel, invisible.
pub(crate) fn to_rust(logo: &Logo) -> String {
    let mut out = String::new();
    // `write!` into a `String` never fails; the results are ignored on purpose.
    let _ = writeln!(
        out,
        "// Generated by crates/ui/build.rs from assets/logo/orchestra-logo.svg: do not edit.\n\
         // The logo is NOT covered by the GPL (assets/logo/README.md); this file is never committed."
    );
    let _ = writeln!(
        out,
        "/// The side of the logo's square canvas, in the SVG's user units."
    );
    let _ = writeln!(
        out,
        "pub(crate) const LOGO_SIDE: f32 = {:?};",
        narrow(logo.side)
    );
    let _ = writeln!(
        out,
        "/// The logo's shapes, each filled on its own with the non-zero rule, in the SVG's user \
         units."
    );
    let _ = writeln!(out, "pub(crate) static LOGO_SHAPES: &[&[LogoElement]] = &[");
    for shape in &logo.shapes {
        let _ = writeln!(out, "    &[");
        for element in shape {
            let _ = match *element {
                Element::MoveTo(x, y) => {
                    writeln!(
                        out,
                        "        LogoElement::MoveTo({:?}, {:?}),",
                        narrow(x),
                        narrow(y)
                    )
                }
                Element::LineTo(x, y) => {
                    writeln!(
                        out,
                        "        LogoElement::LineTo({:?}, {:?}),",
                        narrow(x),
                        narrow(y)
                    )
                }
                Element::CubicTo(x1, y1, x2, y2, x, y) => writeln!(
                    out,
                    "        LogoElement::CubicTo({:?}, {:?}, {:?}, {:?}, {:?}, {:?}),",
                    narrow(x1),
                    narrow(y1),
                    narrow(x2),
                    narrow(y2),
                    narrow(x),
                    narrow(y)
                ),
                Element::Close => writeln!(out, "        LogoElement::Close,"),
            };
        }
        let _ = writeln!(out, "    ],");
    }
    let _ = writeln!(out, "];");
    out
}

/// A coordinate as the display list's `f32` (see [`to_rust`] for why the rounding is invisible).
#[allow(
    clippy::cast_possible_truncation,
    reason = "the logo's coordinates are within ±10 000 user units, far inside f32's range; the \
              rounding to f32's precision is intended (see `to_rust`)"
)]
const fn narrow(value: f64) -> f32 {
    value as f32
}
