// SPDX-License-Identifier: GPL-3.0-or-later

//! Our own title bar (F2.11b2, ADR-0049), drawn only where the system draws no frame (TB1: on
//! Wayland when the compositor gives no decorations, which is GNOME's case).
//!
//! # What it holds (TB4, TB5)
//! One row, as high as the menu bar: the window buttons that go on the leading side (GNOME's
//! `button-layout` may put them there, F2.11b3), then the **menus** (the drawn menu bar of
//! ADR-0048, in its compact form), then the **title** in the free space, then the trailing
//! window buttons (by default only Close, GNOME's default). The title is centred on the whole
//! window when it fits there, otherwise in the free space; when even the free space is too
//! narrow it is not drawn (the window's name is still the accessible name of the bar).
//!
//! # What it does (TB6)
//! - A press of the primary button on the **free space** (the title included, and the menu
//!   bar's small margin) and a move of more than [`DRAG_THRESHOLD`] logical pixels asks the window
//!   to **drag** (`WindowRequest::Drag`): waiting for the move, as GTK does, keeps a double click
//!   possible, which a drag started at the press would swallow (the compositor takes the pointer).
//! - A **double click** there maximises or restores (`WindowRequest::ToggleMaximized`).
//! - A **secondary click** there shows the system's window menu at the pointer
//!   (`WindowRequest::ShowWindowMenu`).
//! - The **buttons** close (`WindowRequest::RequestClose`, the same way as the system's close
//!   button: the window asks the application first, which may ask to save the changes, F2.12a5b),
//!   maximise or restore, and minimise, on release over the button, as every button.
//!
//! # Full screen (a precision of TB6 proposed in F2.11b2)
//! The title and the buttons are not drawn and the free space does nothing; the menus stay, as
//! the menu bar under a system frame stays in full screen, so the program's menus keep their
//! place and the pointer still reaches them.
//!
//! # Looks (TB8)
//! The background is `surface`, with a `border` line under it, like the menu bar; the title is
//! `text`, or `text-muted` while the window does not have the focus, like every system's frame.
//! A button's symbol (×, □, two squares for Restore, _) is drawn with lines, no icon font, in the
//! title's colour; under the pointer the button gets a round `control-hover` background, pressed
//! `control-pressed`.
//!
//! # Accessibility (TB9)
//! The bar is a `TitleBar` node named with the window's title; the buttons are `Button` nodes
//! with their names and the click action, **outside the Tab chain** (window management is the
//! compositor's from the keyboard).
//!
//! # Cost
//! A focus change of the window records the bar and its buttons again, nothing else (their
//! looks change, not their sizes); entering or leaving full screen lays the bar out again (the
//! buttons' sizes change). No allocation after the bar exists, except laying out the title again
//! when the text generation changes (a new theme or language).

use crate::WindowRequest;
use crate::draw::geometry::{Point, Rect, RoundedRect};
use crate::draw::list::Recorder;
use crate::draw::paint::StrokeStyle;
use crate::theme::text::{FamilyToken, SizeToken, Weight};
use crate::theme::token::ColorToken;
use crate::window::input::{ButtonState, PointerButton};
use orchestra_text::TextLayout;

use super::access::{AccessInfo, AccessRole};
use super::layout::{Constraints, Size};
use super::tree::{EventCx, Handled, LayoutCx, RecordCx, Widget, WidgetEvent};

/// The style of the title: the interface family, body size, regular weight (the menus' own).
const STYLE: (FamilyToken, SizeToken, Weight) = (FamilyToken::Ui, SizeToken::Body, Weight::Regular);

/// How far the pointer must move, logical pixels, after a press on the free space before the
/// window is dragged (see the module documentation). GTK's default drag threshold is 8 device
/// pixels; 4 logical pixels is the same at a scale of 2 and a little more eager at 1.
pub(crate) const DRAG_THRESHOLD: f32 = 4.0;

/// A button of the window (ADR-0049 TB4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WindowButton {
    /// Minimise.
    Minimize,
    /// Maximise, or restore while maximised.
    Maximize,
    /// Close.
    Close,
}

/// The names of the window buttons, already localised (CLAUDE.md §7): what assistive technologies
/// read, and the buttons' tooltips.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WindowButtonNames {
    /// "Close".
    pub close: String,
    /// "Maximise".
    pub maximize: String,
    /// "Restore", the Maximise button's name while the window is maximised.
    pub restore: String,
    /// "Minimise".
    pub minimize: String,
}

