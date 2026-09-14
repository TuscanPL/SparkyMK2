//! Project settings (`7D proj 00 …` reply on channel 5, and the header of `PADCONF.BIN`).
//!
//! After the 3-byte prefix come 28 little-endian u32 words and a 32-byte project name.
//! Several words pack more than one value; see `docs/re/03-parameters.md`.

use crate::ProtoError;
use crate::control::reply;
use crate::pad::BANKS;

/// One bank's settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BankSettings {
    /// BPM × 100.
    pub tempo: u16,
    pub protected: bool,
    /// 0..=127 as shown by the app (the device stores 127 − volume).
    pub volume: u8,
}

/// Decoded project settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectSettings {
    raw: Vec<u8>,
}

impl ProjectSettings {
    pub const PREFIX: usize = 3;
    pub const WORDS: usize = 28;
    pub const NAME_OFFSET: usize = Self::PREFIX + Self::WORDS * 4;
    pub const NAME_LEN: usize = 32;
    pub const LEN: usize = Self::NAME_OFFSET + Self::NAME_LEN;

    const TEMPO_WORD: usize = 0;
    const BANK_WORDS: usize = 12;
    const VOLUME_WORDS: usize = 23;

    /// Parse a `7D` payload (including its command byte).
    pub fn parse(payload: &[u8]) -> Result<Self, ProtoError> {
        if payload.len() < Self::LEN {
            return Err(ProtoError::Truncated {
                need: Self::LEN,
                have: payload.len(),
            });
        }
        if payload[0] != reply::PROJECT_SETTINGS {
            return Err(ProtoError::UnexpectedCommand(payload[0]));
        }
        Ok(ProjectSettings {
            raw: payload[..Self::LEN].to_vec(),
        })
    }

    pub fn raw(&self) -> &[u8] {
        &self.raw
    }

    /// The project these settings belong to (0-based).
    pub fn project(&self) -> u8 {
        self.raw[1]
    }

    /// Raw word `n` (0-based, after the prefix).
    pub fn word(&self, n: usize) -> u32 {
        let at = Self::PREFIX + n * 4;
        u32::from_le_bytes(self.raw[at..at + 4].try_into().unwrap())
    }

    /// Project tempo, BPM × 100.
    pub fn project_tempo(&self) -> u16 {
        self.word(Self::TEMPO_WORD) as u16
    }

    /// Tempo Select: `true` = Project tempo, `false` = Bank tempo.
    pub fn uses_project_tempo(&self) -> bool {
        self.word(Self::TEMPO_WORD) & (1 << 16) != 0
    }

    /// `bank` 0..=9 (A..J).
    pub fn bank(&self, bank: usize) -> BankSettings {
        assert!(bank < usize::from(BANKS));
        let word = self.word(Self::BANK_WORDS + bank);
        let volumes = self.word(Self::VOLUME_WORDS + bank / 2);
        let stored_volume = (volumes >> (16 * (bank % 2))) as u16;
        BankSettings {
            tempo: (word >> 1) as u16,
            protected: word & 1 != 0,
            volume: 127u8.saturating_sub(stored_volume.min(127) as u8),
        }
    }

    pub fn name(&self) -> String {
        let bytes = &self.raw[Self::NAME_OFFSET..Self::NAME_OFFSET + Self::NAME_LEN];
        String::from_utf8_lossy(bytes)
            .trim_matches(['\0', ' '])
            .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Captured with Tempo Select = Project, bank A tempo 60.00, bank J tempo 120.00 and
    /// protected, bank A volume 80, bank J volume 80 (values from separate experiments,
    /// combined here).
    fn sample() -> Vec<u8> {
        let mut words = [0u32; ProjectSettings::WORDS];
        words[0] = 0x1_2738;
        words[3..6].fill(64);
        words[7..12].fill(32);
        words[12..22].fill(18_000);
        words[12] = 12_000;
        words[21] = 24_001;
        words[23] = 47;
        words[27] = 47 << 16;
        let mut p = vec![0x7D, 0x05, 0x00];
        for w in words {
            p.extend_from_slice(&w.to_le_bytes());
        }
        let mut name = *b"PROJECT_06                     \0";
        name[31] = 0;
        p.extend_from_slice(&name);
        p
    }

    #[test]
    fn decodes_packed_fields() {
        let s = ProjectSettings::parse(&sample()).unwrap();
        assert_eq!(s.raw().len(), 147);
        assert_eq!(s.project_tempo(), 10_040);
        assert!(s.uses_project_tempo());
        assert_eq!(
            s.bank(0),
            BankSettings {
                tempo: 6000,
                protected: false,
                volume: 80
            }
        );
        assert_eq!(
            s.bank(1),
            BankSettings {
                tempo: 9000,
                protected: false,
                volume: 127
            }
        );
        assert_eq!(
            s.bank(9),
            BankSettings {
                tempo: 12_000,
                protected: true,
                volume: 80
            }
        );
        assert_eq!(s.name(), "PROJECT_06");
    }
}
