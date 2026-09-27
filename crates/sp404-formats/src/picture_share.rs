//! Display images outside the device: PNG files and share codes.
//!
//! A PNG holds one or more 128×64 frames stacked top to bottom, white where the screen is
//! lit. SparkyMK2 writes them as 1-bit greyscale, but any PNG of the right size is read,
//! so frames drawn in another program work too. A share code is one frame as text, for
//! pasting into a chat: `sparky1:` and the frame's packed rows, zlib-compressed and
//! base64url-encoded.

use std::io::Cursor;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;

use crate::FormatError;
use crate::picture::{HEIGHT, ROWS_LEN, STRIDE, WIDTH};

/// What every share code starts with; the digit is the format's version.
pub const SHARE_PREFIX: &str = "sparky1:";

fn bad(why: &'static str) -> FormatError {
    FormatError::BadPicture(why)
}

/// Write frames (packed rows, as [`crate::picture::decode`] returns) as one PNG, the
/// first frame at the top.
pub fn to_png(frames: &[Vec<u8>]) -> Result<Vec<u8>, FormatError> {
    if frames.is_empty() {
        return Err(bad("no frames to write"));
    }
    if frames.iter().any(|f| f.len() != ROWS_LEN) {
        return Err(bad("image is not 128×64"));
    }
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, WIDTH as u32, (HEIGHT * frames.len()) as u32);
    encoder.set_color(png::ColorType::Grayscale);
    // With one bit per pixel, 1 is white: the same packing as the rows themselves.
    encoder.set_depth(png::BitDepth::One);
    let mut writer = encoder
        .write_header()
        .map_err(|_| bad("could not write the PNG"))?;
    writer
        .write_image_data(&frames.concat())
        .map_err(|_| bad("could not write the PNG"))?;
    writer
        .finish()
        .map_err(|_| bad("could not write the PNG"))?;
    Ok(out)
}

/// Read a PNG made of 128×64 frames stacked top to bottom. A pixel is lit when it is
/// brighter than mid-grey and not mostly transparent.
pub fn from_png(bytes: &[u8]) -> Result<Vec<Vec<u8>>, FormatError> {
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().map_err(|_| bad("not a readable PNG"))?;
    let (width, height) = {
        let info = reader.info();
        (info.width as usize, info.height as usize)
    };
    if width != WIDTH || height == 0 || height % HEIGHT != 0 {
        return Err(bad(
            "image is not 128 pixels wide and a multiple of 64 high",
        ));
    }
    let size = reader
        .output_buffer_size()
        .ok_or(bad("image is too large"))?;
    let mut buf = vec![0; size];
    let out = reader
        .next_frame(&mut buf)
        .map_err(|_| bad("not a readable PNG"))?;
    let channels = out.color_type.samples();
    let lit = |px: &[u8]| -> bool {
        let (luma, alpha) = match px.len() {
            1 => (u32::from(px[0]), 255),
            2 => (u32::from(px[0]), px[1]),
            3 => (luma(px), 255),
            _ => (luma(px), px[3]),
        };
        luma >= 128 && alpha >= 128
    };

    let mut rows = vec![0u8; ROWS_LEN * (height / HEIGHT)];
    for y in 0..height {
        let line = &buf[y * out.line_size..][..width * channels];
        for (x, px) in line.chunks_exact(channels).enumerate() {
            if lit(px) {
                rows[y * STRIDE + x / 8] |= 0x80 >> (x % 8);
            }
        }
    }
    Ok(rows.chunks_exact(ROWS_LEN).map(<[u8]>::to_vec).collect())
}

fn luma(rgb: &[u8]) -> u32 {
    (299 * u32::from(rgb[0]) + 587 * u32::from(rgb[1]) + 114 * u32::from(rgb[2])) / 1000
}

/// One frame as a share code.
pub fn share_code(rows: &[u8]) -> Result<String, FormatError> {
    if rows.len() != ROWS_LEN {
        return Err(bad("image is not 128×64"));
    }
    let packed = miniz_oxide::deflate::compress_to_vec_zlib(rows, 10);
    Ok(format!("{SHARE_PREFIX}{}", URL_SAFE_NO_PAD.encode(packed)))
}

