<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import type { SampleInfo, Waveform } from "../api";

type Marker = "start" | "end" | "loopTop";

const props = defineProps<{
  waveform: Waveform | null;
  sample: SampleInfo | null;
  loading: boolean;
  editable: boolean;
  /** Frame a preview has reached, or null when nothing is playing. */
  playhead?: number | null;
  /** Envelope values, 0–127, for the fade handles; null hides them. */
  attack?: number | null;
  release?: number | null;
}>();

const emit = defineEmits<{
  point: [name: "start" | "end" | "loop-top", frame: number];
  chops: [points: number[]];
  envelope: [name: "attack" | "release", value: number];
}>();

const HIT_PX = 7;
const MAX_CHOPS = 16;
/** The fade handles stand this far below the top, clear of the S and E labels. */
const FADE_Y_PX = 44;
/** At most, a fade handle reaches the middle of Start–End. The device gives no time
 * for Attack and Release, so the handles show the 0–127 value, not a fade length. */
const FADE_SHARE = 0.5;
/** Closest zoom: no fewer peaks than this per pixel, or the wave turns into blocks. */
const MIN_PEAKS_PER_PX = 0.5;
const canvas = ref<HTMLCanvasElement | null>(null);
let observer: ResizeObserver | undefined;

type Shown = { start: number; end: number; loopTop: number; chops: number[]; attack: number; release: number };
type Hit =
  | { kind: "marker"; marker: Marker }
  | { kind: "chop"; slot: number }
  | { kind: "fade"; fade: "attack" | "release" };

/** Marker positions while dragging, before they are sent. */
const preview = ref<Shown | null>(null);
const dragging = ref<Hit | null>(null);
const hover = ref<string>("");

/** Frames on screen; null shows the whole sample. */
const view = ref<{ from: number; to: number } | null>(null);
const zoomed = computed(() => view.value !== null);

const shown = computed<Shown | null>(() => {
  const s = props.sample;
  if (!s) return null;
  return (
    preview.value ?? {
      start: s.start,
      end: s.end,
      loopTop: s.loopTop,
      chops: [...s.chopPoints],
      attack: props.attack ?? 0,
      release: props.release ?? 0,
    }
  );
});

const showFades = computed(() => props.attack != null && props.release != null);

function totalFrames() {
  return props.waveform?.frames ?? props.sample?.frames ?? 0;
}

function visible() {
  return view.value ?? { from: 0, to: totalFrames() };
}

/** Where the fade handles sit, in frames. */
function fadeFrames(m: Shown) {
  const reach = (m.end - m.start) * FADE_SHARE;
  return { attack: m.start + (m.attack / 127) * reach, release: m.end - (m.release / 127) * reach };
}

/**
 * Peak range under each pixel column, per channel. Kept until the waveform, the view or
 * the width changes, so a drag that only moves markers does not rescan the peaks.
 */
let columns: { wf: Waveform; width: number; from: number; to: number; lo: Float32Array[]; hi: Float32Array[] } | null = null;

function columnsFor(wf: Waveform, width: number, from: number, to: number) {
  const c = columns;
  if (c && c.wf === wf && c.width === width && c.from === from && c.to === to) return c;
  const span = Math.max(1, to - from);
  const lo: Float32Array[] = [];
  const hi: Float32Array[] = [];
  for (let ch = 0; ch < wf.channels; ch++) {
    const peaks = wf.peaks[ch];
    const perFrame = peaks.length / wf.frames;
    const l = new Float32Array(width);
    const h = new Float32Array(width);
    for (let x = 0; x < width; x++) {
      const p0 = Math.floor((from + (x / width) * span) * perFrame);
      const p1 = Math.max(p0 + 1, Math.floor((from + ((x + 1) / width) * span) * perFrame));
      let min = 0;
      let max = 0;
      for (let p = p0; p < p1 && p < peaks.length; p++) {
        if (peaks[p][0] < min) min = peaks[p][0];
        if (peaks[p][1] > max) max = peaks[p][1];
      }
      l[x] = min;
      h[x] = max;
    }
    lo.push(l);
    hi.push(h);
  }
  columns = { wf, width, from, to, lo, hi };
  return columns;
}

