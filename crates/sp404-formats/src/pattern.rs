//! `PTN/PTNnnnnn.BIN` pattern files. Layout: `docs/re/04-patterns.md`.

use sp404_proto::PadIndex;

use crate::FormatError;

/// Ticks per quarter note.
pub const PPQ: u16 = 480;

const RECORD: usize = 8;
const NOTE_FIRST: u8 = 0x2F;
const NOTE_LAST: u8 = 0x7F;
const END: u8 = 0x8C;
const CONTROL: u8 = 0x8E;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    Note {
        tick: u32,
        pad: PadIndex,
        /// Byte 3: 0 for a plain note, otherwise the chromatic pitch value.
        pitch: u8,
        velocity: u8,
        off_velocity: u8,
        /// Gate length in ticks.
        length: u16,
        /// Byte 2 bits other than the pad group (meaning unknown).
        flags: u8,
    },
    Control {
        tick: u32,
        channel: u8,
        controller: u8,
        value: u8,
    },
}

impl Event {
    pub fn tick(&self) -> u32 {
        match *self {
            Event::Note { tick, .. } | Event::Control { tick, .. } => tick,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pattern {
    pub events: Vec<Event>,
    /// Tick of the end record: the pattern length.
    pub length: u32,
    /// The 8 bytes after the end record, if present.
    pub trailer: Option<[u8; RECORD]>,
}

impl Pattern {
    pub fn parse(bytes: &[u8]) -> Result<Self, FormatError> {
        if bytes.len() % RECORD != 0 || bytes.is_empty() {
            return Err(FormatError::BadPattern("length is not a multiple of 8"));
        }
        let mut events = Vec::new();
        let mut tick = 0u32;
        let mut records = bytes.chunks_exact(RECORD);
        let mut ended = false;
        for r in records.by_ref() {
            match r[1] {
                NOTE_FIRST..=NOTE_LAST => {
                    let group = u16::from(r[2] & 0x0F);
                    let index = group * 5 * 16 + u16::from(r[1] - NOTE_FIRST);
                    let pad = PadIndex::new(index)
                        .ok_or(FormatError::BadPattern("note on a pad beyond J16"))?;
                    events.push(Event::Note {
                        tick,
                        pad,
                        pitch: r[3],
                        velocity: r[4].min(127),
                        off_velocity: r[5],
                        length: u16::from_le_bytes([r[6], r[7]]),
                        flags: r[2] & 0xF0,
                    });
                }
                CONTROL => events.push(Event::Control {
                    tick,
                    channel: r[2] & 0x0F,
                    controller: r[5],
                    value: r[6],
                }),
                END => {
                    ended = true;
                    break;
                }
                _ => {}
            }
            tick += u32::from(r[0]);
        }
        let trailer = if ended {
            records.next().map(|t| t.try_into().unwrap())
        } else {
            None
        };
        Ok(Pattern {
            events,
            length: tick,
            trailer,
        })
    }

    /// Beats per bar from the trailer (the denominator is always 4).
    pub fn beats_per_bar(&self) -> Option<u8> {
        let code = self.trailer?[4];
        Some(match code {
            1..=3 => code,
            4..=6 => code + 1,
            _ => 4,
        })
    }
}

/// A valid pattern containing one full-velocity note at tick zero.
///
/// This lets the unit render a pad with its current playback settings. The pattern lasts
/// `bars` bars of 4/4; no-op records carry gaps longer than a record's one-byte delta.
pub fn single_pad(pad: PadIndex, bars: u8) -> Vec<u8> {
    let bars = bars.clamp(1, 64);
    let ticks = u32::from(bars) * 4 * u32::from(PPQ);
    let group = pad.bank() / 5;
    let within_group = (pad.bank() % 5) * 16 + pad.pad();
    let note = 0x2F + within_group as u8;
    let gate = ticks.min(u32::from(u16::MAX)) as u16;

    let mut out = Vec::new();
    let mut remaining = ticks;
    let first = remaining.min(255) as u8;
    remaining -= u32::from(first);
    out.extend_from_slice(&[
        first,
        note,
        group as u8,
        0,
        127,
        64,
        gate as u8,
        (gate >> 8) as u8,
    ]);
    while remaining > 0 {
        let delta = remaining.min(255) as u8;
        remaining -= u32::from(delta);
        out.extend_from_slice(&[delta, 0x80, 0, 0, 0, 0, 0, 0]);
    }
    out.extend_from_slice(&[0, END, 0, 0, 0, 0, 0, 0]);
    out.extend_from_slice(&[bars, 0, 0, 0, 0, 0x80, 0x04, 0x01]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(b: [u8; 8]) -> Vec<u8> {
        b.to_vec()
    }

    #[test]
    fn delta_applies_after_each_record() {
        let bytes = [
            rec([0x10, 0x56, 0x00, 0x8D, 0x7F, 0x40, 0x77, 0x00]), // C8 at 0, then +16
            rec([0xFF, 0x80, 0, 0, 0, 0, 0, 0]),                   // filler +255
            rec([0x05, 0x6C, 0x40, 0x00, 0x7F, 0x40, 0x23, 0x00]), // D14 at 271 with flag
            rec([0x00, 0x2F, 0x01, 0x00, 0x64, 0x40, 0x10, 0x01]), // F1 at 276, length 272
            rec([0x00, END, 0, 0, 0, 0, 0, 0]),
            rec([0x04, 0, 0, 0, 0x03, 0x80, 0x04, 0x01]),
        ]
        .concat();
        let p = Pattern::parse(&bytes).unwrap();
        assert_eq!(p.length, 276);
        assert_eq!(p.beats_per_bar(), Some(3));
        let ticks: Vec<u32> = p.events.iter().map(Event::tick).collect();
        assert_eq!(ticks, vec![0, 271, 276]);
        match p.events[0] {
            Event::Note {
                pad, pitch, length, ..
            } => {
                assert_eq!((pad.to_string(), pitch, length), ("C8".into(), 0x8D, 0x77));
            }
            _ => panic!(),
        }
        match p.events[1] {
            Event::Note { pad, flags, .. } => {
                assert_eq!((pad.to_string(), flags), ("D14".into(), 0x40))
            }
            _ => panic!(),
        }
        match p.events[2] {
            Event::Note {
                pad,
                velocity,
                length,
                ..
            } => {
                assert_eq!((pad.to_string(), velocity, length), ("F1".into(), 100, 272));
            }
            _ => panic!(),
        }
    }

    #[test]
    fn rejects_ragged_files() {
        assert!(Pattern::parse(&[0; 7]).is_err());
    }

    #[test]
    fn builds_a_single_pad_pattern() {
        let pad = PadIndex::from_bank_pad(7, 12).unwrap();
        let bytes = single_pad(pad, 3);
        let pattern = Pattern::parse(&bytes).unwrap();
        assert_eq!(pattern.length, 3 * 4 * u32::from(PPQ));
        assert_eq!(pattern.beats_per_bar(), Some(4));
        assert!(matches!(
            pattern.events.as_slice(),
            [Event::Note { tick: 0, pad: found, velocity: 127, .. }] if *found == pad
        ));
    }
}
