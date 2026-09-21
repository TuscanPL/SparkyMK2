<script setup lang="ts">
import { computed, ref } from "vue";
import BankStrip from "../components/BankStrip.vue";
import EditableName from "../components/EditableName.vue";
import Icon from "../components/Icon.vue";
import PadGrid from "../components/PadGrid.vue";
import SoundBrowser from "../components/SoundBrowser.vue";
import ParamField from "../components/ParamField.vue";
import WaveformView from "../components/WaveformView.vue";
import { api } from "../api";
import { BANK_LETTERS, BPM_RANGES, PARAM_GROUPS, SAMPLE_NAME_LEN, bpm, duration } from "../format";
import { exportSounds, restoreFromComputer } from "../exporting";
import { drag, pressPad } from "../drag";
import {
  analyzeBpm,
  editBlock,
  handleError,
  padOperation,
  renameSample,
  savePrefs,
  selectPad,
  setChopPoints,
  setPadParam,
  store,
} from "../store";

const counts = computed(() => {
  const c = new Array(10).fill(0);
  for (const p of store.pads) if (p.hasSample) c[Math.floor(p.index / 16)]++;
  return c;
});

/** Every pad of the bank on show, and of the project; exports skip the empty ones. */
const bankPads = computed(() => Array.from({ length: 16 }, (_, i) => store.bank * 16 + i));
const allPads = Array.from({ length: 160 }, (_, i) => i);

/** How far an export or restore has got, from the pads done out of the pads asked for. */
const transferPercent = computed(() => {
  const t = store.transfer;
  return t && t.total > 0 ? Math.round((t.done / t.total) * 100) : 0;
});

const pad = (index: number) => store.pads[index];
const detail = computed(() => store.detail);
const index = computed(() => store.selectedPad);
const blocked = computed(() => editBlock(index.value));
const busy = computed(() => (index.value !== null ? store.working[index.value] : undefined));
const locked = computed(() => !!blocked.value || !!busy.value);
const previewing = ref(false);
const wave = ref<InstanceType<typeof WaveformView> | null>(null);

const groups = computed(() => {
  const byName = new Map(detail.value?.params.map((p) => [p.name, p]));
  return PARAM_GROUPS.map((g) => ({
    title: g.title,
    params: g.names.flatMap((n) => (byName.has(n) ? [byName.get(n)!] : [])),
  }));
});

const vinylOn = computed(() => !!detail.value?.params.find((p) => p.name === "vinyl")?.value);

async function preview() {
  const d = detail.value;
  if (!d?.sample) return;
  const ms = Math.min(4000, Math.max(300, ((d.sample.end - d.sample.start) / 48000) * 1000));
  previewing.value = true;
  try {
    await api.preview(d.index, Math.round(ms));
  } catch (e) {
    handleError(e);
  } finally {
    previewing.value = false;
  }
}

async function commitParam(name: string, value: number) {
  if (index.value !== null) await setPadParam(index.value, name, value);
}

async function commitPoint(name: string, frame: number) {
  if (index.value !== null) await setPadParam(index.value, name, frame);
  wave.value?.reset();
}

async function commitChops(points: number[]) {
  if (index.value !== null) await setChopPoints(index.value, points);
  wave.value?.reset();
}
</script>

