//! `PADCONF.BIN`: a project's settings and all 160 pad parameter blocks.
//!
//! The file stores the same structures the device sends over channel 5 (the `7D` project
//! settings and `1E` pad blocks), as big-endian words instead of little-endian ones. The
//! device writes it as a snapshot, so it can lag the live state.

use sp404_proto::pad::{PAD_COUNT, PadBlock};
use sp404_proto::{PadIndex, ProjectSettings};

use crate::FormatError;

pub const LEN: usize = V3.len;

const MAGIC: &[u8; 4] = b"RFPD";
const VERSION: usize = 0x08;
const SETTINGS: usize = 0x10;
const RECORD_WORDS: usize = 43;
const NAME_LEN: usize = 24;
/// 16 chop points and 16 reserved words per pad.
const CHOP_WORDS: usize = 32;
const CHOP_POINTS: usize = 16;

/// Where a file version keeps its sections.
struct Layout {
    len: usize,
    /// Project name (32 bytes).
    name: Option<usize>,
    records: usize,
    names: usize,
    chops: Option<usize>,
}

const V3: Layout = Layout {
    len: 52_000,
    name: Some(0x80),
    records: 0xA0,
    names: 0x6C20,
    chops: Some(0x7B20),
};

/// Written by older firmware: no project name and no chop points, so the pad records
/// start 32 bytes earlier and the file ends after the name table.
const V2: Layout = Layout {
    len: 31_488,
    name: None,
    records: 0x80,
    names: 0x6C00,
    chops: None,
};

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
        if bytes.len() < SETTINGS {
            return Err(FormatError::TooShort(bytes.len()));
        }
        let layout = match bytes[VERSION] {
            3 => &V3,
            2 => &V2,
            version => return Err(FormatError::UnsupportedPadconf(version)),
        };
        if bytes.len() < layout.len {
            return Err(FormatError::TooShort(bytes.len()));
        }

        let words = ProjectSettings::WORDS;
        let mut settings = vec![sp404_proto::control::reply::PROJECT_SETTINGS, 0x05, 0x00];
        swap_words(&bytes[SETTINGS..SETTINGS + words * 4], &mut settings);
        match layout.name {
            Some(at) => settings.extend_from_slice(&bytes[at..at + ProjectSettings::NAME_LEN]),
            // Space padded, as the device reports a slot without a name.
            None => settings.extend_from_slice(&[b' '; ProjectSettings::NAME_LEN]),
        }
        let settings = ProjectSettings::parse(&settings)?;

        let pads = PadIndex::all()
            .map(|pad| {
                let i = usize::from(pad.index());
                let mut p = vec![sp404_proto::control::reply::PAD_BLOCK];
                p.extend_from_slice(&pad.index().to_le_bytes());
                let record = layout.records + i * RECORD_WORDS * 4;
                swap_words(&bytes[record..record + RECORD_WORDS * 4], &mut p);
                let name = layout.names + i * NAME_LEN;
                p.extend_from_slice(&bytes[name..name + NAME_LEN]);
                match layout.chops {
                    Some(chops) => {
                        let chops = chops + i * CHOP_WORDS * 4;
                        swap_words(&bytes[chops..chops + CHOP_WORDS * 4], &mut p);
                    }
                    // No chop points: unused slots, then zero reserved words.
                    None => {
                        p.extend_from_slice(&[0xFF; CHOP_POINTS * 4]);
                        p.extend_from_slice(&[0; (CHOP_WORDS - CHOP_POINTS) * 4]);
                    }
                }
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
        f[VERSION] = 3;
        put(&mut f, 0x0C, 0x7A80);
        put(&mut f, 0x10, 10_040);
        put(&mut f, 0x10 + 12 * 4, 9000 << 1 | 1);
        f[0x80..0x8A].copy_from_slice(b"PROJECT_06");

        // B1 (index 16): 512,000 frames, loop start moved to frame 1000, one chop point.
        let b1 = 16;
        let rec = V3.records + b1 * 172;
        put(&mut f, rec, 2_048_512);
        put(&mut f, rec + 4, 512);
        put(&mut f, rec + 8, 2_048_512);
        put(&mut f, rec + 12, 127);
        put(&mut f, rec + 44, 4512);
        f[V3.names + b1 * 24..V3.names + b1 * 24 + 4].copy_from_slice(b"BASS");
        let chops = V3.chops.unwrap() + b1 * 128;
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
    fn reads_version_2_without_name_or_chops() {
        let mut f = vec![0u8; V2.len];
        f[..4].copy_from_slice(MAGIC);
        put(&mut f, 0x04, 0xA0);
        f[VERSION] = 2;
        put(&mut f, 0x0C, 0x7A80);
        put(&mut f, 0x10, 9000);
        put(&mut f, 0x10 + 13 * 4, 8400 << 1);
        put(&mut f, 0x10 + 23 * 4, 18 << 16 | 21);

        // A1's record sits where version 3 keeps the project name.
        put(&mut f, V2.records, 2_181_816);
        put(&mut f, V2.records + 4, 512);
        put(&mut f, V2.records + 8, 2_181_816);
        put(&mut f, V2.records + 12, 121);
        f[V2.names..V2.names + 3].copy_from_slice(b"A-1");
        let j16 = 159;
        put(&mut f, V2.records + j16 * 172, 4512);
        f[V2.names + j16 * 24..V2.names + j16 * 24 + 4].copy_from_slice(b"LAST");

        let pc = Padconf::parse(&f).unwrap();
        assert_eq!(pc.settings().name(), "");
        assert_eq!(pc.settings().project_tempo(), 9000);
        assert_eq!(pc.settings().bank(1).tempo, 8400);
        assert_eq!(pc.settings().bank(0).volume, 106);
        assert_eq!(pc.settings().bank(1).volume, 109);

        let a1 = pc.pad("A1".parse().unwrap());
        assert!(a1.has_sample());
        assert_eq!(a1.start_end_bytes(), (512, 2_181_816));
        assert_eq!(a1.param(0x69), Some(121));
        assert_eq!(a1.name(), "A-1");
        assert!(a1.chop_points().iter().all(Option::is_none));
        assert_eq!(pc.pad("J16".parse().unwrap()).name(), "LAST");
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
        let mut f = vec![0u8; LEN];
        f[..4].copy_from_slice(MAGIC);
        f[VERSION] = 4;
        assert!(matches!(
            Padconf::parse(&f),
            Err(FormatError::UnsupportedPadconf(4))
        ));
        f[VERSION] = 2;
        assert!(matches!(
            Padconf::parse(&f[..V2.len - 1]),
            Err(FormatError::TooShort(31_487))
        ));
    }
}
