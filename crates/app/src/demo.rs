// SPDX-License-Identifier: GPL-3.0-or-later

//! The demonstration window of ADR-0040 W8: what `cargo run -p orchestra-app` shows until the
//! real interface arrives with the widgets (F2.08 onwards).
//!
//! It draws with the commands of ADR-0039 and text (ADR-0044) and exists to check by eye:
//! - **presentation and damage:** a disc follows the pointer, re-recording one segment per move,
//!   so only its old and new place are redrawn;
//! - **input:** the disc turns to the accent colour while a button is held; four squares light up
//!   with the primary modifier, the secondary one, Shift and Alt;
//! - **crisp lines and scale:** a staff of five 1 px lines with Bravura noteheads, barlines and bar
//!   numbers, which must stay sharp at every scale factor;
//! - **text:** the program's name as the panel's title (the `heading` size, bold) and the bar
//!   numbers (the `numeric` family, tabular figures), from the glyph cache (F2.07c);
//! - **following the system's theme:** switching the desktop between light and dark while the
//!   window is open redraws it in the other half of the pair (ADR-0038, F2.04b);
//! - **shapes and the theme:** a raised panel with its shadow and three controls in the theme's
//!   colours, at the window's size (resizing lays it out again).
//!
//! Nothing here is business logic (the crate's rule): it is a throw-away view, replaced as soon as
//! real views exist.

use orchestra_text::{FaceId, Faces, GlyphBox, GlyphId, Shaper, TextLayout};
use orchestra_ui::{
    ButtonState, ColorToken, DisplayList, FamilyToken, InputEvent, Modifiers, Paint, Point, Rect,
    RoundedRect, SegmentId, SizeToken, StrokeStyle, Typography, Weight, WindowApp,
};

/// The panel with its shadow and controls.
const PANEL: SegmentId = SegmentId(1);
/// The staff with its noteheads.
const STAFF: SegmentId = SegmentId(2);
/// The four modifier squares.
const MODIFIERS: SegmentId = SegmentId(3);
/// The disc under the pointer, drawn last so it is on top.
const POINTER: SegmentId = SegmentId(4);

/// Margin around the panel, logical pixels.
const MARGIN: f32 = 24.0;
/// Radius of the pointer's disc, logical pixels.
const DISC: f32 = 14.0;

/// The staff space of the demonstration staff, logical pixels: 10, so Bravura is set at 40 px
/// per em (an em is four staff spaces in SMuFL).
const SPACE: f32 = 10.0;

/// How many notes a bar of the demonstration staff holds.
const NOTES_PER_BAR: u8 = 4;

/// The demonstration view's state.
#[derive(Debug)]
pub struct Demo {
    /// The program's name, already localised by the caller (no text in code, CLAUDE.md §7).
    title: TextLayout,
    /// The bar numbers 1, 2, 3, laid out once.
    bar_numbers: [TextLayout; 3],
    /// The black notehead of Bravura (U+E0A4) and its own box at 40 px per em, or `None` if the
    /// font lacks it (it does not: a test checks), in which case no notehead is drawn.
    notehead: Option<(GlyphId, GlyphBox)>,
    /// The window's size, logical pixels.
    bounds: Rect,
    /// Where the pointer is, if it is inside the window.
    pointer: Option<Point>,
    /// Whether a pointer button is held.
    pressed: bool,
    /// The modifiers held.
    modifiers: Modifiers,
}

