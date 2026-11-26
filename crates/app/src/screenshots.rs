// SPDX-License-Identifier: GPL-3.0-or-later

//! The screenshots of the user guides (ADR-0041): a catalogue of **scenes**, each rendered headless
//! by the program's own views and renderer, then encoded as PNG by `orchestra-screenshots`.
//!
//! # Why a catalogue
//! Every screenshot in the guides is one entry of [`SCENES`]: a file name, a logical size and a
//! function that brings a view into the state to photograph. Adding a picture to the guides means
//! adding one entry here; renaming one means renaming the file the guides include. Nothing else in
//! the pipeline changes (ADR-0041 S2).
//!
//! # What is fixed for every scene (ADR-0041 S3)
//! - the theme: **Studio, dark half** ([`theme`]), set explicitly and never read from the system,
//!   so the pictures do not depend on the desktop of whoever runs the tool;
//! - the language: en-US (ADR-0009 §6): the texts come from the en-US catalogue, through the
//!   [`SceneContext`] that [`render`] prepares;
//! - the scale: [`SCALE`] device pixels per logical pixel.
//!
//! # How a scene is drawn
//! Exactly as a window would draw its first frame: the view's [`WindowApp::layout`] records the
//! whole interface for the scene's size, then the scene may feed it [`InputEvent`]s (for example
//! to place the pointer), and [`CpuRenderer::render_to_image`] draws the whole list. The window
//! itself, `winit` and `softbuffer` are not involved, so no display is needed (CLAUDE.md §3).

use std::fmt;

use orchestra_i18n::{Locale, Localizer};
use orchestra_text::Faces;
use orchestra_ui::{
    Appearance, BuiltInTheme, ButtonState, CpuRenderer, DisplayList, DrawError, Image, InputEvent,
    Key, Modifiers, NamedKey, Rect, Theme, WindowApp,
};

use crate::Demo;

/// Device pixels per logical pixel in every screenshot (ADR-0041 S3).
///
/// # Why 2
/// A guide page is A4 with about 16 cm of text width. A scene 640 logical pixels wide becomes
/// 1280 device pixels, which printed across the text width is ≈ 200 dpi: sharp in a PDF viewer at
/// any usual zoom and in print. It also exercises the high-DPI path of the renderer (lines snapped
/// to device pixels), which is what most users of recent laptops see. Scale 3 would make every file
/// 2.25 times larger (3² / 2²) for no visible gain.
pub const SCALE: f32 = 2.0;

/// The largest side of a screenshot, in device pixels: far beyond any page, and small enough that
/// a mistyped size in the catalogue is an error instead of gigabytes of pixels.
const MAX_SIDE: f32 = 16_384.0;

/// One screenshot: its file name, its size and how to bring the view into the state shown.
#[derive(Debug, Clone, Copy)]
pub struct Scene {
    /// The file name without extension, in kebab case: the guides include
    /// `/dist/screenshots/<name>.png`. Unique in [`SCENES`] (checked by a test).
    pub name: &'static str,
    /// Width in logical pixels.
    pub width: f32,
    /// Height in logical pixels.
    pub height: f32,
    /// Records the scene into an empty list for `bounds` (origin at the top left, the scene's
    /// size), with the fonts and texts of `context`; see [`Record`].
    pub record: Record,
}

/// How a scene is recorded, and with which theme it is then drawn.
///
/// # Why two kinds?
/// Almost every scene is drawn with [`theme`] (Studio's dark half), whatever happens in it. The
/// theme editor's scenes (F2.12b1) change the theme itself: the whole window is the preview of
/// the colours typed, which a real window shows because the demonstration sends it the new theme
/// ([`orchestra_ui::WindowRequest::SetTheme`]). Headless there is no window to receive it, so
/// such a scene answers the theme the window would draw with, and [`render`] uses that. An enum
/// rather than one signature returning an `Option` keeps the many plain scenes as they were.
#[derive(Debug, Clone, Copy)]
pub enum Record {
    /// Recorded, then drawn with [`theme`].
    Plain(fn(bounds: Rect, list: &mut DisplayList, context: &SceneContext)),
    /// Recorded, then drawn with the theme the function answers.
    Themed(fn(bounds: Rect, list: &mut DisplayList, context: &SceneContext) -> Theme),
}

