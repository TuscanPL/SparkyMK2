//! Playing sounds from the device on this computer, through an output the user picks.
//!
//! The device can only preview its own pads, so a file in Import & Export is read over
//! USB, decoded, and played here. This used to go through the webview, but webviews
//! cannot choose an output on Linux or macOS (and on Windows only after a microphone
//! prompt), and on Windows the SP-404MKII itself is often the default output, where a
//! preview is silent unless the unit routes USB audio. So it plays from Rust, with cpal.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

/// Decoded audio at the device's rate, 48 kHz, interleaved.
pub struct Sound {
    pub channels: u16,
    pub samples: Vec<i16>,
}

const SOURCE_RATE: f64 = 48_000.0;

impl Sound {
    fn frames(&self) -> usize {
        self.samples.len() / usize::from(self.channels.max(1))
    }

    fn bytes(&self) -> usize {
        self.samples.len() * 2
    }
}

/// Decoded sounds, so going back to one plays it at once instead of reading it again.
/// Least recently heard go first once the cap is reached.
pub struct SoundCache {
    sounds: HashMap<String, (Arc<Sound>, u64)>,
    bytes: usize,
    tick: u64,
}

/// Previews are cut at 30 s, so one sound is at most ~5.8 MB as 16-bit stereo.
pub const CACHE_BYTES: usize = 256 * 1024 * 1024;

impl SoundCache {
    pub fn new() -> Self {
        Self { sounds: HashMap::new(), bytes: 0, tick: 0 }
    }

    pub fn get(&mut self, key: &str) -> Option<Arc<Sound>> {
        self.tick += 1;
        let tick = self.tick;
        self.sounds.get_mut(key).map(|(sound, used)| {
            *used = tick;
            sound.clone()
        })
    }

    pub fn insert(&mut self, key: String, sound: Arc<Sound>) {
        self.tick += 1;
        self.bytes += sound.bytes();
        if let Some((old, _)) = self.sounds.insert(key, (sound, self.tick)) {
            self.bytes -= old.bytes();
        }
        while self.bytes > CACHE_BYTES && self.sounds.len() > 1 {
            let oldest = self.sounds.iter().min_by_key(|(_, (_, used))| *used).map(|(k, _)| k.clone());
            if let Some((sound, _)) = oldest.and_then(|k| self.sounds.remove(&k)) {
                self.bytes -= sound.bytes();
            }
        }
    }
}

/// An output the user can pick in Settings.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioOutput {
    /// Stable id, kept in the app's preferences.
    pub id: String,
    pub name: String,
    pub is_default: bool,
    /// Looks like the SP-404MKII's own USB audio, which "Automatic" avoids.
    pub is_sampler: bool,
}

fn is_sampler(name: &str) -> bool {
    let name = name.to_ascii_uppercase();
    name.contains("SP-404") || name.contains("SP404")
}

pub fn outputs() -> Result<Vec<AudioOutput>, String> {
    let host = cpal::default_host();
    let default = host.default_output_device();
    let devices = host.output_devices().map_err(|e| e.to_string())?;
    Ok(devices
        .filter_map(|d| {
            let id = d.id().ok()?.to_string();
            let name = d.to_string();
            Some(AudioOutput {
                is_default: default.as_ref() == Some(&d),
                is_sampler: is_sampler(&name),
                id,
                name,
            })
        })
        .collect())
}

/// The output to play on: the one asked for if it is still there; otherwise the system
/// default, unless that is the SP-404MKII, in which case the first other output.
fn pick_output(wanted: Option<&str>) -> Option<cpal::Device> {
    let host = cpal::default_host();
    let devices: Vec<cpal::Device> = host.output_devices().map(|d| d.collect()).unwrap_or_default();
    if let Some(wanted) = wanted {
        let found = devices
            .iter()
            .find(|d| d.id().map(|id| id.to_string() == wanted).unwrap_or(false));
        if let Some(d) = found {
            return Some(d.clone());
        }
        log::warn!("audio: output {wanted} is gone, using the automatic choice");
    }
    match host.default_output_device() {
        Some(d) if !is_sampler(&d.to_string()) => Some(d),
        default => devices
            .into_iter()
            .find(|d| !is_sampler(&d.to_string()))
            .or(default),
    }
}

