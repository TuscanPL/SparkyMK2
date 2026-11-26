//! The serial link to the unit: whole writes from any thread, reads from one reader.
//!
//! On Linux and macOS this is `serialport`, with a short read timeout so the reader can
//! notice it should stop. On Windows, synchronous reads and writes on one port queue
//! behind each other, so every write used to wait for the reader's 2 ms timeout; the
//! port is opened for overlapped I/O instead, where a read waits for data without holding
//! up writes. The Windows code is adapted from Jeremy Murray-Wakefield's sp404-drive
//! (src/link/windows.rs, GPL-3.0), offered in issue #13, where it made listing a folder
//! of 149 samples take 0.3 s instead of 3.7 s.

use std::io;
use std::time::Duration;

/// A serial link to the unit.
pub(crate) trait Link: Send + Sync {
    /// Read what has arrived. On Linux and macOS this returns a `TimedOut` error now and
    /// then, so the reader can check whether to stop; on Windows it waits for data.
    fn read(&self, buf: &mut [u8]) -> io::Result<usize>;
    /// Write all of `bytes`; one caller at a time, so messages never interleave.
    fn write_all(&self, bytes: &[u8]) -> io::Result<()>;
    /// Make a waiting `read` return, so the reader thread can end.
    fn cancel_read(&self) {}
}

/// How long to wait after asserting the lines before the first message: the official
/// app pauses here, and the control channel does not answer otherwise.
const SETTLE: Duration = Duration::from_millis(350);

pub(crate) fn open(port_name: &str) -> io::Result<std::sync::Arc<dyn Link>> {
    let link = platform::open(port_name)?;
    std::thread::sleep(SETTLE);
    Ok(link)
}

#[cfg(not(windows))]
mod platform {
    use std::io::{self, Read, Write};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use serialport::SerialPort;

    use super::Link;
    use crate::BAUD;

    const READ_TIMEOUT: Duration = Duration::from_millis(50);

    struct PortLink {
        writer: Mutex<Box<dyn SerialPort>>,
        reader: Mutex<Box<dyn SerialPort>>,
    }

    impl Link for PortLink {
        fn read(&self, buf: &mut [u8]) -> io::Result<usize> {
            self.reader.lock().unwrap().read(buf)
        }

        fn write_all(&self, bytes: &[u8]) -> io::Result<()> {
            // No flush: writes to the serial handle are already synchronous.
            self.writer.lock().unwrap().write_all(bytes)
        }
    }

    pub(super) fn open(port_name: &str) -> io::Result<Arc<dyn Link>> {
        let mut port = serialport::new(port_name, BAUD)
            .flow_control(serialport::FlowControl::None)
            .timeout(READ_TIMEOUT)
            .open()?;
        // The official app asserts both lines (control line state 0x0003).
        port.write_request_to_send(true)?;
        port.write_data_terminal_ready(true)?;
        let reader = port.try_clone()?;
        Ok(Arc::new(PortLink {
            writer: Mutex::new(port),
            reader: Mutex::new(reader),
        }))
    }
}

#[cfg(windows)]
mod platform {
    use std::io;
    use std::os::windows::ffi::OsStrExt;
    use std::ptr;
    use std::sync::{Arc, Mutex};

