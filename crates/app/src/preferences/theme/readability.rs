// SPDX-License-Identifier: GPL-3.0-or-later

//! The theme editor's **readability** warnings (ADR-0037 T8, SCALETTA F2.12b1, owner's choices of
//! 2026-11-26): the contrast failures of a theme told in plain words, without numbers, with
//! advice that always works.
//!
//! # What a warning says
//! Three sentences, each a whole Fluent message (so no grammar ever spans two of them):
//! 1. **the problem**, on the worst pair: "The color “Secondary text” is almost invisible on
//!    “Field background”." How bad, in words: below 2:1 *almost invisible*, below 3:1 *hard to
//!    read* (*hard to make out* for graphics, whose threshold is 3:1), up to 4.5:1 *readable, but
//!    not well enough for people who don't see well* (owner's thresholds);
//! 2. **how many more**, when the colour to fix fails with others too: "“Secondary text” has the
//!    same problem with 3 other colors.";
//! 3. **what to do**: "Make the color “Secondary text” lighter." — or, when only one pair is
//!    involved, also the other colour's way out ("…, or make the color “Field background”
//!    lighter."), or, when no lighter or darker colour can work, *Restore*.
//!
//! # The colour to fix (the "culprit")
//! A warning is about the colour the user changed, not always the one in front: making the
//! window's background white in a dark theme gives **one** warning about the background, not
//! nine about the texts on it. For each failing pair the culprit is, in order: the colour edited
//! last in this session (the editor remembers it), else the one of the two that differs from the
//! theme's base, else (both or neither changed) the foreground. The failing pairs are grouped by
//! culprit.
//!
//! # Advice that works (the owner's report of 2026-11-26)
//! A first version only said "move the colour away from the one behind it": with black text on a
//! dark background it said *make the text darker*, which is impossible. Now the advice is
//! **computed**: the culprit is moved step by step towards white, then towards black (keeping its
//! transparency), and a direction is advised only if some colour on that path makes **every** pair
//! the culprit belongs to pass (the failing ones and those that pass now, so the advice never
//! trades one problem for another). If both directions work, the one needing the smaller change
//! wins; if neither, the advice is *Restore* (the theme's base passes every pair, ADR-0037 T5).
//! The other colour's way out is offered only when the warning is about a single pair (choice c of
//! 2026-11-26): changing several backgrounds shared by other colours would break the theme.
//!
//! # Cost
//! Per edit, never per frame: for each culprit (at most 24) and direction, at most 64 + 16 steps,
//! each checking the culprit's pairs (at most 9) with `contrast_ratio`: a few tens of thousands of
//! arithmetic operations, far below a millisecond.

use orchestra_i18n::{FluentValue, I18nError, Localizer};
use orchestra_ui::{
    CONTRAST_PAIRS, Color, ColorToken, ContrastKind, ContrastPair, Theme, contrast_ratio,
};

use crate::template::Template;

/// How hard a colour is to see on the colour behind it, in words (owner's thresholds of
/// 2026-11-26). Ordered from the worst, so sorting puts the worst problems first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Severity {
    /// Below 2:1: almost invisible.
    Invisible,
    /// From 2:1 to below 3:1: hard to read (text) or to make out (graphics).
    Hard,
    /// From 3:1 to below 4.5:1, text only (graphics need 3:1): readable, but not well enough for
    /// people who do not see well.
    Weak,
}

impl Severity {
    /// The severity of a pair that reaches `ratio` (below its threshold: only failures are
    /// asked). 2:1 and 3:1 are the owner's choice; 3:1 is also WCAG's line for graphics and for
    /// large text, below which even large letters are hard to read.
    pub(crate) fn of(ratio: f64) -> Self {
        if ratio < 2.0 {
            Self::Invisible
        } else if ratio < 3.0 {
            Self::Hard
        } else {
            Self::Weak
        }
    }
}

/// A way to move a colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Direction {
    /// Towards white.
    Lighter,
    /// Towards black.
    Darker,
}

impl Direction {
    /// Both, in the order they are tried.
    const BOTH: [Self; 2] = [Self::Lighter, Self::Darker];

