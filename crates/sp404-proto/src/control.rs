//! Channel 5: control, parameters and pad operations.
//!
//! A payload is a command byte followed by little-endian arguments. Requests often set
//! bit 7 of the command; replies clear it. Short messages carry up to 7 bytes; most short
//! replies are `cmd status` with status 0 on success.

use crate::pad::PadIndex;
use crate::params::Target;

pub const REQUEST_BIT: u8 = 0x80;
pub const SMP_HEADER_LEN: u32 = 512;

/// Commands sent without the request bit and echoed back unchanged.
pub mod cmd {
    pub const SET_PARAM: u8 = 0x01;
    pub const SAMPLE_NAME: u8 = 0x02;
    pub const PROJECT_NAME: u8 = 0x35;
    pub const PAD_BLOCK: u8 = 0x1E;
}

/// Reply command bytes (request command with bit 7 cleared).
pub mod reply {
    pub const PAD_BLOCK: u8 = 0x1E;
    pub const PATTERN_EXISTS: u8 = 0x2B;
    pub const PROJECT_NAME: u8 = 0x35;
    pub const STATUS: u8 = 0x7E;
    pub const PROJECT_SETTINGS: u8 = 0x7D;
    pub const PEAKS: u8 = 0x1C;
    pub const PAD_SLOT: u8 = 0x11;
    pub const PAD_COMMIT: u8 = 0x1F;
    pub const STOP: u8 = 0x1D;
    pub const TRUNCATE: u8 = 0x18;
    pub const NORMALIZE: u8 = 0x23;
    pub const PREVIEW_START: u8 = 0x0E;
    pub const PREVIEW_STOP: u8 = 0x0F;
    pub const MOVE_SAMPLE: u8 = 0x15;
    /// Unsolicited error report: `32 code:i32` (see [`super::error_message`]).
    pub const ERROR: u8 = 0x32;
    /// Answer to Init (`92`) and the first step of a restore.
    pub const INIT: u8 = 0x12;
    /// Short `1A 00` answers a project select. Also sent unsolicited as `1A proj 00 00 00`
    /// when project data changed on the device.
    pub const CHANGED: u8 = 0x1A;
}

/// Message table the official app shows for device errors, in its order.
const ERROR_MESSAGES: [&str; 46] = [
    "success",
    "error",
    "media full",
    "media protected",
    "media error",
    "unsupported format",
    "media unformatted",
    "media damaged",
    "media ejected",
    "media busy",
    "recording too long",
    "invalid format",
    "invalid path",
    "cancelled",
    "not found",
    "open error",
    "read error",
    "write error",
    "duplicate name",
    "no empty pad",
    "invalid filename",
    "already installed",
    "update wave info error",
    "update exp info error",
    "exp check wave error",
    "exp check exp error",
    "exp slot full",
    "memory full",
    "memory fragmentation",
    "hash check error",
    "error",
    "sample too short",
    "license error",
    "sample too long",
    "keyboard sample full",
    "no chop points",
    "BPM detect error",
    "key detect error",
    "bank protected",
    "one or more banks protected",
    "some import errors",
    "error",
    "import error",
    "sampling error",
    "BPM detect error",
    "process completed",
];

/// Text for a device error code from a `32 code:i32` message, as the official app maps it:
/// −128..=−90 index the message table at `code + 130`, −1 is a generic error.
pub fn error_message(code: i32) -> Option<&'static str> {
    match code {
        -128..=-90 => Some(ERROR_MESSAGES[(code + 130) as usize]),
        -1 => Some(ERROR_MESSAGES[1]),
        _ => None,
    }
}

/// Parse an unsolicited device error message `32 code:i32`.
pub fn parse_error(payload: &[u8]) -> Option<i32> {
    match payload {
        [reply::ERROR, a, b, c, d, ..] => Some(i32::from_le_bytes([*a, *b, *c, *d])),
        _ => None,
    }
}

fn with_pad(command: u8, pad: PadIndex, extra: &[u8]) -> Vec<u8> {
    let mut p = vec![command];
    p.extend_from_slice(&pad.index().to_le_bytes());
    p.extend_from_slice(extra);
    p
}

pub fn set_param(id: u8, target: Target, value: i32) -> Vec<u8> {
    let mut p = vec![cmd::SET_PARAM, id, 0];
    p.extend_from_slice(&target.to_u16().to_le_bytes());
    p.extend_from_slice(&value.to_le_bytes());
    p
}

// ---- short requests ----

pub fn status_request() -> Vec<u8> {
    vec![0xFE, 0x66, 0x00]
}

/// Settings of `project` (0-based). Reply: long `7D proj 00 …`, followed by 16 long
/// `35 nn name` project names.
pub fn project_settings_request(project: u8) -> Vec<u8> {
    vec![0xFD, project, 0x00]
}

