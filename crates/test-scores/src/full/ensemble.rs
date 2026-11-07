// SPDX-License-Identifier: GPL-3.0-or-later

//! The **ensembles** of the full preset: what each instrument is called, how it is written
//! (clefs, transposition, key signature) and what it plays (one role and one register per voice).
//!
//! # Why real instruments?
//! The layout of Phase 2 will draw these scores: alto and tenor clefs, transposing parts and a
//! horn without key signature are exactly what an engraver must get right, so a plausible score
//! has them (owner's choice, 2026-11-07). The names are test data stored in the document, not
//! interface texts, so they do not go through the Fluent catalogs (CLAUDE.md §7 is about texts
//! the program shows the user).
//!
//! # Registers
//! A register is a range of **diatonic numbers**, `octave × 7 + step` with C = 0 … B = 6, in
//! *written* pitch: [`dia`] builds one from a step and an octave. The generator moves inside it
//! by scale steps, which is how parts are written.
//!
//! # Why built at run time
//! The tables are small (9 or 28 instruments) and built once per score; plain `Vec`s keep them
//! readable, where `const` tables would need a static slice per instrument for its voices.

use orchestra_core_model::{
    Clef, DocumentBuilder, InstrumentId, Interval, StaffId, TrackId, VoiceId,
};
use orchestra_core_time::Rational;

/// What a voice plays: picks its rhythm cells and how it chooses pitches
/// ([`super::music`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Role {
    /// The tune: varied rhythms, scale-wise motion, ornaments, slurs, cadences with a trill.
    Melody,
    /// Inner parts: repeated eighths, after-beats, chord tones; double stops on strings.
    Inner,
    /// Held notes: halves, dotted halves, wholes.
    Sustain,
    /// The bass line: the chord's root.
    Bass,
    /// The piano's right hand: chords of 1–4 notes.
    PianoRight,
    /// The piano's left hand: root and fifth or octave.
    PianoLeft,
    /// The piano's second voice on the upper staff: held chord tones, in sections A and C only.
    PianoInner,
    /// The piano's second voice on the lower staff: a pedal tonic, in section D only.
    PianoPedal,
    /// The harp's right hand: arpeggiated chords.
    HarpRight,
    /// Tonic and dominant.
    Timpani,
}

/// How often an instrument rests for a whole phrase: winds and brass breathe, strings rarely do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Family {
    /// Woodwinds: tacet in about 30% of the phrases.
    Wind,
    /// Brass: about 40%.
    Brass,
    /// Timpani and the piccolo, used for colour: about 60%.
    Colour,
    /// Piano and harp: never tacet.
    Keyboard,
    /// Strings: about 10%.
    Strings,
}

impl Family {
    /// The probability of a tacet phrase, in percent. O(1).
    pub(super) const fn tacet_percent(self) -> u64 {
        match self {
            Self::Wind => 30,
            Self::Brass => 40,
            Self::Colour => 60,
            Self::Keyboard => 0,
            Self::Strings => 10,
        }
    }
}

/// The diatonic number of `step` (0 = C … 6 = B) in `octave` (scientific pitch notation: C4 is
/// middle C): `octave × 7 + step`. O(1).
pub(super) const fn dia(step: i64, octave: i64) -> i64 {
    octave * 7 + step
}

/// The written register of a voice, as diatonic numbers (inclusive bounds, low first).
pub(super) type Register = (i64, i64);

/// One voice of an instrument as the specification describes it.
#[derive(Debug, Clone, Copy)]
pub(super) struct VoiceSpec {
    /// The staff it lives on (index into the instrument's clefs).
    pub(super) staff: usize,
    /// What it plays.
    pub(super) role: Role,
    /// Its written register.
    pub(super) register: Register,
}

