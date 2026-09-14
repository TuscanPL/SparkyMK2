//! Host-side analysis the official app performs on the PC: tempo ("Analyze BPM") and key
//! detection. These are independent implementations; results are not expected to match
//! the app's detector exactly.

pub mod key;
pub mod tempo;

mod signal;

pub use key::Key;
pub use tempo::{Tempo, TempoRange};

/// Mix interleaved 16-bit audio down to mono floats in −1.0..1.0.
pub fn mono_from_i16(samples: &[i16], channels: usize) -> Vec<f32> {
    let channels = channels.max(1);
    samples
        .chunks_exact(channels)
        .map(|frame| {
            frame.iter().map(|&s| f32::from(s)).sum::<f32>() / (channels as f32 * 32_768.0)
        })
        .collect()
}