/// What [`Ui::add_title_bar`](crate::Ui::add_title_bar) builds (see the module documentation).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TitleBarSpec {
    /// The title, already localised: the program's name (later the project's too).
    pub title: String,
    /// The buttons before the menus, in order (GNOME's `button-layout` before its colon).
    pub leading: Vec<WindowButton>,
    /// The buttons after the title, in order (after the colon; by default only Close).
    pub trailing: Vec<WindowButton>,
    /// The buttons' names.
    pub names: WindowButtonNames,
}

impl TitleBarSpec {
    /// GNOME's default layout (`appmenu:close`, ADR-0049 TB4): only Close, on the trailing side.
    #[must_use]
    pub fn gnome_default(title: String, names: WindowButtonNames) -> Self {
        Self {
            title,
            leading: Vec::new(),
            trailing: vec![WindowButton::Close],
            names,
        }
    }

    /// The bar for GNOME's `button-layout` text `setting` (F2.11b3), as
    /// [`button_layout`](crate::button_layout) read it: `None` (not readable) gives
    /// [`TitleBarSpec::gnome_default`].
    #[must_use]
    pub fn from_setting(setting: Option<&str>, title: String, names: WindowButtonNames) -> Self {
        let Some(setting) = setting else {
            return Self::gnome_default(title, names);
        };
        let (leading, trailing) = crate::platform::decorations::layout::parse(setting);
        Self {
            title,
            leading,
            trailing,
            names,
        }
    }
}

/// The colour of the title and of the buttons' symbols: `text`, or `text-muted` while the window
/// does not have the focus (TB8).
const fn ink(focused: bool) -> ColorToken {
    if focused {
        ColorToken::Text
    } else {
        ColorToken::TextMuted
    }
}

/// The bar itself (see the module documentation): its children are the leading buttons, the
/// compact menu bar, then the trailing buttons, in that order.
#[derive(Debug)]
pub(crate) struct TitleBar {
    /// The title, already localised.
    title: String,
    /// The laid-out title.
    layout: TextLayout,
    /// The text generation the layout was made for.
    laid_out_for: Option<u64>,
    /// How many leading buttons come before the menu bar among the children.
    leading: usize,
    /// The free space of the last `arrange`: between the menu bar (or the leading buttons) and
    /// the trailing buttons.
    free: Rect,
    /// The window has the keyboard focus.
    focused: bool,
    /// The window is in full screen: no title, no dragging.
    fullscreen: bool,
    /// Where the primary button was pressed on the free space, until it moves far enough to drag
    /// or is released.
    press: Option<Point>,
}

impl TitleBar {
    /// A bar titled `title` with `leading` buttons before its menus.
    pub(crate) const fn new(title: String, leading: usize) -> Self {
        Self {
            title,
            layout: TextLayout::new(),
            laid_out_for: None,
            leading,
            free: Rect::new(0.0, 0.0, 0.0, 0.0),
            focused: true,
            fullscreen: false,
            press: None,
        }
    }

    /// Changes the title (F2.12a5b: the project's name and its modification mark follow the open
    /// project). Answers whether it changed: then the bar must be laid out again (the title's
    /// width moves where it is centred), recorded again, and its accessible node rebuilt (the
    /// title is its name). The text is laid out again at the next measure, by forgetting the
    /// generation it was laid out for. O(title length) for the comparison and the copy.
    pub(crate) fn set_title(&mut self, title: &str) -> bool {
        if self.title == title {
            return false;
        }
        title.clone_into(&mut self.title);
        self.laid_out_for = None;
        true
    }

    /// Sets the window's focus and full screen. Answers `(record, layout)`: whether the bar must
    /// be recorded again (its title's colour) and laid out again (full screen changes the
    /// buttons' sizes).
    pub(crate) const fn set_window(&mut self, focused: bool, fullscreen: bool) -> (bool, bool) {
        let record = self.focused != focused || self.fullscreen != fullscreen;
        let layout = self.fullscreen != fullscreen;
        self.focused = focused;
        self.fullscreen = fullscreen;
        if fullscreen {
            self.press = None;
        }
        (record, layout)
    }