/// What the output callback is playing.
struct Playing {
    sound: Arc<Sound>,
    /// Position in source frames; fractional, as the output rate may differ.
    position: f64,
    /// Source frames per output frame.
    step: f64,
    done: Arc<AtomicBool>,
}

enum Command {
    Play { sound: Arc<Sound>, output: Option<String>, id: u64 },
    Stop,
}

/// Plays one sound at a time on its own thread, which owns the output stream (cpal
/// streams cannot move between threads on every system).
pub struct Player {
    commands: Mutex<Sender<Command>>,
    next_id: Mutex<u64>,
}

impl Player {
    pub fn new(app: AppHandle) -> Self {
        let (tx, rx) = mpsc::channel();
        std::thread::Builder::new()
            .name("sound-preview".into())
            .spawn(move || run(rx, app))
            .expect("start the sound preview thread");
        Self { commands: Mutex::new(tx), next_id: Mutex::new(0) }
    }

    /// Start playing `sound`, cutting off anything playing. Answers an id that the
    /// `sound-ended` event repeats when it finishes by itself.
    pub fn play(&self, sound: Arc<Sound>, output: Option<String>) -> u64 {
        let id = {
            let mut next = self.next_id.lock().unwrap();
            *next += 1;
            *next
        };
        let _ = self.commands.lock().unwrap().send(Command::Play { sound, output, id });
        id
    }

    pub fn stop(&self) {
        let _ = self.commands.lock().unwrap().send(Command::Stop);
    }
}

fn run(commands: Receiver<Command>, app: AppHandle) {
    // The stream lives only while a sound plays, so the output is not held open.
    let mut current: Option<(cpal::Stream, Arc<AtomicBool>, u64)> = None;
    loop {
        match commands.recv_timeout(Duration::from_millis(50)) {
            Ok(Command::Play { sound, output, id }) => {
                current = None;
                match open(sound, output.as_deref()) {
                    Ok((stream, done)) => current = Some((stream, done, id)),
                    Err(e) => {
                        log::warn!("audio: {e}");
                        let _ = app.emit("sound-ended", SoundEnded { id, error: Some(e) });
                    }
                }
            }
            Ok(Command::Stop) => current = None,
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
        if let Some((_, done, id)) = &current {
            if done.load(Ordering::Acquire) {
                let id = *id;
                current = None;
                let _ = app.emit("sound-ended", SoundEnded { id, error: None });
            }
        }
    }
}

#[derive(Clone, Serialize)]
struct SoundEnded {
    id: u64,
    /// Set when the sound could not be played at all.
    error: Option<String>,
}

fn open(sound: Arc<Sound>, output: Option<&str>) -> Result<(cpal::Stream, Arc<AtomicBool>), String> {
    let device = pick_output(output).ok_or("no audio output found")?;
    let supported = device.default_output_config().map_err(|e| format!("{device}: {e}"))?;
    let config: cpal::StreamConfig = supported.config();
    log::debug!(
        "audio: {} frames on {device}, {} Hz, {} ch, {:?}",
        sound.frames(),
        config.sample_rate,
        config.channels,
        supported.sample_format()
    );
    let done = Arc::new(AtomicBool::new(false));
    let playing = Arc::new(Mutex::new(Playing {
        step: SOURCE_RATE / f64::from(config.sample_rate),
        sound,
        position: 0.0,
        done: done.clone(),
    }));
    let stream = match supported.sample_format() {
        SampleFormat::F32 => build::<f32>(&device, config, playing),
        SampleFormat::F64 => build::<f64>(&device, config, playing),
        SampleFormat::I16 => build::<i16>(&device, config, playing),
        SampleFormat::I32 => build::<i32>(&device, config, playing),
        SampleFormat::U16 => build::<u16>(&device, config, playing),
        SampleFormat::U8 => build::<u8>(&device, config, playing),
        other => return Err(format!("{device}: sample format {other} is not supported")),
    }
    .map_err(|e| format!("{device}: {e}"))?;
    stream.play().map_err(|e| format!("{device}: {e}"))?;
    Ok((stream, done))
}

fn build<T: SizedSample + FromSample<f32>>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    playing: Arc<Mutex<Playing>>,
) -> Result<cpal::Stream, cpal::Error> {
    let out_channels = usize::from(config.channels.max(1));
    device.build_output_stream(
        config,
        move |data: &mut [T], _| {
            let Ok(mut p) = playing.try_lock() else {
                data.fill(T::EQUILIBRIUM);
                return;
            };
            fill(&mut p, data, out_channels);
        },
        |e| log::warn!("audio: stream error: {e}"),
        None,
    )
}

