<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref } from "vue";
import { open, save } from "@tauri-apps/plugin-dialog";
import Icon from "../components/Icon.vue";
import { errorText, type CardEntry, type Volume } from "../api";
import { bytes as formatBytes, storage } from "../format";
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
import {
  ask,
  cardSized,
  createCardFolder,
  deleteFromCard,
  downloadFromCard,
  loadCard,
  notify,
  renameOnCard,
  store,
  uploadToCard,
} from "../store";

const selected = ref<CardEntry | null>(null);
const listEl = ref<HTMLElement>();
const renaming = ref<string | null>(null);
const renameText = ref("");

/** The card first: its IMPORT folder is what the device's own IMPORT browser reads. */
const volumes: { id: Volume; label: string; hint: string }[] = [
  { id: "card", label: "SD card", hint: "IMPORT, EXPORT and BKUP. Sounds dropped in IMPORT show up in the device's own IMPORT browser." },
  { id: "internal", label: "Device storage", hint: "The device's own memory: projects, samples and the factory library. Changing these can break a project." },
];

onMounted(async () => {
  if (!store.card) await go("", "card");
  // Coming back to the tab: fill in whatever of this folder is not cached yet.
  else preloadWhenSized();
});

onUnmounted(() => {
  stopPreview();
  cancelPreload();
});

const volume = computed<Volume>(() => store.card?.volume ?? "card");
const path = computed(() => store.card?.path ?? "");
const entries = computed(() => store.card?.entries ?? []);
/** Where the keyboard is: the selected row. Listings are rebuilt, so match by path. */
const cursor = computed(() => entries.value.findIndex((e) => e.path === selected.value?.path));
const hint = computed(() => volumes.find((v) => v.id === volume.value)?.hint ?? "");

/** The path split into the steps of a breadcrumb, root first. */
const crumbs = computed(() => {
  const parts = path.value ? path.value.split("/") : [];
  let walked = "";
  return parts.map((name) => {
    walked = walked ? `${walked}/${name}` : name;
    return { name, path: walked };
  });
});

/** Files the device owns; deleting or renaming them can break a project. */
function isProjectFile(entry: CardEntry): boolean {
  if (volume.value === "internal") return true;
  return entry.name.endsWith(".bin");
}

/** Open a folder; `select` names the entry to land on, such as the folder just left. */
async function go(to: string, to_volume: Volume = volume.value, select?: string) {
  selected.value = null;
  renaming.value = null;
  stopPreview();
  await loadCard(to_volume, to);
  if (select) {
    selected.value = entries.value.find((e) => e.name === select) ?? null;
    nextTick(() => reveal(cursor.value));
  }
  preloadWhenSized();
}

/** Preload the folder once its sizes are in, since the cache keys sounds by size. */
async function preloadWhenSized() {
  const listing = store.card;
  if (!store.prefs.preloadFolders || !listing) return;
  await cardSized();
  if (store.card === listing) preloadFolder(listing.volume, listing.entries);
}

function up() {
  if (!path.value) return;
  const i = path.value.lastIndexOf("/");
  // Land back on the folder we came out of, as file managers do.
  go(i < 0 ? "" : path.value.slice(0, i), volume.value, path.value.slice(i + 1));
}

function openEntry(entry: CardEntry | null | undefined) {
  if (!entry) return;
  if (entry.isDir) go(entry.path);
  else if (playable(entry.name)) playPreview(volume.value, entry.path, entry.size);
}

function reveal(i: number) {
  listEl.value?.querySelector(`[data-index="${i}"]`)?.scrollIntoView({ block: "nearest" });
}

/** Select a row from the keyboard; a sound plays as soon as it is reached. */
function move(i: number) {
  if (i < 0 || i >= entries.value.length || i === cursor.value) return;
  const entry = entries.value[i];
  selected.value = entry;
  renaming.value = null;
  reveal(i);
  if (!entry.isDir && playable(entry.name)) auditionPreview(volume.value, entry.path, entry.size);
  else stopPreview();
}

function onKey(event: KeyboardEvent) {
  // The rename box sits inside the list; keys typed there are for the name, not the list.
  if ((event.target as HTMLElement).closest("input")) return;
  const last = entries.value.length - 1;
  const current = selected.value;
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
      openEntry(current);
      break;
    case "Backspace":
    case "ArrowLeft":
      up();
      break;
    case " ":
      if (current && !current.isDir && playable(current.name)) {
        playPreview(volume.value, current.path, current.size);
      }
      break;
    default:
      return;
  }
  event.preventDefault();
}