<template>
  <div class="samples">
    <BankStrip :counts="counts" :loading="store.padsLoading" />

    <div class="body">
      <section class="grid-col">
        <PadGrid
          :bank="store.bank"
          :selected="store.selectedPad"
          :filled="(i) => !!pad(i)?.hasSample"
          :working="store.working"
          droppable
          @select="selectPad"
          @press="pressPad"
        >
          <template #default="{ index: i }">
            <template v-if="pad(i)?.hasSample">
              <span class="pad-name">{{ pad(i)!.name }}</span>
              <span class="pad-meta mono">{{ bpm(pad(i)!.bpm) }}</span>
            </template>
          </template>
        </PadGrid>

        <div class="import panel" :class="{ open: store.prefs.importOpen }">
          <button class="toggle" @click="store.prefs.importOpen = !store.prefs.importOpen; savePrefs()">
            <Icon name="chevron" :class="{ turned: store.prefs.importOpen }" />
            <span class="label">Import &amp; export</span>
          </button>
          <template v-if="store.prefs.importOpen">
            <SoundBrowser />
            <p class="muted hint">
              {{ drag.files ? "Drop on a pad to import. Several files fill the following pads." : "Audio files from your file manager can be dropped on a pad too." }}
            </p>
            <label class="row">
              <input v-model="store.prefs.detectBpm" type="checkbox" @change="savePrefs" />
              Detect BPM after import
            </label>
            <label class="row">
              <span class="muted">BPM range</span>
              <select v-model.number="store.prefs.bpmRange" @change="savePrefs">
                <option v-for="[value, label] in BPM_RANGES" :key="value" :value="value">{{ label }}</option>
              </select>
            </label>

            <div class="export">
              <span class="muted">Export as WAV</span>
              <div class="export-buttons">
                <button
                  class="small"
                  :disabled="store.pending > 0 || !counts[store.bank]"
                  @click="exportSounds('bank', bankPads)"
                >
                  Bank {{ BANK_LETTERS[store.bank] }}
                </button>
                <button
                  class="small"
                  :disabled="store.pending > 0 || !store.pads.some((p) => p.hasSample)"
                  @click="exportSounds('project', allPads)"
                >
                  Project
                </button>
                <button
                  class="small ghost"
                  :disabled="store.pending > 0"
                  title="Put a SparkyMK2 export from this computer back on its pads"
                  @click="restoreFromComputer"
                >
                  Restore…
                </button>
              </div>
            </div>
            <p class="muted hint">
              Each sound is named after its pad, with a settings file beside it, so a restore puts
              every sound back where it was. Restore from the card in the browser above.
            </p>
            <div v-if="store.transfer" class="progress">
              <div class="track"><div class="fill" :style="{ width: `${transferPercent}%` }" /></div>
              <span class="mono">{{ store.transfer.name }}</span>
            </div>
          </template>
        </div>
      </section>

      <section class="detail panel">
        <div v-if="store.selectedPad === null" class="placeholder muted">Select a pad to see its sample and parameters.</div>

        <template v-else>
          <header class="head">
            <span class="pad-label mono">{{ detail?.label ?? "" }}</span>
            <div class="title">
              <EditableName
                v-if="detail?.sample"
                class="name"
                :value="detail.name"
                :max-length="SAMPLE_NAME_LEN"
                :disabled="locked"
                @commit="renameSample(detail.index, $event)"
              />
              <div v-else class="name">{{ detail ? "Empty pad" : "" }}</div>
              <div v-if="detail?.sample" class="facts muted">
                <span>{{ detail.sample.channels === 1 ? "Mono" : "Stereo" }}</span>
                <span class="mono">{{ duration(detail.sample.frames) }}</span>
                <span class="mono">{{ detail.sample.frames.toLocaleString() }} frames</span>
              </div>
            </div>
            <span v-if="store.detailLoading || busy" class="status muted"><span class="spinner" />{{ busy }}</span>
            <div v-if="detail?.sample" class="actions">
              <button :disabled="previewing" title="Play the pad on the device" @click="preview">
                <Icon name="play" :size="14" /> {{ previewing ? "Playing…" : "Preview" }}
              </button>
              <button :disabled="locked" @click="padOperation(detail.index, 'truncate')">Truncate</button>
              <button :disabled="locked" @click="padOperation(detail.index, 'normalize')">Normalize</button>
              <button :disabled="store.pending > 0" title="Save this pad's sample as a WAV" @click="exportSounds('pad', [detail.index])">
                Export…
              </button>
              <button class="danger" :disabled="locked" @click="padOperation(detail.index, 'delete')">Delete</button>
            </div>
          </header>

          <div v-if="blocked && detail" class="notice">{{ blocked }}. Editing is off for this pad.</div>

          <template v-if="detail?.sample">
            <WaveformView
              ref="wave"
              :waveform="store.waveform"
              :sample="detail.sample"
              :loading="store.waveLoading"
              :editable="!locked"
              @point="commitPoint"
              @chops="commitChops"
            />
            <div class="wave-bar">
              <div class="points">
                <div><span class="label">Start</span><span class="mono">{{ duration(detail.sample.start) }}</span></div>
                <div><span class="label">End</span><span class="mono">{{ duration(detail.sample.end) }}</span></div>
                <div><span class="label">Loop top</span><span class="mono">{{ duration(detail.sample.loopTop) }}</span></div>
                <div><span class="label">Chops</span><span class="mono">{{ detail.sample.chopPoints.length }} / 16</span></div>
              </div>
              <button
                class="ghost small"
                :disabled="locked || !detail.sample.chopPoints.length"
                @click="commitChops([])"
              >
                Clear chops
              </button>
            </div>
            <p v-if="!locked" class="muted hint">
              Drag S, E and L to move Start, End and Loop top. Double-click to add a chop point; drag it to move it, right-click it to remove it. The chop at the very start is kept until you clear all chops.
            </p>

            <div class="groups">
              <div v-for="g in groups" :key="g.title" class="group">
                <div class="label">{{ g.title }}</div>
                <div class="fields">
                  <template v-for="p in g.params" :key="`${p.name}-${store.detailRevision}`">
                    <ParamField
                      :param="p"
                      :disabled="locked || (vinylOn && (p.name === 'pitch-coarse' || p.name === 'pitch-fine'))"
                      @commit="commitParam(p.name, $event)"
                    />
                    <div v-if="p.name === 'bpm'" class="bpm-tools">
                      <button class="small" :disabled="locked" @click="analyzeBpm(detail.index, 'detect')">Detect</button>
                      <button class="small" :disabled="locked" title="Whole beats that fit between Start and End" @click="analyzeBpm(detail.index, 'length')">From Start–End</button>
                    </div>
                  </template>
                </div>
              </div>
            </div>
          </template>

          <div v-else-if="detail" class="placeholder muted">
            This pad has no sample. Drop an audio file onto it to import one.
          </div>
        </template>
      </section>
    </div>
  </div>
