//! Key estimation: a pitch-class profile from spectral peaks, matched against major and
//! minor key profiles (Krumhansl–Kessler probe-tone ratings).

use crate::signal::{decimate, spectrogram};

const FRAME: usize = 4096;
const HOP: usize = 2048;
const MIN_HZ: f32 = 55.0;
const MAX_HZ: f32 = 4_000.0;

const MAJOR: [f32; 12] = [
    6.35, 2.23, 3.48, 2.33, 4.38, 4.09, 2.52, 5.19, 2.39, 3.66, 2.29, 2.88,
];
const MINOR: [f32; 12] = [
    6.33, 2.68, 3.52, 5.38, 2.60, 3.53, 2.54, 4.75, 3.98, 2.69, 3.34, 3.17,
];

/// Pitch-class names as the official app spells them in its key list.
const NAMES: [&str; 12] = [
    "C", "Db", "D", "Eb", "E", "F", "F#", "G", "Ab", "A", "Bb", "B",
];

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Key {
    /// Pitch class of the tonic, 0 = C.
    pub tonic: u8,
    pub minor: bool,
    /// Correlation with the best key profile (−1..1).
    pub confidence: f32,
}

impl Key {
    /// "F# min", "C maj".
    pub fn name(&self) -> String {
        format!(
            "{} {}",
            NAMES[usize::from(self.tonic % 12)],
            if self.minor { "min" } else { "maj" }
        )
    }

    /// Position on the Camelot wheel, 1..=12 (minor keys are "A", major keys "B").
    pub fn camelot_number(&self) -> u8 {
        // Minor keys start at Ab (1A), major keys at B (1B); each step adds a fifth.
        let first = if self.minor { 8 } else { 11 };
        (0..12u8)
            .find(|n| (first + 7 * n) % 12 == self.tonic % 12)
            .unwrap()
            + 1
    }

    /// "4A" for F minor, "8B" for C major.
    pub fn camelot(&self) -> String {
        format!(
            "{}{}",
            self.camelot_number(),
            if self.minor { 'A' } else { 'B' }
        )
    }

    /// Value of the device's pad key parameter (`89`): 1..=24 in Camelot order
    /// (1 = 1A Ab minor, 2 = 1B B major, …); 0 means no key. Unverified on the device.
    pub fn device_value(&self) -> u8 {
        (self.camelot_number() - 1) * 2 + if self.minor { 1 } else { 2 }
    }
}

/// Estimate the key of mono audio. `None` for silence or audio shorter than one frame.
pub fn detect(mono: &[f32], rate: u32) -> Option<Key> {
    let chroma = chroma(mono, rate)?;
    let mut best: Option<Key> = None;
    for tonic in 0..12u8 {
        for (minor, profile) in [(false, &MAJOR), (true, &MINOR)] {
            let rotated: Vec<f32> = (0..12)
                .map(|pc| profile[(pc + 12 - usize::from(tonic)) % 12])
                .collect();
            let r = pearson(&chroma, &rotated);
            if best.is_none_or(|b| r > b.confidence) {
                best = Some(Key {
                    tonic,
                    minor,
                    confidence: r,
                });
            }
        }
    }
    best
}

/// Energy per pitch class (0 = C) over the whole signal, normalised to sum 1.
pub fn chroma(mono: &[f32], rate: u32) -> Option<[f32; 12]> {
    let (signal, rate) = decimate(mono, rate);
    let bin_hz = rate / FRAME as f32;
    let mut chroma = [0.0f32; 12];
    for frame in spectrogram(&signal, FRAME, HOP) {
        let peak = frame.iter().copied().fold(0.0f32, f32::max);
        if peak <= 1e-6 {
            continue;
        }
        let lo = (MIN_HZ / bin_hz).ceil() as usize;
        let hi = ((MAX_HZ / bin_hz) as usize).min(frame.len() - 2);
        for k in lo.max(1)..=hi {
            let (a, b, c) = (frame[k - 1], frame[k], frame[k + 1]);
            if b < a || b < c || b < peak * 0.02 {
                continue;
            }
            // Parabolic interpolation of the peak position.
            let denom = a - 2.0 * b + c;
            let offset = if denom.abs() > f32::EPSILON {
                0.5 * (a - c) / denom
            } else {
                0.0
            };
            let hz = (k as f32 + offset) * bin_hz;
            let pitch = 12.0 * (hz / 440.0).log2() + 69.0;
            let lower = pitch.floor();
            let frac = pitch - lower;
            let weight = b.sqrt();
            let pc = (lower as i32).rem_euclid(12) as usize;
            chroma[pc] += weight * (1.0 - frac);
            chroma[(pc + 1) % 12] += weight * frac;
        }
    }
    let total: f32 = chroma.iter().sum();
    if total <= 0.0 {
        return None;
    }
    Some(chroma.map(|c| c / total))
}

