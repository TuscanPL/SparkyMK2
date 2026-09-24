//! Parameters, pad blocks and pad operations (channel 5).

use std::time::Duration;

use sp404_proto::control::{self, PadOp, reply};
use sp404_proto::fileapi;
use sp404_proto::pad::{PAD_COUNT, PadBlock};
use sp404_proto::params::{self, Target};
use sp404_proto::{Channel, Message, PadIndex, ProjectSettings};

use crate::{Device, Error, Result, is_channel};

const TIMEOUT: Duration = Duration::from_secs(3);
const PROJECTS: usize = 16;

/// Summary of the device state from the status poll and project settings.
/// What the device is doing, from the status poll alone: cheap enough to ask several
/// times a second, unlike [`Status`], which also reads the project settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Activity {
    /// Current project, 0-based.
    pub project: u8,
    /// The pad selected on the device: hitting a pad selects it, and choosing a bank
    /// selects its first pad.
    pub selected: Option<PadIndex>,
    /// Frames played so far of the pad sounding, or `None` when nothing is.
    pub position: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct Status {
    /// Raw `7E` status payload.
    pub raw: Vec<u8>,
    /// Current project, 0-based (hypothesis: byte 1 of the status payload).
    pub project: u8,
    /// Selected bank and pad (bytes 3 and 5 of the status payload).
    pub selected: Option<PadIndex>,
    pub settings: ProjectSettings,
}

fn short(payload: &[u8]) -> Message {
    Message::short(Channel::Control, payload)
}

fn long(payload: Vec<u8>) -> Message {
    Message::long(Channel::Control, payload)
}

fn refused(what: &str, code: u8) -> Error {
    Error::Refused(format!("{what}: status {code}"))
}

fn device_error(what: &str, code: i32) -> Error {
    match control::error_message(code) {
        Some(text) => Error::Refused(format!("{what}: {text} (error {code})")),
        None => Error::Refused(format!("{what}: error {code}")),
    }
}

impl Device {
    fn control_call(
        &self,
        msg: Message,
        what: &str,
        mut matches: impl FnMut(&[u8]) -> bool,
    ) -> Result<Vec<u8>> {
        let reply = self.transact(&msg, what, TIMEOUT, |m| {
            is_channel(m, Channel::Control) && matches(m.payload())
        })?;
        Ok(reply.payload().to_vec())
    }

    /// Short command whose reply is `cmd status`; fails unless status is 0.
    fn command(&self, payload: &[u8], expect: u8, what: &str) -> Result<()> {
        let reply = self.control_call(short(payload), what, |p| p.first() == Some(&expect))?;
        match reply.get(1) {
            None | Some(0) => Ok(()),
            Some(&code) => Err(refused(what, code)),
        }
    }

    pub fn status(&self) -> Result<Status> {
        let raw = self.control_call(short(&control::status_request()), "status", |p| {
            p.first() == Some(&reply::STATUS)
        })?;
        let project = raw.get(1).copied().unwrap_or(0);
        let settings = self.project_settings(project)?;
        let selected = match (raw.get(3), raw.get(5)) {
            (Some(&b), Some(&p)) => PadIndex::from_bank_pad(u16::from(b), u16::from(p)),
            _ => None,
        };
        Ok(Status {
            project,
            selected,
            raw,
            settings,
        })
    }

    /// The status poll on its own: bytes 1, 3 and 5 are the project, bank and pad, and
    /// bytes 8 to 11 count the frames a sounding pad has played (all `FF` when none is).
    pub fn activity(&self) -> Result<Activity> {
        let raw = self.control_call(short(&control::status_request()), "status", |p| {
            p.first() == Some(&reply::STATUS)
        })?;
        let selected = match (raw.get(3), raw.get(5)) {
            (Some(&b), Some(&p)) => PadIndex::from_bank_pad(u16::from(b), u16::from(p)),
            _ => None,
        };
        let position = raw
            .get(8..12)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .filter(|&frames| frames != u32::MAX);
        Ok(Activity {
            project: raw.get(1).copied().unwrap_or(0),
            selected,
            position,
        })
    }

    /// Select a pad on the device, bank first, as the app's pad grid does.
    pub fn select_pad(&self, pad: PadIndex) -> Result<()> {
        self.set_param(0x00, Target::Global, i32::from(pad.bank()))?;
        self.set_param(0x01, Target::Global, i32::from(pad.pad()))
    }

