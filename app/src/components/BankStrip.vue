<script setup lang="ts">
import Icon from "./Icon.vue";
import { BANK_LETTERS, bpm } from "../format";
import { chooseBank, store } from "../store";

const props = defineProps<{
  /** Items in use per bank (samples or patterns). */
  counts: number[];
  loading?: boolean;
}>();

const tempo = (bank: number) => {
  const b = store.status?.banks[bank];
  return b ? bpm(b.tempo) : "";
};
</script>

<template>
  <div class="banks">
    <button
      v-for="(letter, i) in BANK_LETTERS"
      :key="letter"
      class="bank"
      :data-bank="i"
      :class="{ active: store.bank === i }"
      @click="chooseBank(i)"
    >
      <span class="top">
        <span class="letter">{{ letter }}</span>
        <Icon v-if="store.status?.banks[i]?.protected" name="lock" :size="12" />
      </span>
      <span class="tempo mono">{{ tempo(i) }}</span>
      <span class="fill">
        <span :style="{ width: `${((props.counts[i] ?? 0) / 16) * 100}%` }" />
      </span>
    </button>
    <span v-if="loading" class="spinner" />
  </div>
</template>

<style scoped>
.banks {
  display: flex;
  align-items: center;
  gap: 6px;
}

.bank {
  flex: 1;
  min-width: 0;
  flex-direction: column;
  align-items: stretch;
  gap: 2px;
  padding: 7px 9px 8px;
  background: var(--panel);
  border-color: var(--line);
}

.bank.active {
  border-color: var(--accent-line);
  background: var(--accent-soft);
}

.top {
  display: flex;
  align-items: center;
  justify-content: space-between;
  color: var(--muted);
}

.letter {
  font-weight: 650;
  font-size: 15px;
  color: var(--text);
}

.bank.active .letter {
  color: var(--accent);
}

.tempo {
  font-size: 11px;
  color: var(--muted);
  text-align: left;
}

.fill {
  height: 3px;
  border-radius: 2px;
  background: var(--line);
  overflow: hidden;
}

.fill span {
  display: block;
  height: 100%;
  background: var(--accent);
  opacity: 0.8;
}
</style>
