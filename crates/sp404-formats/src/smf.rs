//! Pattern → Standard MIDI File, following the official app's export rules
//! (`docs/re/04-patterns.md`, "Export as SMF").

use crate::pattern::{Event, PPQ, Pattern};

/// Note offset by pad position: pad 13 (bottom-left) = 36, pad 1 = 48.
const PAD_NOTE: [u8; 16] = [12, 13, 14, 15, 8, 9, 10, 11, 4, 5, 6, 7, 0, 1, 2, 3];
const NOTE_BASE: u8 = 36;

/// Encode `pattern` as a format-0 SMF. `bank_tempo` is BPM × 100 of the pattern's bank.
pub fn from_pattern(pattern: &Pattern, bank_tempo: u16) -> Vec<u8> {
    // (tick, bytes); the stable sort below keeps insertion order within a tick.
    let mut events: Vec<(u32, Vec<u8>)> = Vec::new();
    let tempo = u32::try_from(60_000_000u64 * 100 / u64::from(bank_tempo.max(1)))
        .unwrap_or(u32::MAX)
        .min(0xFF_FFFF);
    events.push((0, meta(0x51, &tempo.to_be_bytes()[1..])));

    let beats = pattern.beats_per_bar();
    let time_signature = beats.map(|n| meta(0x58, &[n, 2, 1, 0x60]));
    let mut tick0_notes_done = false;

    for event in &pattern.events {
        if !tick0_notes_done && event.tick() > 0 {
            if let Some(ts) = &time_signature {
                events.push((0, ts.clone()));
            }
            tick0_notes_done = true;
        }
        match *event {
            Event::Note {
                tick,
                pad,
                pitch,
                velocity,
                off_velocity,
                length,
                ..
            } => {
                let channel = (pad.bank() as u8).min(15);
                let note = NOTE_BASE + PAD_NOTE[usize::from(pad.pad())];
                let off = tick + u32::from(length);
                let aftertouch = (pitch != 0).then(|| {
                    let v = (if pitch & 0x80 != 0 { 64 } else { 0 }) + (pitch & 0x7F);
                    vec![0xA0 | channel, note, v & 0x7F]
                });
                if let Some(at) = &aftertouch {
                    events.push((tick, at.clone()));
                }
                events.push((tick, vec![0x90 | channel, note, velocity & 0x7F]));
                if let Some(at) = aftertouch {
                    events.push((off, at));
                }
                events.push((off, vec![0x80 | channel, note, off_velocity & 0x7F]));
            }
            Event::Control {
                tick,
                channel,
                controller,
                value,
            } => events.push((
                tick,
                vec![0xB0 | (channel & 0x0F), controller & 0x7F, value & 0x7F],
            )),
        }
    }
    if !tick0_notes_done {
        if let Some(ts) = &time_signature {
            events.push((0, ts.clone()));
        }
    }
    if let Some(ts) = time_signature {
        events.push((pattern.length, ts));
    }
    events.sort_by_key(|(tick, _)| *tick);
    let end = events.last().map_or(0, |(t, _)| *t).max(pattern.length);

    let mut track = Vec::new();
    let mut now = 0;
    // Running status, as the app's MIDI writer uses: a channel message repeating the
    // previous event's status byte omits it. Meta events break the run.
    let mut last_status = None;
    for (tick, bytes) in events {
        write_varlen(&mut track, tick - now);
        let status = bytes[0];
        if status < 0xF0 && last_status == Some(status) {
            track.extend_from_slice(&bytes[1..]);
        } else {
            track.extend_from_slice(&bytes);
        }
        last_status = Some(status);
        now = tick;
    }
    write_varlen(&mut track, end - now);
    track.extend_from_slice(&[0xFF, 0x2F, 0x00]);

    let mut out = Vec::with_capacity(22 + track.len());
    out.extend_from_slice(b"MThd");
    out.extend_from_slice(&6u32.to_be_bytes());
    out.extend_from_slice(&0u16.to_be_bytes());
    out.extend_from_slice(&1u16.to_be_bytes());
    out.extend_from_slice(&PPQ.to_be_bytes());
    out.extend_from_slice(b"MTrk");
    out.extend_from_slice(&(track.len() as u32).to_be_bytes());
    out.extend_from_slice(&track);
    out
}

fn meta(kind: u8, data: &[u8]) -> Vec<u8> {
    let mut m = vec![0xFF, kind, data.len() as u8];
    m.extend_from_slice(data);
    m
}