    /// Current project, 0-based (status poll byte 1).
    pub fn current_project(&self) -> Result<u8> {
        let raw = self.control_call(short(&control::status_request()), "status", |p| {
            p.first() == Some(&reply::STATUS)
        })?;
        Ok(raw.get(1).copied().unwrap_or(0))
    }

    /// Settings of `project` (0-based; `7D` reply).
    pub fn project_settings(&self, project: u8) -> Result<ProjectSettings> {
        let payload = self.control_call(
            short(&control::project_settings_request(project)),
            "project settings",
            |p| p.len() > 1 && p[0] == reply::PROJECT_SETTINGS && p[1] == project,
        )?;
        Ok(ProjectSettings::parse(&payload)?)
    }

    /// Make `project` (0-based) the current project. Returns once the device has loaded it:
    /// settings requested before its `1A proj` notification can still describe the old
    /// project.
    pub fn select_project(&self, project: u8) -> Result<()> {
        let mut refusal = None;
        self.transact(
            &short(&control::select_project(project)),
            "select project",
            Duration::from_secs(10),
            |m| {
                if !is_channel(m, Channel::Control) {
                    return false;
                }
                match m.payload() {
                    // Acknowledgement `1A status`.
                    [reply::CHANGED, status] => {
                        if *status != 0 {
                            refusal = Some(*status);
                        }
                        refusal.is_some()
                    }
                    // Notification `1A proj 00 00 00`.
                    [reply::CHANGED, p, ..] => *p == project,
                    _ => false,
                }
            },
        )?;
        match refusal {
            Some(code) => Err(refused("select project", code)),
            None => Ok(()),
        }
    }

    /// Leave the remote screen on the device (the app's "MKII EXIT"). The link stays up.
    pub fn mkii_exit(&self) -> Result<()> {
        self.fire(&short(&control::mkii_exit()))
    }

    /// Play a pad for `duration`, like holding the app's Preview button.
    pub fn preview(&self, pad: PadIndex, duration: Duration) -> Result<()> {
        self.preview_start(pad)?;
        std::thread::sleep(duration);
        self.preview_stop(pad)
    }

    /// Start playing a pad, as holding the pad down would; it plays until [`Device::preview_stop`].
    pub fn preview_start(&self, pad: PadIndex) -> Result<()> {
        let id = pad.index().to_le_bytes();
        let started = self.control_call(short(&control::preview_start(pad)), "preview", |p| {
            p.len() >= 4 && p[0] == reply::PREVIEW_START && p[1..3] == id
        })?;
        if started[3] != 0 {
            return Err(refused("preview", started[3]));
        }
        Ok(())
    }

    /// Stop a pad started with [`Device::preview_start`].
    pub fn preview_stop(&self, pad: PadIndex) -> Result<()> {
        let id = pad.index().to_le_bytes();
        self.control_call(short(&control::preview_stop(pad)), "stop preview", |p| {
            p.len() >= 4 && p[0] == reply::PREVIEW_STOP && p[1..3] == id
        })?;
        Ok(())
    }

    /// Move a pad's sample onto another pad, replacing or exchanging with the destination.
    pub fn move_sample(&self, from: PadIndex, to: PadIndex, mode: control::MoveMode) -> Result<()> {
        let what = format!("{mode:?} {from} to {to}");
        self.command(
            &control::move_sample(from, to, mode),
            reply::MOVE_SAMPLE,
            &what,
        )?;
        self.check_device_error(&what)
    }

    /// Names of the 16 project slots (empty string for unnamed slots). The device sends
    /// them after the project settings reply.
    pub fn project_names(&self) -> Result<Vec<String>> {
        let msg = short(&control::project_settings_request(self.current_project()?));
        let mut names = vec![String::new(); PROJECTS];
        let mut inbox = self.inbox.lock().unwrap();
        self.send(&msg)?;
        let mut seen = 0;
        while seen < PROJECTS {
            let m = Self::wait(&mut inbox, "project names", TIMEOUT, &mut |m: &Message| {
                is_channel(m, Channel::Control) && m.payload().first() == Some(&reply::PROJECT_NAME)
            })?;
            if let Some((index, name)) = control::parse_project_name(m.payload()) {
                if let Some(slot) = names.get_mut(index as usize) {
                    *slot = name;
                    seen += 1;
                }
            }
        }
        Ok(names)
    }

