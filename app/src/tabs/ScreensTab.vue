<script setup lang="ts">
import { computed, onUnmounted, reactive, ref, shallowReactive, watch } from "vue";
import Icon from "../components/Icon.vue";
import ScreenCanvas from "../components/ScreenCanvas.vue";
import { errorText } from "../api";
import {
  DEFAULT_CONVERSION,
  HEIGHT,
  SCREEN_SAVER_SLOTS,
  STARTUP_SLOTS,
  WIDTH,
  blank,
  convert,
  invert,
  loadImage,
  pack,
  samePixels,
  unpack,
  type Pixels,
} from "../screens";
import { applyScreen, notify, restoreScreen, store } from "../store";

const EDIT_SCALE = 4;
const THUMB_SCALE = 2;
const UNDO_LIMIT = 40;

type Source = { source: CanvasImageSource; width: number; height: number };

const groups = [
  { id: "startup", label: "Startup", slots: STARTUP_SLOTS, hint: "Plays while the project loads." },
  { id: "saver", label: "Screen saver", slots: SCREEN_SAVER_SLOTS, hint: "Plays when the device sits idle." },
];

/** Working copies, loaded images and undo history, by slot. Absent = as on the device. */
const edits = shallowReactive<Record<string, Pixels>>({});
const sources = shallowReactive<Record<string, Source | undefined>>({});
const undo = shallowReactive<Record<string, Pixels[]>>({});

const selected = ref(STARTUP_SLOTS[0]);
const tool = ref<"draw" | "erase">("draw");
const brush = ref(1);
const conversion = reactive({ ...DEFAULT_CONVERSION });
const fillGroup = ref(false);
const playing = ref(false);
const speed = ref(400);
const frame = ref(0);
const fileInput = ref<HTMLInputElement>();

/** What the device holds right now, by slot; null when the slot has no usable file. */
const onDevice = computed(() => {
  const map: Record<string, Pixels | null> = {};
  for (const s of store.screens) map[s.slot] = s.rows ? unpack(s.rows) : null;
  return map;
});

function baseline(slot: string): Pixels {
  return onDevice.value[slot] ?? blank();
}

function pixels(slot: string): Pixels {
  return edits[slot] ?? baseline(slot);
}

function info(slot: string) {
  return store.screens.find((s) => s.slot === slot);
}

function dirty(slot: string): boolean {
  const edit = edits[slot];
  return edit !== undefined && !samePixels(edit, baseline(slot));
}

const changed = computed(() => groups.flatMap((g) => g.slots).filter(dirty));
const group = computed(() => groups.find((g) => g.slots.includes(selected.value)) ?? groups[0]);
const selectedInfo = computed(() => info(selected.value));
/** The animation runs in the editor, so drawing is paused while it plays. */
const shown = computed(() => (playing.value ? pixels(group.value.slots[frame.value % group.value.slots.length]) : pixels(selected.value)));

function label(slot: string): string {
  const n = slot.slice(slot.lastIndexOf("_") + 1);
  return `Frame ${n}`;
}

// ---- history ----

function pushUndo(slot: string) {
  const stack = (undo[slot] ?? []).concat([pixels(slot)]);
  undo[slot] = stack.length > UNDO_LIMIT ? stack.slice(-UNDO_LIMIT) : stack;
}

function undoOne() {
  const stack = undo[selected.value] ?? [];
  if (!stack.length) return;
  edits[selected.value] = stack[stack.length - 1];
  undo[selected.value] = stack.slice(0, -1);
}

function forget(slot: string) {
  delete edits[slot];
  delete sources[slot];
  delete undo[slot];
}

/** Everything is per project, so a project switch drops the working copies. */
watch(
  () => store.status?.project,
  () => {
    for (const slot of Object.keys(edits)) forget(slot);
    playing.value = false;
  },
);

// ---- drawing ----

let strokeValue = 1;

function pointAt(event: PointerEvent): { x: number; y: number } {
  const box = (event.currentTarget as HTMLElement).getBoundingClientRect();
  return {
    x: Math.floor(((event.clientX - box.left) / box.width) * WIDTH),
    y: Math.floor(((event.clientY - box.top) / box.height) * HEIGHT),
  };
}

