//! Pad addressing and the pad parameter block.

use std::fmt;
use std::str::FromStr;

use crate::ProtoError;

pub const BANKS: u16 = 10;
pub const PADS_PER_BANK: u16 = 16;
pub const PAD_COUNT: u16 = BANKS * PADS_PER_BANK;

/// A pad as the device indexes it: bank × 16 + pad, with A1 = 0 and J16 = 159.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PadIndex(u16);

impl PadIndex {
    pub fn new(index: u16) -> Option<Self> {
        (index < PAD_COUNT).then_some(PadIndex(index))
    }

    /// `bank` 0..=9 (A..J), `pad` 0..=15.
    pub fn from_bank_pad(bank: u16, pad: u16) -> Option<Self> {
        (bank < BANKS && pad < PADS_PER_BANK).then_some(PadIndex(bank * PADS_PER_BANK + pad))
    }

    pub fn index(self) -> u16 {
        self.0
    }

    pub fn bank(self) -> u16 {
        self.0 / PADS_PER_BANK
    }

    pub fn pad(self) -> u16 {
        self.0 % PADS_PER_BANK
    }

    pub fn all() -> impl Iterator<Item = PadIndex> {
        (0..PAD_COUNT).map(PadIndex)
    }

    /// Card-relative path of this pad's sample in a project (1-based project number).
    pub fn sample_path(self, project: u8) -> String {
        format!(
            "ROLAND/SP-404MKII/PROJECT_{project:02}/SMPL/BANK{}-{:02}.SMP",
            self.bank() + 1,
            self.pad() + 1
        )
    }
}

impl fmt::Display for PadIndex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}{}",
            (b'A' + self.bank() as u8) as char,
            self.pad() + 1
        )
    }
}

impl FromStr for PadIndex {
    type Err = ProtoError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let err = || ProtoError::BadPad(s.to_string());
        let mut chars = s.trim().chars();
        let bank = chars.next().ok_or_else(err)?.to_ascii_uppercase();
        if !('A'..='J').contains(&bank) {
            return Err(err());
        }
        let pad: u16 = chars.as_str().parse().map_err(|_| err())?;
        if !(1..=PADS_PER_BANK).contains(&pad) {
            return Err(err());
        }
        Ok(PadIndex(
            (bank as u16 - 'A' as u16) * PADS_PER_BANK + pad - 1,
        ))
    }
}

/// Pad parameter block (`1E pad …` on channel 5).
///
/// After the 3-byte prefix the block is little-endian u32 fields; see
/// `docs/re/03-parameters.md` for the layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PadBlock {
    pub pad: PadIndex,
    raw: Vec<u8>,
}

impl PadBlock {
    const PREFIX: usize = 3;
    const NAME_OFFSET: usize = 175;
    const NAME_LEN: usize = 24;
    const CHOP_OFFSET: usize = 199;
    pub const CHOP_POINTS: usize = 16;
    /// Bytes the official app sends when writing a block (everything up to the name).
    pub const WRITE_LEN: usize = Self::CHOP_OFFSET;

    const ABS_END: usize = 0;
    const START: usize = 1;
    const END: usize = 2;
    /// Loop start as a byte offset (the device moves it along when start passes it).
    const LOOP_START: usize = 11;

    /// Parse a `1E` payload (including its command byte).
    pub fn parse(payload: &[u8]) -> Result<Self, ProtoError> {
        let need = Self::CHOP_OFFSET + Self::CHOP_POINTS * 4;
        if payload.len() < need {
            return Err(ProtoError::Truncated {
                need,
                have: payload.len(),
            });
        }
        if payload[0] != crate::control::reply::PAD_BLOCK {
            return Err(ProtoError::UnexpectedCommand(payload[0]));
        }
        let index = u16::from_le_bytes([payload[1], payload[2]]);
        let pad = PadIndex::new(index).ok_or_else(|| ProtoError::BadPad(index.to_string()))?;
        Ok(PadBlock {
            pad,
            raw: payload.to_vec(),
        })
    }

    /// Raw u32 field `n` (0-based, counted from the end of the prefix).
    pub fn field(&self, n: usize) -> u32 {
        let at = Self::PREFIX + n * 4;
        u32::from_le_bytes(self.raw[at..at + 4].try_into().unwrap())
    }