/// What every scene may need besides its size, prepared once by [`render`]: the bundled faces
/// (ADR-0044) and the en-US texts (ADR-0009 §6). Preparing them can fail (in theory: the fonts
/// and catalogues are inside the binary), so [`render`] does it and reports a typed error, and
/// the scenes stay plain functions.
#[derive(Debug, Clone)]
pub struct SceneContext {
    /// The bundled faces.
    pub faces: Faces,
    /// The program's name in en-US (the catalogue's `app-name`).
    pub app_name: String,
    /// The demonstration's texts in en-US.
    pub demo: crate::DemoTexts,
}

impl SceneContext {
    /// The faces and the en-US texts.
    ///
    /// # Errors
    /// [`ScreenshotError::Setup`] if the faces or the en-US catalogue cannot be read.
    pub fn new() -> Result<Self, ScreenshotError> {
        let faces = Faces::bundled().map_err(|error| ScreenshotError::Setup {
            detail: error.to_string(),
        })?;
        let (app_name, demo) = Localizer::new(Locale::EnUs)
            .and_then(|localizer| {
                Ok((
                    localizer.format("app-name", None)?,
                    crate::DemoTexts::new(&localizer)?,
                ))
            })
            .map_err(|error| ScreenshotError::Setup {
                detail: error.to_string(),
            })?;
        Ok(Self {
            faces,
            app_name,
            demo,
        })
    }
}

impl Scene {
    /// The image size in device pixels: the logical size times [`SCALE`], rounded to the nearest
    /// whole pixel (the scenes use sizes that are exact at scale 2, so nothing is rounded today).
    ///
    /// # Errors
    /// [`ScreenshotError::Size`] when a side is not between 1 and 16 384 device pixels (a mistake
    /// in the catalogue, caught by the tests).
    pub fn device_size(&self) -> Result<(u32, u32), ScreenshotError> {
        let side = |logical: f32| {
            let device = (logical * SCALE).round();
            // `as` would saturate silently; check the range first so a bad entry is an error. NaN
            // fails both comparisons, so it is rejected too.
            if (1.0..=MAX_SIDE).contains(&device) {
                #[allow(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    reason = "checked just above: between 1 and 16 384, and already rounded to a \
                              whole number, so the conversion is exact"
                )]
                Ok(device as u32)
            } else {
                Err(ScreenshotError::Size {
                    scene: self.name,
                    width: self.width,
                    height: self.height,
                })
            }
        };
        Ok((side(self.width)?, side(self.height)?))
    }
}

/// Every screenshot of the guides, in the order they are written.
///
/// Today only the demonstration window exists (ADR-0040 W8): it is here to prove the pipeline end
/// to end, and the guides do **not** include it, because it is a throw-away view that the reader
/// will never see (ADR-0041 S7). The first pictures the guides show arrive with the real views.
pub const SCENES: &[Scene] = &[
    Scene {
        name: "demo-window",
        width: 640.0,
        height: 370.0,
        record: Record::Plain(record_demo_window),
    },
    Scene {
        name: "demo-tooltip",
        width: TOOLTIP_SCENE_WIDTH,
        height: 370.0,
        record: Record::Plain(record_demo_tooltip),
    },
    Scene {
        name: "demo-context-menu",
        width: 640.0,
        height: 370.0,
        record: Record::Plain(record_demo_context_menu),
    },
    Scene {
        name: "demo-menu-bar",
        width: 640.0,
        height: 370.0,
        record: Record::Plain(record_demo_menu_bar),
    },
    Scene {
        name: "title-bar",
        width: 640.0,
        height: TITLE_BAR_HEIGHT,
        record: Record::Plain(record_title_bar),
    },
    Scene {
        name: "about-dialog",
        width: 640.0,
        height: ABOUT_SCENE_HEIGHT,
        record: Record::Plain(record_about_dialog),
    },
    Scene {
        name: "save-changes-dialog",
        width: 640.0,
        height: ABOUT_SCENE_HEIGHT,
        record: Record::Plain(record_save_changes_dialog),
    },
    Scene {
        name: "preferences-dialog",
        width: 960.0,
        height: 600.0,
        record: Record::Plain(record_preferences_dialog),
    },
    Scene {
        name: "preferences-language",
        width: 960.0,
        height: 600.0,
        record: Record::Plain(record_preferences_language),
    },
    Scene {
        name: "preferences-playback",
        width: 960.0,
        height: 600.0,
        record: Record::Plain(record_preferences_playback),
    },
    Scene {
        name: "preferences-keyboard",
        width: 960.0,
        height: 600.0,
        record: Record::Plain(record_preferences_keyboard),
    },
    Scene {
        name: "shortcut-conflict",
        width: 960.0,
        height: 600.0,
        record: Record::Plain(record_shortcut_conflict),
    },
    Scene {
        name: "theme-editor",
        width: 960.0,
        height: 600.0,
        record: Record::Themed(record_theme_editor),
    },
    Scene {
        name: "theme-text-lines",
        width: 960.0,
        height: 600.0,
        record: Record::Themed(record_theme_text_lines),
    },
    Scene {
        name: "theme-contrast",
        width: 960.0,
        height: 600.0,
        record: Record::Themed(record_theme_contrast),
    },
];

