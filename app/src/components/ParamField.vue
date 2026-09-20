<script setup lang="ts">
import { computed, ref, watch } from "vue";
import type { Param } from "../api";
import { FLAG, PARAM_VIEW, bpm } from "../format";

const props = defineProps<{ param: Param; disabled?: boolean }>();
const emit = defineEmits<{ commit: [value: number] }>();

const view = computed(() => PARAM_VIEW[props.param.name]);
const control = computed(() => view.value?.control ?? { kind: "slider" as const, min: props.param.min, max: props.param.max });

// Value shown while a slider is held or a number is typed; sent on release or Enter.
const draft = ref(props.param.value);
const bpmText = ref(bpm(props.param.value));
// Follow the device's answer: every read-back replaces the param object, even when the
// value is unchanged, so a draft that didn't take snaps back.
watch(
  () => props.param,
  (p) => {
    draft.value = p.value;
    bpmText.value = bpm(p.value);
  },
);

function commit(value: number) {
  if (value !== props.param.value) emit("commit", value);
}

function commitBpm() {
  const value = Math.round(parseFloat(bpmText.value) * 100);
  if (!Number.isFinite(value) || value < 4000 || value > 20000) {
    bpmText.value = bpm(props.param.value);
    return;
  }
  commit(value);
}

const chromatic = computed(() => props.param.value & (FLAG.chromaticA | FLAG.chromaticB));
function setFlag(bit: number, on: boolean) {
  commit(on ? props.param.value | bit : props.param.value & ~bit);
}
function setChromatic(bits: number) {
  commit((props.param.value & ~(FLAG.chromaticA | FLAG.chromaticB)) | bits);
}
</script>

<template>
  <!-- The trigger flags are two rows of their own, so every label lines up on the left
       and every control ends at the right edge. -->
  <template v-if="control.kind === 'flags'">
    <div class="field">
      <span class="name">{{ view?.label ?? param.name }}</span>
      <div class="checks">
        <label class="check">
          <input
            type="checkbox"
            :checked="!!(param.value & FLAG.oneShot)"
            :disabled="disabled"
            @change="setFlag(FLAG.oneShot, ($event.target as HTMLInputElement).checked)"
          />
          One shot
        </label>
        <label class="check">
          <input
            type="checkbox"
            :checked="!!(param.value & FLAG.fixedVelocity)"
            :disabled="disabled"
            @change="setFlag(FLAG.fixedVelocity, ($event.target as HTMLInputElement).checked)"
          />
          Fixed velocity
        </label>
      </div>
    </div>
    <div class="field">
      <span class="name">Chromatic</span>
      <select
        :value="chromatic"
        :disabled="disabled"
        @change="setChromatic(Number(($event.target as HTMLSelectElement).value))"
      >
        <option :value="0">Mono</option>
        <option :value="FLAG.chromaticA">Mode 2</option>
        <option :value="FLAG.chromaticB">Mode 3</option>
      </select>
    </div>
  </template>

  <div v-else class="field" :class="`kind-${control.kind}`">
    <span class="name">{{ view?.label ?? param.name }}</span>

    <label v-if="control.kind === 'toggle'" class="switch">
      <input
        type="checkbox"
        :checked="!!param.value"
        :disabled="disabled"
        @change="commit(($event.target as HTMLInputElement).checked ? 1 : 0)"
      />
      <span />
    </label>

    <select
      v-else-if="control.kind === 'select'"
      :value="param.value"
      :disabled="disabled"
      @change="commit(Number(($event.target as HTMLSelectElement).value))"
    >
      <option v-for="[value, label] in control.options" :key="value" :value="value">{{ label }}</option>
    </select>

    <div v-else-if="control.kind === 'slider'" class="slider">
      <input
        type="range"
        :min="control.min"
        :max="control.max"
        :step="control.step ?? 1"
        :value="draft"
        :disabled="disabled"
        @input="draft = Number(($event.target as HTMLInputElement).value)"
        @change="commit(draft)"
      />
      <span class="value mono">{{ view?.show(draft) ?? draft }}</span>
    </div>

    <input
      v-else-if="control.kind === 'bpm'"
      v-model="bpmText"
      class="bpm mono"
      inputmode="decimal"
      :disabled="disabled"
      @keydown.enter="($event.target as HTMLInputElement).blur()"
      @blur="commitBpm"
    />

  </div>
</template>

<style scoped>
.field {
  display: grid;
  grid-template-columns: 96px 1fr;
  align-items: center;
  gap: 10px;
  min-height: 30px;
}

.name {
  color: var(--muted);
}

select,
.bpm {
  justify-self: end;
  min-width: 0;
  max-width: 100%;
}

.bpm {
  width: 90px;
  text-align: right;
}

.switch {
  justify-self: end;
}

.slider {
  display: flex;
  align-items: center;
  gap: 10px;
  min-width: 0;
}

.slider input {
  flex: 1;
  min-width: 0;
}

.value {
  width: 58px;
  text-align: right;
  font-size: 12.5px;
}

.checks {
  /* One line, hugging the right edge like the other controls. The parameter groups are
     wide enough to hold both at any window size. */
  justify-self: end;
  display: flex;
  gap: 16px;
}

.check {
  display: flex;
  align-items: center;
  gap: 6px;
  /* Keep "One shot" and "Fixed velocity" whole; the pair wraps instead. */
  white-space: nowrap;
}
</style>
