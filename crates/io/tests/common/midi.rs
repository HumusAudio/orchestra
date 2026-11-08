// SPDX-License-Identifier: GPL-3.0-or-later

//! What the MIDI export's tests share (ADR-0032, "Tests and benchmarks"):
//! - [`decode`]: every file is read back with **`midly`**, an independent reader (E2), so that an
//!   error of our encoder is not hidden by the same error in our own reader;
//! - [`Decoded::units_at`]: the clock a player follows, rebuilt from the file's tempo events
//!   alone, in units of 1/960 µs (one tick at one µs per quarter note);
//! - [`Score`]: small documents built with the real builder.

#![allow(
    dead_code,
    reason = "each test file uses a different part of the shared helpers"
)]

use midly::{MetaMessage, MidiMessage, Smf, Timing, TrackEventKind};
use orchestra_core_model::{
    Alteration, BarId, BaseValue, Clef, Document, DocumentBuilder, EventId, InstrumentId, Interval,
    Key, Metronome, Mode, NoteValue, Pitch, Placement, StaffId, Step, TempoMark, VoiceId,
};
use orchestra_core_time::{Duration, Meter, MeterChange, MeterMap, Rational, Tempo};
use orchestra_io::midi::{MidiExport, MidiOptions, export};
use orchestra_playback::{Performance, Unrolled};

/// One channel event of a decoded track.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Ev {
    /// Note-on with a velocity above 0.
    On { ch: u8, key: u8, vel: u8 },
    /// Note-off (or a note-on with velocity 0, which our encoder never writes).
    Off { ch: u8, key: u8 },
    /// Control change.
    Cc { ch: u8, cc: u8, val: u8 },
    /// Pitch bend, 0 ..= 16 383 (8 192 = centre).
    Bend { ch: u8, value: u16 },
    /// Program change.
    Program { ch: u8, program: u8 },
}

/// One decoded track.
#[derive(Debug, Clone, Default)]
pub(crate) struct Track {
    /// Its name, if it has one.
    pub(crate) name: Option<String>,
    /// Its channel events with their absolute ticks, in file order.
    pub(crate) events: Vec<(u64, Ev)>,
}

/// A decoded file.
#[derive(Debug, Clone, Default)]
pub(crate) struct Decoded {
    /// Ticks per quarter note of the header.
    pub(crate) division: u16,
    /// Tempo events (tick, µs per quarter), from every track.
    pub(crate) tempos: Vec<(u64, u32)>,
    /// Time signatures (tick, numerator, denominator's exponent).
    pub(crate) signatures: Vec<(u64, u8, u8)>,
    /// Markers (tick, text).
    pub(crate) markers: Vec<(u64, String)>,
    /// The tracks after the first (the conductor).
    pub(crate) tracks: Vec<Track>,
}

/// Reads `bytes` with `midly`; panics (fails the test) if it cannot.
pub(crate) fn decode(bytes: &[u8]) -> Decoded {
    let smf = Smf::parse(bytes).unwrap();
    let Timing::Metrical(division) = smf.header.timing else {
        panic!("timecode division");
    };
    let mut out = Decoded {
        division: division.as_int(),
        ..Decoded::default()
    };
    for (index, events) in smf.tracks.iter().enumerate() {
        let mut tick = 0_u64;
        let mut track = Track::default();
        for event in events {
            tick += u64::from(event.delta.as_int());
            match event.kind {
                TrackEventKind::Meta(MetaMessage::Tempo(t)) => out.tempos.push((tick, t.as_int())),
                TrackEventKind::Meta(MetaMessage::TimeSignature(n, d, _, _)) => {
                    out.signatures.push((tick, n, d));
                }
                TrackEventKind::Meta(MetaMessage::Marker(text)) => {
                    out.markers
                        .push((tick, String::from_utf8_lossy(text).into_owned()));
                }
                TrackEventKind::Meta(MetaMessage::TrackName(text)) => {
                    track.name = Some(String::from_utf8_lossy(text).into_owned());
                }
                TrackEventKind::Midi { channel, message } => {
                    let ch = channel.as_int();
                    let ev = match message {
                        MidiMessage::NoteOn { key, vel } if vel.as_int() > 0 => Ev::On {
                            ch,
                            key: key.as_int(),
                            vel: vel.as_int(),
                        },
                        MidiMessage::NoteOn { key, .. } | MidiMessage::NoteOff { key, .. } => {
                            Ev::Off {
                                ch,
                                key: key.as_int(),
                            }
                        }
                        MidiMessage::Controller { controller, value } => Ev::Cc {
                            ch,
                            cc: controller.as_int(),
                            val: value.as_int(),
                        },
                        MidiMessage::PitchBend { bend } => Ev::Bend {
                            ch,
                            value: bend.0.as_int(),
                        },
                        MidiMessage::ProgramChange { program } => Ev::Program {
                            ch,
                            program: program.as_int(),
                        },
                        _ => continue,
                    };
                    track.events.push((tick, ev));
                }
                _ => {}
            }
        }
        if index > 0 {
            out.tracks.push(track);
        }
    }
    out.tempos.sort_by_key(|(t, _)| *t);
    out
}

