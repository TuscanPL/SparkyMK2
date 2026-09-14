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
};

/** Error text from a failed command. */
export function errorText(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : String(e);
}
