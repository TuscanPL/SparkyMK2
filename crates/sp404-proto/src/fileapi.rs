//! Channel 6: the remote file API on the SD card.
//!
//! Payloads are SysEx-shaped (`F0 41 7A <cmd> … F7`). Requests use command `0x03`;
//! the device answers with a status (`0x7A`) or a data reply (`0x02`).

use crate::ProtoError;
use crate::septet::{self, Septets};

/// Prefix that routes a path to the device's card.
pub const ROOT: &str = "/SP404REMOTE//";

/// Largest data block in one write request or one read reply.
pub const CHUNK: usize = 0x5000;

/// Write chunks sent before waiting for the device's byte count.
pub const WRITE_BATCH: usize = 15;

/// Read size the official app uses for bulk reads.
pub const BULK_READ: u32 = 0x20000;

const HEAD: [u8; 3] = [0xF0, 0x41, 0x7A];
const END: u8 = 0xF7;

const CMD_REQUEST: u8 = 0x03;
const CMD_STATUS: u8 = 0x7A;
const CMD_DATA: u8 = 0x02;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Op {
    Open = 0x00,
    Close = 0x03,
    Read = 0x04,
    Write = 0x06,
    Seek = 0x07,
    Unlink = 0x0A,
    OpenDir = 0x0C,
    CloseDir = 0x0D,
    ReadDir = 0x0E,
    Stat = 0x12,
    FStat = 0x13,
    Rename = 0x17,
    FreeKb = 0x18,
}

/// Open flags (newlib values).
pub mod flags {
    pub const READ_ONLY: u32 = 0;
    pub const CREATE_TRUNCATE_READ_WRITE: u32 = 0x602;
}

/// Mode bits returned by stat.
pub const MODE_TYPE_MASK: u32 = 0xF000;
pub const MODE_DIR: u32 = 0x4000;
pub const MODE_FILE: u32 = 0x8000;

/// Directory entry type values (Linux `d_type`).
pub const DT_DIR: u8 = 4;
pub const DT_REG: u8 = 8;

/// Turn a card-relative path (`ROLAND/SP-404MKII/PROJECT_06`) into a remote path.
pub fn remote_path(path: &str) -> String {
    if path.starts_with(ROOT) {
        return path.to_string();
    }
    let rel = path.trim_start_matches(['/', '\\']);
    format!("{ROOT}/{rel}")
}

fn request(op: Op, first: [u8; 5], second: [u8; 5], tail: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(3 + 12 + tail.len() + 1);
    out.extend_from_slice(&HEAD);
    out.push(CMD_REQUEST);
    out.push(op as u8);
    out.extend_from_slice(&first);
    out.extend_from_slice(&second);
    out.extend_from_slice(tail);
    out.push(END);
    out
}

fn path_bytes(path: &str) -> Vec<u8> {
    let mut bytes = remote_path(path).into_bytes();
    bytes.push(0);
    bytes
}

fn path_request(op: Op, first: i32, path: &str) -> Vec<u8> {
    let bytes = path_bytes(path);
    request(
        op,
        septet::encode(first),
        septet::encode(bytes.len() as i32),
        &bytes,
    )
}

fn handle_request(op: Op, handle: i32, arg: [u8; 5]) -> Vec<u8> {
    request(op, septet::encode(handle), arg, &[0])
}

pub fn open(path: &str, open_flags: u32) -> Vec<u8> {
    path_request(Op::Open, open_flags as i32, path)
}

pub fn close(handle: i32) -> Vec<u8> {
    handle_request(Op::Close, handle, septet::encode(0))
}

pub fn read(handle: i32, count: u32) -> Vec<u8> {
    let flags = if count as usize > CHUNK {
        septet::FLAG_MULTI
    } else {
        0
    };
    handle_request(Op::Read, handle, septet::encode_len(count, flags))
}