/// Write the next output frames: linear interpolation between source frames, mono to
/// every channel, stereo to the first two (mixed down for a mono output).
fn fill<T: SizedSample + FromSample<f32>>(p: &mut Playing, data: &mut [T], out_channels: usize) {
    let ch = usize::from(p.sound.channels.max(1));
    let frames = p.sound.frames();
    let s = &p.sound.samples;
    let at = |frame: usize, c: usize| f32::from(s[frame * ch + c.min(ch - 1)]) / 32768.0;
    for out in data.chunks_mut(out_channels) {
        let i = p.position as usize;
        if i + 1 >= frames {
            out.fill(T::EQUILIBRIUM);
            p.done.store(true, Ordering::Release);
            continue;
        }
        let t = (p.position - i as f64) as f32;
        let value = |c: usize| at(i, c) + (at(i + 1, c) - at(i, c)) * t;
        for (c, slot) in out.iter_mut().enumerate() {
            let v = match (ch, out_channels, c) {
                (1, _, _) => value(0),
                (_, 1, _) => (value(0) + value(1)) * 0.5,
                (_, _, c) if c < 2 => value(c),
                _ => 0.0,
            };
            *slot = T::from_sample(v);
        }
        p.position += p.step;
    }
}

/// How much SP audio may wait for the output before the oldest is dropped, so a slow
/// start or a hiccup does not leave the monitor lagging for good.
const MONITOR_MAX_MS: usize = 80;
/// Audio gathered before the output starts taking it, against crackles on startup.
const MONITOR_PRIME_MS: usize = 25;
/// Audio moved per callback. Left to itself, PulseAudio hands over input in chunks of
/// about 340 ms, far more than the queue holds, which made the monitor choppy.
const MONITOR_BUFFER_MS: u32 = 10;

/// Passes the SP-404MKII's USB audio input to an output on this computer, so a pad
/// played by Preview or on the unit is heard here with all its settings and effects.
/// Runs on its own thread, which owns both streams.
#[derive(Clone)]
pub struct Monitor {
    commands: Arc<Mutex<Sender<MonitorRequest>>>,
}

/// The output to monitor on (`None` stops), and where to answer.
type MonitorRequest = (Option<Option<String>>, Sender<Result<String, String>>);