/// One instrument as the specification describes it.
#[derive(Debug, Clone)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent yes/no properties of an instrument in the specification, not a state machine"
)]
pub(super) struct InstrumentSpec {
    /// Its name in the score.
    pub(super) name: &'static str,
    /// Written to sounding, as (diatonic steps, semitones): `(-1, -2)` for a B♭ instrument,
    /// `(-4, -7)` for one in F, `(-7, -12)` for the double bass, `(0, 0)` at concert pitch.
    pub(super) transposition: (i32, i64),
    /// How many fifths its written key lies above the concert key: +2 for B♭ instruments, +1 for
    /// those in F, 0 at concert pitch and for octave transpositions.
    pub(super) written_shift: i8,
    /// One clef per staff, top to bottom.
    pub(super) clefs: Vec<Clef>,
    /// Its voices: the first voice of each staff is the builder's, the others are added.
    pub(super) voices: Vec<VoiceSpec>,
    /// How often it rests for a phrase.
    pub(super) family: Family,
    /// Written without key signature, with accidentals (horns and trumpets in the classical way,
    /// timpani): a key override to "no signature" at the first bar.
    pub(super) no_signature: bool,
    /// The first flute: the cadenza, the "dolce" and the final diminuendo to niente.
    pub(super) soloist: bool,
    /// Plays the repeat of section A only the second time.
    pub(super) second_time_only: bool,
    /// Uses bar-repeat signs in section B (its staff holds no events there).
    pub(super) bar_repeats: bool,
    /// Moves to the tenor clef (and an octave up) in section D.
    pub(super) tenor_passage: bool,
}

impl InstrumentSpec {
    /// A one-staff instrument at concert pitch with one voice, every flag off.
    fn single(
        name: &'static str,
        clef: Clef,
        role: Role,
        register: Register,
        family: Family,
    ) -> Self {
        Self {
            name,
            transposition: (0, 0),
            written_shift: 0,
            clefs: vec![clef],
            voices: vec![VoiceSpec {
                staff: 0,
                role,
                register,
            }],
            family,
            no_signature: false,
            soloist: false,
            second_time_only: false,
            bar_repeats: false,
            tenor_passage: false,
        }
    }

    /// The same instrument, transposing by `transposition` with its written key `shift` fifths
    /// above the concert key.
    fn transposing(mut self, transposition: (i32, i64), shift: i8) -> Self {
        self.transposition = transposition;
        self.written_shift = shift;
        self
    }

    /// The piano: two staves, a right hand with an inner second voice and a left hand with a pedal
    /// second voice (up to two voices per staff, as the specification says).
    fn piano(name: &'static str) -> Self {
        let mut spec = Self::single(
            name,
            Clef::TREBLE,
            Role::PianoRight,
            (dia(0, 4), dia(0, 6)),
            Family::Keyboard,
        );
        spec.clefs.push(Clef::BASS);
        spec.voices = vec![
            VoiceSpec {
                staff: 0,
                role: Role::PianoRight,
                register: (dia(0, 4), dia(0, 6)),
            },
            VoiceSpec {
                staff: 1,
                role: Role::PianoLeft,
                register: (dia(0, 2), dia(0, 4)),
            },
            VoiceSpec {
                staff: 0,
                role: Role::PianoInner,
                register: (dia(2, 4), dia(0, 5)),
            },
            VoiceSpec {
                staff: 1,
                role: Role::PianoPedal,
                register: (dia(0, 2), dia(4, 2)),
            },
        ];
        spec
    }
}

/// The ten staves of the normal score: piano, flute, oboe, clarinet in B♭, horn in F, violins I
/// and II, viola, violoncello.
pub(super) fn chamber() -> Vec<InstrumentSpec> {
    use Family::{Brass, Strings, Wind};
    use Role::{Bass, Inner, Melody, Sustain};
    let mut flute =
        InstrumentSpec::single("Flute", Clef::TREBLE, Melody, (dia(1, 5), dia(4, 6)), Wind);
    flute.soloist = true;
    let mut horn = InstrumentSpec::single(
        "Horn in F",
        Clef::TREBLE,
        Sustain,
        (dia(0, 4), dia(4, 5)),
        Brass,
    )
    .transposing((-4, -7), 1);
    horn.no_signature = true;
    horn.second_time_only = true;
    let mut violin_2 = InstrumentSpec::single(
        "Violin II",
        Clef::TREBLE,
        Inner,
        (dia(4, 4), dia(1, 6)),
        Strings,
    );
    violin_2.bar_repeats = true;
    let mut viola =
        InstrumentSpec::single("Viola", Clef::ALTO, Inner, (dia(1, 3), dia(5, 4)), Strings);
    viola.bar_repeats = true;
    let mut cello = InstrumentSpec::single(
        "Violoncello",
        Clef::BASS,
        Bass,
        (dia(0, 2), dia(2, 3)),
        Strings,
    );
    cello.tenor_passage = true;
    vec![
        InstrumentSpec::piano("Piano"),
        flute,
        InstrumentSpec::single("Oboe", Clef::TREBLE, Melody, (dia(2, 4), dia(0, 6)), Wind),
        InstrumentSpec::single(
            "Clarinet in B♭",
            Clef::TREBLE,
            Inner,
            (dia(2, 4), dia(0, 6)),
            Wind,
        )
        .transposing((-1, -2), 2),
        horn,
        InstrumentSpec::single(
            "Violin I",
            Clef::TREBLE,
            Melody,
            (dia(1, 5), dia(5, 6)),
            Strings,
        ),
        violin_2,
        viola,
        cello,
    ]
}

