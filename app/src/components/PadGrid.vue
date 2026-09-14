<script setup lang="ts">
import { PAD_ROWS, padIndex } from "../format";

const props = defineProps<{
  bank: number;
  selected: number | null;
  /** Whether the slot at a pad index is in use. */
  filled: (index: number) => boolean;
  accent?: "amber" | "teal";
}>();

const emit = defineEmits<{ select: [index: number] }>();
</script>

<template>
  <div class="grid" :class="props.accent ?? 'amber'">
    <template v-for="row in PAD_ROWS" :key="row[0]">
      <button
        v-for="n in row"
        :key="n"
        class="pad"
        :class="{
          filled: props.filled(padIndex(props.bank, n)),
          selected: props.selected === padIndex(props.bank, n),
        }"
        @click="emit('select', padIndex(props.bank, n))"
      >
        <span class="num mono">{{ n }}</span>
        <slot :index="padIndex(props.bank, n)" :number="n" />
      </button>
    </template>
  </div>
</template>

<style scoped>
.grid {
  --tone: var(--accent);
  --tone-soft: var(--accent-soft);
  --tone-line: var(--accent-line);
  display: grid;
  grid-template-columns: repeat(4, 1fr);
  gap: 8px;
}

.grid.teal {
  --tone: var(--teal);
  --tone-soft: var(--teal-soft);
  --tone-line: rgba(69, 212, 184, 0.55);
}

.pad {
  position: relative;
  aspect-ratio: 1;
  flex-direction: column;
  align-items: stretch;
  justify-content: flex-end;
  gap: 2px;
  padding: 8px;
  text-align: left;
  border-radius: 8px;
  border: 1px dashed var(--line-strong);
  background: transparent;
  overflow: hidden;
}

.pad.filled {
  border-style: solid;
  border-color: var(--line-strong);
  background: linear-gradient(160deg, var(--panel-2), var(--panel));
  box-shadow: inset 3px 0 0 var(--tone);
}

.pad.selected {
  border-color: var(--tone);
  background: var(--tone-soft);
  box-shadow:
    inset 3px 0 0 var(--tone),
    0 0 0 1px var(--tone-line);
}

.pad:not(.filled).selected {
  box-shadow: 0 0 0 1px var(--tone-line);
}

.num {
  position: absolute;
  top: 7px;
  left: 9px;
  font-size: 11px;
  color: var(--faint);
}

.pad.filled .num {
  color: var(--muted);
}
</style>
