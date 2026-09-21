//! Exporting pads as WAV files, and putting such an export back on the same pads.
//!
//! Each file is named after its pad (`C05 Kick.wav`), so a folder sorts in pad order, and
//! a bank or project export carries a sidecar, [`SIDECAR`], with every pad's settings.
//! Restoring reads the sidecar and puts each sound back on its own pad with its settings,
//! so a bank with gaps comes back with the same gaps. Exports go to the SD card or to a
//! folder on the computer.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use sp404_device::Device;
use sp404_formats::{Sample, audio, wav};
use sp404_proto::PadIndex;
use sp404_proto::params::{self, Scope, Target};
use tauri::{AppHandle, Emitter, State};

use crate::AppState;
use crate::commands::{CmdResult, Failure, pad_index, read_pad, with_device};
use crate::edit::{check_unprotected, store_import};

/// The settings file written next to the sounds of a bank or project export.
pub const SIDECAR: &str = "sparkymk2.json";
const FORMAT: &str = "sparkymk2-pads";

/// Parameters that the pad block stores as byte offsets but that are written in frames.
/// They are taken from the sample's frame positions instead of the raw block.
const MARKERS: [&str; 3] = ["start", "end", "loop-top"];

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Sidecar {
    format: String,
    version: u32,
    /// Informational: the project the pads came from.
    project: String,
    pads: Vec<SavedPad>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedPad {
    /// `C05`: the pad the sound goes back to.
    pad: String,
    file: String,
    name: String,
    /// Pad parameters by name, markers in sample frames.
    params: BTreeMap<String, i32>,
    chop_points: Vec<u32>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Progress {
    name: String,
    done: u64,
    total: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportSummary {
    written: usize,
    /// Pads asked for that hold no sample.
    empty: usize,
    folder: String,
}

/// `C05`: bank letter and two-digit pad, so file names sort in pad order.
fn label(pad: PadIndex) -> String {
    format!("{}{:02}", (b'A' + pad.bank() as u8) as char, pad.pad() + 1)
}

/// The pad a label or a file name starts with: `C05`, `C05 Kick.wav`.
fn pad_of(name: &str) -> Option<PadIndex> {
    let mut chars = name.chars();
    let bank = chars.next()?.to_ascii_uppercase();
    let digits: String = chars.by_ref().take(2).collect();
    if !('A'..='J').contains(&bank) || digits.len() != 2 {
        return None;
    }
    let pad: u16 = digits.parse().ok()?;
    // A label ends there, or at the space before the sample's name.
    if !matches!(chars.next(), None | Some(' ')) || !(1..=16).contains(&pad) {
        return None;
    }
    PadIndex::from_bank_pad(bank as u16 - 'A' as u16, pad - 1)
}

/// Keep a name usable on the card's FAT filesystem and on the computer.
fn sanitize(name: &str) -> String {
    let clean: String = name
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    let clean = clean.trim().trim_end_matches('.').to_string();
    if clean.is_empty() {
        "Sample".to_string()
    } else {
        clean
    }
}

fn file_name(pad: PadIndex, name: &str) -> String {
    format!("{} {}.wav", label(pad), sanitize(name))
}

/// Where an export lives: the SD card (a card path) or the computer (a local path).
enum Place {
    Card(String),
    Local(PathBuf),
}

impl Place {
    fn new(target: &str, folder: &str) -> Result<Self, Failure> {
        match target {
            "card" => {
                if folder.split('/').any(|part| part == "..") {
                    return Err(Failure::Other(format!("{folder} is not a card folder")));
                }
                Ok(Place::Card(folder.trim_matches('/').to_string()))
            }
            "local" => Ok(Place::Local(PathBuf::from(folder))),
            other => Err(Failure::Other(format!("nowhere called {other:?}"))),
        }
    }

    fn card_path(dir: &str, name: &str) -> String {
        let joined = if dir.is_empty() {
            name.to_string()
        } else {
            format!("{dir}/{name}")
        };
        format!("{}{joined}", sp404_proto::fileapi::CARD)
    }

    /// Make the folder, and any parents, if they are not there yet.
    fn create(&self, dev: &Device) -> Result<(), Failure> {
        match self {
            Place::Local(dir) => std::fs::create_dir_all(dir)
                .map_err(|e| Failure::Other(format!("creating {}: {e}", dir.display()))),
            Place::Card(dir) => {
                let mut walked = String::new();
                for part in dir.split('/').filter(|p| !p.is_empty()) {
                    walked = if walked.is_empty() {
                        part.to_string()
                    } else {
                        format!("{walked}/{part}")
                    };
                    let path = format!("{}{walked}", sp404_proto::fileapi::CARD);
                    if dev.stat(&path)?.is_none() {
                        dev.mkdir(&path)?;
                    }
                }
                Ok(())
            }
        }
    }

    fn write(&self, dev: &Device, name: &str, data: &[u8]) -> Result<(), Failure> {
        match self {
            Place::Local(dir) => {
                let path = dir.join(name);
                std::fs::write(&path, data)
                    .map_err(|e| Failure::Other(format!("writing {}: {e}", path.display())))
            }
            Place::Card(dir) => Ok(dev.write_file(&Self::card_path(dir, name), data)?),
        }
    }

    fn read(&self, dev: &Device, name: &str) -> Result<Vec<u8>, Failure> {
        match self {
            Place::Local(dir) => {
                let path = dir.join(name);
                std::fs::read(&path)
                    .map_err(|e| Failure::Other(format!("reading {}: {e}", path.display())))
            }
            Place::Card(dir) => Ok(dev.read_file(&Self::card_path(dir, name))?),
        }
    }
}

fn emit(app: &AppHandle, name: &str, done: usize, total: usize) {
    let _ = app.emit(
        "transfer",
        Progress {
            name: name.to_string(),
            done: done as u64,
            total: total as u64,
        },
    );
}

/// A pad's settings as the sidecar keeps them, or `None` for an empty pad. One read of
/// the pad covers the name, the settings and the markers.
fn snapshot(dev: &Device, pad: PadIndex) -> Result<Option<SavedPad>, Failure> {
    let state = read_pad(dev, pad)?;
    let Some(sample) = &state.detail.sample else {
        return Ok(None);
    };
    let mut params: BTreeMap<String, i32> = state
        .detail
        .params
        .iter()
        .filter(|p| !MARKERS.contains(&p.name))
        .map(|p| (p.name.to_string(), p.value))
        .collect();
    // The block holds these as byte offsets; the sample info has them in frames.
    params.insert("start".into(), sample.start as i32);
    params.insert("end".into(), sample.end as i32);
    params.insert("loop-top".into(), sample.loop_top as i32);
    let name = state.detail.name.clone();
    Ok(Some(SavedPad {
        pad: label(pad),
        file: file_name(pad, &name),
        name,
        params,
        chop_points: sample.chop_points.clone(),
    }))
}

/// Export pads as WAV files into `folder`: `target` is `card` or `local`. A `sidecar`
/// records each pad's settings so a restore can put everything back. Empty pads are
/// skipped.
#[tauri::command]
pub async fn export_pads(
    app: AppHandle,
    state: State<'_, AppState>,
    pads: Vec<u16>,
    target: String,
    folder: String,
    sidecar: bool,
) -> CmdResult<ExportSummary> {
    with_device(&state, move |dev| {
        let place = Place::new(&target, &folder)?;
        place.create(dev)?;
        let project = dev.current_project()? + 1;
        let project_name = dev
            .project_names()?
            .get(usize::from(project) - 1)
            .cloned()
            .unwrap_or_default();
        let mut saved = Vec::new();
        let mut empty = 0;
        for (i, &index) in pads.iter().enumerate() {
            let pad = pad_index(index)?;
            let Some(record) = snapshot(dev, pad)? else {
                empty += 1;
                continue;
            };
            emit(&app, &record.file, i, pads.len());
            let smp = dev.read_file(&pad.sample_path(project))?;
            let sound = wav::to_bytes(&Sample::from_smp(&smp)?)?;
            place.write(dev, &record.file, &sound)?;
            saved.push(record);
        }
        if sidecar && !saved.is_empty() {
            let sheet = Sidecar {
                format: FORMAT.into(),
                version: 1,
                project: project_name,
                pads: saved,
            };
            let json = serde_json::to_vec_pretty(&sheet).map_err(|e| e.to_string())?;
            place.write(dev, SIDECAR, &json)?;
            Ok(ExportSummary {
                written: sheet.pads.len(),
                empty,
                folder,
            })
        } else {
            Ok(ExportSummary {
                written: saved.len(),
                empty,
                folder,
            })
        }
    })
    .await
}

fn parse_sidecar(bytes: &[u8]) -> Result<Sidecar, Failure> {
    let sheet: Sidecar = serde_json::from_slice(bytes)
        .map_err(|e| Failure::Other(format!("{SIDECAR} is not readable: {e}")))?;
    if sheet.format != FORMAT {
        return Err(Failure::Other(format!(
            "{SIDECAR} is not a SparkyMK2 export"
        )));
    }
    Ok(sheet)
}

/// Read an export's sidecar, so the frontend can say what a restore would replace.
#[tauri::command]
pub async fn read_export(
    state: State<'_, AppState>,
    target: String,
    folder: String,
) -> CmdResult<Sidecar> {
    with_device(&state, move |dev| {
        let place = Place::new(&target, &folder)?;
        parse_sidecar(&place.read(dev, SIDECAR)?)
    })
    .await
}

/// Put an export back: each sound on the pad its sidecar names, then that pad's settings.
/// Pads the export does not mention are left alone.
#[tauri::command]
pub async fn restore_pads(
    app: AppHandle,
    state: State<'_, AppState>,
    target: String,
    folder: String,
) -> CmdResult<usize> {
    with_device(&state, move |dev| {
        let place = Place::new(&target, &folder)?;
        let sheet = parse_sidecar(&place.read(dev, SIDECAR)?)?;
        let total = sheet.pads.len();
        for (i, saved) in sheet.pads.iter().enumerate() {
            let pad = pad_of(&saved.pad)
                .ok_or_else(|| Failure::Other(format!("{:?} is not a pad", saved.pad)))?;
            emit(&app, &saved.file, i, total);
            check_unprotected(dev, pad)?;
            let bytes = place.read(dev, &saved.file)?;
            let imported = audio::read_bytes(bytes, "wav")?;
            // The sidecar's settings follow, BPM among them, so skip detection.
            store_import(dev, pad, imported, &saved.name, false, 0)?;
            apply(dev, pad, saved)?;
        }
        Ok(total)
    })
    .await
}

/// Write a pad's saved settings back.
fn apply(dev: &Device, pad: PadIndex, saved: &SavedPad) -> Result<(), Failure> {
    let target = Target::Pad(pad);
    let set = |name: &str, value: i32| -> Result<(), Failure> {
        match params::by_name(name).filter(|p| p.scope == Scope::Pad) {
            Some(info) => Ok(dev.set_param(info.id, target, value)?),
            // A parameter this version does not know: leave it at its default.
            None => Ok(()),
        }
    };
    // Mode first: turning on one shot can clear gate and loop, and the saved values of
    // those should win.
    if let Some(&mode) = saved.params.get("mode-flags") {
        set("mode-flags", mode)?;
    }
    for (name, &value) in &saved.params {
        if name != "mode-flags" && !MARKERS.contains(&name.as_str()) {
            set(name, value)?;
        }
    }
    // Markers last, end before start so start never lands past a stale end.
    for name in ["end", "start", "loop-top"] {
        if let Some(&value) = saved.params.get(name) {
            set(name, value)?;
        }
    }
    if !saved.chop_points.is_empty() {
        dev.set_chop_points(pad, &saved.chop_points)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_sort_in_pad_order_and_read_back() {
        let c5 = PadIndex::from_bank_pad(2, 4).unwrap();
        assert_eq!(label(c5), "C05");
        assert_eq!(pad_of("C05"), Some(c5));
        assert_eq!(pad_of("C05 Kick.wav"), Some(c5));
        assert_eq!(pad_of("c05 Kick.wav"), Some(c5));
        assert_eq!(label(PadIndex::from_bank_pad(9, 15).unwrap()), "J16");
        assert!(label(c5) < label(PadIndex::from_bank_pad(2, 9).unwrap()));
    }

    #[test]
    fn rejects_names_that_are_not_pads() {
        assert_eq!(pad_of("K01 x.wav"), None);
        assert_eq!(pad_of("A17 x.wav"), None);
        assert_eq!(pad_of("A00 x.wav"), None);
        assert_eq!(pad_of("A1 x.wav"), None);
        assert_eq!(pad_of("A012 x.wav"), None);
        assert_eq!(pad_of("Kick.wav"), None);
        assert_eq!(pad_of(""), None);
    }

    #[test]
    fn names_survive_the_card() {
        assert_eq!(sanitize("Kick/Snare: 90?"), "Kick_Snare_ 90_");
        assert_eq!(sanitize("   "), "Sample");
        assert_eq!(sanitize("loop."), "loop");
        assert_eq!(file_name(PadIndex::new(0).unwrap(), "Kick"), "A01 Kick.wav");
    }

    #[test]
    fn a_sidecar_round_trips() {
        let sheet = Sidecar {
            format: FORMAT.into(),
            version: 1,
            project: "PROJECT_06".into(),
            pads: vec![SavedPad {
                pad: "C05".into(),
                file: "C05 Kick.wav".into(),
                name: "Kick".into(),
                params: BTreeMap::from([("level".into(), 100), ("start".into(), 0)]),
                chop_points: vec![0, 4800],
            }],
        };
        let json = serde_json::to_vec(&sheet).unwrap();
        let back = parse_sidecar(&json).ok().unwrap();
        assert_eq!(back.pads[0].pad, "C05");
        assert_eq!(back.pads[0].params["level"], 100);
        assert_eq!(back.pads[0].chop_points, vec![0, 4800]);
    }

    #[test]
    fn refuses_other_json() {
        assert!(parse_sidecar(br#"{"format":"x","version":1,"project":"","pads":[]}"#).is_err());
        assert!(parse_sidecar(b"not json").is_err());
    }
}
