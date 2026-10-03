// Typed wrappers around the Tauri commands in src-tauri/src/commands.rs.
import { invoke as tauriInvoke } from "@tauri-apps/api/core";

/** Whether the diagnostic log is on; while it is, every command goes into it. */
let logging = false;

export function setLoggingActive(on: boolean) {
  logging = on;
}

/** Add a line to the diagnostic log, if it is on. */
export function logLine(level: "debug" | "info" | "warn" | "error", message: string) {
  if (logging) tauriInvoke("log_ui", { level, message }).catch(() => {});
}

/** A value shortened for the log: long lists, text and binary data become their size. */
function brief(value: unknown): string {
  if (value === undefined) return "";
  const text = JSON.stringify(value, (_key, v) => {
    if (v instanceof ArrayBuffer) return `<${v.byteLength} bytes>`;
    if (ArrayBuffer.isView(v)) return `<${v.byteLength} bytes>`;
    if (Array.isArray(v) && v.length > 16) return `[${v.length} items]`;
    if (typeof v === "string" && v.length > 200) return `${v.slice(0, 200)}… (${v.length} chars)`;
    return v;
  });
  return text === undefined ? "" : text.length > 600 ? `${text.slice(0, 600)}…` : text;
}

/** `invoke`, writing the command, its arguments, result and time to the log when it is on. */
async function invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (!logging) return tauriInvoke<T>(command, args);
  const started = performance.now();
  logLine("debug", `→ ${command} ${brief(args)}`);
  try {
    const result = await tauriInvoke<T>(command, args);
    logLine("debug", `← ${command} ok in ${Math.round(performance.now() - started)} ms ${brief(result)}`);
    return result;
  } catch (e) {
    logLine("error", `✗ ${command} failed in ${Math.round(performance.now() - started)} ms: ${errorText(e)}`);
    throw e;
  }
}

export interface PortInfo {
  name: string;
  product: string | null;
  serialNumber: string | null;
}

export interface Bank {
  letter: string;
  /** BPM × 100. */
  tempo: number;
  volume: number;
  protected: boolean;
}

export interface Status {
  port: string;
  /** 1-based. */
  project: number;
  projectName: string;
  usesProjectTempo: boolean;
  /** BPM × 100. */
  projectTempo: number;
  banks: Bank[];
  freeKb: number;
  selectedPad: number | null;
  /** 4 while the device shows a menu. */
  workingMode: number | null;
}

export interface Pad {
  index: number;
  label: string;
  hasSample: boolean;
  name: string;
  level: number;
  /** BPM × 100. */
  bpm: number;
  fileSize: number;
}

export interface Param {
  name: string;
  value: number;
  min: number;
  max: number;
  help: string;
}

export interface SampleInfo {
  channels: number;
  frames: number;
  start: number;
  end: number;
  loopTop: number;
  chopPoints: number[];
}

export interface PadDetail {
  index: number;
  label: string;
  name: string;
  sample: SampleInfo | null;
  params: Param[];
}

export interface Waveform {
  channels: number;
  frames: number;
  /** Per channel: [min, max] per point. */
  peaks: [number, number][][];
}

export interface Note {
  tick: number;
  pad: number;
  velocity: number;
  length: number;
}

export interface PatternDetail {
  index: number;
  label: string;
  lengthTicks: number;
  ppq: number;
  beatsPerBar: number | null;
  /** BPM × 100. */
  bankTempo: number;
  notes: Note[];
  controlEvents: number;
  padsUsed: number[];
}

/** A pad's grid entry and details, read back after a change. */
export interface PadState {
  pad: Pad;
  detail: PadDetail;
}

export interface ImportResult {
  state: PadState;
  sourceRate: number;
  sourceChannels: number;
  /** BPM × 100. */
  detectedBpm: number | null;
}

export interface BpmResult {
  state: PadState;
  /** BPM × 100 that was stored. */
  bpm: number | null;
}

/** The device's own storage, or the SD card in its slot. */
export type Volume = "internal" | "card";

/** An entry in a directory on one of the device's volumes. */
/** What Clear project removes; whatever is false stays. */
export interface ClearParts {
  samples: boolean;
  patterns: boolean;
  /** Project settings and name. Only possible together with samples and patterns. */
  settings: boolean;
  /** Startup and screen saver images. */
  screenImages: boolean;
  /** Only this bank's samples and patterns (0 = A); null for the whole project. */
  bank: number | null;
}

/** What the device is doing, polled several times a second. */
export interface Activity {
  /** 1-based. */
  project: number;
  /** The pad selected on the device. */
  selectedPad: number | null;
  /** Frames played of the pad sounding, or null when nothing is. */
  position: number | null;
}