/// The Preferences open on their section number `down` (F2.12a4b1): opened as their menu item
/// opens them, then Down pressed `down` times in the list of sections, which has the focus.
fn record_preferences_section(
    bounds: Rect,
    list: &mut DisplayList,
    context: &SceneContext,
    down: usize,
) {
    let mut demo = Demo::new(&context.faces, &theme(), &context.app_name, &context.demo);
    demo.layout(bounds, list);
    if let Some(ids) = demo.actions() {
        demo.run_action(ids.preferences, list);
    }
    for _ in 0..down {
        for state in [ButtonState::Pressed, ButtonState::Released] {
            demo.input(
                &InputEvent::Key {
                    key: Key::Named(NamedKey::ArrowDown),
                    physical: orchestra_ui::PhysicalKey::Other,
                    state,
                    repeat: false,
                    modifiers: Modifiers::default(),
                },
                list,
            );
        }
    }
}

/// The Preferences' Language section (F2.12a4b1): the drop-down on *System language* and the
/// note that a new language applies at the next start.
fn record_preferences_language(bounds: Rect, list: &mut DisplayList, context: &SceneContext) {
    record_preferences_section(bounds, list, context, 1);
}

/// The Preferences' Playback section (F2.12a4b2): the tuning's eighteen values with their
/// sliders, in three groups, the first ones visible.
fn record_preferences_playback(bounds: Rect, list: &mut DisplayList, context: &SceneContext) {
    record_preferences_section(bounds, list, context, 2);
}

/// The Preferences' Keyboard section (F2.12a4b1): the modifier of fine adjustment, as Linux
/// names the keys (the scenes are rendered on Linux).
fn record_preferences_keyboard(bounds: Rect, list: &mut DisplayList, context: &SceneContext) {
    record_preferences_section(bounds, list, context, 3);
}

/// The shortcut editor's question about a conflict (F2.12c1, ADR-0048 AM7): in the Keyboard
/// section, *Open…* selected, *Add…* pressed, then *Primary*+N, which *New* has: "Ctrl+N is
/// already assigned to "New"", with *Reassign* (the default) and *Cancel*, over the Preferences.
fn record_shortcut_conflict(bounds: Rect, list: &mut DisplayList, context: &SceneContext) {
    let mut demo = Demo::new(&context.faces, &theme(), &context.app_name, &context.demo);
    demo.layout(bounds, list);
    if let Some(ids) = demo.actions() {
        demo.run_action(ids.preferences, list);
    }
    let press = |demo: &mut Demo, list: &mut DisplayList, key: Key, modifiers: Modifiers| {
        for state in [ButtonState::Pressed, ButtonState::Released] {
            demo.input(
                &InputEvent::Key {
                    key: key.clone(),
                    physical: orchestra_ui::PhysicalKey::Other,
                    state,
                    repeat: false,
                    modifiers,
                },
                list,
            );
        }
    };
    // The Keyboard section (the last, End in the list of sections), then the focus on the
    // commands' list, where Down selects *Open…*.
    press(
        &mut demo,
        list,
        Key::Named(NamedKey::End),
        Modifiers::default(),
    );
    let Some((commands, add)) = demo
        .shortcut_editor()
        .map(|editor| (editor.commands(), editor.add_button()))
    else {
        return;
    };
    if demo.ui_mut().set_focus(Some(commands), list).is_err() {
        return;
    }
    press(
        &mut demo,
        list,
        Key::Named(NamedKey::ArrowDown),
        Modifiers::default(),
    );
    // *Add…* from the keyboard, then the combination.
    if demo.ui_mut().set_focus(Some(add), list).is_err() {
        return;
    }
    press(
        &mut demo,
        list,
        Key::Named(NamedKey::Space),
        Modifiers::default(),
    );
    let primary = Modifiers {
        primary: true,
        ..Modifiers::default()
    };
    press(&mut demo, list, Key::Character("n".into()), primary);
}