    pub fn pad_block(&self, pad: PadIndex) -> Result<PadBlock> {
        let id = pad.index().to_le_bytes();
        let payload = self.control_call(
            short(&control::pad_block_request(pad)),
            &format!("pad block {pad}"),
            |p| p.len() > 3 && p[0] == reply::PAD_BLOCK && p[1..3] == id,
        )?;
        Ok(PadBlock::parse(&payload)?)
    }

    pub fn pad_blocks(&self) -> Result<Vec<PadBlock>> {
        (0..PAD_COUNT)
            .map(|i| self.pad_block(PadIndex::new(i).unwrap()))
            .collect()
    }

    pub fn pattern_exists(&self, slot: PadIndex) -> Result<bool> {
        let id = slot.index().to_le_bytes();
        let payload = self.control_call(
            short(&control::pattern_exists_request(slot)),
            &format!("pattern {slot}"),
            |p| p.len() >= 4 && p[0] == reply::PATTERN_EXISTS && p[1..3] == id,
        )?;
        Ok(payload[3] != 0)
    }

    pub fn set_param(&self, id: u8, target: Target, value: i32) -> Result<()> {
        let payload = control::set_param(id, target, value);
        let head = payload[..5].to_vec();
        self.control_call(long(payload), &format!("set param 0x{id:02x}"), |p| {
            p.len() >= 5 && p[..5] == head[..]
        })
        .map(drop)
    }

    /// Replace a pad's chop points: slot `n` gets `points[n]` (sample frames), and slots
    /// past the end are cleared. The device keeps them by slot, without sorting.
    pub fn set_chop_points(&self, pad: PadIndex, points: &[u32]) -> Result<()> {
        if points.len() > PadBlock::CHOP_POINTS {
            return Err(Error::Refused(format!(
                "{pad}: {} chop points, at most {} fit",
                points.len(),
                PadBlock::CHOP_POINTS
            )));
        }
        (0..PadBlock::CHOP_POINTS).try_for_each(|slot| {
            let value = points.get(slot).map_or(-1, |&p| p as i32);
            self.set_param(
                params::CHOP_POINT_FIRST + slot as u8,
                Target::Pad(pad),
                value,
            )
        })
    }

    /// Waveform peaks (min, max) for one channel of a pad's sample.
    pub fn peaks(
        &self,
        pad: PadIndex,
        channel: u8,
        start_frame: u32,
        points: u32,
        samples_per_point: u32,
    ) -> Result<Vec<(i16, i16)>> {
        let id = pad.index().to_le_bytes();
        let payload = self.control_call(
            long(control::peaks_request(
                pad,
                channel,
                start_frame,
                points,
                samples_per_point,
            )),
            "peaks",
            |p| p.len() >= 16 && p[0] == reply::PEAKS && p[1..3] == id && p[3] == channel,
        )?;
        Ok(control::parse_peaks(&payload))
    }

    pub fn set_sample_name(&self, pad: PadIndex, name: &str) -> Result<()> {
        let id = pad.index().to_le_bytes();
        self.control_call(long(control::sample_name(pad, name)), "sample name", |p| {
            p.len() >= 3 && p[0] == control::cmd::SAMPLE_NAME && p[1..3] == id
        })
        .map(drop)
    }

    /// Rename `project` (0-based), which must be the current project.
    ///
    /// The device renames whichever project is current and ignores the number it is sent,
    /// so renaming another one would rename the current one instead. It also keeps the
    /// name only in the project's `PADCONF.BIN`: an empty slot has none, and one written
    /// by older firmware has no room for a name, so a rename there is accepted and then
    /// lost when another project is selected. Both are refused with the reason.
    pub fn set_project_name(&self, project: u8, name: &str) -> Result<()> {
        let number = project + 1;
        let current = self.current_project()?;
        if current != project {
            return Err(Error::Unsupported(format!(
                "project {number} is not the current project (project {} is); select it first",
                current + 1
            )));
        }
        let padconf = format!("ROLAND/SP-404MKII/PROJECT_{number:02}/PADCONF.BIN");
        if self.stat(&padconf)?.is_none() {
            return Err(Error::Unsupported(format!(
                "project {number} is empty, and the SP-404MKII only keeps a name for a project \
                 with something saved in it; add a sample first, then rename it"
            )));
        }
        let handle = self.open_file(&padconf, sp404_proto::fileapi::flags::READ_ONLY)?;
        let header = self.read(handle, 12);
        self.close_file(handle)?;
        // Byte 8 is the file's version; names arrived with version 3.
        if header?.get(8).is_some_and(|&version| version < 3) {
            return Err(Error::Unsupported(format!(
                "project {number} was saved by older firmware, and its settings file has no \
                 room for a name"
            )));
        }
        self.control_call(
            long(control::project_name(project, name)),
            "project name",
            |p| p.len() >= 2 && p[0] == control::cmd::PROJECT_NAME && p[1] == project,
        )
        .map(drop)
    }

