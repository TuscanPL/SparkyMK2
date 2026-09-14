//! Pattern rendering: Bounce and MULTIPAD export (`docs/re/04-patterns.md`).
//!
//! The device renders in real time and then writes a complete WAV file into a host file
//! handle, sending write and seek requests on channel 6 that the host must serve.

use std::io::{Seek, SeekFrom, Write};
use std::sync::mpsc::RecvTimeoutError;
use std::time::{Duration, Instant};

use sp404_proto::control::{self, render, reply};
use sp404_proto::fileapi::{self, HostRequest};
use sp404_proto::{Channel, Message, PadIndex};

use crate::{Device, Error, Result, is_channel, push_event};

const TIMEOUT: Duration = Duration::from_secs(3);
/// Longest silence tolerated while the device renders (it reports progress meanwhile).
const RENDER_IDLE: Duration = Duration::from_secs(60);
/// Status poll interval that keeps the device rendering.
const POLL: Duration = Duration::from_millis(500);
/// Handle number the host gives the device; only one render runs at a time.
const HANDLE: u16 = 0;

fn short(payload: &[u8]) -> Message {
    Message::short(Channel::Control, payload)
}

impl Device {
    /// Pads a pattern plays.
    pub fn pattern_pads(&self, pattern: PadIndex) -> Result<Vec<PadIndex>> {
        let reply = self.transact(
            &short(&render::pads_used_request(pattern)),
            "pattern pads",
            TIMEOUT,
            |m| {
                is_channel(m, Channel::Control)
                    && m.payload().first() == Some(&render::PADS_USED_REPLY)
            },
        )?;
        render::parse_pads_used(reply.payload())
            .ok_or_else(|| Error::Unexpected("pattern pads".into()))
    }

    /// Bounce a pattern to a WAV file written into `out`.
    pub fn bounce_pattern<W: Write + Seek>(&self, pattern: PadIndex, out: &mut W) -> Result<()> {
        self.with_render_session(pattern, || self.render(pattern, render::BOUNCE, out))
    }

    /// Render one pad of a pattern (a MULTIPAD export part) into `out`.
    pub fn render_pattern_pad<W: Write + Seek>(
        &self,
        pattern: PadIndex,
        pad: PadIndex,
        out: &mut W,
    ) -> Result<()> {
        self.with_render_session(pattern, || self.render(pattern, pad.index(), out))
    }

    fn with_render_session(
        &self,
        pattern: PadIndex,
        body: impl FnOnce() -> Result<()>,
    ) -> Result<()> {
        self.fire(&short(&control::edit_begin()))?;
        let result = self
            .render_control(pattern, render::BEGIN)
            .and_then(|()| body());
        let ended = self.render_control(pattern, render::END);
        self.fire(&short(&control::edit_end()))?;
        result.and(ended)
    }

    fn render_control(&self, pattern: PadIndex, op: u16) -> Result<()> {
        let what = format!("render control {op}");
        self.transact(
            &short(&render::control(pattern, op, HANDLE)),
            &what,
            TIMEOUT,
            |m| {
                let p = m.payload();
                is_channel(m, Channel::Control)
                    && p.len() >= 5
                    && p[0] == render::CONTROL_REPLY
                    && u16::from_le_bytes([p[3], p[4]]) == op
            },
        )?;
        Ok(())
    }

    /// Start a render and serve the device's file requests until it reports completion.
    fn render<W: Write + Seek>(&self, pattern: PadIndex, op: u16, out: &mut W) -> Result<()> {
        let mut inbox = self.inbox.lock().unwrap();
        while let Ok(m) = inbox.rx.try_recv() {
            push_event(&mut inbox.events, m);
        }
        self.send(&short(&render::control(pattern, op, HANDLE)))?;
        let mut last_poll = Instant::now();
        let mut last_heard = Instant::now();
        loop {
            // The device pauses rendering when the host goes quiet for a few seconds; keep
            // polling status as the official app does.
            if last_poll.elapsed() >= POLL {
                self.send(&short(&control::status_request()))?;
                last_poll = Instant::now();
            }
            let m = match inbox.rx.recv_timeout(POLL) {
                Ok(m) => {
                    last_heard = Instant::now();
                    m
                }
                Err(RecvTimeoutError::Timeout) if last_heard.elapsed() < RENDER_IDLE => continue,
                Err(RecvTimeoutError::Timeout) => return Err(Error::Timeout("render".into())),
                Err(RecvTimeoutError::Disconnected) => return Err(Error::Closed),
            };
            if is_channel(&m, Channel::Control) && m.payload().first() == Some(&reply::STATUS) {
                continue;
            }
            if is_channel(&m, Channel::File) && !m.is_short() {
                let result = match fileapi::parse_host_request(m.payload())? {
                    HostRequest::Write { data, .. } => {
                        out.write_all(&data).map(|()| data.len() as i32)
                    }
                    HostRequest::Seek { offset, .. } => out
                        .seek(SeekFrom::Start(u64::from(offset)))
                        .map(|pos| pos as i32),
                };
                self.send(&Message::short(Channel::File, &fileapi::host_ack()))?;
                let status = result.as_ref().copied().unwrap_or(-1);
                self.send(&Message::long(Channel::File, fileapi::host_status(status)))?;
                result?;
                continue;
            }
            if is_channel(&m, Channel::Control) {
                if let Some((_, done_op, _, code)) = render::parse_done(m.payload()) {
                    if done_op == op {
                        return match code {
                            0 => Ok(()),
                            code => Err(Error::Refused(format!("render (result {code})"))),
                        };
                    }
                }
            }
            // The `38` acknowledgement and progress (global parameter 86).
            push_event(&mut inbox.events, m);
        }
    }
}