impl Decoded {
    /// The time a player reaches at `tick`, in units of 1/960 µs: Σ tempo × ticks (a tick at `T`
    /// µs per quarter lasts `T / 960` µs = `T` units). Before the first tempo event, 500 000 (the
    /// format's default).
    pub(crate) fn units_at(&self, tick: u64) -> i128 {
        let mut units = 0_i128;
        let mut at = 0_u64;
        let mut tempo = 500_000_i128;
        for (t, micros) in &self.tempos {
            if *t >= tick {
                break;
            }
            units += tempo * i128::from(*t - at);
            at = *t;
            tempo = i128::from(*micros);
        }
        units + tempo * i128::from(tick - at)
    }

    /// The notes of a track: (start, end, channel, key, velocity), pairing each note-off with
    /// the earliest note-on of its key still open on its channel.
    pub(crate) fn notes(&self, track: usize) -> Vec<(u64, u64, u8, u8, u8)> {
        let mut open: Vec<(u64, u8, u8, u8)> = Vec::new();
        let mut out = Vec::new();
        for (tick, ev) in &self.tracks[track].events {
            match *ev {
                Ev::On { ch, key, vel } => open.push((*tick, ch, key, vel)),
                Ev::Off { ch, key } => {
                    if let Some(i) = open.iter().position(|o| o.1 == ch && o.2 == key) {
                        let (start, ch, key, vel) = open.remove(i);
                        out.push((start, *tick, ch, key, vel));
                    }
                }
                _ => {}
            }
        }
        assert!(open.is_empty(), "every note-on has its note-off: {open:?}");
        out.sort_unstable();
        out
    }
}

/// `n/den` of a whole note.
pub(crate) fn d(n: i64, den: i64) -> Duration {
    Duration::from_rational(Rational::new(n, den).unwrap())
}

/// A pitch of the fourth octave.
pub(crate) fn p4(step: Step) -> Pitch {
    Pitch::new(step, Alteration::NATURAL, 4)
}

/// A small score: the builder, its bars, one instrument per name with one voice and one staff.
pub(crate) struct Score {
    /// The builder.
    pub(crate) b: DocumentBuilder,
    /// The bars.
    pub(crate) bars: Vec<BarId>,
    /// The instruments: (instrument, first voice, staff).
    pub(crate) parts: Vec<(InstrumentId, VoiceId, StaffId)>,
}

impl Score {
    /// `n` bars of `meters` at ♩ = `bpm`, one one-staff instrument per name.
    pub(crate) fn new(n: usize, meters: MeterMap, bpm: i64, names: &[&str]) -> Self {
        let tempo = Tempo::new(d(1, 4), Rational::from_integer(bpm).unwrap()).unwrap();
        Self::with_tempo(n, meters, tempo, names)
    }

    /// `n` bars of `meters` at `tempo`, one one-staff instrument per name.
    pub(crate) fn with_tempo(n: usize, meters: MeterMap, tempo: Tempo, names: &[&str]) -> Self {
        let mut b = DocumentBuilder::new(
            meters,
            TempoMark::immediate(None, Metronome::shown(tempo)).unwrap(),
            Key::traditional(0, Mode::Major).unwrap(),
        )
        .unwrap();
        let bars = (0..n).map(|_| b.add_bar().unwrap()).collect();
        let mut parts = Vec::new();
        for name in names {
            let track = b
                .add_instrument_track(*name, Interval::new(0, Rational::ZERO), &[Clef::TREBLE])
                .unwrap();
            let i = b.document().track(track).unwrap().instrument();
            parts.push((i.id(), i.voices()[0].id(), i.staves()[0].id()));
        }
        Self { b, bars, parts }
    }

    /// `n` bars of 4/4 at ♩ = 120 with one instrument, "Violin".
    pub(crate) fn simple(n: usize) -> Self {
        Self::new(n, common_time(), 120, &["Violin"])
    }

    /// A chord of `value` in a voice at (bar, offset); returns its event.
    pub(crate) fn chord(
        &mut self,
        voice: VoiceId,
        bar: usize,
        offset: Duration,
        value: BaseValue,
        pitches: &[Pitch],
    ) -> EventId {
        self.b
            .add_chord(
                Placement::new(voice, self.bars[bar], offset),
                NoteValue::plain(value),
                pitches,
            )
            .unwrap()
            .event
    }

    /// The first voice of the first instrument.
    pub(crate) fn voice(&self) -> VoiceId {
        self.parts[0].1
    }

    /// Builds the document and exports it with `options`.
    pub(crate) fn export_with(self, options: &MidiOptions) -> (Document, MidiExport) {
        let doc = self.b.build().unwrap();
        let out = export_doc(&doc, options);
        (doc, out)
    }

    /// Builds the document and exports it with the default options.
    pub(crate) fn export(self) -> (Document, MidiExport) {
        self.export_with(&MidiOptions::new())
    }
}

/// A 4/4 meter map.
pub(crate) fn common_time() -> MeterMap {
    MeterMap::new(MeterChange::new(Meter::new(&[4], 4).unwrap())).unwrap()
}

/// Exports a document (path and performance computed here).
pub(crate) fn export_doc(doc: &Document, options: &MidiOptions) -> MidiExport {
    let path = Unrolled::new(doc).unwrap();
    let performance = Performance::new(doc, &path).unwrap();
    export(doc, &path, &performance, options).unwrap()
}
