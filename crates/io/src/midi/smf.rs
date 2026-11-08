// SPDX-License-Identifier: GPL-3.0-or-later

//! The byte encoder of a Standard MIDI File 1.0 (ADR-0032 E1, E2): variable-length quantities,
//! channel and meta events, track chunks, the header. It knows nothing about music: the other
//! modules of [`crate::midi`] decide *what* to write and hand it over as [`Event`]s with absolute
//! ticks; this module only decides *how* the bytes look.
//!
//! # The format, in short
//! A file is a header chunk `MThd` followed by track chunks `MTrk`. Every chunk is a four-byte
//! tag, a 32-bit big-endian length and the data. A track is a list of events, each preceded by the
//! **delta time** in ticks since the previous event, written as a *variable-length quantity*
//! (7 bits per byte, most significant first, the high bit set on every byte but the last). A track
//! ends with the meta event "end of track" (`FF 2F 00`).
//!
//! # Choices (ADR-0032 E2)
//! - **No running status:** every channel event writes its status byte, even when it repeats the
//!   previous one. A few percent larger, but simpler, and read correctly by every program.
//! - **Deterministic:** the same events give the same bytes on every platform; nothing depends on
//!   the platform's endianness or line endings.

/// The division of the header: ticks per quarter note (ADR-0032 E3).
pub(crate) const TICKS_PER_QUARTER: u16 = 960;

/// The largest value a variable-length quantity can hold in a Standard MIDI File: four bytes of
/// seven bits (`0x0FFF_FFFF`, about 279 620 quarter notes at 960 ticks each). A longer delta time
/// is an error of the export ([`TooLong`]).
pub(crate) const MAX_DELTA: u64 = 0x0FFF_FFFF;

/// The delta time between two events, or the length of a chunk, does not fit the format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TooLong;

/// The content of an event, without its time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Message {
    /// A channel message of three bytes: status (with the channel in the low nibble) and two data
    /// bytes (note-on, note-off, control change, pitch bend).
    Three([u8; 3]),
    /// A channel message of two bytes: status and one data byte (program change).
    Two([u8; 2]),
    /// A meta event: its type byte and its data (tempo, time signature, marker, track name).
    Meta(u8, Vec<u8>),
}

/// An event at an absolute tick of its track.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Event {
    /// Ticks from the start of the file (not a delta: the encoder computes the deltas).
    pub(crate) tick: u64,
    /// What happens at that tick.
    pub(crate) message: Message,
}

// ------------------------------------------------------------------------------------------------
// Message constructors. Channels are 0..=15 on the wire (shown to users as 1..=16); data bytes
// are 0..=127. The callers guarantee the ranges; the constructors mask the values anyway, so that a
// bug elsewhere produces a wrong note rather than a corrupt file.
// ------------------------------------------------------------------------------------------------

/// Note-on (`9n kk vv`). `velocity` must be at least 1: 0 means note-off to most readers.
pub(crate) const fn note_on(channel: u8, key: u8, velocity: u8) -> Message {
    Message::Three([0x90 | (channel & 0x0F), key & 0x7F, velocity & 0x7F])
}

/// Note-off (`8n kk 40`), with the release velocity 64, the conventional "no information" value.
pub(crate) const fn note_off(channel: u8, key: u8) -> Message {
    Message::Three([0x80 | (channel & 0x0F), key & 0x7F, 64])
}

/// Control change (`Bn cc vv`).
pub(crate) const fn control(channel: u8, controller: u8, value: u8) -> Message {
    Message::Three([0xB0 | (channel & 0x0F), controller & 0x7F, value & 0x7F])
}

/// Pitch bend (`En ll mm`): 14 bits, centre 8 192, least significant seven bits first.
pub(crate) const fn pitch_bend(channel: u8, value: u16) -> Message {
    // `as u8` keeps the low byte, after masking to the seven bits each data byte may carry.
    #[allow(
        clippy::cast_possible_truncation,
        reason = "masked to seven bits first: the value always fits"
    )]
    let (lsb, msb) = ((value & 0x7F) as u8, ((value >> 7) & 0x7F) as u8);
    Message::Three([0xE0 | (channel & 0x0F), lsb, msb])
}

/// Program change (`Cn pp`).
pub(crate) const fn program(channel: u8, program: u8) -> Message {
    Message::Two([0xC0 | (channel & 0x0F), program & 0x7F])
}

