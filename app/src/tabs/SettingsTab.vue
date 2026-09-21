<script setup lang="ts">
import { ref, watch } from "vue";
import EditableName from "../components/EditableName.vue";
import Icon from "../components/Icon.vue";
import { PROJECT_NAME_LEN, bpm, storage } from "../format";
import { renameProject, savePrefs, setGlobalParam, store } from "../store";

const locked = () => store.status?.workingMode === 4;

/** Tempo text boxes, keyed by setting name, while being typed in. */
const tempoText = ref<Record<string, string>>({});
const volumeDraft = ref<Record<string, number>>({});

watch(
  () => store.status,
  () => {
    tempoText.value = {};
    volumeDraft.value = {};
  },
);

function tempoValue(name: string, hundredths: number) {
  return tempoText.value[name] ?? bpm(hundredths);
}

function commitTempo(name: string, current: number) {
  const text = tempoText.value[name];
  delete tempoText.value[name];
  if (text === undefined) return;
  const value = Math.round(parseFloat(text) * 100);
  if (Number.isFinite(value) && value >= 4000 && value <= 20000 && value !== current) setGlobalParam(name, value);
}

function commitVolume(letter: string, current: number) {
  const name = `bank-volume-${letter.toLowerCase()}`;
  const value = volumeDraft.value[name];
  if (value !== undefined && value !== current) setGlobalParam(name, value);
}
</script>

<template>
  <div v-if="store.status" class="settings">
    <section class="panel card">
      <div class="label">Project</div>
      <div class="project">
        <span class="num mono">{{ String(store.status.project).padStart(2, "0") }}</span>
        <EditableName
          class="pname"
          :value="store.projects[store.status.project - 1] ?? ''"
          placeholder="Unnamed"
          :max-length="PROJECT_NAME_LEN"
          :disabled="locked()"
          @commit="renameProject(store.status.project, $event)"
        />
      </div>
      <div class="fields">
        <label class="field">
          <span>Tempo source</span>
          <select
            :value="store.status.usesProjectTempo ? 1 : 0"
            :disabled="locked()"
            @change="setGlobalParam('tempo-select', Number(($event.target as HTMLSelectElement).value))"
          >
            <option :value="0">Bank tempo</option>
            <option :value="1">Project tempo</option>
          </select>
        </label>
        <label class="field">
          <span>Project tempo</span>
          <span class="tempo">
            <input
              class="mono"
              inputmode="decimal"
              :value="tempoValue('project-tempo', store.status.projectTempo)"
              :disabled="locked()"
              @input="tempoText['project-tempo'] = ($event.target as HTMLInputElement).value"
              @keydown.enter="($event.target as HTMLInputElement).blur()"
              @blur="commitTempo('project-tempo', store.status.projectTempo)"
            />
            BPM
          </span>
        </label>
      </div>
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
            <th>Tempo (BPM)</th>
            <th>Volume</th>
            <th>Protect</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="b in store.status.banks" :key="b.letter">
            <td class="letter">{{ b.letter }}</td>
            <td>
              <input
                class="mono bank-tempo"
                inputmode="decimal"
                :value="tempoValue(`bank-tempo-${b.letter.toLowerCase()}`, b.tempo)"
                :disabled="locked()"
                @input="tempoText[`bank-tempo-${b.letter.toLowerCase()}`] = ($event.target as HTMLInputElement).value"
                @keydown.enter="($event.target as HTMLInputElement).blur()"
                @blur="commitTempo(`bank-tempo-${b.letter.toLowerCase()}`, b.tempo)"
              />
            </td>
            <td>
              <div class="volume">
                <input
                  type="range"
                  min="0"
                  max="127"
                  :value="volumeDraft[`bank-volume-${b.letter.toLowerCase()}`] ?? b.volume"
                  :disabled="locked()"
                  @input="volumeDraft[`bank-volume-${b.letter.toLowerCase()}`] = Number(($event.target as HTMLInputElement).value)"
                  @change="commitVolume(b.letter, b.volume)"
                />
                <span class="mono">{{ volumeDraft[`bank-volume-${b.letter.toLowerCase()}`] ?? b.volume }}</span>
              </div>
            </td>
            <td>
              <label class="switch-row">
                <span class="switch">
                  <input
                    type="checkbox"
                    :checked="b.protected"
                    :disabled="locked()"
                    @change="setGlobalParam(`bank-protect-${b.letter.toLowerCase()}`, ($event.target as HTMLInputElement).checked ? 1 : 0)"
                  />
                  <span />
                </span>
                <Icon v-if="b.protected" name="lock" :size="13" />
              </label>
            </td>
          </tr>
        </tbody>
      </table>
    </section>

    <p class="note muted">Changes go to the SP-404MKII as soon as you make them.</p>

    <section class="panel app">
      <div class="label">App</div>
      <label class="pref">
        <span class="switch">
          <input v-model="store.prefs.preloadFolders" type="checkbox" @change="savePrefs" />
          <span />
        </span>
        <span>
          Preload sounds when a folder opens
          <span class="muted hint">
            Every sound in a folder is fetched as soon as it opens, in the Files tab and the
            Import browser, so browsing it plays each one at once. Opening a folder keeps the
            device busy for longer while it loads.
          </span>
        </span>
      </label>
    </section>
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

.banks,
.app {
  grid-column: 1 / -1;
}

.app {
  padding: 16px;
}

.pref {
  display: flex;
  align-items: flex-start;
  gap: 12px;
  margin-top: 10px;
  cursor: pointer;
}

.pref .hint {
  display: block;
  margin-top: 3px;
  font-size: 12.5px;
  max-width: 640px;
}

.project {
  display: flex;
  align-items: baseline;
  gap: 12px;
  margin: 8px 0 12px;
  min-width: 0;
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

.fields {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.field {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  color: var(--muted);
}

.field select,
.field input {
  color: var(--text);
}

.tempo {
  display: flex;
  align-items: center;
  gap: 6px;
}

.tempo input,
.bank-tempo {
  width: 84px;
  text-align: right;
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
  padding: 5px 10px;
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

.volume input {
  width: 200px;
}

.volume span {
  width: 28px;
  text-align: right;
}

.switch-row {
  display: flex;
  align-items: center;
  gap: 8px;
  color: var(--accent);
}

.note {
  grid-column: 1 / -1;
  margin: 0;
  font-size: 12.5px;
}
</style>