impl Monitor {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel::<MonitorRequest>();
        std::thread::Builder::new()
            .name("sampler-monitor".into())
            .spawn(move || {
                let mut streams: Option<(cpal::Stream, cpal::Stream)> = None;
                while let Ok((wanted, reply)) = rx.recv() {
                    streams = None;
                    let result = match wanted {
                        None => Ok(String::new()),
                        Some(output) => match start_monitor(output.as_deref()) {
                            Ok((input, output, name)) => {
                                streams = Some((input, output));
                                Ok(name)
                            }
                            Err(e) => {
                                log::warn!("audio: monitor: {e}");
                                Err(e)
                            }
                        },
                    };
                    let _ = reply.send(result);
                }
                drop(streams);
            })
            .expect("start the monitor thread");
        Self { commands: Arc::new(Mutex::new(tx)) }
    }

    /// Start (or restart, on another output) passing the SP through, or stop with `None`.
    /// Answers the name of the output it plays on.
    pub fn set(&self, output: Option<Option<String>>) -> Result<String, String> {
        let (tx, rx) = mpsc::channel();
        self.commands
            .lock()
            .unwrap()
            .send((output, tx))
            .map_err(|_| "the monitor stopped".to_string())?;
        rx.recv().map_err(|_| "the monitor stopped".to_string())?
    }
}

/// PulseAudio and PipeWire list a "monitor" source for every output, which carries what
/// the computer sends to it; the SP-404MKII's output has one too, named like the unit.
fn is_monitor_source(device: &cpal::Device) -> bool {
    let id = device.id().map(|id| id.to_string()).unwrap_or_default();
    id.ends_with(".monitor") || device.to_string().to_ascii_lowercase().starts_with("monitor of")
}

fn start_monitor(output: Option<&str>) -> Result<(cpal::Stream, cpal::Stream, String), String> {
    let host = cpal::default_host();
    let input = host
        .input_devices()
        .map_err(|e| e.to_string())?
        .find(|d| is_sampler(&d.to_string()) && !is_monitor_source(d))
        .ok_or("the SP-404MKII's USB audio input was not found")?;
    let out = pick_output(output).ok_or("no audio output found")?;
    if is_sampler(&out.to_string()) {
        return Err("the SP-404MKII cannot be monitored on itself; choose another output".into());
    }
    let in_config = input.default_input_config().map_err(|e| format!("{input}: {e}"))?;
    let out_config = out.default_output_config().map_err(|e| format!("{out}: {e}"))?;
    let in_rate = in_config.sample_rate();
    let out_rate = out_config.sample_rate();
    let in_channels = usize::from(in_config.channels().max(1));
    log::info!(
        "audio: monitoring {input} ({in_rate} Hz, {in_channels} ch) on {out} ({out_rate} Hz, {} ch)",
        out_config.channels()
    );
    // Stereo frames waiting for the output.
    let queue = Arc::new(Mutex::new(std::collections::VecDeque::<[f32; 2]>::new()));
    let max = MONITOR_MAX_MS * in_rate as usize / 1000;

    let q = queue.clone();
    let push = move |frames: &mut dyn Iterator<Item = [f32; 2]>| {
        let mut q = q.lock().unwrap();
        q.extend(frames);
        while q.len() > max {
            q.pop_front();
        }
    };
    let push = Arc::new(Mutex::new(push));
    let build_in = |config: cpal::StreamConfig| {
        let push = push.clone();
        let push = move |frames: &mut dyn Iterator<Item = [f32; 2]>| (push.lock().unwrap())(frames);
        match in_config.sample_format() {
            SampleFormat::I16 => build_input::<i16>(&input, config, in_channels, push),
            SampleFormat::I32 => build_input::<i32>(&input, config, in_channels, push),
            SampleFormat::F32 => build_input::<f32>(&input, config, in_channels, push),
            _ => Err(cpal::Error::from(cpal::ErrorKind::UnsupportedConfig)),
        }
    };
    let input_stream = build_in(small_buffers(in_config.config()))
        .or_else(|_| build_in(in_config.config()))
        .map_err(|e| format!("{input}: {e}"))?;

    let pull = Puller {
        queue,
        step: f64::from(in_rate) / f64::from(out_rate),
        position: 0.0,
        primed: false,
        prime: MONITOR_PRIME_MS * in_rate as usize / 1000,
    };
    let pull = Arc::new(Mutex::new(pull));
    let build_out = |config: cpal::StreamConfig| match out_config.sample_format() {
        SampleFormat::F32 => build_monitor_output::<f32>(&out, config, pull.clone()),
        SampleFormat::F64 => build_monitor_output::<f64>(&out, config, pull.clone()),
        SampleFormat::I16 => build_monitor_output::<i16>(&out, config, pull.clone()),
        SampleFormat::I32 => build_monitor_output::<i32>(&out, config, pull.clone()),
        SampleFormat::U16 => build_monitor_output::<u16>(&out, config, pull.clone()),
        _ => Err(cpal::Error::from(cpal::ErrorKind::UnsupportedConfig)),
    };
    let output_stream = build_out(small_buffers(out_config.config()))
        .or_else(|_| build_out(out_config.config()))
        .map_err(|e| format!("{out}: {e}"))?;
    input_stream.play().map_err(|e| format!("{input}: {e}"))?;
    output_stream.play().map_err(|e| format!("{out}: {e}"))?;
    Ok((input_stream, output_stream, out.to_string()))
}

