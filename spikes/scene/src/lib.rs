// SPDX-License-Identifier: GPL-3.0-or-later

//! Common test scene for the GUI toolkit spikes (Phase 0, Step 13, ADR-0012).
//!
//! Every candidate toolkit draws exactly this content, so that measurements compare toolkits and
//! not scenes:
//! - 10 staves × 20 measures with **exactly 2000 SMuFL glyphs** (clefs, noteheads, accidentals,
//!   rests) from the bundled Bravura font, plus staff lines, barlines and stems as lines;
//! - one line of text in each of the six interface languages, in the bundled Inter font;
//! - a deterministic path of 120 views (scroll + zoom) used for frame-time measurements.
//!
//! Coordinates are in **staff spaces** (sp, the distance between two staff lines); a view maps
//! them to pixels with `pixels_per_sp`. At zoom 1.0 (8 px per sp) the whole score (≈ 205 × 120 sp)
//! fits in a 1920 × 1080 window, which is the worst case: every glyph is visible.

use std::time::Duration;

/// Number of staves in the scene.
pub const STAVES: usize = 10;
/// Measures per staff.
pub const MEASURES: usize = 20;
/// Exact number of music glyphs in the scene (the brief asks for 2000).
pub const GLYPHS: usize = 2000;
/// Vertical distance between the top lines of two consecutive staves, in sp.
pub const STAFF_DISTANCE: f32 = 12.0;
/// Width of one measure, in sp.
pub const MEASURE_WIDTH: f32 = 10.0;
/// Horizontal room for the clef at the start of each staff, in sp.
pub const CLEF_WIDTH: f32 = 4.0;
/// Pixels per staff space at zoom 1.0.
pub const BASE_PX_PER_SP: f32 = 8.0;
/// Window size used for every measurement.
pub const WIDTH: u32 = 1920;
/// Window height used for every measurement.
pub const HEIGHT: u32 = 1080;

/// Bravura (SMuFL reference font, OFL-1.1), bundled in the repository.
pub static MUSIC_FONT: &[u8] = include_bytes!("../../../assets/fonts/bravura/Bravura.otf");
/// Inter Regular (OFL-1.1), bundled in the repository.
pub static TEXT_FONT: &[u8] = include_bytes!("../../../assets/fonts/inter/Inter-Regular.otf");

/// One sample sentence per interface language, using the glossary terms and the accented
/// characters each language needs (ß, ü, é, ñ, ’…).
pub const TEXTS: [(&str, &str); 6] = [
    ("en-US", "Measure 12 — quarter note, tie, staff"),
    ("en-GB", "Bar 12 — crotchet, tie, stave"),
    ("it", "Battuta 12 — semiminima, legatura di valore, rigo"),
    ("fr", "Mesure 12 — noire, liaison d’expression, portée"),
    ("de", "Takt 12 — Viertelnote, Haltebogen, Größe ändern"),
    (
        "es",
        "Compás 12 — negra, ligadura de prolongación, pentagrama",
    ),
];

/// SMuFL code points used by the scene (from the SMuFL specification).
pub mod smufl {
    /// gClef.
    pub const G_CLEF: char = '\u{E050}';
    /// noteheadBlack.
    pub const NOTEHEAD_BLACK: char = '\u{E0A4}';
    /// noteheadHalf.
    pub const NOTEHEAD_HALF: char = '\u{E0A3}';
    /// accidentalSharp.
    pub const SHARP: char = '\u{E262}';
    /// accidentalFlat.
    pub const FLAT: char = '\u{E260}';
    /// restQuarter.
    pub const REST_QUARTER: char = '\u{E4E5}';
}

/// A glyph of the music font. `(x, y)` is the glyph origin (SMuFL: on the staff position it
/// refers to), in sp. Font size is always 4 sp (SMuFL: 1 em = 4 staff spaces).
#[derive(Clone, Copy, Debug)]
pub struct Glyph {
    /// Horizontal position, sp.
    pub x: f32,
    /// Vertical position (down is positive), sp.
    pub y: f32,
    /// SMuFL code point.
    pub ch: char,
}