/**
 * The wave itself, drawn on a canvas of its own and only redrawn when what it shows
 * changes (view, size, Start or End); markers, handles and the playhead go on top.
 */
const layer = document.createElement("canvas");
let layerKey = "";
let layerWave: Waveform | null = null;

function drawLayer(wf: Waveform, width: number, height: number, from: number, to: number, start: number, end: number, colors: Colors) {
  const key = [width, height, from, to, Math.round(start), Math.round(end), colors.faint, colors.accent, colors.line].join("|");
  if (key === layerKey && wf === layerWave) return;
  layerKey = key;
  layerWave = wf;
  const cols = columnsFor(wf, width, from, to);
  layer.width = width;
  layer.height = height;
  const ctx = layer.getContext("2d")!;
  ctx.clearRect(0, 0, width, height);
  const dpr = window.devicePixelRatio || 1;
  const lanes = wf.channels;
  const laneHeight = height / lanes;
  ctx.fillStyle = colors.line;
  for (let ch = 0; ch < lanes; ch++) {
    ctx.fillRect(0, Math.round(laneHeight * (ch + 0.5)), width, dpr);
    if (ch > 0) ctx.fillRect(0, Math.round(laneHeight * ch), width, dpr);
  }
  // One path per colour: far fewer fills than one per column.
  const first = Math.max(0, Math.floor(start));
  const last = Math.min(width, Math.ceil(end));
  for (const [color, inside] of [
    [colors.faint, false],
    [colors.accent, true],
  ] as const) {
    ctx.beginPath();
    for (let ch = 0; ch < lanes; ch++) {
      const mid = laneHeight * (ch + 0.5);
      const scale = ((laneHeight / 2) * 0.9) / 32768;
      const l = cols.lo[ch];
      const h = cols.hi[ch];
      for (let x = 0; x < width; x++) {
        if ((x >= first && x < last) !== inside) continue;
        ctx.rect(x, mid - h[x] * scale, 1, Math.max(dpr, (h[x] - l[x]) * scale));
      }
    }
    ctx.fillStyle = color;
    ctx.fill();
  }
}

type Colors = { line: string; faint: string; accent: string; teal: string; text: string; mono: string };

function readColors(): Colors {
  const style = getComputedStyle(document.documentElement);
  const v = (name: string) => style.getPropertyValue(name).trim();
  return { line: v("--line"), faint: v("--faint"), accent: v("--accent"), teal: v("--teal"), text: v("--text"), mono: v("--mono") };
}

let frame = 0;

/** Redraw on the next screen frame; any number of calls before it make one redraw. */
function schedule() {
  if (!frame) frame = requestAnimationFrame(draw);
}

