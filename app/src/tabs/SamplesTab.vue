<script setup lang="ts">
import { computed, onUnmounted, reactive, ref, watch } from "vue";
import BankStrip from "../components/BankStrip.vue";
import EditableName from "../components/EditableName.vue";
import Icon from "../components/Icon.vue";
import PadGrid from "../components/PadGrid.vue";
import SoundBrowser from "../components/SoundBrowser.vue";
import ParamField from "../components/ParamField.vue";
import WaveformView from "../components/WaveformView.vue";
import { api, type PadDetail } from "../api";
import { BANK_LETTERS, BPM_RANGES, PARAM_GROUPS, SAMPLE_NAME_LEN, bpm, duration } from "../format";
import { exportSounds, resampleSound, restoreFromComputer } from "../exporting";
import { drag, pressPad } from "../drag";
import {
  analyzeBpm,
  copyBpm,
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
/** Pad the device is playing for Preview, or null. */
const previewing = ref<number | null>(null);
/** The previewed pad has Gate off, so letting go does not stop it and Stop must cut it off. */
let previewLatched = false;
/** Ends a preview that would otherwise run on. */
let previewTimer: number | undefined;
const wave = ref<InstanceType<typeof WaveformView> | null>(null);

const groups = computed(() => {
  const byName = new Map(detail.value?.params.map((p) => [p.name, p]));
  return PARAM_GROUPS.map((g) => ({
    title: g.title,
    params: g.names.flatMap((n) => (byName.has(n) ? [byName.get(n)!] : [])),
  }));
});

/** Keys found this session, by project and pad; the device has nowhere to keep them. */
const keys = reactive<Record<string, string | null>>({});
const keyBusy = ref(false);
const keyId = (pad: number) => `${store.status?.project}:${pad}`;
const keyText = computed(() => {
  const d = detail.value;
  if (!d) return null;
  const found = keys[keyId(d.index)];
  return found === undefined ? null : found ?? "No clear key";
});

async function findKey(pad: number) {
  keyBusy.value = true;
  try {
    const key = await api.detectKey(pad);
    keys[keyId(pad)] = key ? `${key.name} (${key.camelot})` : null;
  } catch (e) {
    handleError(e);
  } finally {
    keyBusy.value = false;
  }
}

const vinylOn = computed(() => !!detail.value?.params.find((p) => p.name === "vinyl")?.value);

/** Longest a preview runs by itself; a looping pad would otherwise play until Stop. */
const PREVIEW_MAX_MS = 60_000;

/** Play the pad on the device from start to end, or stop it if it is playing. */
async function preview() {
  if (previewing.value !== null) return stopPreview();
  if (soundingOnDevice.value) return stopDevicePlay();
  const d = detail.value;
  if (!d?.sample) return;
  const { start, end } = d.sample;
  // As long as Start–End takes at the pad's speed (BPM Sync or Time Stretch).
  const ms = Math.min(PREVIEW_MAX_MS, Math.max(300, (end - start) / playRate(d)));
  previewing.value = d.index;
  previewLatched = d.params.find((p) => p.name === "gate")?.value === 0;
  try {
    await api.previewStart(d.index);
  } catch (e) {
    previewing.value = null;
    handleError(e);
    return;
  }
  previewTimer = window.setTimeout(() => {
    if (previewing.value === d.index) stopPreview();
  }, ms);
}

async function stopPreview() {
  const pad = previewing.value;
  if (pad === null) return;
  previewing.value = null;
  window.clearTimeout(previewTimer);
  forgetPlaying(pad);
  try {
    await api.previewStop(pad, previewLatched);
  } catch (e) {
    handleError(e);
  }
}

/** The shown pad is sounding because it was hit on the unit, not by Preview here. */
const soundingOnDevice = computed(
  () => previewing.value === null && index.value !== null && store.playingPad === index.value,
);

/** Stop a pad that was hit on the unit. Only a Gate-off pad needs cutting off; a Gate-on
 * one stops when the pad is let go, and halting it would leave its Gate switched off. */
async function stopDevicePlay() {
  const d = detail.value;
  if (!d) return;
  forgetPlaying(d.index);
  try {
    await api.previewStop(d.index, d.params.find((p) => p.name === "gate")?.value === 0);
  } catch (e) {
    handleError(e);
  }
}

/** Show the pad as stopped at once, instead of until the next status poll. */
function forgetPlaying(pad: number) {
  if (store.playingPad !== pad) return;
  store.playingPad = null;
  store.playPosition = null;
}

/** The Preview button shows Stop: a preview from here, or the pad hit on the unit. */
const sounding = computed(() => previewing.value !== null || soundingOnDevice.value);

// Picking another pad or leaving the tab ends the preview, as Stop does.
watch(index, stopPreview);

/** Frame the shown pad has reached while it sounds, whether started here or on the unit. */
const playhead = ref<number | null>(null);
let playheadFrame = 0;

/** Frames played per millisecond at normal speed. */
const NORMAL_RATE = 48;
/** Further than this from a report (in ms of playback), the playhead is put right. */
const DRIFT_MS = 250;

/**
 * How fast the pad's play position moves, from its settings. Measured on the unit
 * (fw 5.52): with BPM Sync on, tempo ÷ pad BPM (the bank's or the project's tempo, as
 * Tempo Select says) and Time Stretch is ignored; with it off, Time Stretch %. Pitch
 * and Vinyl do not change it.
 */
function playRate(d: PadDetail): number {
  const param = (name: string) => d.params.find((p) => p.name === name)?.value ?? 0;
  const st = store.status;
  if (param("bpm-sync") && st && param("bpm") > 0) {
    const tempo = st.usesProjectTempo ? st.projectTempo : (st.banks[Math.floor(d.index / 16)]?.tempo ?? 0);
    if (tempo > 0) return (NORMAL_RATE * tempo) / param("bpm");
  }
  return (NORMAL_RATE * (param("time-stretch") || 10000)) / 10000;
}

/** The playhead's anchor: a report, and how the playhead moves on from it. */
let clock: { at: number; frame: number; rate: number; direction: 1 | -1 } | null = null;
/** The report before the latest, to tell which way a ping-pong pad is going. */
let lastReport: { frames: number; at: number } | null = null;

/**
 * Where the shown pad is, in frames from the start of its file. The device reports that
 * position itself (measured: an absolute frame, counting down in Reverse), every few
 * hundred ms. Between reports the playhead moves on at the fixed speed from the pad's
 * settings, in the direction it is travelling; reports only re-anchor it when it starts,
 * turns or drifts more than DRIFT_MS.
 */
function followPlayhead() {
  cancelAnimationFrame(playheadFrame);
  const d = detail.value;
  const pos = store.playPosition;
  if (!d?.sample || !pos || store.playingPad !== d.index) {
    playhead.value = null;
    clock = null;
    lastReport = null;
    return;
  }
  const { start, end, loopTop } = d.sample;
  const param = (name: string) => d.params.find((p) => p.name === name)?.value ?? 0;
  const mode = param("play-mode"); // 0 forward, 1 reverse, 2/3 ping-pong
  const rate = playRate(d);
  if (pos !== lastReport) {
    // Ping-pong turns at the ends: which way it goes shows between two reports.
    const moved = lastReport ? pos.frames - lastReport.frames : 0;
    let direction: 1 | -1 = mode === 1 || mode === 3 ? -1 : 1;
    if ((mode === 2 || mode === 3) && Math.abs(moved) > 0) direction = moved > 0 ? 1 : -1;
    const expected = clock ? clock.frame + clock.direction * (pos.at - clock.at) * clock.rate : NaN;
    if (
      !clock ||
      clock.rate !== rate ||
      clock.direction !== direction ||
      !(Math.abs(expected - pos.frames) <= DRIFT_MS * rate)
    ) {
      clock = { at: pos.at, frame: pos.frames, rate, direction };
    }
    lastReport = pos;
  }
  const c = clock!;
  let frame = c.frame + c.direction * (performance.now() - c.at) * c.rate;
  const looping = !!param("loop");
  if (c.direction > 0 && frame > end) {
    frame = looping && end > loopTop ? loopTop + ((frame - end) % (end - loopTop)) : end;
  } else if (c.direction < 0 && frame < start) {
    frame = looping && end > start ? end - ((start - frame) % (end - start)) : start;
  }
  playhead.value = frame;
  playheadFrame = requestAnimationFrame(followPlayhead);
}

watch(() => [store.playPosition, store.playingPad, detail.value], followPlayhead);
onUnmounted(() => cancelAnimationFrame(playheadFrame));
onUnmounted(stopPreview);

async function commitParam(name: string, value: number) {
  if (index.value !== null) await setPadParam(index.value, name, value);
}

async function commitPoint(name: string, frame: number) {
  if (index.value !== null) await setPadParam(index.value, name, frame);
  wave.value?.reset();
}

/** A pad parameter's value, or null when the pad has none (an empty pad). */
function paramValue(name: string): number | null {
  return detail.value?.params.find((p) => p.name === name)?.value ?? null;
}

async function commitEnvelope(name: "attack" | "release", value: number) {
  if (index.value !== null) await setPadParam(index.value, name, value);
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
          :playing="store.playingPad"
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
              <span title="Set each imported sample's BPM by listening to it">Detect BPM after import</span>
            </label>
            <label class="row">
              <span class="muted" title="Where detection looks for a tempo; a half or double tempo is folded into this range">BPM range</span>
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
                  title="Save every sample in this bank as WAV files on this computer"
                  @click="exportSounds('bank', bankPads)"
                >
                  Bank {{ BANK_LETTERS[store.bank] }}
                </button>
                <button
                  class="small"
                  :disabled="store.pending > 0 || !store.pads.some((p) => p.hasSample)"
                  title="Save every sample in the project as WAV files on this computer"
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
              <button :title="sounding ? 'Stop the pad' : 'Play the pad on the device'" @click="preview">
                <Icon :name="sounding ? 'stop' : 'play'" :size="14" />
                {{ sounding ? "Stop" : "Preview" }}
              </button>
              <button :disabled="locked" title="Cut away the audio before Start and after End, on the device. Can't be undone." @click="padOperation(detail.index, 'truncate')">Truncate</button>
              <button :disabled="locked" title="Raise the sample's level to full scale, on the device. Can't be undone." @click="padOperation(detail.index, 'normalize')">Normalize</button>
              <button :disabled="store.pending > 0" title="Save this pad's sample as a WAV" @click="exportSounds('pad', [detail.index])">
                Export…
              </button>
              <button
                :disabled="store.pending > 0"
                title="Play this pad on the unit and record it over USB audio, as it sounds"
                @click="resampleSound(detail.index)"
              >
                Resample…
              </button>
              <button class="danger" :disabled="locked" title="Erase the sample from this pad. Files on the SD card are not touched." @click="padOperation(detail.index, 'delete')">Delete</button>
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
              :playhead="playhead"
              :attack="paramValue('attack')"
              :release="paramValue('release')"
              @point="commitPoint"
              @chops="commitChops"
              @envelope="commitEnvelope"
            />
            <div class="wave-bar">
              <div class="points">
                <div><span class="label">Start</span><span class="mono">{{ duration(detail.sample.start) }}</span></div>
                <div><span class="label">End</span><span class="mono">{{ duration(detail.sample.end) }}</span></div>
                <div><span class="label">Loop top</span><span class="mono">{{ duration(detail.sample.loopTop) }}</span></div>
                <div><span class="label">Chops</span><span class="mono">{{ detail.sample.chopPoints.length }} / 16</span></div>
              </div>
              <div class="zoom">
                <button class="ghost small" title="Zoom out (or scroll down on the wave)" @click="wave?.zoom(0.5)">−</button>
                <button class="ghost small" title="Zoom in (or scroll up on the wave)" @click="wave?.zoom(2)">+</button>
                <button class="ghost small" :disabled="!wave?.zoomed" title="Show the whole sample" @click="wave?.fit()">Fit</button>
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
              Drag S, E and L to move Start, End and Loop top. Double-click to add a chop point; drag it to move it, right-click it to remove it. The chop at the very start is kept until you clear all chops. Drag the A and R diamonds to set Attack and Release (they show the 0–127 value, not a time). Scroll on the wave to zoom, Shift+scroll to move along.
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
                    <div v-if="p.name === 'pitch-fine'" class="key-field">
                      <span class="key-label">Key</span>
                      <span class="key-value">
                        <span v-if="keyText" class="mono">{{ keyText }}</span>
                        <span v-else class="muted">Not detected</span>
                        <button class="small" :disabled="keyBusy" title="Estimate the key between Start and End; shown here only, as the SP-404MKII keeps no key" @click="findKey(detail.index)">
                          {{ keyBusy ? "Listening…" : "Detect" }}
                        </button>
                      </span>
                    </div>
                    <div v-if="p.name === 'bpm'" class="bpm-tools">
                      <button class="small" :disabled="locked" title="Listen to the sample and set the tempo it finds" @click="analyzeBpm(detail.index, 'detect')">Detect</button>
                      <button class="small" :disabled="locked" title="Whole beats that fit between Start and End" @click="analyzeBpm(detail.index, 'length')">From Start–End</button>
                      <button class="small" :disabled="locked" title="Give this BPM to the other pads in this bank or in the whole project" @click="copyBpm(detail.index)">Use for…</button>
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

.zoom {
  display: flex;
  gap: 2px;
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

.key-field {
  display: grid;
  grid-template-columns: 96px 1fr;
  align-items: center;
  gap: 10px;
  min-height: 30px;
}

.key-label {
  color: var(--muted);
}

.key-value {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 8px;
  font-size: 12.5px;
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