    /// Where the title goes in `rect` (the bar's): centred on the bar when it fits in the free
    /// space there, otherwise centred in the free space; `None` when the free space is narrower
    /// than the title.
    fn title_origin(&self, rect: Rect) -> Option<Point> {
        let width = self.layout.width();
        if width <= 0.0 || width > self.free.width() {
            return None;
        }
        let y = rect.y0 + (rect.height() - self.layout.height()) / 2.0;
        // Centred on the whole bar, as GNOME centres its titles, if that stays in the free space.
        let centred = rect.x0 + (rect.width() - width) / 2.0;
        let x = if centred >= self.free.x0 && centred + width <= self.free.x1 {
            centred
        } else {
            self.free.x0 + (self.free.width() - width) / 2.0
        };
        Some(Point::new(x, y))
    }
}

impl<M: 'static> Widget<M> for TitleBar {
    fn measure(&mut self, cx: &mut LayoutCx<'_, M>, constraints: Constraints) -> Size {
        let generation = cx.text_generation();
        if self.laid_out_for != Some(generation) {
            if cx
                .lay_out_text(&self.title, STYLE, None, &mut self.layout)
                .is_err()
            {
                self.layout = TextLayout::new();
            }
            self.laid_out_for = Some(generation);
        }
        let mut height = cx.spacing().control_height;
        for child in 0..cx.child_count() {
            let size = cx.measure_child(child, Constraints::loose(constraints.max));
            height = height.max(size.height);
        }
        // It spans the window, as high as its highest child (the menu bar, a control).
        constraints.constrain(Size::new(constraints.max.width, height))
    }

    fn arrange(&mut self, cx: &mut LayoutCx<'_, M>, rect: Rect) {
        let count = cx.child_count();
        // Leading buttons and the menu bar from the leading edge…
        let mut x = rect.x0;
        for child in 0..=self.leading.min(count.saturating_sub(1)) {
            let width = cx.child_size(child).width;
            cx.place_child(child, Rect::new(x, rect.y0, x + width, rect.y1));
            x += width;
        }
        // …the trailing buttons from the trailing edge, keeping their order.
        let mut end = rect.x1;
        for child in (self.leading + 1..count).rev() {
            let width = cx.child_size(child).width;
            cx.place_child(child, Rect::new(end - width, rect.y0, end, rect.y1));
            end -= width;
        }
        self.free = Rect::new(x, rect.y0, end.max(x), rect.y1);
    }

    fn record(&self, cx: &RecordCx, recorder: &mut Recorder<'_>) {
        let rect = cx.rect;
        recorder.fill_rect(rect, ColorToken::Surface);
        let width = f32::from(cx.shape.line_width.max(1));
        let y = rect.y1 - width / 2.0;
        recorder.line(
            Point::new(rect.x0, y),
            Point::new(rect.x1, y),
            StrokeStyle::new(width),
            ColorToken::Border,
        );
        if !self.fullscreen
            && let Some(origin) = self.title_origin(rect)
        {
            recorder.text(&self.layout, origin, ink(self.focused));
        }
    }

    fn event(&mut self, cx: &mut EventCx<'_, M>, event: &WidgetEvent<'_>) -> Handled {
        // Everything that reaches the bar itself happened on its free space (a child that wanted
        // it kept it): the drag, the double click, the window menu.
        if self.fullscreen {
            return Handled::No;
        }
        match *event {
            WidgetEvent::PointerButton {
                button: PointerButton::Left,
                state: ButtonState::Pressed,
                position,
                clicks,
            } => {
                if clicks >= 2 {
                    self.press = None;
                    cx.request_window(WindowRequest::ToggleMaximized);
                } else {
                    self.press = Some(position);
                }
                Handled::Yes
            }
            WidgetEvent::PointerMoved { position } => {
                if let Some(start) = self.press
                    && (position.x - start.x).hypot(position.y - start.y) > DRAG_THRESHOLD
                {
                    // The compositor moves the window from here and takes the pointer until the
                    // release; the press is done with.
                    self.press = None;
                    cx.request_window(WindowRequest::Drag);
                }
                Handled::Yes
            }
            WidgetEvent::PointerButton {
                button: PointerButton::Left,
                state: ButtonState::Released,
                ..
            } => {
                self.press = None;
                Handled::Yes
            }
            WidgetEvent::PointerButton {
                button: PointerButton::Right,
                state: ButtonState::Pressed,
                position,
                ..
            } => {
                cx.request_window(WindowRequest::ShowWindowMenu { at: position });
                Handled::Yes
            }
            _ => Handled::No,
        }
    }

    fn access_role(&self) -> AccessRole {
        AccessRole::TitleBar
    }

    fn access_info(&self, info: &mut AccessInfo) {
        info.label = Some(self.title.clone());
    }
}