    /// After an acknowledged edit the device reports failure separately, as `32 code`
    /// (for example on an empty pad). Wait briefly for that or for the `1A` change
    /// notification that follows a successful edit.
    fn check_device_error(&self, what: &str) -> Result<()> {
        let mut inbox = self.inbox.lock().unwrap();
        let reply = Self::wait(
            &mut inbox,
            what,
            Duration::from_millis(1500),
            &mut |m: &Message| {
                is_channel(m, Channel::Control)
                    && matches!(m.payload().first(), Some(&reply::ERROR | &reply::CHANGED))
            },
        );
        match reply {
            Ok(m) => match control::parse_error(m.payload()) {
                Some(code) => Err(device_error(what, code)),
                None => Ok(()),
            },
            // Neither arrived: nothing went wrong that the device reported.
            Err(Error::Timeout(_)) => Ok(()),
            Err(e) => Err(e),
        }
    }

    /// Init: erase data of the current project (all of it, all samples or patterns, or one
    /// bank's). `project` (0-based) must be the current project, as a guard against
    /// erasing the wrong one.
    pub fn init_project(&self, project: u8, scope: control::InitScope) -> Result<()> {
        let current = self.current_project()?;
        if current != project {
            return Err(Error::Refused(format!(
                "init project {}: project {} is current (select it first)",
                project + 1,
                current + 1
            )));
        }
        let what = format!("init {scope:?}");
        // The device answers `12 status` and announces the reloaded project with `1A proj`,
        // in either order (Patterns Bank sends the notification first). Wait for both, so
        // the next command doesn't reach the device while it is still reloading.
        let mut status = None;
        let mut reloaded = false;
        let mut error = None;
        let waited = self.transact(
            &short(&control::init_project(scope)),
            &what,
            Duration::from_secs(15),
            |m| {
                if !is_channel(m, Channel::Control) {
                    return false;
                }
                match m.payload() {
                    [reply::INIT, code, ..] => status = Some(*code),
                    [reply::CHANGED, p, _, ..] => reloaded |= *p == project,
                    payload => error = error.or(control::parse_error(payload)),
                }
                error.is_some() || status.is_some_and(|c| c != 0) || (status.is_some() && reloaded)
            },
        );
        if let Some(code) = error {
            return Err(device_error(&what, code));
        }
        match (waited, status) {
            (_, Some(code)) if code != 0 => Err(refused(&what, code)),
            (Ok(_), _) => Ok(()),
            // Acknowledged, but the reload notification never came: the erase went through.
            (Err(Error::Timeout(_)), Some(_)) => {
                log::warn!("{what}: no reload notification from the device");
                Ok(())
            }
            (Err(e), _) => Err(e),
        }
    }

    /// Truncate, normalize or delete a pad's sample on the device.
    pub fn pad_op(&self, op: PadOp, pad: PadIndex) -> Result<()> {
        if op.bracketed() {
            self.fire(&short(&control::edit_begin()))?;
        }
        let what = format!("{op:?} {pad}");
        let result = self
            .command(&op.payload(pad), op.reply(), &what)
            .and_then(|()| self.check_device_error(&what));
        if op.bracketed() {
            self.fire(&short(&control::edit_end()))?;
        }
        result
    }