/// A straight line (staff line, barline, stem), in sp.
#[derive(Clone, Copy, Debug)]
pub struct Line {
    /// Start x, sp.
    pub x0: f32,
    /// Start y, sp.
    pub y0: f32,
    /// End x, sp.
    pub x1: f32,
    /// End y, sp.
    pub y1: f32,
    /// Stroke width, sp.
    pub width: f32,
}

/// The whole test scene.
#[derive(Clone, Debug)]
pub struct Scene {
    /// Exactly [`GLYPHS`] music glyphs.
    pub glyphs: Vec<Glyph>,
    /// Staff lines, barlines and stems.
    pub lines: Vec<Line>,
    /// Total width, sp.
    pub width: f32,
    /// Total height, sp.
    pub height: f32,
}

/// Builds the scene deterministically (a tiny linear congruential generator chooses pitches, so
/// every toolkit and every run sees identical content).
pub fn build() -> Scene {
    let mut glyphs = Vec::with_capacity(GLYPHS);
    let mut lines = Vec::new();
    let mut seed: u32 = 0x5EED;
    let mut next = || {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        seed >> 16
    };
    let per_staff = GLYPHS / STAVES; // 200
    let width = CLEF_WIDTH + MEASURES as f32 * MEASURE_WIDTH + 1.0;
    for staff in 0..STAVES {
        let top = 2.0 + staff as f32 * STAFF_DISTANCE;
        for l in 0..5 {
            let y = top + l as f32;
            lines.push(Line {
                x0: 0.5,
                y0: y,
                x1: width - 0.5,
                y1: y,
                width: 0.13,
            });
        }
        for m in 0..=MEASURES {
            let x = CLEF_WIDTH + m as f32 * MEASURE_WIDTH;
            lines.push(Line {
                x0: x,
                y0: top,
                x1: x,
                y1: top + 4.0,
                width: 0.16,
            });
        }
        glyphs.push(Glyph {
            x: 1.0,
            y: top + 3.0,
            ch: smufl::G_CLEF,
        });
        // 199 more glyphs: ~10 per measure, as notes (sometimes with an accidental) and rests.
        let mut count = 1;
        let mut slot = 0;
        while count < per_staff {
            // 9 slots per measure, 0.95 sp apart: 180 slots hold the ~165 a staff needs.
            let measure = slot / 9;
            let in_measure = slot % 9;
            let x = CLEF_WIDTH + measure as f32 * MEASURE_WIDTH + 1.0 + in_measure as f32 * 0.95;
            let kind = next() % 10;
            if kind == 0 {
                glyphs.push(Glyph {
                    x,
                    y: top + 2.0,
                    ch: smufl::REST_QUARTER,
                });
                count += 1;
            } else {
                // Staff position: 0 = top line, 4 = bottom line, half steps in between.
                let position = (next() % 9) as f32 * 0.5;
                let y = top + position;
                if kind <= 2 && count + 1 < per_staff {
                    let accidental = if kind == 1 { smufl::SHARP } else { smufl::FLAT };
                    glyphs.push(Glyph {
                        x: x - 0.9,
                        y,
                        ch: accidental,
                    });
                    count += 1;
                }
                let head = if kind == 9 {
                    smufl::NOTEHEAD_HALF
                } else {
                    smufl::NOTEHEAD_BLACK
                };
                glyphs.push(Glyph { x, y, ch: head });
                count += 1;
                // Stem up from the right side of the notehead (≈ 1.18 sp wide).
                lines.push(Line {
                    x0: x + 1.12,
                    y0: y - 0.15,
                    x1: x + 1.12,
                    y1: y - 3.5,
                    width: 0.12,
                });
            }
            slot += 1;
        }
    }
    debug_assert_eq!(glyphs.len(), GLYPHS);
    Scene {
        glyphs,
        lines,
        width,
        height: 2.0 + STAVES as f32 * STAFF_DISTANCE,
    }
}

