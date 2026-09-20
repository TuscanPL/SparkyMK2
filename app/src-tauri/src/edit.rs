//! Commands that change the device. Each one reads the changed state back, so the
//! frontend shows what the device holds rather than what it asked for.

use std::path::Path;

use serde::Serialize;
use sp404_device::Device;
use sp404_dsp::TempoRange;
use sp404_formats::{Sample, audio, smp};
use sp404_proto::PadIndex;
use sp404_proto::control::{MoveMode, PadOp};
use sp404_proto::params::{self, Scope, Target};
use tauri::State;

use crate::AppState;
use crate::commands::{
    CmdResult, Failure, PadState, StatusDto, pad_index, read_pad, read_status, with_device,
};

const MODE_FLAGS: u8 = 0x70;
const ONE_SHOT: i32 = 0x20;
const GATE: u8 = 0x6A;
const LOOP: u8 = 0x6B;
/// Cleared by the official app when One Shot is turned on; meaning unknown.
const CLEARED_BY_ONE_SHOT: u8 = 0x72;
const BPM: u8 = 0x6F;
const BPM_RANGE: std::ops::RangeInclusive<i32> = 4000..=20000;
const CHOP_POINTS: u8 = 16;
/// Longest sample name the pad block holds.
const SAMPLE_NAME_LEN: usize = 23;
/// Longest project name: 32 bytes with a terminating byte.
const PROJECT_NAME_LEN: usize = 31;

fn letter(bank: u16) -> char {
    (b'A' + bank as u8) as char
}

/// Refuse to change a pad in a protected bank, as the official app does.
fn check_unprotected(dev: &Device, pad: PadIndex) -> Result<(), Failure> {
    let project = dev.current_project()?;
    if dev
        .project_settings(project)?
        .bank(usize::from(pad.bank()))
        .protected
    {
        return Err(format!("bank {} is protected", letter(pad.bank())).into());
    }
    Ok(())
}

fn printable(name: &str, max: usize) -> Result<String, Failure> {
    let name = name.trim();
    if name.is_empty() {
        return Err("the name is empty".to_string().into());
    }
    if let Some(c) = name.chars().find(|c| !(' '..='~').contains(c)) {
        return Err(format!("{c:?} can't be used in a name on the SP-404MKII").into());
    }
    Ok(name.chars().take(max).collect())
}

/// Frames of a pad's sample, or an error for an empty pad.
fn sample_frames(state: &PadState, pad: PadIndex) -> Result<u32, Failure> {
    state
        .detail
        .sample
        .as_ref()
        .map(|s| s.frames)
        .ok_or_else(|| format!("{pad} is empty").into())
}

#[tauri::command]
pub async fn set_pad_param(
    state: State<'_, AppState>,
    pad: u16,
    name: String,
    value: i32,
) -> CmdResult<PadState> {
    with_device(&state, move |dev| {
        let pad = pad_index(pad)?;
        let info = params::by_name(&name)
            .filter(|p| p.scope == Scope::Pad)
            .ok_or_else(|| format!("unknown pad parameter {name:?}"))?;
        if !(info.min..=info.max).contains(&value) {
            return Err(format!("{} must be in {}..{}", info.name, info.min, info.max).into());
        }
        check_unprotected(dev, pad)?;
        let before = read_pad(dev, pad)?;
        if matches!(info.name, "start" | "end" | "loop-top") {
            let frames = sample_frames(&before, pad)?;
            if value as u32 > frames {
                return Err(format!("{} is beyond the sample's {frames} frames", info.name).into());
            }
        }
        let target = Target::Pad(pad);
        dev.set_param(info.id, target, value)?;
        let old_flags = before
            .detail
            .params
            .iter()
            .find(|p| p.name == "mode-flags")
            .map_or(0, |p| p.value);
        if info.id == MODE_FLAGS && value & ONE_SHOT != 0 && old_flags & ONE_SHOT == 0 {
            for id in [GATE, LOOP, CLEARED_BY_ONE_SHOT] {
                dev.set_param(id, target, 0)?;
            }
        }
        read_pad(dev, pad)
    })
    .await
}

/// Replace a pad's chop points. Points are sorted, ones beyond the sample are dropped, and
/// a non-empty set always starts with a chop at frame 0, as chops made on the device do.
#[tauri::command]
pub async fn set_chop_points(
    state: State<'_, AppState>,
    pad: u16,
    points: Vec<u32>,
) -> CmdResult<PadState> {
    with_device(&state, move |dev| {
        let pad = pad_index(pad)?;
        check_unprotected(dev, pad)?;
        let frames = sample_frames(&read_pad(dev, pad)?, pad)?;
        let mut points: Vec<u32> = points.into_iter().filter(|&p| p < frames).collect();
        if !points.is_empty() {
            points.push(0);
        }
        points.sort_unstable();
        points.dedup();
        if points.len() > usize::from(CHOP_POINTS) {
            return Err(format!("a pad holds at most {CHOP_POINTS} chop points").into());
        }
        dev.set_chop_points(pad, &points)?;
        read_pad(dev, pad)
    })
    .await
}