/// Presses and releases `key` with `modifiers` in `demo` (the theme editor's scenes).
fn press_key(demo: &mut Demo, list: &mut DisplayList, key: &Key, modifiers: Modifiers) {
    for state in [ButtonState::Pressed, ButtonState::Released] {
        demo.input(
            &InputEvent::Key {
                key: key.clone(),
                physical: orchestra_ui::PhysicalKey::Other,
                state,
                repeat: false,
                modifiers,
            },
            list,
        );
    }
}

/// The Preferences on Appearance with Studio duplicated into "Studio (copy)" (F2.12b1) and the
/// colour token `token` given the value `value`, typed into its field as a user would (the focus
/// on it, *Primary*+A, the text). Answers the demonstration, for the scene to continue, and the
/// theme the window would now draw with (the dark half of the pair the demonstration last sent
/// it); `None` if the editor is not there (never expected: the scene is then drawn as far as it
/// got, with the catalogue's theme).
///
/// # The half
/// The scenes are drawn with a dark half ([`theme`]). A real window tells the interface which half
/// it draws as soon as it exists ([`InputEvent::Appearance`]); headless nobody does, and the
/// interface would believe it is the light one (its default): the editor would then show and
/// change the light half's values beside swatches drawn in the dark one. So the scene tells it
/// first, as the window does.
fn theme_editor_with(
    bounds: Rect,
    list: &mut DisplayList,
    context: &SceneContext,
    token: orchestra_ui::ColorToken,
    value: &str,
) -> Option<(Demo, Theme)> {
    custom_theme_scene(bounds, list, context, |demo, list| {
        // The token's field, in `ColorToken::ALL`'s order like the rows.
        let index = orchestra_ui::ColorToken::ALL
            .iter()
            .position(|known| *known == token)?;
        let field = demo.theme_editor()?.field(index)?;
        demo.ui_mut().set_focus(Some(field), list).ok()?;
        let primary = Modifiers {
            primary: true,
            ..Modifiers::default()
        };
        press_key(demo, list, &Key::Character("a".into()), primary);
        demo.input(&InputEvent::Text(value.to_owned()), list);
        Some(())
    })
}

/// The Preferences on Appearance with Studio duplicated into "Studio (copy)", the dark half told
/// (see [`theme_editor_with`]), then `edit` run on the demonstration. Answers it with the theme
/// the window would now draw with (the dark half of the last pair sent to it); `None` if a widget
/// is missing (never expected).
fn custom_theme_scene(
    bounds: Rect,
    list: &mut DisplayList,
    context: &SceneContext,
    edit: impl FnOnce(&mut Demo, &mut DisplayList) -> Option<()>,
) -> Option<(Demo, Theme)> {
    let mut demo = Demo::new(&context.faces, &theme(), &context.app_name, &context.demo);
    demo.layout(bounds, list);
    demo.input(&InputEvent::Appearance(Appearance::Dark), list);
    let ids = demo.actions()?;
    demo.run_action(ids.preferences, list);
    // *Duplicate* from the keyboard: the focus on it, then Space.
    let duplicate = demo.theme_editor()?.duplicate_button();
    demo.ui_mut().set_focus(Some(duplicate), list).ok()?;
    press_key(
        &mut demo,
        list,
        &Key::Named(NamedKey::Space),
        Modifiers::default(),
    );
    edit(&mut demo, list)?;
    // The last theme sent to the window: the one it would draw with now.
    let mut drawn_with = theme();
    while let Some(request) = demo.take_window_request() {
        if let orchestra_ui::WindowRequest::SetTheme { pair, .. } = request {
            drawn_with = *pair.instance(Appearance::Dark);
        }
    }
    Some((demo, drawn_with))
}

