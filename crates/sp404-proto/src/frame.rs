//! Message framing on the serial link.
//!
//! Short messages are 12 bytes: up to 7 payload bytes inline plus a length byte (the
//! device ignores a short message whose length byte is 0). Long messages have a 16-byte
//! header followed by a length-prefixed payload. Each direction is a byte stream: messages
//! may be split across or packed into USB transfers.

pub const KIND_SHORT: u8 = 0x12;
pub const KIND_LONG: u8 = 0x13;
pub const ADDR_HOST: u8 = 0x60;
pub const ADDR_DEVICE: u8 = 0xE0;
pub const SHORT_LEN: usize = 12;
/// Largest payload of a short message.
pub const SHORT_MAX: usize = 7;
pub const LONG_HEADER_LEN: usize = 16;

/// Largest long payload accepted by the decoder before it treats the header as garbage.
const MAX_PAYLOAD: usize = 1 << 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Channel {
    /// Control and parameters.
    Control,
    /// Remote file API.
    File,
    Other(u8),
}

impl Channel {
    pub fn from_byte(b: u8) -> Self {
        match b {
            0x05 => Channel::Control,
            0x06 => Channel::File,
            other => Channel::Other(other),
        }
    }

    pub fn to_byte(self) -> u8 {
        match self {
            Channel::Control => 0x05,
            Channel::File => 0x06,
            Channel::Other(b) => b,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Body {
    /// The 8 raw bytes: payload followed by padding, length in the last byte.
    Short([u8; 8]),
    Long(Vec<u8>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub source: u8,
    pub destination: u8,
    pub channel: Channel,
    pub body: Body,
}

impl Message {
    /// A host-to-device short message. Panics if `payload` exceeds [`SHORT_MAX`] bytes.
    pub fn short(channel: Channel, payload: &[u8]) -> Self {
        assert!(
            payload.len() <= SHORT_MAX,
            "short payload of {} bytes",
            payload.len()
        );
        let mut raw = [0u8; 8];
        raw[..payload.len()].copy_from_slice(payload);
        raw[7] = payload.len() as u8;
        Message {
            source: ADDR_HOST,
            destination: ADDR_DEVICE,
            channel,
            body: Body::Short(raw),
        }
    }

    /// A host-to-device long message.
    pub fn long(channel: Channel, payload: Vec<u8>) -> Self {
        Message {
            source: ADDR_HOST,
            destination: ADDR_DEVICE,
            channel,
            body: Body::Long(payload),
        }
    }

    pub fn payload(&self) -> &[u8] {
        match &self.body {
            Body::Short(raw) => &raw[..usize::from(raw[7]).min(SHORT_MAX)],
            Body::Long(p) => p,
        }
    }

    pub fn is_short(&self) -> bool {
        matches!(self.body, Body::Short(_))
    }

    pub fn encode(&self) -> Vec<u8> {
        let head = [self.source, self.destination, self.channel.to_byte()];
        match &self.body {
            Body::Short(p) => {
                let mut out = Vec::with_capacity(SHORT_LEN);
                out.push(KIND_SHORT);
                out.extend_from_slice(&head);
                out.extend_from_slice(p);
                out
            }
            Body::Long(p) => {
                let mut out = Vec::with_capacity(LONG_HEADER_LEN + p.len());
                out.push(KIND_LONG);
                out.extend_from_slice(&head);
                // The value and tag fields are unused by the device; send zeros.
                out.extend_from_slice(&[0u8; 8]);
                out.extend_from_slice(&(p.len() as u32).to_le_bytes());
                out.extend_from_slice(p);
                out
            }
        }
    }
}

/// Incremental decoder for one direction of the byte stream.
#[derive(Debug, Default)]
pub struct Decoder {
    buf: Vec<u8>,
    skipped: usize,
}

impl Decoder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(bytes);
    }

    /// Number of bytes discarded while resynchronising.
    pub fn skipped(&self) -> usize {
        self.skipped
    }

    /// Next complete message, if the buffer holds one.
    pub fn next_message(&mut self) -> Option<Message> {
        loop {
            let kind = *self.buf.first()?;
            let total = match kind {
                KIND_SHORT => SHORT_LEN,
                KIND_LONG => {
                    if self.buf.len() < LONG_HEADER_LEN {
                        return None;
                    }
                    let len = u32::from_le_bytes(self.buf[12..16].try_into().unwrap()) as usize;
                    if len > MAX_PAYLOAD {
                        self.skip();
                        continue;
                    }
                    LONG_HEADER_LEN + len
                }
                _ => {
                    self.skip();
                    continue;
                }
            };
            if self.buf.len() < total {
                return None;
            }
            let frame: Vec<u8> = self.buf.drain(..total).collect();
            let channel = Channel::from_byte(frame[3]);
            let body = if kind == KIND_SHORT {
                Body::Short(frame[4..12].try_into().unwrap())
            } else {
                Body::Long(frame[LONG_HEADER_LEN..].to_vec())
            };
            return Some(Message {
                source: frame[1],
                destination: frame[2],
                channel,
                body,
            });
        }
    }

    fn skip(&mut self) {
        self.buf.remove(0);
        self.skipped += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_and_long_round_trip_across_split_input() {
        let a = Message::short(Channel::Control, &[0xfe, 0x66, 0x00]);
        assert_eq!(
            a.encode(),
            vec![0x12, 0x60, 0xe0, 0x05, 0xfe, 0x66, 0, 0, 0, 0, 0, 0x03]
        );
        assert_eq!(a.payload(), &[0xfe, 0x66, 0x00]);
        let b = Message::long(Channel::File, vec![0xf0, 0x41, 0x7a, 0x03, 0xf7]);
        let mut stream = a.encode();
        stream.extend(b.encode());
        assert_eq!(stream.len(), 12 + 16 + 5);

        let mut dec = Decoder::new();
        for chunk in stream.chunks(7) {
            dec.push(chunk);
        }
        assert_eq!(dec.next_message(), Some(a));
        assert_eq!(dec.next_message(), Some(b));
        assert_eq!(dec.next_message(), None);
    }

    #[test]
    fn long_header_layout() {
        let bytes = Message::long(Channel::Control, vec![1, 2, 3]).encode();
        assert_eq!(&bytes[..4], &[0x13, 0x60, 0xe0, 0x05]);
        assert_eq!(&bytes[12..16], &3u32.to_le_bytes());
    }

    #[test]
    fn resynchronises_after_garbage() {
        let msg = Message::short(Channel::Control, &[1; 7]);
        let mut dec = Decoder::new();
        dec.push(&[0x00, 0xff]);
        dec.push(&msg.encode());
        assert_eq!(dec.next_message(), Some(msg));
        assert_eq!(dec.skipped(), 2);
    }
}