/** A click selects a row and hands the list the keyboard; a sound also plays. */
function selectRow(entry: CardEntry, event: MouseEvent) {
  // The second click of a double-click would stop the sound the first one started.
  if (event.detail > 1) return;
  selected.value = entry;
  listEl.value?.focus({ preventScroll: true });
  if (!entry.isDir && playable(entry.name)) playPreview(volume.value, entry.path, entry.size);
}

async function onUpload() {
  try {
    const picked = await open({ multiple: true, title: "Copy to the card" });
    const locals = Array.isArray(picked) ? picked : picked ? [picked] : [];
    if (locals.length) await uploadToCard(locals, volume.value, path.value);
  } catch (e) {
    notify(errorText(e));
  }
}

async function onDownload() {
  const entry = selected.value;
  if (!entry) return;
  try {
    const into = entry.isDir
      ? await open({ directory: true, title: `Save ${entry.name} into` })
      : await save({ defaultPath: entry.name, title: `Save ${entry.name}` });
    if (typeof into === "string") await downloadFromCard(volume.value, entry.path, entry.isDir, into);
  } catch (e) {
    notify(errorText(e));
  }
}

async function onNewFolder() {
  const name = `NEW_FOLDER`;
  await createCardFolder(volume.value, path.value, name);
  // Drop straight into renaming it, so the placeholder name never sticks.
  renaming.value = path.value ? `${path.value}/${name}` : name;
  renameText.value = name;
}

function startRename(entry: CardEntry) {
  renaming.value = entry.path;
  renameText.value = entry.name;
}

async function commitRename(entry: CardEntry) {
  const name = renameText.value.trim();
  renaming.value = null;
  if (!name || name === entry.name) return;
  await renameOnCard(volume.value, entry.path, name);
  selected.value = null;
}

async function onDelete() {
  const entry = selected.value;
  if (!entry) return;
  const what = entry.isDir ? "folder" : "file";
  const warning = isProjectFile(entry)
    ? "\n\nThis belongs to the device, not to you. Deleting it can break a project."
    : "";
  const answer = await ask(
    `Delete this ${what}?`,
    `${entry.path} is removed from ${volume.value === "card" ? "the SD card" : "the device"}. ${
      !entry.isDir
        ? "This cannot be undone."
        : volume.value === "card"
          ? "Everything inside it is deleted too. This cannot be undone."
          : "Only empty folders can go."
    }${warning}`,
    [
      { label: "Cancel", value: "cancel" },
      { label: "Delete", value: "ok", kind: "danger" },
    ],
  );
  if (answer !== "ok") return;
  await deleteFromCard(volume.value, entry.path, entry.isDir);
  selected.value = null;
}

const percent = computed(() => {
  const t = store.transfer;
  return t && t.total > 0 ? Math.round((t.done / t.total) * 100) : 0;
});
</script>