/// Scrolls the Appearance section by `pixels` (positive: towards the top), as a wheel turned over
/// it: the pointer first moves to the centre of the Preferences' panel, which lies in the
/// section's scroll area (the list of sections takes only the panel's left quarter), then the
/// turn. A turn far larger than the section stops at its end. Nothing if the Preferences are
/// closed.
fn scroll_section(demo: &mut Demo, list: &mut DisplayList, pixels: f32) {
    let Some(rect) = demo.preferences().and_then(|panel| demo.ui().rect(panel)) else {
        return;
    };
    demo.input(
        &InputEvent::PointerMoved {
            position: orchestra_ui::Point::new(
                f32::midpoint(rect.x0, rect.x1),
                f32::midpoint(rect.y0, rect.y1),
            ),
        },
        list,
    );
    demo.input(
        &InputEvent::Wheel {
            delta: orchestra_ui::WheelDelta::Pixels { x: 0.0, y: pixels },
        },
        list,
    );
}

/// The theme editor's text, lines and density (F2.12b2a): a custom theme from Studio with its
/// text at 110% and its corners rounder (12 px), set with the keyboard as a user would; the whole
/// window already draws with them, *Restore* is enabled on the corners' row; the focus on the
/// density's group brings the lines' settings and the density into view.
fn record_theme_text_lines(bounds: Rect, list: &mut DisplayList, context: &SceneContext) -> Theme {
    custom_theme_scene(bounds, list, context, |demo, list| {
        let rows = *demo.theme_editor()?.shared_rows()?;
        let none = Modifiers::default();
        demo.ui_mut()
            .set_focus(Some(rows.text_scale.slider), list)
            .ok()?;
        for _ in 0..2 {
            press_key(demo, list, &Key::Named(NamedKey::ArrowRight), none);
        }
        demo.ui_mut()
            .set_focus(Some(rows.shapes[0].slider), list)
            .ok()?;
        // Home to 0 px, then twelve steps of one pixel (the slider's step): 12 px.
        press_key(demo, list, &Key::Named(NamedKey::Home), none);
        for _ in 0..12 {
            press_key(demo, list, &Key::Named(NamedKey::ArrowRight), none);
        }
        // The density group last: its focus brings the end of the rows into view.
        demo.ui_mut()
            .set_focus(Some(rows.density.control), list)
            .ok()?;
        Some(())
    })
    .map_or_else(theme, |(_, drawn_with)| drawn_with)
}

/// The theme editor (F2.12b1, ADR-0037 T7): a custom theme duplicated from Studio, with the
/// accent of its dark half changed to a light blue. The whole window already draws with it (the
/// selected section, the dialog's frame, the check box behind, the swatch), *Restore* is enabled
/// on that row only; the note about the half and the first ten colours are in view.
fn record_theme_editor(bounds: Rect, list: &mut DisplayList, context: &SceneContext) -> Theme {
    // The value is a picture's choice, not a colour of a theme: a blue far from Studio's orange,
    // light enough to keep the contrast of the dark text written on the accent.
    // The section stays where typing left it: the accent's row in view, at the bottom (the top,
    // with the theme and the editor's buttons, is in `preferences-dialog`).
    theme_editor_with(
        bounds,
        list,
        context,
        orchestra_ui::ColorToken::Accent,
        "#5b9bf0",
    )
    .map_or_else(theme, |(_, drawn_with)| drawn_with)
}

/// The theme editor's readability warning (F2.12b1, ADR-0037 T8, owner's requests of
/// 2026-11-26): secondary text made too dark on the dark half, so the box at the top says in plain
/// words that it is almost invisible on the menus' background and on three other colours, and to
/// make it lighter; its row and its backgrounds' rows carry the warning dot. The section is
/// scrolled back to its top with the wheel over it (typing had scrolled to the field).
fn record_theme_contrast(bounds: Rect, list: &mut DisplayList, context: &SceneContext) -> Theme {
    // A grey close to the dark surfaces: below 4.5:1 on all four.
    let Some((mut demo, drawn_with)) = theme_editor_with(
        bounds,
        list,
        context,
        orchestra_ui::ColorToken::TextMuted,
        "#4a4a48",
    ) else {
        return theme();
    };
    scroll_section(&mut demo, list, 10_000.0);
    drawn_with
}

