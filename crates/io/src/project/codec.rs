// SPDX-License-Identifier: GPL-3.0-or-later

//! The text forms of the values a project file writes as strings (ADR-0029 S6; owner's choices
//! I2, I4, I5 of 2026-11-06), each with a **strict** reader: one value, one spelling.
//!
//! # Why strings for these, and objects for the rest?
//! I2: compact strings only for what occurs **per note** (positions, note values, pitches,
//! ornaments), where size and reading time matter (the spike of `spikes/fileformat/` measured
//! compact values reading twice as fast as structured ones); everything rare (marks, form,
//! instruments) is written as objects with full field names, readable without a manual.
//!
//! # How each type is wired to serde
//! Each text form is a newtype (`Rat`, `PitchText`, …) implementing `Serialize` (its text) and
//! `Deserialize` (a string, parsed strictly; a bad text becomes a serde "custom" error, which
//! `serde_json` reports with the line and column of the value). The schema
//! (`crate::project::schema`) uses the newtypes as field types, so `Option<Rat>` and `Vec<Rat>`
//! work with no extra code.
//!
//! The enumerations are written with lower-case English identifiers chosen here, explicitly, one
//! by one (`named!`): renaming a Rust variant never changes the file (ADR-0029 S6). They are file
//! identifiers, never shown to the user, never translated.

use std::fmt;

use orchestra_core_model::{
    Alteration, Arpeggio, Articulation, BarNumbering, BarRepeatSpan, BaseValue, Clef, ClefSign,
    DynamicLevel, Fermata, GraceAttach, GraceKind, HairpinKind, HairpinStyle, MarkerKind, Mode,
    NoteValue, Ornament, Pitch, RepeatsAfterJump, RestValue, SlurPlacement, Step, Tolerance,
    TremoloStrokes, TupletBracket, TupletNumber,
};
use orchestra_core_time::{BarKind, BeatGrouping, Duration, Meter, Rational};
use serde::de::{self, Deserializer, Visitor};
use serde::{Deserialize, Serialize, Serializer};

/// Reads a string and parses it with `parse`, turning a parse failure into a serde error that
/// names what was expected and the offending text. Shared by every text newtype. O(text).
fn parse_str<'de, D, T>(
    deserializer: D,
    expected: &'static str,
    parse: fn(&str) -> Option<T>,
) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
{
    /// The visitor: accepts only strings (a number or an object is a type error).
    struct TextVisitor<T> {
        /// What the text means, for the error message ("a pitch such as \"F#5\"").
        expected: &'static str,
        /// The strict reader.
        parse: fn(&str) -> Option<T>,
    }
    impl<T> Visitor<'_> for TextVisitor<T> {
        type Value = T;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str(self.expected)
        }
        fn visit_str<E: de::Error>(self, v: &str) -> Result<T, E> {
            (self.parse)(v).ok_or_else(|| {
                E::custom(format_args!(
                    "{v:?} is not {} in canonical form",
                    self.expected
                ))
            })
        }
    }
    deserializer.deserialize_str(TextVisitor { expected, parse })
}

/// Defines a text newtype: `$name($inner)`, serialised with `$encode` and read with `$decode`
/// (strict; `None` means "not in canonical form").
macro_rules! text {
    ($(#[$doc:meta])* $name:ident($inner:ty), $expected:literal, $encode:path, $decode:path) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub(crate) struct $name(pub(crate) $inner);

        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_str(&$encode(self.0))
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                parse_str(deserializer, $expected, $decode).map(Self)
            }
        }
    };
}

/// Defines a named enumeration: a newtype over a model enum, written as one of the given
/// lower-case names, each spelled out here (renaming a Rust variant never changes the file).
macro_rules! named {
    ($(#[$doc:meta])* $name:ident($inner:ty), $expected:literal, { $($variant:path => $text:literal),+ $(,)? }) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub(crate) struct $name(pub(crate) $inner);

        impl $name {
            /// The file name of a value. O(1).
            pub(crate) fn encode(value: $inner) -> &'static str {
                match value { $($variant => $text),+ }
            }
            /// The value of a file name, or `None` for an unknown name. O(variants).
            pub(crate) fn decode(text: &str) -> Option<$inner> {
                match text { $($text => Some($variant),)+ _ => None }
            }
        }

        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_str(Self::encode(self.0))
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                parse_str(deserializer, $expected, Self::decode).map(Self)
            }
        }
    };
}

