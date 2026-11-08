// SPDX-License-Identifier: GPL-3.0-or-later

//! The instruments' tracks: channels (ADR-0032 E6), programs (E7), velocity and the CC 11 curve
//! of the attack profile (E8), microtones as pitch bend (E9), unisons (E10).
//!
//! # The steps
//! 1. **Channels.** Instrument *i* (score order) plays on `CHANNELS[i mod 15]`, the 16 channels
//!    without channel 10 (General MIDI drums). Beyond 15 instruments channels are shared, with a
//!    warning. Each channel is set up once, at tick 0, in the track of its first instrument: pitch
//!    bend range ±2 semitones (RPN 0), CC 11 = 127, bend at the centre.
//! 2. **Notes.** Each played note gets its key and bend from its exact pitch with the model's own
//!    rule, `MidiNote::nearest` (key = the nearest semitone, the lower one exactly half way; bend =
//!    the remainder, −½ <.. ½ semitone, ADR-0033), the bend written as `8 192 + round(b × 4 096)`;
//!    its ticks (E3; at least one tick long) and its note-on velocity `max(v, s) >> 9`, at least 1
//!    (a 7-bit 0 would be a note-off). A key outside 0 ..= 127 is skipped with a warning.
//! 3. **Who owns the channel's CC 11 (E8).** The latest attack: the notes of a channel are sorted
//!    by start, and each owns the controller from its start until the next note-on of the channel
//!    or its own end, whichever comes first. In that window its curve is written: `127 × level(t)
//!    / max(v, s)`, `level` being `v` for `hold`, then linear to `s` in `settle_in`, then `s`; in
//!    steps of 10 ms of the reader's clock (the tempo track's inverse), only where the 7-bit
//!    value changes, with a point exactly at the end of the ramp.
//! 4. **The channel's state, in time order (E9, E10).** All the channel's actions sorted by tick,
//!    then note-offs, controllers (CC 11, bend), note-ons, then key: a note-on finding nothing
//!    sounding sets the bend it needs; finding other notes sounding with a different bend, it is
//!    played on the nearest key under the current bend, the lower one on a tie (`BendRounded`,
//!    ADR-0033); finding its own key
//!    sounding (a unison), it writes a note-off and a new note-on, and only the **last** note on
//!    that key writes the final note-off.
//! 5. **Tracks.** Each event goes to the track of its instrument; each track is sorted by tick,
//!    then by the group of E10 (note-offs, controllers, note-ons), channel and key, so the bytes
//!    are deterministic.

use orchestra_core_model::{Document, MidiNote};
use orchestra_core_time::Seconds;
use orchestra_playback::{Performance, PlayedNote};

use super::smf::{self, Event, Message};
use super::timing::{TempoTrack, UNITS_PER_SECOND, round_div, ticks_of};
use super::{MidiError, MidiOptions, MidiWarning};

/// The channels instruments are given, in order: all 16 but channel 10 (index 9), the General MIDI
/// drums.
const CHANNELS: [u8; 15] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 10, 11, 12, 13, 14, 15];

/// The centre of the 14-bit pitch bend: no bend.
const BEND_CENTRE: u16 = 8_192;

/// Bend units per semitone with a range of ±2 semitones: 8 192 / 2.
const BEND_PER_SEMITONE: i128 = 4_096;

/// The step of the CC 11 curves: 10 ms, in units of 1/960 µs (ADR-0032 E8).
const CURVE_STEP: i64 = UNITS_PER_SECOND / 100;

/// The expression controller.
const CC_EXPRESSION: u8 = 11;

/// The order of events at one tick (ADR-0032 E10): note-offs, then controllers (CC 11, bend,
/// programs), then note-ons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Group {
    /// Meta events of the track (its name), before everything.
    Meta,
    /// Note-offs.
    Off,
    /// Control changes, pitch bends, program changes.
    Control,
    /// Note-ons.
    On,
}

/// A note ready to be written.
#[derive(Debug, Clone, Copy)]
struct Prepared<'a> {
    /// The played note.
    note: &'a PlayedNote,
    /// Its instrument's track (index among the instruments).
    track: usize,
    /// The key of its pitch (step 2).
    key: u8,
    /// The bend its pitch needs.
    bend: u16,
    /// Its first tick.
    start: u64,
    /// Its note-off tick, after `start`.
    end: u64,
}

