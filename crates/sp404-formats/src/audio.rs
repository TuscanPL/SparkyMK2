//! Decode audio files (WAV, AIFF, FLAC, MP3) into a [`Sample`] ready for import.
//!
//! The device stores 48 kHz 16-bit audio, mono or stereo. Mono sources stay mono (as the
//! official app does), sources with more than two channels keep the first two, and other
//! sample rates are converted with a band-limited (FFT) resampler.

use std::fs::File;
use std::path::Path;

use audioadapter_buffers::direct::InterleavedSlice;
use rubato::{Fft, FixedSync, Resampler};
use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;

use crate::FormatError;
use crate::smp::{SAMPLE_RATE, Sample};

/// File extensions the official app accepts for import.
pub const EXTENSIONS: &[&str] = &["wav", "bwf", "aif", "aiff", "flac", "mp3"];

/// An imported sample plus what the source looked like.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Imported {
    pub sample: Sample,
    pub source_rate: u32,
    pub source_channels: usize,
}

impl Imported {
    /// Whether the audio went through sample-rate conversion.
    pub fn resampled(&self) -> bool {
        self.source_rate != SAMPLE_RATE
    }
}

/// Decode an audio file into 48 kHz 16-bit mono or stereo audio.
pub fn read(path: impl AsRef<Path>) -> Result<Imported, FormatError> {
    let path = path.as_ref();
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let file = File::open(path)?;
    let stream = MediaSourceStream::new(Box::new(file), Default::default());
    let mut format = symphonia::default::get_probe().probe(
        &hint,
        stream,
        FormatOptions::default(),
        MetadataOptions::default(),
    )?;
    let track = format
        .default_track(TrackType::Audio)
        .ok_or(FormatError::NoAudioTrack)?;
    let track_id = track.id;
    let params = track
        .codec_params
        .as_ref()
        .and_then(|p| p.audio())
        .ok_or(FormatError::NoAudioTrack)?
        .clone();
    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(&params, &AudioDecoderOptions::default())?;

    let mut rate = params.sample_rate;
    let mut source_channels = params.channels.as_ref().map_or(0, |c| c.count());
    let mut scratch: Vec<f32> = Vec::new();
    // Output channels, fixed by the first decoded packet.
    let mut out_channels: Option<usize> = None;
    // Audio quantized directly when no resampling is needed.
    let mut direct: Vec<i16> = Vec::new();
    let mut pending: Vec<f32> = Vec::new();

    while let Some(packet) = format.next_packet()? {
        if packet.track_id != track_id {
            continue;
        }
        let buf = match decoder.decode(&packet) {
            Ok(buf) => buf,
            // A corrupt frame (common in MP3s) is skipped, as players do.
            Err(SymphoniaError::DecodeError(_)) => continue,
            Err(e) => return Err(e.into()),
        };
        let spec = buf.spec();
        let rate = *rate.get_or_insert(spec.rate());
        let channels = spec.channels().count();
        if channels == 0 {
            continue;
        }
        source_channels = channels;
        let keep = *out_channels.get_or_insert(channels.min(2));
        buf.copy_to_vec_interleaved(&mut scratch);
        let kept = scratch
            .chunks_exact(channels)
            .flat_map(|frame| (0..keep).map(move |c| frame[c.min(frame.len() - 1)]));
        if rate == SAMPLE_RATE {
            direct.extend(kept.map(quantize));
        } else {
            pending.extend(kept);
        }
    }

    let rate = rate.ok_or(FormatError::Empty)?;
    let channels = out_channels.ok_or(FormatError::Empty)?;
    let samples = if rate == SAMPLE_RATE {
        direct
    } else {
        resample(&pending, channels, rate)?
            .into_iter()
            .map(quantize)
            .collect()
    };
    if samples.is_empty() {
        return Err(FormatError::Empty);
    }
    Ok(Imported {
        sample: Sample {
            channels: channels as u16,
            samples,
        },
        source_rate: rate,
        source_channels,
    })
}

/// Full-scale float (−1.0..1.0) to 16-bit. Exact for audio that was 16-bit to begin with.
fn quantize(v: f32) -> i16 {
    (v * 32_768.0).round().clamp(-32_768.0, 32_767.0) as i16
}

