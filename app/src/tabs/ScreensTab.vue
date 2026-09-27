<script setup lang="ts">
import { computed, onUnmounted, reactive, ref, shallowReactive, watch } from "vue";
import Icon from "../components/Icon.vue";
import ScreenCanvas from "../components/ScreenCanvas.vue";
import ScreenLibrary from "../components/ScreenLibrary.vue";
import { errorText, type LibraryItem } from "../api";
import {
  DEFAULT_CONVERSION,
  DEFAULT_FRAMING,
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
import {
  applyScreen,
  loadLibrary,
  notify,
  projectName,
  promptText,
  restoreScreen,
  saveToLibrary,
  slotName,
  store,
} from "../store";

const EDIT_SCALE = 4;
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
const tool = ref<"draw" | "erase" | "move">("draw");
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

const framing = computed(() => sources[selected.value] !== undefined);
/** Dragging pans the image while Move is picked and there is an image to pan. */
const panning = computed(() => tool.value === "move" && framing.value);
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

/** Where the pointer was last seen while panning, in device pixels. */
let panFrom: { x: number; y: number } | null = null;

function startStroke(event: PointerEvent) {
  // Left button draws, right button erases; ignore the rest.
  if (playing.value || (event.button !== 0 && event.button !== 2)) return;
  if (tool.value === "move") {
    // Move never paints: with no image loaded there is simply nothing to drag.
    if (!panning.value) return;
    panFrom = pointAt(event);
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
    return;
  }
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
  if (panFrom) {
    const now = pointAt(event);
    conversion.offsetX += now.x - panFrom.x;
    conversion.offsetY += now.y - panFrom.y;
    panFrom = now;
    return;
  }
  const px = Uint8Array.from(pixels(selected.value));
  const { x, y } = pointAt(event);
  stamp(px, x, y);
  edits[selected.value] = px;
}

function endStroke(event: PointerEvent) {
  const el = event.currentTarget as HTMLElement;
  if (el.hasPointerCapture(event.pointerId)) el.releasePointerCapture(event.pointerId);
  panFrom = null;
}

/** The wheel zooms about the middle while framing, which is what a crop wants. */
function onWheel(event: WheelEvent) {
  if (!panning.value || playing.value) return;
  event.preventDefault();
  const step = event.deltaY < 0 ? 1.1 : 1 / 1.1;
  conversion.zoom = Math.min(20, Math.max(0.1, conversion.zoom * step));
}

function resetFraming() {
  Object.assign(conversion, DEFAULT_FRAMING);
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
    Object.assign(conversion, DEFAULT_FRAMING);
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

/** Play one group in the editor, from its first frame. */
function play(slots: string[]) {
  selected.value = slots[0];
  playing.value = true;
}

function pick(slot: string) {
  selected.value = slot;
  playing.value = false;
}

// The list's own screen saver thumbnail always runs, a frame a second, skipping blanks.
const listTick = ref(0);
const listTimer = window.setInterval(() => listTick.value++, 1000);
onUnmounted(() => window.clearInterval(listTimer));

const saverThumb = computed(() => {
  const lit = SCREEN_SAVER_SLOTS.map(pixels).filter((p) => p.some((v) => v));
  return lit.length ? lit[listTick.value % lit.length] : blank();
});

// ---- sizing ----

/** The largest whole-pixel scale at which the image fits the space the editor has. */
const stage = ref<HTMLElement>();
const editScale = ref(EDIT_SCALE);
const observer = new ResizeObserver(([entry]) => {
  const { width, height } = entry.contentRect;
  editScale.value = Math.max(2, Math.floor(Math.min((width - 2) / WIDTH, (height - 2) / HEIGHT)));
});
watch(stage, (el, old) => {
  if (old) observer.unobserve(old);
  if (el) observer.observe(el);
});
onUnmounted(() => observer.disconnect());

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

// ---- the library ----

const ALL_SLOTS = [...STARTUP_SLOTS, ...SCREEN_SAVER_SLOTS];
const picking = ref(false);

/** Load a library entry as edits: a frame into the selected slot, a set into all six. */
function openInEditor(item: LibraryItem) {
  const targets = item.kind === "set" ? ALL_SLOTS : [selected.value];
  targets.forEach((slot, i) => {
    pushUndo(slot);
    delete sources[slot];
    edits[slot] = unpack(item.frames[i]);
  });
  playing.value = false;
  picking.value = false;
  store.screensMode = "editor";
  notify(`"${item.name}" loaded; Apply writes it to project ${store.status?.project}`, "info");
}

function openPicker() {
  if (!store.library.loaded && !store.library.loading) loadLibrary();
  picking.value = true;
}

async function saveFrame() {
  const slot = selected.value;
  const project = store.status?.project ?? 0;
  const name = await promptText("Save to the library", "Name the frame.", `${projectName(project)} – ${slotName(slot)}`, "Save");
  if (name) await saveToLibrary(name, [pack(pixels(slot))]);
}

async function saveSet() {
  const project = store.status?.project ?? 0;
  const name = await promptText(
    "Save all six as a set",
    "The two startup frames and the four screen saver frames, as they are in the editor now, go to the library together.",
    projectName(project),
    "Save",
  );
  if (name) await saveToLibrary(name, ALL_SLOTS.map((slot) => pack(pixels(slot))));
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
    <div class="modes">
      <button :class="{ active: store.screensMode === 'editor' }" @click="store.screensMode = 'editor'">Editor</button>
      <button :class="{ active: store.screensMode === 'library' }" @click="store.screensMode = 'library'">
        Library
        <span v-if="store.library.loaded" class="muted">{{ store.library.items.length }}</span>
      </button>
    </div>
    <ScreenLibrary v-if="store.screensMode === 'library'" @edit="openInEditor" />
    <div v-else class="editor">
      <nav class="slots panel">
        <div v-if="store.screensLoading" class="loading muted"><span class="spinner" /> Reading images…</div>
        <div class="label" :title="groups[0].hint">Startup</div>
        <button
          v-for="slot in STARTUP_SLOTS"
          :key="slot"
          class="slot-row"
          :class="{ active: selected === slot && !playing }"
          @click="pick(slot)"
        >
          <ScreenCanvas :pixels="info(slot) || dirty(slot) ? pixels(slot) : null" :scale="1" />
          <span class="caption">
            {{ slotName(slot) }}
            <span v-if="dirty(slot)" class="tag">edited</span>
            <span v-else-if="info(slot) && !info(slot)!.rows" class="tag missing">no file</span>
          </span>
        </button>

        <div class="label" :title="groups[1].hint">Screen saver</div>
        <button
          class="slot-row"
          :class="{ active: playing && group.id === 'saver' }"
          title="Play the screen saver in the editor"
          @click="play(SCREEN_SAVER_SLOTS)"
        >
          <ScreenCanvas :pixels="saverThumb" :scale="1" />
          <span class="caption"><Icon name="play" :size="11" /> Animation</span>
        </button>
        <div class="nested">
          <button
            v-for="slot in SCREEN_SAVER_SLOTS"
            :key="slot"
            class="slot-row"
            :class="{ active: selected === slot && !playing }"
            @click="pick(slot)"
          >
            <ScreenCanvas :pixels="info(slot) || dirty(slot) ? pixels(slot) : null" :scale="1" />
            <span class="caption">
              {{ label(slot) }}
              <span v-if="dirty(slot)" class="tag">edited</span>
              <span v-else-if="info(slot) && !info(slot)!.rows" class="tag missing">no file</span>
            </span>
          </button>
        </div>
      </nav>

      <section class="center">
        <div class="bar">
          <span class="title">{{ playing ? `${group.label} animation` : slotName(selected) }}</span>
          <span class="muted size">{{ WIDTH }} × {{ HEIGHT }}, black and white</span>
          <div class="spacer" />
          <template v-if="playing">
            <label class="speed">
              <input v-model.number="speed" type="range" min="100" max="1200" step="50" />
              <span class="mono">{{ speed }} ms</span>
            </label>
            <button @click="playing = false"><Icon name="stop" /> Stop</button>
          </template>
          <button v-else @click="play(group.slots)"><Icon name="play" /> Play {{ group.label.toLowerCase() }}</button>
        </div>

        <div class="tools">
          <div class="tool-group">
            <button :class="{ active: tool === 'draw' }" :disabled="playing" @click="tool = 'draw'">Draw</button>
            <button :class="{ active: tool === 'erase' }" :disabled="playing" @click="tool = 'erase'">Erase</button>
            <button
              :class="{ active: tool === 'move' }"
              :disabled="playing || !framing"
              :title="framing ? 'Drag the loaded image to frame it' : 'Load an image to frame it'"
              @click="tool = 'move'"
            >
              Move
            </button>
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
          <div class="tool-group">
            <button :disabled="playing || !(undo[selected] ?? []).length" @click="undoOne">Undo</button>
            <button :disabled="!dirty(selected)" @click="revert">Revert</button>
          </div>
        </div>

        <div ref="stage" class="stage">
          <ScreenCanvas
            class="board"
            :class="{ playing, panning }"
            :pixels="shown"
            :scale="editScale"
            @pointerdown="startStroke"
            @pointermove="continueStroke"
            @pointerup="endStroke"
            @pointercancel="endStroke"
            @wheel="onWheel"
            @contextmenu.prevent
          />
        </div>
        <p v-if="playing" class="muted note">Preview only — the device runs the animation at its own speed.</p>
        <p v-else-if="panning" class="muted note">
          Drag to move the image, scroll to zoom. Anything outside the screen is cropped off.
        </p>
        <p v-else class="muted note">Drag to draw, right-drag to erase.</p>

        <div class="footer">
          <button v-if="selectedInfo?.hasOriginal" :disabled="store.pending > 0" @click="putBackOriginal">
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
      </section>

      <aside class="side panel">
        <div class="label">Load</div>
        <button class="primary" :disabled="playing" @click="fileInput?.click()">Load image…</button>
        <input ref="fileInput" type="file" accept="image/*" hidden @change="onFile" />
        <label class="check">
          <input v-model="fillGroup" type="checkbox" />
          Put it in every frame of {{ group.label.toLowerCase() }}
        </label>
        <button :disabled="playing" @click="openPicker">From library…</button>

        <div class="label">Image</div>
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
            <span>Zoom</span>
            <input v-model.number="conversion.zoom" type="range" min="0.1" max="8" step="0.05" :disabled="!sources[selected]" />
            <span class="mono">{{ conversion.zoom.toFixed(2) }}×</span>
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
          <div class="field-row">
            <label class="check">
              <input v-model="conversion.invert" type="checkbox" :disabled="!sources[selected]" />
              Invert
            </label>
            <button class="small" :disabled="!sources[selected]" @click="resetFraming">Recentre</button>
          </div>
        </div>
        <p v-if="!sources[selected]" class="hint muted">
          Load a PNG, JPEG, GIF, WebP or BMP to use these. Pick Move to drag the image around and crop the part you
          want. Changing these re-converts it, replacing anything drawn by hand — Undo brings it back.
        </p>

        <div class="label">Library</div>
        <button :disabled="playing" @click="saveFrame">Save {{ slotName(selected).toLowerCase() }} to library</button>
        <button @click="saveSet">Save all six as a set</button>
      </aside>
    </div>

    <div v-if="picking" class="backdrop" @click.self="picking = false">
      <div class="picker panel" role="dialog" aria-modal="true">
        <h2>From the library</h2>
        <p class="muted">
          A frame goes into {{ slotName(selected) }}; a set fills all six. Nothing is written until you apply.
        </p>
        <div v-if="store.library.loading" class="muted"><span class="spinner" /> Reading the library…</div>
        <p v-else-if="!store.library.items.length" class="muted">The library is empty.</p>
        <div class="choices">
          <button v-for="item in store.library.items" :key="item.path" class="choice" @click="openInEditor(item)">
            <ScreenCanvas :pixels="unpack(item.frames[item.kind === 'set' ? 2 : 0])" :scale="1" />
            <span class="name">{{ item.name }}</span>
            <span v-if="item.kind === 'set'" class="tag">set</span>
          </button>
        </div>
        <div class="picker-buttons">
          <button @click="picking = false">Cancel</button>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.modes {
  display: flex;
  gap: 4px;
  flex: none;
}

.modes button.active {
  border-color: var(--accent);
  color: var(--accent);
  background: var(--accent-soft);
}

.backdrop {
  position: fixed;
  inset: 0;
  z-index: 20;
  display: grid;
  place-items: center;
  background: rgba(5, 6, 8, 0.6);
}

.picker {
  width: min(760px, calc(100vw - 40px));
  max-height: calc(100vh - 40px);
  display: flex;
  flex-direction: column;
  padding: 20px;
  box-shadow: 0 20px 60px rgba(0, 0, 0, 0.5);
}

.picker h2 {
  margin: 0 0 8px;
  font-size: 16px;
}

.picker p {
  margin: 0 0 12px;
  font-size: 13px;
}

.choices {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  overflow: auto;
}

.choice {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 4px;
  padding: 6px;
  background: var(--panel-2);
}

.choice .name {
  max-width: 128px;
  font-size: 12px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.picker-buttons {
  display: flex;
  justify-content: flex-end;
  margin-top: 14px;
}

.screens {
  height: 100%;
  display: flex;
  flex-direction: column;
  gap: 12px;
  padding: 14px;
  min-height: 0;
}

.editor {
  flex: 1;
  min-height: 0;
  display: grid;
  grid-template-columns: auto minmax(0, 1fr) 300px;
  gap: 14px;
}

.loading {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 12px;
}

/* The six images, down the left. */
.slots {
  display: flex;
  flex-direction: column;
  gap: 5px;
  padding: 10px;
  overflow: auto;
}

.slots .label:not(:first-child) {
  margin-top: 8px;
}



.slot-row {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 3px;
  padding: 4px;
  border-color: var(--line);
  background: var(--panel-2);
}

.slot-row.active {
  border-color: var(--accent);
  box-shadow: 0 0 0 2px var(--accent-soft);
}

.nested {
  display: flex;
  flex-direction: column;
  gap: 5px;
  margin-left: 12px;
  padding-left: 10px;
  border-left: 1px solid var(--line);
}

.caption {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 11px;
  color: var(--muted);
}

.tag {
  color: var(--accent);
}

.tag.missing {
  color: var(--faint);
}

/* The editor itself: title, tools, the image as large as it fits, and the actions. */
.center {
  min-height: 0;
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.bar,
.tools,
.footer,
.field-row {
  display: flex;
  align-items: center;
  gap: 10px;
  flex-wrap: wrap;
}

.title {
  font-weight: 600;
}

.stage {
  flex: 1;
  min-height: 160px;
  display: grid;
  place-items: center;
  overflow: hidden;
}

.spacer {
  flex: 1;
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

.board.panning {
  cursor: grab;
}

.board.panning:active {
  cursor: grabbing;
}

.note {
  margin: 0;
  font-size: 12px;
  text-align: center;
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

/* Where images come from and go to, on the right. */
.side {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 12px;
  overflow: auto;
}

.side .label:not(:first-child) {
  margin-top: 12px;
}

.side .fields {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.side .fields.off {
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

.side .field > span:first-child {
  min-width: 74px;
}

.side .field select {
  flex: 1;
}

.field.slider input[type="range"] {
  flex: 1;
  min-width: 60px;
}

.field.slider .mono {
  min-width: 40px;
  text-align: right;
  font-size: 12px;
}

.field-row {
  justify-content: space-between;
}

.check {
  display: flex;
  align-items: center;
  gap: 7px;
  font-size: 12px;
  color: var(--muted);
}

.hint {
  margin: 0;
  font-size: 12px;
}

.problem {
  margin: 0;
  color: var(--danger);
  font-size: 12px;
}
</style>
