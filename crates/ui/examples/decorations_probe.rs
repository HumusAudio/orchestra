// SPDX-License-Identifier: GPL-3.0-or-later

//! Asks the system who draws the window's frame (ADR-0049 TB2, F2.11b2) and prints the answer and
//! how long the question took: the measure of the probe's cost at start-up, and a quick check by
//! hand on a desktop (`cargo run -p orchestra-ui --example decorations_probe`). With
//! `ORCHESTRA_DECORATIONS=own` or `=system` the answer is forced and nothing is asked.

#![allow(
    clippy::print_stdout,
    reason = "an example run by hand: printing the answer is its whole job"
)]

use std::time::Instant;

/// Asks ten times (the first one includes loading `libwayland-client`) and prints each answer
/// and time, then the fastest.
fn main() {
    let mut fastest = None;
    for run in 1..=10 {
        let start = Instant::now();
        let answer = orchestra_ui::decorations();
        let took = start.elapsed();
        println!("run {run:2}: {answer:?} in {took:?}"); // i18n-ignore: developer output
        fastest = Some(fastest.map_or(took, |best: std::time::Duration| best.min(took)));
    }
    if let Some(best) = fastest {
        println!("fastest: {best:?}"); // i18n-ignore: developer output
    }
}
