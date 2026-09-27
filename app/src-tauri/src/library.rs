//! The screen library: display images kept in a folder on the computer, to reuse across
//! projects and to share.
//!
//! Every entry is a PNG in the folder: 128×64 for a single frame, 128×384 for a set (the
//! two startup frames, then the four screen saver frames). The file name is the entry's
//! name, so the folder can be tidied, synced or added to outside the app.

use std::path::{Path, PathBuf};

use serde::Serialize;
use sp404_formats::{picture, picture_share};
use tauri::{AppHandle, Manager};

use crate::commands::CmdResult;

/// Frames in a set: startup 1–2, then screen saver 1–4.
const SET_FRAMES: usize = 6;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryItem {
    /// The file name without `.png`.
    name: String,
    /// `"frame"` or `"set"`.
    kind: &'static str,
    /// Packed rows, one entry per frame (1 for a frame, 6 for a set).
    frames: Vec<Vec<u8>>,
    path: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Library {
    dir: String,
    items: Vec<LibraryItem>,
    /// Files in the folder that are not library entries, with the reason.
    skipped: Vec<String>,
}

/// Run file work off the main thread.
async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> CmdResult<T> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| e.to_string())?
}

fn kind_of(frames: usize) -> Result<&'static str, String> {
    match frames {
        1 => Ok("frame"),
        SET_FRAMES => Ok("set"),
        n => Err(format!(
            "{n} frames; a library entry is 1 frame (128×64) or a set of 6 (128×384)"
        )),
    }
}

fn read_item(path: &Path) -> Result<LibraryItem, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let frames = picture_share::from_png(&bytes).map_err(|e| e.to_string())?;
    Ok(LibraryItem {
        name: stem(path),
        kind: kind_of(frames.len())?,
        frames,
        path: path.to_string_lossy().into_owned(),
    })
}

fn stem(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn is_png(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("png"))
}

/// A name that is safe as a file name on every system the app runs on.
fn clean_name(name: &str) -> Result<String, String> {
    let cleaned: String = name
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '-',
            c if c.is_control() => ' ',
            c => c,
        })
        .collect();
    let cleaned = cleaned.trim().trim_matches('.').trim().to_string();
    if cleaned.is_empty() {
        return Err("give it a name".into());
    }
    Ok(cleaned.chars().take(80).collect())
}

fn entry_path(dir: &Path, name: &str) -> PathBuf {
    dir.join(format!("{name}.png"))
}

/// `name`, or `name (2)`, `name (3)`… whichever is free.
fn free_name(dir: &Path, name: &str) -> String {
    let mut candidate = name.to_string();
    let mut n = 2;
    while entry_path(dir, &candidate).exists() {
        candidate = format!("{name} ({n})");
        n += 1;
    }
    candidate
}

fn write_item(dir: &Path, name: &str, frames: Vec<Vec<u8>>) -> Result<LibraryItem, String> {
    let kind = kind_of(frames.len())?;
    let png = picture_share::to_png(&frames).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(dir).map_err(|e| format!("making {}: {e}", dir.display()))?;
    let name = free_name(dir, &clean_name(name)?);
    let path = entry_path(dir, &name);
    std::fs::write(&path, png).map_err(|e| format!("writing {}: {e}", path.display()))?;
    Ok(LibraryItem {
        name,
        kind,
        frames,
        path: path.to_string_lossy().into_owned(),
    })
}

fn list(dir: &Path) -> Result<Library, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("making {}: {e}", dir.display()))?;
    let mut items = Vec::new();
    let mut skipped = Vec::new();
    let entries = std::fs::read_dir(dir).map_err(|e| format!("reading {}: {e}", dir.display()))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() || !is_png(&path) {
            continue;
        }
        match read_item(&path) {
            Ok(item) => items.push(item),
            Err(why) => skipped.push(format!("{}: {why}", stem(&path))),
        }
    }
    items.sort_by_key(|i| i.name.to_lowercase());
    Ok(Library {
        dir: dir.to_string_lossy().into_owned(),
        items,
        skipped,
    })
}

/// Where the library lives unless the user picks another folder.
#[tauri::command]
pub fn library_default_dir(app: AppHandle) -> CmdResult<String> {
    let docs = app
        .path()
        .document_dir()
        .map_err(|e| format!("no Documents folder: {e}"))?;
    Ok(docs
        .join("SparkyMK2")
        .join("Screens")
        .to_string_lossy()
        .into_owned())
}

/// Every entry in the library folder, made if it does not exist yet.
#[tauri::command]
pub async fn library_list(dir: String) -> CmdResult<Library> {
    blocking(move || list(Path::new(&dir))).await
}

/// Add a frame or a set under `name`, numbered if the name is taken.
#[tauri::command]
pub async fn library_save(
    dir: String,
    name: String,
    frames: Vec<Vec<u8>>,
) -> CmdResult<LibraryItem> {
    blocking(move || write_item(Path::new(&dir), &name, frames)).await
}

