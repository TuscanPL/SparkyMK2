//! Integers inside file-API payloads: 5 septets, big-endian, 35 bits.
//!
//! Plain values are signed 32-bit integers sign-extended to 35 bits, so −1 is
//! `7F 7F 7F 7F 7F`. Data-length fields reuse the three bits above bit 31 as flags.

use crate::ProtoError;

/// Encoded length of one integer.
pub const LEN: usize = 5;

/// First-septet flag on a data length: last read chunk, or last write chunk of a batch.
pub const FLAG_LAST: u8 = 0x40;

/// First-septet flag the host sets on read requests larger than one chunk.
pub const FLAG_MULTI: u8 = 0x20;

/// Encode a signed value (sign-extended to 35 bits).
pub fn encode(value: i32) -> [u8; LEN] {
    encode_raw((value as i64 as u64) & 0x7_FFFF_FFFF)
}

/// Encode an unsigned length with first-septet flags ([`FLAG_LAST`], [`FLAG_MULTI`]).
pub fn encode_len(value: u32, flags: u8) -> [u8; LEN] {
    let mut out = encode_raw(u64::from(value));
    out[0] |= flags;
    out
}

fn encode_raw(raw: u64) -> [u8; LEN] {
    let mut out = [0u8; LEN];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = ((raw >> (7 * (LEN - 1 - i))) & 0x7f) as u8;
    }
    out
}

/// A decoded 35-bit integer, split into the low 32 bits and the 3 bits above them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Septets {
    pub low: u32,
    pub high: u8,
}

impl Septets {
    /// The value as a signed 32-bit integer (for results, handles, offsets).
    pub fn as_i32(self) -> i32 {
        self.low as i32
    }

    /// The low 32 bits as an unsigned value (lengths), ignoring flags.
    pub fn as_u32(self) -> u32 {
        self.low
    }

    /// Whether the [`FLAG_LAST`] bit is set (only meaningful on data lengths).
    pub fn is_last(self) -> bool {
        self.high & 0b100 != 0
    }
}

/// Decode 5 septets from the start of `bytes`.
pub fn decode(bytes: &[u8]) -> Result<Septets, ProtoError> {
    let bytes = bytes.get(..LEN).ok_or(ProtoError::Truncated {
        need: LEN,
        have: bytes.len(),
    })?;
    let mut raw = 0u64;
    for &b in bytes {
        if b & 0x80 != 0 {
            return Err(ProtoError::BadSeptet(b));
        }
        raw = (raw << 7) | u64::from(b);
    }
    Ok(Septets {
        low: raw as u32,
        high: (raw >> 32) as u8,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_examples() {
        assert_eq!(encode(-1), [0x7f; 5]);
        assert_eq!(encode(0x2b), [0, 0, 0, 0, 0x2b]);
        assert_eq!(encode(512), [0, 0, 0, 0x04, 0x00]);
        // A device-side pointer used as a directory handle.
        assert_eq!(
            encode(0x8043_CDE8u32 as i32),
            [0x78, 0x02, 0x0f, 0x1b, 0x68]
        );
        assert_eq!(encode_len(512, FLAG_LAST), [0x40, 0, 0, 0x04, 0x00]);
        assert_eq!(encode_len(0x20000, FLAG_MULTI), [0x20, 0, 0x08, 0, 0]);
    }

    #[test]
    fn round_trip() {
        for v in [
            0,
            1,
            127,
            128,
            20480,
            -1,
            i32::MIN,
            i32::MAX,
            0x8043_CDE8u32 as i32,
        ] {
            assert_eq!(decode(&encode(v)).unwrap().as_i32(), v);
        }
        let d = decode(&[0x40, 0, 0, 0x04, 0x00]).unwrap();
        assert_eq!((d.as_u32(), d.is_last()), (512, true));
        assert!(!decode(&[0, 0, 0x01, 0x20, 0]).unwrap().is_last());
    }

    #[test]
    fn rejects_bad_input() {
        assert_eq!(
            decode(&[0x80, 0, 0, 0, 0]),
            Err(ProtoError::BadSeptet(0x80))
        );
        assert!(matches!(decode(&[0, 0]), Err(ProtoError::Truncated { .. })));
    }
}
