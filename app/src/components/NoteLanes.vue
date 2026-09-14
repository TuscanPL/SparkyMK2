<script setup lang="ts">
import { computed } from "vue";
import type { PatternDetail } from "../api";
import { padLabel } from "../format";

const props = defineProps<{ pattern: PatternDetail }>();

const ROW = 22;
const LABEL = 44;

const beatsPerBar = computed(() => props.pattern.beatsPerBar ?? 4);
const ticksPerBar = computed(() => props.pattern.ppq * beatsPerBar.value);
const length = computed(() => Math.max(props.pattern.lengthTicks, 1));
const bars = computed(() => Math.ceil(length.value / ticksPerBar.value));
const rows = computed(() => props.pattern.padsUsed);
const height = computed(() => rows.value.length * ROW);
const x = (tick: number) => (tick / length.value) * 1000;
</script>

<template>
  <div class="lanes">
    <div class="labels" :style="{ width: `${LABEL}px` }">
      <div v-for="pad in rows" :key="pad" class="row-label mono" :style="{ height: `${ROW}px` }">
        {{ padLabel(pad) }}
      </div>
    </div>
    <svg :viewBox="`0 0 1000 ${height}`" preserveAspectRatio="none" :style="{ height: `${height}px` }">
      <rect
        v-for="(pad, r) in rows"
        :key="`row${pad}`"
        x="0"
        :y="r * ROW"
        width="1000"
        :height="ROW"
        :class="r % 2 ? 'stripe' : 'stripe alt'"
      />
      <template v-for="b in bars" :key="`bar${b}`">
        <line
          v-for="beat in beatsPerBar"
          :key="`b${b}-${beat}`"
          :x1="x((b - 1) * ticksPerBar + (beat - 1) * pattern.ppq)"
          :x2="x((b - 1) * ticksPerBar + (beat - 1) * pattern.ppq)"
          y1="0"
          :y2="height"
          :class="beat === 1 ? 'bar' : 'beat'"
          vector-effect="non-scaling-stroke"
        />
      </template>
      <rect
        v-for="(n, i) in pattern.notes"
        :key="i"
        :x="x(n.tick)"
        :y="rows.indexOf(n.pad) * ROW + 4"
        :width="Math.max(2, x(n.length))"
        :height="ROW - 8"
        rx="2"
        class="note"
        :style="{ opacity: 0.35 + (n.velocity / 127) * 0.65 }"
      />
    </svg>
  </div>
</template>

<style scoped>
.lanes {
  display: flex;
  border: 1px solid var(--line);
  border-radius: 8px;
  overflow: hidden;
  background: #0d0e11;
}

.labels {
  flex: none;
  border-right: 1px solid var(--line);
}

.row-label {
  display: flex;
  align-items: center;
  padding-left: 8px;
  font-size: 11px;
  color: var(--muted);
}

svg {
  flex: 1;
  display: block;
  min-width: 0;
}

.stripe {
  fill: transparent;
}

.stripe.alt {
  fill: rgba(255, 255, 255, 0.02);
}

.bar {
  stroke: var(--line-strong);
  stroke-width: 1;
}

.beat {
  stroke: var(--line);
  stroke-width: 1;
}

.note {
  fill: var(--teal);
}
</style>
