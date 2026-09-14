<script setup lang="ts">
import { watch } from "vue";
import ConnectScreen from "./components/ConnectScreen.vue";
import Toasts from "./components/Toasts.vue";
import TopBar from "./components/TopBar.vue";
import PatternsTab from "./tabs/PatternsTab.vue";
import SamplesTab from "./tabs/SamplesTab.vue";
import SettingsTab from "./tabs/SettingsTab.vue";
import { loadPatterns, resume, store } from "./store";

resume();

// Pattern slots take 160 requests, so they load when the tab is first opened.
watch(
  () => store.tab,
  (tab) => {
    if (tab === "patterns" && store.connected && !store.patterns.length && !store.patternsLoading) loadPatterns();
  },
);
</script>

<template>
  <div class="app">
    <TopBar />
    <div v-if="store.connected && store.status?.workingMode === 4" class="banner">
      The SP-404MKII is showing a menu. Leave it on the device before editing from here.
    </div>
    <main>
      <ConnectScreen v-if="!store.connected" />
      <SamplesTab v-else-if="store.tab === 'samples'" />
      <PatternsTab v-else-if="store.tab === 'patterns'" />
      <SettingsTab v-else />
    </main>
    <Toasts />
  </div>
</template>

<style scoped>
.app {
  height: 100%;
  display: flex;
  flex-direction: column;
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
</style>
