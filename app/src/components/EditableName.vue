<script setup lang="ts">
import { nextTick, ref } from "vue";

const props = defineProps<{ value: string; maxLength: number; disabled?: boolean; placeholder?: string }>();
const emit = defineEmits<{ commit: [name: string] }>();

const editing = ref(false);
const text = ref("");
const input = ref<HTMLInputElement | null>(null);

async function start() {
  if (props.disabled) return;
  text.value = props.value;
  editing.value = true;
  await nextTick();
  input.value?.select();
}

function finish(save: boolean) {
  if (!editing.value) return;
  editing.value = false;
  const name = text.value.trim();
  if (save && name && name !== props.value) emit("commit", name);
}
</script>

<template>
  <input
    v-if="editing"
    ref="input"
    v-model="text"
    class="edit"
    :maxlength="maxLength"
    @keydown.enter="finish(true)"
    @keydown.escape="finish(false)"
    @blur="finish(true)"
  />
  <button v-else class="name-button" :disabled="disabled" title="Rename" @click="start">
    <span class="text">{{ value || placeholder }}</span>
    <svg v-if="!disabled" class="pencil" width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M4 20h4L19 9l-4-4L4 16v4zM14 6l4 4" /></svg>
  </button>
</template>

<style scoped>
.name-button {
  border: none;
  background: transparent;
  padding: 2px 4px;
  margin-left: -4px;
  font: inherit;
  max-width: 100%;
  min-width: 0;
}

.name-button:hover:not(:disabled) {
  background: var(--panel-2);
  border: none;
}

.name-button:disabled {
  opacity: 1;
}

.text {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.pencil {
  flex: none;
  color: var(--faint);
}

.name-button:hover .pencil {
  color: var(--muted);
}

.edit {
  font: inherit;
  width: 100%;
  max-width: 320px;
}
</style>