/// One button of the window (see the module documentation).
#[derive(Debug)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "five independent states (hovered, pressed, maximised, focused, full screen) set by \
              different events; an enum would have to list their combinations"
)]
pub(crate) struct WindowButtonWidget {
    /// Which button.
    kind: WindowButton,
    /// Its names (Maximise's changes while maximised).
    names: WindowButtonNames,
    /// The pointer is over it.
    hovered: bool,
    /// The primary button was pressed on it and not released yet.
    pressed: bool,
    /// The window is maximised: Maximise shows and says Restore.
    maximized: bool,
    /// The window has the keyboard focus.
    focused: bool,
    /// The window is in full screen: the button takes no room and draws nothing.
    fullscreen: bool,
}

impl WindowButtonWidget {
    /// The button `kind`, named from `names`.
    pub(crate) const fn new(kind: WindowButton, names: WindowButtonNames) -> Self {
        Self {
            kind,
            names,
            hovered: false,
            pressed: false,
            maximized: false,
            focused: true,
            fullscreen: false,
        }
    }

    /// Sets the window's state. Answers `(record, layout, access)`: whether the button must be
    /// recorded again, laid out again (full screen changes its size) and its accessible node
    /// rebuilt (Maximise becomes Restore).
    pub(crate) const fn set_window(
        &mut self,
        maximized: bool,
        focused: bool,
        fullscreen: bool,
    ) -> (bool, bool, bool) {
        let access = self.maximized != maximized && matches!(self.kind, WindowButton::Maximize);
        let record = access || self.focused != focused || self.fullscreen != fullscreen;
        let layout = self.fullscreen != fullscreen;
        self.maximized = maximized;
        self.focused = focused;
        self.fullscreen = fullscreen;
        (record, layout, access)
    }

    /// Gives it the names `names` (another language, ADR-0053 LR1): its accessible node and its
    /// tooltip say the new name from now on; its look does not change (it draws a symbol).
    pub(crate) fn set_names(&mut self, names: WindowButtonNames) {
        self.names = names;
    }

    /// Its name now.
    fn name(&self) -> &str {
        match self.kind {
            WindowButton::Close => &self.names.close,
            WindowButton::Minimize => &self.names.minimize,
            WindowButton::Maximize if self.maximized => &self.names.restore,
            WindowButton::Maximize => &self.names.maximize,
        }
    }

    /// What pressing it asks of the window.
    const fn request(&self) -> WindowRequest {
        match self.kind {
            // Not `Close`: the window asks the application first, as for the system's own close
            // button (F2.12a5b), so unsaved changes are never lost by this button.
            WindowButton::Close => WindowRequest::RequestClose,
            WindowButton::Maximize => WindowRequest::ToggleMaximized,
            WindowButton::Minimize => WindowRequest::Minimize,
        }
    }

    /// Draws the symbol in the square `area`, with lines of `width` in `paint`.
    fn symbol(&self, recorder: &mut Recorder<'_>, area: Rect, width: f32, paint: ColorToken) {
        let stroke = StrokeStyle::new(width);
        // Lines on half pixels stay crisp at a whole stroke width.
        let (x0, y0, x1, y1) = (area.x0, area.y0, area.x1, area.y1);
        match self.kind {
            WindowButton::Close => {
                recorder.line(Point::new(x0, y0), Point::new(x1, y1), stroke, paint);
                recorder.line(Point::new(x1, y0), Point::new(x0, y1), stroke, paint);
            }
            WindowButton::Minimize => {
                let y = y1 - width / 2.0;
                recorder.line(Point::new(x0, y), Point::new(x1, y), stroke, paint);
            }
            WindowButton::Maximize if self.maximized => {
                // Restore: a square in front, and the corner of one behind it.
                let step = area.width() * 0.25;
                let front = Rect::new(x0, y0 + step, x1 - step, y1).inflate(-width / 2.0);
                recorder.stroke_rect(front, stroke, paint);
                recorder.line(
                    Point::new(x0 + step, y0 + step),
                    Point::new(x0 + step, y0 + width / 2.0),
                    stroke,
                    paint,
                );
                recorder.line(
                    Point::new(x0 + step, y0 + width / 2.0),
                    Point::new(x1 - width / 2.0, y0 + width / 2.0),
                    stroke,
                    paint,
                );
                recorder.line(
                    Point::new(x1 - width / 2.0, y0 + width / 2.0),
                    Point::new(x1 - width / 2.0, y1 - step),
                    stroke,
                    paint,
                );
                recorder.line(
                    Point::new(x1 - width / 2.0, y1 - step),
                    Point::new(x1 - step, y1 - step),
                    stroke,
                    paint,
                );
            }
            WindowButton::Maximize => {
                recorder.stroke_rect(area.inflate(-width / 2.0), stroke, paint);
            }
        }
    }
}

