//! The display images a project shows at startup and as a screen saver.
//!
//! Images live in the project's `PICTURE` folder on the card, so a write takes effect the
//! next time that project loads. The editor works on the current project; images from the
//! library can go to any project, written straight into its folder.

use std::path::{Path, PathBuf};

use serde::Serialize;
use sp404_device::{Device, screens};
use sp404_formats::picture;
use tauri::{AppHandle, Manager, State};

use crate::AppState;
use crate::commands::{CmdResult, Failure, with_device};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Screen {
    slot: String,
    /// Packed pixels: 64 rows of 16 bytes, top row first, set bit = lit. None when the
    /// project has no file in this slot, or the file could not be read.
    rows: Option<Vec<u8>>,
    /// Why `rows` is missing, when the file exists but is not a display image.
    problem: Option<String>,
    /// Whether the image this slot held before the app first changed it was kept.
    has_original: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Screens {
    /// 1-based, the project the images were read from.
    project: u8,
    slots: Vec<Screen>,
}

/// Where the untouched image of each slot is kept, so an edit can be undone later.
fn originals_dir(app: &AppHandle, project: u8) -> Result<PathBuf, Failure> {
    let base = app
        .path()
        .app_data_dir()
        .map_err(|e| Failure::Other(format!("no place to keep the original images: {e}")))?;
    Ok(base
        .join("original-screens")
        .join(format!("PROJECT_{project:02}")))
}

fn original_path(dir: &Path, slot: &str) -> PathBuf {
    dir.join(format!("{slot}.bmp"))
}

fn read_slot(dev: &Device, project: u8, slot: &str, originals: &Path) -> Result<Screen, Failure> {
    let (rows, problem) = match dev.read_screen(project, slot)? {
        None => (None, None),
        Some(bytes) => match picture::decode(&bytes) {
            Ok(rows) => (Some(rows), None),
            Err(e) => (None, Some(e.to_string())),
        },
    };
    Ok(Screen {
        slot: slot.to_string(),
        rows,
        problem,
        has_original: original_path(originals, slot).is_file(),
    })
}

/// Read every display image of the current project.
#[tauri::command]
pub async fn screens(app: AppHandle, state: State<'_, AppState>) -> CmdResult<Screens> {
    with_device(&state, move |dev| {
        let project = dev.current_project()? + 1;
        let originals = originals_dir(&app, project)?;
        let slots = screens::slots()
            .map(|slot| read_slot(dev, project, slot, &originals))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Screens { project, slots })
    })
    .await
}

/// Write one display image to `project`, keeping the image that was there before this
/// app first changed the slot so it can be put back later.
fn store_slot(
    dev: &Device,
    app: &AppHandle,
    project: u8,
    slot: &str,
    rows: &[u8],
) -> Result<PathBuf, Failure> {
    let bmp = picture::encode(rows)?;
    let originals = originals_dir(app, project)?;
    let original = original_path(&originals, slot);
    if !original.is_file() {
        if let Some(previous) = dev.read_screen(project, slot)? {
            let _ = std::fs::create_dir_all(&originals)
                .and_then(|()| std::fs::write(&original, &previous));
        }
    }
    dev.write_screen(project, slot, &bmp)?;
    Ok(originals)
}

/// Replace one display image of the current project. `rows` is packed like [`Screen::rows`].
#[tauri::command]
pub async fn set_screen(
    app: AppHandle,
    state: State<'_, AppState>,
    slot: String,
    rows: Vec<u8>,
) -> CmdResult<Screen> {
    with_device(&state, move |dev| {
        let project = dev.current_project()? + 1;
        let originals = store_slot(dev, &app, project, &slot, &rows)?;
        read_slot(dev, project, &slot, &originals)
    })
    .await
}

/// Write display images to any project, current or not, without selecting it: `frames[i]`
/// goes to `slots[i]`. `project` is 1-based.
#[tauri::command]
pub async fn apply_screens(
    app: AppHandle,
    state: State<'_, AppState>,
    project: u8,
    slots: Vec<String>,
    frames: Vec<Vec<u8>>,
) -> CmdResult<()> {
    with_device(&state, move |dev| {
        if !(1..=16).contains(&project) {
            return Err(Failure::Other(format!("there is no project {project}")));
        }
        if slots.len() != frames.len() {
            return Err(Failure::Other("each image needs a slot".into()));
        }
        // Check every slot name before anything is written.
        for slot in &slots {
            screens::screen_path(project, slot)?;
        }
        for (slot, rows) in slots.iter().zip(&frames) {
            store_slot(dev, &app, project, slot, rows)?;
        }
        Ok(())
    })
    .await
}

/// Put back the image a slot held before the app first changed it.
#[tauri::command]
pub async fn restore_screen(
    app: AppHandle,
    state: State<'_, AppState>,
    slot: String,
) -> CmdResult<Screen> {
    with_device(&state, move |dev| {
        let project = dev.current_project()? + 1;
        let originals = originals_dir(&app, project)?;
        let original = original_path(&originals, &slot);
        let bytes = std::fs::read(&original)
            .map_err(|e| Failure::Other(format!("reading {}: {e}", original.display())))?;
        dev.write_screen(project, &slot, &bytes)?;
        read_slot(dev, project, &slot, &originals)
    })
    .await
}