    /// Replace a project's files from a backup and make the device reload it (the app's
    /// "Import to MKII"). `project` is 0-based and **must be the current project**: the
    /// first step erases the current project, on the card as well as in memory. `files`
    /// are paths relative to the `PROJECT_NN` folder, in the order to write them.
    /// `progress` is called per file.
    pub fn restore_project(
        &self,
        project: u8,
        files: &[(String, Vec<u8>)],
        mut progress: impl FnMut(usize, &str),
    ) -> Result<()> {
        use control::restore;
        let current = self.current_project()?;
        if current != project {
            return Err(Error::Refused(format!(
                "restore into project {}: project {} is current (select it first)",
                project + 1,
                current + 1
            )));
        }
        let folder = format!("ROLAND/SP-404MKII/PROJECT_{:02}", project + 1);
        self.fire(&short(&control::edit_begin()))?;
        let result = (|| -> Result<()> {
            let prepared = self.transact(
                &short(&restore::prepare()),
                "prepare restore",
                Duration::from_secs(15),
                |m| {
                    is_channel(m, Channel::Control)
                        && m.payload().first() == Some(&restore::PREPARE_REPLY)
                },
            )?;
            if let Some(&code) = prepared.payload().get(1).filter(|&&c| c != 0) {
                return Err(refused("prepare restore", code));
            }
            for (i, (name, data)) in files.iter().enumerate() {
                progress(i, name);
                self.write_file(&format!("{folder}/{name}"), data)?;
            }
            let reloaded = self.transact(
                &short(&restore::reload(project)),
                "reload project",
                Duration::from_secs(15),
                |m| {
                    is_channel(m, Channel::Control)
                        && m.payload().first() == Some(&restore::RELOAD_REPLY)
                },
            )?;
            if let Some(&code) = reloaded.payload().get(1).filter(|&&c| c != 0) {
                return Err(refused("reload project", code));
            }
            let mut inbox = self.inbox.lock().unwrap();
            Self::wait(
                &mut inbox,
                "project reload",
                Duration::from_secs(15),
                &mut |m: &Message| {
                    is_channel(m, Channel::Control)
                        && m.payload().first() == Some(&restore::RELOADED)
                },
            )?;
            Ok(())
        })();
        self.fire(&short(&control::edit_end()))?;
        result
    }

    /// Write a complete SMP file to a pad and point the pad at it, following the official
    /// app's import sequence. `project` is 1-based (the `PROJECT_NN` folder) and **must be
    /// the current project**: the pad block is written to the current project, so importing
    /// into another one would change a current pad while its file went elsewhere.
    pub fn import_smp(&self, project: u8, pad: PadIndex, smp: &[u8], name: &str) -> Result<()> {
        let current = self.current_project()? + 1;
        if current != project {
            return Err(Error::Refused(format!(
                "import into project {project}: project {current} is current (select it first)"
            )));
        }
        let header_len = control::SMP_HEADER_LEN as usize;
        if smp.len() < header_len {
            return Err(Error::Unexpected("SMP shorter than its header".into()));
        }
        let mut block = self.pad_block(pad)?;
        let path = pad.sample_path(project);
        let tmp = path.replace(".SMP", ".TMP");

        self.fire(&short(&control::import::begin()))?;
        let result = (|| -> Result<()> {
            self.command(
                &control::import::release_slot(pad),
                reply::PAD_SLOT,
                "release slot",
            )?;
            self.command(&control::stop(), reply::STOP, "stop")?;
            let _ = self.unlink(&tmp);
            let _ = self.unlink(&path);

            let handle = self.open_file(&path, fileapi::flags::CREATE_TRUNCATE_READ_WRITE)?;
            let written = (|| -> Result<()> {
                self.seek(handle, header_len as u32)?;
                self.write(handle, &smp[header_len..])?;
                self.seek(handle, 0)?;
                self.write(handle, &smp[..header_len])
            })();
            self.close_file(handle)?;
            written?;

            let size = self.stat(&path)?.map(|s| s.size).unwrap_or(0);
            if size as usize != smp.len() {
                return Err(Error::Refused(format!(
                    "write {path} (size {size}, expected {})",
                    smp.len()
                )));
            }

            block.assign_sample(smp.len() as u32, name);
            let id = pad.index().to_le_bytes();
            self.control_call(long(block.write_payload()), "pad block write", |p| {
                p.first() == Some(&reply::PAD_BLOCK) && (p.len() <= 2 || p[1..3] == id)
            })?;
            // Reply: `1F pad:u16 status`.
            let commit =
                self.control_call(short(&control::import::commit(pad)), "commit import", |p| {
                    p.len() >= 4 && p[0] == reply::PAD_COMMIT && p[1..3] == id
                })?;
            if commit[3] != 0 {
                return Err(refused("commit import", commit[3]));
            }
            Ok(())
        })();
        self.fire(&short(&control::import::end()))?;
        result
    }
}