/// The demonstration window with the Preferences open (F2.12a4a, ADR-0050 DG8–DG9), as their menu
/// item opens them, in the window's default size: the list of sections (Appearance), the theme's
/// drop-down, the mode's group, the blinking cursor's check box, *Close*; the focus on the list.
fn record_preferences_dialog(bounds: Rect, list: &mut DisplayList, context: &SceneContext) {
    let mut demo = Demo::new(&context.faces, &theme(), &context.app_name, &context.demo);
    demo.layout(bounds, list);
    if let Some(ids) = demo.actions() {
        demo.run_action(ids.preferences, list);
    }
}

/// The height of the About dialog's scene (F2.12a3b), logical pixels: the demonstration window
/// made tall enough for the whole dialog (logo, title, version, four details, *Close*) to show
/// without scrolling its texts.
const ABOUT_SCENE_HEIGHT: f32 = 600.0;

/// The demonstration window with *About Orchestra* open (F2.12a3b, ADR-0050 DG10), run as its
/// menu item would: the logo in the text colour, the version, the licence, the logo's notice, the
/// repository's address and the bundled fonts, the focus on *Close*. The buttons' order does not
/// matter with one button, so the scene does not depend on the desktop.
fn record_about_dialog(bounds: Rect, list: &mut DisplayList, context: &SceneContext) {
    let mut demo = Demo::new(&context.faces, &theme(), &context.app_name, &context.demo);
    demo.layout(bounds, list);
    if let Some(ids) = demo.actions() {
        demo.run_action(ids.about, list);
    }
}

/// The question "Save the changes?" (F2.12a5b, ADR-0050 DG5): the open project, never saved,
/// has a change (two bars inserted, the same edit as the tests), and the window was asked to
/// close. The title shows *Untitled*, the three buttons are in the interface's default order
/// (the scenes never read the desktop's), *Save* is the default.
fn record_save_changes_dialog(bounds: Rect, list: &mut DisplayList, context: &SceneContext) {
    let mut demo = Demo::new(&context.faces, &theme(), &context.app_name, &context.demo);
    demo.layout(bounds, list);
    let Some(flow) = demo.project_mut() else {
        return;
    };
    let insert = orchestra_core_model::Command::InsertBars {
        at: orchestra_core_time::BarIndex::FIRST,
        count: 2,
    };
    if flow.project_mut().editor_mut().execute(&insert).is_err() {
        return;
    }
    demo.refresh_title();
    let _ = demo.close_requested(list);
}

/// The height of the title bar's strip (F2.11b3): the bar (a control's height at Studio's
/// density) and the resize band under it, with the frame's line around.
const TITLE_BAR_HEIGHT: f32 = 36.0;

/// Our own title bar alone (F2.11b3, ADR-0049 TB11, owner's choice: a strip, not the whole
/// window): the program's four menus, its name, and the three buttons of GNOME's
/// `appmenu:minimize,maximize,close`, in the frame of a window that is neither maximised nor in
/// full screen. The pointer rests on nothing, so the picture shows the bar at rest.
fn record_title_bar(bounds: Rect, list: &mut DisplayList, context: &SceneContext) {
    use orchestra_ui::{Flex, Platform, Space, TitleBarSpec, Ui};
    let mut ui: Ui<crate::DemoMessage> = Ui::new(
        Flex::column(Space::None, Space::None),
        &theme(),
        context.faces.clone(),
    );
    ui.set_title(&context.app_name);
    // The program's own actions and menus, with their names (Linux: the platform the bar is for).
    let Ok((mut registry, ids)) = crate::actions::registry(Platform::Linux) else {
        return;
    };
    context.demo.actions.apply(&mut registry);
    ui.set_actions(registry, crate::DemoMessage::Action);
    let model = crate::actions::menu_bar(&ids, &context.demo.actions);
    let spec = TitleBarSpec::from_setting(
        Some("appmenu:minimize,maximize,close"),
        context.app_name.clone(),
        context.demo.window_buttons.clone(),
    );
    let root = ui.root();
    if ui.add_title_bar(root, &model, &spec).is_err() {
        return;
    }
    ui.layout(bounds, list);
}

