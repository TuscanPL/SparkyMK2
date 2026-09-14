//! WAV export for [`Sample`]. Import of all formats lives in [`crate::audio`].

use std::path::Path;

use crate::FormatError;
use crate::smp::{SAMPLE_RATE, Sample};

/// Write a sample as a 16-bit PCM WAV file.
pub fn write(sample: &Sample, path: impl AsRef<Path>) -> Result<(), FormatError> {
    let spec = hound::WavSpec {
        channels: sample.channels,
        sample_rate: SAMPLE_RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec)?;
    for &s in &sample.samples {
        writer.write_sample(s)?;
    }
    writer.finalize()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_import_round_trip() {
        let path = std::env::temp_dir().join(format!("sp404-wav-{}-rt.wav", std::process::id()));
        let s = Sample {
            channels: 2,
            samples: vec![1, -2, 3, -4],
        };
        write(&s, &path).unwrap();
        assert_eq!(crate::audio::read(&path).unwrap().sample, s);
        std::fs::remove_file(path).ok();
    }
}