function draw() {
  cancelAnimationFrame(frame);
  frame = 0;
  const el = canvas.value;
  if (!el) return;
  const dpr = window.devicePixelRatio || 1;
  const width = Math.max(1, Math.round(el.clientWidth * dpr));
  const height = Math.max(1, Math.round(el.clientHeight * dpr));
  if (el.width !== width || el.height !== height) {
    el.width = width;
    el.height = height;
  }
  const ctx = el.getContext("2d")!;
  ctx.clearRect(0, 0, width, height);

  const wf = props.waveform;
  if (!wf || !wf.frames) return;
  const colors = readColors();
  const css = (name: string) => colors[name.slice(2) as keyof Colors];
  const v = visible();
  const span = Math.max(1, v.to - v.from);
  const toX = (frame: number) => ((frame - v.from) / span) * width;
  const m = shown.value;
  const start = m ? toX(m.start) : 0;
  const end = m ? toX(m.end) : width;

  drawLayer(wf, width, height, v.from, v.to, start, end, colors);
  ctx.drawImage(layer, 0, 0);

  if (!m) return;
  const font = `${11 * dpr}px ${css("--mono")}`;
  // Label rows: 0 = top, 1 = below it (so L doesn't cover S), 2 = bottom.
  const line = (x: number, color: string, label: string, dashed = false, row = 0) => {
    ctx.save();
    ctx.strokeStyle = color;
    ctx.lineWidth = dpr;
    if (dashed) ctx.setLineDash([4 * dpr, 3 * dpr]);
    ctx.beginPath();
    ctx.moveTo(Math.round(x) + 0.5, 0);
    ctx.lineTo(Math.round(x) + 0.5, height);
    ctx.stroke();
    ctx.restore();
    if (!label) return;
    ctx.font = font;
    const w = ctx.measureText(label).width + 8 * dpr;
    const lx = Math.min(Math.max(0, x - w / 2), width - w);
    const ly = row === 2 ? height - 16 * dpr : row * 18 * dpr;
    ctx.fillStyle = color;
    ctx.fillRect(lx, ly, w, 16 * dpr);
    ctx.fillStyle = "#111215";
    ctx.fillText(label, lx + 4 * dpr, ly + 12 * dpr);
  };

  m.chops.forEach((frame, i) => line(toX(frame), "rgba(231,232,236,0.55)", String(i + 1), false, 2));
  line(toX(m.loopTop), css("--teal"), "L", true, 1);
  line(start, css("--accent"), "S");
  line(end, css("--accent"), "E");

  if (showFades.value) {
    const f = fadeFrames(m);
    const ax = toX(f.attack);
    const rx = toX(f.release);
    const top = FADE_Y_PX * dpr;
    ctx.save();
    ctx.strokeStyle = css("--teal");
    ctx.lineWidth = 1.5 * dpr;
    ctx.beginPath();
    ctx.moveTo(start, height);
    ctx.lineTo(ax, top);
    ctx.lineTo(rx, top);
    ctx.lineTo(end, height);
    ctx.stroke();
    ctx.fillStyle = css("--teal");
    const r = 5 * dpr;
    for (const [x, label] of [
      [ax, `A ${m.attack}`],
      [rx, `R ${m.release}`],
    ] as const) {
      ctx.beginPath();
      ctx.moveTo(x, top - r);
      ctx.lineTo(x + r, top);
      ctx.lineTo(x, top + r);
      ctx.lineTo(x - r, top);
      ctx.closePath();
      ctx.fill();
      ctx.font = font;
      const w = ctx.measureText(label).width;
      const lx = Math.min(Math.max(0, x - w / 2), width - w);
      ctx.fillText(label, lx, top - r - 4 * dpr);
    }
    ctx.restore();
  }

  if (props.playhead != null) line(toX(props.playhead), css("--text"), "");

  // Where the zoomed view sits in the whole sample.
  if (view.value) {
    const total = wf.frames;
    ctx.fillStyle = "rgba(231,232,236,0.12)";
    ctx.fillRect(0, height - 3 * dpr, width, 3 * dpr);
    ctx.fillStyle = "rgba(231,232,236,0.6)";
    ctx.fillRect((v.from / total) * width, height - 3 * dpr, Math.max(2 * dpr, (span / total) * width), 3 * dpr);
  }
}

/** Frame under a pointer position. */
function frameAt(e: PointerEvent | MouseEvent): number {
  const el = canvas.value!;
  const rect = el.getBoundingClientRect();
  const v = visible();
  const ratio = Math.min(1, Math.max(0, (e.clientX - rect.left) / rect.width));
  return Math.round(v.from + ratio * (v.to - v.from));
}

