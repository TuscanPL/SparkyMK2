//! Exporting pads as WAV files, and putting such an export back on the same pads.
//!
//! Each file is named after its pad (`C05 Kick.wav`), so a folder sorts in pad order, and
//! a bank or project export carries a sidecar, [`SIDECAR`], with every pad's settings.
//! Restoring reads the sidecar and puts each sound back on its own pad with its settings,
//! so a bank with gaps comes back with the same gaps. Exports go to the SD card or to a
//! folder on the computer.
//!
//! Patterns export the three ways the official app offers: a Standard MIDI File, a Bounce
//! (one WAV of the whole pattern) or MULTIPAD (one WAV per pad it plays). The device
//! renders the WAVs itself, in real time.
//!
//! A project backup is a copy of its whole folder, the same files the official app's
//! export holds, and restores into the current project.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sp404_device::Device;
use sp404_formats::{Pattern, Sample, audio, smf, wav};
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

/// A file name without the characters Windows, macOS or the card refuse.
fn file_safe(name: &str) -> String {
    name.chars()
        .map(|c| if r#"\/:*?"<>|"#.contains(c) { '_' } else { c })
        .collect::<String>()
        .trim()
        .to_string()
}

/// Export the pattern in `slot` of the current project into `folder` (`target` is `card`
/// or `local`). `format` is `smf` or `bounce`, written as `{name}.mid` or `{name}.wav`,
/// or `multipad`, one WAV per pad the pattern plays, named after the pad. Returns the
/// number of files written.
#[tauri::command]
pub async fn export_pattern(
    app: AppHandle,
    state: State<'_, AppState>,
    slot: u16,
    format: String,
    target: String,
    folder: String,
    name: String,
) -> CmdResult<usize> {
    with_device(&state, move |dev| {
        let slot = pad_index(slot)?;
        if !dev.pattern_exists(slot)? {
            return Err(Failure::Other(format!("there is no pattern in {slot}")));
        }
        let place = Place::new(&target, &folder)?;
        place.create(dev)?;
        let name = file_safe(&name);
        match format.as_str() {
            "smf" => {
                let project = dev.current_project()? + 1;
                let path = format!(
                    "ROLAND/SP-404MKII/PROJECT_{project:02}/PTN/PTN{:05}.BIN",
                    slot.index() + 1
                );
                let pattern = Pattern::parse(&dev.read_file(&path)?)?;
                // The official app writes the tempo of the pattern's bank.
                let tempo = dev
                    .project_settings(project - 1)?
                    .bank(usize::from(slot.bank()))
                    .tempo;
                place.write(
                    dev,
                    &format!("{name}.mid"),
                    &smf::from_pattern(&pattern, tempo),
                )?;
                Ok(1)
            }
            "bounce" => {
                let file = format!("{name}.wav");
                emit(&app, &file, 0, 1);
                let mut out = std::io::Cursor::new(Vec::new());
                dev.bounce_pattern(slot, &mut out)?;
                place.write(dev, &file, out.get_ref())?;
                Ok(1)
            }
            "multipad" => {
                let pads = dev.pattern_pads(slot)?;
                for (i, &pad) in pads.iter().enumerate() {
                    let pad_name = dev.pad_block(pad)?.name();
                    let pad_name = pad_name.trim_end();
                    let file = if pad_name.is_empty() {
                        format!("{}.wav", label(pad))
                    } else {
                        format!("{} {}.wav", label(pad), file_safe(pad_name))
                    };
                    emit(&app, &file, i, pads.len());
                    let mut out = std::io::Cursor::new(Vec::new());
                    dev.render_pattern_pad(slot, pad, &mut out)?;
                    place.write(dev, &file, out.get_ref())?;
                }
                Ok(pads.len())
            }
            other => Err(Failure::Other(format!("no export format called {other:?}"))),
        }
    })
    .await
}

/// The files of a project folder on the device, relative to it, sub-folders included.
fn project_files(dev: &Device, root: &str) -> Result<Vec<String>, Failure> {
    let mut files = Vec::new();
    let mut stack = vec![String::new()];
    while let Some(rel) = stack.pop() {
        let dir = if rel.is_empty() {
            root.to_string()
        } else {
            format!("{root}/{rel}")
        };
        for e in dev.list_dir(&dir)? {
            let child = if rel.is_empty() {
                e.name.clone()
            } else {
                format!("{rel}/{}", e.name)
            };
            if e.is_dir() {
                stack.push(child);
            } else {
                files.push(child);
            }
        }
    }
    files.sort();
    Ok(files)
}

/// Copy the whole folder of `project` (1-based) into `folder` on the computer.
/// Returns the number of files.
#[tauri::command]
pub async fn backup_project(
    app: AppHandle,
    state: State<'_, AppState>,
    project: u8,
    folder: String,
) -> CmdResult<usize> {
    with_device(&state, move |dev| {
        if !(1..=16).contains(&project) {
            return Err(Failure::Other(format!("there is no project {project}")));
        }
        let root = format!("ROLAND/SP-404MKII/PROJECT_{project:02}");
        let files = project_files(dev, &root)?;
        let base = PathBuf::from(&folder);
        for (i, rel) in files.iter().enumerate() {
            emit(&app, rel, i, files.len());
            let data = dev.read_file(&format!("{root}/{rel}"))?;
            let path = base.join(rel);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| Failure::Other(format!("creating {}: {e}", parent.display())))?;
            }
            std::fs::write(&path, data)
                .map_err(|e| Failure::Other(format!("writing {}: {e}", path.display())))?;
        }
        Ok(files.len())
    })
    .await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupInfo {
    /// The project folder itself, which may be inside the folder that was picked.
    folder: String,
    name: String,
    files: usize,
    bytes: u64,
}

