// SPDX-License-Identifier: GPL-3.0-or-later

//! The colour picker (SCALETTA F2.12b3a, owner's choices 1–7 of 2026-11-26): choosing a colour
//! by sight, without writing its code.
//!
//! # What it shows
//! From top to bottom, in one widget:
//! - **the square**: brightness upwards, intensity (saturation) to the right, for the hue
//!   chosen; a small ring marks the colour;
//! - **the hue strip**: every hue from red through yellow, green, blue and magenta back to red;
//!   a bar marks the hue;
//! - **the transparency strip**, only when the application asks for it (a colour that is
//!   transparent in the theme's base, choice 3): from transparent on the left to opaque on the
//!   right, over a checkerboard;
//! - **the two swatches**: the colour before the picker opened on the left, the colour now on the
//!   right (choice 2).
//!
//! The model is HSV (hue, saturation, value), the one GIMP, macOS and the browsers show (choice
//! 2). The widget keeps hue, saturation and value as numbers of its own, not only the colour: a
//! grey has no hue, and dragging the saturation to zero and back must not lose the hue the user
//! was on.
//!
//! # Drawing: pictures, not literal colours
//! The drawing carries theme tokens only (ADR-0039 D3); the only other pixels are pictures (D4).
//! The square, the strips and the swatches are pictures the widget makes
//! ([`Widget::take_images`], the precision of ADR-0039 D4 and ADR-0045 approved on 2026-11-26):
//! the square 64 × 64 pixels (rebuilt only when the hue changes), the hue strip 64 × 1 (made
//! once), the transparency strip 64 × 1, the swatches 64 × 1 (half each); the renderer stretches
//! them with bilinear filtering, so 64 pixels are smooth at any size. The checkerboard, the
//! outlines and the markers are drawn with tokens.
//!
//! # Pointer
//! A press in a part moves the colour under the pointer and captures it; dragging follows, also
//! outside the widget, until the release (ADR-0045 WA3). With the modifier of fine adjustment held
//! the drag moves a tenth as fast and the press does not jump (ADR-0046 V3), as the sliders.
//!
//! # Keyboard
//! The widget has one focus and three (or four) **parts**: the square, the hue strip, the
//! transparency strip. Tab moves to the next part, Shift+Tab to the previous one; past the first
//! or the last, the focus leaves the widget as usual. In the square the arrows move the
//! intensity (left, right) and the brightness (up, down); on a strip they move its value. A step
//! is 4% (6° of hue), one with the fine modifier (1%, 1°); Home and End go to the ends.
//!
//! # Assistive technologies
//! The part with the focus is announced as a slider: its name (the picker's, then the part's),
//! its value and range (so increase, decrease and set work: in the square they act on the
//! intensity, the arrows on the brightness too), and the colour's code as the description.
//!
//! # Messages
//! Each change emits the application's message built from the new colour (`fn(Color) -> M`),
//! alpha included.

use crate::draw::geometry::{Point, Rect, RoundedRect};
use crate::draw::image::Image;
use crate::draw::list::Recorder;
use crate::draw::paint::{Paint, StrokeStyle};
use crate::theme::color::Color;
use crate::theme::token::ColorToken;
use crate::window::input::{ButtonState, Key, NamedKey, PointerButton};

use super::access::{AccessInfo, AccessRole};
use super::layout::{Constraints, Size};
use super::tree::{EventCx, Handled, LayoutCx, RecordCx, Widget, WidgetEvent};

/// The side of the square's picture, pixels: 64 × 64 = 4096 colours, smooth once stretched with
/// bilinear filtering, rebuilt in a few tens of microseconds when the hue changes.
const SQUARE_PIXELS: u32 = 64;
/// The length of a strip's picture, pixels.
const STRIP_PIXELS: u32 = 64;
/// The width the picker asks for when its parent does not size it, logical pixels.
const PREFERRED_WIDTH: f32 = 240.0;
/// The square's height, logical pixels.
const SQUARE_HEIGHT: f32 = 150.0;
/// A strip's height, logical pixels.
const STRIP_HEIGHT: f32 = 16.0;
/// The swatches' height, logical pixels.
const SWATCH_HEIGHT: f32 = 24.0;
/// The side of a checkerboard's square, logical pixels.
const CHECKER: f32 = 4.0;
/// A coarse step of a part: 4% of the square or the transparency, 6° of hue (1/60 of the
/// circle, ADR-0046's "coarse" for a value without steps of its own).
const COARSE: f64 = 0.04;
/// A fine step: 1%, 1° (ADR-0046 V3).
const FINE: f64 = 0.01;
/// How much slower a fine drag is (ADR-0046 V3).
const FINE_DRAG: f64 = 0.1;