</template>

<style scoped>
.samples {
  height: 100%;
  display: flex;
  flex-direction: column;
  gap: 14px;
  padding: 14px;
  min-height: 0;
}

.body {
  flex: 1;
  min-height: 0;
  display: grid;
  grid-template-columns: minmax(340px, 420px) 1fr;
  gap: 14px;
}

.grid-col {
  min-height: 0;
  overflow: auto;
  display: flex;
  flex-direction: column;
  gap: 14px;
}

.pad-name {
  font-size: 12px;
  line-height: 1.25;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
  overflow-wrap: anywhere;
}

.pad-meta {
  font-size: 10.5px;
  color: var(--muted);
}

.import {
  padding: 12px 14px;
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.export {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  flex-wrap: wrap;
  margin-top: 4px;
  padding-top: 10px;
  border-top: 1px solid var(--line);
}

.export-buttons {
  display: flex;
  gap: 6px;
}

button.small {
  padding: 4px 10px;
  font-size: 12.5px;
}

.progress {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 11.5px;
  color: var(--muted);
}

.progress .track {
  flex: 1;
  height: 4px;
  border-radius: 2px;
  background: var(--line-strong);
  overflow: hidden;
}

.progress .fill {
  height: 100%;
  background: var(--accent);
  transition: width 0.1s linear;
}

.progress .mono {
  max-width: 55%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.toggle {
  display: flex;
  align-items: center;
  gap: 7px;
  padding: 0;
  border: none;
  background: none;
  align-self: flex-start;
}

.toggle .label {
  color: var(--muted);
}

.toggle:hover .label {
  color: var(--text);
}

.toggle svg {
  color: var(--faint);
  transition: transform 0.15s;
}

.toggle svg.turned {
  transform: rotate(90deg);
}

.row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
}

label.row:first-of-type {
  justify-content: flex-start;
}

.hint {
  margin: 0;
  font-size: 12.5px;
}

.detail {
  min-height: 0;
  overflow: auto;
  padding: 16px;
  display: flex;
  flex-direction: column;
  gap: 12px;
}

/* The panel scrolls; keep sections, including the fixed-height waveform, at full size. */
.detail > * {
  flex-shrink: 0;
}

.placeholder {
  margin: auto;
  text-align: center;
}

.head {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 10px 14px;
}

.pad-label {
  font-size: 22px;
  font-weight: 600;
  color: var(--accent);
  min-width: 48px;
}

.title {
  flex: 1;
  min-width: 160px;
}

.name {
  font-size: 17px;
  font-weight: 600;
}

.facts {
  display: flex;
  gap: 14px;
  font-size: 12.5px;
}

.status {
  display: flex;
  align-items: center;
  gap: 8px;
}

.actions {
  display: flex;
  gap: 6px;
}

.notice {
  padding: 8px 12px;
  border-radius: 6px;
  border: 1px solid var(--accent-line);
  background: var(--accent-soft);
  color: #ffd699;
  font-size: 13px;
}

.wave-bar {
  display: flex;
  align-items: center;
  gap: 10px;
}

.points {
  flex: 1;
  display: grid;
  grid-template-columns: repeat(4, 1fr);
  gap: 10px;
}

.points div {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.groups {
  display: grid;
  /* Wide enough that a group never squeezes the trigger checkboxes onto two lines. */
  grid-template-columns: repeat(auto-fill, minmax(360px, 1fr));
  gap: 10px;
}

.group {
  padding: 12px;
  border: 1px solid var(--line);
  border-radius: 8px;
  background: var(--panel-2);
}

.fields {
  margin-top: 8px;
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.bpm-tools {
  display: flex;
  justify-content: flex-end;
  gap: 6px;
  margin-bottom: 4px;
}

.small {
  padding: 3px 8px;
  font-size: 12px;
}
</style>
