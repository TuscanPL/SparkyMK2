//! Diagnostic log: when turned on in Settings, every command the app runs, its result and
//! the device traffic behind it (not the raw bytes) go to a file someone can send in with a
//! bug report. The setting is kept on disk so a log also covers the next start.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use log::{Level, LevelFilter, Log, Metadata, Record};
use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::commands::CmdResult;

/// Log files kept in the folder; older ones are deleted when a new one starts.
const KEEP: usize = 10;
/// Name of the file whose presence turns logging on at start.
const FLAG: &str = "logging-enabled";

struct FileLogger {
    file: Mutex<Option<(File, PathBuf)>>,
}

static LOGGER: OnceLock<FileLogger> = OnceLock::new();
static DIR: OnceLock<PathBuf> = OnceLock::new();

impl Log for FileLogger {
    fn enabled(&self, meta: &Metadata) -> bool {
        // Our own crates in detail; Tauri and the webview only when something is wrong.
        let ours = ["sparkymk2", "sp404", "ui"]
            .iter()
            .any(|p| meta.target().starts_with(p));
        if ours {
            meta.level() <= Level::Debug
        } else {
            meta.level() <= Level::Warn
        }
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let mut file = self.file.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((f, _)) = file.as_mut() {
            let thread = std::thread::current();
            // Flushed line by line, so a crash or a freeze still leaves everything before it.
            let _ = writeln!(
                f,
                "{} {:<5} [{}] {}: {}",
                chrono::Local::now().format("%H:%M:%S%.3f"),
                record.level(),
                thread.name().unwrap_or("-"),
                record.target(),
                record.args()
            );
            let _ = f.flush();
        }
    }

    fn flush(&self) {}
}

fn logger() -> &'static FileLogger {
    LOGGER.get_or_init(|| FileLogger {
        file: Mutex::new(None),
    })
}

fn dir() -> &'static Path {
    DIR.get().expect("logging::init runs first")
}

/// Set up the logger and start a log file if logging was left on.
pub fn init(app: &AppHandle) {
    let folder = app
        .path()
        .app_log_dir()
        .unwrap_or_else(|_| std::env::temp_dir().join("SparkyMK2"));
    let _ = DIR.set(folder);
    if log::set_logger(logger()).is_err() {
        return;
    }
    log::set_max_level(LevelFilter::Off);

    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        log::error!(
            "panic: {info}\n{}",
            std::backtrace::Backtrace::force_capture()
        );
        previous(info);
    }));

    if dir().join(FLAG).exists() {
        if let Err(e) = start() {
            eprintln!("could not start the log: {e}");
        }
    }
}

/// Open a new log file and send everything to it.
fn start() -> std::io::Result<()> {
    fs::create_dir_all(dir())?;
    prune()?;
    let name = format!(
        "sparkymk2-{}.log",
        chrono::Local::now().format("%Y-%m-%d_%H-%M-%S")
    );
    let path = dir().join(name);
    let mut f = OpenOptions::new().create(true).append(true).open(&path)?;
    writeln!(
        f,
        "SparkyMK2 {} on {} {} ({}), started {}",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        os_version(),
        std::env::consts::ARCH,
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S %:z"),
    )?;
    *logger().file.lock().unwrap() = Some((f, path));
    log::set_max_level(LevelFilter::Debug);
    Ok(())
}

fn stop() {
    log::info!("logging turned off");
    log::set_max_level(LevelFilter::Off);
    *logger().file.lock().unwrap() = None;
}

/// Delete the oldest logs, leaving room for the one about to start.
fn prune() -> std::io::Result<()> {
    let mut logs: Vec<PathBuf> = fs::read_dir(dir())?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "log"))
        .collect();
    // The names start with the date and time, so they sort oldest first.
    logs.sort();
    let extra = (logs.len() + 1).saturating_sub(KEEP);
    for old in &logs[..extra] {
        let _ = fs::remove_file(old);
    }
    Ok(())
}

/// The OS release, as far as the standard library can tell without extra crates.
fn os_version() -> String {
    #[cfg(target_os = "windows")]
    let out = {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW: no console flashing up behind the app.
        std::process::Command::new("cmd")
            .args(["/C", "ver"])
            .creation_flags(0x0800_0000)
            .output()
    };
    #[cfg(target_os = "macos")]
    let out = std::process::Command::new("sw_vers")
        .arg("-productVersion")
        .output();
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let out = std::process::Command::new("uname").arg("-r").output();
    out.ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown version".into())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoggingState {
    enabled: bool,
    folder: String,
    /// The file being written now, if logging is on.
    file: Option<String>,
}

fn state() -> LoggingState {
    let file = logger()
        .file
        .lock()
        .unwrap()
        .as_ref()
        .map(|(_, p)| p.to_string_lossy().into_owned());
    LoggingState {
        enabled: file.is_some(),
        folder: dir().to_string_lossy().into_owned(),
        file,
    }
}

#[tauri::command]
pub fn logging_state() -> LoggingState {
    state()
}

/// Turn logging on or off, now and for later starts.
#[tauri::command]
pub fn set_logging(enabled: bool) -> CmdResult<LoggingState> {
    let flag = dir().join(FLAG);
    let on = logger().file.lock().unwrap().is_some();
    if enabled {
        fs::create_dir_all(dir()).map_err(|e| e.to_string())?;
        fs::write(&flag, b"").map_err(|e| e.to_string())?;
        if !on {
            start().map_err(|e| format!("could not start the log: {e}"))?;
        }
    } else {
        let _ = fs::remove_file(&flag);
        if on {
            stop();
        }
    }
    Ok(state())
}

/// A line from the frontend: a command it ran, its result, or an error in the page.
#[tauri::command]
pub fn log_ui(level: String, message: String) {
    let level = match level.as_str() {
        "error" => Level::Error,
        "warn" => Level::Warn,
        "info" => Level::Info,
        _ => Level::Debug,
    };
    log::log!(target: "ui", level, "{message}");
}

/// Show the log folder in the system's file manager.
#[tauri::command]
pub fn open_log_folder() -> CmdResult<()> {
    let dir = dir();
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    #[cfg(target_os = "windows")]
    let program = "explorer";
    #[cfg(target_os = "macos")]
    let program = "open";
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let program = "xdg-open";
    std::process::Command::new(program)
        .arg(dir)
        .spawn()
        .map(drop)
        .map_err(|e| format!("could not open {}: {e}", dir.display()))
}