/// What happens on a channel, before the channel's state decides the bytes (step 4).
#[derive(Debug, Clone, Copy)]
enum Action {
    /// The end of the prepared note at this index.
    Off(usize),
    /// A CC 11 value, in the track of the prepared note at this index (its owner).
    Expression(usize, u8),
    /// The start of the prepared note at this index.
    On(usize),
}

impl Action {
    /// Its group at one tick (E10).
    const fn group(self) -> Group {
        match self {
            Self::Off(_) => Group::Off,
            Self::Expression(..) => Group::Control,
            Self::On(_) => Group::On,
        }
    }
}

/// The time of a profile's duration in units, rounded. O(1).
fn units_of_seconds(seconds: Seconds) -> i64 {
    let r = seconds.as_rational();
    let units = round_div(
        i128::from(r.numerator()) * i128::from(UNITS_PER_SECOND),
        i128::from(r.denominator()),
    );
    i64::try_from(units).unwrap_or(i64::MAX)
}

/// The key and the 14-bit bend of an exact pitch in semitones (step 2), or `None` outside
/// 0 ..= 127.
///
/// # Why reuse `MidiNote::nearest`?
/// The key of a note half way between two keys must be the same in the model and in the file
/// (ADR-0033: the lower key, bent up by a half). Computing it here with a formula of our own is
/// exactly how the two once disagreed, so the encoder only adds the conversion of the bend.
///
/// # Bend units
/// The bend `b` (semitones, in `(−1/2, +1/2]`) becomes `8 192 + round(b × 4 096)`: 4 096 units per
/// semitone with the ±2 semitone range set by RPN 0, so the result lies in 6 145 ..= 10 240 and
/// the clamp to 0 ..= 16 383 is only a guard.
///
/// # Complexity
/// O(1).
fn key_and_bend(pitch: orchestra_core_time::Rational) -> Option<(u8, u16)> {
    // An out-of-range key (or an absurd pitch) is reported by the caller as `PitchOutOfRange`.
    let note = MidiNote::nearest(pitch).ok()?;
    // bend × 4 096, rounded half away from zero (`round_div`, the rounding of the whole encoder).
    let bend = round_div(
        i128::from(note.bend.numerator()) * BEND_PER_SEMITONE,
        i128::from(note.bend.denominator()),
    );
    let bend = u16::try_from((i128::from(BEND_CENTRE) + bend).clamp(0, 16_383)).ok()?;
    Some((note.key, bend))
}

/// The nearest key to `pitch` when the channel is bent by `bend` (step 4, `BendRounded`): the key
/// whose bent sound is closest, the **lower** one on a tie (ADR-0033), i.e.
/// `⌈pitch − c − 1/2⌉` for the channel's offset `c = (bend − 8 192) / 4 096` semitones. `None`
/// outside 0 ..= 127.
///
/// # Exact integer form
/// With `pitch = n/d`, `pitch − c − 1/2 = (4 096·n − offset·d − 2 048·d) / (4 096·d)`; the
/// denominator is positive (`d > 0`), so the ceiling is `−⌊−a / b⌋`, computed with `div_euclid`
/// (which floors for a positive divisor). Everything fits in `i128`: `|n|, d < 2⁶³` and the
/// factors are below 2¹⁴.
///
/// # Complexity
/// O(1).
fn key_under_bend(pitch: orchestra_core_time::Rational, bend: u16) -> Option<u8> {
    let (n, d) = (
        i128::from(pitch.numerator()),
        i128::from(pitch.denominator()),
    );
    let offset = i128::from(bend) - i128::from(BEND_CENTRE);
    let a = n * BEND_PER_SEMITONE - offset * d - (BEND_PER_SEMITONE / 2) * d;
    let b = BEND_PER_SEMITONE * d;
    let key = -(-a).div_euclid(b);
    u8::try_from(key).ok().filter(|k| *k <= 127)
}

/// The CC 11 value of a level relative to the note's peak: `round(127 × level / peak)`, within
/// 0 ..= 127. O(1).
fn expression(level: i64, peak: i64) -> u8 {
    let value = round_div(i128::from(level) * 127, i128::from(peak.max(1)));
    u8::try_from(value.clamp(0, 127)).unwrap_or(127)
}

