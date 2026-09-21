<script setup lang="ts">
import { onMounted, watch } from "vue";
import { listen } from "@tauri-apps/api/event";
import type { Transfer } from "./api";
import ConfirmDialog from "./components/ConfirmDialog.vue";
import ConnectScreen from "./components/ConnectScreen.vue";
import Toasts from "./components/Toasts.vue";
import TopBar from "./components/TopBar.vue";
import PatternsTab from "./tabs/PatternsTab.vue";
import FilesTab from "./tabs/FilesTab.vue";
import SamplesTab from "./tabs/SamplesTab.vue";
import ScreensTab from "./tabs/ScreensTab.vue";
import SettingsTab from "./tabs/SettingsTab.vue";
import { drag, listenForFileDrops } from "./drag";
import { padLabel } from "./format";
import { loadPatterns, loadScreens, resume, store } from "./store";

resume();
onMounted(listenForFileDrops);
// Copies, exports and restores all report progress this way, from whichever tab ran them.
onMounted(() => listen<Transfer>("transfer", (e) => (store.transfer = e.payload)));

// Pattern slots take 160 requests, so they load when the tab is first opened. The
// display images are read from the card the same way.
watch(
  () => store.tab,
  (tab) => {
    if (!store.connected) return;
    if (tab === "patterns" && !store.patterns.length && !store.patternsLoading) loadPatterns();
    if (tab === "screens" && !store.screens.length && !store.screensLoading) loadScreens();
  },
);
</script>

<template>
  <div class="app" :class="{ 'is-dragging': drag.from >= 0 || !!drag.sound }">
    <TopBar />
    <div v-if="store.connected && store.status?.workingMode === 4" class="banner">
      The SP-404MKII is showing a menu. Leave it on the device to edit from here.
    </div>
    <main>
      <ConnectScreen v-if="!store.connected" />
      <SamplesTab v-else-if="store.tab === 'samples'" />
      <PatternsTab v-else-if="store.tab === 'patterns'" />
      <ScreensTab v-else-if="store.tab === 'screens'" />
      <FilesTab v-else-if="store.tab === 'files'" />
      <SettingsTab v-else />
    </main>
    <div v-if="drag.from >= 0" class="ghost" :style="{ left: `${drag.x + 12}px`, top: `${drag.y + 12}px` }">
      <span class="mono">{{ padLabel(drag.from) }}</span>
      {{ store.pads[drag.from]?.name }}
    </div>
    <div v-else-if="drag.sound" class="ghost" :style="{ left: `${drag.x + 12}px`, top: `${drag.y + 12}px` }">
      {{ drag.sound.name }}
    </div>
    <ConfirmDialog />
    <Toasts />
  </div>
</template>

<style scoped>
.app {
  height: 100%;
  display: flex;
  flex-direction: column;
}

.app.is-dragging {
  cursor: grabbing;
}

main {
  flex: 1;
  min-height: 0;
}

.banner {
  padding: 7px 14px;
  font-size: 13px;
  color: #ffd699;
  background: rgba(255, 173, 51, 0.1);
  border-bottom: 1px solid var(--accent-line);
}

.ghost {
  position: fixed;
  z-index: 30;
  pointer-events: none;
  display: flex;
  gap: 8px;
  padding: 6px 10px;
  border-radius: 6px;
  background: var(--panel-2);
  border: 1px solid var(--accent);
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.45);
  max-width: 260px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.ghost .mono {
  color: var(--accent);
}
</style>
