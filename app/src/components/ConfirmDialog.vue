<script setup lang="ts">
import { onBeforeUnmount, onMounted } from "vue";
import { closeDialog, store } from "../store";

function onKey(e: KeyboardEvent) {
  if (store.dialog && e.key === "Escape") closeDialog(null);
}
onMounted(() => window.addEventListener("keydown", onKey));
onBeforeUnmount(() => window.removeEventListener("keydown", onKey));
</script>

<template>
  <div v-if="store.dialog" class="backdrop" @click.self="closeDialog(null)">
    <div class="dialog panel" role="dialog" aria-modal="true">
      <h2>{{ store.dialog.title }}</h2>
      <p>{{ store.dialog.message }}</p>
      <div class="buttons">
        <button
          v-for="b in store.dialog.buttons"
          :key="b.value"
          :class="b.kind"
          @click="closeDialog(b.value)"
        >
          {{ b.label }}
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.backdrop {
  position: fixed;
  inset: 0;
  z-index: 20;
  display: grid;
  place-items: center;
  background: rgba(5, 6, 8, 0.6);
}

.dialog {
  width: 430px;
  padding: 20px;
  box-shadow: 0 20px 60px rgba(0, 0, 0, 0.5);
}

h2 {
  margin: 0 0 8px;
  font-size: 16px;
}

p {
  margin: 0;
  color: var(--muted);
}

.buttons {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  margin-top: 18px;
}
</style>