    use windows_sys::Win32::Devices::Communication::{
        COMMTIMEOUTS, DCB, GetCommState, NOPARITY, ONESTOPBIT, PURGE_RXCLEAR, PurgeComm,
        SetCommState, SetCommTimeouts,
    };
    use windows_sys::Win32::Foundation::{
        CloseHandle, ERROR_IO_PENDING, GENERIC_READ, GENERIC_WRITE, GetLastError, HANDLE,
        INVALID_HANDLE_VALUE,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_FLAG_OVERLAPPED, OPEN_EXISTING, ReadFile, WriteFile,
    };
    use windows_sys::Win32::System::IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED};
    use windows_sys::Win32::System::Threading::CreateEventW;

    use super::Link;
    use crate::BAUD;

    /// DCB flags: binary mode, DTR and RTS asserted (the official app's line state
    /// 0x0003), no flow control.
    const DCB_BINARY: u32 = 1;
    const DCB_DTR_ENABLE: u32 = 1 << 4;
    const DCB_RTS_ENABLE: u32 = 1 << 12;
    /// Deadline for one write; a 20 KB file chunk needs more than a few ms.
    const WRITE_TIMEOUT_MS: u32 = 5000;

    struct OverlappedPort {
        handle: HANDLE,
        read_event: HANDLE,
        /// The write event; the lock also keeps writes whole.
        write_event: Mutex<HANDLE>,
    }

    // SAFETY: the handles are kernel objects usable from any thread; writes are
    // serialised by the mutex, and only the reader thread reads.
    unsafe impl Send for OverlappedPort {}
    unsafe impl Sync for OverlappedPort {}

    fn last_error() -> io::Error {
        io::Error::last_os_error()
    }

    fn check(ok: i32) -> io::Result<()> {
        if ok == 0 { Err(last_error()) } else { Ok(()) }
    }

    fn event() -> io::Result<HANDLE> {
        // SAFETY: a manual-reset, unsignalled, unnamed event.
        let event = unsafe { CreateEventW(ptr::null(), 1, 0, ptr::null()) };
        if event == 0 { Err(last_error()) } else { Ok(event) }
    }

    impl OverlappedPort {
        /// Start an overlapped transfer and wait for it to finish.
        fn transfer(
            &self,
            event: HANDLE,
            start: impl FnOnce(*mut OVERLAPPED) -> i32,
        ) -> io::Result<usize> {
            // SAFETY: OVERLAPPED is plain data; zeroed is its documented initial state.
            let mut ov: OVERLAPPED = unsafe { std::mem::zeroed() };
            ov.hEvent = event;
            if start(&mut ov) == 0 {
                // SAFETY: read right after the failing call.
                let code = unsafe { GetLastError() };
                if code != ERROR_IO_PENDING {
                    return Err(io::Error::from_raw_os_error(code as i32));
                }
            }
            let mut n = 0u32;
            // SAFETY: `ov` lives on this frame until the transfer completes; bWait blocks
            // on its event.
            check(unsafe { GetOverlappedResult(self.handle, &ov, &mut n, 1) })?;
            Ok(n as usize)
        }
    }

    impl Link for OverlappedPort {
        fn read(&self, buf: &mut [u8]) -> io::Result<usize> {
            let len = buf.len().min(u32::MAX as usize) as u32;
            // SAFETY: the buffer outlives the transfer, which `transfer` waits for.
            self.transfer(self.read_event, |ov| unsafe {
                ReadFile(self.handle, buf.as_mut_ptr(), len, ptr::null_mut(), ov)
            })
        }

        fn write_all(&self, mut bytes: &[u8]) -> io::Result<()> {
            let event = self.write_event.lock().unwrap();
            while !bytes.is_empty() {
                let len = bytes.len().min(u32::MAX as usize) as u32;
                // SAFETY: as for read.
                let n = self.transfer(*event, |ov| unsafe {
                    WriteFile(self.handle, bytes.as_ptr(), len, ptr::null_mut(), ov)
                })?;
                if n == 0 {
                    return Err(io::ErrorKind::TimedOut.into());
                }
                bytes = &bytes[n..];
            }
            Ok(())
        }

        fn cancel_read(&self) {
            // SAFETY: cancelling I/O on our own handle; the waiting reader then sees
            // ERROR_OPERATION_ABORTED and ends.
            unsafe { CancelIoEx(self.handle, ptr::null()) };
        }
    }

    impl Drop for OverlappedPort {
        fn drop(&mut self) {
            // SAFETY: closing handles this value owns; the reader thread has ended by the
            // time the last reference goes.
            unsafe {
                CloseHandle(self.handle);
                CloseHandle(self.read_event);
                CloseHandle(*self.write_event.lock().unwrap());
            }
        }
    }

    pub(super) fn open(port_name: &str) -> io::Result<Arc<dyn Link>> {
        let path: Vec<u16> = std::ffi::OsStr::new(&format!("\\\\.\\{port_name}"))
            .encode_wide()
            .chain(Some(0))
            .collect();
        // SAFETY: plain Win32 calls on a handle owned by the returned value.
        unsafe {
            let handle = CreateFileW(
                path.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                0,
                ptr::null(),
                OPEN_EXISTING,
                FILE_FLAG_OVERLAPPED,
                0,
            );
            if handle == INVALID_HANDLE_VALUE {
                return Err(last_error());
            }
            let port = OverlappedPort {
                handle,
                read_event: event()?,
                write_event: Mutex::new(event()?),
            };
            let mut dcb: DCB = std::mem::zeroed();
            dcb.DCBlength = std::mem::size_of::<DCB>() as u32;
            check(GetCommState(handle, &mut dcb))?;
            dcb.BaudRate = BAUD;
            dcb._bitfield = DCB_BINARY | DCB_DTR_ENABLE | DCB_RTS_ENABLE;
            dcb.ByteSize = 8;
            dcb.Parity = NOPARITY;
            dcb.StopBits = ONESTOPBIT;
            check(SetCommState(handle, &dcb))?;
            // Reads return as soon as any byte is there, and otherwise wait for one. The
            // longest wait Windows allows is MAXDWORD - 1 ms (49.7 days); then the reader
            // simply reads again.
            let timeouts = COMMTIMEOUTS {
                ReadIntervalTimeout: u32::MAX,
                ReadTotalTimeoutMultiplier: u32::MAX,
                ReadTotalTimeoutConstant: u32::MAX - 1,
                WriteTotalTimeoutMultiplier: 0,
                WriteTotalTimeoutConstant: WRITE_TIMEOUT_MS,
            };
            check(SetCommTimeouts(handle, &timeouts))?;
            check(PurgeComm(handle, PURGE_RXCLEAR))?;
            Ok(Arc::new(port))
        }
    }
}