/// Tempo meta event (`FF 51 03 tt tt tt`): microseconds per quarter note, 1 ..= 16 777 215. The
/// caller clamps the value (ADR-0032 E4); the top byte of `micros` is dropped.
pub(crate) fn tempo(micros: u32) -> Message {
    let [_, a, b, c] = micros.to_be_bytes();
    Message::Meta(0x51, vec![a, b, c])
}

/// Time signature meta event (`FF 58 04 nn dd cc bb`): numerator, the denominator's exponent of
/// two, 24 MIDI clocks per metronome click and 8 thirty-second notes per quarter (the standard
/// values, ADR-0032 E5).
pub(crate) fn time_signature(numerator: u8, denominator_exponent: u8) -> Message {
    Message::Meta(0x58, vec![numerator, denominator_exponent, 24, 8])
}

/// Marker meta event (`FF 06 len text`), the text as UTF-8 bytes.
pub(crate) fn marker(text: &str) -> Message {
    Message::Meta(0x06, text.as_bytes().to_vec())
}

/// Track name meta event (`FF 03 len text`), the text as UTF-8 bytes (ADR-0032 E1: readers that
/// expect Latin-1 show odd characters for non-ASCII names, which is harmless).
pub(crate) fn track_name(text: &str) -> Message {
    Message::Meta(0x03, text.as_bytes().to_vec())
}

// ------------------------------------------------------------------------------------------------
// Encoding
// ------------------------------------------------------------------------------------------------

/// Appends `value` as a variable-length quantity: groups of seven bits, most significant first,
/// the high bit set on every byte but the last. `0` is one byte `00`; `0x0FFF_FFFF` four bytes.
///
/// # Errors
/// [`TooLong`] above [`MAX_DELTA`]: the format allows at most four bytes.
///
/// # Complexity
/// O(1): at most four bytes.
pub(crate) fn push_vlq(out: &mut Vec<u8>, value: u64) -> Result<(), TooLong> {
    if value > MAX_DELTA {
        return Err(TooLong);
    }
    // The groups from the least significant, into a small buffer read backwards: there are at
    // most four of them.
    let mut groups = [0_u8; 4];
    let mut count = 0;
    let mut rest = value;
    loop {
        #[allow(
            clippy::cast_possible_truncation,
            reason = "masked to seven bits: the value always fits"
        )]
        let group = (rest & 0x7F) as u8;
        groups[count] = group;
        count += 1;
        rest >>= 7;
        if rest == 0 {
            break;
        }
    }
    // The most significant group comes first; every group but the last one written (the least
    // significant) carries the continuation bit.
    for (i, group) in groups[..count].iter().enumerate().rev() {
        out.push(if i == 0 { *group } else { group | 0x80 });
    }
    Ok(())
}

/// The bytes of one track chunk (`MTrk`) holding `events`, which must be **sorted by tick**
/// (stable: the order of events at the same tick is kept as given, ADR-0032 E10). The "end of
/// track" meta event is added at the tick of the last event.
///
/// # Errors
/// [`TooLong`] if a delta time exceeds [`MAX_DELTA`], a meta event's data exceeds it, or the
/// chunk exceeds 4 GiB (the length field has 32 bits).
///
/// # Complexity
/// O(events + bytes).
pub(crate) fn track_chunk(events: &[Event]) -> Result<Vec<u8>, TooLong> {
    // The data first, then the header with its length: the length is known only at the end.
    let mut data = Vec::with_capacity(events.len().saturating_mul(4).saturating_add(4));
    let mut previous = 0_u64;
    for event in events {
        // A tick before the previous one means the caller did not sort: report it as an encoding
        // failure instead of writing a negative delta (which the format cannot express).
        let delta = event.tick.checked_sub(previous).ok_or(TooLong)?;
        push_vlq(&mut data, delta)?;
        previous = event.tick;
        match &event.message {
            Message::Three(bytes) => data.extend_from_slice(bytes),
            Message::Two(bytes) => data.extend_from_slice(bytes),
            Message::Meta(kind, payload) => {
                data.push(0xFF);
                data.push(*kind);
                let len = u64::try_from(payload.len()).map_err(|_| TooLong)?;
                push_vlq(&mut data, len)?;
                data.extend_from_slice(payload);
            }
        }
    }
    // End of track, right after the last event (delta 0).
    data.extend_from_slice(&[0x00, 0xFF, 0x2F, 0x00]);
    let len = u32::try_from(data.len()).map_err(|_| TooLong)?;
    let mut chunk = Vec::with_capacity(data.len().saturating_add(8));
    chunk.extend_from_slice(b"MTrk");
    chunk.extend_from_slice(&len.to_be_bytes());
    chunk.extend_from_slice(&data);
    Ok(chunk)
}