/// The CC 11 curve of `note`, owner of its channel in `[from, until)` (step 3): (tick, value)
/// pairs, increasing ticks, consecutive values distinct.
///
/// # Complexity
/// O(`settle_in` / 10 ms + 1), each point O(log s) in the tempo segments.
fn curve(note: &PlayedNote, from: u64, until: u64, tempo: &TempoTrack) -> Vec<(u64, u8)> {
    let (v, s) = (i64::from(note.velocity), i64::from(note.settle_to));
    let peak = v.max(s);
    let mut out: Vec<(u64, u8)> = Vec::new();
    let mut push = |tick: u64, value: u8| {
        if tick < until && out.last().is_none_or(|(_, last)| *last != value) {
            out.push((tick, value));
        }
    };
    push(from, expression(v, peak));
    let hold = units_of_seconds(note.hold);
    let settle = units_of_seconds(note.settle_in);
    if hold == 0 && settle == 0 && s == v {
        // No profile: the attack's level for the whole note.
        return out;
    }
    let start = tempo.units_at(from);
    let ramp = start.saturating_add(hold);
    if settle > 0 {
        let mut elapsed = 0_i64;
        while elapsed < settle {
            let tick = tempo.tick_at(ramp.saturating_add(elapsed));
            if tick >= until {
                return out;
            }
            // level = v + (s − v) × elapsed / settle, exact in integers then rounded by `expression`
            // on the ratio: 127 × (v·settle + (s − v)·elapsed) / (peak·settle).
            let level =
                i128::from(v) * i128::from(settle) + i128::from(s - v) * i128::from(elapsed);
            let value = round_div(level * 127, i128::from(peak.max(1)) * i128::from(settle));
            push(tick, u8::try_from(value.clamp(0, 127)).unwrap_or(127));
            elapsed = elapsed.saturating_add(CURVE_STEP);
        }
    }
    push(
        tempo.tick_at(ramp.saturating_add(settle)),
        expression(s, peak),
    );
    out
}

/// An event of a track before the final sort (step 5), with its sorting keys.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Staged {
    /// Its tick.
    tick: u64,
    /// Its group at that tick (E10).
    group: Group,
    /// Its channel.
    channel: u8,
    /// Its key, for note-ons and note-offs (0 otherwise).
    key: u8,
    /// The event.
    message: Message,
}

impl Staged {
    /// A staged event. O(1).
    const fn new(tick: u64, group: Group, channel: u8, key: u8, message: Message) -> Self {
        Self {
            tick,
            group,
            channel,
            key,
            message,
        }
    }
}

/// Step 1: one track per instrument with its name, the set-up of its channel (if it is the first
/// on it) and its program; and the channel of each track. O(instruments).
fn set_up(
    doc: &Document,
    performance: &Performance,
    options: &MidiOptions,
    warnings: &mut Vec<MidiWarning>,
) -> (Vec<Vec<Staged>>, Vec<u8>) {
    let mut tracks = Vec::new();
    let mut channel_of = Vec::new();
    let mut done = [false; 16];
    for (index, (instrument, _)) in performance.instruments().enumerate() {
        let channel = CHANNELS[index % CHANNELS.len()];
        let mut events = Vec::new();
        let name = doc.instrument(instrument).map_or("", |i| i.name());
        if !name.is_empty() {
            events.push(Staged::new(
                0,
                Group::Meta,
                channel,
                0,
                smf::track_name(name),
            ));
        }
        let slot = usize::from(channel);
        if done[slot] {
            warnings.push(MidiWarning::ChannelShared {
                instrument,
                channel,
            });
        } else {
            done[slot] = true;
            // RPN 0 (pitch bend range) = 2 semitones, 0 cents; then the null RPN, so that a stray
            // data entry later cannot change it; CC 11 at full; bend at the centre.
            let setup = [(101, 0), (100, 0), (6, 2), (38, 0), (101, 127), (100, 127)]
                .into_iter()
                .map(|(controller, value)| smf::control(channel, controller, value))
                .chain([
                    smf::control(channel, CC_EXPRESSION, 127),
                    smf::pitch_bend(channel, BEND_CENTRE),
                ]);
            events.extend(setup.map(|m| Staged::new(0, Group::Control, channel, 0, m)));
        }
        if let Some(program) = options.program(instrument) {
            events.push(Staged::new(
                0,
                Group::Control,
                channel,
                0,
                smf::program(channel, program),
            ));
        }
        tracks.push(events);
        channel_of.push(channel);
    }
    (tracks, channel_of)
}