fn write_varlen(out: &mut Vec<u8>, mut value: u32) {
    let mut buf = [0u8; 5];
    let mut i = buf.len() - 1;
    buf[i] = (value & 0x7F) as u8;
    value >>= 7;
    while value > 0 {
        i -= 1;
        buf[i] = 0x80 | (value & 0x7F) as u8;
        value >>= 7;
    }
    out.extend_from_slice(&buf[i..]);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pattern(records: &[[u8; 8]]) -> Pattern {
        Pattern::parse(&records.concat()).unwrap()
    }

    #[test]
    fn varlen() {
        let mut v = Vec::new();
        for n in [0, 0x7F, 0x80, 7680, 0x0FFF_FFFF] {
            write_varlen(&mut v, n);
        }
        assert_eq!(
            v,
            [0x00, 0x7F, 0x81, 0x00, 0xBC, 0x00, 0xFF, 0xFF, 0xFF, 0x7F]
        );
    }

    #[test]
    fn running_status_within_a_channel() {
        // D13 and D14 together at tick 0, gate 5, pattern length 10.
        let p = pattern(&[
            [0x00, 0x6B, 0, 0, 0x7F, 0x40, 0x05, 0],
            [0x0A, 0x6C, 0, 0, 0x7F, 0x40, 0x05, 0],
            [0, 0x8C, 0, 0, 0, 0, 0, 0],
            [0xFF, 0, 0, 0, 0x07, 0, 0, 1],
        ]);
        let smf = from_pattern(&p, 12000);
        let expected: Vec<u8> = [
            &[0x00, 0xFF, 0x51, 0x03, 0x07, 0xA1, 0x20][..], // 500000 µs = 120 BPM
            &[0x00, 0x93, 36, 0x7F],                         // D13 on
            &[0x00, 37, 0x7F],                               // D14 on, running status
            &[0x00, 0xFF, 0x58, 0x04, 4, 2, 1, 0x60],        // code 7 falls back to 4/4
            &[0x05, 0x83, 36, 0x40],
            &[0x00, 37, 0x40], // running status again
            &[0x05, 0xFF, 0x58, 0x04, 4, 2, 1, 0x60],
            &[0x00, 0xFF, 0x2F, 0x00],
        ]
        .concat();
        assert_eq!(&smf[22..], &expected[..]);
    }

    #[test]
    fn follows_app_export_rules() {
        // D13 kick at 0 (plain), C8 pitched at 480, end at 960, 4/4.
        let p = pattern(&[
            [0xFF, 0x6B, 0, 0, 0x7F, 0x40, 0x23, 0],
            [0xE1, 0x80, 0, 0, 0, 0, 0, 0],
            [0xFF, 0x56, 0, 0x8D, 0x7F, 0x40, 0x10, 0],
            [0xE1, 0x80, 0, 0, 0, 0, 0, 0],
            [0, 0x8C, 0, 0, 0, 0, 0, 0],
            [4, 0, 0, 0, 0, 0x80, 4, 1],
        ]);
        assert_eq!(p.length, 960);
        let smf = from_pattern(&p, 9000);
        assert_eq!(&smf[..14], b"MThd\0\0\0\x06\0\0\0\x01\x01\xE0");
        let track = &smf[22..];
        let expected: Vec<u8> = [
            &[0x00, 0xFF, 0x51, 0x03, 0x0A, 0x2C, 0x2A][..], // 666666 µs = 90 BPM
            &[0x00, 0x93, 36, 0x7F],                         // D13 → channel 4, note 36
            &[0x00, 0xFF, 0x58, 0x04, 4, 2, 1, 0x60],        // time signature after tick-0 notes
            &[0x23, 0x83, 36, 0x40],                         // note off at 35
            &[0x83, 0x3D, 0xA2, 47, 77],                     // tick 480: C8 aftertouch, v = 64 + 13
            &[0x00, 0x92, 47, 0x7F],                         // C8 = channel 3, pad 8 → note 36 + 11
            &[0x10, 0xA2, 47, 77],
            &[0x00, 0x82, 47, 0x40],
            &[0x83, 0x50, 0xFF, 0x58, 0x04, 4, 2, 1, 0x60], // at 960
            &[0x00, 0xFF, 0x2F, 0x00],
        ]
        .concat();
        assert_eq!(track, &expected[..]);
        assert_eq!(
            u32::from_be_bytes(smf[18..22].try_into().unwrap()) as usize,
            expected.len()
        );
    }
}
