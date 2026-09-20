<script setup lang="ts">
// A small view of the SD card for the Samples tab: walk into folders, audition a sound,
// and drag it onto a pad. It keeps its own listing so it does not fight with the Files
// tab, which owns `store.card`.
import { onMounted, ref } from "vue";
import Icon from "./Icon.vue";
import { api, errorText, type CardEntry } from "../api";
import { clickAfterDrag, drag, pressSound } from "../drag";
import { playPreview, playable, preview } from "../preview";
import { notify } from "../store";

const path = ref("");
const entries = ref<CardEntry[]>([]);
const loading = ref(false);

async function go(to: string) {
  loading.value = true;
  try {
    const listing = await api.listVolume("card", to);
    path.value = listing.path;
    entries.value = listing.entries.filter((e) => e.isDir || playable(e.name));
  } catch (e) {
    notify(errorText(e));
  } finally {
    loading.value = false;
  }
}

onMounted(() => go("IMPORT"));

function up() {
  const i = path.value.lastIndexOf("/");
  go(i < 0 ? "" : path.value.slice(0, i));
}

function onRow(entry: CardEntry) {
  // A drag that ended on a pad also lands a click here; that is not a request to play.
  if (clickAfterDrag()) return;
  if (entry.isDir) go(entry.path);
  else playPreview("card", entry.path);
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
      <button class="ghost icon" title="Reload" :disabled="loading" @click="go(path)">
        <Icon name="refresh" />
      </button>
    </div>

    <div class="rows">
      <div
        v-for="entry in entries"
        :key="entry.path"
        class="row"
        :class="{ sound: !entry.isDir, playing: preview.playing === entry.path }"
        @pointerdown="!entry.isDir && pressSound($event, { volume: 'card', path: entry.path, name: entry.name })"
        @click="onRow(entry)"
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
          : "Click a sound to hear it, drag it onto a pad to import it."
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