// ------------------------------------------------------------------------------------------------
// Rationals and durations
// ------------------------------------------------------------------------------------------------

/// The canonical text of a rational (`core-time`'s `Display`). O(digits).
fn encode_rational(r: Rational) -> String {
    r.to_string()
}

/// A rational in canonical form only (`core-time`'s strict `FromStr`). O(digits).
fn decode_rational(text: &str) -> Option<Rational> {
    text.parse().ok()
}

/// The text of a duration: its rational, in whole notes. O(digits).
fn encode_duration(d: Duration) -> String {
    d.as_rational().to_string()
}

/// A duration in canonical form. O(digits).
fn decode_duration(text: &str) -> Option<Duration> {
    decode_rational(text).map(Duration::from_rational)
}

text!(
    /// A rational: `"3/16"`, `"-1/8"`, `"2"` (ADR-0029 S6).
    Rat(Rational), "a rational such as \"3/16\"", encode_rational, decode_rational
);
text!(
    /// A duration or an offset in whole notes, as a rational: `"1/4"` is a quarter note.
    Dur(Duration), "a duration such as \"1/4\"", encode_duration, decode_duration
);

// ------------------------------------------------------------------------------------------------
// Pitches
// ------------------------------------------------------------------------------------------------

/// The letter of a step. O(1).
const fn step_letter(step: Step) -> char {
    match step {
        Step::C => 'C',
        Step::D => 'D',
        Step::E => 'E',
        Step::F => 'F',
        Step::G => 'G',
        Step::A => 'A',
        Step::B => 'B',
    }
}

/// The step of a letter (upper case only). O(1).
const fn letter_step(letter: u8) -> Option<Step> {
    match letter {
        b'C' => Some(Step::C),
        b'D' => Some(Step::D),
        b'E' => Some(Step::E),
        b'F' => Some(Step::F),
        b'G' => Some(Step::G),
        b'A' => Some(Step::A),
        b'B' => Some(Step::B),
        _ => None,
    }
}

/// The signs of the chromatic alterations other than the natural: `#x` (triple sharp), `x`
/// (double sharp), `#`, `bbb`, `bb`, `b`, with their semitones.
///
/// One table for both directions (review point B18: the decoder had a copy of it). The order
/// matters for decoding, which takes the first sign that prefixes the text: `#x` before `#`, and
/// `bbb` before `bb` before `b`, so the longest sign wins.
const SIGNS: [(&str, i64); 6] = [
    ("#x", 3),
    ("x", 2),
    ("#", 1),
    ("bbb", -3),
    ("bb", -2),
    ("b", -1),
];

/// The sign of a chromatic alteration of -3 to +3 semitones: `""` for the natural, otherwise
/// the entry of [`SIGNS`]. `None` outside that range. O(1) (six entries).
fn chromatic_sign(semitones: i64) -> Option<&'static str> {
    if semitones == 0 {
        return Some("");
    }
    SIGNS
        .iter()
        .find(|&&(_, n)| n == semitones)
        .map(|&(sign, _)| sign)
}

/// The text of an alteration: a chromatic sign, or the exact semitones in brackets for a
/// microtone (`[-1/2]`, a quarter-tone flat; ADR-0023). O(digits).
fn encode_alteration(alteration: Alteration) -> String {
    let semitones = alteration.semitones();
    match semitones
        .is_integer()
        .then(|| chromatic_sign(semitones.numerator()))
        .flatten()
    {
        Some(sign) => sign.to_owned(),
        None => format!("[{semitones}]"),
    }
}

/// Reads an alteration from the start of `text` and returns it with the rest of the text: a
/// chromatic sign (the longest that matches: `#x` before `#`, `bbb` before `bb` before `b`), or
/// a bracketed microtone, which must **not** be chromatic (a chromatic one has its sign). `None`
/// if the text has a bracket that is not canonical. O(text).
fn decode_alteration(text: &str) -> Option<(Alteration, &str)> {
    if let Some(inner) = text.strip_prefix('[') {
        let (value, rest) = inner.split_once(']')?;
        let semitones = decode_rational(value)?;
        if semitones.is_integer() && chromatic_sign(semitones.numerator()).is_some() {
            return None;
        }
        return Some((Alteration::new(semitones).ok()?, rest));
    }
    for (sign, semitones) in SIGNS {
        if let Some(rest) = text.strip_prefix(sign) {
            return Some((Alteration::chromatic(semitones).ok()?, rest));
        }
    }
    Some((Alteration::NATURAL, text))
}

