<script setup lang="ts">
import { computed, ref, watch } from "vue";
import BankStrip from "../components/BankStrip.vue";
import NoteLanes from "../components/NoteLanes.vue";
import PadGrid from "../components/PadGrid.vue";
import { api, type PatternDetail } from "../api";
import { bpm, padLabel } from "../format";
import { handleError, loadPatterns, store } from "../store";

const counts = computed(() => {
  const c = new Array(10).fill(0);
  store.patterns.forEach((exists, i) => exists && c[Math.floor(i / 16)]++);
  return c;
});

const detail = ref<PatternDetail | null>(null);
const loading = ref(false);
let request = 0;

if (!store.patterns.length && !store.patternsLoading) loadPatterns();

const bars = computed(() => {
  const d = detail.value;
  if (!d) return "";
  const perBar = d.ppq * (d.beatsPerBar ?? 4);
  const value = d.lengthTicks / perBar;
  return Number.isInteger(value) ? String(value) : value.toFixed(2);
});

watch(
  () => [store.selectedPattern, store.generation, store.patterns] as const,
  async ([slot]) => {
    const token = ++request;
    detail.value = null;
    if (slot === null || !store.patterns[slot]) return;
    loading.value = true;
    try {
      const d = await api.patternDetail(slot);
      if (token === request) detail.value = d;
    } catch (e) {
      if (token === request) handleError(e);
    } finally {
      if (token === request) loading.value = false;
    }
  },
  { immediate: true },
);
</script>

<template>
  <div class="patterns">
    <BankStrip :counts="counts" :loading="store.patternsLoading" />

    <div class="body">
      <section>
        <PadGrid
          :bank="store.bank"
          :selected="store.selectedPattern"
          :filled="(i) => !!store.patterns[i]"
          accent="teal"
          @select="store.selectedPattern = $event"
        >
          <template #default="{ index }">
            <span v-if="store.patterns[index]" class="slot-name">Pattern</span>
          </template>
        </PadGrid>
      </section>

      <section class="detail panel">
        <div v-if="store.selectedPattern === null" class="placeholder muted">Select a pattern slot.</div>
        <div v-else-if="!store.patterns[store.selectedPattern]" class="placeholder muted">
          {{ padLabel(store.selectedPattern) }} holds no pattern.
        </div>
        <div v-else-if="loading" class="placeholder muted"><span class="spinner" /> Reading pattern…</div>

        <template v-else-if="detail">
          <header class="head">
            <span class="slot-label mono">{{ detail.label }}</span>
            <div class="facts">
              <div><span class="label">Length</span><span class="mono">{{ bars }} bars</span></div>
              <div><span class="label">Time</span><span class="mono">{{ detail.beatsPerBar ?? "?" }}/4</span></div>
              <div><span class="label">Bank tempo</span><span class="mono">{{ bpm(detail.bankTempo) }}</span></div>
              <div><span class="label">Notes</span><span class="mono">{{ detail.notes.length }}</span></div>
              <div v-if="detail.controlEvents"><span class="label">Controls</span><span class="mono">{{ detail.controlEvents }}</span></div>
            </div>
          </header>

          <div>
            <div class="label">Pads played</div>
            <div class="chips">
              <span v-for="p in detail.padsUsed" :key="p" class="chip mono">{{ padLabel(p) }}</span>
              <span v-if="!detail.padsUsed.length" class="muted">None</span>
            </div>
          </div>

          <NoteLanes v-if="detail.padsUsed.length" :pattern="detail" />
        </template>
      </section>
    </div>
  </div>
</template>

<style scoped>
.patterns {
  height: 100%;
  display: flex;
  flex-direction: column;
  gap: 14px;
  padding: 14px;
  min-height: 0;
}

.body {
  flex: 1;
  min-height: 0;
  display: grid;
  grid-template-columns: minmax(360px, 440px) 1fr;
  gap: 14px;
}

.slot-name {
  font-size: 12px;
  color: var(--teal);
}

.detail {
  min-height: 0;
  overflow: auto;
  padding: 16px;
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.detail > * {
  flex-shrink: 0;
}

.placeholder {
  margin: auto;
  display: flex;
  align-items: center;
  gap: 10px;
}

.head {
  display: flex;
  align-items: center;
  gap: 22px;
}

.slot-label {
  font-size: 22px;
  font-weight: 600;
  color: var(--teal);
}

.facts {
  display: flex;
  gap: 24px;
}

.facts div {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.chips {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  margin-top: 6px;
}

.chip {
  padding: 2px 8px;
  border-radius: 999px;
  font-size: 12px;
  background: var(--teal-soft);
  color: var(--teal);
  border: 1px solid rgba(69, 212, 184, 0.35);
}
</style>