/// A part of the picker (see the module documentation).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickerPart {
    /// The square: intensity and brightness.
    Square,
    /// The hue strip.
    Hue,
    /// The transparency strip (only when shown).
    Alpha,
}

/// A colour as hue (degrees, `0.0..360.0`), saturation and value (`0.0..=1.0`): the picker's own
/// numbers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hsv {
    /// Hue, degrees, `0.0..360.0`.
    pub h: f64,
    /// Saturation, `0.0..=1.0`.
    pub s: f64,
    /// Value (brightness), `0.0..=1.0`.
    pub v: f64,
}

impl Hsv {
    /// The HSV of `color`'s red, green and blue (its alpha aside). A grey's hue is `hue_if_grey`
    /// (it has none of its own: the picker keeps the one the user was on). O(1).
    #[must_use]
    pub fn of(color: Color, hue_if_grey: f64) -> Self {
        let (red, green, blue) = (
            f64::from(color.r) / 255.0,
            f64::from(color.g) / 255.0,
            f64::from(color.b) / 255.0,
        );
        let max = red.max(green).max(blue);
        let min = red.min(green).min(blue);
        let chroma = max - min;
        // The hue from whichever channel is largest, in degrees (the usual sector formula).
        let hue = if chroma <= 0.0 {
            hue_if_grey
        } else if (max - red).abs() < f64::EPSILON {
            60.0 * ((green - blue) / chroma).rem_euclid(6.0)
        } else if (max - green).abs() < f64::EPSILON {
            60.0 * ((blue - red) / chroma + 2.0)
        } else {
            60.0 * ((red - green) / chroma + 4.0)
        };
        let saturation = if max <= 0.0 { 0.0 } else { chroma / max };
        Self {
            h: hue.rem_euclid(360.0),
            s: saturation,
            v: max,
        }
    }

    /// The colour of these numbers with `alpha`, each channel rounded to the nearest byte. O(1).
    #[must_use]
    pub fn color(self, alpha: u8) -> Color {
        let (r, g, b) = self.rgb();
        Color::rgba(u32::from_be_bytes([byte(r), byte(g), byte(b), alpha]))
    }

    /// Red, green and blue, `0.0..=1.0`, by the usual sector formula.
    fn rgb(self) -> (f64, f64, f64) {
        let sector = self.h.rem_euclid(360.0) / 60.0;
        let chroma = self.v * self.s;
        let second = chroma * (1.0 - (sector.rem_euclid(2.0) - 1.0).abs());
        let base = self.v - chroma;
        let (red, green, blue) = match sector {
            s if s < 1.0 => (chroma, second, 0.0),
            s if s < 2.0 => (second, chroma, 0.0),
            s if s < 3.0 => (0.0, chroma, second),
            s if s < 4.0 => (0.0, second, chroma),
            s if s < 5.0 => (second, 0.0, chroma),
            _ => (chroma, 0.0, second),
        };
        (red + base, green + base, blue + base)
    }
}

/// `value` (`0.0..=1.0`) as a byte, rounded to the nearest.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "clamped to 0..=255 and rounded just before"
)]
fn byte(value: f64) -> u8 {
    (value * 255.0).round().clamp(0.0, 255.0) as u8
}

/// `color` premultiplied by its alpha, as pictures store pixels.
fn premultiplied(color: Color) -> [u8; 4] {
    let a = u16::from(color.a);
    let scale = |c: u8| -> u8 {
        // (c × a + 127) / 255, the rounded product: at most 255.
        u8::try_from((u16::from(c) * a + 127) / 255).unwrap_or(u8::MAX)
    };
    [scale(color.r), scale(color.g), scale(color.b), color.a]
}

