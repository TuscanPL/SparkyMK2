//! Browsing the device's two filesystems and moving files between them and the computer.
//!
//! The device exposes its own storage (projects, samples, the factory library) and the SD
//! card (`IMPORT`, `EXPORT`, `BKUP`, firmware) as separate volumes. Paths here are
//! relative to whichever volume is named; [`qualify`] turns them into device paths.

use std::path::PathBuf;

use serde::Serialize;
use sp404_proto::fileapi;
use tauri::{AppHandle, Emitter, State};

use crate::AppState;
use crate::commands::{CmdResult, Failure, with_device};

/// Progress of the transfer in flight, sent to the frontend as `transfer`.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Transfer {
    name: String,
    done: u64,
    total: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    name: String,
    /// Path within the volume.
    path: String,
    is_dir: bool,
    /// Bytes; 0 for directories.
    size: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Listing {
    volume: String,
    path: String,
    entries: Vec<Entry>,
    /// Only the device's own storage reports free space; the card refuses the call.
    free_kb: Option<u32>,
}

fn join(dir: &str, name: &str) -> String {
    if dir.is_empty() {
        name.to_string()
    } else {
        format!("{dir}/{name}")
    }
}

/// Turn a volume name and a path inside it into a path the device's file API takes.
fn qualify(volume: &str, path: &str) -> Result<String, Failure> {
    if path.split(['/', '\\']).any(|part| part == "..") {
        return Err(Failure::Other(format!(
            "{path} is not a path on the device"
        )));
    }
    match volume {
        "internal" => Ok(path.to_string()),
        "card" => Ok(format!("{}{path}", fileapi::CARD)),
        other => Err(Failure::Other(format!("no volume called {other:?}"))),
    }
}

fn file_name(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).to_string()
}

/// List a directory on one volume; `path` is `""` for its root.
#[tauri::command]
pub async fn list_volume(
    state: State<'_, AppState>,
    volume: String,
    path: String,
) -> CmdResult<Listing> {
    with_device(&state, move |dev| {
        let base = qualify(&volume, &path)?;
        let mut entries = Vec::new();
        for e in dev.list_dir(&base)? {
            let child = join(&path, &e.name);
            let is_dir = e.is_dir();
            // The listing carries no size, so ask for each file's.
            let size = if is_dir {
                0
            } else {
                dev.stat(&qualify(&volume, &child)?)?
                    .map_or(0, |s| u64::from(s.size))
            };
            entries.push(Entry {
                name: e.name,
                path: child,
                is_dir,
                size,
            });
        }
        entries.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| a.name.cmp(&b.name)));
        let free_kb = if volume == "internal" {
            Some(dev.free_kb()?)
        } else {
            None
        };
        Ok(Listing {
            volume,
            path,
            entries,
            free_kb,
        })
    })
    .await
}

fn emit(app: &AppHandle, name: &str, done: u64, total: u64) {
    let _ = app.emit(
        "transfer",
        Transfer {
            name: name.to_string(),
            done,
            total,
        },
    );
}

/// Copy a file off the device into `local`.
#[tauri::command]
pub async fn download_file(
    app: AppHandle,
    state: State<'_, AppState>,
    volume: String,
    remote: String,
    local: String,
) -> CmdResult<u64> {
    with_device(&state, move |dev| {
        let path = qualify(&volume, &remote)?;
        let name = file_name(&remote);
        let data = dev.read_file_with(&path, |done, total| emit(&app, &name, done, total))?;
        std::fs::write(&local, &data)
            .map_err(|e| Failure::Other(format!("writing {local}: {e}")))?;
        Ok(data.len() as u64)
    })
    .await
}

/// Copy a local file onto the device at `remote`, replacing it if it exists.
#[tauri::command]
pub async fn upload_file(
    app: AppHandle,
    state: State<'_, AppState>,
    local: String,
    volume: String,
    remote: String,
) -> CmdResult<u64> {
    with_device(&state, move |dev| {
        let path = qualify(&volume, &remote)?;
        let data =
            std::fs::read(&local).map_err(|e| Failure::Other(format!("reading {local}: {e}")))?;
        // Only the device's own storage answers the free-space call.
        if volume == "internal" {
            let free = u64::from(dev.free_kb()?) * 1024;
            if data.len() as u64 > free {
                return Err(Failure::Other(format!(
                    "{} needs more space than the device has free",
                    file_name(&local)
                )));
            }
        }
        let name = file_name(&remote);
        dev.write_file_with(&path, &data, |done, total| emit(&app, &name, done, total))?;
        Ok(data.len() as u64)
    })
    .await
}