/** What is within reach of the pointer: a fade handle, a marker (S/E/L) or a chop point. */
function hitAt(e: PointerEvent | MouseEvent): Hit | null {
  const m = shown.value;
  const wf = props.waveform;
  if (!m || !wf) return null;
  const rect = canvas.value!.getBoundingClientRect();
  const v = visible();
  const px = (frame: number) => rect.left + ((frame - v.from) / (v.to - v.from)) * rect.width;
  // The fade handles sit on top of the wave, so they win where the pointer is on one.
  if (showFades.value && Math.abs(e.clientY - rect.top - FADE_Y_PX) <= HIT_PX + 2) {
    const f = fadeFrames(m);
    const fades = (["attack", "release"] as const)
      .map((fade) => ({ fade, distance: Math.abs(e.clientX - px(f[fade])) }))
      .filter((c) => c.distance <= HIT_PX + 2)
      .sort((a, b) => a.distance - b.distance);
    if (fades.length) return { kind: "fade", fade: fades[0].fade };
  }
  const candidates: { hit: Hit; distance: number }[] = [
    { hit: { kind: "marker", marker: "start" }, distance: Math.abs(e.clientX - px(m.start)) },
    { hit: { kind: "marker", marker: "end" }, distance: Math.abs(e.clientX - px(m.end)) },
    { hit: { kind: "marker", marker: "loopTop" }, distance: Math.abs(e.clientX - px(m.loopTop)) },
    // The chop at frame 0 marks the sample start; it goes away only with Clear chops.
    ...m.chops.flatMap((c, slot) =>
      c === 0 ? [] : [{ hit: { kind: "chop" as const, slot }, distance: Math.abs(e.clientX - px(c)) }],
    ),
  ];
  const best = candidates.filter((c) => c.distance <= HIT_PX).sort((a, b) => a.distance - b.distance)[0];
  return best?.hit ?? null;
}

function onPointerDown(e: PointerEvent) {
  if (!props.editable || e.button !== 0 || !shown.value) return;
  const hit = hitAt(e);
  if (!hit) return;
  e.preventDefault();
  canvas.value!.setPointerCapture(e.pointerId);
  dragging.value = hit;
  preview.value = { ...shown.value, chops: [...shown.value.chops] };
}

function onPointerMove(e: PointerEvent) {
  if (!props.editable) return;
  const d = dragging.value;
  const p = preview.value;
  const frames = props.sample?.frames ?? 0;
  if (!d || !p) {
    hover.value = hitAt(e) ? "ew-resize" : "";
    return;
  }
  const f = frameAt(e);
  if (d.kind === "fade") {
    const reach = Math.max(1, (p.end - p.start) * FADE_SHARE);
    const share = d.fade === "attack" ? (f - p.start) / reach : (p.end - f) / reach;
    p[d.fade] = Math.round(Math.min(1, Math.max(0, share)) * 127);
  } else if (d.kind === "chop") {
    p.chops[d.slot] = Math.min(Math.max(1, f), frames - 1);
  } else if (d.marker === "start") {
    p.start = Math.min(Math.max(0, f), p.end - 1);
  } else if (d.marker === "end") {
    p.end = Math.max(Math.min(frames, f), p.start + 1);
  } else {
    p.loopTop = Math.min(Math.max(p.start, f), p.end - 1);
  }
  schedule();
}

function onPointerUp() {
  const d = dragging.value;
  const p = preview.value;
  const s = props.sample;
  dragging.value = null;
  if (!d || !p || !s) {
    preview.value = null;
    return;
  }
  if (d.kind === "fade") {
    const before = d.fade === "attack" ? props.attack : props.release;
    if (p[d.fade] !== before) emit("envelope", d.fade, p[d.fade]);
    else preview.value = null;
  } else if (d.kind === "chop") {
    if (p.chops[d.slot] !== s.chopPoints[d.slot]) emit("chops", p.chops);
  } else if (d.marker === "start" && p.start !== s.start) {
    emit("point", "start", p.start);
  } else if (d.marker === "end" && p.end !== s.end) {
    emit("point", "end", p.end);
  } else if (d.marker === "loopTop" && p.loopTop !== s.loopTop) {
    emit("point", "loop-top", p.loopTop);
  }
  // Keep the dragged position on screen until the device's answer replaces the sample.
}

function onDoubleClick(e: MouseEvent) {
  const s = props.sample;
  if (!props.editable || !s) return;
  // The device also keeps a chop at frame 0, which the first added chop brings along.
  const needed = s.chopPoints.includes(0) ? 1 : 2;
  if (s.chopPoints.length + needed > MAX_CHOPS) return;
  const frame = frameAt(e);
  if (frame > 0) emit("chops", [...s.chopPoints, frame]);
}

