//! SparkyMK2 desktop app: Tauri commands over the `sp404-device` core.

mod commands;
mod edit;
mod export;
mod files;
mod library;
mod logging;
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
    /// than staying on it after the app is gone. With `wait`, a running operation finishes
    /// first; without (quitting), MKII EXIT is skipped rather than holding the app open.
    fn close(&self, wait: bool) {
        let device = self.device.lock().unwrap().take();
        if let Some(device) = device {
            // The port may already be gone (unplugged); closing it is all that is left then.
            if wait {
                let _ = device.exclusive(|d| d.mkii_exit());
            } else {
                let _ = device.try_exclusive(|d| d.mkii_exit());
            }
        }
    }
}

/// On Wayland inside an AppImage, a bundled libwayland-client can cause protocol
/// mismatches and `EGL_BAD_PARAMETER` errors. Preloading the host system's native
/// 64-bit libwayland-client ensures compatibility with the running compositor.
#[cfg(target_os = "linux")]
fn appimage_wayland_preload_fix() {
    use std::io::Read;
    use std::os::unix::process::CommandExt;

    let in_appimage = std::env::var_os("APPIMAGE").is_some();
    let on_wayland = std::env::var_os("WAYLAND_DISPLAY").is_some();
    let already_reexeced = std::env::var_os("__SPARKY_WAYLAND_PRELOAD_DONE").is_some();

    if !in_appimage || !on_wayland || already_reexeced || std::env::var_os("LD_PRELOAD").is_some() {
        return;
    }

    let candidates = [
        "/usr/lib64/libwayland-client.so.0",
        "/usr/lib/x86_64-linux-gnu/libwayland-client.so.0",
        "/usr/lib/libwayland-client.so.0",
    ];

    for candidate in candidates {
        if let Ok(mut f) = std::fs::File::open(candidate) {
            let mut header = [0u8; 5];
            if f.read_exact(&mut header).is_ok() && &header[..4] == b"\x7fELF" && header[4] == 2 {
                if let Ok(exe) = std::env::current_exe() {
                    let mut cmd = std::process::Command::new(exe);
                    cmd.args(std::env::args_os().skip(1));
                    cmd.env("LD_PRELOAD", candidate);
                    cmd.env("__SPARKY_WAYLAND_PRELOAD_DONE", "1");
                    let _ = cmd.exec();
                }
            }
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(target_os = "linux")]
    {
        appimage_wayland_preload_fix();
        if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
            // SAFETY: Called early at startup before any worker threads are spawned.
            unsafe {
                std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
            }
        }
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .setup(|app| {
            logging::init(app.handle());
            log::info!("SparkyMK2 {} started", env!("CARGO_PKG_VERSION"));
            Ok(())
        })
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
            export::export_pattern,
            edit::detect_key,
            export::backup_project,
            export::read_backup,
            export::restore_backup,
            screens::apply_screens,
            library::library_default_dir,
            library::library_list,
            library::library_save,
            library::library_collect,
            library::library_rename,
            library::library_delete,
            library::library_import,
            library::library_export,
            library::share_code,
            library::read_share_code,
            logging::logging_state,
            logging::set_logging,
            logging::log_ui,
            logging::open_log_folder,
        ])
        .build(tauri::generate_context!())
        .expect("error while building SparkyMK2")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                log::info!("quitting");
                app.state::<AppState>().close(false);
            }
        });
}
