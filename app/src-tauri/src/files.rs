//! Browsing the SD card and moving files between it and the computer.
//!
//! Paths are card-relative, as the device's file API wants them: `ROLAND/SP-404MKII`,
//! and `""` for the card's root.

use std::path::PathBuf;

use serde::Serialize;
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
    /// Card-relative path of this entry.
    path: String,
    is_dir: bool,
    /// Bytes; 0 for directories.
    size: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Listing {
    path: String,
    entries: Vec<Entry>,
    free_kb: u32,
}

fn join(dir: &str, name: &str) -> String {
    if dir.is_empty() {
        name.to_string()
    } else {
        format!("{dir}/{name}")
    }
}

/// Reject anything that could climb out of the card's tree.
fn check(path: &str) -> Result<(), Failure> {
    if path.split(['/', '\\']).any(|part| part == "..") {
        return Err(Failure::Other(format!("{path} is not a path on the card")));
    }
    Ok(())
}

fn file_name(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).to_string()
}

/// List a directory on the card, newest listing wins; `path` is `""` for the root.
#[tauri::command]
pub async fn list_card(state: State<'_, AppState>, path: String) -> CmdResult<Listing> {
    with_device(&state, move |dev| {
        check(&path)?;
        let mut entries = Vec::new();
        for e in dev.list_dir(&path)? {
            let child = join(&path, &e.name);
            let is_dir = e.is_dir();
            // The listing carries no size, so ask for each file's.
            let size = if is_dir {
                0
            } else {
                dev.stat(&child)?.map_or(0, |s| u64::from(s.size))
            };
            entries.push(Entry {
                name: e.name,
                path: child,
                is_dir,
                size,
            });
        }
        entries.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| a.name.cmp(&b.name)));
        let free_kb = dev.free_kb()?;
        Ok(Listing {
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

/// Copy a file off the card into `local`.
#[tauri::command]
pub async fn download_file(
    app: AppHandle,
    state: State<'_, AppState>,
    remote: String,
    local: String,
) -> CmdResult<u64> {
    with_device(&state, move |dev| {
        check(&remote)?;
        let name = file_name(&remote);
        let data = dev.read_file_with(&remote, |done, total| emit(&app, &name, done, total))?;
        std::fs::write(&local, &data)
            .map_err(|e| Failure::Other(format!("writing {local}: {e}")))?;
        Ok(data.len() as u64)
    })
    .await
}

/// Copy a local file onto the card at `remote`, replacing it if it exists.
#[tauri::command]
pub async fn upload_file(
    app: AppHandle,
    state: State<'_, AppState>,
    local: String,
    remote: String,
) -> CmdResult<u64> {
    with_device(&state, move |dev| {
        check(&remote)?;
        let data =
            std::fs::read(&local).map_err(|e| Failure::Other(format!("reading {local}: {e}")))?;
        let free = u64::from(dev.free_kb()?) * 1024;
        if data.len() as u64 > free {
            return Err(Failure::Other(format!(
                "{} needs more space than the card has free",
                file_name(&local)
            )));
        }
        let name = file_name(&remote);
        dev.write_file_with(&remote, &data, |done, total| emit(&app, &name, done, total))?;
        Ok(data.len() as u64)
    })
    .await
}

/// Copy a whole folder off the card into `local`, keeping its layout.
#[tauri::command]
pub async fn download_folder(
    app: AppHandle,
    state: State<'_, AppState>,
    remote: String,
    local: String,
) -> CmdResult<u64> {
    with_device(&state, move |dev| {
        check(&remote)?;
        let root = PathBuf::from(&local).join(file_name(&remote));
        let mut bytes = 0u64;
        let mut stack = vec![(remote.clone(), root)];
        while let Some((dir, target)) = stack.pop() {
            std::fs::create_dir_all(&target)
                .map_err(|e| Failure::Other(format!("creating {}: {e}", target.display())))?;
            for e in dev.list_dir(&dir)? {
                let child = join(&dir, &e.name);
                if e.is_dir() {
                    stack.push((child, target.join(&e.name)));
                    continue;
                }
                let data =
                    dev.read_file_with(&child, |done, total| emit(&app, &e.name, done, total))?;
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
pub async fn delete_card_path(
    state: State<'_, AppState>,
    path: String,
    is_dir: bool,
) -> CmdResult<()> {
    with_device(&state, move |dev| {
        check(&path)?;
        if is_dir {
            dev.rmdir(&path)?;
        } else {
            dev.unlink(&path)?;
        }
        Ok(())
    })
    .await
}

/// Rename an entry within its directory.
#[tauri::command]
pub async fn rename_card_path(
    state: State<'_, AppState>,
    path: String,
    name: String,
) -> CmdResult<()> {
    with_device(&state, move |dev| {
        check(&path)?;
        if name.is_empty() || name.contains(['/', '\\']) {
            return Err(Failure::Other(format!("{name:?} is not a file name")));
        }
        let parent = match path.rfind('/') {
            Some(i) => path[..i].to_string(),
            None => String::new(),
        };
        dev.rename(&path, &join(&parent, &name))?;
        Ok(())
    })
    .await
}

/// Create a directory inside `parent`.
#[tauri::command]
pub async fn create_card_dir(
    state: State<'_, AppState>,
    parent: String,
    name: String,
) -> CmdResult<()> {
    with_device(&state, move |dev| {
        check(&parent)?;
        if name.is_empty() || name.contains(['/', '\\']) {
            return Err(Failure::Other(format!("{name:?} is not a folder name")));
        }
        dev.mkdir(&join(&parent, &name))?;
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
    fn refuses_to_climb_out_of_the_card() {
        assert!(check("ROLAND/../../etc/passwd").is_err());
        assert!(check("ROLAND/SP-404MKII").is_ok());
    }

    #[test]
    fn takes_the_last_segment_as_the_name() {
        assert_eq!(file_name("ROLAND/SP-404MKII/QSPI.bin"), "QSPI.bin");
        assert_eq!(file_name("QSPI.bin"), "QSPI.bin");
    }
}
