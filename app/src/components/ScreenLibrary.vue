<script setup lang="ts">
// The screen library: frames and sets kept on the computer, to reuse, apply to many
// projects at once and share.
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { open, save } from "@tauri-apps/plugin-dialog";
import ApplyScreens from "./ApplyScreens.vue";
import EditableName from "./EditableName.vue";
import Icon from "./Icon.vue";
import ScreenCanvas from "./ScreenCanvas.vue";
import { api, errorText, type LibraryItem } from "../api";
import { SCREEN_SAVER_SLOTS, STARTUP_SLOTS, unpack, type Pixels } from "../screens";
import {
  deleteLibraryItem,
  exportLibraryItem,
  importToLibrary,
  loadLibrary,
  notify,
  promptText,
  renameLibraryItem,
  saveToLibrary,
  slotName,
  store,
} from "../store";

const emit = defineEmits<{ edit: [item: LibraryItem] }>();

const filter = ref("");
const selectedName = ref<string | null>(null);
const applying = ref<LibraryItem | null>(null);
const sharing = ref<{ name: string; code: string } | null>(null);
const copied = ref(false);
const codeBox = ref<HTMLTextAreaElement>();
/** The frame of a set shown large while it plays. */
const tick = ref(0);
/** The frame of the selected set under the pointer, shown large instead. */
const hovered = ref<number | null>(null);

if (!store.library.loading) loadLibrary();

const cache = new WeakMap<number[], Pixels>();
function px(rows: number[]): Pixels {
  let p = cache.get(rows);
  if (!p) {
    p = unpack(rows);
    cache.set(rows, p);
  }
  return p;
}

const shown = computed(() => {
  const q = filter.value.trim().toLowerCase();
  return q ? store.library.items.filter((i) => i.name.toLowerCase().includes(q)) : store.library.items;
});
const sets = computed(() => shown.value.filter((i) => i.kind === "set"));
const frames = computed(() => shown.value.filter((i) => i.kind === "frame"));
const selected = computed(() => store.library.items.find((i) => i.name === selectedName.value) ?? null);

let timer: number | undefined;
onMounted(() => (timer = window.setInterval(() => tick.value++, 1000)));
onBeforeUnmount(() => window.clearInterval(timer));
watch(selectedName, () => {
  tick.value = 0;
  hovered.value = null;
});

const SET_SLOTS = [...STARTUP_SLOTS, ...SCREEN_SAVER_SLOTS];

const blank = (rows: number[]) => rows.every((b) => b === 0);

/** The screen saver frames that are not blank, as the device plays them. */
const animation = computed(() => {
  const item = selected.value;
  if (!item || item.kind !== "set") return [];
  return [2, 3, 4, 5].filter((i) => !blank(item.frames[i]));
});

/**
 * The large preview. A set shows the frame under the pointer; otherwise it plays its
 * screen saver, or shows Startup 1 when the screen saver is blank.
 */
const preview = computed((): { pixels: Pixels | null; caption: string } => {
  const item = selected.value;
  if (!item) return { pixels: null, caption: "" };
  if (item.kind === "frame") return { pixels: px(item.frames[0]), caption: "" };
  const frames = animation.value;
  const i = hovered.value ?? (frames.length ? frames[tick.value % frames.length] : 0);
  return { pixels: px(item.frames[i]), caption: slotName(SET_SLOTS[i]) };
});

async function rename(to: string) {
  const item = selected.value;
  if (!item) return;
  const renamed = await renameLibraryItem(item, to);
  if (renamed) selectedName.value = renamed.name;
}

async function remove() {
  if (selected.value && (await deleteLibraryItem(selected.value))) selectedName.value = null;
}

async function addFiles() {
  try {
    const picked = await open({
      multiple: true,
      title: "Add to the library",
      filters: [{ name: "Display images", extensions: ["png", "bmp", "PNG", "BMP"] }],
    });
    if (picked) await importToLibrary(Array.isArray(picked) ? picked : [picked]);
  } catch (e) {
    notify(errorText(e));
  }
}

async function exportFile() {
  const item = selected.value;
  if (!item) return;
  try {
    const to = await save({
      defaultPath: `${item.name}.png`,
      title: `Save "${item.name}"`,
      filters: [{ name: "PNG image", extensions: ["png"] }],
    });
    if (to) await exportLibraryItem(item, to);
  } catch (e) {
    notify(errorText(e));
  }
}

async function share() {
  const item = selected.value;
  if (!item || item.kind !== "frame") return;
  try {
    sharing.value = { name: item.name, code: await api.shareCode(item.frames[0]) };
    copied.value = false;
  } catch (e) {
    notify(errorText(e));
  }
}

