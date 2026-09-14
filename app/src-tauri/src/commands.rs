//! Commands the frontend invokes. Errors reach it as display strings.

use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use sp404_device::{Device, find_ports};
use sp404_formats::pattern::{Event, PPQ};
use sp404_formats::{Pattern, Sample, smp};
use sp404_proto::PadIndex;
use sp404_proto::params::{self, Scope};
use tauri::State;

use crate::AppState;

type CmdResult<T> = Result<T, String>;

/// A failed device call. `Closed` means the link is gone, so the device is dropped.
enum Failure {
    Device(sp404_device::Error),
    Other(String),
}

impl From<sp404_device::Error> for Failure {
    fn from(e: sp404_device::Error) -> Self {
        Failure::Device(e)
    }
}

impl From<sp404_formats::FormatError> for Failure {
    fn from(e: sp404_formats::FormatError) -> Self {
        Failure::Other(e.to_string())
    }
}

impl From<String> for Failure {
    fn from(e: String) -> Self {
        Failure::Other(e)
    }
}

/// Run `f` with the open device on the blocking pool.
async fn with_device<T, F>(state: &AppState, f: F) -> CmdResult<T>
where
    T: Send + 'static,
    F: FnOnce(&Device) -> Result<T, Failure> + Send + 'static,
{
    let device = state
        .device
        .lock()
        .unwrap()
        .clone()
        .ok_or("not connected")?;
    let worker = device.clone();
    let result = tauri::async_runtime::spawn_blocking(move || f(&worker))
        .await
        .map_err(|e| e.to_string())?;
    match result {
        Ok(value) => Ok(value),
        Err(Failure::Device(sp404_device::Error::Closed)) => {
            let mut slot = state.device.lock().unwrap();
            if slot.as_ref().is_some_and(|d| Arc::ptr_eq(d, &device)) {
                *slot = None;
            }
            Err("connection to the SP-404MKII was lost".into())
        }
        Err(Failure::Device(e)) => Err(e.to_string()),
        Err(Failure::Other(e)) => Err(e),
    }
}

