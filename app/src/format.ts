import type { Param } from "./api";

export const SAMPLE_RATE = 48_000;
export const BANK_LETTERS = "ABCDEFGHIJ".split("");
export const SAMPLE_NAME_LEN = 23;
export const PROJECT_NAME_LEN = 31;

/** Pad numbers row by row, top to bottom. */
export const PAD_ROWS = [
  [1, 2, 3, 4],
  [5, 6, 7, 8],
  [9, 10, 11, 12],
  [13, 14, 15, 16],
];

export const padIndex = (bank: number, pad: number) => bank * 16 + pad - 1;
export const padLabel = (index: number) => `${BANK_LETTERS[Math.floor(index / 16)]}${(index % 16) + 1}`;
export const bankOf = (index: number) => Math.floor(index / 16);

export const bpm = (hundredths: number) => (hundredths / 100).toFixed(2);

export function duration(frames: number): string {
  const seconds = frames / SAMPLE_RATE;
  if (seconds < 60) return `${seconds.toFixed(3)} s`;
  const m = Math.floor(seconds / 60);
  return `${m}:${(seconds - m * 60).toFixed(3).padStart(6, "0")}`;
}

/** A file size, from bytes up. */
export function bytes(n: number): string {
  if (n < 1024) return `${n} B`;
  const units = ["kB", "MB", "GB"];
  let value = n / 1024;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit++;
  }
  return `${value < 10 ? value.toFixed(1) : Math.round(value)} ${units[unit]}`;
}

export function storage(kb: number): string {
  const gb = kb / 1024 / 1024;
  return gb >= 1 ? `${gb.toFixed(1)} GB` : `${(kb / 1024).toFixed(0)} MB`;
}

/** The device's BPM detect range presets. */
export const BPM_RANGES: [number, string][] = [
  [0, "99–199"],
  [1, "79–159"],
  [2, "69–139"],
  [3, "49–99"],
  [4, "75–150"],
];

/** Mode flag bits in parameter `mode-flags`. */
export const FLAG = { fixedVelocity: 1, chromaticA: 8, chromaticB: 16, oneShot: 32 };

export type Control =
  | { kind: "toggle" }
  | { kind: "select"; options: [number, string][] }
  | { kind: "slider"; min: number; max: number; step?: number }
  | { kind: "bpm" }
  | { kind: "flags" };

interface ParamView {
  label: string;
  show: (v: number) => string;
  control: Control;
}

const onOff = (v: number) => (v ? "On" : "Off");
const signed = (v: number, unit = "") => `${v > 0 ? "+" : ""}${v}${unit}`;
const toggle: Control = { kind: "toggle" };
const slider = (min: number, max: number, step?: number): Control => ({ kind: "slider", min, max, step });
const select = (options: [number, string][]): Control => ({ kind: "select", options });
const offOrNumber = (max: number): [number, string][] =>
  Array.from({ length: max + 1 }, (_, i) => [i, i ? String(i) : "Off"]);
const labelOf = (options: [number, string][]) => (v: number) => options.find(([o]) => o === v)?.[1] ?? String(v);

const PLAY_MODES: [number, string][] = [
  [0, "Forward"],
  [1, "Reverse"],
  [2, "Ping-pong"],
  [3, "Reverse ping-pong"],
];
const GROOVES: [number, string][] = [
  [0, "Off"],
  [1, "8 beat <"],
  [2, "8 beat <<"],
  [3, "8 beat >"],
  [4, "8 beat >>"],
  [5, "16 beat <"],
  [6, "16 beat <<"],
  [7, "16 beat >"],
  [8, "16 beat >>"],
];
const HUMANIZE: [number, string][] = [
  [0, "Off"],
  [1, "Low"],
  [2, "Medium"],
  [3, "High"],
];
// Only BUS 1 = 1 is confirmed; 0 and 2 follow the likely order.
const BUS_FX: [number, string][] = [
  [0, "Off"],
  [1, "Bus 1"],
  [2, "Bus 2"],
];
// Only 2 = 1/4 is confirmed.
const ROLL: [number, string][] = Array.from({ length: 11 }, (_, i) => [i, i === 2 ? "2 (1/4)" : String(i)]);

export function flagsText(v: number): string {
  return (
    [v & FLAG.fixedVelocity ? "Fixed velocity" : "", v & (FLAG.chromaticA | FLAG.chromaticB) ? "Chromatic" : "", v & FLAG.oneShot ? "One shot" : ""]
      .filter(Boolean)
      .join(", ") || "None"
  );
}

/** Label, display value and editor for each pad parameter shown in the app. */
export const PARAM_VIEW: Record<string, ParamView> = {
  level: { label: "Level", show: String, control: slider(0, 127) },
  balance: {
    label: "Pan",
    show: (v) => (v === 64 ? "Centre" : v < 64 ? `L${64 - v}` : `R${v - 64}`),
    control: slider(13, 115),
  },
  gate: { label: "Gate", show: onOff, control: toggle },
  loop: { label: "Loop", show: onOff, control: toggle },
  "play-mode": { label: "Direction", show: labelOf(PLAY_MODES), control: select(PLAY_MODES) },
  "mode-flags": { label: "Trigger", show: flagsText, control: { kind: "flags" } },
  bpm: { label: "BPM", show: bpm, control: { kind: "bpm" } },
  "bpm-sync": { label: "BPM sync", show: onOff, control: toggle },
  "time-stretch": { label: "Time stretch", show: (v) => `${(v / 100).toFixed(0)} %`, control: slider(5000, 15000, 100) },
  "pitch-coarse": { label: "Pitch", show: (v) => signed(v, " st"), control: slider(-12, 12) },
  "pitch-fine": { label: "Fine pitch", show: (v) => signed(v, " ct"), control: slider(-100, 100) },
  vinyl: { label: "Vinyl", show: onOff, control: toggle },
  groove: { label: "Groove", show: labelOf(GROOVES), control: select(GROOVES) },
  rate: { label: "Rate", show: (v) => String(v + 8), control: slider(-7, 7) },
  humanize: { label: "Humanize", show: labelOf(HUMANIZE), control: select(HUMANIZE) },
  attack: { label: "Attack", show: String, control: slider(0, 127) },
  hold: { label: "Hold", show: String, control: slider(1, 100) },
  release: { label: "Release", show: String, control: slider(0, 127) },
  "mute-group": { label: "Mute group", show: labelOf(offOrNumber(10)), control: select(offOrNumber(10)) },
  "pad-link": { label: "Pad link", show: labelOf(offOrNumber(10)), control: select(offOrNumber(10)) },
  "bus-fx": { label: "Bus FX", show: labelOf(BUS_FX), control: select(BUS_FX) },
  roll: { label: "Roll", show: labelOf(ROLL), control: select(ROLL) },
};

export const PARAM_GROUPS: { title: string; names: string[] }[] = [
  { title: "Playback", names: ["level", "balance", "gate", "loop", "play-mode", "mode-flags"] },
  {
    title: "Tempo and pitch",
    names: ["bpm", "bpm-sync", "time-stretch", "vinyl", "pitch-coarse", "pitch-fine", "groove", "rate", "humanize"],
  },
  { title: "Envelope", names: ["attack", "hold", "release"] },
  { title: "Routing", names: ["mute-group", "pad-link", "bus-fx", "roll"] },
];

export function showParam(p: Param): string {
  return PARAM_VIEW[p.name]?.show(p.value) ?? String(p.value);
}
