//! SP-404MKII card file formats. Layouts are specified in `docs/re/02-files.md`.

pub mod audio;
pub mod padconf;
pub mod pattern;
pub mod smf;
pub mod smp;
pub mod wav;

pub use padconf::Padconf;
pub use pattern::Pattern;
pub use smp::Sample;

#[derive(Debug, thiserror::Error)]
pub enum FormatError {
    #[error("file too short ({0} bytes)")]
    TooShort(usize),
    #[error("wrong file type (magic bytes do not match)")]
    BadMagic,
    #[error("unsupported SMP format: {rate} Hz, {channels} channels, {bits} bits")]
    UnsupportedSmp { rate: u32, channels: u32, bits: u32 },
    #[error("SMP size field says {declared} bytes of audio, file has {actual}")]
    SizeMismatch { declared: u64, actual: u64 },
    #[error("SMP header digest does not match its fields")]
    DigestMismatch,
    #[error("invalid pattern file: {0}")]
    BadPattern(&'static str),
    #[error("no audio track found")]
    NoAudioTrack,
    #[error("file has no audio")]
    Empty,
    #[error("decoding audio: {0}")]
    Decode(#[from] symphonia::core::errors::Error),
    #[error("resampling: {0}")]
    Resample(String),
    #[error("{0}")]
    Proto(#[from] sp404_proto::ProtoError),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("WAV: {0}")]
    Wav(#[from] hound::Error),
}