export interface CardEntry {
  name: string;
  /** Path within the volume. */
  path: string;
  isDir: boolean;
  /** Bytes, or null until `fillSizes` reaches it; always null for directories. */
  size: number | null;
}

export interface CardListing {
  volume: Volume;
  path: string;
  entries: CardEntry[];
  /** Only the device's own storage reports free space. */
  freeKb: number | null;
}

/** Where an export goes or a restore comes from: the SD card or a folder on the computer. */
export type Place = "card" | "local";

/** One pad in an export's `sparkymk2.json`. */
export interface SavedPad {
  /** `C05`: the pad it goes back to. */
  pad: string;
  file: string;
  name: string;
  params: Record<string, number>;
  chopPoints: number[];
}

export interface ExportSheet {
  format: string;
  version: number;
  /** The project the pads came from. */
  project: string;
  pads: SavedPad[];
}

export interface ExportSummary {
  written: number;
  /** Pads asked for that hold no sample. */
  empty: number;
  folder: string;
}

/** Progress of the transfer in flight, from the `transfer` event. */
export interface Transfer {
  name: string;
  done: number;
  total: number;
}

/** One display image of a project, as it is in the device's own storage. */
export interface ScreenImage {
  slot: string;
  /** Packed rows, 16 bytes per row, top row first; null when the slot has no usable file. */
  rows: number[] | null;
  problem: string | null;
  hasOriginal: boolean;
}

export interface ScreenImages {
  /** 1-based, the project the images were read from. */
  project: number;
  slots: ScreenImage[];
}

/** An entry of the screen library: one frame, or a set of all six. */
export interface LibraryItem {
  name: string;
  kind: "frame" | "set";
  /** Packed rows per frame: 1 for a frame; startup 1–2 then screen saver 1–4 for a set. */
  frames: number[][];
  path: string;
}

export interface Library {
  dir: string;
  items: LibraryItem[];
  /** Files in the folder that are not library entries, with the reason. */
  skipped: string[];
}

/** A Standard MIDI File, one WAV of the whole pattern, or one WAV per pad it plays. */
export type PatternFormat = "smf" | "bounce" | "multipad";

/** A project backup found on the computer, before it is restored. */
export interface BackupInfo {
  /** The project folder itself, which may sit inside the folder picked. */
  folder: string;
  name: string;
  files: number;
  bytes: number;
}

/** A pad's estimated key, found on this computer; the device keeps none. */
export interface KeyResult {
  name: string;
  camelot: string;
  confidence: number;
}

export type PadOperation = "truncate" | "normalize" | "delete";

export interface LoggingState {
  enabled: boolean;
  folder: string;
  /** The file being written now, while logging is on. */
  file: string | null;
}