function stamp(px: Pixels, x: number, y: number) {
  const reach = brush.value - 1;
  for (let dy = -reach; dy <= reach; dy++) {
    for (let dx = -reach; dx <= reach; dx++) {
      const nx = x + dx;
      const ny = y + dy;
      if (nx < 0 || nx >= WIDTH || ny < 0 || ny >= HEIGHT) continue;
      px[ny * WIDTH + nx] = strokeValue;
    }
  }
}

function startStroke(event: PointerEvent) {
  // Left button draws, right button erases; ignore the rest.
  if (playing.value || (event.button !== 0 && event.button !== 2)) return;
  strokeValue = event.button === 2 || tool.value === "erase" ? 0 : 1;
  pushUndo(selected.value);
  const px = Uint8Array.from(pixels(selected.value));
  const { x, y } = pointAt(event);
  stamp(px, x, y);
  edits[selected.value] = px;
  (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
}

function continueStroke(event: PointerEvent) {
  const el = event.currentTarget as HTMLElement;
  if (playing.value || !el.hasPointerCapture(event.pointerId)) return;
  const px = Uint8Array.from(pixels(selected.value));
  const { x, y } = pointAt(event);
  stamp(px, x, y);
  edits[selected.value] = px;
}

function endStroke(event: PointerEvent) {
  const el = event.currentTarget as HTMLElement;
  if (el.hasPointerCapture(event.pointerId)) el.releasePointerCapture(event.pointerId);
}

function clear(value: 0 | 1) {
  pushUndo(selected.value);
  edits[selected.value] = new Uint8Array(WIDTH * HEIGHT).fill(value);
}

function invertSelected() {
  pushUndo(selected.value);
  edits[selected.value] = invert(pixels(selected.value));
}

function revert() {
  forget(selected.value);
}

// ---- loading an image ----

function render(src: Source): Pixels {
  return convert(src.source, src.width, src.height, conversion);
}

async function onFile(event: Event) {
  const input = event.target as HTMLInputElement;
  const file = input.files?.[0];
  input.value = "";
  if (!file) return;
  try {
    const image = await loadImage(file);
    const targets = fillGroup.value ? group.value.slots : [selected.value];
    for (const slot of targets) {
      pushUndo(slot);
      sources[slot] = image;
      edits[slot] = render(image);
    }
  } catch (e) {
    notify(errorText(e));
  }
}

/** Sliders re-convert the loaded image; the previous result stays on the undo stack. */
watch(conversion, () => {
  const src = sources[selected.value];
  if (src) edits[selected.value] = render(src);
});

// ---- animation ----

let timer: number | undefined;

watch([playing, speed, group], () => {
  window.clearInterval(timer);
  frame.value = 0;
  if (playing.value) timer = window.setInterval(() => frame.value++, speed.value);
});

onUnmounted(() => window.clearInterval(timer));

// ---- writing to the device ----

async function apply(slots: string[]) {
  for (const slot of slots) {
    if (!(await applyScreen(slot, pack(pixels(slot))))) return;
    forget(slot);
  }
  notify(
    slots.length === 1
      ? `${slots[0]} stored; the device shows it the next time the project loads`
      : `${slots.length} images stored; the device shows them the next time the project loads`,
    "info",
  );
}

async function putBackOriginal() {
  if (await restoreScreen(selected.value)) {
    forget(selected.value);
    notify(`${selected.value} put back`, "info");
  }
}
</script>

<template>
  <div class="screens">
    <div class="strip">
      <div v-if="store.screensLoading" class="loading muted"><span class="spinner" /> Reading images…</div>
      <section v-for="g in groups" :key="g.id" class="panel group">
        <div class="head">
          <span class="label">{{ g.label }}</span>
          <span class="count muted">{{ g.slots.length }} frames</span>
        </div>
        <p class="hint muted">{{ g.hint }}</p>
        <div class="frames" :class="{ wide: g.slots.length > 2 }">
          <button
            v-for="slot in g.slots"
            :key="slot"
            class="frame"
            :class="{ active: selected === slot, dirty: dirty(slot) }"
            @click="selected = slot"
          >
            <ScreenCanvas :pixels="info(slot) ? pixels(slot) : null" :scale="THUMB_SCALE" />
            <span class="caption">
              {{ label(slot) }}
              <span v-if="dirty(slot)" class="tag">edited</span>
              <span v-else-if="info(slot) && !info(slot)!.rows" class="tag missing">no file</span>
            </span>
          </button>
        </div>
      </section>
    </div>

    <div class="bar">
        <span class="slot mono">{{ selected }}</span>
        <span class="muted size">{{ WIDTH }} × {{ HEIGHT }}, black and white</span>
        <div class="spacer" />
        <button :class="{ active: playing }" @click="playing = !playing">
          <Icon name="play" /> {{ playing ? "Stop" : `Play ${group.label.toLowerCase()}` }}
        </button>
        <label v-if="playing" class="speed">
          <input v-model.number="speed" type="range" min="100" max="1200" step="50" />
          <span class="mono">{{ speed }} ms</span>
        </label>
      </div>

    <div class="work">
      <div class="drawing">
      <div class="stage">
        <ScreenCanvas
          class="board"
          :class="{ playing }"
          :pixels="shown"
          :scale="EDIT_SCALE"
          @pointerdown="startStroke"
          @pointermove="continueStroke"
          @pointerup="endStroke"
          @pointercancel="endStroke"
          @contextmenu.prevent
        />
        <p v-if="playing" class="muted note">
          Preview only — the device runs the animation at its own speed.
        </p>
        <p v-else class="muted note">Drag to draw, right-drag to erase.</p>
      </div>

      <div class="tools">
        <div class="tool-group">
          <button :class="{ active: tool === 'draw' }" :disabled="playing" @click="tool = 'draw'">Draw</button>
          <button :class="{ active: tool === 'erase' }" :disabled="playing" @click="tool = 'erase'">Erase</button>
        </div>
        <label class="field">
          <span>Brush</span>
          <select v-model.number="brush" :disabled="playing">
            <option :value="1">1 px</option>
            <option :value="2">3 px</option>
            <option :value="3">5 px</option>
          </select>
        </label>
        <div class="tool-group">
          <button :disabled="playing" @click="invertSelected">Invert</button>
          <button :disabled="playing" @click="clear(0)">Clear</button>
          <button :disabled="playing" @click="clear(1)">Fill</button>
        </div>
        <button :disabled="playing || !(undo[selected] ?? []).length" @click="undoOne">Undo</button>
        <button :disabled="!dirty(selected)" @click="revert">Revert</button>
      </div>
      </div>

      <div class="panel convert">
        <div class="head">
          <span class="label">From an image</span>
          <button class="primary small" :disabled="playing" @click="fileInput?.click()">Load image…</button>
          <input ref="fileInput" type="file" accept="image/*" hidden @change="onFile" />
          <label class="check">
            <input v-model="fillGroup" type="checkbox" />
            Put it in every frame of this group
          </label>
        </div>
        <div class="fields" :class="{ off: !sources[selected] }">
          <label class="field">
            <span>Fit</span>
            <select v-model="conversion.fit" :disabled="!sources[selected]">
              <option value="contain">Fit inside</option>
              <option value="cover">Fill and crop</option>
              <option value="stretch">Stretch</option>
            </select>
          </label>
          <label class="field">
            <span>Dither</span>
            <select v-model="conversion.dither" :disabled="!sources[selected]">
              <option value="none">None</option>
              <option value="floyd">Floyd–Steinberg</option>
              <option value="atkinson">Atkinson</option>
              <option value="bayer4">Ordered 4×4</option>
              <option value="bayer8">Ordered 8×8</option>
            </select>
          </label>
          <label class="field slider">
            <span>Threshold</span>
            <input v-model.number="conversion.threshold" type="range" min="0" max="255" :disabled="!sources[selected]" />
            <span class="mono">{{ conversion.threshold }}</span>
          </label>
          <label class="field slider">
            <span>Brightness</span>
            <input v-model.number="conversion.brightness" type="range" min="-100" max="100" :disabled="!sources[selected]" />
            <span class="mono">{{ conversion.brightness }}</span>
          </label>
          <label class="field slider">
            <span>Contrast</span>
            <input v-model.number="conversion.contrast" type="range" min="-100" max="100" :disabled="!sources[selected]" />
            <span class="mono">{{ conversion.contrast }}</span>
          </label>
          <label class="field check">
            <input v-model="conversion.invert" type="checkbox" :disabled="!sources[selected]" />
            <span>Invert</span>
          </label>
        </div>
        <p v-if="!sources[selected]" class="hint muted">
          Load a PNG, JPEG, GIF, WebP or BMP to use these. Moving them re-converts it, replacing
          anything drawn by hand — Undo brings it back.
        </p>
      </div>
    </div>

    <div class="footer">
        <button
          v-if="selectedInfo?.hasOriginal"
          :disabled="store.pending > 0"
          @click="putBackOriginal"
        >
          Put back the original
        </button>
        <p v-if="selectedInfo?.problem" class="problem">{{ selectedInfo.problem }}</p>
        <div class="spacer" />
        <button v-if="changed.length > 1" :disabled="store.pending > 0" @click="apply(changed)">
          Apply {{ changed.length }} changed
        </button>
        <button class="primary" :disabled="!dirty(selected) || store.pending > 0" @click="apply([selected])">
          Apply to project {{ store.status?.project }}
        </button>
    </div>
  </div>
</template>

<style scoped>
.screens {
  height: 100%;
  display: flex;
  flex-direction: column;
  gap: 14px;
  padding: 14px;
  overflow: auto;
}

/* Every frame of both groups, across the top. */
.strip {
  display: flex;
  flex-wrap: wrap;
  gap: 12px;
}

.loading {
  display: flex;
  align-items: center;
  gap: 8px;
}

.group {
  padding: 12px;
}

.head {
  display: flex;
  align-items: center;
  gap: 10px;
}

.hint {
  margin: 4px 0 10px;
  font-size: 12px;
}

.frames {
  display: flex;
  gap: 10px;
}

.frame {
  padding: 5px;
  display: block;
  border-color: var(--line);
  background: var(--panel-2);
}

.frame.active {
  border-color: var(--accent);
  box-shadow: 0 0 0 2px var(--accent-soft);
}

.caption {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-top: 5px;
  font-size: 11px;
  color: var(--muted);
}

.tag {
  color: var(--accent);
}

.tag.missing {
  color: var(--faint);
}

.bar,
.tools,
.footer {
  display: flex;
  align-items: center;
  gap: 10px;
  flex-wrap: wrap;
}

.work {
  display: flex;
  align-items: flex-start;
  gap: 16px;
  flex-wrap: wrap;
}

.drawing {
  display: flex;
  flex-direction: column;
  gap: 12px;
  flex: none;
}

.spacer {
  flex: 1;
}

.slot {
  color: var(--accent);
}

.size {
  font-size: 12px;
}

.speed {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 12px;
}

.board {
  cursor: crosshair;
  border: 1px solid var(--line-strong);
  touch-action: none;
}

.board.playing {
  cursor: default;
  border-color: var(--accent-line);
}

.note {
  margin: 6px 0 0;
  font-size: 12px;
}

button.active {
  border-color: var(--accent);
  color: var(--accent);
}

button.small {
  padding: 4px 10px;
  font-size: 13px;
}

.tool-group {
  display: flex;
  gap: 6px;
}

.convert {
  flex: 1;
  min-width: 340px;
  padding: 12px;
}

.convert .head {
  flex-wrap: wrap;
}

.convert .fields {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(230px, 1fr));
  gap: 10px 18px;
  margin-top: 12px;
}

.convert .fields.off {
  opacity: 0.55;
}

.field {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 13px;
}

.field > span:first-child {
  color: var(--muted);
}

/* Line the conversion controls up in their grid; the toolbar's stay snug. */
.convert .field > span:first-child {
  min-width: 74px;
}

.field.slider input[type="range"] {
  flex: 1;
  min-width: 90px;
}

.field.slider .mono {
  min-width: 34px;
  text-align: right;
  font-size: 12px;
}

.check {
  display: flex;
  align-items: center;
  gap: 7px;
  font-size: 12px;
  color: var(--muted);
}

.problem {
  margin: 0;
  color: var(--danger);
  font-size: 12px;
}

.footer {
  padding-bottom: 4px;
}
</style>
