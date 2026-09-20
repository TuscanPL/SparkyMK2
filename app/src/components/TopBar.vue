<script setup lang="ts">
import { computed } from "vue";
import Icon from "./Icon.vue";
import logo from "../assets/sparkymk2.png";
import { bpm, storage } from "../format";
import { disconnect, refresh, selectProject, store, type Tab } from "../store";

const tabs: { id: Tab; label: string }[] = [
  { id: "samples", label: "Samples" },
  { id: "patterns", label: "Patterns" },
  { id: "screens", label: "Screens" },
  { id: "files", label: "Files" },
  { id: "settings", label: "Settings" },
];

const tempo = computed(() => {
  const s = store.status;
  if (!s) return "";
  return s.usesProjectTempo ? `${bpm(s.projectTempo)} BPM` : "Per bank";
});

function onProject(event: Event) {
  selectProject(Number((event.target as HTMLSelectElement).value));
}
</script>

<template>
  <header class="topbar">
    <div class="brand">
      <img class="logo" :src="logo" alt="" width="26" height="26" />
      <span>SparkyMK2</span>
    </div>

    <nav v-if="store.connected" class="tabs">
      <button
        v-for="t in tabs"
        :key="t.id"
        :class="{ active: store.tab === t.id }"
        @click="store.tab = t.id"
      >
        {{ t.label }}
      </button>
    </nav>

    <div class="spacer" />

    <template v-if="store.connected && store.status">
      <label class="project">
        <span class="label">Project</span>
        <select :value="store.status.project" :disabled="store.switchingProject" @change="onProject">
          <option v-for="(name, i) in store.projects" :key="i" :value="i + 1">
            {{ String(i + 1).padStart(2, "0") }} · {{ name || "Unnamed" }}
          </option>
        </select>
        <span v-if="store.switchingProject" class="spinner" />
      </label>

      <div class="readout">
        <span class="label">Tempo</span>
        <span class="mono">{{ tempo }}</span>
      </div>
      <div class="readout free">
        <span class="label">Free</span>
        <span class="mono">{{ storage(store.status.freeKb) }}</span>
      </div>

      <span v-if="store.pending" class="saving muted"><span class="spinner" /> Saving</span>
      <button class="ghost icon" title="Reload from the device" @click="refresh">
        <Icon name="refresh" />
      </button>
      <div class="device">
        <span class="dot" />
        <span class="mono">{{ store.status.port }}</span>
        <button class="ghost small" @click="disconnect">Disconnect</button>
      </div>
    </template>
  </header>
</template>

<style scoped>
.topbar {
  height: 52px;
  display: flex;
  align-items: center;
  gap: 16px;
  padding: 0 14px;
  white-space: nowrap;
  overflow: hidden;
  border-bottom: 1px solid var(--line);
  background: #14161a;
}

.brand {
  display: flex;
  align-items: center;
  gap: 8px;
  font-weight: 650;
  letter-spacing: 0.01em;
}

.logo {
  width: 26px;
  height: 26px;
  display: block;
}

.tabs {
  display: flex;
  gap: 2px;
  padding: 3px;
  background: var(--panel);
  border: 1px solid var(--line);
  border-radius: 8px;
}

.tabs button {
  border: none;
  background: transparent;
  color: var(--muted);
  padding: 5px 14px;
}

.tabs button.active {
  background: var(--panel-2);
  color: var(--text);
  box-shadow: inset 0 -2px 0 var(--accent);
}

.spacer {
  flex: 1;
}

.project {
  display: flex;
  align-items: center;
  gap: 8px;
}

.project select {
  width: 200px;
  text-overflow: ellipsis;
}

.readout {
  display: flex;
  flex-direction: column;
  line-height: 1.15;
}

.icon {
  padding: 6px;
}

.saving {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 12.5px;
}

.device {
  display: flex;
  align-items: center;
  gap: 8px;
  padding-left: 12px;
  border-left: 1px solid var(--line);
  color: var(--muted);
}

.dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: var(--ok);
  box-shadow: 0 0 8px rgba(95, 211, 138, 0.6);
}

.small {
  padding: 4px 8px;
  font-size: 12px;
}

/* Tiling window managers often open the window narrower than its minimum size. */
@media (max-width: 1180px) {
  .device .mono,
  .project .label {
    display: none;
  }
}

@media (max-width: 1000px) {
  .readout.free {
    display: none;
  }

  .project select {
    width: 160px;
  }
}
</style>
