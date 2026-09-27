//! SparkyMK2 desktop app: Tauri commands over the `sp404-device` core.

mod commands;
mod edit;
mod export;
mod files;
mod screens;

use std::sync::{Arc, Mutex};

use sp404_device::Device;
use tauri::Manager;

/// The open device, if any. Device calls are blocking and run on Tauri's blocking pool;
/// the device serializes concurrent requests itself.
#[derive(Default)]
pub struct AppState {
    device: Mutex<Option<Arc<Device>>>,
}

impl AppState {
    /// Let go of the device, sending MKII EXIT first so it leaves its remote screen rather
    /// than staying on it after the app is gone. Blocks until any request in flight is done.
    fn close(&self) {
        let device = self.device.lock().unwrap().take();
        if let Some(device) = device {
            // The port may already be gone (unplugged); closing it is all that is left then.
            let _ = device.mkii_exit();
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
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
            commands::device_activity,
            commands::select_on_device,
            commands::preview_start,
            commands::preview_stop,
            commands::patterns,
            commands::pattern_detail,
            edit::set_pad_param,
            edit::set_chop_points,
            edit::rename_sample,
            edit::pad_operation,
            edit::move_sample,
            edit::import_audio,
            edit::import_from_device,
            edit::analyze_bpm,
            edit::set_global_param,
            edit::rename_project,
            edit::clear_project,
            screens::screens,
            screens::set_screen,
            screens::restore_screen,
            files::list_volume,
            files::file_sizes,
            files::download_file,
            files::download_folder,
            files::upload_file,
            files::delete_path,
            files::rename_path,
            files::create_dir,
            files::preview_audio,
            export::export_pads,
            export::read_export,
            export::restore_pads,
        ])
        .build(tauri::generate_context!())
        .expect("error while building SparkyMK2")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                app.state::<AppState>().close();
            }
        });
}
