<script setup lang="ts">
// A small view of the SD card for the Samples tab: walk into folders, audition a sound,
// and drag it onto a pad. It keeps its own listing so it does not fight with the Files
// tab, which owns `store.card`.
//
// Once it has focus, the arrow keys walk the list and each sound plays as it is reached,
// so a folder of samples can be auditioned without reaching for the mouse.
import { nextTick, onMounted, onUnmounted, ref } from "vue";
import Icon from "./Icon.vue";
import { api, errorText, type CardEntry } from "../api";
import { clickAfterDrag, drag, pressSound } from "../drag";
import {
  auditionPreview,
  cancelPreload,
  playPreview,
  playable,
  preload,
  preloadFolder,
  preview,
  stopPreview,
} from "../preview";
import { notify, store } from "../store";

const path = ref("");
const entries = ref<CardEntry[]>([]);
const loading = ref(false);
/** The row the keyboard is on, or -1 before the first key. */
const cursor = ref(-1);
const list = ref<HTMLElement>();

/** Open a folder; `select` names the entry to land on, such as the folder just left. */
async function go(to: string, select?: string) {
  if (to !== path.value) stopPreview();
  loading.value = true;
  try {
    const listing = await api.listVolume("card", to);
    path.value = listing.path;
    entries.value = listing.entries.filter((e) => e.isDir || playable(e.name));
    cursor.value = select ? entries.value.findIndex((e) => e.name === select) : -1;
    if (cursor.value >= 0) nextTick(() => reveal(cursor.value));
    if (store.prefs.preloadFolders) preloadFolder("card", entries.value);
  } catch (e) {
    notify(errorText(e));
  } finally {
    loading.value = false;
  }
}

onMounted(() => go("IMPORT"));
// Leaving the Samples tab leaves the folder: stop filling the cache for it.
onUnmounted(cancelPreload);

function up() {
  if (!path.value) return;
  const i = path.value.lastIndexOf("/");
  // Land back on the folder we came out of, as file managers do.
  go(i < 0 ? "" : path.value.slice(0, i), path.value.slice(i + 1));
}

function reveal(i: number) {
  list.value?.querySelector(`[data-index="${i}"]`)?.scrollIntoView({ block: "nearest" });
}

/** Move the keyboard to a row; a sound plays as soon as it is reached. */
function move(i: number) {
  if (i < 0 || i >= entries.value.length || i === cursor.value) return;
  cursor.value = i;
  reveal(i);
  const entry = entries.value[i];
  if (entry.isDir) stopPreview();
  else auditionPreview("card", entry.path, entry.size);
}

function open(entry: CardEntry | undefined) {
  if (!entry) return;
  if (entry.isDir) go(entry.path);
  else playPreview("card", entry.path, entry.size);
}

function onKey(event: KeyboardEvent) {
  const last = entries.value.length - 1;
  const current = entries.value[cursor.value];
  switch (event.key) {
    case "ArrowDown":
      move(Math.min(last, cursor.value + 1));
      break;
    case "ArrowUp":
      move(Math.max(0, cursor.value - 1));
      break;
    case "Home":
      move(0);
      break;
    case "End":
      move(last);
      break;
    case "Enter":
    case "ArrowRight":
      open(current);
      break;
    case "Backspace":
    case "ArrowLeft":
      up();
      break;
    case " ":
      // Replay or stop the sound under the cursor; a folder has nothing to play.
      if (current && !current.isDir) playPreview("card", current.path, current.size);
      break;
    default:
      return;
  }
  event.preventDefault();
}

function onRow(entry: CardEntry, i: number, event: MouseEvent) {
  // A drag that ended on a pad also lands a click here; that is not a request to play.
  if (clickAfterDrag()) return;
  // Nor is the second click of a double-click: it would stop the sound the first one
  // started, or, on a folder, land on a row of the folder just opened.
  if (event.detail > 1) return;
  cursor.value = i;
  // Take the keyboard, so the arrows carry on from the row just clicked.
  list.value?.focus({ preventScroll: true });
  open(entry);
}
</script>

<template>
  <div class="browser">
    <div class="head">
      <button class="ghost icon" title="Up one folder" :disabled="!path || loading" @click="up">
        <Icon name="up" />
      </button>
      <span class="where mono" :title="path || 'SD card'">{{ path || "SD card" }}</span>
      <span v-if="loading" class="spinner" />
      <div class="spacer" />
      <span v-if="preload.running" class="loaded mono" title="Fetching this folder's sounds">
        {{ preload.done }}/{{ preload.total }}
      </span>
      <button class="ghost icon" title="Reload" :disabled="loading" @click="go(path)">
        <Icon name="refresh" />
      </button>
    </div>

    <div ref="list" class="rows" tabindex="0" @keydown="onKey">
      <div
        v-for="(entry, i) in entries"
        :key="entry.path"
        :data-index="i"
        class="row"
        :class="{ sound: !entry.isDir, playing: preview.playing === entry.path, current: i === cursor }"
        @pointerdown="!entry.isDir && pressSound($event, { volume: 'card', path: entry.path, name: entry.name })"
        @click="onRow(entry, i, $event)"
      >
        <span v-if="preview.loading === entry.path" class="spinner" />
        <Icon v-else :name="entry.isDir ? 'folder' : preview.playing === entry.path ? 'stop' : 'play'" />
        <span class="name">{{ entry.name }}</span>
      </div>
      <p v-if="!entries.length && !loading" class="empty muted">No folders or sounds here.</p>
    </div>

    <p class="muted hint">
      {{
        drag.sound
          ? `Drop ${drag.sound.name} on a pad to import it.`
          : "Click a sound to hear it, then use the arrow keys to hear the rest. Drag one onto a pad to import it."
      }}
    </p>
  </div>
</template>

<style scoped>
.browser {
  display: flex;
  flex-direction: column;
  gap: 6px;
  min-height: 0;
}

.head {
  display: flex;
  align-items: center;
  gap: 6px;
}

.where {
  font-size: 11.5px;
  color: var(--muted);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  direction: rtl;
  text-align: left;
}

.spacer {
  flex: 1;
}

.loaded {
  font-size: 11px;
  color: var(--faint);
}

.rows {
  max-height: 168px;
  overflow: auto;
  border: 1px solid var(--line);
  border-radius: 6px;
  background: #0f1013;
  padding: 3px;
}

.row {
  display: flex;
  align-items: center;
  gap: 7px;
  padding: 4px 7px;
  border-radius: 5px;
  font-size: 12.5px;
  color: var(--muted);
  cursor: default;
}

.row.sound {
  cursor: grab;
  color: var(--text);
}

.row:hover {
  background: var(--panel-2);
}

.rows:focus {
  outline: none;
}

/* Only show where the keyboard is while it can act on it. */
.rows:focus-visible,
.rows:focus-within {
  border-color: var(--accent-line);
}

.rows:focus .row.current {
  background: var(--accent-soft);
  color: var(--text);
}

.row.playing {
  color: var(--accent);
}

.name {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.empty {
  margin: 0;
  padding: 8px;
  font-size: 12px;
}

.hint {
  margin: 0;
  font-size: 12px;
}
</style>