/// A picture of `width × height` pixels whose pixel `(x, y)` is `pixel(x, y)` (straight colours,
/// premultiplied here). `None` only for a zero side (never asked).
fn picture(width: u32, height: u32, pixel: impl Fn(u32, u32) -> Color) -> Option<Image> {
    let mut data = Vec::with_capacity(4 * width as usize * height as usize);
    for y in 0..height {
        for x in 0..width {
            data.extend_from_slice(&premultiplied(pixel(x, y)));
        }
    }
    Image::from_premultiplied_rgba(width, height, data).ok()
}

/// Where `index` of `count` pixels lies along `0.0..=1.0` (the first at 0, the last at 1).
fn along(index: u32, count: u32) -> f64 {
    if count <= 1 {
        0.0
    } else {
        f64::from(index) / f64::from(count - 1)
    }
}

/// A drag in progress: the part, and where it started (relatively, as the slider's, so the fine
/// modifier can change speed without the colour jumping).
#[derive(Debug, Clone, Copy)]
struct Drag {
    /// The part dragged.
    part: PickerPart,
    /// The pointer at the anchor.
    at: Point,
    /// The numbers at the anchor.
    hsv: Hsv,
    /// The alpha at the anchor, `0.0..=1.0`.
    alpha: f64,
    /// Whether this stretch is fine.
    fine: bool,
    /// The pointer at the last move (the next anchor when the modifier changes).
    last: Point,
}

/// The colour picker (see the module documentation).
#[derive(Debug)]
pub struct ColorPicker<M> {
    /// The colour's numbers.
    hsv: Hsv,
    /// The colour's alpha, `0.0..=1.0` (a number, as the HSV ones, so a fine drag keeps its
    /// fractions).
    alpha: f64,
    /// The colour before the picker opened: the left swatch.
    original: Color,
    /// Whether the transparency strip is shown.
    show_alpha: bool,
    /// The message built from the new colour at each change.
    message: fn(Color) -> M,
    /// The part the keyboard acts on.
    part: PickerPart,
    /// The drag in progress, if any.
    drag: Option<Drag>,
    /// The square's picture and the hue it was made for (rebuilt only when the hue changes).
    square: Option<(f64, Image)>,
    /// The pictures changed since [`Widget::take_images`] last gave them.
    pictures_changed: bool,
    /// The names for assistive technologies: the picker's, then its parts' (already localised).
    names: [String; 4],
    /// The space between the parts, logical pixels: the density's small gap, kept from the last
    /// measure (events do not carry the spacing, and the parts must be found where they were
    /// drawn).
    gap: f32,
}

impl<M> ColorPicker<M> {
    /// A picker showing `color`, which is also the "before" swatch; `message` is built from the
    /// colour at each change. Without the transparency strip until [`ColorPicker::with_alpha`].
    #[must_use]
    pub fn new(color: Color, message: fn(Color) -> M) -> Self {
        Self {
            hsv: Hsv::of(color, 0.0),
            alpha: f64::from(color.a) / 255.0,
            original: color,
            show_alpha: false,
            message,
            part: PickerPart::Square,
            drag: None,
            square: None,
            pictures_changed: true,
            names: Default::default(),
            gap: 8.0,
        }
    }

    /// The same picker, with the transparency strip shown or not (choice 3: the application
    /// shows it for colours transparent in the theme's base).
    #[must_use]
    pub const fn with_alpha(mut self, show: bool) -> Self {
        self.show_alpha = show;
        self
    }

    /// The same picker, named for assistive technologies: `name` the picker's (the colour's
    /// name, "Accent color"), then the square's, the hue strip's and the transparency strip's
    /// (already localised).
    #[must_use]
    pub fn labelled(mut self, name: &str, square: &str, hue: &str, alpha: &str) -> Self {
        self.names = [
            name.to_owned(),
            square.to_owned(),
            hue.to_owned(),
            alpha.to_owned(),
        ];
        self
    }

    /// The colour shown, alpha included.
    #[must_use]
    pub fn color(&self) -> Color {
        self.hsv.color(byte(self.alpha))
    }

    /// The picker's numbers (tests and the application's display).
    #[must_use]
    pub const fn hsv(&self) -> Hsv {
        self.hsv
    }