/// The header chunk (`MThd`) of a format-1 file with `tracks` tracks at [`TICKS_PER_QUARTER`].
pub(crate) fn header(tracks: u16) -> [u8; 14] {
    let [t0, t1] = tracks.to_be_bytes();
    let [d0, d1] = TICKS_PER_QUARTER.to_be_bytes();
    // Tag, length 6, format 1, number of tracks, division (positive: ticks per quarter note).
    [b'M', b'T', b'h', b'd', 0, 0, 0, 6, 0, 1, t0, t1, d0, d1]
}

#[cfg(test)]
mod tests {
    //! The encoder byte by byte, against the examples of the SMF 1.0 specification.

    use super::*;

    /// The variable-length quantities of the specification's table.
    #[test]
    fn vlq_matches_the_specification() -> Result<(), TooLong> {
        let cases: [(u64, &[u8]); 8] = [
            (0x00, &[0x00]),
            (0x40, &[0x40]),
            (0x7F, &[0x7F]),
            (0x80, &[0x81, 0x00]),
            (0x2000, &[0xC0, 0x00]),
            (0x3FFF, &[0xFF, 0x7F]),
            (0x0010_0000, &[0xC0, 0x80, 0x00]),
            (0x0FFF_FFFF, &[0xFF, 0xFF, 0xFF, 0x7F]),
        ];
        for (value, bytes) in cases {
            let mut out = Vec::new();
            push_vlq(&mut out, value)?;
            assert_eq!(out, bytes, "{value:#x}");
        }
        assert_eq!(push_vlq(&mut Vec::new(), MAX_DELTA + 1), Err(TooLong));
        Ok(())
    }

    /// A track with one note: deltas, status bytes, end of track and the chunk's length.
    #[test]
    fn a_track_with_one_note() -> Result<(), TooLong> {
        let events = [
            Event {
                tick: 0,
                message: note_on(2, 60, 100),
            },
            Event {
                tick: 960,
                message: note_off(2, 60),
            },
        ];
        let chunk = track_chunk(&events)?;
        let expected: &[u8] = &[
            b'M', b'T', b'r', b'k', 0, 0, 0, 13, // tag and length 13
            0x00, 0x92, 60, 100, // delta 0, note-on channel 3
            0x87, 0x40, 0x82, 60, 64, // delta 960 = 0x3C0 → 87 40, note-off
            0x00, 0xFF, 0x2F, 0x00, // end of track
        ];
        assert_eq!(chunk, expected);
        Ok(())
    }

    /// Unsorted events are refused rather than encoded with a negative delta.
    #[test]
    fn unsorted_events_are_refused() {
        let events = [
            Event {
                tick: 10,
                message: note_on(0, 60, 1),
            },
            Event {
                tick: 5,
                message: note_off(0, 60),
            },
        ];
        assert_eq!(track_chunk(&events), Err(TooLong));
    }

    /// Meta events and the header.
    #[test]
    fn meta_events_and_header() -> Result<(), TooLong> {
        let events = [
            Event {
                tick: 0,
                message: tempo(500_000),
            },
            Event {
                tick: 0,
                message: time_signature(6, 3),
            },
        ];
        let chunk = track_chunk(&events)?;
        assert_eq!(
            chunk.get(8..),
            Some(
                &[
                    0x00, 0xFF, 0x51, 0x03, 0x07, 0xA1, 0x20, // 500 000 = 0x07A120
                    0x00, 0xFF, 0x58, 0x04, 6, 3, 24, 8, // 6/8
                    0x00, 0xFF, 0x2F, 0x00,
                ][..]
            )
        );
        assert_eq!(
            header(3),
            [b'M', b'T', b'h', b'd', 0, 0, 0, 6, 0, 1, 0, 3, 0x03, 0xC0]
        );
        assert_eq!(
            pitch_bend(0, 8192),
            Message::Three([0xE0, 0x00, 0x40]),
            "the centre"
        );
        assert_eq!(pitch_bend(1, 16_383), Message::Three([0xE1, 0x7F, 0x7F]));
        Ok(())
    }
}