/// Make `project` (0-based) the current project. Reply: short `1A 00`.
pub fn select_project(project: u8) -> Vec<u8> {
    vec![0x9A, project, 0x00]
}

/// Leave the remote screen on the device ("MKII EXIT"). No reply.
pub fn mkii_exit() -> Vec<u8> {
    vec![0x3A, 0x00, 0x00, 0x00]
}

/// Start playing a pad, as the app's Preview button does on mouse down. Reply `0E pad 00`.
pub fn preview_start(pad: PadIndex) -> Vec<u8> {
    with_pad(0x8E, pad, &[0x7F, 0xFF])
}

/// Stop a preview (mouse up). Reply `0F pad 00`.
pub fn preview_stop(pad: PadIndex) -> Vec<u8> {
    with_pad(0x8F, pad, &[0x00])
}

/// How [`move_sample`] treats the destination pad.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoveMode {
    /// Replace the destination; the source pad becomes empty (the app's "Overwrite").
    Overwrite = 0,
    /// Swap the two pads.
    Exchange = 1,
}

/// Move or exchange a pad's sample. Reply `15 00`, then a `1A proj` notification.
pub fn move_sample(from: PadIndex, to: PadIndex, mode: MoveMode) -> Vec<u8> {
    let mut p = with_pad(0x95, from, &to.index().to_le_bytes());
    p.extend_from_slice(&(mode as u16).to_le_bytes());
    p
}

pub fn pad_block_request(pad: PadIndex) -> Vec<u8> {
    with_pad(0x9E, pad, &[])
}

pub fn pattern_exists_request(slot: PadIndex) -> Vec<u8> {
    with_pad(0xAB, slot, &[])
}

/// Sent by the official app on Disconnect and before an import (reply `1D 00`).
pub fn stop() -> Vec<u8> {
    vec![0x9D]
}

// ---- long requests ----

/// Waveform peaks: `points` (min, max) pairs, each over `samples_per_point` frames.
pub fn peaks_request(
    pad: PadIndex,
    channel: u8,
    start_frame: u32,
    points: u32,
    samples_per_point: u32,
) -> Vec<u8> {
    let mut p = vec![0x9C, 0x05];
    p.extend_from_slice(&pad.index().to_le_bytes());
    p.push(channel);
    p.extend_from_slice(&start_frame.to_le_bytes());
    p.extend_from_slice(&points.to_le_bytes());
    p.extend_from_slice(&samples_per_point.to_le_bytes());
    p
}

/// Parse a peaks reply into (min, max) pairs.
pub fn parse_peaks(payload: &[u8]) -> Vec<(i16, i16)> {
    payload
        .get(16..)
        .unwrap_or_default()
        .chunks_exact(4)
        .map(|c| {
            (
                i16::from_le_bytes([c[0], c[1]]),
                i16::from_le_bytes([c[2], c[3]]),
            )
        })
        .collect()
}

pub fn sample_name(pad: PadIndex, name: &str) -> Vec<u8> {
    let mut p = with_pad(cmd::SAMPLE_NAME, pad, &[]);
    p.extend(name.bytes().filter(|b| (0x20..0x7f).contains(b)).take(23));
    p
}

/// `project` is 0-based (0 = project 1).
pub fn project_name(project: u8, name: &str) -> Vec<u8> {
    let mut p = vec![cmd::PROJECT_NAME, project];
    p.extend(name.bytes().filter(|b| (0x20..0x7f).contains(b)));
    p
}

/// Parse a `35 nn name` project-name reply.
pub fn parse_project_name(payload: &[u8]) -> Option<(u8, String)> {
    if payload.first() != Some(&reply::PROJECT_NAME) || payload.len() < 2 {
        return None;
    }
    let name = String::from_utf8_lossy(&payload[2..])
        .trim_matches([' ', '\0'])
        .to_string();
    Some((payload[1], name))
}

/// Device-side operations on one pad's sample.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PadOp {
    Truncate,
    Normalize,
    Delete,
}

impl PadOp {
    pub fn payload(self, pad: PadIndex) -> Vec<u8> {
        match self {
            PadOp::Truncate => with_pad(0x98, pad, &[]),
            PadOp::Normalize => with_pad(0xA3, pad, &[]),
            PadOp::Delete => with_pad(0x91, pad, &[0x00, 0x00]),
        }
    }

    pub fn reply(self) -> u8 {
        match self {
            PadOp::Truncate => reply::TRUNCATE,
            PadOp::Normalize => reply::NORMALIZE,
            PadOp::Delete => reply::PAD_SLOT,
        }
    }

