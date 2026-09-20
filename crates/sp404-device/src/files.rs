//! Remote file API on the device's SD card (channel 6).

use std::time::Duration;

use sp404_proto::fileapi::{self, Reply};
use sp404_proto::{Channel, Message};

use crate::{Device, Error, Result, is_channel};

pub use sp404_proto::fileapi::{DirEntry, Stat};

const TIMEOUT: Duration = Duration::from_secs(5);

impl Device {
    fn file_call(&self, payload: Vec<u8>, what: &str) -> Result<Reply> {
        let msg = Message::long(Channel::File, payload);
        // Short messages on this channel are notifications, never replies.
        let reply = self.transact(&msg, what, TIMEOUT, |m| {
            is_channel(m, Channel::File) && !m.is_short()
        })?;
        Ok(fileapi::parse_reply(reply.payload())?)
    }

    fn status_call(&self, payload: Vec<u8>, what: &str) -> Result<i32> {
        match self.file_call(payload, what)? {
            Reply::Status { result: -1, .. } => Err(Error::Refused(what.to_string())),
            Reply::Status { result, .. } => Ok(result),
            Reply::Data { .. } => Err(Error::Unexpected(what.to_string())),
        }
    }

    fn data_call(&self, payload: Vec<u8>, what: &str) -> Result<Option<Vec<u8>>> {
        match self.file_call(payload, what)? {
            Reply::Data { data, .. } => Ok(Some(data)),
            Reply::Status { result: -1, .. } => Ok(None),
            Reply::Status { .. } => Err(Error::Unexpected(what.to_string())),
        }
    }

    pub fn open_file(&self, path: &str, flags: u32) -> Result<i32> {
        self.status_call(fileapi::open(path, flags), &format!("open {path}"))
    }

    pub fn close_file(&self, handle: i32) -> Result<()> {
        self.status_call(fileapi::close(handle), "close").map(drop)
    }

    pub fn seek(&self, handle: i32, offset: u32) -> Result<u32> {
        self.status_call(fileapi::seek(handle, offset), "seek")
            .map(|r| r as u32)
    }

    pub fn fstat(&self, handle: i32) -> Result<Stat> {
        let data = self
            .data_call(fileapi::fstat(handle), "fstat")?
            .ok_or_else(|| Error::Refused("fstat".into()))?;
        Ok(Stat::parse(&data)?)
    }

    /// `None` when the path does not exist.
    pub fn stat(&self, path: &str) -> Result<Option<Stat>> {
        match self.data_call(fileapi::stat(path), &format!("stat {path}"))? {
            Some(data) => Ok(Some(Stat::parse(&data)?)),
            None => Ok(None),
        }
    }

    pub fn unlink(&self, path: &str) -> Result<()> {
        self.status_call(fileapi::unlink(path), &format!("unlink {path}"))
            .map(drop)
    }

    /// Create a directory; fails if the name is taken.
    pub fn mkdir(&self, path: &str) -> Result<()> {
        self.status_call(fileapi::mkdir(path), &format!("mkdir {path}"))
            .map(drop)
    }

    /// Remove a directory; fails unless it is empty.
    pub fn rmdir(&self, path: &str) -> Result<()> {
        self.status_call(fileapi::rmdir(path), &format!("rmdir {path}"))
            .map(drop)
    }

    /// Move or rename a file or directory within the card.
    pub fn rename(&self, from: &str, to: &str) -> Result<()> {
        self.status_call(fileapi::rename(from, to), &format!("rename {from}"))
            .map(drop)
    }

    pub fn free_kb(&self) -> Result<u32> {
        self.status_call(fileapi::free_kb(fileapi::ROOT), "free space")
            .map(|r| r as u32)
    }

    /// Read up to `count` bytes; the device answers in chunks of at most [`fileapi::CHUNK`].
    pub fn read(&self, handle: i32, count: u32) -> Result<Vec<u8>> {
        let msg = Message::long(Channel::File, fileapi::read(handle, count));
        let mut out = Vec::with_capacity(count as usize);
        let mut inbox = self.inbox.lock().unwrap();
        self.send(&msg)?;
        loop {
            let reply = Self::wait(&mut inbox, "read", TIMEOUT, &mut |m: &Message| {
                is_channel(m, Channel::File) && !m.is_short()
            })?;
            match fileapi::parse_reply(reply.payload())? {
                Reply::Data { data, last, .. } => {
                    out.extend_from_slice(&data);
                    if last {
                        return Ok(out);
                    }
                }
                Reply::Status { result: -1, .. } => return Err(Error::Refused("read".into())),
                Reply::Status { .. } => return Ok(out),
            }
        }
    }