/// One write chunk (at most [`CHUNK`] bytes). `last_in_batch` asks the device to reply.
pub fn write(handle: i32, data: &[u8], last_in_batch: bool) -> Vec<u8> {
    debug_assert!(data.len() <= CHUNK);
    let flags = if last_in_batch { septet::FLAG_LAST } else { 0 };
    request(
        Op::Write,
        septet::encode(handle),
        septet::encode_len(data.len() as u32, flags),
        data,
    )
}

/// Seek to an absolute offset. The status result is the new offset.
pub fn seek(handle: i32, offset: u32) -> Vec<u8> {
    handle_request(Op::Seek, handle, septet::encode(offset as i32))
}

pub fn fstat(handle: i32) -> Vec<u8> {
    handle_request(Op::FStat, handle, septet::encode(0))
}

pub fn stat(path: &str) -> Vec<u8> {
    path_request(Op::Stat, 0, path)
}

pub fn unlink(path: &str) -> Vec<u8> {
    path_request(Op::Unlink, 0, path)
}

pub fn opendir(path: &str) -> Vec<u8> {
    path_request(Op::OpenDir, 0, path)
}

pub fn readdir(dir: i32) -> Vec<u8> {
    handle_request(Op::ReadDir, dir, septet::encode(0))
}

pub fn closedir(dir: i32) -> Vec<u8> {
    handle_request(Op::CloseDir, dir, septet::encode(0))
}

pub fn free_kb(path: &str) -> Vec<u8> {
    path_request(Op::FreeKb, 0, path)
}

/// Rename: two paths, each NUL-padded into a 200-byte slot.
pub fn rename(from: &str, to: &str) -> Vec<u8> {
    let mut block = vec![0u8; 400];
    for (slot, path) in [(0, from), (200, to)] {
        let bytes = remote_path(path).into_bytes();
        let n = bytes.len().min(199);
        block[slot..slot + n].copy_from_slice(&bytes[..n]);
    }
    request(Op::Rename, septet::encode(0), septet::encode(400), &block)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reply {
    /// Result of an operation: a handle, a new offset, a byte count, 0, or −1 on failure.
    Status { result: i32, extra: i32 },
    /// A block of data (read, stat, readdir).
    Data {
        handle: i32,
        last: bool,
        data: Vec<u8>,
    },
}

impl Reply {
    pub fn is_failure(&self) -> bool {
        matches!(self, Reply::Status { result: -1, .. })
    }
}

pub fn parse_reply(payload: &[u8]) -> Result<Reply, ProtoError> {
    if payload.len() < 5 || payload[..3] != HEAD || payload[payload.len() - 1] != END {
        return Err(ProtoError::NotSysex);
    }
    let body = &payload[3..payload.len() - 1];
    let need = 1 + 2 * septet::LEN;
    if body.len() < need {
        return Err(ProtoError::Truncated {
            need,
            have: body.len(),
        });
    }
    let first = septet::decode(&body[1..])?;
    let second: Septets = septet::decode(&body[1 + septet::LEN..])?;
    match body[0] {
        CMD_STATUS => Ok(Reply::Status {
            result: first.as_i32(),
            extra: second.as_i32(),
        }),
        CMD_DATA => {
            let len = second.as_u32() as usize;
            let data = body.get(need..need + len).ok_or(ProtoError::Truncated {
                need: need + len,
                have: body.len(),
            })?;
            Ok(Reply::Data {
                handle: first.as_i32(),
                last: second.is_last(),
                data: data.to_vec(),
            })
        }
        other => Err(ProtoError::UnexpectedCommand(other)),
    }
}

/// A request the device sends to the host while it writes rendered audio into a host file
/// (pattern Bounce/MULTIPAD export). Same layout as host requests, opposite direction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostRequest {
    Write { handle: i32, data: Vec<u8> },
    Seek { handle: i32, offset: u32 },
}

