//! Parameter ids for the set-parameter command (`01 id 00 target value`).

use crate::pad::PadIndex;

/// Target of a parameter: one pad, or the global scope (`FFFF`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Pad(PadIndex),
    Global,
}

impl Target {
    pub fn to_u16(self) -> u16 {
        match self {
            Target::Pad(p) => p.index(),
            Target::Global => 0xFFFF,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Pad,
    Global,
}

#[derive(Debug, Clone, Copy)]
pub struct ParamInfo {
    pub name: &'static str,
    pub id: u8,
    pub scope: Scope,
    pub min: i32,
    pub max: i32,
    pub help: &'static str,
}

const fn pad(name: &'static str, id: u8, min: i32, max: i32, help: &'static str) -> ParamInfo {
    ParamInfo {
        name,
        id,
        scope: Scope::Pad,
        min,
        max,
        help,
    }
}

const fn global(name: &'static str, id: u8, min: i32, max: i32, help: &'static str) -> ParamInfo {
    ParamInfo {
        name,
        id,
        scope: Scope::Global,
        min,
        max,
        help,
    }
}

/// Parameters mapped by differential captures (docs/re/03-parameters.md).
pub const PARAMS: &[ParamInfo] = &[
    pad("start", 0x67, 0, i32::MAX, "start point, sample frames"),
    pad("end", 0x68, 0, i32::MAX, "end point, sample frames"),
    pad("level", 0x69, 0, 127, ""),
    pad("gate", 0x6A, 0, 1, ""),
    pad("loop", 0x6B, 0, 1, ""),
    pad("mute-group", 0x6D, 0, 10, "0 = off"),
    pad("bpm-sync", 0x6E, 0, 1, ""),
    pad("bpm", 0x6F, 4000, 20000, "BPM x 100"),
    pad(
        "mode-flags",
        0x70,
        0,
        255,
        "bit0 fixed velocity, bits 3/4 chromatic mode, bit5 one shot",
    ),
    pad("loop-top", 0x71, 0, i32::MAX, "loop top, sample frames"),
    pad("pitch-coarse", 0x73, -12, 12, "semitones (Vinyl off)"),
    pad("pitch-fine", 0x74, -100, 100, "cents (Vinyl off)"),
    pad(
        "play-mode",
        0x75,
        0,
        3,
        "0 forward, 1 reverse, 2 fwd ping-pong, 3 rev ping-pong",
    ),
    pad("time-stretch", 0x76, 5000, 15000, "percent x 100"),
    pad("vinyl", 0x77, 0, 1, ""),
    pad("balance", 0x78, 13, 115, "64 = centre"),
    pad("pad-link", 0x79, 0, 10, "0 = off"),
    pad("bus-fx", 0x7A, 0, 2, "1 = BUS 1"),
    pad("roll", 0x7B, 0, 10, "2 = 1/4"),
    pad("attack", 0x7D, 0, 127, ""),
    pad("hold", 0x7E, 1, 100, ""),
    pad("release", 0x7F, 0, 127, ""),
    pad("groove", 0x8A, 0, 8, "0 = off"),
    pad("rate", 0x8B, -7, 7, "0 = default"),
    pad("humanize", 0x8C, 0, 3, "0 = off"),
    global("selected-bank", 0x00, 0, 9, "UI sync"),
    global("selected-pad", 0x01, 0, 15, "UI sync"),
    global("project-tempo", 0x0A, 4000, 20000, "BPM x 100"),
    global("tempo-select", 0x0B, 0, 1, "0 bank, 1 project"),
];

pub const CHOP_POINT_FIRST: u8 = 0x8D;
pub const BANK_PROTECT_BASE: u8 = 0x17;
pub const BANK_TEMPO_BASE: u8 = 0x21;
pub const BANK_VOLUME_BASE: u8 = 0x35;

pub fn by_name(name: &str) -> Option<ParamInfo> {
    PARAMS
        .iter()
        .copied()
        .find(|p| p.name.eq_ignore_ascii_case(name))
}

/// Bank-level global parameters: `bank-protect-a`, `bank-tempo-j`, `bank-volume-c`.
/// Returns the id and whether the value is stored inverted (127 − value).
pub fn bank_param(name: &str) -> Option<(u8, bool)> {
    let name = name.to_ascii_lowercase();
    let (kind, bank) = name.rsplit_once('-')?;
    let bank = bank
        .chars()
        .next()
        .filter(|c| ('a'..='j').contains(c) && bank.len() == 1)? as u8
        - b'a';
    match kind {
        "bank-protect" => Some((BANK_PROTECT_BASE + bank, false)),
        "bank-tempo" => Some((BANK_TEMPO_BASE + bank, false)),
        "bank-volume" => Some((BANK_VOLUME_BASE + bank, true)),
        _ => None,
    }
}

/// Position of a parameter inside the pad block, for ids verified against known values.
pub(crate) fn block_field(id: u8) -> Option<usize> {
    match id {
        // Contiguous run: field n holds id 0x66 + n.
        0x67..=0x7B => Some((id - 0x66) as usize),
        // After Roll there is no field for 0x7C; Attack starts one field earlier.
        0x7D..=0x7F => Some((id - 0x67) as usize),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookups() {
        assert_eq!(by_name("Level").unwrap().id, 0x69);
        assert_eq!(bank_param("bank-tempo-j"), Some((0x2A, false)));
        assert_eq!(bank_param("bank-volume-a"), Some((0x35, true)));
        assert_eq!(bank_param("bank-tempo-k"), None);
        assert_eq!(block_field(0x69), Some(3));
        assert_eq!(block_field(0x6F), Some(9));
        assert_eq!(block_field(0x7E), Some(23));
    }
}