/// Copy a whole folder off the device into `local`, keeping its layout.
#[tauri::command]
pub async fn download_folder(
    app: AppHandle,
    state: State<'_, AppState>,
    volume: String,
    remote: String,
    local: String,
) -> CmdResult<u64> {
    with_device(&state, move |dev| {
        let root = PathBuf::from(&local).join(file_name(&remote));
        let mut bytes = 0u64;
        let mut stack = vec![(remote.clone(), root)];
        while let Some((dir, target)) = stack.pop() {
            std::fs::create_dir_all(&target)
                .map_err(|e| Failure::Other(format!("creating {}: {e}", target.display())))?;
            for e in dev.list_dir(&qualify(&volume, &dir)?)? {
                let child = join(&dir, &e.name);
                if e.is_dir() {
                    stack.push((child, target.join(&e.name)));
                    continue;
                }
                let data = dev.read_file_with(&qualify(&volume, &child)?, |done, total| {
                    emit(&app, &e.name, done, total)
                })?;
                let out = target.join(&e.name);
                std::fs::write(&out, &data)
                    .map_err(|err| Failure::Other(format!("writing {}: {err}", out.display())))?;
                bytes += data.len() as u64;
            }
        }
        Ok(bytes)
    })
    .await
}

/// Delete a file, or an empty directory.
#[tauri::command]
pub async fn delete_path(
    state: State<'_, AppState>,
    volume: String,
    path: String,
    is_dir: bool,
) -> CmdResult<()> {
    with_device(&state, move |dev| {
        let target = qualify(&volume, &path)?;
        if is_dir {
            dev.rmdir(&target)?;
        } else {
            dev.unlink(&target)?;
        }
        Ok(())
    })
    .await
}

/// Rename an entry within its directory.
#[tauri::command]
pub async fn rename_path(
    state: State<'_, AppState>,
    volume: String,
    path: String,
    name: String,
) -> CmdResult<()> {
    with_device(&state, move |dev| {
        if name.is_empty() || name.contains(['/', '\\']) {
            return Err(Failure::Other(format!("{name:?} is not a file name")));
        }
        let parent = match path.rfind('/') {
            Some(i) => path[..i].to_string(),
            None => String::new(),
        };
        dev.rename(
            &qualify(&volume, &path)?,
            &qualify(&volume, &join(&parent, &name))?,
        )?;
        Ok(())
    })
    .await
}

/// Create a directory inside `parent`.
#[tauri::command]
pub async fn create_dir(
    state: State<'_, AppState>,
    volume: String,
    parent: String,
    name: String,
) -> CmdResult<()> {
    with_device(&state, move |dev| {
        if name.is_empty() || name.contains(['/', '\\']) {
            return Err(Failure::Other(format!("{name:?} is not a folder name")));
        }
        dev.mkdir(&qualify(&volume, &join(&parent, &name))?)?;
        Ok(())
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joins_paths_from_the_root() {
        assert_eq!(join("", "ROLAND"), "ROLAND");
        assert_eq!(join("ROLAND", "SP-404MKII"), "ROLAND/SP-404MKII");
    }

    #[test]
    fn qualifies_each_volume() {
        assert_eq!(
            qualify("internal", "ROLAND").ok().as_deref(),
            Some("ROLAND")
        );
        assert_eq!(qualify("card", "IMPORT").ok().as_deref(), Some("SD:IMPORT"));
        assert_eq!(qualify("card", "").ok().as_deref(), Some("SD:"));
    }

    #[test]
    fn refuses_to_climb_out_of_a_volume() {
        assert!(qualify("internal", "ROLAND/../../etc/passwd").is_err());
        assert!(qualify("card", "IMPORT/..").is_err());
        assert!(qualify("nowhere", "IMPORT").is_err());
    }

    #[test]
    fn takes_the_last_segment_as_the_name() {
        assert_eq!(file_name("ROLAND/SP-404MKII/QSPI.bin"), "QSPI.bin");
        assert_eq!(file_name("QSPI.bin"), "QSPI.bin");
    }
}