/// Height of the menu bar at the top of the window, px.
pub const MENU_HEIGHT: f32 = 28.0;
/// Menu titles drawn in the menu bar (the real app takes them from orchestra-i18n).
pub const MENU_ITEMS: [&str; 3] = ["File", "Edit", "View"];
/// Top of the score area, px: menu bar + a band with the six sample texts in two columns.
pub const SCORE_TOP: f32 = 92.0;
/// Size of the sample texts, px.
pub const TEXT_SIZE: f32 = 15.0;

/// Baseline positions (px) of the six sample texts: three rows, two columns, under the menu bar.
pub fn text_positions() -> [(f32, f32); 6] {
    let row = |r: usize| MENU_HEIGHT + 20.0 + r as f32 * 20.0;
    [
        (16.0, row(0)),
        (16.0, row(1)),
        (16.0, row(2)),
        (976.0, row(0)),
        (976.0, row(1)),
        (976.0, row(2)),
    ]
}

/// A view on the scene: what part is visible and how large.
#[derive(Clone, Copy, Debug)]
pub struct View {
    /// Scene x (sp) shown at the left edge.
    pub scroll_x: f32,
    /// Scene y (sp) shown at the top edge.
    pub scroll_y: f32,
    /// Zoom factor; pixels per sp = `BASE_PX_PER_SP * zoom`.
    pub zoom: f32,
}

impl View {
    /// The view used for screenshots: whole score visible.
    pub const OVERVIEW: View = View {
        scroll_x: 0.0,
        scroll_y: 0.0,
        zoom: 1.0,
    };

    /// Pixels per staff space.
    pub fn px_per_sp(self) -> f32 {
        BASE_PX_PER_SP * self.zoom
    }

    /// Maps a scene point (sp) to window pixels (the score area starts at [`SCORE_TOP`]).
    pub fn to_px(self, x: f32, y: f32) -> (f32, f32) {
        (
            (x - self.scroll_x) * self.px_per_sp(),
            SCORE_TOP + (y - self.scroll_y) * self.px_per_sp(),
        )
    }
}

/// The deterministic benchmark path: 120 frames that zoom between 0.6× and 1.6× while scrolling
/// diagonally, i.e. what a user does when navigating a score.
pub fn bench_views() -> Vec<View> {
    (0..120)
        .map(|i| {
            let t = i as f32 / 119.0;
            let zoom = 1.1 + 0.5 * (t * std::f32::consts::TAU).sin();
            View {
                scroll_x: t * 60.0,
                scroll_y: t * 30.0,
                zoom,
            }
        })
        .collect()
}

/// Peak resident memory of this process (VmHWM), in KiB. Linux only (`/proc`); `None` elsewhere.
pub fn peak_rss_kib() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let line = status.lines().find(|l| l.starts_with("VmHWM:"))?;
    line.split_whitespace().nth(1)?.parse().ok()
}

/// Prints frame-time statistics in a fixed, grep-friendly format.
pub fn report(name: &str, mut times: Vec<Duration>) {
    times.sort();
    let ms = |d: Duration| d.as_secs_f64() * 1e3;
    let mean = times.iter().map(|d| ms(*d)).sum::<f64>() / times.len() as f64;
    let p = |q: f64| ms(times[((times.len() - 1) as f64 * q).round() as usize]);
    println!(
        "RESULT {name}: frames={} mean={mean:.2}ms p50={:.2}ms p95={:.2}ms max={:.2}ms peak_rss={}KiB",
        times.len(),
        p(0.5),
        p(0.95),
        p(1.0),
        peak_rss_kib().map_or("?".into(), |k| k.to_string()),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The brief requires exactly 2000 glyphs.
    #[test]
    fn scene_has_2000_glyphs() {
        assert_eq!(build().glyphs.len(), GLYPHS);
    }
}