/// `true` if `point` is inside `rect`.
fn contains(rect: Rect, point: Point) -> bool {
    point.x >= rect.x0 && point.x < rect.x1 && point.y >= rect.y0 && point.y < rect.y1
}

impl<M: 'static> Widget<M> for WindowButtonWidget {
    fn measure(&mut self, cx: &mut LayoutCx<'_, M>, constraints: Constraints) -> Size {
        if self.fullscreen {
            return constraints.constrain(Size::new(0.0, 0.0));
        }
        // A square as high as a control, so the bar keeps the menu bar's height.
        let side = cx.spacing().control_height;
        constraints.constrain(Size::new(side, side))
    }

    fn record(&self, cx: &RecordCx, recorder: &mut Recorder<'_>) {
        if self.fullscreen {
            return;
        }
        let rect = cx.rect;
        // The round background under the pointer, as GNOME's buttons have it.
        let pad = cx.spacing.gap_xs;
        let back = rect.inflate(-pad);
        if self.pressed || self.hovered {
            let fill = if self.pressed {
                ColorToken::ControlPressed
            } else {
                ColorToken::ControlHover
            };
            recorder.fill_rounded_rect(RoundedRect::new(back, back.width() / 2.0), fill);
        }
        // The symbol in a square of 10 logical pixels at the button's centre (GNOME's 16 px
        // icons draw their symbol in about that much).
        let side = 10.0_f32.min(back.width());
        let x = rect.x0 + (rect.width() - side) / 2.0;
        let y = rect.y0 + (rect.height() - side) / 2.0;
        let width = f32::from(cx.shape.line_width.max(1));
        self.symbol(
            recorder,
            Rect::new(x, y, x + side, y + side),
            width,
            ink(self.focused),
        );
    }

    fn event(&mut self, cx: &mut EventCx<'_, M>, event: &WidgetEvent<'_>) -> Handled {
        if self.fullscreen {
            return Handled::No;
        }
        match *event {
            WidgetEvent::PointerEntered | WidgetEvent::PointerLeft => {
                self.hovered = matches!(event, WidgetEvent::PointerEntered);
                cx.request_record();
                Handled::Yes
            }
            WidgetEvent::PointerButton {
                button: PointerButton::Left,
                state: ButtonState::Pressed,
                ..
            } => {
                self.pressed = true;
                cx.request_record();
                Handled::Yes
            }
            WidgetEvent::PointerButton {
                button: PointerButton::Left,
                state: ButtonState::Released,
                position,
                ..
            } => {
                // Released over the button: done; dragged off first: cancelled (every button).
                if self.pressed {
                    self.pressed = false;
                    cx.request_record();
                    if contains(cx.rect(), position) {
                        cx.request_window(self.request());
                    }
                }
                Handled::Yes
            }
            WidgetEvent::PointerMoved { .. } => Handled::Yes,
            WidgetEvent::Activate => {
                cx.request_window(self.request());
                Handled::Yes
            }
            _ => Handled::No,
        }
    }

    fn tooltip(&self) -> Option<&str> {
        Some(self.name())
    }

    fn access_role(&self) -> AccessRole {
        AccessRole::Button
    }

    fn access_info(&self, info: &mut AccessInfo) {
        info.label = Some(self.name().to_owned());
        info.clickable = true;
    }
}

/// The width of the resize band along the window's edges, logical pixels (ADR-0049 TB7): inside
/// the window, since `softbuffer` cannot draw outside it yet.
pub(crate) const BAND: f32 = 6.0;

/// The length of a corner's resize area along each edge, logical pixels (ADR-0049 TB7): a press
/// this close to a corner, inside the band, resizes from the corner.
pub(crate) const CORNER: f32 = 16.0;