pub fn parse_host_request(payload: &[u8]) -> Result<HostRequest, ProtoError> {
    if payload.len() < 5 || payload[..3] != HEAD || payload[payload.len() - 1] != END {
        return Err(ProtoError::NotSysex);
    }
    let body = &payload[3..payload.len() - 1];
    let need = 2 + 2 * septet::LEN;
    if body.len() < need {
        return Err(ProtoError::Truncated {
            need,
            have: body.len(),
        });
    }
    if body[0] != CMD_REQUEST {
        return Err(ProtoError::UnexpectedCommand(body[0]));
    }
    let handle = septet::decode(&body[2..])?.as_i32();
    let arg = septet::decode(&body[2 + septet::LEN..])?;
    match body[1] {
        op if op == Op::Write as u8 => {
            let len = arg.as_u32() as usize;
            let data = body.get(need..need + len).ok_or(ProtoError::Truncated {
                need: need + len,
                have: body.len(),
            })?;
            Ok(HostRequest::Write {
                handle,
                data: data.to_vec(),
            })
        }
        op if op == Op::Seek as u8 => Ok(HostRequest::Seek {
            handle,
            offset: arg.as_u32(),
        }),
        other => Err(ProtoError::UnexpectedCommand(other)),
    }
}

/// First answer to a [`HostRequest`] (sent as a short message).
pub fn host_ack() -> Vec<u8> {
    vec![0xF0, 0x41, 0x7A, 0x7C, 0x00, END]
}