/// The text of a pitch: letter, alteration, scientific octave (`"C4"`, `"F#5"`, `"Bb3"`,
/// `"C-1"`, `"E[-1/2]4"`). O(digits).
fn encode_pitch(pitch: Pitch) -> String {
    format!(
        "{}{}{}",
        step_letter(pitch.step()),
        encode_alteration(pitch.alteration()),
        pitch.octave()
    )
}

/// A pitch in canonical form: an upper-case letter, the alteration's canonical sign, an octave
/// without leading zeros, `+` or `-0`. O(text).
fn decode_pitch(text: &str) -> Option<Pitch> {
    let step = letter_step(*text.as_bytes().first()?)?;
    let (alteration, octave) = decode_alteration(text.get(1..)?)?;
    Some(Pitch::new(step, alteration, decode_small_int(octave)?))
}

/// A signed decimal in canonical form (no `+`, no leading zeros, no `-0`) that fits an `i8`.
/// O(digits).
fn decode_small_int(text: &str) -> Option<i8> {
    let digits = text.strip_prefix('-').unwrap_or(text);
    let canonical = !digits.is_empty()
        && digits.bytes().all(|b| b.is_ascii_digit())
        && !(digits.len() > 1 && digits.starts_with('0'))
        && text != "-0";
    canonical.then(|| text.parse().ok()).flatten()
}

text!(
    /// A spelled pitch: `"C4"`, `"F#5"`, `"E[-1/2]4"` (ADR-0029 S6, I4).
    PitchText(Pitch), "a pitch such as \"F#5\"", encode_pitch, decode_pitch
);

// ------------------------------------------------------------------------------------------------
// Note values
// ------------------------------------------------------------------------------------------------

/// The written name of each base value: its usual number (`"4"` a quarter) or, for the three
/// longest, a word, as `LilyPond` writes them (I4). In `BaseValue::ALL` order.
const BASE_NAMES: [(BaseValue, &str); 14] = [
    (BaseValue::Maxima, "maxima"),
    (BaseValue::Longa, "longa"),
    (BaseValue::Breve, "breve"),
    (BaseValue::Whole, "1"),
    (BaseValue::Half, "2"),
    (BaseValue::Quarter, "4"),
    (BaseValue::Eighth, "8"),
    (BaseValue::Sixteenth, "16"),
    (BaseValue::ThirtySecond, "32"),
    (BaseValue::SixtyFourth, "64"),
    (BaseValue::OneHundredTwentyEighth, "128"),
    (BaseValue::TwoHundredFiftySixth, "256"),
    (BaseValue::FiveHundredTwelfth, "512"),
    (BaseValue::OneThousandTwentyFourth, "1024"),
];

/// The text of a note value: its base name and one `.` per dot (`"8."`). O(dots).
fn encode_value(value: NoteValue) -> String {
    let base = BASE_NAMES
        .iter()
        .find(|(b, _)| *b == value.base())
        .map_or("?", |(_, name)| name);
    format!("{base}{}", ".".repeat(usize::from(value.dots())))
}

/// A note value: a base name followed by its dots (at most the model's limit). O(text).
fn decode_value(text: &str) -> Option<NoteValue> {
    let base_text = text.trim_end_matches('.');
    let dots = u8::try_from(text.len().saturating_sub(base_text.len())).ok()?;
    let base = BASE_NAMES
        .iter()
        .find(|(_, name)| *name == base_text)
        .map(|(b, _)| *b)?;
    NoteValue::new(base, dots).ok()
}

/// The text of a rest's value: a note value, or `"bar"` for a whole-bar rest. O(dots).
fn encode_rest(value: RestValue) -> String {
    match value {
        RestValue::Value(v) => encode_value(v),
        RestValue::WholeBar => "bar".to_owned(),
    }
}

