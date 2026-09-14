<script setup lang="ts">
import Icon from "./Icon.vue";
import { dismiss, store } from "../store";
</script>

<template>
  <div class="toasts">
    <div v-for="t in store.toasts" :key="t.id" class="toast">
      <span>{{ t.text }}</span>
      <button class="ghost" title="Dismiss" @click="dismiss(t.id)"><Icon name="close" :size="14" /></button>
    </div>
  </div>
</template>

<style scoped>
.toasts {
  position: fixed;
  right: 16px;
  bottom: 16px;
  display: flex;
  flex-direction: column;
  gap: 8px;
  z-index: 10;
  max-width: 420px;
}

.toast {
  display: flex;
  align-items: flex-start;
  gap: 10px;
  padding: 10px 8px 10px 14px;
  border-radius: 8px;
  background: #2a1d1f;
  border: 1px solid rgba(255, 107, 107, 0.45);
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.4);
}

.toast span {
  flex: 1;
  padding-top: 3px;
}

.toast button {
  padding: 4px;
}
</style>