#[tauri::command]
pub async fn rename_sample(
    state: State<'_, AppState>,
    pad: u16,
    name: String,
) -> CmdResult<PadState> {
    with_device(&state, move |dev| {
        let pad = pad_index(pad)?;
        let name = printable(&name, SAMPLE_NAME_LEN)?;
        check_unprotected(dev, pad)?;
        sample_frames(&read_pad(dev, pad)?, pad)?;
        dev.set_sample_name(pad, &name)?;
        read_pad(dev, pad)
    })
    .await
}

/// Truncate, normalize or delete a pad's sample.
#[tauri::command]
pub async fn pad_operation(
    state: State<'_, AppState>,
    pad: u16,
    operation: String,
) -> CmdResult<PadState> {
    with_device(&state, move |dev| {
        let pad = pad_index(pad)?;
        let op = match operation.as_str() {
            "truncate" => PadOp::Truncate,
            "normalize" => PadOp::Normalize,
            "delete" => PadOp::Delete,
            other => return Err(format!("unknown pad operation {other:?}").into()),
        };
        check_unprotected(dev, pad)?;
        sample_frames(&read_pad(dev, pad)?, pad)?;
        dev.pad_op(op, pad)?;
        read_pad(dev, pad)
    })
    .await
}

/// Move a sample to another pad, replacing what is there, or swap the two pads.
#[tauri::command]
pub async fn move_sample(
    state: State<'_, AppState>,
    from: u16,
    to: u16,
    exchange: bool,
) -> CmdResult<()> {
    with_device(&state, move |dev| {
        let (from, to) = (pad_index(from)?, pad_index(to)?);
        if from == to {
            return Ok(());
        }
        check_unprotected(dev, from)?;
        check_unprotected(dev, to)?;
        sample_frames(&read_pad(dev, from)?, from)?;
        let mode = if exchange {
            MoveMode::Exchange
        } else {
            MoveMode::Overwrite
        };
        Ok(dev.move_sample(from, to, mode)?)
    })
    .await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    state: PadState,
    source_rate: u32,
    source_channels: usize,
    /// BPM × 100, when detection was asked for and found a tempo.
    detected_bpm: Option<i32>,
}

/// Import an audio file onto a pad of the current project, optionally detecting its tempo
/// (the app's Auto Detect BPM). `bpm_range` is the device's detect range preset.
#[tauri::command]
pub async fn import_audio(
    state: State<'_, AppState>,
    pad: u16,
    path: String,
    detect_bpm: bool,
    bpm_range: u8,
) -> CmdResult<ImportResult> {
    with_device(&state, move |dev| {
        let pad = pad_index(pad)?;
        let path = Path::new(&path);
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase)
            .unwrap_or_default();
        if !audio::EXTENSIONS.contains(&ext.as_str()) {
            return Err(format!(
                "{} isn't a supported audio file (WAV, AIFF, FLAC or MP3)",
                path.file_name()
                    .map_or_else(Default::default, |n| n.to_string_lossy())
            )
            .into());
        }
        check_unprotected(dev, pad)?;
        let imported = audio::read(path)?;
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        store_import(dev, pad, imported, &stem, detect_bpm, bpm_range)
    })
    .await
}

/// Import a sound already on the device (an SD card file) straight onto a pad, without
/// a round trip through the computer's disk.
#[tauri::command]
pub async fn import_from_device(
    state: State<'_, AppState>,
    pad: u16,
    volume: String,
    remote: String,
    detect_bpm: bool,
    bpm_range: u8,
) -> CmdResult<ImportResult> {
    with_device(&state, move |dev| {
        let pad = pad_index(pad)?;
        let name = remote.rsplit('/').next().unwrap_or(&remote).to_string();
        let (stem, ext) = match name.rsplit_once('.') {
            Some((stem, ext)) => (stem.to_string(), ext.to_ascii_lowercase()),
            None => (name.clone(), String::new()),
        };
        if !audio::EXTENSIONS.contains(&ext.as_str()) {
            return Err(
                format!("{name} isn't a supported audio file (WAV, AIFF, FLAC or MP3)").into(),
            );
        }
        check_unprotected(dev, pad)?;
        let bytes = dev.read_file(&crate::files::qualify(&volume, &remote)?)?;
        let imported = audio::read_bytes(bytes, &ext)?;
        store_import(dev, pad, imported, &stem, detect_bpm, bpm_range)
    })
    .await
}