/// Add a frame unless the library already has one with the same pixels, or it is all dark
/// or all lit. Returns the new entry, or `None` when nothing was added.
#[tauri::command]
pub async fn library_collect(
    dir: String,
    name: String,
    rows: Vec<u8>,
) -> CmdResult<Option<LibraryItem>> {
    blocking(move || {
        if rows.iter().all(|&b| b == 0) || rows.iter().all(|&b| b == 0xFF) {
            return Ok(None);
        }
        let dir = Path::new(&dir);
        let library = list(dir)?;
        if library
            .items
            .iter()
            .any(|i| i.kind == "frame" && i.frames[0] == rows)
        {
            return Ok(None);
        }
        write_item(dir, &name, vec![rows]).map(Some)
    })
    .await
}

#[tauri::command]
pub async fn library_rename(dir: String, name: String, to: String) -> CmdResult<LibraryItem> {
    blocking(move || {
        let dir = Path::new(&dir);
        let to = clean_name(&to)?;
        let from = entry_path(dir, &name);
        let target = entry_path(dir, &to);
        // A change of case only is still a rename, on file systems that ignore case too.
        if target.exists() && !to.eq_ignore_ascii_case(&name) {
            return Err(format!("the library already has {to:?}"));
        }
        std::fs::rename(&from, &target).map_err(|e| format!("renaming {name:?}: {e}"))?;
        read_item(&target)
    })
    .await
}

#[tauri::command]
pub async fn library_delete(dir: String, name: String) -> CmdResult<()> {
    blocking(move || {
        let path = entry_path(Path::new(&dir), &name);
        std::fs::remove_file(&path).map_err(|e| format!("deleting {name:?}: {e}"))
    })
    .await
}

/// Add files from the computer: library PNGs (a frame or a set) and card BMPs. Each keeps
/// its file name. Returns what was added and, separately, why any file was not.
#[tauri::command]
pub async fn library_import(
    dir: String,
    paths: Vec<String>,
) -> CmdResult<(Vec<LibraryItem>, Vec<String>)> {
    blocking(move || {
        let dir = Path::new(&dir);
        let mut added = Vec::new();
        let mut refused = Vec::new();
        for path in paths.iter().map(Path::new) {
            let result = std::fs::read(path)
                .map_err(|e| e.to_string())
                .and_then(|bytes| {
                    if is_png(path) {
                        picture_share::from_png(&bytes).map_err(|e| e.to_string())
                    } else {
                        picture::decode(&bytes)
                            .map(|rows| vec![rows])
                            .map_err(|e| e.to_string())
                    }
                })
                .and_then(|frames| write_item(dir, &stem(path), frames));
            match result {
                Ok(item) => added.push(item),
                Err(why) => refused.push(format!("{}: {why}", stem(path))),
            }
        }
        Ok((added, refused))
    })
    .await
}

/// Copy an entry's file to `to`, for sharing.
#[tauri::command]
pub async fn library_export(dir: String, name: String, to: String) -> CmdResult<()> {
    blocking(move || {
        std::fs::copy(entry_path(Path::new(&dir), &name), &to)
            .map(|_| ())
            .map_err(|e| format!("saving {to}: {e}"))
    })
    .await
}

/// One frame as text to paste into a chat.
#[tauri::command]
pub fn share_code(rows: Vec<u8>) -> CmdResult<String> {
    picture_share::share_code(&rows).map_err(|e| e.to_string())
}

/// Read a frame from a share code, which may sit inside a longer message.
#[tauri::command]
pub fn read_share_code(text: String) -> CmdResult<Vec<u8>> {
    picture_share::from_share_code(&text).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("sparkymk2-library-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn frame(value: u8) -> Vec<u8> {
        vec![value; picture::ROWS_LEN]
    }

    #[test]
    fn saves_lists_and_numbers_taken_names() {
        let dir = temp_dir("save");
        write_item(&dir, "Doom", vec![frame(0x0F)]).unwrap();
        let second = write_item(&dir, "Doom", vec![frame(0xF0)]).unwrap();
        assert_eq!(second.name, "Doom (2)");
        write_item(&dir, "Look: a/set", (0..6).map(frame).collect()).unwrap();
        std::fs::write(dir.join("notes.png"), b"not an image").unwrap();

        let library = list(&dir).unwrap();
        let names: Vec<_> = library
            .items
            .iter()
            .map(|i| (i.name.as_str(), i.kind))
            .collect();
        assert_eq!(
            names,
            [
                ("Doom", "frame"),
                ("Doom (2)", "frame"),
                ("Look- a-set", "set")
            ]
        );
        assert_eq!(library.items[2].frames.len(), 6);
        assert_eq!(library.skipped.len(), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn refuses_other_frame_counts_and_empty_names() {
        let dir = temp_dir("refuse");
        assert!(write_item(&dir, "two", vec![frame(1), frame(2)]).is_err());
        assert!(write_item(&dir, " .. ", vec![frame(1)]).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
