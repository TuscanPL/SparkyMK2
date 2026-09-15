<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import type { SampleInfo, Waveform } from "../api";

type Marker = "start" | "end" | "loopTop";

const props = defineProps<{
  waveform: Waveform | null;
  sample: SampleInfo | null;
  loading: boolean;
  editable: boolean;
}>();

const emit = defineEmits<{
  point: [name: "start" | "end" | "loop-top", frame: number];
  chops: [points: number[]];
}>();

const HIT_PX = 7;
const MAX_CHOPS = 16;
const canvas = ref<HTMLCanvasElement | null>(null);
let observer: ResizeObserver | undefined;

/** Marker positions while dragging, before they are sent. */
const preview = ref<{ start: number; end: number; loopTop: number; chops: number[] } | null>(null);
const dragging = ref<{ kind: "marker"; marker: Marker } | { kind: "chop"; slot: number } | null>(null);
const hover = ref<string>("");

const shown = computed(() => {
  const s = props.sample;
  if (!s) return null;
  return preview.value ?? { start: s.start, end: s.end, loopTop: s.loopTop, chops: [...s.chopPoints] };
});

const css = (name: string) => getComputedStyle(document.documentElement).getPropertyValue(name).trim();

function draw() {
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
  const lanes = wf.channels;
  const laneHeight = height / lanes;
  const toX = (frame: number) => (frame / wf.frames) * width;
  const m = shown.value;
  const start = m ? toX(m.start) : 0;
  const end = m ? toX(m.end) : width;

  const waveIn = (color: string, from: number, to: number) => {
    ctx.save();
    ctx.beginPath();
    ctx.rect(from, 0, to - from, height);
    ctx.clip();
    ctx.fillStyle = color;
    for (let ch = 0; ch < lanes; ch++) {
      const peaks = wf.peaks[ch];
      const mid = laneHeight * (ch + 0.5);
      const scale = ((laneHeight / 2) * 0.9) / 32768;
      for (let x = 0; x < width; x++) {
        const p0 = Math.floor((x / width) * peaks.length);
        const p1 = Math.max(p0 + 1, Math.floor(((x + 1) / width) * peaks.length));
        let lo = 0;
        let hi = 0;
        for (let p = p0; p < p1 && p < peaks.length; p++) {
          lo = Math.min(lo, peaks[p][0]);
          hi = Math.max(hi, peaks[p][1]);
        }
        ctx.fillRect(x, mid - hi * scale, 1, Math.max(dpr, (hi - lo) * scale));
      }
    }
    ctx.restore();
  };

  ctx.fillStyle = css("--line");
  for (let ch = 0; ch < lanes; ch++) {
    ctx.fillRect(0, Math.round(laneHeight * (ch + 0.5)), width, dpr);
    if (ch > 0) ctx.fillRect(0, Math.round(laneHeight * ch), width, dpr);
  }

  waveIn(css("--faint"), 0, width);
  waveIn(css("--accent"), start, end);

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
}

/** Frame under a pointer position. */
function frameAt(e: PointerEvent | MouseEvent): number {
  const el = canvas.value!;
  const rect = el.getBoundingClientRect();
  const frames = props.waveform?.frames ?? props.sample?.frames ?? 0;
  const ratio = Math.min(1, Math.max(0, (e.clientX - rect.left) / rect.width));
  return Math.round(ratio * frames);
}

/** What is within reach of the pointer: a marker (S/E/L) or a chop point. */
function hitAt(e: PointerEvent | MouseEvent) {
  const m = shown.value;
  const wf = props.waveform;
  if (!m || !wf) return null;
  const rect = canvas.value!.getBoundingClientRect();
  const px = (frame: number) => rect.left + (frame / wf.frames) * rect.width;
  const candidates: { hit: NonNullable<typeof dragging.value>; distance: number }[] = [
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
  if (d.kind === "chop") {
    p.chops[d.slot] = Math.min(Math.max(1, f), frames - 1);
  } else if (d.marker === "start") {
    p.start = Math.min(Math.max(0, f), p.end - 1);
  } else if (d.marker === "end") {
    p.end = Math.max(Math.min(frames, f), p.start + 1);
  } else {
    p.loopTop = Math.min(Math.max(p.start, f), p.end - 1);
  }
  draw();
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
  if (d.kind === "chop") {
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
  draw();
}

defineExpose({ reset });

onMounted(() => {
  observer = new ResizeObserver(draw);
  if (canvas.value) observer.observe(canvas.value);
  draw();
});
onBeforeUnmount(() => observer?.disconnect());
watch(
  () => props.sample,
  () => {
    if (!dragging.value) preview.value = null;
    draw();
  },
);
watch(() => props.waveform, draw);
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