    /// The colour at the end of the path: white or black, with `alpha` kept (a translucent
    /// colour stays translucent: only its colour is advised, not its transparency).
    fn extreme(self, alpha: u8) -> Color {
        let rgb: u32 = match self {
            Self::Lighter => 0x00ff_ffff,
            Self::Darker => 0,
        };
        Color::rgba(rgb << 8 | u32::from(alpha))
    }

    /// The value the advice message selects on.
    const fn fluent(self) -> &'static str {
        match self {
            Self::Lighter => "lighter",
            Self::Darker => "darker",
        }
    }
}

/// What a warning advises (see the module documentation).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Advice {
    /// Move the culprit this way; `target` is the nearest colour that fixes it (the tests apply
    /// it).
    Change {
        /// The way.
        direction: Direction,
        /// The nearest colour on that path that makes every pair of the culprit pass.
        target: Color,
    },
    /// One pair only: move the culprit, or move the other colour of the pair.
    Either {
        /// The culprit's way.
        direction: Direction,
        /// The culprit's nearest fixing colour.
        target: Color,
        /// The other colour's way.
        partner_direction: Direction,
        /// The other colour's nearest fixing colour.
        partner_target: Color,
    },
    /// No lighter or darker colour works: restore the culprit's base value.
    Restore,
}

/// One warning: the failing pairs that share a culprit (see the module documentation).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Issue {
    /// The worst pair's foreground (the colour hard to see).
    pub(crate) foreground: ColorToken,
    /// The worst pair's background.
    pub(crate) background: ColorToken,
    /// The worst pair's kind: letters or a graphic.
    pub(crate) kind: ContrastKind,
    /// How bad the worst pair is.
    pub(crate) severity: Severity,
    /// The colour to fix: the worst pair's foreground or background.
    pub(crate) culprit: ColorToken,
    /// The worst pair's other colour.
    pub(crate) partner: ColorToken,
    /// How many other failing pairs the culprit is in.
    pub(crate) others: usize,
    /// What to do.
    pub(crate) advice: Advice,
    /// The worst ratio (never shown: it chooses the worst pair and the severity).
    worst: f64,
}

/// The culprit of `pair` (see the module documentation): the colour edited last if it is in the
/// pair, else the one that differs from `base`, else the foreground.
fn culprit_of(
    pair: ContrastPair,
    theme: &Theme,
    base: &Theme,
    last_edited: Option<ColorToken>,
) -> ColorToken {
    let (front, back) = (pair.foreground, pair.background);
    if let Some(last) = last_edited
        && (last == front || last == back)
    {
        return last;
    }
    let changed = |token: ColorToken| theme.color(token) != base.color(token);
    if changed(back) && !changed(front) {
        back
    } else {
        front
    }
}

/// Every pair of `CONTRAST_PAIRS` that `token` belongs to, in either role.
fn pairs_of(token: ColorToken) -> impl Iterator<Item = ContrastPair> {
    CONTRAST_PAIRS
        .iter()
        .copied()
        .filter(move |pair| pair.foreground == token || pair.background == token)
}

/// Whether every pair of `token` passes in `theme` with `token` set to `color`. O(pairs of the
/// token), at most 9.
fn all_pass(theme: &Theme, token: ColorToken, color: Color) -> bool {
    let theme = theme.with_color(token, color);
    pairs_of(token).all(|pair| {
        contrast_ratio(theme.color(pair.foreground), theme.color(pair.background))
            >= pair.kind.minimum()
    })
}

/// `from` moved by `t` (0 to 1) towards `to`, channel by channel, alpha kept from `from`.
fn mix(from: Color, to: Color, t: f64) -> Color {
    let channel = |a: u8, b: u8| -> u8 {
        let value = f64::from(a) + (f64::from(b) - f64::from(a)) * t;
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "clamped to 0–255 and rounded just before"
        )]
        let byte = value.round().clamp(0.0, 255.0) as u8;
        byte
    };
    Color::rgba(u32::from_be_bytes([
        channel(from.r, to.r),
        channel(from.g, to.g),
        channel(from.b, to.b),
        from.a,
    ]))
}

