<script setup lang="ts">
import Icon from "../components/Icon.vue";
import { bpm, storage } from "../format";
import { store } from "../store";
</script>

<template>
  <div v-if="store.status" class="settings">
    <section class="panel card">
      <div class="label">Project</div>
      <div class="project">
        <span class="num mono">{{ String(store.status.project).padStart(2, "0") }}</span>
        <span class="pname">{{ store.projects[store.status.project - 1] || "Unnamed" }}</span>
      </div>
      <dl>
        <dt>Tempo source</dt>
        <dd>{{ store.status.usesProjectTempo ? "Project" : "Bank" }}</dd>
        <dt>Project tempo</dt>
        <dd class="mono">{{ bpm(store.status.projectTempo) }} BPM</dd>
      </dl>
    </section>

    <section class="panel card">
      <div class="label">Device</div>
      <dl>
        <dt>Port</dt>
        <dd class="mono">{{ store.status.port }}</dd>
        <dt>Free space</dt>
        <dd class="mono">{{ storage(store.status.freeKb) }}</dd>
        <dt>Screen</dt>
        <dd>{{ store.status.workingMode === 4 ? "Menu open" : "Normal" }}</dd>
      </dl>
    </section>

    <section class="panel banks">
      <div class="label">Banks</div>
      <table>
        <thead>
          <tr>
            <th>Bank</th>
            <th>Tempo</th>
            <th>Volume</th>
            <th>Protect</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="b in store.status.banks" :key="b.letter">
            <td class="letter">{{ b.letter }}</td>
            <td class="mono">{{ bpm(b.tempo) }}</td>
            <td>
              <div class="volume">
                <div class="meter"><span :style="{ width: `${(b.volume / 127) * 100}%` }" /></div>
                <span class="mono">{{ b.volume }}</span>
              </div>
            </td>
            <td>
              <span v-if="b.protected" class="lock"><Icon name="lock" :size="13" /> On</span>
              <span v-else class="muted">Off</span>
            </td>
          </tr>
        </tbody>
      </table>
    </section>

    <p class="note muted">Settings are read-only in this version.</p>
  </div>
</template>

<style scoped>
.settings {
  height: 100%;
  overflow: auto;
  padding: 14px;
  display: grid;
  grid-template-columns: 1fr 1fr;
  grid-auto-rows: min-content;
  gap: 14px;
  align-content: start;
  max-width: 980px;
}

.card,
.banks {
  padding: 16px;
}

.banks {
  grid-column: 1 / -1;
}

.project {
  display: flex;
  align-items: baseline;
  gap: 12px;
  margin: 8px 0 12px;
}

.num {
  font-size: 22px;
  color: var(--accent);
  font-weight: 600;
}

.pname {
  font-size: 18px;
  font-weight: 600;
}

dl {
  margin: 10px 0 0;
  display: grid;
  grid-template-columns: 140px 1fr;
  row-gap: 6px;
}

dt {
  color: var(--muted);
}

dd {
  margin: 0;
}

table {
  width: 100%;
  border-collapse: collapse;
  margin-top: 10px;
}

th {
  text-align: left;
  font-weight: 500;
  color: var(--muted);
  font-size: 12px;
  padding: 6px 10px;
  border-bottom: 1px solid var(--line);
}

td {
  padding: 7px 10px;
  border-bottom: 1px solid var(--line);
}

tr:last-child td {
  border-bottom: none;
}

.letter {
  font-weight: 650;
}

.volume {
  display: flex;
  align-items: center;
  gap: 10px;
}

.meter {
  width: 180px;
  height: 6px;
  border-radius: 3px;
  background: var(--line);
  overflow: hidden;
}

.meter span {
  display: block;
  height: 100%;
  background: var(--accent);
}

.lock {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  color: var(--accent);
}

.note {
  grid-column: 1 / -1;
  margin: 0;
  font-size: 12.5px;
}
</style>