fn pad_index(index: u16) -> Result<PadIndex, Failure> {
    PadIndex::new(index).ok_or_else(|| Failure::Other(format!("no pad with index {index}")))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PortDto {
    name: String,
    product: Option<String>,
    serial_number: Option<String>,
}

#[tauri::command]
pub async fn list_ports() -> CmdResult<Vec<PortDto>> {
    tauri::async_runtime::spawn_blocking(|| {
        find_ports().map(|ports| {
            ports
                .into_iter()
                .map(|p| PortDto {
                    name: p.name,
                    product: p.product,
                    serial_number: p.serial_number,
                })
                .collect()
        })
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())
}

/// Open `port`, or the first SP-404MKII found. Returns the port name. An existing
/// connection is closed first, since the port can only be opened once.
#[tauri::command]
pub async fn connect(state: State<'_, AppState>, port: Option<String>) -> CmdResult<String> {
    let previous = state.device.lock().unwrap().take();
    drop(previous);
    let device = tauri::async_runtime::spawn_blocking(move || match port {
        Some(p) => Device::open(&p),
        None => Device::open_first(),
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| format!("{e} (is another program using the device?)"))?;
    let name = device.port_name().to_string();
    *state.device.lock().unwrap() = Some(Arc::new(device));
    Ok(name)
}

#[tauri::command]
pub fn disconnect(state: State<'_, AppState>) {
    state.device.lock().unwrap().take();
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BankDto {
    letter: char,
    /// BPM × 100.
    tempo: u16,
    volume: u8,
    protected: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusDto {
    port: String,
    /// 1-based.
    project: u8,
    project_name: String,
    uses_project_tempo: bool,
    /// BPM × 100.
    project_tempo: u16,
    banks: Vec<BankDto>,
    free_kb: u32,
    selected_pad: Option<u16>,
    /// Status byte 12: 4 while the device shows a menu, 0 otherwise (other values unknown).
    working_mode: Option<u8>,
}

#[tauri::command]
pub async fn device_status(state: State<'_, AppState>) -> CmdResult<StatusDto> {
    with_device(&state, |dev| {
        let st = dev.status()?;
        let free_kb = dev.free_kb()?;
        Ok(StatusDto {
            port: dev.port_name().to_string(),
            project: st.project + 1,
            project_name: st.settings.name(),
            uses_project_tempo: st.settings.uses_project_tempo(),
            project_tempo: st.settings.project_tempo(),
            banks: (0..10)
                .map(|b| {
                    let bank = st.settings.bank(b);
                    BankDto {
                        letter: (b'A' + b as u8) as char,
                        tempo: bank.tempo,
                        volume: bank.volume,
                        protected: bank.protected,
                    }
                })
                .collect(),
            free_kb,
            selected_pad: st.selected.map(PadIndex::index),
            working_mode: st.raw.get(12).copied(),
        })
    })
    .await
}

/// The 16 project names; empty for unnamed slots.
#[tauri::command]
pub async fn project_names(state: State<'_, AppState>) -> CmdResult<Vec<String>> {
    with_device(&state, |dev| Ok(dev.project_names()?)).await
}

/// Make `project` (1-based) the current project.
#[tauri::command]
pub async fn select_project(state: State<'_, AppState>, project: u8) -> CmdResult<()> {
    with_device(&state, move |dev| {
        if !(1..=16).contains(&project) {
            return Err(format!("no project {project}").into());
        }
        Ok(dev.select_project(project - 1)?)
    })
    .await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PadDto {
    index: u16,
    label: String,
    has_sample: bool,
    name: String,
    level: i32,
    /// BPM × 100.
    bpm: i32,
    file_size: u32,
}

#[tauri::command]
pub async fn pads(state: State<'_, AppState>) -> CmdResult<Vec<PadDto>> {
    with_device(&state, |dev| {
        Ok(dev
            .pad_blocks()?
            .iter()
            .map(|b| PadDto {
                index: b.pad.index(),
                label: b.pad.to_string(),
                has_sample: b.has_sample(),
                name: if b.has_sample() {
                    b.name()
                } else {
                    String::new()
                },
                level: b.param(0x69).unwrap_or(0),
                bpm: b.param(0x6F).unwrap_or(0),
                file_size: b.file_size(),
            })
            .collect())
    })
    .await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParamDto {
    name: &'static str,
    value: i32,
    min: i32,
    max: i32,
    help: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SampleInfoDto {
    channels: u16,
    frames: u32,
    /// Start, end and loop top in sample frames.
    start: u32,
    end: u32,
    loop_top: u32,
    chop_points: Vec<u32>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PadDetailDto {
    index: u16,
    label: String,
    name: String,
    sample: Option<SampleInfoDto>,
    params: Vec<ParamDto>,
}

#[tauri::command]
pub async fn pad_detail(state: State<'_, AppState>, pad: u16) -> CmdResult<PadDetailDto> {
    with_device(&state, move |dev| {
        let pad = pad_index(pad)?;
        let block = dev.pad_block(pad)?;
        let sample = if block.has_sample() {
            let project = dev.current_project()? + 1;
            let head = dev
                .read_prefix(&pad.sample_path(project), smp::HEADER_LEN as u32)?
                .ok_or_else(|| format!("{pad}: sample file is missing"))?;
            let channels = smp::HeaderInfo::parse(&head)?.channels.max(1);
            // Pad blocks hold byte offsets into the SMP file.
            let to_frame = |bytes: u32| {
                bytes.saturating_sub(smp::HEADER_LEN as u32) / (2 * u32::from(channels))
            };
            let (start, end) = block.start_end_bytes();
            Some(SampleInfoDto {
                channels,
                frames: to_frame(block.file_size()),
                start: to_frame(start),
                end: to_frame(end),
                loop_top: to_frame(block.loop_start_bytes()),
                chop_points: block.chop_points().into_iter().flatten().collect(),
            })
        } else {
            None
        };
        let params = params::PARAMS
            .iter()
            .filter(|p| p.scope == Scope::Pad)
            .filter_map(|p| {
                block.param(p.id).map(|value| ParamDto {
                    name: p.name,
                    value,
                    min: p.min,
                    max: p.max,
                    help: p.help,
                })
            })
            .collect();
        Ok(PadDetailDto {
            index: pad.index(),
            label: pad.to_string(),
            name: if block.has_sample() {
                block.name()
            } else {
                String::new()
            },
            sample,
            params,
        })
    })
    .await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WaveformDto {
    channels: u16,
    frames: u32,
    /// Per channel: `[min, max]` for each point.
    peaks: Vec<Vec<[i16; 2]>>,
}

/// Waveform of a pad's sample, computed from the SMP file. The device's own peaks
/// request can return data from a previously loaded project, so it isn't used here.
#[tauri::command]
pub async fn waveform(state: State<'_, AppState>, pad: u16, points: u32) -> CmdResult<WaveformDto> {
    with_device(&state, move |dev| {
        let pad = pad_index(pad)?;
        let project = dev.current_project()? + 1;
        let sample = Sample::from_smp(&dev.read_file(&pad.sample_path(project))?)?;
        Ok(peaks(&sample, points.clamp(1, 16_384) as usize))
    })
    .await
}

fn peaks(sample: &Sample, points: usize) -> WaveformDto {
    let channels = usize::from(sample.channels.max(1));
    let frames = sample.frames();
    let points = points.min(frames.max(1));
    let peaks = (0..channels)
        .map(|ch| {
            (0..points)
                .map(|i| {
                    let from = i * frames / points;
                    let to = ((i + 1) * frames / points).max(from + 1).min(frames);
                    let mut range = [0i16, 0i16];
                    for f in from..to {
                        let v = sample.samples[f * channels + ch];
                        range[0] = range[0].min(v);
                        range[1] = range[1].max(v);
                    }
                    range
                })
                .collect()
        })
        .collect();
    WaveformDto {
        channels: sample.channels,
        frames: frames as u32,
        peaks,
    }
}

/// Play a pad on the device for `ms` milliseconds.
#[tauri::command]
pub async fn preview_pad(state: State<'_, AppState>, pad: u16, ms: u64) -> CmdResult<()> {
    with_device(&state, move |dev| {
        let pad = pad_index(pad)?;
        Ok(dev.preview(pad, Duration::from_millis(ms.min(10_000)))?)
    })
    .await
}

/// Whether each of the 160 pattern slots holds a pattern.
#[tauri::command]
pub async fn patterns(state: State<'_, AppState>) -> CmdResult<Vec<bool>> {
    with_device(&state, |dev| {
        Ok(PadIndex::all()
            .map(|slot| dev.pattern_exists(slot))
            .collect::<Result<_, _>>()?)
    })
    .await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteDto {
    tick: u32,
    pad: u16,
    velocity: u8,
    length: u16,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PatternDto {
    index: u16,
    label: String,
    length_ticks: u32,
    ppq: u16,
    beats_per_bar: Option<u8>,
    /// The bank tempo the pattern plays at, BPM × 100.
    bank_tempo: u16,
    notes: Vec<NoteDto>,
    control_events: usize,
    pads_used: Vec<u16>,
}

/// Read a pattern from the card of the current project.
#[tauri::command]
pub async fn pattern_detail(state: State<'_, AppState>, slot: u16) -> CmdResult<PatternDto> {
    with_device(&state, move |dev| {
        let slot = pad_index(slot)?;
        let project = dev.current_project()? + 1;
        let path = format!(
            "ROLAND/SP-404MKII/PROJECT_{project:02}/PTN/PTN{:05}.BIN",
            slot.index() + 1
        );
        let pattern = Pattern::parse(&dev.read_file(&path)?)?;
        let settings = dev.project_settings(project - 1)?;
        let mut notes = Vec::new();
        let mut control_events = 0;
        for event in &pattern.events {
            match *event {
                Event::Note {
                    tick,
                    pad,
                    velocity,
                    length,
                    ..
                } => notes.push(NoteDto {
                    tick,
                    pad: pad.index(),
                    velocity,
                    length,
                }),
                Event::Control { .. } => control_events += 1,
            }
        }
        let mut pads_used: Vec<u16> = notes.iter().map(|n| n.pad).collect();
        pads_used.sort_unstable();
        pads_used.dedup();
        Ok(PatternDto {
            index: slot.index(),
            label: slot.to_string(),
            length_ticks: pattern.length,
            ppq: PPQ,
            beats_per_bar: pattern.beats_per_bar(),
            bank_tempo: settings.bank(usize::from(slot.bank())).tempo,
            notes,
            control_events,
            pads_used,
        })
    })
    .await
}