/// How far `a` is from `b`: the Euclidean distance of their red, green and blue bytes (only to
/// compare the two directions' changes).
fn distance(a: Color, b: Color) -> f64 {
    let d = |x: u8, y: u8| f64::from(x) - f64::from(y);
    (d(a.r, b.r).powi(2) + d(a.g, b.g).powi(2) + d(a.b, b.b).powi(2)).sqrt()
}

/// The nearest colour on the path from `token`'s value towards `direction`'s extreme that makes
/// every pair of `token` pass, or `None` if not even the extreme does.
///
/// # Why steps and not a plain bisection
/// Along the path a pair that passes now may fail in the middle (the colour crosses its partner's
/// lightness) and pass again further on, so "passes" is not monotonic in the distance travelled.
/// 64 even steps find the first stretch that passes; 16 bisections inside that step then refine
/// it to 1/1 000 000 of the path, well under one byte per channel.
fn nearest_fix(theme: &Theme, token: ColorToken, direction: Direction) -> Option<Color> {
    const STEPS: u32 = 64;
    let from = theme.color(token);
    let to = direction.extreme(from.a);
    let passes = |t: f64| all_pass(theme, token, mix(from, to, t));
    let step = (1..=STEPS).find(|&i| passes(f64::from(i) / f64::from(STEPS)))?;
    let (mut low, mut high) = (
        f64::from(step - 1) / f64::from(STEPS),
        f64::from(step) / f64::from(STEPS),
    );
    for _ in 0..16 {
        let middle = f64::midpoint(low, high);
        if passes(middle) {
            high = middle;
        } else {
            low = middle;
        }
    }
    Some(mix(from, to, high))
}

/// The direction needing the smaller change to fix `token`, with its target, or `None` when
/// neither works.
fn best_fix(theme: &Theme, token: ColorToken) -> Option<(Direction, Color)> {
    let from = theme.color(token);
    Direction::BOTH
        .into_iter()
        .filter_map(|direction| nearest_fix(theme, token, direction).map(|c| (direction, c)))
        .min_by(|(_, a), (_, b)| distance(from, *a).total_cmp(&distance(from, *b)))
}

/// The warnings of `theme` (the half shown), compared with `base` (the same half of the custom
/// theme's base pair), `last_edited` being the colour the user changed last in this session.
/// None for every built-in pair (ADR-0037 T5). The worst first, then in `CONTRAST_PAIRS`' order.
pub(crate) fn readability_issues(
    theme: &Theme,
    base: &Theme,
    last_edited: Option<ColorToken>,
) -> Vec<Issue> {
    let mut issues: Vec<Issue> = Vec::new();
    for failure in theme.contrast_failures() {
        let pair = failure.pair;
        let culprit = culprit_of(pair, theme, base, last_edited);
        let partner = if culprit == pair.foreground {
            pair.background
        } else {
            pair.foreground
        };
        if let Some(issue) = issues.iter_mut().find(|issue| issue.culprit == culprit) {
            issue.others += 1;
            if failure.ratio < issue.worst {
                issue.foreground = pair.foreground;
                issue.background = pair.background;
                issue.kind = pair.kind;
                issue.partner = partner;
                issue.worst = failure.ratio;
            }
        } else {
            issues.push(Issue {
                foreground: pair.foreground,
                background: pair.background,
                kind: pair.kind,
                severity: Severity::Weak,
                culprit,
                partner,
                others: 0,
                advice: Advice::Restore,
                worst: failure.ratio,
            });
        }
    }
    for issue in &mut issues {
        issue.severity = Severity::of(issue.worst);
        issue.advice = match best_fix(theme, issue.culprit) {
            None => Advice::Restore,
            Some((direction, target)) => {
                // The other colour's way out: only for a single pair (choice c of 2026-11-26).
                let partner = (issue.others == 0)
                    .then(|| best_fix(theme, issue.partner))
                    .flatten();
                match partner {
                    Some((partner_direction, partner_target)) => Advice::Either {
                        direction,
                        target,
                        partner_direction,
                        partner_target,
                    },
                    None => Advice::Change { direction, target },
                }
            }
        };
    }
    // Stable: equal severities keep the pairs' order.
    issues.sort_by_key(|issue| issue.severity);
    issues
}

