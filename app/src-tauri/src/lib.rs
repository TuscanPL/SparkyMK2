//! SparkyMK2 desktop app: Tauri commands over the `sp404-device` core.

mod commands;
mod edit;

use std::sync::{Arc, Mutex};

use sp404_device::Device;

/// The open device, if any. Device calls are blocking and run on Tauri's blocking pool;
/// the device serializes concurrent requests itself.
#[derive(Default)]
pub struct AppState {
    device: Mutex<Option<Arc<Device>>>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            commands::list_ports,
            commands::connect,
            commands::disconnect,
            commands::device_status,
            commands::project_names,
            commands::select_project,
            commands::pads,
            commands::pad_detail,
            commands::waveform,
            commands::preview_pad,
            commands::patterns,
            commands::pattern_detail,
            edit::set_pad_param,
            edit::set_chop_points,
            edit::rename_sample,
            edit::pad_operation,
            edit::move_sample,
            edit::import_audio,
            edit::analyze_bpm,
            edit::set_global_param,
            edit::rename_project,
        ])
        .run(tauri::generate_context!())
        .expect("error while running SparkyMK2");
}
