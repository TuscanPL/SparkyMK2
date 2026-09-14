import type { Param } from "./api";

export const SAMPLE_RATE = 48_000;
export const BANK_LETTERS = "ABCDEFGHIJ".split("");

/** Pad numbers row by row, top to bottom. */
export const PAD_ROWS = [
  [1, 2, 3, 4],
  [5, 6, 7, 8],
  [9, 10, 11, 12],
  [13, 14, 15, 16],
];

export const padIndex = (bank: number, pad: number) => bank * 16 + pad - 1;
export const padLabel = (index: number) => `${BANK_LETTERS[Math.floor(index / 16)]}${(index % 16) + 1}`;

export const bpm = (hundredths: number) => (hundredths / 100).toFixed(2);

export function duration(frames: number): string {
  const seconds = frames / SAMPLE_RATE;
  if (seconds < 60) return `${seconds.toFixed(3)} s`;
  const m = Math.floor(seconds / 60);
  return `${m}:${(seconds - m * 60).toFixed(3).padStart(6, "0")}`;
}

export function storage(kb: number): string {
  const gb = kb / 1024 / 1024;
  return gb >= 1 ? `${gb.toFixed(1)} GB` : `${(kb / 1024).toFixed(0)} MB`;
}

const onOff = (v: number) => (v ? "On" : "Off");
const offOr = (v: number) => (v ? String(v) : "Off");
const signed = (v: number, unit = "") => `${v > 0 ? "+" : ""}${v}${unit}`;
const PLAY_MODES = ["Forward", "Reverse", "Ping-pong", "Reverse ping-pong"];

/** Label and display value for each pad parameter shown in the app. */
export const PARAM_VIEW: Record<string, { label: string; show: (v: number) => string }> = {
  level: { label: "Level", show: String },
  balance: { label: "Pan", show: (v) => (v === 64 ? "Centre" : v < 64 ? `L${64 - v}` : `R${v - 64}`) },
  gate: { label: "Gate", show: onOff },
  loop: { label: "Loop", show: onOff },
  "play-mode": { label: "Direction", show: (v) => PLAY_MODES[v] ?? String(v) },
  "mode-flags": {
    label: "Flags",
    show: (v) =>
      [v & 1 ? "Fixed velocity" : "", v & 0x18 ? "Chromatic" : "", v & 0x20 ? "One shot" : ""]
        .filter(Boolean)
        .join(", ") || "None",
  },
  bpm: { label: "BPM", show: bpm },
  "bpm-sync": { label: "BPM sync", show: onOff },
  "time-stretch": { label: "Time stretch", show: (v) => `${(v / 100).toFixed(0)} %` },
  "pitch-coarse": { label: "Pitch", show: (v) => signed(v, " st") },
  "pitch-fine": { label: "Fine pitch", show: (v) => signed(v, " ct") },
  vinyl: { label: "Vinyl", show: onOff },
  groove: { label: "Groove", show: offOr },
  rate: { label: "Rate", show: (v) => signed(v) },
  humanize: { label: "Humanize", show: offOr },
  attack: { label: "Attack", show: String },
  hold: { label: "Hold", show: String },
  release: { label: "Release", show: String },
  "mute-group": { label: "Mute group", show: offOr },
  "pad-link": { label: "Pad link", show: offOr },
  "bus-fx": { label: "Bus FX", show: String },
  roll: { label: "Roll", show: String },
};

export const PARAM_GROUPS: { title: string; names: string[] }[] = [
  { title: "Playback", names: ["level", "balance", "gate", "loop", "play-mode", "mode-flags"] },
  {
    title: "Tempo and pitch",
    names: ["bpm", "bpm-sync", "time-stretch", "pitch-coarse", "pitch-fine", "vinyl", "groove", "rate", "humanize"],
  },
  { title: "Envelope", names: ["attack", "hold", "release"] },
  { title: "Routing", names: ["mute-group", "pad-link", "bus-fx", "roll"] },
];

export function showParam(p: Param): string {
  return PARAM_VIEW[p.name]?.show(p.value) ?? String(p.value);
}