/// Second answer to a [`HostRequest`]: bytes written or the new offset (−1 on failure).
pub fn host_status(result: i32) -> Vec<u8> {
    let mut out = Vec::with_capacity(15);
    out.extend_from_slice(&HEAD);
    out.push(CMD_STATUS);
    out.extend_from_slice(&septet::encode(result));
    out.extend_from_slice(&septet::encode(-1));
    out.push(END);
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stat {
    pub mode: u32,
    pub size: u32,
}

impl Stat {
    pub fn parse(data: &[u8]) -> Result<Self, ProtoError> {
        let d = data.get(..8).ok_or(ProtoError::Truncated {
            need: 8,
            have: data.len(),
        })?;
        Ok(Stat {
            mode: u32::from_le_bytes(d[..4].try_into().unwrap()),
            size: u32::from_le_bytes(d[4..].try_into().unwrap()),
        })
    }

    pub fn is_dir(&self) -> bool {
        self.mode & MODE_TYPE_MASK == MODE_DIR
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntry {
    pub name: String,
    pub kind: u8,
}

impl DirEntry {
    pub fn parse(data: &[u8]) -> Result<Self, ProtoError> {
        let header = data.get(..6).ok_or(ProtoError::Truncated {
            need: 6,
            have: data.len(),
        })?;
        let len = header[4] as usize;
        let name = data.get(6..6 + len).ok_or(ProtoError::Truncated {
            need: 6 + len,
            have: data.len(),
        })?;
        Ok(DirEntry {
            name: String::from_utf8_lossy(name).into_owned(),
            kind: header[5],
        })
    }

    pub fn is_dir(&self) -> bool {
        self.kind == DT_DIR
    }

    pub fn is_dot(&self) -> bool {
        self.name == "." || self.name == ".."
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_requests_from_capture() {
        let write = [
            0xF0, 0x41, 0x7A, 0x03, 0x06, 0, 0, 0, 0, 0, 0x40, 0, 0, 0, 0x0C, b'R', b'I', b'F',
            b'F', 0x24, 0x40, 0x1F, 0x00, b'W', b'A', b'V', b'E', 0xF7,
        ];
        assert_eq!(
            parse_host_request(&write).unwrap(),
            HostRequest::Write {
                handle: 0,
                data: b"RIFF\x24\x40\x1f\x00WAVE".to_vec()
            }
        );
        let seek = [
            0xF0, 0x41, 0x7A, 0x03, 0x07, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x0C, 0x00, 0xF7,
        ];
        assert_eq!(
            parse_host_request(&seek).unwrap(),
            HostRequest::Seek {
                handle: 0,
                offset: 12
            }
        );
        assert_eq!(
            host_status(0x2C),
            [
                0xF0, 0x41, 0x7A, 0x7A, 0, 0, 0, 0, 0x2C, 0x7F, 0x7F, 0x7F, 0x7F, 0x7F, 0xF7
            ]
        );
    }

    #[test]
    fn open_request_matches_spec_layout() {
        let req = open("ROLAND/SP-404MKII/PROJECT_06/PADCONF.BIN", flags::READ_ONLY);
        let path = b"/SP404REMOTE///ROLAND/SP-404MKII/PROJECT_06/PADCONF.BIN\0";
        assert_eq!(&req[..5], &[0xF0, 0x41, 0x7A, 0x03, 0x00]);
        assert_eq!(&req[5..10], &[0; 5]);
        assert_eq!(&req[10..15], &septet::encode(path.len() as i32));
        assert_eq!(&req[15..15 + path.len()], path);
        assert_eq!(*req.last().unwrap(), 0xF7);
    }

    #[test]
    fn read_and_seek_requests() {
        assert_eq!(
            read(0x2b, 512),
            vec![
                0xF0, 0x41, 0x7A, 3, 4, 0, 0, 0, 0, 0x2b, 0, 0, 0, 4, 0, 0, 0xF7
            ]
        );
        assert_eq!(&read(0x2c, BULK_READ)[10..15], &[0x20, 0, 0x08, 0, 0]);
        assert_eq!(
            seek(0x2c, 12),
            vec![
                0xF0, 0x41, 0x7A, 3, 7, 0, 0, 0, 0, 0x2c, 0, 0, 0, 0, 12, 0, 0xF7
            ]
        );
    }

    #[test]
    fn write_request_flags_last_chunk() {
        let req = write(0x2b, &[1, 2, 3], true);
        assert_eq!(
            &req[..15],
            &[0xF0, 0x41, 0x7A, 3, 6, 0, 0, 0, 0, 0x2b, 0x40, 0, 0, 0, 3]
        );
        assert_eq!(&req[15..], &[1, 2, 3, 0xF7]);
    }

    #[test]
    fn parses_status_and_data_replies() {
        let not_found = [
            0xF0, 0x41, 0x7A, 0x7A, 0x7F, 0x7F, 0x7F, 0x7F, 0x7F, 0, 0, 0, 0, 2, 0xF7,
        ];
        assert_eq!(
            parse_reply(&not_found).unwrap(),
            Reply::Status {
                result: -1,
                extra: 2
            }
        );

        let fstat = [
            0xF0, 0x41, 0x7A, 0x02, 0, 0, 0, 0, 0, 0x40, 0, 0, 0, 8, 0xff, 0x81, 0, 0, 0x44, 0x8c,
            0, 0, 0xF7,
        ];
        let Reply::Data { data, last, .. } = parse_reply(&fstat).unwrap() else {
            panic!()
        };
        assert!(last);
        let st = Stat::parse(&data).unwrap();
        assert_eq!((st.size, st.is_dir()), (35908, false));
    }

    #[test]
    fn parses_dir_entries() {
        let e = DirEntry::parse(&[
            0, 0, 0, 0, 12, 8, b'B', b'A', b'N', b'K', b'4', b'-', b'1', b'3', b'.', b'S', b'M',
            b'P', 0,
        ])
        .unwrap();
        assert_eq!(e.name, "BANK4-13.SMP");
        assert!(!e.is_dir());
        assert!(
            DirEntry::parse(&[0, 0, 0, 0, 1, 4, b'.', 0])
                .unwrap()
                .is_dot()
        );
    }

    #[test]
    fn rename_uses_fixed_slots() {
        let req = rename("a", "b");
        assert_eq!(&req[10..15], &septet::encode(400));
        assert_eq!(req.len(), 15 + 400 + 1);
        assert_eq!(&req[15..15 + 18], b"/SP404REMOTE///a\0\0");
    }
}