/// A rest's value. O(text).
fn decode_rest(text: &str) -> Option<RestValue> {
    if text == "bar" {
        Some(RestValue::WholeBar)
    } else {
        decode_value(text).map(RestValue::Value)
    }
}

text!(
    /// A note value: `"4"`, `"8."`, `"breve"` (I4).
    ValueText(NoteValue), "a note value such as \"8.\"", encode_value, decode_value
);
text!(
    /// A rest's value: a note value or `"bar"` (a whole-bar rest).
    RestText(RestValue), "a rest value such as \"4\" or \"bar\"", encode_rest, decode_rest
);

// ------------------------------------------------------------------------------------------------
// Meters and beat groupings
// ------------------------------------------------------------------------------------------------

/// `groups` joined by `+`, then `/` and the denominator: `"4/4"`, `"3+2+2/8"`. O(groups).
fn encode_groups(groups: &[u8], denominator: u16) -> String {
    let numbers: Vec<String> = groups.iter().map(u8::to_string).collect();
    format!("{}/{denominator}", numbers.join("+"))
}

/// The groups and the denominator of `"3+2+2/8"`, each a canonical positive decimal. O(text).
fn decode_groups(text: &str) -> Option<(Vec<u8>, u16)> {
    /// A canonical positive decimal (no sign, no leading zero).
    fn positive<T: std::str::FromStr>(t: &str) -> Option<T> {
        let canonical =
            !t.is_empty() && t.bytes().all(|b| b.is_ascii_digit()) && !t.starts_with('0');
        canonical.then(|| t.parse().ok()).flatten()
    }
    let (groups, denominator) = text.split_once('/')?;
    let groups = groups
        .split('+')
        .map(positive)
        .collect::<Option<Vec<u8>>>()?;
    Some((groups, positive(denominator)?))
}

/// The text of a meter. O(groups).
fn encode_meter(meter: Meter) -> String {
    encode_groups(meter.groups(), meter.denominator())
}

/// A meter (the model checks the limits). O(text).
fn decode_meter(text: &str) -> Option<Meter> {
    let (groups, denominator) = decode_groups(text)?;
    Meter::new(&groups, denominator).ok()
}

/// The text of a beat grouping: its groups over its unit (`"3+2+2+2/8"`, ADR-0019). O(groups).
fn encode_grouping(grouping: BeatGrouping) -> String {
    encode_groups(grouping.groups(), grouping.unit())
}

/// A beat grouping. O(text).
fn decode_grouping(text: &str) -> Option<BeatGrouping> {
    let (groups, unit) = decode_groups(text)?;
    BeatGrouping::new(&groups, unit).ok()
}

text!(
    /// A written meter: `"4/4"`, `"3+2+2/8"`.
    MeterText(Meter), "a meter such as \"3+2+2/8\"", encode_meter, decode_meter
);
text!(
    /// A beat grouping: `"3+2+2+2/8"` (groups of eighths).
    GroupingText(BeatGrouping), "a beat grouping such as \"2+2+3/8\"", encode_grouping, decode_grouping
);

// ------------------------------------------------------------------------------------------------
// Clefs
// ------------------------------------------------------------------------------------------------

/// The text of a clef: its sign (`G`, `F`, `C`, `P` for percussion), its staff line, and an
/// octave shift if any (`"G2"` treble, `"F4"` bass, `"C3"` alto, `"G2-1"` the tenor voice's
/// treble an octave down). O(1).
fn encode_clef(clef: Clef) -> String {
    let sign = match clef.sign() {
        ClefSign::G => 'G',
        ClefSign::F => 'F',
        ClefSign::C => 'C',
        ClefSign::Percussion => 'P',
    };
    match clef.octave() {
        0 => format!("{sign}{}", clef.line()),
        o if o > 0 => format!("{sign}{}+{o}", clef.line()),
        o => format!("{sign}{}{o}", clef.line()),
    }
}