export const api = {
  loggingState: () => tauriInvoke<LoggingState>("logging_state"),
  setLogging: (enabled: boolean) => tauriInvoke<LoggingState>("set_logging", { enabled }),
  openLogFolder: () => tauriInvoke<void>("open_log_folder"),
  listPorts: () => invoke<PortInfo[]>("list_ports"),
  connect: (port: string | null) => invoke<string>("connect", { port }),
  disconnect: () => invoke<void>("disconnect"),
  status: () => invoke<Status>("device_status"),
  projectNames: () => invoke<string[]>("project_names"),
  selectProject: (project: number) => invoke<void>("select_project", { project }),
  pads: () => invoke<Pad[]>("pads"),
  padDetail: (pad: number) => invoke<PadDetail>("pad_detail", { pad }),
  waveform: (pad: number, points: number) => invoke<Waveform>("waveform", { pad, points }),
  deviceActivity: () => invoke<Activity>("device_activity"),
  selectOnDevice: (pad: number) => invoke<void>("select_on_device", { pad }),
  previewStart: (pad: number) => invoke<void>("preview_start", { pad }),
  previewStop: (pad: number) => invoke<void>("preview_stop", { pad }),
  patterns: () => invoke<boolean[]>("patterns"),
  patternDetail: (slot: number) => invoke<PatternDetail>("pattern_detail", { slot }),
  screens: () => invoke<ScreenImages>("screens"),
  listVolume: (volume: Volume, path: string) => invoke<CardListing>("list_volume", { volume, path }),
  fileSizes: (volume: Volume, paths: string[]) => invoke<(number | null)[]>("file_sizes", { volume, paths }),
  previewAudio: (volume: Volume, path: string) => invoke<ArrayBuffer>("preview_audio", { volume, path }),
  exportPads: (pads: number[], target: Place, folder: string, sidecar: boolean) =>
    invoke<ExportSummary>("export_pads", { pads, target, folder, sidecar }),
  readExport: (target: Place, folder: string) => invoke<ExportSheet>("read_export", { target, folder }),
  restorePads: (target: Place, folder: string) => invoke<number>("restore_pads", { target, folder }),
  backupProject: (project: number, folder: string) => invoke<number>("backup_project", { project, folder }),
  readBackup: (folder: string) => invoke<BackupInfo>("read_backup", { folder }),
  restoreBackup: (folder: string) => invoke<number>("restore_backup", { folder }),
  exportPattern: (slot: number, format: PatternFormat, target: Place, folder: string, name: string) =>
    invoke<number>("export_pattern", { slot, format, target, folder, name }),

  setPadParam: (pad: number, name: string, value: number) =>
    invoke<PadState>("set_pad_param", { pad, name, value }),
  setChopPoints: (pad: number, points: number[]) => invoke<PadState>("set_chop_points", { pad, points }),
  renameSample: (pad: number, name: string) => invoke<PadState>("rename_sample", { pad, name }),
  padOperation: (pad: number, operation: PadOperation) =>
    invoke<PadState>("pad_operation", { pad, operation }),
  moveSample: (from: number, to: number, exchange: boolean) =>
    invoke<void>("move_sample", { from, to, exchange }),
  importAudio: (pad: number, path: string, detectBpm: boolean, bpmRange: number) =>
    invoke<ImportResult>("import_audio", { pad, path, detectBpm, bpmRange }),
  importFromDevice: (pad: number, volume: Volume, remote: string, detectBpm: boolean, bpmRange: number) =>
    invoke<ImportResult>("import_from_device", { pad, volume, remote, detectBpm, bpmRange }),
  detectKey: (pad: number) => invoke<KeyResult | null>("detect_key", { pad }),
  analyzeBpm: (pad: number, mode: "detect" | "length", bpmRange: number) =>
    invoke<BpmResult>("analyze_bpm", { pad, mode, bpmRange }),
  setGlobalParam: (name: string, value: number) => invoke<Status>("set_global_param", { name, value }),
  renameProject: (project: number, name: string) => invoke<string[]>("rename_project", { project, name }),
  clearProject: (project: number, parts: ClearParts) => invoke<Status>("clear_project", { project, ...parts }),
  setScreen: (slot: string, rows: number[]) => invoke<ScreenImage>("set_screen", { slot, rows }),
  restoreScreen: (slot: string) => invoke<ScreenImage>("restore_screen", { slot }),
  applyScreens: (project: number, slots: string[], frames: number[][]) =>
    invoke<void>("apply_screens", { project, slots, frames }),
  libraryDefaultDir: () => invoke<string>("library_default_dir"),
  libraryList: (dir: string) => invoke<Library>("library_list", { dir }),
  librarySave: (dir: string, name: string, frames: number[][]) =>
    invoke<LibraryItem>("library_save", { dir, name, frames }),
  libraryCollect: (dir: string, name: string, rows: number[]) =>
    invoke<LibraryItem | null>("library_collect", { dir, name, rows }),
  libraryRename: (dir: string, name: string, to: string) => invoke<LibraryItem>("library_rename", { dir, name, to }),
  libraryDelete: (dir: string, name: string) => invoke<void>("library_delete", { dir, name }),
  libraryImport: (dir: string, paths: string[]) =>
    invoke<[LibraryItem[], string[]]>("library_import", { dir, paths }),
  libraryExport: (dir: string, name: string, to: string) => invoke<void>("library_export", { dir, name, to }),
  shareCode: (rows: number[]) => invoke<string>("share_code", { rows }),
  readShareCode: (text: string) => invoke<number[]>("read_share_code", { text }),
  downloadFile: (volume: Volume, remote: string, local: string) =>
    invoke<number>("download_file", { volume, remote, local }),
  downloadFolder: (volume: Volume, remote: string, local: string) =>
    invoke<number>("download_folder", { volume, remote, local }),
  uploadFile: (local: string, volume: Volume, remote: string) =>
    invoke<number>("upload_file", { local, volume, remote }),
  deletePath: (volume: Volume, path: string, isDir: boolean) =>
    invoke<void>("delete_path", { volume, path, isDir }),
  renamePath: (volume: Volume, path: string, name: string) =>
    invoke<void>("rename_path", { volume, path, name }),
  createDir: (volume: Volume, parent: string, name: string) =>
    invoke<void>("create_dir", { volume, parent, name }),
};

/** Error text from a failed command. */
export function errorText(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : String(e);
}