/// Write a decoded sound to a pad and, if asked, detect and store its tempo.
fn store_import(
    dev: &sp404_device::Device,
    pad: PadIndex,
    imported: audio::Imported,
    stem: &str,
    detect_bpm: bool,
    bpm_range: u8,
) -> Result<ImportResult, Failure> {
    // Keep the printable part of the file name; the device accepts ASCII only.
    let name: String = stem
        .chars()
        .filter(|c| (' '..='~').contains(c))
        .take(SAMPLE_NAME_LEN)
        .collect();
    let name = if name.trim().is_empty() {
        "Sample".to_string()
    } else {
        name
    };
    let project = dev.current_project()? + 1;
    dev.import_smp(project, pad, &imported.sample.to_smp(), &name)?;
    let detected_bpm = if detect_bpm {
        let detected = detect_tempo(&imported.sample, bpm_range);
        if let Some(bpm) = detected {
            dev.set_param(BPM, Target::Pad(pad), bpm)?;
        }
        detected
    } else {
        None
    };
    Ok(ImportResult {
        state: read_pad(dev, pad)?,
        source_rate: imported.source_rate,
        source_channels: imported.source_channels,
        detected_bpm,
    })
}

fn detect_tempo(sample: &Sample, preset: u8) -> Option<i32> {
    let mono = sp404_dsp::mono_from_i16(&sample.samples, usize::from(sample.channels.max(1)));
    sp404_dsp::tempo::detect(&mono, smp::SAMPLE_RATE, TempoRange::device_preset(preset))
        .map(|t| (t.bpm_x100() as i32).clamp(*BPM_RANGE.start(), *BPM_RANGE.end()))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BpmResult {
    state: PadState,
    /// BPM × 100 that was stored, or none if no tempo was found.
    bpm: Option<i32>,
}

/// Analyze BPM (`detect`, whole sample) or Set BPM by St/End (`length`), storing the result.
#[tauri::command]
pub async fn analyze_bpm(
    state: State<'_, AppState>,
    pad: u16,
    mode: String,
    bpm_range: u8,
) -> CmdResult<BpmResult> {
    with_device(&state, move |dev| {
        let pad = pad_index(pad)?;
        check_unprotected(dev, pad)?;
        let before = read_pad(dev, pad)?;
        let sample = before
            .detail
            .sample
            .as_ref()
            .ok_or_else(|| format!("{pad} is empty"))?;
        let bpm = match mode.as_str() {
            "detect" => {
                let project = dev.current_project()? + 1;
                let audio = Sample::from_smp(&dev.read_file(&pad.sample_path(project))?)?;
                detect_tempo(&audio, bpm_range)
            }
            "length" => {
                let current = before.pad.bpm.max(0) as u32;
                sp404_dsp::tempo::bpm_from_length(sample.end.saturating_sub(sample.start), current)
                    .map(|b| (b as i32).clamp(*BPM_RANGE.start(), *BPM_RANGE.end()))
            }
            other => return Err(format!("unknown BPM mode {other:?}").into()),
        };
        if let Some(value) = bpm {
            dev.set_param(BPM, Target::Pad(pad), value)?;
        }
        Ok(BpmResult {
            state: read_pad(dev, pad)?,
            bpm,
        })
    })
    .await
}

/// Project and bank settings: `project-tempo`, `tempo-select`, and `bank-tempo-a`,
/// `bank-volume-a` or `bank-protect-a` through `…-j`.
#[tauri::command]
pub async fn set_global_param(
    state: State<'_, AppState>,
    name: String,
    value: i32,
) -> CmdResult<StatusDto> {
    with_device(&state, move |dev| {
        let (id, stored, range) = if let Some((id, inverted)) = params::bank_param(&name) {
            let range = if name.starts_with("bank-tempo") {
                BPM_RANGE
            } else if name.starts_with("bank-volume") {
                0..=127
            } else {
                0..=1
            };
            (id, if inverted { 127 - value } else { value }, range)
        } else {
            let info = params::by_name(&name)
                .filter(|p| matches!(p.name, "project-tempo" | "tempo-select"))
                .ok_or_else(|| format!("unknown setting {name:?}"))?;
            (info.id, value, info.min..=info.max)
        };
        if !range.contains(&value) {
            return Err(format!("{name} must be in {}..{}", range.start(), range.end()).into());
        }
        dev.set_param(id, Target::Global, stored)?;
        read_status(dev)
    })
    .await
}

/// Rename a project (1-based). Returns the 16 project names.
#[tauri::command]
pub async fn rename_project(
    state: State<'_, AppState>,
    project: u8,
    name: String,
) -> CmdResult<Vec<String>> {
    with_device(&state, move |dev| {
        if !(1..=16).contains(&project) {
            return Err(format!("no project {project}").into());
        }
        let name = printable(&name, PROJECT_NAME_LEN)?;
        dev.set_project_name(project - 1, &name)?;
        Ok(dev.project_names()?)
    })
    .await
}
