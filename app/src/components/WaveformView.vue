<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref, watch } from "vue";
import type { SampleInfo, Waveform } from "../api";

const props = defineProps<{
  waveform: Waveform | null;
  sample: SampleInfo | null;
  loading: boolean;
}>();

const canvas = ref<HTMLCanvasElement | null>(null);
let observer: ResizeObserver | undefined;

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
  const sample = props.sample;
  const start = sample ? toX(sample.start) : 0;
  const end = sample ? toX(sample.end) : width;

  const waveAt = (color: string, from: number, to: number) => {
    ctx.save();
    ctx.beginPath();
    ctx.rect(from, 0, to - from, height);
    ctx.clip();
    ctx.fillStyle = color;
    for (let ch = 0; ch < lanes; ch++) {
      const peaks = wf.peaks[ch];
      const mid = laneHeight * (ch + 0.5);
      const scale = (laneHeight / 2) * 0.9 / 32768;
      for (let x = 0; x < width; x++) {
        const p0 = Math.floor((x / width) * peaks.length);
        const p1 = Math.max(p0 + 1, Math.floor(((x + 1) / width) * peaks.length));
        let lo = 0;
        let hi = 0;
        for (let p = p0; p < p1 && p < peaks.length; p++) {
          lo = Math.min(lo, peaks[p][0]);
          hi = Math.max(hi, peaks[p][1]);
        }
        const top = mid - hi * scale;
        ctx.fillRect(x, top, 1, Math.max(dpr, (hi - lo) * scale));
      }
    }
    ctx.restore();
  };

  // Channel centre lines and separators.
  ctx.fillStyle = css("--line");
  for (let ch = 0; ch < lanes; ch++) {
    ctx.fillRect(0, Math.round(laneHeight * (ch + 0.5)), width, dpr);
    if (ch > 0) ctx.fillRect(0, Math.round(laneHeight * ch), width, dpr);
  }

  waveAt(css("--faint"), 0, width);
  waveAt(css("--accent"), start, end);

  if (!sample) return;
  const line = (x: number, color: string, label: string, dashed = false) => {
    ctx.save();
    ctx.strokeStyle = color;
    ctx.lineWidth = dpr;
    if (dashed) ctx.setLineDash([4 * dpr, 3 * dpr]);
    ctx.beginPath();
    ctx.moveTo(Math.round(x) + 0.5, 0);
    ctx.lineTo(Math.round(x) + 0.5, height);
    ctx.stroke();
    ctx.restore();
    if (label) {
      ctx.font = `${11 * dpr}px ${css("--mono")}`;
      const w = ctx.measureText(label).width + 8 * dpr;
      const lx = Math.min(Math.max(0, x - w / 2), width - w);
      ctx.fillStyle = color;
      ctx.fillRect(lx, 0, w, 16 * dpr);
      ctx.fillStyle = "#111215";
      ctx.fillText(label, lx + 4 * dpr, 12 * dpr);
    }
  };

  for (const frame of sample.chopPoints) line(toX(frame), "rgba(231,232,236,0.35)", "");
  if (sample.loopTop > sample.start) line(toX(sample.loopTop), css("--teal"), "L", true);
  line(start, css("--accent"), "S");
  line(end, css("--accent"), "E");
}

onMounted(() => {
  observer = new ResizeObserver(draw);
  if (canvas.value) observer.observe(canvas.value);
  draw();
});
onBeforeUnmount(() => observer?.disconnect());
watch(() => [props.waveform, props.sample], draw);
</script>

<template>
  <div class="wave">
    <canvas ref="canvas" />
    <div v-if="loading" class="overlay"><span class="spinner" /> Loading sample…</div>
  </div>
</template>

<style scoped>
.wave {
  position: relative;
  height: 180px;
  border-radius: 8px;
  border: 1px solid var(--line);
  background: #0d0e11;
  overflow: hidden;
}

canvas {
  width: 100%;
  height: 100%;
  display: block;
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
