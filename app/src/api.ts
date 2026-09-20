// Typed wrappers around the Tauri commands in src-tauri/src/commands.rs.
import { invoke } from "@tauri-apps/api/core";

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

/** An entry in a directory on the SD card. */
export interface CardEntry {
  name: string;
  /** Card-relative path. */
  path: string;
  isDir: boolean;
  /** Bytes; 0 for directories. */
  size: number;
}

export interface CardListing {
  path: string;
  entries: CardEntry[];
  freeKb: number;
}

/** Progress of the transfer in flight, from the `transfer` event. */
export interface Transfer {
  name: string;
  done: number;
  total: number;
}

/** One display image of a project, as it is on the card. */
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

export type PadOperation = "truncate" | "normalize" | "delete";

export const api = {
  listPorts: () => invoke<PortInfo[]>("list_ports"),
  connect: (port: string | null) => invoke<string>("connect", { port }),
  disconnect: () => invoke<void>("disconnect"),
  status: () => invoke<Status>("device_status"),
  projectNames: () => invoke<string[]>("project_names"),
  selectProject: (project: number) => invoke<void>("select_project", { project }),
  pads: () => invoke<Pad[]>("pads"),
  padDetail: (pad: number) => invoke<PadDetail>("pad_detail", { pad }),
  waveform: (pad: number, points: number) => invoke<Waveform>("waveform", { pad, points }),
  preview: (pad: number, ms: number) => invoke<void>("preview_pad", { pad, ms }),
  patterns: () => invoke<boolean[]>("patterns"),
  patternDetail: (slot: number) => invoke<PatternDetail>("pattern_detail", { slot }),
  screens: () => invoke<ScreenImages>("screens"),
  listCard: (path: string) => invoke<CardListing>("list_card", { path }),

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
  analyzeBpm: (pad: number, mode: "detect" | "length", bpmRange: number) =>
    invoke<BpmResult>("analyze_bpm", { pad, mode, bpmRange }),
  setGlobalParam: (name: string, value: number) => invoke<Status>("set_global_param", { name, value }),
  renameProject: (project: number, name: string) => invoke<string[]>("rename_project", { project, name }),
  setScreen: (slot: string, rows: number[]) => invoke<ScreenImage>("set_screen", { slot, rows }),
  restoreScreen: (slot: string) => invoke<ScreenImage>("restore_screen", { slot }),
  downloadFile: (remote: string, local: string) => invoke<number>("download_file", { remote, local }),
  downloadFolder: (remote: string, local: string) => invoke<number>("download_folder", { remote, local }),
  uploadFile: (local: string, remote: string) => invoke<number>("upload_file", { local, remote }),
  deleteCardPath: (path: string, isDir: boolean) => invoke<void>("delete_card_path", { path, isDir }),
  renameCardPath: (path: string, name: string) => invoke<void>("rename_card_path", { path, name }),
  createCardDir: (parent: string, name: string) => invoke<void>("create_card_dir", { parent, name }),
};

/** Error text from a failed command. */
export function errorText(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : String(e);
}