async function copyCode() {
  if (!sharing.value) return;
  try {
    await navigator.clipboard.writeText(sharing.value.code);
  } catch {
    // Some webviews refuse the clipboard API; selecting and copying still works there.
    codeBox.value?.select();
    document.execCommand("copy");
  }
  copied.value = true;
}

async function pasteCode() {
  const text = await promptText(
    "Add a shared frame",
    "Paste a share code (it starts with sparky1:). A whole chat message is fine; the code is picked out of it.",
    "",
    "Add",
    true,
  );
  if (!text) return;
  let rows: number[];
  try {
    rows = await api.readShareCode(text);
  } catch (e) {
    notify(errorText(e));
    return;
  }
  const name = await promptText("Name the frame", "It is saved to the library under this name.", "Shared frame", "Save");
  if (!name) return;
  const item = await saveToLibrary(name, [rows]);
  if (item) selectedName.value = item.name;
}

function onKey(e: KeyboardEvent) {
  if (e.key === "Escape" && sharing.value) sharing.value = null;
}
onMounted(() => window.addEventListener("keydown", onKey));
onBeforeUnmount(() => window.removeEventListener("keydown", onKey));
</script>

<template>
  <div class="library">
    <div class="toolbar">
      <input v-model="filter" class="search" placeholder="Search the library" />
      <div class="spacer" />
      <button @click="pasteCode">Paste share code…</button>
      <button @click="addFiles">Add files…</button>
      <button class="ghost" title="Read the library folder again" :disabled="store.library.loading" @click="loadLibrary">
        <Icon name="refresh" />
      </button>
    </div>

    <div class="body">
      <div class="entries">
        <div v-if="store.library.loading && !store.library.items.length" class="muted loading">
          <span class="spinner" /> Reading the library…
        </div>
        <div v-else-if="!store.library.items.length" class="empty muted">
          <p>The library is empty.</p>
          <p>
            Every image you apply in the editor is kept here, and you can save a frame or a whole project's six
            images from the editor. Frames and sets can then go to many projects at once.
          </p>
        </div>
        <p v-else-if="!shown.length" class="muted">Nothing matches "{{ filter }}".</p>

        <section v-if="sets.length">
          <div class="head"><span class="label">Sets</span><span class="muted count">{{ sets.length }}</span></div>
          <div class="grid sets">
            <button
              v-for="item in sets"
              :key="item.path"
              class="item"
              :class="{ active: selectedName === item.name }"
              @click="selectedName = item.name"
              @dblclick="emit('edit', item)"
            >
              <span class="strip">
                <ScreenCanvas v-for="(f, i) in item.frames" :key="i" :pixels="px(f)" :scale="1" :class="{ gap: i === 2 }" />
              </span>
              <span class="name">{{ item.name }}</span>
            </button>
          </div>
        </section>

        <section v-if="frames.length">
          <div class="head"><span class="label">Frames</span><span class="muted count">{{ frames.length }}</span></div>
          <div class="grid">
            <button
              v-for="item in frames"
              :key="item.path"
              class="item"
              :class="{ active: selectedName === item.name }"
              @click="selectedName = item.name"
              @dblclick="emit('edit', item)"
            >
              <ScreenCanvas :pixels="px(item.frames[0])" :scale="1.5" />
              <span class="name">{{ item.name }}</span>
            </button>
          </div>
        </section>

        <details v-if="store.library.skipped.length" class="skipped muted">
          <summary>{{ store.library.skipped.length }} other files in the folder were left out</summary>
          <ul>
            <li v-for="why in store.library.skipped" :key="why">{{ why }}</li>
          </ul>
        </details>
        <p class="muted where">
          Library folder: <span class="mono">{{ store.library.dir }}</span> (change it in Settings)
        </p>
      </div>

      <aside v-if="selected" class="detail panel">
        <EditableName :value="selected.name" :max-length="80" @commit="rename" />
        <div class="muted kind">{{ selected.kind === "set" ? "Set: 2 startup and 4 screen saver frames" : "Frame" }}</div>
        <ScreenCanvas class="big" :pixels="preview.pixels" :scale="2" />
        <div v-if="preview.caption" class="muted caption">{{ preview.caption }}</div>
        <div v-if="selected.kind === 'set'" class="parts">
          <div class="muted small-label">Startup</div>
          <div class="row">
            <ScreenCanvas
              v-for="i in [0, 1]"
              :key="i"
              :pixels="px(selected.frames[i])"
              :scale="1"
              :class="{ shown: preview.caption === slotName(SET_SLOTS[i]) }"
              @mouseenter="hovered = i"
              @mouseleave="hovered = null"
            />
          </div>
          <div class="muted small-label">Screen saver</div>
          <div class="row">
            <ScreenCanvas
              v-for="i in [2, 3, 4, 5]"
              :key="i"
              :pixels="px(selected.frames[i])"
              :scale="1"
              :class="{ shown: preview.caption === slotName(SET_SLOTS[i]) }"
              @mouseenter="hovered = i"
              @mouseleave="hovered = null"
            />
          </div>
        </div>
        <div class="actions">
          <button class="primary" :disabled="store.pending > 0" @click="applying = selected">Apply to projects…</button>
          <button @click="emit('edit', selected)">Open in the editor</button>
          <button v-if="selected.kind === 'frame'" @click="share">Share code…</button>
          <button @click="exportFile">Save as file…</button>
          <button class="danger" @click="remove">Delete</button>
        </div>
        <p v-if="selected.kind === 'set'" class="muted hint">
          Share a set by sending its file: whoever gets it adds it with Add files….
        </p>
      </aside>
    </div>

    <ApplyScreens v-if="applying" :item="applying" @close="applying = null" />

    <div v-if="sharing" class="backdrop" @click.self="sharing = null">
      <div class="dialog panel" role="dialog" aria-modal="true">
        <h2>Share "{{ sharing.name }}"</h2>
        <p class="muted">
          Paste this anywhere, a chat message will do. In SparkyMK2, Paste share code… turns it back into the frame.
          {{ sharing.code.length.toLocaleString() }} characters.
        </p>
        <textarea ref="codeBox" class="code mono" readonly :value="sharing.code" @focus="codeBox?.select()" />
        <div class="buttons">
          <span v-if="copied" class="ok">Copied</span>
          <div class="spacer" />
          <button @click="sharing = null">Close</button>
          <button class="primary" @click="copyCode">Copy</button>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.library {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 12px;
  min-height: 0;
}

