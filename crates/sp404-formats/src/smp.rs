//! `*.SMP` sample files: a 512-byte `RFWV` header followed by big-endian 16-bit PCM.

use md5::{Digest, Md5};

use crate::FormatError;

pub const HEADER_LEN: usize = 512;
pub const SAMPLE_RATE: u32 = 48_000;
pub const BITS: u32 = 16;

const MAGIC: &[u8; 4] = b"RFWV";
const FIELDS: std::ops::Range<usize> = 4..20;
const DIGEST: std::ops::Range<usize> = 0x20..0x30;
const FILLER: std::ops::Range<usize> = 0x30..HEADER_LEN;
/// Size field = PCM bytes + this (the header minus magic and size field).
const SIZE_BIAS: u32 = (HEADER_LEN - 8) as u32;

/// Interleaved 16-bit audio at 48 kHz.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sample {
    pub channels: u16,
    pub samples: Vec<i16>,
}

impl Sample {
    pub fn frames(&self) -> usize {
        self.samples.len() / usize::from(self.channels.max(1))
    }

    /// Decode an SMP file, verifying magic, format, size and digest.
    pub fn from_smp(bytes: &[u8]) -> Result<Self, FormatError> {
        let info = HeaderInfo::parse(bytes)?;
        let pcm = &bytes[HEADER_LEN..];
        let declared = u64::from(info.pcm_len);
        if declared != pcm.len() as u64 {
            return Err(FormatError::SizeMismatch {
                declared,
                actual: pcm.len() as u64,
            });
        }
        let samples = pcm
            .chunks_exact(2)
            .map(|c| i16::from_be_bytes([c[0], c[1]]))
            .collect();
        Ok(Sample {
            channels: info.channels,
            samples,
        })
    }

    /// Encode as an SMP file.
    pub fn to_smp(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(HEADER_LEN + self.samples.len() * 2);
        out.extend_from_slice(&header(
            (self.samples.len() * 2) as u32,
            u32::from(self.channels),
        ));
        for s in &self.samples {
            out.extend_from_slice(&s.to_be_bytes());
        }
        out
    }
}

/// Format fields of an SMP header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeaderInfo {
    pub channels: u16,
    /// PCM bytes after the header, per the size field.
    pub pcm_len: u32,
}

impl HeaderInfo {
    /// Parse and verify the first 512 bytes of an SMP file.
    pub fn parse(header: &[u8]) -> Result<Self, FormatError> {
        if header.len() < HEADER_LEN {
            return Err(FormatError::TooShort(header.len()));
        }
        if &header[..4] != MAGIC {
            return Err(FormatError::BadMagic);
        }
        let be = |at: usize| u32::from_be_bytes(header[at..at + 4].try_into().unwrap());
        let (size, rate, channels, bits) = (be(4), be(8), be(12), be(16));
        if rate != SAMPLE_RATE || bits != BITS || !(1..=2).contains(&channels) {
            return Err(FormatError::UnsupportedSmp {
                rate,
                channels,
                bits,
            });
        }
        if !digest_matches(header) {
            return Err(FormatError::DigestMismatch);
        }
        Ok(HeaderInfo {
            channels: channels as u16,
            pcm_len: size.saturating_sub(SIZE_BIAS),
        })
    }

    pub fn frames(&self) -> u32 {
        self.pcm_len / (2 * u32::from(self.channels))
    }

    /// Frame number of a byte offset into the file (as stored in pad blocks).
    pub fn frame_at(&self, byte_offset: u32) -> u32 {
        byte_offset.saturating_sub(HEADER_LEN as u32) / (2 * u32::from(self.channels))
    }
}

/// Build a header for `pcm_len` bytes of audio.
pub fn header(pcm_len: u32, channels: u32) -> [u8; HEADER_LEN] {
    let mut h = [0u8; HEADER_LEN];
    h[..4].copy_from_slice(MAGIC);
    h[4..8].copy_from_slice(&(pcm_len + SIZE_BIAS).to_be_bytes());
    h[8..12].copy_from_slice(&SAMPLE_RATE.to_be_bytes());
    h[12..16].copy_from_slice(&channels.to_be_bytes());
    h[16..20].copy_from_slice(&BITS.to_be_bytes());
    let digest = Md5::digest(&h[FIELDS]);
    h[DIGEST].copy_from_slice(&digest);
    fill(&mut h[FILLER], &digest);
    h
}

/// The digest rule: MD5 over the four big-endian fields after the magic.
pub fn digest_matches(header: &[u8]) -> bool {
    header.len() >= DIGEST.end && Md5::digest(&header[FIELDS]).as_slice() == &header[DIGEST]
}

/// The filler has no meaning; use deterministic pseudo-random bytes so output is
/// reproducible.
fn fill(out: &mut [u8], seed: &[u8]) {
    let mut x = u64::from_le_bytes(seed[..8].try_into().unwrap()) | 1;
    for b in out {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        *b = (x >> 32) as u8;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_fields_match_spec() {
        // 144000 stereo frames, as in the captured import of a 3 s test tone.
        let h = header(576_000, 2);
        assert_eq!(
            &h[..20],
            &[
                b'R', b'F', b'W', b'V', 0x00, 0x08, 0xcb, 0xf8, 0x00, 0x00, 0xbb, 0x80, 0, 0, 0, 2,
                0, 0, 0, 0x10
            ]
        );
        assert!(h[20..32].iter().all(|&b| b == 0));
        assert!(digest_matches(&h));
        assert_eq!(
            &h[0x20..0x24],
            &[0x29, 0x74, 0xff, 0x85],
            "digest bytes seen in the capture"
        );
    }

    #[test]
    fn round_trip() {
        let s = Sample {
            channels: 2,
            samples: vec![0, 1, -1, i16::MAX, i16::MIN, 1234],
        };
        let bytes = s.to_smp();
        assert_eq!(bytes.len(), 512 + 12);
        assert_eq!(&bytes[512..516], &[0x00, 0x00, 0x00, 0x01]);
        assert_eq!(Sample::from_smp(&bytes).unwrap(), s);
    }

    #[test]
    fn detects_corruption() {
        let mut bytes = Sample {
            channels: 2,
            samples: vec![0; 8],
        }
        .to_smp();
        bytes[0x21] ^= 1;
        assert!(matches!(
            Sample::from_smp(&bytes),
            Err(FormatError::DigestMismatch)
        ));
        let mut bytes = Sample {
            channels: 2,
            samples: vec![0; 8],
        }
        .to_smp();
        bytes.pop();
        assert!(matches!(
            Sample::from_smp(&bytes),
            Err(FormatError::SizeMismatch { .. })
        ));
    }
}