    /// The part the keyboard acts on.
    #[must_use]
    pub const fn part(&self) -> PickerPart {
        self.part
    }

    /// Shows `color` (typed elsewhere, restored), keeping the hue the user was on when `color` is
    /// a grey; the caller asks the interface to record the picker again.
    pub fn set_color(&mut self, color: Color) {
        if color != self.color() {
            self.hsv = Hsv::of(color, self.hsv.h);
            self.alpha = f64::from(color.a) / 255.0;
            self.pictures_changed = true;
        }
    }

    /// The parts in order: the transparency strip only when shown.
    fn parts(&self) -> &'static [PickerPart] {
        if self.show_alpha {
            &[PickerPart::Square, PickerPart::Hue, PickerPart::Alpha]
        } else {
            &[PickerPart::Square, PickerPart::Hue]
        }
    }

    /// The rectangles of the parts inside `rect`: the square, the hue strip, the transparency
    /// strip (`None` when hidden) and the swatches, `gap` apart.
    fn layout(&self, rect: Rect, gap: f32) -> (Rect, Rect, Option<Rect>, Rect) {
        let (x0, x1) = (rect.x0, rect.x1);
        let square = Rect::new(x0, rect.y0, x1, rect.y0 + SQUARE_HEIGHT);
        let hue = Rect::new(x0, square.y1 + gap, x1, square.y1 + gap + STRIP_HEIGHT);
        let alpha = self
            .show_alpha
            .then(|| Rect::new(x0, hue.y1 + gap, x1, hue.y1 + gap + STRIP_HEIGHT));
        let below = alpha.map_or(hue.y1, |a| a.y1);
        let swatches = Rect::new(x0, below + gap, x1, below + gap + SWATCH_HEIGHT);
        (square, hue, alpha, swatches)
    }

    /// Changes the colour to `hsv` and `alpha` (clamped); tells the application and asks for a
    /// redraw only when the colour, or the numbers behind it, changed.
    fn change(&mut self, cx: &mut EventCx<'_, M>, hsv: Hsv, alpha: f64) {
        let hsv = Hsv {
            h: hsv.h.rem_euclid(360.0),
            s: hsv.s.clamp(0.0, 1.0),
            v: hsv.v.clamp(0.0, 1.0),
        };
        let alpha = if self.show_alpha {
            alpha.clamp(0.0, 1.0)
        } else {
            self.alpha
        };
        if hsv == self.hsv && alpha.to_bits() == self.alpha.to_bits() {
            return;
        }
        let before = self.color();
        self.hsv = hsv;
        self.alpha = alpha;
        self.pictures_changed = true;
        cx.request_record();
        let after = self.color();
        if after != before {
            cx.emit((self.message)(after));
        }
    }

    /// The part under `point` in the picker laid out in `rect`, if any.
    fn part_at(&self, rect: Rect, gap: f32, point: Point) -> Option<PickerPart> {
        let (square, hue, alpha, _) = self.layout(rect, gap);
        if inside(square, point) {
            Some(PickerPart::Square)
        } else if inside(hue, point) {
            Some(PickerPart::Hue)
        } else if alpha.is_some_and(|a| inside(a, point)) {
            Some(PickerPart::Alpha)
        } else {
            None
        }
    }

    /// The colour under `point` in `part` (the pointer's position, clamped to the part).
    fn at_point(&self, part: PickerPart, area: Rect, point: Point) -> (Hsv, f64) {
        let fx = f64::from(((point.x - area.x0) / area.width().max(1.0)).clamp(0.0, 1.0));
        let fy = f64::from(((point.y - area.y0) / area.height().max(1.0)).clamp(0.0, 1.0));
        match part {
            PickerPart::Square => (
                Hsv {
                    s: fx,
                    v: 1.0 - fy,
                    ..self.hsv
                },
                self.alpha,
            ),
            PickerPart::Hue => (
                Hsv {
                    h: (fx * 360.0).min(359.999),
                    ..self.hsv
                },
                self.alpha,
            ),
            PickerPart::Alpha => (self.hsv, fx),
        }
    }

    /// The area of `part` in the picker laid out in `rect`.
    fn area_of(&self, part: PickerPart, rect: Rect, gap: f32) -> Rect {
        let (square, hue, alpha, _) = self.layout(rect, gap);
        match part {
            PickerPart::Square => square,
            PickerPart::Hue => hue,
            PickerPart::Alpha => alpha.unwrap_or(hue),
        }
    }

    /// A left press at `point`: the part under it takes the keyboard too; the colour jumps to the
    /// pointer unless fine (V3); a drag starts.
    fn press(&mut self, cx: &mut EventCx<'_, M>, rect: Rect, gap: f32, point: Point) -> Handled {
        let Some(part) = self.part_at(rect, gap, point) else {
            return Handled::No;
        };
        self.part = part;
        cx.request_focus();
        let fine = cx.fine();
        if !fine {
            let (hsv, alpha) = self.at_point(part, self.area_of(part, rect, gap), point);
            self.change(cx, hsv, alpha);
        }
        self.drag = Some(Drag {
            part,
            at: point,
            hsv: self.hsv,
            alpha: self.alpha,
            fine,
            last: point,
        });
        cx.request_record();
        Handled::Yes
    }

    /// The pointer moved to `point` during a drag: the colour follows relatively to the anchor,
    /// at the plain or the fine speed; a change of the modifier anchors again (no jump).
    fn drag_to(&mut self, cx: &mut EventCx<'_, M>, rect: Rect, gap: f32, point: Point) {
        let Some(mut drag) = self.drag else {
            return;
        };
        let fine = cx.fine();
        if fine != drag.fine {
            drag = Drag {
                at: drag.last,
                hsv: self.hsv,
                alpha: self.alpha,
                fine,
                ..drag
            };
        }
        let area = self.area_of(drag.part, rect, gap);
        let speed = if fine { FINE_DRAG } else { 1.0 };
        let dx = f64::from(point.x - drag.at.x) / f64::from(area.width().max(1.0)) * speed;
        let dy = f64::from(point.y - drag.at.y) / f64::from(area.height().max(1.0)) * speed;
        let (hsv, alpha) = match drag.part {
            PickerPart::Square => (
                Hsv {
                    s: drag.hsv.s + dx,
                    v: drag.hsv.v - dy,
                    ..drag.hsv
                },
                drag.alpha,
            ),
            // The hue stops at the strip's ends while dragging (no wrap: the marker would jump
            // from one end to the other under the pointer).
            PickerPart::Hue => (
                Hsv {
                    h: (drag.hsv.h + dx * 360.0).clamp(0.0, 359.999),
                    ..drag.hsv
                },
                drag.alpha,
            ),
            PickerPart::Alpha => (drag.hsv, drag.alpha + dx),
        };
        drag.last = point;
        self.drag = Some(drag);
        self.change(cx, hsv, alpha);
    }

    /// A key on the part with the focus (see the module documentation); `Handled::No` for keys it
    /// does not use, and for Tab past the last part (the focus leaves the widget).
    fn key(&mut self, cx: &mut EventCx<'_, M>, key: NamedKey) -> Handled {
        let step = if cx.fine() { FINE } else { COARSE };
        let shift = cx.modifiers().shift;
        let (hsv, alpha) = (self.hsv, self.alpha);
        let (hsv, alpha) = match (self.part, key) {
            (_, NamedKey::Tab) => {
                let parts = self.parts();
                let index = parts.iter().position(|p| *p == self.part).unwrap_or(0);
                let next = if shift {
                    index.checked_sub(1)
                } else {
                    Some(index + 1)
                };
                return match next.and_then(|i| parts.get(i)) {
                    Some(&part) => {
                        self.part = part;
                        cx.request_record();
                        Handled::Yes
                    }
                    None => Handled::No,
                };
            }
            (PickerPart::Square, NamedKey::ArrowRight) => (
                Hsv {
                    s: hsv.s + step,
                    ..hsv
                },
                alpha,
            ),
            (PickerPart::Square, NamedKey::ArrowLeft) => (
                Hsv {
                    s: hsv.s - step,
                    ..hsv
                },
                alpha,
            ),
            (PickerPart::Square, NamedKey::ArrowUp) => (
                Hsv {
                    v: hsv.v + step,
                    ..hsv
                },
                alpha,
            ),
            (PickerPart::Square, NamedKey::ArrowDown) => (
                Hsv {
                    v: hsv.v - step,
                    ..hsv
                },
                alpha,
            ),
            (PickerPart::Square, NamedKey::Home) => (Hsv { s: 0.0, ..hsv }, alpha),
            (PickerPart::Square, NamedKey::End) => (Hsv { s: 1.0, ..hsv }, alpha),
            (PickerPart::Hue, NamedKey::ArrowRight | NamedKey::ArrowUp) => (
                Hsv {
                    h: hsv.h + step * 150.0,
                    ..hsv
                },
                alpha,
            ),
            (PickerPart::Hue, NamedKey::ArrowLeft | NamedKey::ArrowDown) => (
                Hsv {
                    h: hsv.h - step * 150.0,
                    ..hsv
                },
                alpha,
            ),
            (PickerPart::Hue, NamedKey::Home) => (Hsv { h: 0.0, ..hsv }, alpha),
            (PickerPart::Hue, NamedKey::End) => (Hsv { h: 359.0, ..hsv }, alpha),
            (PickerPart::Alpha, NamedKey::ArrowRight | NamedKey::ArrowUp) => (hsv, alpha + step),
            (PickerPart::Alpha, NamedKey::ArrowLeft | NamedKey::ArrowDown) => (hsv, alpha - step),
            (PickerPart::Alpha, NamedKey::Home) => (hsv, 0.0),
            (PickerPart::Alpha, NamedKey::End) => (hsv, 1.0),
            _ => return Handled::No,
        };
        self.change(cx, hsv, alpha);
        Handled::Yes
    }

    /// The part's value for assistive technologies, with its range and step: the square's
    /// intensity and the transparency in percent, the hue in degrees.
    fn access_value(&self) -> (f64, (f64, f64, f64)) {
        match self.part {
            PickerPart::Square => ((self.hsv.s * 100.0).round(), (0.0, 100.0, 1.0)),
            PickerPart::Hue => (self.hsv.h.round(), (0.0, 359.0, 1.0)),
            PickerPart::Alpha => ((self.alpha * 100.0).round(), (0.0, 100.0, 1.0)),
        }
    }

    /// Sets the part's value from assistive technologies (percent or degrees).
    fn set_access_value(&mut self, cx: &mut EventCx<'_, M>, value: f64) {
        let (hsv, alpha) = match self.part {
            PickerPart::Square => (
                Hsv {
                    s: value / 100.0,
                    ..self.hsv
                },
                self.alpha,
            ),
            PickerPart::Hue => (
                Hsv {
                    h: value.clamp(0.0, 359.0),
                    ..self.hsv
                },
                self.alpha,
            ),
            PickerPart::Alpha => (self.hsv, value / 100.0),
        };
        self.change(cx, hsv, alpha);
    }

    /// The square's picture for the hue `h`: x the saturation, y the value (top bright).
    fn square_picture(h: f64) -> Option<Image> {
        picture(SQUARE_PIXELS, SQUARE_PIXELS, |x, y| {
            Hsv {
                h,
                s: along(x, SQUARE_PIXELS),
                v: 1.0 - along(y, SQUARE_PIXELS),
            }
            .color(u8::MAX)
        })
    }
}