/// The same stream with callbacks of [`MONITOR_BUFFER_MS`].
fn small_buffers(mut config: cpal::StreamConfig) -> cpal::StreamConfig {
    config.buffer_size = cpal::BufferSize::Fixed(config.sample_rate * MONITOR_BUFFER_MS / 1000);
    config
}

fn build_input<T: SizedSample>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    channels: usize,
    mut push: impl FnMut(&mut dyn Iterator<Item = [f32; 2]>) + Send + 'static,
) -> Result<cpal::Stream, cpal::Error>
where
    f32: FromSample<T>,
{
    device.build_input_stream(
        config,
        move |data: &[T], _| {
            let mut frames = data.chunks(channels).map(|f| {
                let l = <f32 as FromSample<T>>::from_sample_(f[0]);
                [l, f.get(1).map(|&r| <f32 as FromSample<T>>::from_sample_(r)).unwrap_or(l)]
            });
            push(&mut frames);
        },
        |e| log::warn!("audio: monitor input: {e}"),
        None,
    )
}

/// The output side of the monitor: reads the queue at the output's rate.
struct Puller {
    queue: Arc<Mutex<std::collections::VecDeque<[f32; 2]>>>,
    step: f64,
    position: f64,
    primed: bool,
    prime: usize,
}

fn build_monitor_output<T: SizedSample + FromSample<f32>>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    pull: Arc<Mutex<Puller>>,
) -> Result<cpal::Stream, cpal::Error> {
    let channels = usize::from(config.channels.max(1));
    device.build_output_stream(
        config,
        move |data: &mut [T], _| {
            let mut pull = pull.lock().unwrap();
            let pull = &mut *pull;
            let mut q = pull.queue.lock().unwrap();
            if !pull.primed {
                pull.primed = q.len() >= pull.prime;
            }
            for out in data.chunks_mut(channels) {
                let i = pull.position as usize;
                let frame = if pull.primed && i + 1 < q.len() {
                    let t = (pull.position - i as f64) as f32;
                    let (a, b) = (q[i], q[i + 1]);
                    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
                } else {
                    // Ran dry: wait for a little to gather again rather than stutter.
                    pull.primed = false;
                    [0.0, 0.0]
                };
                if pull.primed {
                    pull.position += pull.step;
                }
                for (c, slot) in out.iter_mut().enumerate() {
                    *slot = T::from_sample(match (channels, c) {
                        (1, _) => (frame[0] + frame[1]) * 0.5,
                        (_, c) if c < 2 => frame[c],
                        _ => 0.0,
                    });
                }
            }
            let used = (pull.position as usize).min(q.len());
            q.drain(..used);
            pull.position -= used as f64;
        },
        |e| log::warn!("audio: monitor output: {e}"),
        None,
    )
}