    /// Write `data` at the current position in batches, checking the device's byte counts.
    pub fn write(&self, handle: i32, data: &[u8]) -> Result<()> {
        self.write_with(handle, data, |_, _| {})
    }

    /// Like [`Device::write`], calling `progress(done, total)` after each batch.
    pub fn write_with(
        &self,
        handle: i32,
        data: &[u8],
        mut progress: impl FnMut(u64, u64),
    ) -> Result<()> {
        let total = data.len() as u64;
        let mut done = 0u64;
        let chunks: Vec<&[u8]> = data.chunks(fileapi::CHUNK).collect();
        for batch in chunks.chunks(fileapi::WRITE_BATCH) {
            let expected: usize = batch.iter().map(|c| c.len()).sum();
            let mut inbox = self.inbox.lock().unwrap();
            for (i, chunk) in batch.iter().enumerate() {
                let last = i + 1 == batch.len();
                self.send(&Message::long(
                    Channel::File,
                    fileapi::write(handle, chunk, last),
                ))?;
            }
            let reply = Self::wait(&mut inbox, "write", TIMEOUT, &mut |m: &Message| {
                is_channel(m, Channel::File) && !m.is_short()
            })?;
            match fileapi::parse_reply(reply.payload())? {
                Reply::Status { result, .. } if result as usize == expected => {}
                Reply::Status { result, .. } => {
                    return Err(Error::Refused(format!(
                        "write ({result} of {expected} bytes accepted)"
                    )));
                }
                Reply::Data { .. } => return Err(Error::Unexpected("write".into())),
            }
            done += expected as u64;
            progress(done, total);
        }
        Ok(())
    }

    /// Read a whole file.
    pub fn read_file(&self, path: &str) -> Result<Vec<u8>> {
        self.read_file_with(path, |_, _| {})
    }

    /// Like [`Device::read_file`], calling `progress(done, total)` as blocks arrive.
    pub fn read_file_with(
        &self,
        path: &str,
        mut progress: impl FnMut(u64, u64),
    ) -> Result<Vec<u8>> {
        let handle = self.open_file(path, fileapi::flags::READ_ONLY)?;
        let result = (|| -> Result<Vec<u8>> {
            let size = self.fstat(handle)?.size;
            let mut out = Vec::with_capacity(size as usize);
            while out.len() < size as usize {
                let want = (size as usize - out.len()).min(fileapi::BULK_READ as usize) as u32;
                let chunk = self.read(handle, want)?;
                if chunk.is_empty() {
                    break;
                }
                out.extend_from_slice(&chunk);
                progress(out.len() as u64, u64::from(size));
            }
            Ok(out)
        })();
        let closed = self.close_file(handle);
        let data = result?;
        closed?;
        Ok(data)
    }

    /// Create or replace a file with `data`.
    pub fn write_file(&self, path: &str, data: &[u8]) -> Result<()> {
        self.write_file_with(path, data, |_, _| {})
    }

    /// Like [`Device::write_file`], calling `progress(done, total)` as batches go out.
    pub fn write_file_with(
        &self,
        path: &str,
        data: &[u8],
        progress: impl FnMut(u64, u64),
    ) -> Result<()> {
        let handle = self.open_file(path, fileapi::flags::CREATE_TRUNCATE_READ_WRITE)?;
        let written = self.write_with(handle, data, progress);
        let closed = self.close_file(handle);
        written?;
        closed
    }

    /// Read the first `len` bytes of a file; `None` if it does not exist.
    pub fn read_prefix(&self, path: &str, len: u32) -> Result<Option<Vec<u8>>> {
        if self.stat(path)?.is_none() {
            return Ok(None);
        }
        let handle = self.open_file(path, fileapi::flags::READ_ONLY)?;
        let data = self.read(handle, len);
        let closed = self.close_file(handle);
        let data = data?;
        closed?;
        Ok(Some(data))
    }

    /// List a directory, without `.` and `..`.
    pub fn list_dir(&self, path: &str) -> Result<Vec<DirEntry>> {
        let dir = self.status_call(fileapi::opendir(path), &format!("opendir {path}"))?;
        let mut entries = Vec::new();
        let result = loop {
            match self.data_call(fileapi::readdir(dir), "readdir") {
                Ok(Some(data)) => {
                    let entry = DirEntry::parse(&data)?;
                    if !entry.is_dot() {
                        entries.push(entry);
                    }
                }
                Ok(None) => break Ok(()),
                Err(e) => break Err(e),
            }
        };
        self.status_call(fileapi::closedir(dir), "closedir")?;
        result.map(|()| entries)
    }
}