/// Read a frame back from a share code. The code may sit inside other text, such as a
/// whole chat message; it ends at the first character a code cannot hold.
pub fn from_share_code(text: &str) -> Result<Vec<u8>, FormatError> {
    let start = text
        .find(SHARE_PREFIX)
        .ok_or(bad("no share code found (they start with sparky1:)"))?;
    let body: String = text[start + SHARE_PREFIX.len()..]
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    let packed = URL_SAFE_NO_PAD
        .decode(body)
        .map_err(|_| bad("the share code is damaged"))?;
    // The zlib checksum catches a code that was cut short or mistyped.
    let rows = miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(&packed, ROWS_LEN)
        .map_err(|_| bad("the share code is damaged"))?;
    if rows.len() != ROWS_LEN {
        return Err(bad("the share code is damaged"));
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(seed: u8) -> Vec<u8> {
        (0..ROWS_LEN)
            .map(|i| (i as u8).wrapping_mul(seed) ^ seed)
            .collect()
    }

    #[test]
    fn png_round_trips_one_frame_and_a_set() {
        let one = vec![frame(3)];
        assert_eq!(from_png(&to_png(&one).unwrap()).unwrap(), one);
        let set: Vec<_> = (1..=6).map(frame).collect();
        assert_eq!(from_png(&to_png(&set).unwrap()).unwrap(), set);
    }

    #[test]
    fn reads_colour_pngs_from_other_programs() {
        // RGBA: lit where white and opaque; a transparent white pixel stays dark.
        let mut data = vec![0u8; WIDTH * HEIGHT * 4];
        data[..4].copy_from_slice(&[255, 255, 255, 255]);
        data[4..8].copy_from_slice(&[255, 255, 255, 0]);
        data[(WIDTH * HEIGHT - 1) * 4..].copy_from_slice(&[200, 220, 210, 255]);
        let mut bytes = Vec::new();
        let mut encoder = png::Encoder::new(&mut bytes, WIDTH as u32, HEIGHT as u32);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&data)
            .unwrap();
        let rows = &from_png(&bytes).unwrap()[0];
        assert_eq!(rows[0], 0x80);
        assert_eq!(rows[ROWS_LEN - 1], 0x01);
        assert_eq!(rows.iter().map(|b| b.count_ones()).sum::<u32>(), 2);
    }

    #[test]
    fn rejects_pngs_of_other_sizes() {
        let mut bytes = Vec::new();
        let mut encoder = png::Encoder::new(&mut bytes, 64, 64);
        encoder.set_color(png::ColorType::Grayscale);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&[0; 64 * 64])
            .unwrap();
        assert!(from_png(&bytes).is_err());
        assert!(from_png(b"not a png").is_err());
    }

    #[test]
    fn share_codes_round_trip_inside_other_text() {
        let rows = frame(7);
        let code = share_code(&rows).unwrap();
        assert!(code.starts_with(SHARE_PREFIX));
        let message = format!("here's my screen saver:\n{code}\nenjoy!");
        assert_eq!(from_share_code(&message).unwrap(), rows);
    }

    #[test]
    fn a_blank_frame_makes_a_short_code() {
        assert!(share_code(&[0; ROWS_LEN]).unwrap().len() < 40);
    }

    #[test]
    fn rejects_damaged_codes() {
        let code = share_code(&frame(5)).unwrap();
        assert!(from_share_code(&code[..code.len() - 6]).is_err());
        assert!(from_share_code("hello").is_err());
        let mut flipped = code.into_bytes();
        let last = flipped.len() - 10;
        flipped[last] = if flipped[last] == b'A' { b'B' } else { b'A' };
        assert!(from_share_code(std::str::from_utf8(&flipped).unwrap()).is_err());
    }

    #[test]
    fn reads_a_real_code_of_a_detailed_frame() {
        // Made by the app from a dithered image; nearly 1,000 characters long.
        let code = concat!(
            "sparky1:eNpFUjFvFEcUfvPu4ZtBljw7scJSMTueIEeylAsgpQuzq42yJyzZoUpnJ46UMr8g8tx6IeeI",
            "Ym1AMqLAlhA1NQhhIwSiAwpa_wJqCophjj3Daot5-733ve_7ZgG-PoSgTs5MOXxNu6_ATTEHKG-Pl740",
            "CxCWFs6D7sqzY6DxFRqC72pnvNw72179wg1w2jXffDvlY9kymOoi_XWyjkuwrXLb03leGNjWy-mjKY4J",
            "hyTbwBnW1bZxzPq0eDPFtZZQFLvppSmdEB4r9yJ7MJmdyCk0bOOg-X6qZlcC2XpJrXTSiBpP5bURvuvw",
            "RGtIEvq9uhMLGV8hHFPR8JMOL48SqAphb_59Yo9ALapKXJxo6T75RjN798Q_gvWnHqp_4vEQhIa2R7BK",
            "t9Y6UFgOOV1Z_-5c1A4waDLN04QbcbljH4yd8P6-f7v22c4RImhj7HPVzfNq1RN3itmnEzuIjOC5F0VV",
            "fpzwodvJUDbNEHc2YzPwxZLH2_wxgZthwse2qBZmA8trInT79ABTB1xtvJ9eNsmRgx7ORX4b69ZZ6xnw",
            "_8JnuzXJLMo65D_Fei_ixYgPL7H5C79-iNGnPVtBoX5hpO_F_Y4wZ0Jer3q6_CE4JjXtWCT7P-0lK7Ff",
            "ckZ8cJDETr92HHME7nzDh8CYOBcANWC6y9QizJrs5xB_J-dqWbm69H7mcgCWK6XzmIg0zj0NMQ7ygi_0",
            "klqB3Ix51G7pmdG6MfmINo8d6iHIER_v81nI_436iRc0XwiT1yBD0LNHtTyQMrelQQqTfHTDi6Y8kIsw",
            "-vDY4azNFM_kPo0hizgfUeVGHM__YXAY55nOka8Wf67zAvKIy9NptjCTiAyvUj8ENwP1POnrzP4mYSsE",
            "CTv68HC8cGG9uRGvJ_YT5ynOoTHr2A_H7pTeMrj6UtwXXPTDpgTeomvO6My2cGay_5lsl7M-7TctzoVQ",
            "oSwt4yv2QaJhJeJIOWuLfq2HmiL-CePPvc4",
        );
        assert_eq!(from_share_code(code).unwrap().len(), ROWS_LEN);
    }
}