/// The largest number of other pairs one colour can share a warning with: the most pairs any
/// token belongs to, minus one (the window's background is behind nine colours, so 8).
pub(crate) fn max_others() -> usize {
    ColorToken::ALL
        .iter()
        .map(|&token| pairs_of(token).count())
        .max()
        .unwrap_or(1)
        .saturating_sub(1)
}

/// The kinds and severities that can occur (graphics are never `Weak`: their threshold is 3:1),
/// in the problem templates' order, with the values the message selects on.
const KIND_SEVERITY: [(ContrastKind, Severity, &str, &str); 5] = [
    (ContrastKind::Text, Severity::Invisible, "text", "invisible"),
    (ContrastKind::Text, Severity::Hard, "text", "hard"),
    (ContrastKind::Text, Severity::Weak, "text", "weak"),
    (
        ContrastKind::Graphic,
        Severity::Invisible,
        "graphic",
        "invisible",
    ),
    (ContrastKind::Graphic, Severity::Hard, "graphic", "hard"),
];

/// The warnings' texts in one language, formatted once (see [`Template::with_values`]): the
/// messages select on values, which placeholders cannot stand for, so each is formatted for every
/// combination of them, with only the colours' names left as placeholders.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReadabilityTexts {
    /// `prefs-theme-readability-problem`, in `KIND_SEVERITY`'s order; `$color`, `$background`.
    problem: Vec<Template>,
    /// `prefs-theme-readability-same`, for 1 to [`max_others`] others; `$culprit`.
    same: Vec<Template>,
    /// `prefs-theme-readability-advice`: *change* lighter and darker, *either* in the four
    /// combinations of directions (culprit major), *restore*; `$culprit`, `$partner`.
    advice: Vec<Template>,
}