.toolbar,
.head,
.buttons {
  display: flex;
  align-items: center;
  gap: 8px;
}

.search {
  width: 260px;
}

.spacer {
  flex: 1;
}

.body {
  flex: 1;
  min-height: 0;
  display: flex;
  gap: 16px;
}

.entries {
  flex: 1;
  min-width: 0;
  overflow: auto;
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.loading {
  display: flex;
  align-items: center;
  gap: 8px;
}

.empty p {
  margin: 0 0 8px;
  max-width: 560px;
}

.count {
  font-size: 12px;
}

.grid {
  display: flex;
  flex-wrap: wrap;
  gap: 10px;
  margin-top: 8px;
}

.item {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 6px;
  border-color: var(--line);
  background: var(--panel-2);
  text-align: left;
}

.item.active {
  border-color: var(--accent);
  box-shadow: 0 0 0 2px var(--accent-soft);
}

.item .name {
  max-width: 100%;
  font-size: 12px;
  color: var(--muted);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.sets .item .name {
  max-width: 800px;
}

.strip {
  display: flex;
  gap: 3px;
}

/* Space between the startup frames and the screen saver frames. */
.strip .gap {
  margin-left: 8px;
}

.skipped {
  font-size: 12px;
}

.where {
  margin: 0;
  font-size: 12px;
}

.detail {
  width: 300px;
  flex: none;
  padding: 14px;
  display: flex;
  flex-direction: column;
  gap: 10px;
  overflow: auto;
}

.caption {
  margin-top: -6px;
  font-size: 12px;
}

.kind,
.small-label,
.hint {
  font-size: 12px;
}

.hint {
  margin: 0;
}

.big {
  border: 1px solid var(--line-strong);
  align-self: flex-start;
}

.parts canvas {
  outline: 1px solid transparent;
  cursor: zoom-in;
}

/* The frame the large preview is showing. */
.parts canvas.shown {
  outline-color: var(--accent);
}

.parts .row {
  display: grid;
  grid-template-columns: repeat(2, max-content);
  gap: 4px;
  margin: 4px 0 8px;
}

.actions {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.backdrop {
  position: fixed;
  inset: 0;
  z-index: 20;
  display: grid;
  place-items: center;
  background: rgba(5, 6, 8, 0.6);
}

.dialog {
  width: 520px;
  padding: 20px;
  box-shadow: 0 20px 60px rgba(0, 0, 0, 0.5);
}

.dialog h2 {
  margin: 0 0 8px;
  font-size: 16px;
}

.dialog p {
  margin: 0;
  font-size: 13px;
}

.code {
  width: 100%;
  height: 120px;
  margin-top: 12px;
  resize: vertical;
  font-size: 12px;
  word-break: break-all;
}

.buttons {
  margin-top: 14px;
}

.ok {
  color: var(--ok);
  font-size: 13px;
}
</style>
