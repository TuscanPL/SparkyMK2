<script setup lang="ts">
import { onMounted } from "vue";
import Icon from "./Icon.vue";
import { connect, scanPorts, store } from "../store";

onMounted(scanPorts);
</script>

<template>
  <div class="connect">
    <div class="card panel">
      <div class="badge"><Icon name="usb" :size="22" /></div>
      <h1>Connect your SP-404MKII</h1>
      <p class="muted">Plug the sampler in over USB. SparkyMK2 finds it by its USB ID.</p>

      <div class="ports">
        <div v-for="p in store.ports" :key="p.name" class="port">
          <div>
            <div class="mono">{{ p.name }}</div>
            <div class="muted small">{{ p.product ?? "SP-404MKII" }}</div>
          </div>
          <button class="primary" :disabled="store.connecting" @click="connect(p.name)">
            <span v-if="store.connecting" class="spinner" />
            Connect
          </button>
        </div>
        <div v-if="!store.ports.length && !store.scanning" class="empty">
          No SP-404MKII found.
        </div>
      </div>

      <button :disabled="store.scanning" @click="scanPorts">
        <span v-if="store.scanning" class="spinner" />
        <Icon v-else name="refresh" />
        Scan again
      </button>

      <ul class="hints muted">
        <li>Only one program can use the device at a time. Close the official app and the <span class="mono">sp404</span> tool first.</li>
        <li>On Linux your user needs serial port access: the <span class="mono">uucp</span> group on Arch, <span class="mono">dialout</span> elsewhere.</li>
      </ul>
    </div>
  </div>
</template>

<style scoped>
.connect {
  height: 100%;
  display: grid;
  place-items: center;
  padding: 24px;
  background: radial-gradient(1200px 500px at 50% -10%, rgba(255, 173, 51, 0.07), transparent);
}

.card {
  width: 460px;
  padding: 28px;
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 14px;
}

.badge {
  width: 44px;
  height: 44px;
  border-radius: 10px;
  display: grid;
  place-items: center;
  color: var(--accent);
  background: var(--accent-soft);
  border: 1px solid var(--accent-line);
}

h1 {
  margin: 0;
  font-size: 20px;
  font-weight: 650;
}

p {
  margin: 0;
}

.ports {
  width: 100%;
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.port {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 10px 12px;
  border: 1px solid var(--line-strong);
  border-radius: 8px;
  background: var(--panel-2);
}

.empty {
  padding: 14px;
  border: 1px dashed var(--line-strong);
  border-radius: 8px;
  color: var(--muted);
  text-align: center;
}

.small {
  font-size: 12px;
}

.hints {
  margin: 4px 0 0;
  padding-left: 18px;
  font-size: 12.5px;
  display: flex;
  flex-direction: column;
  gap: 6px;
}
</style>