impl ReadabilityTexts {
    /// The texts in `localizer`'s language: 5 + 8 + 7 short formats.
    ///
    /// # Errors
    /// The localizer's, if a message is missing.
    pub(crate) fn new(localizer: &Localizer) -> Result<Self, I18nError> {
        let problem = KIND_SEVERITY
            .iter()
            .map(|&(_, _, kind, severity)| {
                Template::with_values(
                    localizer,
                    "prefs-theme-readability-problem",
                    &["color", "background"],
                    &[
                        ("kind", FluentValue::from(kind)),
                        ("severity", FluentValue::from(severity)),
                    ],
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        let same = (1..=max_others())
            .map(|count| {
                // A plural needs a number, not a text: Fluent picks "one" or "other" from it.
                let count = FluentValue::from(u32::try_from(count).unwrap_or(u32::MAX));
                Template::with_values(
                    localizer,
                    "prefs-theme-readability-same",
                    &["culprit"],
                    &[("count", count)],
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        let advice_of = |advice: &str, direction: Direction, partner: Direction| {
            Template::with_values(
                localizer,
                "prefs-theme-readability-advice",
                &["culprit", "partner"],
                &[
                    ("advice", FluentValue::from(advice.to_owned())),
                    ("direction", FluentValue::from(direction.fluent())),
                    ("partner-direction", FluentValue::from(partner.fluent())),
                ],
            )
        };
        let mut advice = Vec::with_capacity(7);
        for direction in Direction::BOTH {
            advice.push(advice_of("change", direction, direction)?);
        }
        for direction in Direction::BOTH {
            for partner in Direction::BOTH {
                advice.push(advice_of("either", direction, partner)?);
            }
        }
        advice.push(advice_of(
            "restore",
            Direction::Lighter,
            Direction::Lighter,
        )?);
        Ok(Self {
            problem,
            same,
            advice,
        })
    }

    /// Every template with how many times each of its placeholders must appear (tests): the
    /// problem names both colours once; "the same" names the culprit once; the advice names the
    /// culprit once, and the partner once in its *either* forms only.
    pub(crate) fn expectations(&self) -> Vec<(&Template, [usize; 2])> {
        let mut out: Vec<(&Template, [usize; 2])> = Vec::new();
        out.extend(self.problem.iter().map(|t| (t, [1, 1])));
        out.extend(self.same.iter().map(|t| (t, [1, 0])));
        for (index, template) in self.advice.iter().enumerate() {
            let either = (2..6).contains(&index);
            out.push((template, [1, usize::from(either)]));
        }
        out
    }

    /// The template of `advice` (see the field's order).
    fn advice_template(&self, advice: &Advice) -> Option<&Template> {
        let index = |d: Direction| usize::from(d == Direction::Darker);
        let at = match advice {
            Advice::Change { direction, .. } => index(*direction),
            Advice::Either {
                direction,
                partner_direction,
                ..
            } => 2 + index(*direction) * 2 + index(*partner_direction),
            Advice::Restore => 6,
        };
        self.advice.get(at)
    }
}

/// The warning of `issue` in the texts' language: its sentences joined by a space, `name` giving
/// each colour's name as the editor's rows write it. A sentence whose combination cannot occur is
/// left out.
///
/// # Why joining is not concatenation (CLAUDE.md §7)
/// Each part is a whole sentence, formatted by Fluent with its own grammar; no word of one depends
/// on another, and the six languages all separate sentences with a space.
pub(crate) fn issue_line<'a>(
    texts: &ReadabilityTexts,
    name: impl Fn(ColorToken) -> &'a str,
    issue: &Issue,
) -> String {
    let mut sentences = Vec::with_capacity(3);
    if let Some(template) = KIND_SEVERITY
        .iter()
        .position(|(kind, severity, _, _)| *kind == issue.kind && *severity == issue.severity)
        .and_then(|index| texts.problem.get(index))
    {
        sentences.push(template.fill(&[name(issue.foreground), name(issue.background)]));
    }
    if let Some(template) = issue
        .others
        .checked_sub(1)
        .and_then(|index| texts.same.get(index))
    {
        sentences.push(template.fill(&[name(issue.culprit)]));
    }
    if let Some(template) = texts.advice_template(&issue.advice) {
        sentences.push(template.fill(&[name(issue.culprit), name(issue.partner)]));
    }
    sentences.join(" ")
}

#[cfg(test)]
mod tests {
    //! The thresholds, the culprit, and advice that always works: every built-in pair, both
    //! halves, every colour set to black, white and a middle grey.
    use super::*;
    use orchestra_ui::{Appearance, BuiltInTheme};

    /// Below 2:1 almost invisible, below 3:1 hard, then weak.
    #[test]
    fn severity_follows_the_thresholds() {
        assert_eq!(Severity::of(1.0), Severity::Invisible);
        assert_eq!(Severity::of(1.99), Severity::Invisible);
        assert_eq!(Severity::of(2.0), Severity::Hard);
        assert_eq!(Severity::of(2.99), Severity::Hard);
        assert_eq!(Severity::of(3.0), Severity::Weak);
        assert_eq!(Severity::of(4.49), Severity::Weak);
    }

    /// Studio's dark half.
    fn dark() -> Theme {
        *BuiltInTheme::Studio.pair().instance(Appearance::Dark)
    }

    /// The owner's case: secondary text made pure black on the dark half. One warning about it,
    /// on its darkest background, with three more; the advice is to make it **lighter** (darker
    /// is impossible), and only that (four backgrounds).
    #[test]
    fn black_text_on_a_dark_theme_is_made_lighter() {
        let base = dark();
        let theme = base.with_color(ColorToken::TextMuted, Color::rgba(0x0000_00ff));
        let issues = readability_issues(&theme, &base, Some(ColorToken::TextMuted));
        assert_eq!(issues.len(), 1, "{issues:?}");
        let issue = issues[0];
        assert_eq!(issue.culprit, ColorToken::TextMuted);
        assert_eq!(issue.severity, Severity::Invisible);
        assert_eq!(issue.others, 3);
        assert!(
            matches!(
                issue.advice,
                Advice::Change {
                    direction: Direction::Lighter,
                    ..
                }
            ),
            "{issue:?}"
        );
    }

    /// The window's background made white on the dark half: one warning about the background
    /// (not one per text), to be made darker.
    #[test]
    fn a_white_background_is_one_warning_about_the_background() {
        let base = dark();
        let theme = base.with_color(ColorToken::Surface, Color::rgba(0xffff_ffff));
        // Whether or not the editor remembers the last edit: the background differs from the base.
        for last in [Some(ColorToken::Surface), None] {
            let issues = readability_issues(&theme, &base, last);
            assert_eq!(issues.len(), 1, "{issues:?}");
            assert_eq!(issues[0].culprit, ColorToken::Surface);
            assert!(issues[0].others >= 5, "{issues:?}");
            assert!(matches!(
                issues[0].advice,
                Advice::Change {
                    direction: Direction::Darker,
                    ..
                }
            ));
        }
    }

    /// A single pair: selected text made the selection's own colour (selected text has only
    /// that pair; text on the accent has three, the accent's three states). Both ways out are
    /// given, each actually fixing the pair.
    #[test]
    fn a_single_pair_offers_both_colours() {
        let base = dark();
        let selection = base.color(ColorToken::Selection);
        let theme = base.with_color(ColorToken::OnSelection, selection);
        let issues = readability_issues(&theme, &base, Some(ColorToken::OnSelection));
        assert_eq!(issues.len(), 1, "{issues:?}");
        let issue = issues[0];
        assert_eq!(issue.culprit, ColorToken::OnSelection);
        assert_eq!(issue.partner, ColorToken::Selection);
        assert_eq!(issue.others, 0);
        let Advice::Either {
            target,
            partner_target,
            ..
        } = issue.advice
        else {
            panic!("{issue:?}");
        };
        assert!(all_pass(&theme, ColorToken::OnSelection, target));
        assert!(all_pass(&theme, ColorToken::Selection, partner_target));
    }

    /// When both ways work, the smaller change wins: control borders the grey of mid-grey
    /// surfaces can reach 3:1 both towards black (about `#323232`) and towards white (about
    /// `#d9d9d9`); black is nearer to `#7a7a7a`.
    #[test]
    fn the_nearer_way_wins() {
        let grey = Color::rgba(0x7a7a_7aff);
        let base = dark()
            .with_color(ColorToken::Surface, grey)
            .with_color(ColorToken::SurfaceRaised, grey)
            .with_color(ColorToken::SurfaceSunken, grey);
        let theme = base.with_color(ColorToken::ControlBorder, grey);
        assert!(nearest_fix(&theme, ColorToken::ControlBorder, Direction::Lighter).is_some());
        assert!(nearest_fix(&theme, ColorToken::ControlBorder, Direction::Darker).is_some());
        let issue = readability_issues(&theme, &base, Some(ColorToken::ControlBorder))
            .into_iter()
            .find(|issue| issue.culprit == ColorToken::ControlBorder)
            .unwrap();
        assert!(
            matches!(
                issue.advice,
                Advice::Change {
                    direction: Direction::Darker,
                    ..
                }
            ),
            "{issue:?}"
        );
    }

    /// The colour edited last is the culprit even when both changed; without it, the foreground.
    #[test]
    fn the_last_edit_is_the_culprit() {
        let base = dark();
        let grey = Color::rgba(0x8080_80ff);
        let theme = base
            .with_color(ColorToken::OnSelection, grey)
            .with_color(ColorToken::Selection, grey);
        let pair = CONTRAST_PAIRS
            .iter()
            .copied()
            .find(|p| p.foreground == ColorToken::OnSelection)
            .unwrap();
        assert_eq!(
            culprit_of(pair, &theme, &base, Some(ColorToken::Selection)),
            ColorToken::Selection
        );
        assert_eq!(
            culprit_of(pair, &theme, &base, None),
            ColorToken::OnSelection
        );
        assert_eq!(
            culprit_of(pair, &theme, &base, Some(ColorToken::Text)),
            ColorToken::OnSelection,
            "an edit outside the pair does not count"
        );
    }

    /// Whether no pair of `token` fails in `theme` with `token` set to `color`, by the theme's
    /// own check (`contrast_failures`): an oracle independent of `all_pass`, which the code under
    /// test uses.
    fn fixed(theme: &Theme, token: ColorToken, color: Color) -> bool {
        theme
            .with_color(token, color)
            .contrast_failures()
            .all(|f| f.pair.foreground != token && f.pair.background != token)
    }

    /// Checks one warning's advice (see `the_advice_always_works`).
    fn check_advice(theme: &Theme, base: &Theme, issue: &Issue, case: &str) {
        let from = theme.color(issue.culprit);
        let targets = match issue.advice {
            Advice::Change { direction, target } => vec![(issue.culprit, direction, target)],
            Advice::Either {
                direction,
                target,
                partner_direction,
                partner_target,
            } => vec![
                (issue.culprit, direction, target),
                (issue.partner, partner_direction, partner_target),
            ],
            Advice::Restore => {
                let restored = base.color(issue.culprit);
                assert!(fixed(theme, issue.culprit, restored), "{case}");
                // Restore only when no lighter or darker colour works.
                for direction in Direction::BOTH {
                    let extreme = direction.extreme(from.a);
                    assert!(
                        !fixed(theme, issue.culprit, extreme),
                        "{case} {direction:?}"
                    );
                }
                return;
            }
        };
        let sum = |c: Color| u32::from(c.r) + u32::from(c.g) + u32::from(c.b);
        for (token, direction, target) in targets {
            let start = theme.color(token);
            // Doing what it says works.
            assert!(fixed(theme, token, target), "{case} {token:?}");
            // The way advised is the way the target lies.
            match direction {
                Direction::Lighter => assert!(sum(target) >= sum(start), "{case}"),
                Direction::Darker => assert!(sum(target) <= sum(start), "{case}"),
            }
            // The nearest on that way: a tenth less of the change does not work yet (unless the
            // change is a step of a few bytes, where rounding decides).
            let before = mix(start, target, 0.9);
            if distance(start, target) > 8.0 {
                assert!(
                    !fixed(theme, token, before),
                    "{case} {token:?} not the nearest"
                );
            }
            // The smaller of the two ways, when both work.
            let other = match direction {
                Direction::Lighter => Direction::Darker,
                Direction::Darker => Direction::Lighter,
            };
            if let Some(alternative) = nearest_fix(theme, token, other) {
                assert!(
                    distance(start, target) <= distance(start, alternative),
                    "{case} {token:?} not the smaller change"
                );
            }
        }
    }

    /// Every case a user can make with one colour: each built-in pair, both halves, every colour
    /// token set to black, white or a middle grey, with and without the last edit remembered. No
    /// built-in theme warns; and for every warning, doing what it says works (checked by the
    /// theme's own contrast check), the way advised is the right one, the colour advised is the
    /// nearest, the smaller of the two ways is chosen, and *Restore* comes only when nothing else
    /// works.
    #[test]
    fn the_advice_always_works() {
        let values = [0x0000_00ff, 0xffff_ffff, 0x7777_77ff];
        let mut warnings = 0_usize;
        for pair in BuiltInTheme::ALL {
            for half in [Appearance::Light, Appearance::Dark] {
                let base = *pair.pair().instance(half);
                assert_eq!(
                    readability_issues(&base, &base, None),
                    [],
                    "{pair:?} {half:?}"
                );
                for &token in ColorToken::ALL {
                    for value in values {
                        let theme = base.with_color(token, Color::rgba(value));
                        for last in [Some(token), None] {
                            for issue in readability_issues(&theme, &base, last) {
                                warnings += 1;
                                let case = format!("{pair:?} {half:?} {token:?} {value:08x}");
                                assert_eq!(issue.culprit, token, "{case}: the colour changed");
                                check_advice(&theme, &base, &issue, &case);
                            }
                        }
                    }
                }
            }
        }
        assert!(warnings > 500, "the cases do produce warnings: {warnings}");
    }
}