fn pearson(a: &[f32], b: &[f32]) -> f32 {
    let n = a.len() as f32;
    let (ma, mb) = (a.iter().sum::<f32>() / n, b.iter().sum::<f32>() / n);
    let cov: f32 = a.iter().zip(b).map(|(x, y)| (x - ma) * (y - mb)).sum();
    let va: f32 = a.iter().map(|x| (x - ma).powi(2)).sum();
    let vb: f32 = b.iter().map(|y| (y - mb).powi(2)).sum();
    if va <= 0.0 || vb <= 0.0 {
        0.0
    } else {
        cov / (va * vb).sqrt()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;

    fn midi_hz(note: i32) -> f32 {
        440.0 * 2f32.powf((note - 69) as f32 / 12.0)
    }

    /// Chords (MIDI note lists), one second each, with a few harmonics per note.
    fn progression(chords: &[&[i32]]) -> Vec<f32> {
        let mut out = Vec::new();
        for chord in chords {
            for i in 0..RATE as usize {
                let t = i as f32 / RATE as f32;
                let env = (t * 30.0).min(1.0) * (-t * 1.5).exp();
                let s: f32 = chord
                    .iter()
                    .flat_map(|&n| {
                        (1..=4).map(move |h| {
                            (2.0 * std::f32::consts::PI * midi_hz(n) * h as f32 * t).sin()
                                / h as f32
                        })
                    })
                    .sum();
                out.push(s * env * 0.1);
            }
        }
        out
    }

    fn key_of(chords: &[&[i32]]) -> String {
        detect(&progression(chords), RATE).unwrap().name()
    }

    #[test]
    fn major_and_minor_progressions() {
        // I–IV–V–I in C major, with bass notes.
        assert_eq!(
            key_of(&[
                &[48, 60, 64, 67],
                &[41, 60, 65, 69],
                &[43, 62, 67, 71],
                &[48, 60, 64, 67]
            ]),
            "C maj"
        );
        // i–iv–V–i in A minor.
        assert_eq!(
            key_of(&[
                &[45, 57, 60, 64],
                &[38, 57, 62, 65],
                &[40, 56, 59, 64],
                &[45, 57, 60, 64]
            ]),
            "A min"
        );
        // i–iv–V–i in F minor.
        assert_eq!(
            key_of(&[
                &[41, 53, 56, 60],
                &[46, 53, 58, 61],
                &[48, 55, 60, 64],
                &[41, 53, 56, 60]
            ]),
            "F min"
        );
    }

    #[test]
    fn every_major_key_by_transposition() {
        for shift in 0..12 {
            let c = |n: i32| n + shift;
            let chords: [&[i32]; 4] = [
                &[c(48), c(60), c(64), c(67)],
                &[c(41), c(60), c(65), c(69)],
                &[c(43), c(62), c(67), c(71)],
                &[c(48), c(60), c(64), c(67)],
            ];
            let key = detect(&progression(&chords), RATE).unwrap();
            assert_eq!(
                (key.tonic, key.minor),
                (shift as u8, false),
                "shift {shift}"
            );
        }
    }

    #[test]
    fn camelot_positions_match_app_order() {
        let k = |tonic, minor| Key {
            tonic,
            minor,
            confidence: 1.0,
        };
        assert_eq!(
            (k(8, true).camelot(), k(8, true).device_value()),
            ("1A".into(), 1)
        );
        assert_eq!(
            (k(11, false).camelot(), k(11, false).device_value()),
            ("1B".into(), 2)
        );
        assert_eq!(
            (k(5, true).camelot(), k(5, true).device_value()),
            ("4A".into(), 7)
        );
        assert_eq!(
            (k(0, false).camelot(), k(0, false).device_value()),
            ("8B".into(), 16)
        );
        assert_eq!(
            (k(4, false).camelot(), k(4, false).device_value()),
            ("12B".into(), 24)
        );
    }

    #[test]
    fn silence_has_no_key() {
        assert_eq!(detect(&vec![0.0; RATE as usize], RATE), None);
    }
}