/// Step 2: the prepared notes, grouped by channel (16 lists), each sorted by start, key and
/// track (a total order on the channel's notes; the performance's own order breaks the remaining
/// ties, the sort being stable).
///
/// # Errors
/// The arithmetic errors of the ticks.
///
/// # Complexity
/// O(n log n).
fn prepare<'a>(
    performance: &'a Performance,
    channel_of: &[u8],
    lead: i64,
    warnings: &mut Vec<MidiWarning>,
) -> Result<Vec<Vec<Prepared<'a>>>, MidiError> {
    let tick = |p| -> Result<u64, MidiError> {
        Ok(u64::try_from(ticks_of(p)?.saturating_add(lead)).unwrap_or(0))
    };
    let mut by_channel: Vec<Vec<Prepared<'a>>> = vec![Vec::new(); 16];
    for (track, (_, notes)) in performance.instruments().enumerate() {
        let channel = channel_of.get(track).copied().unwrap_or(0);
        for note in notes {
            let Some((key, bend)) = key_and_bend(note.pitch) else {
                warnings.push(MidiWarning::PitchOutOfRange {
                    note: note.note,
                    played: note.played,
                });
                continue;
            };
            let start = tick(note.start)?;
            // At least one tick long (E3): a very short grace note may round to nothing.
            let end = tick(note.end)?.max(start.saturating_add(1));
            if let Some(list) = by_channel.get_mut(usize::from(channel)) {
                list.push(Prepared {
                    note,
                    track,
                    key,
                    bend,
                    start,
                    end,
                });
            }
        }
    }
    for list in &mut by_channel {
        list.sort_by_key(|p| (p.start, p.key, p.track));
    }
    Ok(by_channel)
}

/// Step 3: the actions of a channel's sorted notes, with the CC 11 curve of each note in the
/// window where it owns the controller, sorted by tick, group and key (E10), the creation order
/// breaking ties (stable sort). O(a log a) for a actions.
fn actions(notes: &[Prepared<'_>], tempo: &TempoTrack) -> Vec<(u64, Action)> {
    let mut actions: Vec<(u64, Action)> = Vec::with_capacity(notes.len().saturating_mul(3));
    for (i, p) in notes.iter().enumerate() {
        actions.push((p.start, Action::On(i)));
        actions.push((p.end, Action::Off(i)));
        // The window: until the next note-on of the channel (the latest attack takes over) or the
        // note's own end. Empty when the next note starts at the same tick.
        let next = notes.get(i + 1).map_or(u64::MAX, |q| q.start);
        let until = next.min(p.end);
        if until > p.start {
            for (tick, value) in curve(p.note, p.start, until, tempo) {
                actions.push((tick, Action::Expression(i, value)));
            }
        }
    }
    let key_of = |a: Action| match a {
        Action::Off(i) | Action::On(i) => notes.get(i).map_or(0, |p| p.key),
        Action::Expression(..) => 0,
    };
    actions.sort_by_key(|(tick, a)| (*tick, a.group(), key_of(*a)));
    actions
}

/// The state of one channel while its actions are played in order (step 4).
struct Channel<'n, 'a> {
    /// The channel, 0 ..= 15.
    number: u8,
    /// Its sorted notes.
    notes: &'n [Prepared<'a>],
    /// The current pitch bend.
    bend: u16,
    /// The current CC 11 value.
    expression: u8,
    /// How many notes sound on each key (logically: a note cut short by a unison still counts
    /// until its own end).
    sounding: [u16; 128],
    /// For each key, the track of the MIDI note actually sounding on it: the latest note-on. A
    /// unison's note-off goes to that track, so that every track closes the notes it opened even
    /// when two tracks share the channel.
    holder: [usize; 128],
    /// For each key, the tick of the latest note-on written on it.
    holder_tick: [u64; 128],
    /// How many notes sound in all.
    total: u32,
    /// The key each note was actually played on (`None` before its note-on, or if skipped): it
    /// may differ from its own key after a `BendRounded`.
    played_key: Vec<Option<u8>>,
}