    pub fn set_field(&mut self, n: usize, value: u32) {
        let at = Self::PREFIX + n * 4;
        self.raw[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }

    /// Whether the pad holds a sample.
    pub fn has_sample(&self) -> bool {
        self.field(Self::ABS_END) != 0
    }

    /// Size of the SMP file in bytes.
    pub fn file_size(&self) -> u32 {
        self.field(Self::ABS_END)
    }

    /// Start and end as byte offsets into the SMP file.
    pub fn start_end_bytes(&self) -> (u32, u32) {
        (self.field(Self::START), self.field(Self::END))
    }

    /// Parameter value by set-parameter id, for ids whose position is verified.
    pub fn param(&self, id: u8) -> Option<i32> {
        crate::params::block_field(id).map(|n| self.field(n) as i32)
    }

    pub fn name(&self) -> String {
        let bytes = &self.raw[Self::NAME_OFFSET..Self::NAME_OFFSET + Self::NAME_LEN];
        let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
        String::from_utf8_lossy(&bytes[..end])
            .trim_end()
            .to_string()
    }

    pub fn set_name(&mut self, name: &str) {
        let field = &mut self.raw[Self::NAME_OFFSET..Self::NAME_OFFSET + Self::NAME_LEN];
        field.fill(0);
        let bytes: Vec<u8> = name
            .bytes()
            .filter(|b| (0x20..0x7f).contains(b))
            .take(Self::NAME_LEN - 1)
            .collect();
        field[..bytes.len()].copy_from_slice(&bytes);
    }

    /// Chop points in sample frames; `None` for unused slots.
    pub fn chop_points(&self) -> Vec<Option<u32>> {
        (0..Self::CHOP_POINTS)
            .map(|i| {
                let at = Self::CHOP_OFFSET + i * 4;
                let v = i32::from_le_bytes(self.raw[at..at + 4].try_into().unwrap());
                (v >= 0).then_some(v as u32)
            })
            .collect()
    }

    /// Point the block at a freshly written SMP file of `file_size` bytes, as the official
    /// app does on import.
    pub fn assign_sample(&mut self, file_size: u32, name: &str) {
        self.set_field(Self::ABS_END, file_size);
        self.set_field(Self::START, crate::control::SMP_HEADER_LEN);
        self.set_field(Self::END, file_size);
        self.set_field(Self::LOOP_START, crate::control::SMP_HEADER_LEN);
        self.set_name(name);
    }

    /// The whole reply payload as received.
    pub fn raw(&self) -> &[u8] {
        &self.raw
    }

    /// Loop start as a byte offset into the SMP file.
    pub fn loop_start_bytes(&self) -> u32 {
        self.field(Self::LOOP_START)
    }

    /// Payload to send when writing the block back to the device.
    pub fn write_payload(&self) -> Vec<u8> {
        self.raw[..Self::WRITE_LEN].to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pad_names() {
        assert_eq!("A1".parse::<PadIndex>().unwrap().index(), 0);
        assert_eq!("b1".parse::<PadIndex>().unwrap().index(), 16);
        assert_eq!("J16".parse::<PadIndex>().unwrap().index(), 159);
        assert!("K1".parse::<PadIndex>().is_err());
        assert!("A17".parse::<PadIndex>().is_err());
        assert_eq!(PadIndex::new(159).unwrap().to_string(), "J16");
        assert_eq!(
            PadIndex::new(16).unwrap().sample_path(6),
            "ROLAND/SP-404MKII/PROJECT_06/SMPL/BANK2-01.SMP"
        );
    }

    fn empty_block(pad: u16) -> Vec<u8> {
        let mut p = vec![0u8; 327];
        p[0] = 0x1E;
        p[1..3].copy_from_slice(&pad.to_le_bytes());
        p[175..198].fill(b' ');
        p[199..263].fill(0xff);
        p
    }

    #[test]
    fn block_fields_and_import_patch() {
        let mut block = PadBlock::parse(&empty_block(159)).unwrap();
        assert!(!block.has_sample());
        assert_eq!(block.name(), "");
        assert_eq!(block.chop_points(), vec![None; 16]);

        block.assign_sample(576_512, "spmk2_test_48k_stereo");
        assert_eq!(block.start_end_bytes(), (512, 576_512));
        assert_eq!(block.name(), "spmk2_test_48k_stereo");
        let out = block.write_payload();
        assert_eq!(out.len(), 199);
        assert_eq!(&out[..3], &[0x1E, 0x9F, 0x00]);
        assert_eq!(
            &out[3..15],
            &[
                0x00, 0xcc, 0x08, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0xcc, 0x08, 0x00
            ]
        );
    }
}