/// A clef: sign, one digit for the line, then nothing or a signed non-zero octave. O(text).
fn decode_clef(text: &str) -> Option<Clef> {
    let bytes = text.as_bytes();
    let sign = match bytes.first()? {
        b'G' => ClefSign::G,
        b'F' => ClefSign::F,
        b'C' => ClefSign::C,
        b'P' => ClefSign::Percussion,
        _ => return None,
    };
    let line = bytes
        .get(1)
        .filter(|b| b.is_ascii_digit())?
        .wrapping_sub(b'0');
    let rest = text.get(2..)?;
    let octave = if rest.is_empty() {
        0
    } else {
        // A shift is written with its sign, never zero: "+1", "-1".
        let digits = rest
            .strip_prefix('+')
            .or_else(|| rest.strip_prefix('-').map(|_| rest))?;
        let octave = decode_small_int(digits)?;
        if octave == 0 {
            return None;
        }
        octave
    };
    Clef::new(sign, line, octave).ok()
}

text!(
    /// A clef: `"G2"`, `"F4"`, `"C3"`, `"G2-1"`.
    ClefText(Clef), "a clef such as \"G2\"", encode_clef, decode_clef
);

// ------------------------------------------------------------------------------------------------
// Ornaments
// ------------------------------------------------------------------------------------------------

/// The text of an ornament: a name, with a parameter in brackets where it has one (I5): the
/// auxiliary note's alteration of a trill in semitones (`"trill[1]"`), the strokes of a tremolo
/// (`"tremolo[3]"`). O(digits).
fn encode_ornament(ornament: Ornament) -> String {
    match ornament {
        Ornament::Trill { auxiliary: None } => "trill".to_owned(),
        Ornament::Trill {
            auxiliary: Some(alteration),
        } => format!("trill[{}]", alteration.semitones()),
        Ornament::Turn => "turn".to_owned(),
        Ornament::InvertedTurn => "inverted_turn".to_owned(),
        Ornament::Mordent => "mordent".to_owned(),
        Ornament::UpperMordent => "upper_mordent".to_owned(),
        Ornament::Tremolo(strokes) => format!("tremolo[{}]", strokes.get()),
    }
}

/// An ornament. O(text).
fn decode_ornament(text: &str) -> Option<Ornament> {
    Some(match text {
        "trill" => Ornament::Trill { auxiliary: None },
        "turn" => Ornament::Turn,
        "inverted_turn" => Ornament::InvertedTurn,
        "mordent" => Ornament::Mordent,
        "upper_mordent" => Ornament::UpperMordent,
        _ => {
            let (name, parameter) = text.strip_suffix(']')?.split_once('[')?;
            match name {
                "trill" => Ornament::Trill {
                    auxiliary: Some(Alteration::new(decode_rational(parameter)?).ok()?),
                },
                "tremolo" => {
                    let strokes = decode_small_int(parameter)?;
                    Ornament::Tremolo(TremoloStrokes::new(u8::try_from(strokes).ok()?).ok()?)
                }
                _ => return None,
            }
        }
    })
}

text!(
    /// An ornament: `"turn"`, `"trill[1]"`, `"tremolo[3]"` (I5).
    OrnamentText(Ornament), "an ornament such as \"turn\" or \"tremolo[3]\"", encode_ornament, decode_ornament
);

// ------------------------------------------------------------------------------------------------
// The tolerance threshold
// ------------------------------------------------------------------------------------------------

/// The text of a tolerance threshold (ADR-0028): a rational in whole notes (`"1/32"`), or
/// milliseconds with their unit (`"25ms"`). O(digits).
fn encode_tolerance(tolerance: Tolerance) -> String {
    match tolerance {
        Tolerance::Musical(d) => d.as_rational().to_string(),
        Tolerance::Millis(ms) => format!("{ms}ms"),
    }
}

/// A tolerance threshold. O(text).
fn decode_tolerance(text: &str) -> Option<Tolerance> {
    match text.strip_suffix("ms") {
        Some(ms) => {
            let canonical = !ms.is_empty()
                && ms.bytes().all(|b| b.is_ascii_digit())
                && !(ms.len() > 1 && ms.starts_with('0'));
            canonical
                .then(|| ms.parse().ok().map(Tolerance::Millis))
                .flatten()
        }
        None => decode_duration(text).map(Tolerance::Musical),
    }
}

text!(
    /// A tolerance threshold: `"1/32"` or `"25ms"`.
    ToleranceText(Tolerance), "a tolerance such as \"1/32\" or \"25ms\"", encode_tolerance, decode_tolerance
);