    /// Whether the official app brackets the operation with [`edit_begin`] / [`edit_end`].
    pub fn bracketed(self) -> bool {
        matches!(self, PadOp::Normalize)
    }
}

pub fn edit_begin() -> Vec<u8> {
    vec![0xFB, 0x00, 0x00]
}

pub fn edit_end() -> Vec<u8> {
    vec![0xFC, 0x00, 0x00]
}

/// Pattern rendering on the device (Bounce and MULTIPAD export). The device writes the
/// WAV into a host file handle through [`crate::fileapi::HostRequest`]s.
pub mod render {
    use super::*;

    pub const BEGIN: u16 = 1000;
    pub const END: u16 = 1001;
    pub const BOUNCE: u16 = 1003;
    /// Result code of a failed render.
    pub const FAILED: u16 = 1002;

    pub const PADS_USED_REPLY: u8 = 0x37;
    pub const CONTROL_REPLY: u8 = 0x38;
    pub const DONE: u8 = 0x39;

    /// Pads used by a pattern. Reply: long `37 ? count nbytes bitmap`.
    pub fn pads_used_request(pattern: PadIndex) -> Vec<u8> {
        with_pad(0xB7, pattern, &[])
    }

    pub fn parse_pads_used(payload: &[u8]) -> Option<Vec<PadIndex>> {
        if payload.first() != Some(&PADS_USED_REPLY) || payload.len() < 4 {
            return None;
        }
        let bitmap = payload.get(4..4 + usize::from(payload[3]))?;
        Some(
            PadIndex::all()
                .filter(|p| {
                    let i = usize::from(p.index());
                    bitmap.get(i / 8).is_some_and(|b| b & (1 << (i % 8)) != 0)
                })
                .collect(),
        )
    }

    /// `op`: [`BEGIN`], [`END`], [`BOUNCE`], or a pad index 0..=159 (MULTIPAD).
    /// Reply: `38 pattern op`.
    pub fn control(pattern: PadIndex, op: u16, handle: u16) -> Vec<u8> {
        let mut p = with_pad(0xB8, pattern, &op.to_le_bytes());
        p.extend_from_slice(&handle.to_le_bytes());
        p
    }

    /// Parse the device's `39 pattern op handle result` completion message.
    pub fn parse_done(payload: &[u8]) -> Option<(u16, u16, u16, u16)> {
        if payload.first() != Some(&DONE) || payload.len() < 9 {
            return None;
        }
        let w = |at: usize| u16::from_le_bytes([payload[at], payload[at + 1]]);
        Some((w(1), w(3), w(5), w(7)))
    }
}

/// What Init clears in the **current** project.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InitScope {
    All,
    AllSamples,
    /// Samples of one bank, 0..=9 (A..J).
    SamplesBank(u8),
    AllPatterns,
    /// Patterns of one bank, 0..=9 (A..J).
    PatternsBank(u8),
}

/// Init: `92 mode:u16 option:u16 bank:u16`. Erases data of the current project, in memory
/// and on the card. Reply `12 status`, then a `1A proj` notification.
pub fn init_project(scope: InitScope) -> Vec<u8> {
    let (option, bank) = match scope {
        InitScope::All => (0u16, 0u16),
        InitScope::AllSamples => (1, 0),
        InitScope::SamplesBank(b) => (2, u16::from(b)),
        InitScope::AllPatterns => (3, 0),
        InitScope::PatternsBank(b) => (4, u16::from(b)),
    };
    init_request(0, option, bank)
}

fn init_request(mode: u16, option: u16, bank: u16) -> Vec<u8> {
    let mut p = vec![0x92];
    for v in [mode, option, bank] {
        p.extend_from_slice(&v.to_le_bytes());
    }
    p
}

/// Restoring a whole project from a PC backup (the app's "Import to MKII").
pub mod restore {
    /// Sent before any file is written: Init in import mode (100). **Erases the current
    /// project**, in memory and on the card. Reply `12 status` after about a second.
    pub fn prepare() -> Vec<u8> {
        super::init_request(100, 0, 0)
    }

    pub const PREPARE_REPLY: u8 = super::reply::INIT;

    /// Sent after the files are written: reload `project` (0-based) from the card.
    /// Reply `31 status`, later an unsolicited `2C 00 00 00 00`.
    pub fn reload(project: u8) -> Vec<u8> {
        vec![0xB1, project, 0x00]
    }

    pub const RELOAD_REPLY: u8 = 0x31;
    pub const RELOADED: u8 = 0x2C;
}

/// The short messages the official app sends around a sample import, in order.
pub mod import {
    use super::*;

    pub fn begin() -> Vec<u8> {
        vec![0xFB, 0xA0, 0x00]
    }

