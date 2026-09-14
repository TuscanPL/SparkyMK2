//! Shared building blocks: decimation and short-time spectra.

use std::f32::consts::PI;

use realfft::RealFftPlanner;

/// Rate the analysers work at; content above ~5 kHz is not needed for tempo or key.
pub const ANALYSIS_RATE: f32 = 12_000.0;

/// Low-pass and decimate `input` towards [`ANALYSIS_RATE`]. Returns the signal and its rate.
pub fn decimate(input: &[f32], rate: u32) -> (Vec<f32>, f32) {
    let factor = (rate as f32 / ANALYSIS_RATE).floor().max(1.0) as usize;
    if factor == 1 {
        return (input.to_vec(), rate as f32);
    }
    // Windowed-sinc low-pass at 0.4 × the new Nyquist-normalised rate.
    let taps = 16 * factor + 1;
    let cutoff = 0.4 / factor as f32;
    let mid = (taps / 2) as f32;
    let kernel: Vec<f32> = (0..taps)
        .map(|i| {
            let x = i as f32 - mid;
            let sinc = if x == 0.0 {
                2.0 * cutoff
            } else {
                (2.0 * PI * cutoff * x).sin() / (PI * x)
            };
            let window = 0.5 - 0.5 * (2.0 * PI * i as f32 / (taps - 1) as f32).cos();
            sinc * window
        })
        .collect();
    let gain: f32 = kernel.iter().sum();
    let out = (0..input.len() / factor)
        .map(|n| {
            let centre = n * factor;
            kernel
                .iter()
                .enumerate()
                .map(|(i, k)| {
                    let j = (centre + i).checked_sub(taps / 2);
                    j.and_then(|j| input.get(j)).map_or(0.0, |s| s * k)
                })
                .sum::<f32>()
                / gain
        })
        .collect();
    (out, rate as f32 / factor as f32)
}

/// Magnitude spectra of Hann-windowed frames: `frames[t][bin]`.
pub fn spectrogram(signal: &[f32], size: usize, hop: usize) -> Vec<Vec<f32>> {
    if signal.len() < size {
        return Vec::new();
    }
    let mut planner = RealFftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(size);
    let window: Vec<f32> = (0..size)
        .map(|i| 0.5 - 0.5 * (2.0 * PI * i as f32 / size as f32).cos())
        .collect();
    let mut input = fft.make_input_vec();
    let mut output = fft.make_output_vec();
    let mut scratch = fft.make_scratch_vec();
    (0..=(signal.len() - size) / hop)
        .map(|t| {
            let frame = &signal[t * hop..t * hop + size];
            for ((dst, s), w) in input.iter_mut().zip(frame).zip(&window) {
                *dst = s * w;
            }
            fft.process_with_scratch(&mut input, &mut output, &mut scratch)
                .expect("buffer sizes come from the planner");
            output.iter().map(|c| c.norm()).collect()
        })
        .collect()
}