// ------------------------------------------------------------------------------------------------
// Named enumerations
// ------------------------------------------------------------------------------------------------

named!(
    /// A key's mode.
    ModeName(Mode), "a mode such as \"major\"", {
        Mode::Major => "major", Mode::Dorian => "dorian", Mode::Phrygian => "phrygian",
        Mode::Lydian => "lydian", Mode::Mixolydian => "mixolydian", Mode::Minor => "minor",
        Mode::Locrian => "locrian", Mode::None => "none",
    }
);
named!(
    /// An articulation (written in the model's fixed order, ADR-0024 §6).
    ArticulationName(Articulation), "an articulation such as \"staccato\"", {
        Articulation::Accent => "accent", Articulation::Marcato => "marcato",
        Articulation::Staccato => "staccato", Articulation::Staccatissimo => "staccatissimo",
        Articulation::Spiccato => "spiccato", Articulation::Tenuto => "tenuto",
        Articulation::Portato => "portato", Articulation::Stress => "stress",
        Articulation::Unstress => "unstress", Articulation::SoftAccent => "soft_accent",
    }
);
named!(
    /// A fermata's length.
    FermataName(Fermata), "a fermata such as \"normal\"", {
        Fermata::Short => "short", Fermata::Normal => "normal", Fermata::Long => "long",
        Fermata::VeryLong => "very_long",
    }
);
named!(
    /// An arpeggio sign.
    ArpeggioName(Arpeggio), "an arpeggio such as \"up\"", {
        Arpeggio::Plain => "plain", Arpeggio::Up => "up", Arpeggio::Down => "down",
        Arpeggio::NonArpeggio => "non_arpeggio",
    }
);
named!(
    /// A grace note's kind.
    GraceKindName(GraceKind), "\"acciaccatura\" or \"appoggiatura\"", {
        GraceKind::Acciaccatura => "acciaccatura", GraceKind::Appoggiatura => "appoggiatura",
    }
);
named!(
    /// Which main event a grace note belongs to.
    GraceAttachName(GraceAttach), "\"before\" or \"after\"", {
        GraceAttach::Before => "before", GraceAttach::After => "after",
    }
);
named!(
    /// A tuplet's bracket.
    BracketName(TupletBracket), "\"auto\", \"shown\" or \"hidden\"", {
        TupletBracket::Auto => "auto", TupletBracket::Shown => "shown",
        TupletBracket::Hidden => "hidden",
    }
);
named!(
    /// A tuplet's number.
    TupletNumberName(TupletNumber), "\"actual\", \"ratio\" or \"hidden\"", {
        TupletNumber::Actual => "actual", TupletNumber::Ratio => "ratio",
        TupletNumber::Hidden => "hidden",
    }
);
named!(
    /// How an irregular bar is aligned (ADR-0018).
    BarKindName(BarKind), "\"align_start\", \"align_end\" or \"free\"", {
        BarKind::AlignStart => "align_start", BarKind::AlignEnd => "align_end",
        BarKind::Free => "free",
    }
);
named!(
    /// Where a slur is drawn.
    SlurPlacementName(SlurPlacement), "\"auto\", \"above\" or \"below\"", {
        SlurPlacement::Auto => "auto", SlurPlacement::Above => "above",
        SlurPlacement::Below => "below",
    }
);
named!(
    /// A dynamic level.
    LevelName(DynamicLevel), "a dynamic level such as \"mf\"", {
        DynamicLevel::Pppppp => "pppppp", DynamicLevel::Ppppp => "ppppp",
        DynamicLevel::Pppp => "pppp", DynamicLevel::Ppp => "ppp", DynamicLevel::Pp => "pp",
        DynamicLevel::P => "p", DynamicLevel::Mp => "mp", DynamicLevel::Mf => "mf",
        DynamicLevel::F => "f", DynamicLevel::Ff => "ff", DynamicLevel::Fff => "fff",
        DynamicLevel::Ffff => "ffff", DynamicLevel::Fffff => "fffff",
        DynamicLevel::Ffffff => "ffffff",
    }
);
named!(
    /// A hairpin's direction.
    HairpinKindName(HairpinKind), "\"crescendo\" or \"diminuendo\"", {
        HairpinKind::Crescendo => "crescendo", HairpinKind::Diminuendo => "diminuendo",
    }
);
named!(
    /// A hairpin's look.
    HairpinStyleName(HairpinStyle), "\"wedge\" or \"text\"", {
        HairpinStyle::Wedge => "wedge", HairpinStyle::Text => "text",
    }
);
named!(
    /// The kind of a form marker.
    MarkerKindName(MarkerKind), "\"segno\", \"coda\", \"fine\" or \"to_coda\"", {
        MarkerKind::Segno => "segno", MarkerKind::Coda => "coda", MarkerKind::Fine => "fine",
        MarkerKind::ToCoda => "to_coda",
    }
);
named!(
    /// Whether the repeats are played again after a jump (ADR-0025 §4).
    RepeatsName(RepeatsAfterJump), "\"skip\" or \"play\"", {
        RepeatsAfterJump::Skip => "skip", RepeatsAfterJump::Play => "play",
    }
);
named!(
    /// The bar-numbering style (ADR-0025, F9).
    NumberingName(BarNumbering), "a numbering style such as \"continue\"", {
        BarNumbering::Continue => "continue",
        BarNumbering::SameAsFirstLettered => "same_as_first_lettered",
        BarNumbering::SameAsFirst => "same_as_first",
    }
);
named!(
    /// How many bars a bar-repeat sign repeats.
    SpanName(BarRepeatSpan), "\"1\", \"2\" or \"4\"", {
        BarRepeatSpan::One => "1", BarRepeatSpan::Two => "2", BarRepeatSpan::Four => "4",
    }
);

