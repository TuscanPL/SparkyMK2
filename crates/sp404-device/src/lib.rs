//! Serial connection to a Roland SP-404MKII.
//!
//! [`Device`] owns the port and a reader thread. Transactions are serialised: each call
//! sends a request and waits for the matching reply; anything else that arrives meanwhile
//! (status notifications, duplicate echoes) is kept as an event.

mod control;
mod files;
mod render;

use std::collections::VecDeque;
use std::io::{ErrorKind, Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use serialport::{SerialPort, SerialPortType};
use sp404_proto::{Channel, Decoder, Message, ProtoError};

pub use files::{DirEntry, Stat};

/// USB ids of the SP-404MKII's serial function.
pub const USB_VID: u16 = 0x0582;
pub const USB_PID: u16 = 0x02E7;
pub const BAUD: u32 = 921_600;

const MAX_EVENTS: usize = 256;
const SETTLE: Duration = Duration::from_millis(350);
/// Read timeout of the reader thread. On Windows, synchronous reads and writes on the
/// (duplicated) port handle are serialised, so a write waits for a pending read to time
/// out: keep it short there.
#[cfg(windows)]
const READ_TIMEOUT: Duration = Duration::from_millis(2);
#[cfg(not(windows))]
const READ_TIMEOUT: Duration = Duration::from_millis(50);

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("serial port: {0}")]
    Serial(#[from] serialport::Error),
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("protocol: {0}")]
    Proto(#[from] ProtoError),
    #[error("timed out waiting for {0}")]
    Timeout(String),
    #[error("device refused {0}")]
    Refused(String),
    #[error("unexpected reply to {0}")]
    Unexpected(String),
    #[error("connection closed")]
    Closed,
    #[error("no SP-404MKII found (USB {USB_VID:04x}:{USB_PID:04x})")]
    NotFound,
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone)]
pub struct PortInfo {
    pub name: String,
    pub serial_number: Option<String>,
    pub product: Option<String>,
}

/// Serial ports that belong to an SP-404MKII.
pub fn find_ports() -> Result<Vec<PortInfo>> {
    Ok(serialport::available_ports()?
        .into_iter()
        .filter_map(|p| match p.port_type {
            SerialPortType::UsbPort(usb) if usb.vid == USB_VID && usb.pid == USB_PID => {
                Some(PortInfo {
                    name: p.port_name,
                    serial_number: usb.serial_number,
                    product: usb.product,
                })
            }
            _ => None,
        })
        .collect())
}

struct Inbox {
    rx: Receiver<Message>,
    events: VecDeque<Message>,
}

pub struct Device {
    port_name: String,
    writer: Mutex<Box<dyn SerialPort>>,
    inbox: Mutex<Inbox>,
    stop: Arc<AtomicBool>,
    reader: Option<JoinHandle<()>>,
}

impl Device {
    /// Open the first SP-404MKII found.
    pub fn open_first() -> Result<Self> {
        let port = find_ports()?.into_iter().next().ok_or(Error::NotFound)?;
        Self::open(&port.name)
    }

    pub fn open(port_name: &str) -> Result<Self> {
        let mut port = serialport::new(port_name, BAUD)
            .flow_control(serialport::FlowControl::None)
            .timeout(READ_TIMEOUT)
            .open()?;
        // The official app asserts both lines (control line state 0x0003) and pauses before
        // its first message; the control channel does not answer otherwise.
        port.write_request_to_send(true)?;
        port.write_data_terminal_ready(true)?;
        std::thread::sleep(SETTLE);
        let reader_port = port.try_clone()?;

        let (tx, rx) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let reader = {
            let stop = stop.clone();
            std::thread::Builder::new()
                .name("sp404-reader".into())
                .spawn(move || read_loop(reader_port, tx, stop))?
        };
        Ok(Device {
            port_name: port_name.to_string(),
            writer: Mutex::new(port),
            inbox: Mutex::new(Inbox {
                rx,
                events: VecDeque::new(),
            }),
            stop,
            reader: Some(reader),
        })
    }

    pub fn port_name(&self) -> &str {
        &self.port_name
    }

    /// Messages that arrived without matching a request (notifications, extra echoes).
    pub fn take_events(&self) -> Vec<Message> {
        let mut inbox = self.inbox.lock().unwrap();
        while let Ok(m) = inbox.rx.try_recv() {
            push_event(&mut inbox.events, m);
        }
        inbox.events.drain(..).collect()
    }