impl<'n, 'a> Channel<'n, 'a> {
    /// A channel just set up: no bend, CC 11 at 127, nothing sounding. O(notes).
    fn new(number: u8, notes: &'n [Prepared<'a>]) -> Self {
        Self {
            number,
            notes,
            bend: BEND_CENTRE,
            expression: 127,
            sounding: [0; 128],
            holder: [0; 128],
            holder_tick: [0; 128],
            total: 0,
            played_key: vec![None; notes.len()],
        }
    }

    /// Plays one action, appending its events (if any) to the tracks. O(1).
    fn play(
        &mut self,
        tick: u64,
        action: Action,
        tracks: &mut [Vec<Staged>],
        warnings: &mut Vec<MidiWarning>,
    ) {
        let ch = self.number;
        match action {
            Action::Off(i) => {
                let Some(Some(key)) = self.played_key.get(i).copied() else {
                    return;
                };
                let count = &mut self.sounding[usize::from(key)];
                *count = count.saturating_sub(1);
                self.total = self.total.saturating_sub(1);
                // Only the last note on the key ends it (E10), in the track that holds the key.
                let holder = self.holder[usize::from(key)];
                if let (0, Some(t)) = (*count, tracks.get_mut(holder)) {
                    t.push(Staged::new(
                        tick,
                        Group::Off,
                        ch,
                        key,
                        smf::note_off(ch, key),
                    ));
                }
            }
            Action::Expression(i, value) => {
                if value == self.expression {
                    return;
                }
                self.expression = value;
                if let Some(t) = self.notes.get(i).and_then(|p| tracks.get_mut(p.track)) {
                    t.push(Staged::new(
                        tick,
                        Group::Control,
                        ch,
                        0,
                        smf::control(ch, CC_EXPRESSION, value),
                    ));
                }
            }
            Action::On(i) => self.note_on(tick, i, tracks, warnings),
        }
    }

    /// The note-on of the note at `i` (step 4): its bend, or the nearest key under the current
    /// one, and the unison rule. O(1).
    fn note_on(
        &mut self,
        tick: u64,
        i: usize,
        tracks: &mut [Vec<Staged>],
        warnings: &mut Vec<MidiWarning>,
    ) {
        let ch = self.number;
        let Some(p) = self.notes.get(i).copied() else {
            return;
        };
        let key = if p.bend == self.bend {
            Some(p.key)
        } else if self.total == 0 {
            // Nothing sounds: the channel takes the bend this note needs.
            self.bend = p.bend;
            let Some(t) = tracks.get_mut(p.track) else {
                return;
            };
            t.push(Staged::new(
                tick,
                Group::Control,
                ch,
                0,
                smf::pitch_bend(ch, p.bend),
            ));
            Some(p.key)
        } else {
            // Another bend sounds: the nearest key under it (E9).
            warnings.push(MidiWarning::BendRounded {
                note: p.note.note,
                played: p.note.played,
            });
            key_under_bend(p.note.pitch, self.bend)
        };
        let Some(key) = key else {
            warnings.push(MidiWarning::PitchOutOfRange {
                note: p.note.note,
                played: p.note.played,
            });
            return;
        };
        if let Some(slot) = self.played_key.get_mut(i) {
            *slot = Some(key);
        }
        // max(v, s) >> 9 on the 7-bit scale, at least 1 (E8).
        let velocity =
            u8::try_from((p.note.velocity.max(p.note.settle_to) >> 9).max(1)).unwrap_or(127);
        let slot = usize::from(key);
        let unison = self.sounding[slot] > 0;
        self.sounding[slot] = self.sounding[slot].saturating_add(1);
        self.total = self.total.saturating_add(1);
        if unison && self.holder_tick[slot] == tick {
            // Two attacks of one key at the same instant (two voices in unison, two instruments
            // sharing the channel) are one attack: the note sounding already is it. Writing a
            // note-off here would be sorted before that note-on (E10) and leave it open.
            return;
        }
        if unison {
            // A unison: the attack is heard again (E10). The note-off closes the MIDI note of the
            // track holding the key; the final note-off, at the last end, belongs to this track.
            if let Some(t) = tracks.get_mut(self.holder[slot]) {
                t.push(Staged::new(
                    tick,
                    Group::Off,
                    ch,
                    key,
                    smf::note_off(ch, key),
                ));
            }
        }
        self.holder[slot] = p.track;
        self.holder_tick[slot] = tick;
        let Some(t) = tracks.get_mut(p.track) else {
            return;
        };
        t.push(Staged::new(
            tick,
            Group::On,
            ch,
            key,
            smf::note_on(ch, key, velocity),
        ));
    }
}