function onContextMenu(e: MouseEvent) {
  const s = props.sample;
  if (!props.editable || !s) return;
  const hit = hitAt(e);
  if (hit?.kind !== "chop") return;
  e.preventDefault();
  emit(
    "chops",
    s.chopPoints.filter((_, i) => i !== hit.slot),
  );
}

/** Drop the dragged positions, after the device answered or the edit failed. */
function reset() {
  preview.value = null;
  schedule();
}

/** Show `from`–`to`, kept inside the sample and no closer than the peaks allow. */
function setView(from: number, to: number) {
  const total = totalFrames();
  const peaks = props.waveform?.peaks[0]?.length ?? 0;
  const px = canvas.value?.clientWidth ?? 1;
  if (!total || !peaks) return;
  const minSpan = Math.min(total, Math.ceil((px * MIN_PEAKS_PER_PX * total) / peaks));
  let span = Math.min(total, Math.max(minSpan, to - from));
  if (span >= total) {
    view.value = null;
  } else {
    span = Math.round(span);
    const start = Math.round(Math.min(total - span, Math.max(0, from)));
    view.value = { from: start, to: start + span };
  }
  schedule();
}

/** Zoom by `factor` (2 = twice as close) around `frame`, the middle of the view by default. */
function zoom(factor: number, frame?: number) {
  const v = visible();
  const at = frame ?? (v.from + v.to) / 2;
  const ratio = (at - v.from) / Math.max(1, v.to - v.from);
  const span = (v.to - v.from) / factor;
  setView(at - ratio * span, at - ratio * span + span);
}

function fit() {
  view.value = null;
  schedule();
}

/** The wheel zooms around the pointer; Shift or a sideways scroll moves along. */
function onWheel(e: WheelEvent) {
  if (!props.waveform) return;
  e.preventDefault();
  const v = visible();
  const sideways = e.shiftKey || Math.abs(e.deltaX) > Math.abs(e.deltaY);
  if (sideways) {
    if (!view.value) return;
    const delta = e.shiftKey && !e.deltaX ? e.deltaY : e.deltaX;
    const perPx = (v.to - v.from) / (canvas.value?.clientWidth ?? 1);
    setView(v.from + delta * perPx, v.to + delta * perPx);
  } else {
    zoom(Math.exp(-e.deltaY * 0.002), frameAt(e));
  }
}

defineExpose({ reset, zoom, fit, zoomed });

onMounted(() => {
  observer = new ResizeObserver(schedule);
  if (canvas.value) observer.observe(canvas.value);
  draw();
});
onBeforeUnmount(() => {
  observer?.disconnect();
  cancelAnimationFrame(frame);
});
watch(
  () => [props.sample, props.attack, props.release],
  () => {
    if (!dragging.value) preview.value = null;
    schedule();
  },
);
watch(
  () => props.waveform,
  () => {
    // Another sample: its length differs, so the old view means nothing.
    view.value = null;
    schedule();
  },
);
watch(() => props.playhead, schedule);
</script>

<template>
  <div class="wave">
    <canvas
      ref="canvas"
      :style="{ cursor: dragging ? 'ew-resize' : hover }"
      @pointerdown="onPointerDown"
      @pointermove="onPointerMove"
      @pointerup="onPointerUp"
      @pointercancel="onPointerUp"
      @dblclick="onDoubleClick"
      @contextmenu="onContextMenu"
      @wheel="onWheel"
    />
    <div v-if="loading" class="overlay"><span class="spinner" /> Loading sample…</div>
  </div>
</template>

<style scoped>
.wave {
  position: relative;
  height: 190px;
  border-radius: 8px;
  border: 1px solid var(--line);
  background: #0d0e11;
  overflow: hidden;
}

canvas {
  width: 100%;
  height: 100%;
  display: block;
  touch-action: none;
}

.overlay {
  position: absolute;
  inset: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 10px;
  color: var(--muted);
  background: rgba(13, 14, 17, 0.6);
}
</style>