/// The project folder in `picked`: `picked` itself when it holds `PADCONF.BIN`, or the
/// one project under `ROLAND/SP-404MKII`, as the official app and `sp404` lay it out.
fn backup_root(picked: &Path) -> Result<PathBuf, Failure> {
    if picked.join("PADCONF.BIN").is_file() {
        return Ok(picked.to_path_buf());
    }
    let nested = picked.join("ROLAND").join("SP-404MKII");
    let projects: Vec<PathBuf> = std::fs::read_dir(&nested)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.join("PADCONF.BIN").is_file())
        .collect();
    match projects.as_slice() {
        [one] => Ok(one.clone()),
        [] => Err(Failure::Other(format!(
            "{} holds no project backup (no PADCONF.BIN)",
            picked.display()
        ))),
        _ => Err(Failure::Other(format!(
            "{} holds {} projects; pick the one to restore",
            nested.display(),
            projects.len()
        ))),
    }
}

/// The files of a backup in the order the official app writes them: `PADCONF.BIN`, then
/// pictures, the pattern chain, patterns and samples.
fn backup_files(root: &Path) -> Result<Vec<(String, Vec<u8>)>, Failure> {
    let mut names = vec!["PADCONF.BIN".to_string()];
    for sub in ["PICTURE", "PTN", "SMPL"] {
        let mut found: Vec<String> = std::fs::read_dir(root.join(sub))
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.path().is_file())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            // The Mac's own files, when a backup went through Finder.
            .filter(|n| !n.starts_with("._") && n != ".DS_Store")
            .collect();
        found.sort_by_key(|n| (!n.ends_with(".CHN"), n.clone()));
        names.extend(found.into_iter().map(|n| format!("{sub}/{n}")));
    }
    names
        .into_iter()
        .map(|rel| {
            let path = root.join(&rel);
            std::fs::read(&path)
                .map(|data| (rel, data))
                .map_err(|e| Failure::Other(format!("reading {}: {e}", path.display())))
        })
        .collect()
}

/// Look at a backup before restoring it.
#[tauri::command]
pub async fn read_backup(folder: String) -> CmdResult<BackupInfo> {
    tauri::async_runtime::spawn_blocking(move || -> Result<BackupInfo, String> {
        let root = backup_root(Path::new(&folder)).map_err(failure_text)?;
        let files = backup_files(&root).map_err(failure_text)?;
        let name = sp404_formats::padconf::Padconf::parse(&files[0].1)
            .map_err(|e| e.to_string())?
            .settings()
            .name();
        Ok(BackupInfo {
            folder: root.to_string_lossy().into_owned(),
            name,
            files: files.len(),
            bytes: files.iter().map(|(_, d)| d.len() as u64).sum(),
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

fn failure_text(f: Failure) -> String {
    match f {
        Failure::Device(e) => e.to_string(),
        Failure::Other(e) => e,
    }
}

/// Replace the current project with a backup (Import to MKII). Everything in the current
/// project is erased first.
#[tauri::command]
pub async fn restore_backup(
    app: AppHandle,
    state: State<'_, AppState>,
    folder: String,
) -> CmdResult<usize> {
    with_device(&state, move |dev| {
        let root = backup_root(Path::new(&folder))?;
        let files = backup_files(&root)?;
        let total: u64 = files.iter().map(|(_, d)| d.len() as u64).sum();
        let free = u64::from(dev.free_kb()?) * 1024;
        if total > free {
            return Err(Failure::Other(format!(
                "the backup needs {} MB and the device has {} MB free",
                total / 1_000_000,
                free / 1_000_000
            )));
        }
        let project = dev.current_project()?;
        let count = files.len();
        dev.restore_project(project, &files, |i, f| emit(&app, f, i, count))?;
        Ok(count)
    })
    .await
}