/// The thirty staves of the heavy score: a small orchestra (piccolo, 2 flutes, 2 oboes, English
/// horn, 2 clarinets, bass clarinet, 2 bassoons, 4 horns, 2 trumpets, 3 trombones, timpani, piano,
/// harp, strings with double bass).
#[allow(
    clippy::too_many_lines,
    reason = "a table of 28 instruments, one line each: splitting it would only scatter it"
)]
pub(super) fn orchestra() -> Vec<InstrumentSpec> {
    use Family::{Brass, Colour, Keyboard, Strings, Wind};
    use Role::{Bass, HarpRight, Inner, Melody, Sustain, Timpani};
    let treble = Clef::TREBLE;
    let one = InstrumentSpec::single;
    let mut flute_1 = one("Flute 1", treble, Melody, (dia(1, 5), dia(4, 6)), Wind);
    flute_1.soloist = true;
    let mut horn_1 = one(
        "Horn in F 1",
        treble,
        Sustain,
        (dia(0, 4), dia(4, 5)),
        Brass,
    )
    .transposing((-4, -7), 1);
    horn_1.second_time_only = true;
    let mut horns = vec![horn_1];
    for name in ["Horn in F 2", "Horn in F 3", "Horn in F 4"] {
        horns.push(
            one(name, treble, Sustain, (dia(4, 3), dia(0, 5)), Brass).transposing((-4, -7), 1),
        );
    }
    for horn in &mut horns {
        horn.no_signature = true;
    }
    let mut trombone_1 = one(
        "Trombone 1",
        Clef::BASS,
        Sustain,
        (dia(2, 2), dia(3, 3)),
        Brass,
    );
    trombone_1.tenor_passage = true;
    let mut timpani = one(
        "Timpani",
        Clef::BASS,
        Timpani,
        (dia(3, 2), dia(3, 3)),
        Colour,
    );
    timpani.no_signature = true;
    let mut harp = one("Harp", treble, HarpRight, (dia(0, 4), dia(0, 6)), Keyboard);
    harp.clefs.push(Clef::BASS);
    harp.voices.push(VoiceSpec {
        staff: 1,
        role: Bass,
        register: (dia(0, 2), dia(0, 4)),
    });
    let mut violin_2 = one("Violin II", treble, Inner, (dia(4, 4), dia(1, 6)), Strings);
    violin_2.bar_repeats = true;
    let mut viola = one("Viola", Clef::ALTO, Inner, (dia(1, 3), dia(5, 4)), Strings);
    viola.bar_repeats = true;
    let mut cello = one(
        "Violoncello",
        Clef::BASS,
        Bass,
        (dia(0, 2), dia(2, 3)),
        Strings,
    );
    cello.tenor_passage = true;
    let mut list = vec![
        one("Piccolo", treble, Melody, (dia(1, 5), dia(4, 6)), Colour).transposing((7, 12), 0),
        flute_1,
        one("Flute 2", treble, Inner, (dia(4, 4), dia(2, 6)), Wind),
        one("Oboe 1", treble, Melody, (dia(2, 4), dia(0, 6)), Wind),
        one("Oboe 2", treble, Inner, (dia(0, 4), dia(5, 5)), Wind),
        one(
            "English Horn",
            treble,
            Sustain,
            (dia(2, 4), dia(0, 6)),
            Wind,
        )
        .transposing((-4, -7), 1),
        one(
            "Clarinet in B♭ 1",
            treble,
            Inner,
            (dia(2, 4), dia(0, 6)),
            Wind,
        )
        .transposing((-1, -2), 2),
        one(
            "Clarinet in B♭ 2",
            treble,
            Inner,
            (dia(4, 3), dia(4, 5)),
            Wind,
        )
        .transposing((-1, -2), 2),
        one(
            "Bass Clarinet in B♭",
            treble,
            Bass,
            (dia(2, 3), dia(4, 4)),
            Wind,
        )
        .transposing((-8, -14), 2),
        one("Bassoon 1", Clef::BASS, Bass, (dia(0, 2), dia(0, 4)), Wind),
        one("Bassoon 2", Clef::BASS, Bass, (dia(4, 1), dia(4, 3)), Wind),
    ];
    list.extend(horns);
    list.extend([
        one(
            "Trumpet in B♭ 1",
            treble,
            Sustain,
            (dia(0, 4), dia(4, 5)),
            Brass,
        )
        .transposing((-1, -2), 2),
        one(
            "Trumpet in B♭ 2",
            treble,
            Sustain,
            (dia(4, 3), dia(0, 5)),
            Brass,
        )
        .transposing((-1, -2), 2),
        trombone_1,
        one(
            "Trombone 2",
            Clef::BASS,
            Sustain,
            (dia(0, 2), dia(0, 4)),
            Brass,
        ),
        one(
            "Trombone 3",
            Clef::BASS,
            Bass,
            (dia(4, 1), dia(4, 3)),
            Brass,
        ),
        timpani,
        InstrumentSpec::piano("Piano"),
        harp,
        one("Violin I", treble, Melody, (dia(1, 5), dia(5, 6)), Strings),
        violin_2,
        viola,
        cello,
        one(
            "Contrabass",
            Clef::BASS,
            Bass,
            (dia(2, 2), dia(0, 4)),
            Strings,
        )
        .transposing((-7, -12), 0),
    ]);
    list
}

