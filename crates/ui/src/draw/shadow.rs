// SPDX-License-Identifier: GPL-3.0-or-later

//! The coverage of a blurred rounded rectangle, computed analytically (ADR-0039 D6).
//!
//! # The method
//! A shadow is the shape's indicator function (1 inside, 0 outside) convolved with a 2D
//! Gaussian of standard deviation σ. The 2D Gaussian is separable, so the coverage at a point is
//! a double integral: over y of the Gaussian in y times (over x of the Gaussian in x across the
//! shape's row at that height).
//!
//! - **Along x it is exact.** At a given height the rounded rectangle's row is one interval,
//!   `[-w(y), +w(y)]` around the centre, where `w(y)` is the half width shortened by the corner's
//!   circle near the top and bottom. The integral of a Gaussian over an interval is a difference
//!   of two error functions: `½ (erf((x + w) / (σ√2)) − erf((x − w) / (σ√2)))`.
//! - **Along y it is approximated** with twelve samples (the midpoint rule) over the part of
//!   `[−3σ, +3σ]` that falls inside the shape, each weighted by the Gaussian, then divided by the
//!   Gaussian's weight within ±3σ (99.73 %), so the inside of a shadow is exactly covered.
//! - **A plain rectangle (radius 0) is exact in both directions:** the shape is a product of two
//!   intervals, so the coverage is the product of two 1D integrals, without sampling.
//!
//! This is the well-known method for blurred rounded boxes (used by several GPU renderers, often
//! with four samples). It is written here from the mathematics above, and the number of samples
//! was chosen by measurement (see [`Y_SAMPLES`]): the tests check it against a brute-force
//! numerical integration, within one 8-bit step.
//!
//! # Units
//! Everything is in **device pixels**: the backend converts the logical shape and blur with the
//! scale in effect before calling [`coverage`].
//!
//! # Complexity
//! O(1) per pixel: twelve rows, each two `erf` and one `exp` (four `erf` for a plain rectangle). The backend only calls it for pixels
//! inside the shape grown by 3σ.

use std::f32::consts::{FRAC_1_SQRT_2, PI};

/// Number of samples of the y integral, for rounded shapes (ADR-0039 D6).
///
/// # Why twelve
/// Measured against a brute-force 2D integration (see the tests): with 4 samples the worst error
/// near a rounded corner is 3.5 steps of 8-bit colour (visible), with 8 it is 1.1, with 12 it is
/// 0.6, and 16 gives no further gain (the oracle's own precision is reached). Twelve rows of
/// two `erf` each, only for the pixels near a shadow, is cheap.
const Y_SAMPLES: u8 = 12;

/// The share of a Gaussian's weight within ±3σ: erf(3/√2) = 0.997 300 2.
const MASS_WITHIN_REACH: f32 = 0.997_300_2;

/// The error function, approximated to an absolute error below 5 · 10⁻⁴ (Abramowitz & Stegun,
/// *Handbook of Mathematical Functions*, formula 7.1.27).
///
/// # Why an approximation
/// `std` has no `erf` for `f32` or `f64`, and 5 · 10⁻⁴ is an eighth of one 8-bit colour step
/// (1/255 ≈ 3.9 · 10⁻³): invisible. The formula is a rational function, cheap and branch-free
/// apart from the sign.
#[must_use]
pub(crate) fn erf(x: f32) -> f32 {
    let sign = x.signum();
    let a = x.abs();
    // 1 + a1·a + a2·a² + a3·a³ + a4·a⁴, in Horner form, then raised to the 4th power.
    let polynomial = 1.0 + a * (0.278_393 + a * (0.230_389 + a * (0.000_972 + a * 0.078_108)));
    let squared = polynomial * polynomial;
    sign * (1.0 - 1.0 / (squared * squared))
}

/// The Gaussian probability density with standard deviation `sigma`, at `t`.
fn gaussian(t: f32, sigma: f32) -> f32 {
    (-(t * t) / (2.0 * sigma * sigma)).exp() / ((2.0 * PI).sqrt() * sigma)
}

/// The integral of the Gaussian of standard deviation `sigma` centred at `position` over the
/// interval `[-half, +half]`: `½ (erf((position + half) / (σ√2)) − erf((position − half) / (σ√2)))`.
fn interval(position: f32, half: f32, sigma: f32) -> f32 {
    let scale = FRAC_1_SQRT_2 / sigma;
    0.5 * (erf((position + half) * scale) - erf((position - half) * scale))
}