impl Demo {
    /// A demonstration view titled `title` (the localised program name), laying out its texts
    /// with `faces` in the default typography (ADR-0044 TX11).
    ///
    /// # Why the default typography and not the window's theme
    /// `WindowApp::layout` does not receive the theme yet (who owns the widgets is decided in
    /// F2.08), and every built-in pair has the same text tokens, so the default is what any
    /// window would use today.
    ///
    /// A text that cannot be laid out (impossible with the bundled faces) is left empty rather
    /// than failing the window: this is a throw-away view.
    #[must_use]
    pub fn new(faces: &Faces, title: &str) -> Self {
        let typography = Typography::DEFAULT;
        let mut shaper = Shaper::new();
        let mut lay_out = |text: &str, family, size, weight| {
            let mut layout = TextLayout::new();
            let style = typography.text_style(family, size, weight);
            if shaper
                .layout(faces, text, style, None, &mut layout)
                .is_err()
            {
                layout = TextLayout::new();
            }
            layout
        };
        let title = lay_out(title, FamilyToken::Ui, SizeToken::Heading, Weight::Bold);
        let bar_numbers = ["1", "2", "3"].map(|number| {
            lay_out(
                number,
                FamilyToken::Numeric,
                SizeToken::Caption,
                Weight::Regular,
            )
        });
        let size = 4.0 * SPACE;
        let notehead = faces
            .glyph_for_char(FaceId::Bravura, '\u{E0A4}')
            .and_then(|glyph| {
                // The notehead's own box, not the face's: Bravura's box is 2.75 em wide and
                // would make every edit redraw far more than the staff (ADR-0044 TD2).
                let own = faces
                    .glyph_box(FaceId::Bravura, glyph, size)
                    .ok()
                    .flatten()?;
                Some((glyph, own))
            });
        Self {
            title,
            bar_numbers,
            notehead,
            bounds: Rect::default(),
            pointer: None,
            pressed: false,
            modifiers: Modifiers::default(),
        }
    }

    /// The panel: a raised surface with a shadow, and three controls. Sized to the window, up to
    /// 520 × 260.
    fn record_panel(&self, list: &mut DisplayList) {
        let width = (self.bounds.width() - 2.0 * MARGIN).clamp(0.0, 520.0);
        let height = (self.bounds.height() - 2.0 * MARGIN).clamp(0.0, 260.0);
        let panel = RoundedRect::new(Rect::from_origin_size(MARGIN, MARGIN, width, height), 10.0);
        let title = &self.title;
        list.record(PANEL, |r| {
            // The shadow, offset downwards as ADR-0037's shadow tokens describe.
            let shadow = RoundedRect::new(
                Rect::from_origin_size(MARGIN, MARGIN + 4.0, width, height),
                10.0,
            );
            r.shadow(shadow, 8.0, ColorToken::Shadow);
            r.fill_rounded_rect(panel, ColorToken::SurfaceRaised);
            r.stroke_rounded_rect(panel, StrokeStyle::new(1.0), ColorToken::Border);
            // Three controls: a plain one, a focused one, the accent one.
            for (index, token) in
                (0_u8..).zip([ColorToken::Control, ColorToken::Control, ColorToken::Accent])
            {
                let x = MARGIN + 20.0 + f32::from(index) * 110.0;
                let control =
                    RoundedRect::new(Rect::from_origin_size(x, MARGIN + 20.0, 96.0, 32.0), 6.0);
                r.fill_rounded_rect(control, token);
                r.stroke_rounded_rect(control, StrokeStyle::new(1.0), ColorToken::ControlBorder);
                if index == 1 {
                    let ring = RoundedRect::new(control.rect.inflate(3.0), 9.0);
                    r.stroke_rounded_rect(ring, StrokeStyle::new(2.0), ColorToken::Focus);
                }
            }
            // The title, right of the controls, its line centred on theirs (they are 32 px
            // tall from y = MARGIN + 20).
            let top = MARGIN + 20.0 + (32.0 - title.height()) / 2.0;
            r.text(title, Point::new(MARGIN + 360.0, top), ColorToken::Text);
        });
    }

