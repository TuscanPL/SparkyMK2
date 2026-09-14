//! Tempo estimation: spectral-flux onset envelope, autocorrelation, and a comb search over
//! the allowed BPM range.

use crate::signal::{decimate, spectrogram};

const FRAME: usize = 1024;
const HOP: usize = 128;
/// Periods of the beat the comb looks at (1 = the beat itself).
const COMB: [f32; 4] = [1.0, 2.0, 3.0, 4.0];

/// Allowed tempo range in BPM. Results are folded into it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TempoRange {
    pub min: f32,
    pub max: f32,
}

impl TempoRange {
    /// The official app's ranges for the device's "BPM detect range" setting.
    pub fn device_preset(setting: u8) -> TempoRange {
        let (min, max) = match setting {
            0 => (99.0, 199.0),
            1 => (79.0, 159.0),
            2 => (69.0, 139.0),
            3 => (49.0, 99.0),
            _ => (75.0, 150.0),
        };
        TempoRange { min, max }
    }
}

impl Default for TempoRange {
    /// Preset 0 (99–199), which matched the device used for testing.
    fn default() -> Self {
        TempoRange::device_preset(0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tempo {
    pub bpm: f32,
    /// Peak salience relative to the average over the range (≥ 1; higher is clearer).
    pub confidence: f32,
}

impl Tempo {
    /// BPM × 100 for pad parameter `6F`, rounded to 0.1 BPM as the official app stores it.
    pub fn bpm_x100(&self) -> u32 {
        (self.bpm * 10.0).round().max(0.0) as u32 * 10
    }
}

/// Estimate the tempo of mono audio. `None` when the audio is too short or has no onsets.
pub fn detect(mono: &[f32], rate: u32, range: TempoRange) -> Option<Tempo> {
    let (envelope, fps) = onset_envelope(mono, rate);
    let (min, max) = (range.min.max(1.0), range.max.max(range.min.max(1.0) + 1.0));
    let max_lag = (COMB[COMB.len() - 1] * 60.0 * fps / min).ceil() as usize + 2;
    if envelope.len() < 2 * (60.0 * fps / min) as usize {
        return None;
    }
    let acf = autocorrelation(&envelope, max_lag.min(envelope.len() - 1));
    if acf[0] <= 0.0 {
        return None;
    }
    let salience = |bpm: f32| -> f32 {
        let period = 60.0 * fps / bpm;
        COMB.iter()
            .map(|&k| interpolate(&acf, k * period) / k.sqrt())
            .sum()
    };

    // Coarse grid, then a fine search around the best point.
    let coarse = 0.25;
    let mut best = (min, f32::MIN);
    let mut total = 0.0;
    let mut count = 0usize;
    let mut bpm = min;
    while bpm <= max {
        let s = salience(bpm);
        total += s;
        count += 1;
        if s > best.1 {
            best = (bpm, s);
        }
        bpm += coarse;
    }
    let mut fine = best;
    let mut bpm = (best.0 - coarse).max(min);
    while bpm <= (best.0 + coarse).min(max) {
        let s = salience(bpm);
        if s > fine.1 {
            fine = (bpm, s);
        }
        bpm += 0.01;
    }
    let mean = total / count as f32;
    if fine.1 <= 0.0 || mean <= 0.0 {
        return None;
    }
    Some(Tempo {
        bpm: (fine.0 * 100.0).round() / 100.0,
        confidence: fine.1 / mean,
    })
}

/// Onset strength per frame and the frame rate.
pub fn onset_envelope(mono: &[f32], rate: u32) -> (Vec<f32>, f32) {
    let (signal, rate) = decimate(mono, rate);
    let fps = rate / HOP as f32;
    let spectra = spectrogram(&signal, FRAME, HOP);
    let log: Vec<Vec<f32>> = spectra
        .iter()
        .map(|frame| frame.iter().map(|m| (1.0 + 100.0 * m).ln()).collect())
        .collect();
    let mut flux = vec![0.0f32; log.len()];
    for t in 1..log.len() {
        flux[t] = log[t]
            .iter()
            .zip(&log[t - 1])
            .skip(1)
            .map(|(a, b)| (a - b).max(0.0))
            .sum();
    }
    // Remove the slowly varying part so sustained loudness does not count as onsets.
    let half = (0.1 * fps) as usize;
    let envelope = (0..flux.len())
        .map(|t| {
            let lo = t.saturating_sub(half);
            let hi = (t + half + 1).min(flux.len());
            let mean = flux[lo..hi].iter().sum::<f32>() / (hi - lo) as f32;
            (flux[t] - mean).max(0.0)
        })
        .collect();
    (envelope, fps)
}

/// Unbiased autocorrelation for lags 0..=max_lag.
fn autocorrelation(x: &[f32], max_lag: usize) -> Vec<f32> {
    (0..=max_lag)
        .map(|lag| {
            let n = x.len() - lag;
            x[..n]
                .iter()
                .zip(&x[lag..])
                .map(|(a, b)| a * b)
                .sum::<f32>()
                / n as f32
        })
        .collect()
}

fn interpolate(values: &[f32], at: f32) -> f32 {
    let i = at.floor() as usize;
    let frac = at - i as f32;
    match (values.get(i), values.get(i + 1)) {
        (Some(a), Some(b)) => a + (b - a) * frac,
        (Some(a), None) => *a,
        _ => 0.0,
    }
}

/// "Set BPM by St/End": the whole-beat tempo nearest the pad's current BPM, for a region
/// of `frames` frames at 48 kHz. `current_x100` is BPM × 100 (0 counts as 96.00). Returns
/// BPM × 100, truncated, as the official app computes it.
pub fn bpm_from_length(frames: u32, current_x100: u32) -> Option<u32> {
    if frames == 0 {
        return None;
    }
    let target = if current_x100 == 0 {
        9600
    } else {
        current_x100
    };
    // BPM × 100 for `num / den` beats spanning the region.
    let candidate = |num: u64, den: u64| -> u64 { num * 288_000_000 / (den * u64::from(frames)) };
    let pick = |below: Option<(u64, u64)>, above: Option<(u64, u64)>| -> Option<u32> {
        // (value, beats rank); a tie goes to fewer beats, i.e. the lower value.
        let t = u64::from(target);
        match (below, above) {
            (Some((b, _)), Some((a, _))) => Some(if a - t < t - b { a } else { b }),
            (Some((b, _)), None) => Some(b),
            (None, Some((a, _))) => Some(a),
            (None, None) => None,
        }
        .map(|v| v as u32)
    };
    if candidate(4, 1) < u64::from(target) {
        let mut beats = 4;
        loop {
            let (low, high) = (candidate(beats, 1), candidate(beats + 4, 1));
            if high >= u64::from(target) {
                return pick(Some((low, beats)), Some((high, beats + 4)));
            }
            beats += 4;
        }
    }
    // Short regions: powers of two from 1/64 to 4 beats.
    let (mut below, mut above) = (None, None);
    for exp in -6i32..=2 {
        let (num, den) = if exp < 0 {
            (1, 1u64 << -exp)
        } else {
            (1u64 << exp, 1)
        };
        let value = candidate(num, den);
        if value < u64::from(target) {
            below = Some((value, num));
        } else if above.is_none() {
            above = Some((value, num));
        }
    }
    // The app rejects an upper choice above 300.00 BPM.
    let above = above.filter(|&(v, _)| v <= 30_000);
    pick(below, above)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A drum loop: kick on every beat, snare on 2 and 4, closed hat on eighths.
    fn drum_loop(bpm: f32, bars: usize, rate: u32) -> Vec<f32> {
        let beat = 60.0 / bpm;
        let len = (bars as f32 * 4.0 * beat * rate as f32) as usize;
        let mut out = vec![0.0f32; len];
        let mut noise = 0x1234_5678u32;
        let mut rand = move || {
            noise ^= noise << 13;
            noise ^= noise >> 17;
            noise ^= noise << 5;
            (noise as f32 / u32::MAX as f32) * 2.0 - 1.0
        };
        for eighth in 0..bars * 8 {
            let start = (eighth as f32 * beat / 2.0 * rate as f32) as usize;
            let on_beat = eighth % 2 == 0;
            let backbeat = eighth % 4 == 2;
            for i in 0..(0.25 * rate as f32) as usize {
                let Some(s) = out.get_mut(start + i) else {
                    break;
                };
                let t = i as f32 / rate as f32;
                if on_beat {
                    *s += (2.0 * std::f32::consts::PI * 60.0 * t).sin() * (-t * 20.0).exp() * 0.8;
                }
                if backbeat {
                    *s += rand() * (-t * 25.0).exp() * 0.5;
                }
                *s += rand() * (-t * 80.0).exp() * 0.2;
            }
        }
        out
    }

    #[test]
    fn finds_tempo_of_drum_loops() {
        for (bpm, range) in [
            (90.0, TempoRange::device_preset(2)),
            (123.5, TempoRange::device_preset(2)),
            (100.0, TempoRange::device_preset(2)),
            (
                172.0,
                TempoRange {
                    min: 100.0,
                    max: 200.0,
                },
            ),
            (
                140.0,
                TempoRange {
                    min: 100.0,
                    max: 200.0,
                },
            ),
        ] {
            let audio = drum_loop(bpm, 4, 48_000);
            let found = detect(&audio, 48_000, range).expect("tempo");
            assert!(
                (found.bpm - bpm).abs() < 0.3,
                "expected {bpm}, got {:.2} (confidence {:.2})",
                found.bpm,
                found.confidence
            );
        }
    }

    #[test]
    fn silence_has_no_tempo() {
        assert_eq!(
            detect(&vec![0.0; 48_000 * 4], 48_000, TempoRange::default()),
            None
        );
        assert_eq!(detect(&[0.1; 100], 48_000, TempoRange::default()), None);
    }

    #[test]
    fn bpm_by_length_follows_app_rule() {
        // B1: 512,000 frames. From 172.30, 32 beats (180.00) is closer than 28 (157.50).
        assert_eq!(bpm_from_length(512_000, 17_230), Some(18_000));
        assert_eq!(bpm_from_length(512_000, 9_000), Some(9_000));
        // 0 counts as 96.00: 16 beats = 90.00 beats 20 beats = 112.50.
        assert_eq!(bpm_from_length(512_000, 0), Some(9_000));
        // A 49,153-frame hit: 4 beats is already 234.36, so try powers of two.
        assert_eq!(bpm_from_length(49_153, 9_000), Some(11_718));
        assert_eq!(bpm_from_length(0, 9_000), None);
    }

    #[test]
    fn app_rounding_and_presets() {
        let t = Tempo {
            bpm: 172.34,
            confidence: 2.0,
        };
        assert_eq!(t.bpm_x100(), 17_230);
        assert_eq!(
            TempoRange::device_preset(2),
            TempoRange {
                min: 69.0,
                max: 139.0
            }
        );
    }
}
