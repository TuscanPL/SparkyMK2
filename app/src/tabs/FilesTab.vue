<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { open, save } from "@tauri-apps/plugin-dialog";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import Icon from "../components/Icon.vue";
import { errorText, type CardEntry, type Transfer, type Volume } from "../api";
import { bytes as formatBytes, storage } from "../format";
import {
  ask,
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
const renaming = ref<string | null>(null);
const renameText = ref("");
let unlisten: UnlistenFn | undefined;

/** The card first: its IMPORT folder is what the device's own IMPORT browser reads. */
const volumes: { id: Volume; label: string; hint: string }[] = [
  { id: "card", label: "SD card", hint: "IMPORT, EXPORT and BKUP. Sounds dropped in IMPORT show up in the device's own IMPORT browser." },
  { id: "internal", label: "Device storage", hint: "The device's own memory: projects, samples and the factory library. Changing these can break a project." },
];

onMounted(async () => {
  unlisten = await listen<Transfer>("transfer", (e) => (store.transfer = e.payload));
  if (!store.card) await loadCard("card", "");
});

onUnmounted(() => unlisten?.());

const volume = computed<Volume>(() => store.card?.volume ?? "card");
const path = computed(() => store.card?.path ?? "");
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

async function go(to: string, to_volume: Volume = volume.value) {
  selected.value = null;
  renaming.value = null;
  await loadCard(to_volume, to);
}

function up() {
  const i = path.value.lastIndexOf("/");
  go(i < 0 ? "" : path.value.slice(0, i));
}

function openEntry(entry: CardEntry) {
  if (entry.isDir) go(entry.path);
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
    `${entry.path} is removed from the card. ${
      entry.isDir ? "Only empty folders can go." : "This cannot be undone."
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

    <div class="list panel">
      <div
        v-for="entry in store.card?.entries ?? []"
        :key="entry.path"
        class="row"
        :class="{ active: selected?.path === entry.path }"
        @click="selected = entry"
        @dblclick="openEntry(entry)"
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
        <span class="size mono muted">{{ entry.isDir ? "" : formatBytes(entry.size) }}</span>
      </div>
      <p v-if="store.card && !store.card.entries.length && !store.cardLoading" class="empty muted">
        This folder is empty.
      </p>
    </div>

    <p class="muted hint">{{ hint }}</p>
    <p class="muted hint">
      Double-click a folder to open it. Files can also be dragged in from the file manager.
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