<template>
  <div class="files">
    <div class="bar">
      <nav class="volumes">
        <button
          v-for="v in volumes"
          :key="v.id"
          :class="{ active: volume === v.id }"
          :disabled="store.pending > 0"
          @click="go('', v.id)"
        >
          {{ v.label }}
        </button>
      </nav>
      <button class="ghost icon" title="Up one folder" :disabled="!path" @click="up">
        <Icon name="up" />
      </button>
      <nav class="crumbs">
        <button class="crumb" :class="{ here: !path }" @click="go('')">
          {{ volume === "card" ? "SD card" : "Device" }}
        </button>
        <template v-for="c in crumbs" :key="c.path">
          <span class="sep">/</span>
          <button class="crumb" :class="{ here: c.path === path }" @click="go(c.path)">{{ c.name }}</button>
        </template>
      </nav>
      <span v-if="store.cardLoading" class="spinner" />
      <div class="spacer" />
      <span v-if="preload.running" class="muted loaded mono" title="Fetching this folder's sounds">
        Loading sounds {{ preload.done }}/{{ preload.total }}
      </span>
      <span v-if="store.card?.freeKb != null" class="muted free">{{ storage(store.card.freeKb) }} free</span>
      <button class="ghost icon" title="Reload this folder" @click="go(path)">
        <Icon name="refresh" />
      </button>
    </div>

    <div class="tools">
      <button class="primary" :disabled="store.pending > 0" @click="onUpload">
        Copy to {{ volume === "card" ? "card" : "device" }}…
      </button>
      <button :disabled="store.pending > 0" @click="onNewFolder">New folder</button>
      <button
        :class="{ active: selected && preview.playing === selected.path }"
        :disabled="!selected || selected.isDir || !playable(selected.name)"
        @click="selected && playPreview(volume, selected.path, selected.size)"
      >
        <span v-if="selected && preview.loading === selected.path" class="spinner" />
        <Icon v-else :name="selected && preview.playing === selected.path ? 'stop' : 'play'" />
        {{ selected && preview.playing === selected.path ? "Stop" : "Preview" }}
      </button>
      <div class="spacer" />
      <button :disabled="!selected || store.pending > 0" @click="onDownload">
        Save {{ selected?.isDir ? "folder" : "file" }} to computer…
      </button>
      <button :disabled="!selected || store.pending > 0" @click="selected && startRename(selected)">Rename</button>
      <button class="danger" :disabled="!selected || store.pending > 0" @click="onDelete">Delete</button>
    </div>

    <div v-if="store.transfer" class="progress">
      <div class="track"><div class="fill" :style="{ width: `${percent}%` }" /></div>
      <span class="mono">{{ store.transfer.name }} · {{ percent }}%</span>
    </div>

    <div ref="listEl" class="list panel" tabindex="0" @keydown="onKey">
      <div
        v-for="(entry, i) in entries"
        :key="entry.path"
        :data-index="i"
        class="row"
        :class="{ active: selected?.path === entry.path, playing: preview.playing === entry.path }"
        @click="selectRow(entry, $event)"
        @dblclick="entry.isDir && openEntry(entry)"
      >
        <Icon :name="entry.isDir ? 'folder' : 'file'" />
        <input
          v-if="renaming === entry.path"
          v-model="renameText"
          class="rename"
          @click.stop
          @keydown.enter="commitRename(entry)"
          @keydown.escape="renaming = null"
          @blur="commitRename(entry)"
        />
        <span v-else class="name">{{ entry.name }}</span>
        <span class="size mono muted">{{ entry.size === null ? "" : formatBytes(entry.size) }}</span>
      </div>
      <p v-if="store.card && !store.card.entries.length && !store.cardLoading" class="empty muted">
        This folder is empty.
      </p>
    </div>

    <p class="muted hint">{{ hint }}</p>
    <p class="muted hint">
      Click a sound to hear it, then use the arrow keys to hear the rest; Enter opens a folder
      and Backspace goes back. Files can also be dragged in from the file manager.
      Audio is copied as-is — use the Samples tab to put a sound straight on a pad.
    </p>
  </div>
</template>

<style scoped>
.files {
  height: 100%;
  display: flex;
  flex-direction: column;
  gap: 12px;
  padding: 14px;
  min-height: 0;
}

.bar,
.tools {
  display: flex;
  align-items: center;
  gap: 10px;
  flex-wrap: wrap;
}

.spacer {
  flex: 1;
}

.volumes {
  display: flex;
  gap: 4px;
  padding: 2px;
  border-radius: 7px;
  background: var(--panel);
}

.volumes button {
  border: none;
  background: none;
  color: var(--muted);
  padding: 4px 10px;
  font-size: 13px;
}

.volumes button.active {
  background: var(--panel-2);
  color: var(--accent);
}

.crumbs {
  display: flex;
  align-items: center;
  gap: 4px;
  flex-wrap: wrap;
}

.crumb {
  border: none;
  background: none;
  padding: 2px 6px;
  border-radius: 5px;
  color: var(--muted);
}

.crumb:hover {
  color: var(--text);
  background: var(--panel-2);
}

.crumb.here {
  color: var(--accent);
}

.sep {
  color: var(--faint);
}

.free {
  font-size: 12px;
}

.progress {
  display: flex;
  align-items: center;
  gap: 10px;
  font-size: 12px;
}

.track {
  flex: 1;
  height: 5px;
  border-radius: 3px;
  background: var(--line-strong);
  overflow: hidden;
}

.fill {
  height: 100%;
  background: var(--accent);
  transition: width 0.1s linear;
}

.list {
  flex: 1;
  min-height: 0;
  overflow: auto;
  padding: 4px;
}

.row {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 6px 10px;
  border-radius: 6px;
  cursor: default;
  color: var(--muted);
}

.row:hover {
  background: var(--panel-2);
}

.row.active {
  background: var(--accent-soft);
  color: var(--text);
}

.row.playing .name {
  color: var(--accent);
}

.list:focus {
  outline: none;
  border-color: var(--accent-line);
}

.loaded {
  font-size: 12px;
}

button.active {
  border-color: var(--accent);
  color: var(--accent);
}

.name {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--text);
}

.rename {
  flex: 1;
  min-width: 0;
  font: inherit;
  color: var(--text);
  background: #0f1013;
  border: 1px solid var(--accent-line);
  border-radius: 5px;
  padding: 2px 6px;
  outline: none;
}

.size {
  font-size: 12px;
}

.empty {
  padding: 14px;
  font-size: 13px;
}

.hint {
  margin: 0;
  font-size: 12px;
}
</style>
