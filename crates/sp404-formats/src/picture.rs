//! `PICTURE/*.bmp`: the 128×64 display images a project shows at startup and as a
//! screen saver. Each file is a 1086-byte 1-bit Windows BMP.

use crate::FormatError;

pub const WIDTH: usize = 128;
pub const HEIGHT: usize = 64;
/// Bytes per packed row; within a byte the leftmost pixel is the high bit.
pub const STRIDE: usize = WIDTH / 8;
/// A decoded image: [`HEIGHT`] rows of [`STRIDE`] bytes, top row first, set bit = lit.
pub const ROWS_LEN: usize = HEIGHT * STRIDE;
const HEADER_LEN: usize = 62;
/// Size of the BMP files on the card.
pub const FILE_LEN: usize = HEADER_LEN + ROWS_LEN;

/// The header the device's own files carry, reproduced byte for byte: a 40-byte
/// `BITMAPINFOHEADER`, bottom-up rows, and a black/white two-colour palette.
const HEADER: [u8; HEADER_LEN] = [
    b'B', b'M', // magic
    0x3E, 0x04, 0x00, 0x00, // file size, 1086
    0x00, 0x00, 0x00, 0x00, // reserved
    0x3E, 0x00, 0x00, 0x00, // pixel data at 62
    0x28, 0x00, 0x00, 0x00, // DIB header size, 40
    0x80, 0x00, 0x00, 0x00, // width, 128
    0x40, 0x00, 0x00, 0x00, // height, 64 (positive: bottom-up)
    0x01, 0x00, // planes
    0x01, 0x00, // bits per pixel
    0x00, 0x00, 0x00, 0x00, // compression: BI_RGB
    0x00, 0x00, 0x00, 0x00, // image size, unspecified
    0x00, 0x00, 0x00, 0x00, // pixels per metre, x
    0x00, 0x00, 0x00, 0x00, // pixels per metre, y
    0x00, 0x00, 0x00, 0x00, // colours used
    0x00, 0x00, 0x00, 0x00, // important colours
    0x00, 0x00, 0x00, 0x00, // palette 0: black
    0xFF, 0xFF, 0xFF, 0x00, // palette 1: white
];

fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

fn u16_at(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap())
}

fn bad(why: &'static str) -> FormatError {
    FormatError::BadPicture(why)
}

/// Build a card file from packed rows. Accepts what [`decode`] returns.
pub fn encode(rows: &[u8]) -> Result<Vec<u8>, FormatError> {
    if rows.len() != ROWS_LEN {
        return Err(bad("image is not 128×64"));
    }
    let mut out = Vec::with_capacity(FILE_LEN);
    out.extend_from_slice(&HEADER);
    // BMP stores the bottom row first.
    for row in rows.chunks_exact(STRIDE).rev() {
        out.extend_from_slice(row);
    }
    Ok(out)
}

/// Read a card file into packed rows, top row first, set bit = lit pixel.
///
/// Also accepts the two things other tools produce: top-down rows (a negative height)
/// and a palette with the colours the other way round.
pub fn decode(bytes: &[u8]) -> Result<Vec<u8>, FormatError> {
    if bytes.len() < HEADER_LEN {
        return Err(FormatError::TooShort(bytes.len()));
    }
    if &bytes[0..2] != b"BM" {
        return Err(FormatError::BadMagic);
    }
    let pixels_at = u32_at(bytes, 0x0A) as usize;
    let dib_len = u32_at(bytes, 0x0E) as usize;
    if dib_len < 40 {
        return Err(bad("unsupported BMP header"));
    }
    let width = u32_at(bytes, 0x12) as i32;
    let height = u32_at(bytes, 0x16) as i32;
    let bpp = u16_at(bytes, 0x1C);
    let compression = u32_at(bytes, 0x1E);
    if width != WIDTH as i32 || height.unsigned_abs() != HEIGHT as u32 {
        return Err(bad("image is not 128×64"));
    }
    if bpp != 1 {
        return Err(bad("image is not 1-bit black and white"));
    }
    if compression != 0 {
        return Err(bad("compressed BMPs are not supported"));
    }

    // A 1-bit palette follows the DIB header: two BGRA entries, index 0 then index 1.
    let palette_at = 14 + dib_len;
    let lit_is_set = match bytes.get(palette_at..palette_at + 8) {
        Some(p) => brightness(&p[4..8]) >= brightness(&p[0..4]),
        None => true,
    };

    let end = pixels_at
        .checked_add(ROWS_LEN)
        .filter(|&e| e <= bytes.len())
        .ok_or(FormatError::TooShort(bytes.len()))?;
    let pixels = &bytes[pixels_at..end];

    let mut rows = Vec::with_capacity(ROWS_LEN);
    let top_down = height < 0;
    for i in 0..HEIGHT {
        let source = if top_down { i } else { HEIGHT - 1 - i };
        rows.extend_from_slice(&pixels[source * STRIDE..(source + 1) * STRIDE]);
    }
    if !lit_is_set {
        for byte in &mut rows {
            *byte = !*byte;
        }
    }
    Ok(rows)
}

fn brightness(bgra: &[u8]) -> u32 {
    u32::from(bgra[0]) + u32::from(bgra[1]) + u32::from(bgra[2])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Rows with a recognisable pattern: a lit top-left pixel and a lit bottom row.
    fn sample() -> Vec<u8> {
        let mut rows = vec![0u8; ROWS_LEN];
        rows[0] = 0x80;
        for byte in &mut rows[(HEIGHT - 1) * STRIDE..] {
            *byte = 0xFF;
        }
        rows
    }

    #[test]
    fn encodes_the_device_header() {
        let file = encode(&sample()).unwrap();
        assert_eq!(file.len(), FILE_LEN);
        assert_eq!(&file[..HEADER_LEN], &HEADER);
        // Bottom-up: the last row of the image comes first in the file.
        assert_eq!(file[HEADER_LEN], 0xFF);
        assert_eq!(file[FILE_LEN - STRIDE], 0x80);
    }

    #[test]
    fn round_trips() {
        let rows = sample();
        assert_eq!(decode(&encode(&rows).unwrap()).unwrap(), rows);
    }

    #[test]
    fn reads_top_down_and_inverted_files() {
        let rows = sample();
        let mut file = encode(&rows).unwrap();
        // Negative height, rows in the other order, and a swapped palette.
        file[0x16..0x1A].copy_from_slice(&(-(HEIGHT as i32)).to_le_bytes());
        let flipped: Vec<u8> = file[HEADER_LEN..]
            .chunks_exact(STRIDE)
            .rev()
            .flat_map(|r| r.iter().map(|b| !b))
            .collect();
        file.truncate(HEADER_LEN);
        file.extend_from_slice(&flipped);
        file[0x36..0x3A].copy_from_slice(&[0xFF, 0xFF, 0xFF, 0x00]);
        file[0x3A..0x3E].copy_from_slice(&[0x00, 0x00, 0x00, 0x00]);
        assert_eq!(decode(&file).unwrap(), rows);
    }

    #[test]
    fn rejects_other_sizes() {
        let mut file = encode(&sample()).unwrap();
        file[0x12..0x16].copy_from_slice(&64u32.to_le_bytes());
        assert!(decode(&file).is_err());
        assert!(encode(&[0u8; 8]).is_err());
    }
}