    pub fn release_slot(pad: PadIndex) -> Vec<u8> {
        with_pad(0x91, pad, &[0x01, 0x00])
    }

    pub fn commit(pad: PadIndex) -> Vec<u8> {
        with_pad(0x9F, pad, &[])
    }

    pub fn end() -> Vec<u8> {
        edit_end()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_payloads() {
        let b1: PadIndex = "B1".parse().unwrap();
        assert_eq!(
            set_param(0x69, Target::Pad(b1), 100),
            vec![0x01, 0x69, 0x00, 0x10, 0x00, 0x64, 0, 0, 0]
        );
        assert_eq!(
            set_param(0x2A, Target::Global, 12000),
            vec![0x01, 0x2A, 0x00, 0xFF, 0xFF, 0xE0, 0x2E, 0, 0]
        );
        assert_eq!(pad_block_request(b1), vec![0x9E, 0x10, 0x00]);
        assert_eq!(project_settings_request(11), vec![0xFD, 0x0B, 0x00]);
        let d13: PadIndex = "D13".parse().unwrap();
        assert_eq!(preview_start(d13), vec![0x8E, 0x3C, 0x00, 0x7F, 0xFF]);
        assert_eq!(preview_stop(d13), vec![0x8F, 0x3C, 0x00, 0x00]);
        assert_eq!(
            peaks_request(b1, 1, 0x2000, 32, 256),
            vec![
                0x9C, 0x05, 0x10, 0x00, 0x01, 0x00, 0x20, 0, 0, 0x20, 0, 0, 0, 0x00, 0x01, 0, 0
            ]
        );
        let j16: PadIndex = "J16".parse().unwrap();
        assert_eq!(
            PadOp::Delete.payload(j16),
            vec![0x91, 0x9F, 0x00, 0x00, 0x00]
        );
        assert_eq!(
            import::release_slot(j16),
            vec![0x91, 0x9F, 0x00, 0x01, 0x00]
        );
        assert_eq!(
            project_name(5, "PROJECT_06"),
            b"\x35\x05PROJECT_06".to_vec()
        );
        assert_eq!(sample_name(j16, "TEST"), b"\x02\x9f\x00TEST".to_vec());
        let j15: PadIndex = "J15".parse().unwrap();
        assert_eq!(
            move_sample(j16, j15, MoveMode::Overwrite),
            vec![0x95, 0x9F, 0x00, 0x9E, 0x00, 0x00, 0x00]
        );
        let (b2, b3) = ("B2".parse().unwrap(), "B3".parse().unwrap());
        assert_eq!(
            move_sample(b2, b3, MoveMode::Exchange),
            vec![0x95, 0x11, 0x00, 0x12, 0x00, 0x01, 0x00]
        );
        assert_eq!(
            init_project(InitScope::SamplesBank(2)),
            vec![0x92, 0x00, 0x00, 0x02, 0x00, 0x02, 0x00]
        );
        assert_eq!(
            init_project(InitScope::PatternsBank(2)),
            vec![0x92, 0x00, 0x00, 0x04, 0x00, 0x02, 0x00]
        );
        assert_eq!(
            restore::prepare(),
            vec![0x92, 0x64, 0x00, 0x00, 0x00, 0x00, 0x00]
        );
        assert_eq!(parse_error(&[0x32, 0x89, 0xFF, 0xFF, 0xFF]), Some(-119));
        assert_eq!(error_message(-119), Some("invalid format"));
        assert_eq!(error_message(-92), Some("bank protected"));
        assert_eq!(error_message(-111), Some("no empty pad"));
        assert_eq!(error_message(-5), None);
    }

    #[test]
    fn render_messages_from_capture() {
        let a1 = PadIndex::new(0).unwrap();
        assert_eq!(
            render::control(a1, render::BOUNCE, 0),
            vec![0xB8, 0x00, 0x00, 0xEB, 0x03, 0x00, 0x00]
        );
        let mut used = vec![0x37, 0x00, 0x01, 0x14];
        used.extend_from_slice(&[0, 0, 0, 0, 0x80]);
        used.resize(4 + 20, 0);
        assert_eq!(
            render::parse_pads_used(&used),
            Some(vec!["C8".parse().unwrap()])
        );
        assert_eq!(
            render::parse_done(&[0x39, 0, 0, 0xEB, 0x03, 0, 0, 0, 0]),
            Some((0, render::BOUNCE, 0, 0))
        );
    }

    #[test]
    fn parses_project_name() {
        assert_eq!(
            parse_project_name(b"\x35\x02PROJECT_03"),
            Some((2, "PROJECT_03".into()))
        );
        assert_eq!(
            parse_project_name(b"\x35\x00      "),
            Some((0, String::new()))
        );
    }
}