#[cfg(test)]
mod tests {
    //! Round trips and refusals of the text forms; the file-level tests are in `tests/`.
    use super::*;

    /// Every pitch spelling round-trips, and the non-canonical ones are refused.
    #[test]
    fn pitches() {
        for text in [
            "C4", "F#5", "Bb3", "Cx4", "Dbb2", "G#x3", "Abbb3", "C-1", "E[-1/2]4", "B[1/4]-1",
        ] {
            assert_eq!(
                decode_pitch(text).map(encode_pitch).as_deref(),
                Some(text),
                "{text}"
            );
        }
        for text in [
            "c4", "C##4", "C[1]4", "C[0]4", "C+4", "C04", "C-0", "C", "H4", "C[1/2", "C 4",
        ] {
            assert_eq!(decode_pitch(text), None, "{text}");
        }
    }

    /// Note values, rests, meters, clefs, ornaments and tolerances round-trip.
    #[test]
    fn other_texts() {
        for text in ["1", "4", "8.", "16..", "breve", "longa.", "maxima", "1024"] {
            assert_eq!(decode_value(text).map(encode_value).as_deref(), Some(text));
        }
        for text in ["3", "04", "4 ", "quarter", ".4", "4.:"] {
            assert_eq!(decode_value(text), None, "{text}");
        }
        assert_eq!(decode_rest("bar"), Some(RestValue::WholeBar));
        for text in ["4/4", "3+2+2/8", "6/8"] {
            assert_eq!(decode_meter(text).map(encode_meter).as_deref(), Some(text));
        }
        for text in ["4/04", "0/4", "4/", "+3/8", "3++2/8"] {
            assert_eq!(decode_meter(text), None, "{text}");
        }
        for text in ["G2", "F4", "C3", "C4", "G2-1", "G2+1", "P3"] {
            assert_eq!(
                decode_clef(text).map(encode_clef).as_deref(),
                Some(text),
                "{text}"
            );
        }
        for text in ["G2+0", "G2-0", "G2+01", "g2", "G"] {
            assert_eq!(decode_clef(text), None, "{text}");
        }
        for text in ["trill", "trill[1]", "trill[-1/2]", "turn", "tremolo[3]"] {
            assert_eq!(
                decode_ornament(text).map(encode_ornament).as_deref(),
                Some(text)
            );
        }
        for text in ["tremolo[0]", "tremolo[9]", "trill[]", "spin"] {
            assert_eq!(decode_ornament(text), None, "{text}");
        }
        for text in ["1/32", "25ms", "0ms"] {
            assert_eq!(
                decode_tolerance(text).map(encode_tolerance).as_deref(),
                Some(text)
            );
        }
        assert_eq!(decode_tolerance("025ms"), None);
    }
}