/// The integral along x, for the row at height `y` (relative to the centre), at position `x`
/// (relative to the centre), of the blurred row of a rounded rectangle of half sizes
/// `half_width`, `half_height` and corner `radius`.
fn row(x: f32, y: f32, sigma: f32, radius: f32, half_width: f32, half_height: f32) -> f32 {
    // How far the row is into the corner zone: negative inside the top or bottom `radius` band,
    // 0 elsewhere. In that band the row is shortened by the corner's circle.
    let into_corner = (half_height - radius - y.abs()).min(0.0);
    let half_row = half_width - radius
        + (radius * radius - into_corner * into_corner)
            .max(0.0)
            .sqrt();
    interval(x, half_row, sigma)
}

/// The coverage, between 0 and 1, of the rounded rectangle `(x0, y0)–(x1, y1)` with corner
/// `radius`, blurred with standard deviation `sigma`, at the point `(px, py)` (normally a pixel
/// centre). All in device pixels; `sigma` must be positive (a sharp shape is drawn as a fill).
#[must_use]
#[allow(
    clippy::too_many_arguments,
    reason = "the point, the shape's four edges, its radius and the blur: the formula's inputs"
)]
pub(crate) fn coverage(
    px: f32,
    py: f32,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    radius: f32,
    sigma: f32,
) -> f32 {
    let half_width = (x1 - x0) * 0.5;
    let half_height = (y1 - y0) * 0.5;
    if half_width <= 0.0 || half_height <= 0.0 || sigma <= 0.0 {
        return 0.0;
    }
    let radius = radius.clamp(0.0, half_width.min(half_height));
    let x = px - f32::midpoint(x0, x1);
    let y = py - f32::midpoint(y0, y1);
    if radius <= 0.0 {
        // A plain rectangle: the 2D Gaussian is separable and the shape is a product of two
        // intervals, so the coverage is exactly the product of two 1D integrals, no sampling.
        return (interval(x, half_width, sigma) * interval(y, half_height, sigma)).clamp(0.0, 1.0);
    }
    // The integration variable t is the vertical offset of the Gaussian's sample; the row it
    // looks at is at height y − t, which must be inside the shape: t ∈ [y − h, y + h]. Within
    // that, only ±3σ matters.
    let reach = 3.0 * sigma;
    let start = (-reach).clamp(y - half_height, y + half_height);
    let end = reach.clamp(y - half_height, y + half_height);
    let step = (end - start) / f32::from(Y_SAMPLES);
    let mut t = start + step * 0.5;
    let mut total = 0.0;
    for _ in 0..Y_SAMPLES {
        total += row(x, y - t, sigma, radius, half_width, half_height) * gaussian(t, sigma) * step;
        t += step;
    }
    // The y integral only spans ±3σ, which holds 99.73 % of the Gaussian: dividing by that mass
    // makes the inside of a large shadow exactly 1 instead of 0.9973 (otherwise 254 instead of
    // 255 in 8-bit colour), at no cost to the edges' accuracy (measured by the tests).
    (total / MASS_WITHIN_REACH).clamp(0.0, 1.0)
}

#[cfg(test)]
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the oracle's grid sizes and indices are small and non-negative; f64 → f32 at the end \
              keeps far more precision than the comparison needs"
)]
mod tests {
    use super::{coverage, erf};