/// The edge or corner of `bounds` (the window, logical pixels) under `point`, if it lies in the
/// resize band (ADR-0049 TB7, owner's choice of F2.11b3: the band is inside the window on every
/// side; at the top it lies over the title bar's highest pixels instead of above it).
///
/// A corner wins over its two edges within [`CORNER`] of it, so the diagonal resize is easy to
/// hit although the band is thin. Outside the window, `None`.
///
/// # Complexity
/// A few comparisons.
pub(crate) fn band_edge(bounds: Rect, point: Point) -> Option<crate::ResizeEdge> {
    use crate::ResizeEdge;
    if !contains(bounds, point) {
        return None;
    }
    let (left, right) = (point.x - bounds.x0, bounds.x1 - point.x);
    let (top, bottom) = (point.y - bounds.y0, bounds.y1 - point.y);
    let in_band = left < BAND || right < BAND || top < BAND || bottom < BAND;
    if !in_band {
        return None;
    }
    // Near which ends of each axis, for the corners.
    let west = left < CORNER;
    let east = right < CORNER;
    let north = top < CORNER;
    let south = bottom < CORNER;
    Some(match (north, south, west, east) {
        (true, _, true, _) => ResizeEdge::NorthWest,
        (true, _, _, true) => ResizeEdge::NorthEast,
        (_, true, true, _) => ResizeEdge::SouthWest,
        (_, true, _, true) => ResizeEdge::SouthEast,
        // Not near a corner: the edge whose band holds the point.
        _ if top < BAND => ResizeEdge::North,
        _ if bottom < BAND => ResizeEdge::South,
        _ if left < BAND => ResizeEdge::West,
        _ => ResizeEdge::East,
    })
}

/// The window's own frame while our title bar draws it and the window is neither maximised nor
/// in full screen (ADR-0049 TB7): a line of `border` along its four edges, so the window stands
/// apart from a window or desktop of the same colour. A root of its own in the widget arena,
/// recorded after the interface on its layer (so above the content, below every menu), never hit
/// by the pointer: the band it outlines is handled by the interface itself.
#[derive(Debug, Default)]
pub(crate) struct WindowFrame {
    /// The frame is drawn (windowed, not maximised, not in full screen).
    pub(crate) shown: bool,
}

impl<M: 'static> Widget<M> for WindowFrame {
    fn measure(&mut self, _cx: &mut LayoutCx<'_, M>, constraints: Constraints) -> Size {
        constraints.max
    }

    fn record(&self, cx: &RecordCx, recorder: &mut Recorder<'_>) {
        if !self.shown {
            return;
        }
        // One physical pixel would need the scale here; the theme's line width is what every
        // other line of the interface uses (1 logical pixel at the default density).
        let width = f32::from(cx.shape.line_width.max(1));
        let rect = cx.rect.inflate(-width / 2.0);
        recorder.stroke_rect(rect, StrokeStyle::new(width), ColorToken::Border);
    }
}

#[cfg(test)]
mod tests {
    use super::{BAND, CORNER, band_edge};
    use crate::ResizeEdge;
    use crate::draw::geometry::{Point, Rect};

    /// The window of these tests, logical pixels.
    const WINDOW: Rect = Rect::new(0.0, 0.0, 800.0, 600.0);

    #[test]
    fn the_band_has_four_edges_and_four_corners() {
        let at = |x, y| band_edge(WINDOW, Point::new(x, y));
        // The middle of each edge, inside the band.
        assert_eq!(at(400.0, 1.0), Some(ResizeEdge::North));
        assert_eq!(at(400.0, 598.0), Some(ResizeEdge::South));
        assert_eq!(at(1.0, 300.0), Some(ResizeEdge::West));
        assert_eq!(at(798.0, 300.0), Some(ResizeEdge::East));
        // Near the corners, within CORNER of them along the band.
        assert_eq!(at(2.0, 2.0), Some(ResizeEdge::NorthWest));
        assert_eq!(at(CORNER - 1.0, 1.0), Some(ResizeEdge::NorthWest));
        assert_eq!(at(1.0, CORNER - 1.0), Some(ResizeEdge::NorthWest));
        assert_eq!(at(798.0, 1.0), Some(ResizeEdge::NorthEast));
        assert_eq!(at(1.0, 598.0), Some(ResizeEdge::SouthWest));
        assert_eq!(at(798.0, 598.0), Some(ResizeEdge::SouthEast));
        // Just past the corner's length, the edge again.
        assert_eq!(at(CORNER + 1.0, 1.0), Some(ResizeEdge::North));
    }

    #[test]
    fn outside_the_band_or_the_window_nothing() {
        let at = |x, y| band_edge(WINDOW, Point::new(x, y));
        assert_eq!(at(400.0, 300.0), None);
        assert_eq!(at(BAND + 0.5, BAND + 0.5), None);
        assert_eq!(at(-1.0, 300.0), None);
        assert_eq!(at(800.0, 300.0), None);
    }
}