/// Convert interleaved audio from `rate` to 48 kHz.
fn resample(input: &[f32], channels: usize, rate: u32) -> Result<Vec<f32>, FormatError> {
    let frames = input.len() / channels;
    if frames == 0 {
        return Ok(Vec::new());
    }
    let resample_err = |e: &dyn std::fmt::Display| FormatError::Resample(e.to_string());
    let mut resampler = Fft::<f32>::new(
        rate as usize,
        SAMPLE_RATE as usize,
        1024,
        channels,
        FixedSync::Input,
    )
    .map_err(|e| resample_err(&e))?;
    let capacity = resampler.process_all_needed_output_len(frames);
    let mut output = vec![0.0f32; capacity * channels];
    let input_adapter =
        InterleavedSlice::new(input, channels, frames).map_err(|e| resample_err(&e))?;
    let mut output_adapter =
        InterleavedSlice::new_mut(&mut output, channels, capacity).map_err(|e| resample_err(&e))?;
    let (_, written) = resampler
        .process_all_into_buffer(&input_adapter, &mut output_adapter, frames, None)
        .map_err(|e| resample_err(&e))?;
    // rubato derives the length from a float ratio and can overshoot by a frame.
    let expected = (frames as u64 * u64::from(SAMPLE_RATE)).div_ceil(u64::from(rate)) as usize;
    output.truncate(written.min(expected) * channels);
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("sp404-audio-{}-{name}", std::process::id()))
    }

    fn write_wav(
        name: &str,
        rate: u32,
        channels: u16,
        bits: u16,
        values: impl IntoIterator<Item = i32>,
    ) -> std::path::PathBuf {
        let path = temp(name);
        let spec = hound::WavSpec {
            channels,
            sample_rate: rate,
            bits_per_sample: bits,
            sample_format: hound::SampleFormat::Int,
        };
        let mut w = hound::WavWriter::create(&path, spec).unwrap();
        for v in values {
            w.write_sample(v).unwrap();
        }
        w.finalize().unwrap();
        path
    }

    #[test]
    fn stereo_16bit_48k_is_bit_exact() {
        let values = [0, 1, -1, 32_767, -32_768, 1234, -4321, 7];
        let path = write_wav("exact.wav", 48_000, 2, 16, values);
        let imported = read(&path).unwrap();
        assert_eq!(
            imported.sample.samples,
            values.iter().map(|&v| v as i16).collect::<Vec<_>>()
        );
        assert!(!imported.resampled());
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn mono_24bit_stays_mono_16bit() {
        let path = write_wav(
            "mono24.wav",
            48_000,
            1,
            24,
            [0, 0x100, -0x80_0000, 0x7f_ffff],
        );
        let imported = read(&path).unwrap();
        assert_eq!(imported.source_channels, 1);
        assert_eq!(imported.sample.channels, 1);
        assert_eq!(imported.sample.samples, vec![0, 1, i16::MIN, i16::MAX]);
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn resamples_44k_tone_to_48k() {
        // One second of a 1 kHz tone at 44.1 kHz.
        let tone = (0..44_100).map(|n| {
            let t = n as f64 / 44_100.0;
            ((2.0 * std::f64::consts::PI * 1000.0 * t).sin() * 16_000.0) as i32
        });
        let path = write_wav("tone44.wav", 44_100, 1, 16, tone);
        let imported = read(&path).unwrap();
        assert!(imported.resampled());
        let s = &imported.sample;
        assert_eq!((s.channels, s.frames()), (1, 48_000));

        // Compare the middle against an ideal 1 kHz tone at 48 kHz.
        let mut err = 0.0f64;
        let mut ref_energy = 0.0f64;
        for n in 4_800..43_200 {
            let t = n as f64 / 48_000.0;
            let ideal = (2.0 * std::f64::consts::PI * 1000.0 * t).sin() * 16_000.0;
            let got = f64::from(s.samples[n]);
            err += (got - ideal).powi(2);
            ref_energy += ideal.powi(2);
        }
        let snr_db = 10.0 * (ref_energy / err).log10();
        assert!(snr_db > 60.0, "SNR {snr_db:.1} dB");
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn rejects_non_audio() {
        let path = temp("junk.wav");
        std::fs::write(&path, b"definitely not audio").unwrap();
        assert!(read(&path).is_err());
        std::fs::remove_file(path).ok();
    }
}