    fn send(&self, msg: &Message) -> Result<()> {
        let bytes = msg.encode();
        log::trace!("> {}", hex_preview(&bytes));
        let mut port = self.writer.lock().unwrap();
        // No flush: writes to the serial handle are already synchronous, and on Windows a
        // flush waits for the driver to drain, adding ~120 ms per message.
        port.write_all(&bytes)?;
        Ok(())
    }

    /// Send `msg`, then return the first incoming message accepted by `matches`.
    fn transact(
        &self,
        msg: &Message,
        what: &str,
        timeout: Duration,
        mut matches: impl FnMut(&Message) -> bool,
    ) -> Result<Message> {
        let mut inbox = self.inbox.lock().unwrap();
        while let Ok(m) = inbox.rx.try_recv() {
            push_event(&mut inbox.events, m);
        }
        self.send(msg)?;
        Self::wait(&mut inbox, what, timeout, &mut matches)
    }

    fn wait(
        inbox: &mut Inbox,
        what: &str,
        timeout: Duration,
        matches: &mut impl FnMut(&Message) -> bool,
    ) -> Result<Message> {
        let deadline = Instant::now() + timeout;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match inbox.rx.recv_timeout(left) {
                Ok(m) if matches(&m) => return Ok(m),
                Ok(m) => push_event(&mut inbox.events, m),
                Err(RecvTimeoutError::Timeout) => return Err(Error::Timeout(what.to_string())),
                Err(RecvTimeoutError::Disconnected) => return Err(Error::Closed),
            }
        }
    }

    /// Debugging aid: send a raw message and collect everything that arrives for `wait`.
    pub fn raw_exchange(&self, msg: &Message, wait: Duration) -> Result<Vec<Message>> {
        let inbox = self.inbox.lock().unwrap();
        self.send(msg)?;
        let deadline = Instant::now() + wait;
        let mut out = Vec::new();
        loop {
            match inbox
                .rx
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            {
                Ok(m) => out.push(m),
                Err(RecvTimeoutError::Timeout) => return Ok(out),
                Err(RecvTimeoutError::Disconnected) => return Err(Error::Closed),
            }
        }
    }

    /// Send without waiting for a reply.
    fn fire(&self, msg: &Message) -> Result<()> {
        let _inbox = self.inbox.lock().unwrap();
        self.send(msg)
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.reader.take() {
            let _ = handle.join();
        }
    }
}

fn push_event(events: &mut VecDeque<Message>, m: Message) {
    log::debug!(
        "event ch{:02x} {}",
        m.channel.to_byte(),
        hex_preview(m.payload())
    );
    if events.len() == MAX_EVENTS {
        events.pop_front();
    }
    events.push_back(m);
}

fn read_loop(mut port: Box<dyn SerialPort>, tx: mpsc::Sender<Message>, stop: Arc<AtomicBool>) {
    let mut decoder = Decoder::new();
    let mut buf = vec![0u8; 64 * 1024];
    while !stop.load(Ordering::Relaxed) {
        match port.read(&mut buf) {
            Ok(0) => continue,
            Ok(n) => {
                decoder.push(&buf[..n]);
                while let Some(m) = decoder.next_message() {
                    log::trace!(
                        "< ch{:02x} {}",
                        m.channel.to_byte(),
                        hex_preview(m.payload())
                    );
                    if tx.send(m).is_err() {
                        return;
                    }
                }
            }
            Err(e) if e.kind() == ErrorKind::TimedOut || e.kind() == ErrorKind::Interrupted => {
                continue;
            }
            Err(e) => {
                log::warn!("serial read failed: {e}");
                return;
            }
        }
    }
}

fn is_channel(m: &Message, channel: Channel) -> bool {
    m.channel == channel
}

pub(crate) fn hex_preview(bytes: &[u8]) -> String {
    const MAX: usize = 48;
    let shown: Vec<String> = bytes.iter().take(MAX).map(|b| format!("{b:02x}")).collect();
    if bytes.len() > MAX {
        format!("{} … (+{} bytes)", shown.join(" "), bytes.len() - MAX)
    } else {
        shown.join(" ")
    }
}