/// The demonstration window with the menu bar used from the keyboard (F2.11d2b, ADR-0048 AM12):
/// F10 puts the keyboard on *File*, Enter opens its menu; its first enabled item (*New*, since
/// F2.12a5b) is highlighted and the mnemonics are underlined, as they are while the bar is used from the
/// keyboard. The scenes are rendered on Linux, where the bar is drawn.
fn record_demo_menu_bar(bounds: Rect, list: &mut DisplayList, context: &SceneContext) {
    let mut demo = Demo::new(&context.faces, &theme(), &context.app_name, &context.demo);
    demo.layout(bounds, list);
    for key in [NamedKey::Function(10), NamedKey::Enter] {
        for state in [ButtonState::Pressed, ButtonState::Released] {
            demo.input(
                &InputEvent::Key {
                    key: Key::Named(key),
                    physical: orchestra_ui::PhysicalKey::Other,
                    state,
                    repeat: false,
                    modifiers: Modifiers::default(),
                },
                list,
            );
        }
    }
}

/// The demonstration window with the context menu of the *Track name* field open (F2.11d2a,
/// ADR-0048 AM12): a name typed and selected, the menu opened by a secondary click inside the
/// selection, the pointer resting on *Copy* (highlighted). Undo is enabled by the typing, Redo is
/// disabled (nothing undone yet), so the picture shows both looks, the separators and the
/// shortcuts written at the right.
fn record_demo_context_menu(bounds: Rect, list: &mut DisplayList, context: &SceneContext) {
    let mut demo = Demo::new(&context.faces, &theme(), &context.app_name, &context.demo);
    demo.layout(bounds, list);
    let field = demo.track_name();
    if demo.ui_mut().set_focus(Some(field), list).is_err() {
        return;
    }
    // The name, typed as a keyboard would give its text (the keys themselves are not needed: a
    // field inserts the text events).
    demo.input(&InputEvent::Text("Violin I".to_owned()), list);
    // Select everything with the field's own editing key (Ctrl+A, ⌘A on macOS).
    for state in [ButtonState::Pressed, ButtonState::Released] {
        demo.input(
            &InputEvent::Key {
                key: Key::Character("a".to_owned()),
                physical: orchestra_ui::PhysicalKey::Letter(0),
                state,
                repeat: false,
                modifiers: Modifiers {
                    primary: true,
                    ..Modifiers::default()
                },
            },
            list,
        );
    }
    let Some(rect) = demo.ui().rect(field) else {
        return;
    };
    let at = orchestra_ui::Point::new(rect.x0 + 24.0, f32::midpoint(rect.y0, rect.y1));
    demo.input(&InputEvent::PointerMoved { position: at }, list);
    // The secondary click (on the press on Linux, on the release on Windows: both are sent).
    for state in [ButtonState::Pressed, ButtonState::Released] {
        demo.input(
            &InputEvent::PointerButton {
                button: orchestra_ui::PointerButton::Right,
                state,
                position: at,
                clicks: 1,
            },
            list,
        );
    }
    // The pointer onto Copy, the fifth item (Undo, Redo, a separator, Cut, Copy…).
    let copy = demo
        .ui()
        .menus()
        .next()
        .and_then(|menu| demo.ui().menu_items(menu).nth(4))
        .and_then(|item| demo.ui().rect(item));
    if let Some(copy) = copy {
        let position = orchestra_ui::Point::new(copy.x0 + 30.0, f32::midpoint(copy.y0, copy.y1));
        demo.input(&InputEvent::PointerMoved { position }, list);
    }
}

/// The width of the tooltip's scene, logical pixels: a window so narrow that the options' pane
/// is below its minimum and *Restore defaults* is shortened with an ellipsis, so it has a tooltip
/// (at the options' minimum width, 160 pixels, the button still fits). Measured in F2.11c: the
/// button is whole at 240 pixels and shortened at 200.
pub const TOOLTIP_SCENE_WIDTH: f32 = 200.0;