/// Whether `point` lies in `area` (edges included: a press on the square's last pixel row still
/// picks it).
fn inside(area: Rect, point: Point) -> bool {
    (area.x0..=area.x1).contains(&point.x) && (area.y0..=area.y1).contains(&point.y)
}

/// Draws a checkerboard of [`CHECKER`] squares over `area`, with two tokens (it shows through
/// the transparent colours).
fn checkerboard(recorder: &mut Recorder<'_>, area: Rect) {
    recorder.fill_rect(area, ColorToken::SurfaceRaised);
    let mut y = area.y0;
    let mut row = 0_u32;
    while y < area.y1 {
        let mut x = area.x0 + if row.is_multiple_of(2) { 0.0 } else { CHECKER };
        while x < area.x1 {
            let cell = Rect::new(x, y, (x + CHECKER).min(area.x1), (y + CHECKER).min(area.y1));
            recorder.fill_rect(
                cell,
                Paint {
                    token: ColorToken::ControlBorder,
                    opacity: 96,
                },
            );
            x += 2.0 * CHECKER;
        }
        y += CHECKER;
        row += 1;
    }
}

impl<M: std::fmt::Debug + 'static> Widget<M> for ColorPicker<M> {
    fn measure(&mut self, cx: &mut LayoutCx<'_, M>, constraints: Constraints) -> Size {
        let gap = cx.spacing().gap_s;
        self.gap = gap;
        let width = if constraints.max.width.is_finite() {
            PREFERRED_WIDTH.min(constraints.max.width)
        } else {
            PREFERRED_WIDTH
        };
        let strips = if self.show_alpha { 2.0 } else { 1.0 };
        let height = SQUARE_HEIGHT + strips * (gap + STRIP_HEIGHT) + gap + SWATCH_HEIGHT;
        constraints.constrain(Size::new(width, height))
    }

    fn take_images(&mut self) -> Option<Vec<Image>> {
        if !self.pictures_changed {
            return None;
        }
        self.pictures_changed = false;
        let h = self.hsv.h;
        let square = match self.square.take() {
            Some((made_for, image)) if made_for.to_bits() == h.to_bits() => image,
            _ => Self::square_picture(h)?,
        };
        self.square = Some((h, square.clone()));
        let hue = picture(STRIP_PIXELS, 1, |x, _| {
            Hsv {
                h: along(x, STRIP_PIXELS) * 360.0,
                s: 1.0,
                v: 1.0,
            }
            .color(u8::MAX)
        })?;
        let opaque = self.hsv.color(u8::MAX);
        let alpha = picture(STRIP_PIXELS, 1, |x, _| Color {
            a: byte(along(x, STRIP_PIXELS)),
            ..opaque
        })?;
        let (before, now) = (self.original, self.color());
        let swatches = picture(STRIP_PIXELS, 1, |x, _| {
            if x < STRIP_PIXELS / 2 { before } else { now }
        })?;
        Some(vec![square, hue, alpha, swatches])
    }

    fn record(&self, cx: &RecordCx, recorder: &mut Recorder<'_>) {
        let gap = cx.spacing.gap_s;
        let (square, hue, alpha, swatches) = self.layout(cx.rect, gap);
        let [square_image, hue_image, alpha_image, swatch_image] = cx.images;
        let outline = |recorder: &mut Recorder<'_>, area: Rect| {
            recorder.stroke_rect(
                area.inflate(-0.5),
                StrokeStyle::new(1.0),
                ColorToken::ControlBorder,
            );
        };
        // The square, with a two-line ring at the colour: dark inside light, visible on any
        // colour.
        if let Some(id) = square_image {
            recorder.image(id, square);
        }
        outline(recorder, square);
        #[allow(
            clippy::cast_possible_truncation,
            reason = "fractions in 0..=1 of sizes in pixels: f32 is plenty"
        )]
        let at = Point::new(
            square.x0 + self.hsv.s as f32 * square.width(),
            square.y0 + (1.0 - self.hsv.v) as f32 * square.height(),
        );
        let ring = Rect::new(at.x - 6.0, at.y - 6.0, at.x + 6.0, at.y + 6.0);
        recorder.stroke_rounded_rect(
            RoundedRect::new(ring, 6.0),
            StrokeStyle::new(2.0),
            ColorToken::SurfaceRaised,
        );
        recorder.stroke_rounded_rect(
            RoundedRect::new(ring.inflate(1.5), 7.5),
            StrokeStyle::new(1.0),
            ColorToken::Text,
        );
        // A strip with its marker: a bar 4 px wide, light inside a dark line.
        let marker = |recorder: &mut Recorder<'_>, area: Rect, fraction: f64| {
            #[allow(
                clippy::cast_possible_truncation,
                reason = "a fraction in 0..=1 of a width in pixels"
            )]
            let x = area.x0 + fraction as f32 * area.width();
            let bar = Rect::new(x - 2.0, area.y0 - 2.0, x + 2.0, area.y1 + 2.0);
            recorder.fill_rect(bar, ColorToken::SurfaceRaised);
            recorder.stroke_rect(bar.inflate(0.5), StrokeStyle::new(1.0), ColorToken::Text);
        };
        if let Some(id) = hue_image {
            recorder.image(id, hue);
        }
        outline(recorder, hue);
        marker(recorder, hue, self.hsv.h / 360.0);
        if let Some(area) = alpha {
            checkerboard(recorder, area);
            if let Some(id) = alpha_image {
                recorder.image(id, area);
            }
            outline(recorder, area);
            marker(recorder, area, self.alpha);
        }
        // The swatches: before on the left, now on the right, a line between them.
        checkerboard(recorder, swatches);
        if let Some(id) = swatch_image {
            recorder.image(id, swatches);
        }
        let middle = swatches.x0 + swatches.width() / 2.0;
        recorder.line(
            Point::new(middle, swatches.y0),
            Point::new(middle, swatches.y1),
            StrokeStyle::new(1.0),
            ColorToken::ControlBorder,
        );
        outline(recorder, swatches);
        if cx.focused {
            let area = match self.part {
                PickerPart::Square => square,
                PickerPart::Hue => hue,
                PickerPart::Alpha => alpha.unwrap_or(hue),
            };
            super::look::focus_ring(recorder, area, 0.0, cx.shape);
        }
    }

    fn event(&mut self, cx: &mut EventCx<'_, M>, event: &WidgetEvent<'_>) -> Handled {
        let rect = cx.rect();
        let gap = self.gap;
        match *event {
            WidgetEvent::PointerButton {
                button: PointerButton::Left,
                state: ButtonState::Pressed,
                position,
                ..
            } => self.press(cx, rect, gap, position),
            WidgetEvent::PointerMoved { position } => {
                self.drag_to(cx, rect, gap, position);
                Handled::Yes
            }
            WidgetEvent::PointerButton {
                button: PointerButton::Left,
                state: ButtonState::Released,
                ..
            } => {
                if self.drag.take().is_some() {
                    cx.request_record();
                }
                Handled::Yes
            }
            WidgetEvent::FocusGained => {
                // Entered backwards (Shift+Tab from after it): the last part; otherwise the
                // first, unless a press chose one.
                if self.drag.is_none() {
                    let parts = self.parts();
                    self.part = if cx.modifiers().shift {
                        parts.last().copied().unwrap_or(PickerPart::Square)
                    } else {
                        PickerPart::Square
                    };
                }
                cx.request_record();
                Handled::Yes
            }
            WidgetEvent::FocusLost => {
                cx.request_record();
                Handled::Yes
            }
            WidgetEvent::Key {
                key: Key::Named(key),
                state: ButtonState::Pressed,
                ..
            } => self.key(cx, *key),
            WidgetEvent::Increment => {
                let (value, (_, _, step)) = self.access_value();
                self.set_access_value(cx, value + step);
                Handled::Yes
            }
            WidgetEvent::Decrement => {
                let (value, (_, _, step)) = self.access_value();
                self.set_access_value(cx, value - step);
                Handled::Yes
            }
            WidgetEvent::SetValue(value) => {
                self.set_access_value(cx, value);
                Handled::Yes
            }
            _ => Handled::No,
        }
    }

    fn focusable(&self) -> bool {
        true
    }

    fn access_role(&self) -> AccessRole {
        AccessRole::Slider
    }

    fn access_info(&self, info: &mut AccessInfo) {
        let part = match self.part {
            PickerPart::Square => &self.names[1],
            PickerPart::Hue => &self.names[2],
            PickerPart::Alpha => &self.names[3],
        };
        // The picker's name then the part's, as a table's cell is read (row, column): both
        // already translated, joined as the Playback section's sliders are.
        info.label = Some(match (self.names[0].is_empty(), part.is_empty()) {
            (true, _) => part.clone(),
            (false, true) => self.names[0].clone(),
            (false, false) => format!("{}, {part}", self.names[0]),
        });
        let (value, range) = self.access_value();
        info.value = Some(value);
        info.range = Some(range);
        let c = self.color();
        info.description = Some(if c.a == u8::MAX {
            format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
        } else {
            format!("#{:02x}{:02x}{:02x}{:02x}", c.r, c.g, c.b, c.a)
        });
    }
}