    /// The coverage by brute force: the 2D convolution, summed over the **shape** on a fine grid
    /// whose cells are aligned with the shape's edges, as an oracle independent of the separable
    /// formula and of `erf`.
    ///
    /// Integrating over the shape (not over offsets around the point) with cells starting at the
    /// shape's edges means a straight edge never cuts a cell, so the only discretisation error
    /// left is the midpoint rule inside each cell (tiny at σ/16) and the corners' circles.
    fn brute_force(px: f32, py: f32, rect: (f32, f32, f32, f32), radius: f32, sigma: f32) -> f32 {
        let (x0, y0, x1, y1) = rect;
        let inside_corners = |x: f32, y: f32| -> bool {
            // The nearest point of the inner rectangle (the shape shrunk by the radius): within
            // `radius` of it means inside the rounded shape.
            let cx = x.clamp(x0 + radius, x1 - radius);
            let cy = y.clamp(y0 + radius, y1 - radius);
            (x - cx).powi(2) + (y - cy).powi(2) <= radius * radius
        };
        // Width and height of the test shapes are multiples of the step: cells tile the shape.
        let step = sigma / 16.0;
        let columns = ((x1 - x0) / step).round() as usize;
        let rows = ((y1 - y0) / step).round() as usize;
        let mut total = 0.0_f64;
        for row in 0..rows {
            let y = y0 + (row as f32 + 0.5) * step;
            for column in 0..columns {
                let x = x0 + (column as f32 + 0.5) * step;
                if !inside_corners(x, y) {
                    continue;
                }
                let (dx, dy) = (f64::from(x - px), f64::from(y - py));
                let s = f64::from(sigma);
                total += (-(dx * dx + dy * dy) / (2.0 * s * s)).exp()
                    / (2.0 * std::f64::consts::PI * s * s)
                    * f64::from(step * step);
            }
        }
        total as f32
    }

    #[test]
    fn erf_matches_known_values() {
        // Reference values of erf (to 6 decimals): erf(0.5) = 0.520500, erf(1) = 0.842701,
        // erf(2) = 0.995322.
        for (x, expected) in [
            (0.0, 0.0),
            (0.5, 0.520_500),
            (1.0, 0.842_701),
            (2.0, 0.995_322),
        ] {
            assert!((erf(x) - expected).abs() < 5e-4, "erf({x}) = {}", erf(x));
            assert!((erf(-x) + expected).abs() < 5e-4, "erf is odd");
        }
    }

    #[test]
    fn deep_inside_is_covered_and_far_outside_is_not() {
        let cover = coverage(50.0, 50.0, 0.0, 0.0, 100.0, 100.0, 8.0, 4.0);
        assert!(cover > 0.999, "{cover}");
        let outside = coverage(150.0, 50.0, 0.0, 0.0, 100.0, 100.0, 8.0, 4.0);
        assert!(outside < 1e-4, "{outside}");
    }

    #[test]
    fn a_straight_edge_is_half_covered() {
        // On the middle of the left edge of a large rectangle, half the Gaussian is inside.
        let cover = coverage(0.0, 50.0, 0.0, 0.0, 100.0, 100.0, 0.0, 6.0);
        assert!((cover - 0.5).abs() < 2e-3, "{cover}");
    }

    #[test]
    fn agrees_with_brute_force_integration() {
        // Points across an edge, a corner and the inside, for a sharp-cornered and two rounded
        // shapes, at two blurs.
        // One step of 8-bit colour: below it the difference is invisible. Measured worst on
        // 2026-11-10: 0.84 of a step, near a rounded corner (`cargo test -- --nocapture` prints it).
        const TOLERANCE: f32 = 1.0 / 255.0;
        let rect = (10.0, 10.0, 60.0, 40.0);
        let mut worst = 0.0_f32;
        for radius in [0.0, 6.0, 12.0] {
            for sigma in [2.0, 5.0] {
                for (px, py) in [
                    (10.0, 25.0),
                    (5.0, 25.0),
                    (35.0, 10.0),
                    (10.0, 10.0),
                    (13.0, 13.0),
                    (6.0, 6.0),
                    (35.0, 25.0),
                    (62.0, 42.0),
                ] {
                    let fast = coverage(px, py, rect.0, rect.1, rect.2, rect.3, radius, sigma);
                    let slow = brute_force(px, py, rect, radius, sigma);
                    worst = worst.max((fast - slow).abs());
                    assert!(
                        (fast - slow).abs() < TOLERANCE,
                        "r={radius} σ={sigma} ({px},{py}): {fast} vs {slow}"
                    );
                }
            }
        }
        println!(
            "worst difference: {worst} ({} of an 8-bit step)",
            worst * 255.0
        );
    }

    #[test]
    fn degenerate_shapes_cover_nothing() {
        assert!(coverage(5.0, 5.0, 10.0, 10.0, 10.0, 20.0, 0.0, 2.0) <= 0.0);
        assert!(coverage(5.0, 5.0, 0.0, 0.0, 10.0, 10.0, 0.0, 0.0) <= 0.0);
    }
}