/// The demonstration window, narrowed, with the pointer resting on the shortened *Restore
/// defaults* button for the 500 ms its tooltip waits for (F2.11c, ADR-0048 AM14): the tooltip
/// shows the button's whole text below the pointer, inside the window.
fn record_demo_tooltip(bounds: Rect, list: &mut DisplayList, context: &SceneContext) {
    let mut demo = Demo::new(&context.faces, &theme(), &context.app_name, &context.demo);
    demo.layout(bounds, list);
    let Some(rect) = demo.ui().rect(demo.restore_defaults()) else {
        return;
    };
    let position = orchestra_ui::Point::new(
        f32::midpoint(rect.x0, rect.x1),
        f32::midpoint(rect.y0, rect.y1),
    );
    demo.input(&InputEvent::PointerMoved { position }, list);
    let rested = demo.ui().now() + std::time::Duration::from_millis(500);
    demo.advance(rested, list);
}

/// The demonstration window (the Playback options preview, F2.09c) with the keyboard focus on
/// its first control, the *Metronome* check box, so the focus ring is in the picture.
fn record_demo_window(bounds: Rect, list: &mut DisplayList, context: &SceneContext) {
    let mut demo = Demo::new(&context.faces, &theme(), &context.app_name, &context.demo);
    demo.layout(bounds, list);
    demo.input(
        &InputEvent::Key {
            key: Key::Named(NamedKey::Tab),
            physical: orchestra_ui::PhysicalKey::Other,
            state: ButtonState::Pressed,
            repeat: false,
            modifiers: Modifiers::default(),
        },
        list,
    );
}

/// The theme of every screenshot: Studio's dark half (ADR-0041 S3, which amends ADR-0014 §7 and
/// ADR-0037 T10 on the owner's request).
#[must_use]
pub fn theme() -> Theme {
    *BuiltInTheme::Studio.pair().instance(Appearance::Dark)
}

/// Draws `scene` into a new image with `renderer` (reuse one renderer for the whole catalogue: it
/// keeps its scratch buffers).
///
/// # Complexity
/// One full frame: O(commands + device pixels).
///
/// # Errors
/// [`ScreenshotError::Size`] for a bad catalogue entry, [`ScreenshotError::Setup`] if the fonts or
/// the texts cannot be read, [`ScreenshotError::Draw`] if the renderer rejects the image.
pub fn render(scene: &Scene, renderer: &mut CpuRenderer) -> Result<Image, ScreenshotError> {
    let (width, height) = scene.device_size()?;
    let context = SceneContext::new()?;
    let mut list = DisplayList::new();
    let bounds = Rect::from_origin_size(0.0, 0.0, scene.width, scene.height);
    // The theme to draw with: the catalogue's, or the one a themed scene answers (see `Record`).
    let drawn_with = match scene.record {
        Record::Plain(record) => {
            record(bounds, &mut list, &context);
            theme()
        }
        Record::Themed(record) => record(bounds, &mut list, &context),
    };
    renderer
        .render_to_image(&list, &drawn_with, width, height, SCALE)
        .map_err(|error| ScreenshotError::Draw {
            scene: scene.name,
            error,
        })
}

/// What can go wrong while rendering a screenshot. Exhaustive (ADR-0034 D2). The messages are for
/// developers (the tool is never shown to users), so they are English and not translated.
#[derive(Debug, Clone, PartialEq)]
pub enum ScreenshotError {
    /// A catalogue entry whose size is not a positive, representable number of device pixels.
    Size {
        /// The scene's name.
        scene: &'static str,
        /// Its logical width.
        width: f32,
        /// Its logical height.
        height: f32,
    },
    /// The bundled faces or the en-US catalogue could not be read (never with the files in the
    /// binary; a typed error because library code does not panic).
    Setup {
        /// The underlying error's message.
        detail: String,
    },
    /// The renderer failed, or the PNG encoder did.
    Draw {
        /// The scene's name.
        scene: &'static str,
        /// The drawing module's error.
        error: DrawError,
    },
}

impl fmt::Display for ScreenshotError {
    /// One English line naming the scene.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Size {
                scene,
                width,
                height,
            } => write!(
                f,
                "scene `{scene}`: size {width} × {height} at scale {SCALE} is not a valid image"
            ),
            Self::Setup { detail } => write!(f, "the fonts or texts of the scenes: {detail}"),
            Self::Draw { scene, error } => write!(f, "scene `{scene}`: {error}"),
        }
    }
}

impl std::error::Error for ScreenshotError {
    /// The drawing error, when there is one.
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Size { .. } | Self::Setup { .. } => None,
            Self::Draw { error, .. } => Some(error),
        }
    }
}