    /// A staff of five crisp lines with Bravura noteheads, stems, barlines and bar numbers,
    /// inside the panel.
    fn record_staff(&self, list: &mut DisplayList) {
        let left = MARGIN + 20.0;
        let top = MARGIN + 90.0;
        let bar_width = f32::from(NOTES_PER_BAR) * 34.0;
        let notehead = self.notehead;
        let numbers = &self.bar_numbers;
        list.record(STAFF, |r| {
            r.push_clip_rect(Rect::from_origin_size(left, top - 40.0, 480.0, 120.0));
            for line in 0..5_u8 {
                let y = top + f32::from(line) * SPACE;
                r.line(
                    Point::new(left, y),
                    Point::new(left + 440.0, y),
                    StrokeStyle::new(1.0),
                    ColorToken::Text,
                );
            }
            // Three bars of four notes after a 14 px lead-in, each with its barline and its
            // number above the staff's top line, at the bar's start.
            for (bar, number) in (0_u8..).zip(numbers) {
                let start = left + 14.0 + f32::from(bar) * bar_width;
                let barline = start + bar_width;
                r.line(
                    Point::new(barline, top),
                    Point::new(barline, top + 4.0 * SPACE),
                    StrokeStyle::new(1.0),
                    ColorToken::Text,
                );
                let above = top - 6.0 - number.height();
                r.text(
                    number,
                    Point::new(start + 2.0, above),
                    ColorToken::TextMuted,
                );
            }
            let notes = (0..3 * NOTES_PER_BAR).map(|note| {
                let x = left + 24.0 + f32::from(note) * 34.0;
                let y = top + 4.0 * SPACE - f32::from(note % 9) * SPACE * 0.5;
                (x, y)
            });
            if let Some((glyph, own)) = notehead {
                r.glyphs(
                    FaceId::Bravura,
                    4.0 * SPACE,
                    own,
                    notes.clone().map(|(x, y)| (glyph, Point::new(x, y))),
                    ColorToken::Text,
                );
            }
            // Stems up, on the notehead's right side (SMuFL's stemUpSE anchor of a black
            // notehead is 1.18 staff spaces right of its origin), three and a half spaces long.
            for (x, y) in notes {
                let stem = x + 1.18 * SPACE - 0.5;
                r.line(
                    Point::new(stem, y - 0.168 * SPACE),
                    Point::new(stem, y - 3.5 * SPACE),
                    StrokeStyle::new(1.0),
                    ColorToken::Text,
                );
            }
            r.pop_clip();
        });
    }

    /// Four squares: primary, secondary, Shift, Alt; filled with the accent while held.
    fn record_modifiers(&self, list: &mut DisplayList) {
        let held = [
            self.modifiers.primary,
            self.modifiers.secondary,
            self.modifiers.shift,
            self.modifiers.alt,
        ];
        list.record(MODIFIERS, |r| {
            for (index, on) in (0_u8..).zip(held) {
                let square = RoundedRect::new(
                    Rect::from_origin_size(
                        MARGIN + 20.0 + f32::from(index) * 36.0,
                        MARGIN + 200.0,
                        24.0,
                        24.0,
                    ),
                    4.0,
                );
                if on {
                    r.fill_rounded_rect(square, ColorToken::Accent);
                }
                r.stroke_rounded_rect(square, StrokeStyle::new(1.0), ColorToken::ControlBorder);
            }
        });
    }

    /// The disc under the pointer (nothing when the pointer is outside).
    fn record_pointer(&self, list: &mut DisplayList) {
        let pointer = self.pointer;
        let paint = if self.pressed {
            Paint::token(ColorToken::Accent)
        } else {
            Paint::with_opacity(ColorToken::Text, 160)
        };
        list.record(POINTER, |r| {
            if let Some(at) = pointer {
                let disc = Rect::new(at.x - DISC, at.y - DISC, at.x + DISC, at.y + DISC);
                r.fill_rounded_rect(RoundedRect::new(disc, DISC), paint);
            }
        });
    }
}

impl WindowApp for Demo {
    /// Records everything for the window's new size.
    fn layout(&mut self, bounds: Rect, list: &mut DisplayList) {
        self.bounds = bounds;
        self.record_panel(list);
        self.record_staff(list);
        self.record_modifiers(list);
        self.record_pointer(list);
    }

    /// Re-records only what the event changes: the disc, or the modifier squares.
    fn input(&mut self, event: &InputEvent, list: &mut DisplayList) {
        match event {
            InputEvent::PointerMoved { position } => {
                self.pointer = Some(*position);
                self.record_pointer(list);
            }
            InputEvent::PointerLeft => {
                self.pointer = None;
                self.record_pointer(list);
            }
            InputEvent::PointerButton { state, .. } => {
                self.pressed = *state == ButtonState::Pressed;
                self.record_pointer(list);
            }
            InputEvent::ModifiersChanged { modifiers } | InputEvent::Key { modifiers, .. } => {
                if *modifiers != self.modifiers {
                    self.modifiers = *modifiers;
                    self.record_modifiers(list);
                }
            }
            InputEvent::Wheel { .. } | InputEvent::Focus(_) => {}
        }
    }
}
