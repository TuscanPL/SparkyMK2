//! Sans-IO implementation of the SP-404MKII serial link protocol.
//!
//! Everything here is derived from the specification in `docs/re/`:
//! framing and the file API in `01-transport.md`, parameters and pad commands in
//! `03-parameters.md`.

pub mod control;
pub mod fileapi;
pub mod frame;
pub mod pad;
pub mod params;
pub mod septet;
pub mod settings;

pub use frame::{Body, Channel, Decoder, Message};
pub use pad::PadIndex;
pub use settings::ProjectSettings;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ProtoError {
    #[error("payload too short: need {need} bytes, have {have}")]
    Truncated { need: usize, have: usize },
    #[error("byte 0x{0:02x} is not a valid septet")]
    BadSeptet(u8),
    #[error("not a file API payload (missing F0 41 7A … F7)")]
    NotSysex,
    #[error("unexpected reply command 0x{0:02x}")]
    UnexpectedCommand(u8),
    #[error("invalid pad name {0:?} (expected A1..J16)")]
    BadPad(String),
}