/// The events of every instrument's track (see the module documentation), with the warnings of
/// the channels and the notes appended to `warnings`.
///
/// # Errors
/// The arithmetic errors of the ticks.
///
/// # Complexity
/// O(n log n) for n played notes, plus the curves.
pub(crate) fn tracks(
    doc: &Document,
    performance: &Performance,
    tempo: &TempoTrack,
    lead: i64,
    options: &MidiOptions,
    warnings: &mut Vec<MidiWarning>,
) -> Result<Vec<Vec<Event>>, MidiError> {
    let (mut tracks, channel_of) = set_up(doc, performance, options, warnings);
    let by_channel = prepare(performance, &channel_of, lead, warnings)?;
    for (number, notes) in by_channel.iter().enumerate() {
        let mut channel = Channel::new(u8::try_from(number).unwrap_or(0), notes);
        for (tick, action) in actions(notes, tempo) {
            channel.play(tick, action, &mut tracks, warnings);
        }
    }
    // Step 5: sort each track and drop the sorting keys.
    Ok(tracks
        .into_iter()
        .map(|mut events| {
            events.sort_by_key(|e| (e.tick, e.group, e.channel, e.key));
            events
                .into_iter()
                .map(|e| Event {
                    tick: e.tick,
                    message: e.message,
                })
                .collect()
        })
        .collect())
}

#[cfg(test)]
mod tests {
    //! The pure helpers on hand-computed values.

    use super::*;
    use orchestra_core_time::{Rational, RationalError};

    /// Keys and bends: a plain C4, a quarter-tone above (exactly half way: the lower key bent up,
    /// ADR-0033), a quarter and three quarters of a semitone above, the ends of the range.
    #[test]
    fn keys_and_bends() -> Result<(), RationalError> {
        assert_eq!(key_and_bend(Rational::from_integer(60)?), Some((60, 8_192)));
        assert_eq!(
            key_and_bend(Rational::new(121, 2)?),
            Some((60, 8_192 + 2_048)),
            "half way: the lower key"
        );
        assert_eq!(
            key_and_bend(Rational::new(241, 4)?),
            Some((60, 8_192 + 1_024))
        );
        assert_eq!(
            key_and_bend(Rational::new(243, 4)?),
            Some((61, 8_192 - 1_024))
        );
        assert_eq!(key_and_bend(Rational::from_integer(128)?), None);
        assert_eq!(key_and_bend(Rational::from_integer(-1)?), None);
        // The ends move with the rule, exactly as in `Pitch::midi`: 127½ is 127 bent up, −½ would
        // need key −1.
        assert_eq!(
            key_and_bend(Rational::new(255, 2)?),
            Some((127, 8_192 + 2_048))
        );
        assert_eq!(key_and_bend(Rational::new(-1, 2)?), None);
        Ok(())
    }

    /// The nearest key under a bend: a plain D under an eighth-tone up stays D (error ¼); a
    /// quarter of a semitone above C under no bend becomes C; half-way cases take the lower key.
    #[test]
    fn keys_under_a_bend() -> Result<(), RationalError> {
        assert_eq!(
            key_under_bend(Rational::from_integer(62)?, 8_192 + 1_024),
            Some(62)
        );
        assert_eq!(key_under_bend(Rational::new(241, 4)?, 8_192), Some(60));
        assert_eq!(
            key_under_bend(Rational::from_integer(62)?, 8_192 + 2_048 + 100),
            Some(61)
        );
        // Half way under the bend: E4 + ½ under no bend, and D4 under a bend of +½, take the
        // lower key (ADR-0033).
        assert_eq!(key_under_bend(Rational::new(129, 2)?, 8_192), Some(64));
        assert_eq!(
            key_under_bend(Rational::from_integer(62)?, 8_192 + 2_048),
            Some(61)
        );
        Ok(())
    }

    /// CC 11 values relative to the peak.
    #[test]
    fn expression_values() {
        assert_eq!(expression(1_000, 1_000), 127);
        assert_eq!(expression(500, 1_000), 64, "63.5 rounds up");
        assert_eq!(expression(0, 1_000), 0);
    }
}