/// An instrument once added to the builder: its IDs and, per voice, the voice ID and its spec.
#[derive(Debug, Clone)]
pub(super) struct Built {
    /// The track holding the instrument.
    pub(super) track: TrackId,
    /// The instrument.
    pub(super) instrument: InstrumentId,
    /// Its staves, top to bottom.
    pub(super) staves: Vec<StaffId>,
    /// Its voices, in the order of the spec's `voices`, with that spec.
    pub(super) voices: Vec<(VoiceId, VoiceSpec)>,
}

/// Adds `spec` to the builder: the track (one voice per staff, from the builder), then the extra
/// voices of the spec (those after the first of their staff). O(staves + voices × bars).
pub(super) fn add(b: &mut DocumentBuilder, spec: &InstrumentSpec) -> Built {
    let (steps, semitones) = spec.transposition;
    let track = b
        .add_instrument_track(
            spec.name,
            Interval::new(steps, Rational::from_integer(semitones).unwrap()),
            &spec.clefs,
        )
        .unwrap();
    let (instrument, staves, first_voices) = {
        let i = b.document().track(track).unwrap().instrument();
        let staves: Vec<StaffId> = i
            .staves()
            .iter()
            .map(orchestra_core_model::Staff::id)
            .collect();
        // The builder gives each staff one voice, in staff order.
        let first: Vec<VoiceId> = i
            .voices()
            .iter()
            .map(orchestra_core_model::Voice::id)
            .collect();
        (i.id(), staves, first)
    };
    // Walk the spec's voices: the first one met on each staff takes that staff's builder voice,
    // every later one is a new voice on the staff.
    let mut used = vec![false; staves.len()];
    let mut voices = Vec::with_capacity(spec.voices.len());
    for v in &spec.voices {
        let id = if used[v.staff] {
            b.add_voice(staves[v.staff]).unwrap()
        } else {
            used[v.staff] = true;
            first_voices[v.staff]
        };
        voices.push((id, *v));
    }
    Built {
        track,
        instrument,
        staves,
        voices,
    }
}