/// A recording of the SP-404MKII's USB audio input, for resampling a pad over USB.
/// Made and finished on one thread, as cpal streams may not move between threads.
pub struct Recording {
    _stream: cpal::Stream,
    frames: Arc<Mutex<Vec<[f32; 2]>>>,
    rate: u32,
}

/// Start recording what the unit sends over USB audio.
pub fn record_sampler() -> Result<Recording, String> {
    let host = cpal::default_host();
    let input = host
        .input_devices()
        .map_err(|e| e.to_string())?
        .find(|d| is_sampler(&d.to_string()) && !is_monitor_source(d))
        .ok_or("the SP-404MKII's USB audio input was not found")?;
    let config = input.default_input_config().map_err(|e| format!("{input}: {e}"))?;
    let channels = usize::from(config.channels().max(1));
    let rate = config.sample_rate();
    let frames = Arc::new(Mutex::new(Vec::<[f32; 2]>::new()));
    let sink = frames.clone();
    let push = Arc::new(Mutex::new(move |it: &mut dyn Iterator<Item = [f32; 2]>| {
        sink.lock().unwrap().extend(it);
    }));
    let build = |stream_config: cpal::StreamConfig| {
        let push = push.clone();
        let push = move |it: &mut dyn Iterator<Item = [f32; 2]>| (push.lock().unwrap())(it);
        match config.sample_format() {
            SampleFormat::I16 => build_input::<i16>(&input, stream_config, channels, push),
            SampleFormat::I32 => build_input::<i32>(&input, stream_config, channels, push),
            SampleFormat::F32 => build_input::<f32>(&input, stream_config, channels, push),
            _ => Err(cpal::Error::from(cpal::ErrorKind::UnsupportedConfig)),
        }
    };
    let stream = build(small_buffers(config.config()))
        .or_else(|_| build(config.config()))
        .map_err(|e| format!("{input}: {e}"))?;
    stream.play().map_err(|e| format!("{input}: {e}"))?;
    log::info!("audio: recording {input} at {rate} Hz");
    Ok(Recording { _stream: stream, frames, rate })
}

/// Below this (about −66 dBFS) a frame counts as silence when trimming.
const SILENCE: f32 = 1.0 / 2048.0;

impl Recording {
    /// Stop and answer the sound as 48 kHz 16-bit stereo, trimmed of the silence before
    /// it starts (keeping 5 ms) and after it ends (keeping 50 ms). `None` if nothing above
    /// silence came in.
    pub fn finish(self) -> Option<Vec<i16>> {
        let Recording { _stream: stream, frames, rate } = self;
        drop(stream);
        let frames = std::mem::take(&mut *frames.lock().unwrap());
        let frames = to_48k(frames, rate);
        let loud = |f: &[f32; 2]| f[0].abs() > SILENCE || f[1].abs() > SILENCE;
        let first = frames.iter().position(loud)?;
        let last = frames.iter().rposition(loud)?;
        let from = first.saturating_sub(240);
        let to = (last + 2_400).min(frames.len() - 1);
        Some(
            frames[from..=to]
                .iter()
                .flat_map(|f| f.map(|v| (v.clamp(-1.0, 1.0) * 32767.0) as i16))
                .collect(),
        )
    }
}

/// Linear resampling to the device's rate; the unit itself sends 48 kHz.
fn to_48k(frames: Vec<[f32; 2]>, rate: u32) -> Vec<[f32; 2]> {
    if rate == 48_000 || frames.len() < 2 {
        return frames;
    }
    let step = f64::from(rate) / SOURCE_RATE;
    let out_len = ((frames.len() - 1) as f64 / step) as usize;
    (0..out_len)
        .map(|n| {
            let pos = n as f64 * step;
            let i = pos as usize;
            let t = (pos - i as f64) as f32;
            let (a, b) = (frames[i], frames[(i + 1).min(frames.len() - 1)]);
            [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
        })
        .collect()
}
