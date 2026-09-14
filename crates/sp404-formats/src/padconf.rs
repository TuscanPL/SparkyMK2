//! `PADCONF.BIN`: a project's settings and all 160 pad parameter blocks.
//!
//! The file stores the same structures the device sends over channel 5 (the `7D` project
//! settings and `1E` pad blocks), as big-endian words instead of little-endian ones. The
//! device writes it as a snapshot, so it can lag the live state.

use sp404_proto::pad::{PAD_COUNT, PadBlock};
use sp404_proto::{PadIndex, ProjectSettings};

use crate::FormatError;

pub const LEN: usize = 52_000;

const MAGIC: &[u8; 4] = b"RFPD";
const SETTINGS: usize = 0x10;
const RECORDS: usize = 0xA0;
const RECORD_WORDS: usize = 43;
const NAMES: usize = 0x6C20;
const NAME_LEN: usize = 24;
const CHOPS: usize = 0x7B20;
/// 16 chop points and 16 reserved words per pad.
const CHOP_WORDS: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Padconf {
    settings: ProjectSettings,
    pads: Vec<PadBlock>,
}

impl Padconf {
    pub fn parse(bytes: &[u8]) -> Result<Self, FormatError> {
        if bytes.len() < 4 || &bytes[..4] != MAGIC {
            return Err(FormatError::BadMagic);
        }
        if bytes.len() < LEN {
            return Err(FormatError::TooShort(bytes.len()));
        }

        let words = ProjectSettings::WORDS;
        let mut settings = vec![sp404_proto::control::reply::PROJECT_SETTINGS, 0x05, 0x00];
        swap_words(&bytes[SETTINGS..SETTINGS + words * 4], &mut settings);
        let name_at = SETTINGS + words * 4;
        settings.extend_from_slice(&bytes[name_at..name_at + ProjectSettings::NAME_LEN]);
        let settings = ProjectSettings::parse(&settings)?;

        let pads = PadIndex::all()
            .map(|pad| {
                let i = usize::from(pad.index());
                let mut p = vec![sp404_proto::control::reply::PAD_BLOCK];
                p.extend_from_slice(&pad.index().to_le_bytes());
                let record = RECORDS + i * RECORD_WORDS * 4;
                swap_words(&bytes[record..record + RECORD_WORDS * 4], &mut p);
                let name = NAMES + i * NAME_LEN;
                p.extend_from_slice(&bytes[name..name + NAME_LEN]);
                let chops = CHOPS + i * CHOP_WORDS * 4;
                swap_words(&bytes[chops..chops + CHOP_WORDS * 4], &mut p);
                PadBlock::parse(&p)
            })
            .collect::<Result<Vec<_>, _>>()?;
        debug_assert_eq!(pads.len(), usize::from(PAD_COUNT));
        Ok(Padconf { settings, pads })
    }

    pub fn settings(&self) -> &ProjectSettings {
        &self.settings
    }

    pub fn pad(&self, pad: PadIndex) -> &PadBlock {
        &self.pads[usize::from(pad.index())]
    }

    pub fn pads(&self) -> &[PadBlock] {
        &self.pads
    }
}

fn swap_words(src: &[u8], out: &mut Vec<u8>) {
    for word in src.chunks_exact(4) {
        out.extend(word.iter().rev());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put(buf: &mut [u8], at: usize, value: u32) {
        buf[at..at + 4].copy_from_slice(&value.to_be_bytes());
    }

    #[test]
    fn maps_sections_to_live_structures() {
        let mut f = vec![0u8; LEN];
        f[..4].copy_from_slice(MAGIC);
        put(&mut f, 0x04, 0xA0);
        put(&mut f, 0x0C, 0x7A80);
        put(&mut f, 0x10, 10_040);
        put(&mut f, 0x10 + 12 * 4, 9000 << 1 | 1);
        f[0x80..0x8A].copy_from_slice(b"PROJECT_06");

        // B1 (index 16): 512,000 frames, loop start moved to frame 1000, one chop point.
        let b1 = 16;
        let rec = RECORDS + b1 * 172;
        put(&mut f, rec, 2_048_512);
        put(&mut f, rec + 4, 512);
        put(&mut f, rec + 8, 2_048_512);
        put(&mut f, rec + 12, 127);
        put(&mut f, rec + 44, 4512);
        f[NAMES + b1 * 24..NAMES + b1 * 24 + 4].copy_from_slice(b"BASS");
        let chops = CHOPS + b1 * 128;
        for n in 0..16 {
            put(&mut f, chops + n * 4, u32::MAX);
        }
        put(&mut f, chops, 96_000);

        let pc = Padconf::parse(&f).unwrap();
        assert_eq!(pc.settings().project_tempo(), 10_040);
        assert!(pc.settings().bank(0).protected);
        assert_eq!(pc.settings().bank(0).tempo, 9000);
        assert_eq!(pc.settings().name(), "PROJECT_06");

        let pad = pc.pad("B1".parse().unwrap());
        assert!(pad.has_sample());
        assert_eq!(pad.start_end_bytes(), (512, 2_048_512));
        assert_eq!(pad.param(0x69), Some(127));
        assert_eq!(pad.loop_start_bytes(), 4512);
        assert_eq!(pad.name(), "BASS");
        assert_eq!(pad.chop_points()[0], Some(96_000));
        assert_eq!(pad.chop_points()[1], None);
        assert!(!pc.pad("A1".parse().unwrap()).has_sample());
    }

    #[test]
    fn rejects_other_files() {
        assert!(matches!(
            Padconf::parse(b"RFWV...."),
            Err(FormatError::BadMagic)
        ));
        assert!(matches!(
            Padconf::parse(b"RFPD"),
            Err(FormatError::TooShort(4))
        ));
    }
}
