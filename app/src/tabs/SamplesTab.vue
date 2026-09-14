<script setup lang="ts">
import { computed, ref, watch } from "vue";
import BankStrip from "../components/BankStrip.vue";
import Icon from "../components/Icon.vue";
import PadGrid from "../components/PadGrid.vue";
import WaveformView from "../components/WaveformView.vue";
import { api, type PadDetail, type Waveform } from "../api";
import { PARAM_GROUPS, PARAM_VIEW, bpm, duration, showParam } from "../format";
import { handleError, store } from "../store";

const WAVE_POINTS = 2048;

const counts = computed(() => {
  const c = new Array(10).fill(0);
  for (const p of store.pads) if (p.hasSample) c[Math.floor(p.index / 16)]++;
  return c;
});

const pad = (index: number) => store.pads[index];
const detail = ref<PadDetail | null>(null);
const waveform = ref<Waveform | null>(null);
const detailLoading = ref(false);
const waveLoading = ref(false);
const previewing = ref(false);
const waveCache = new Map<string, Waveform>();
let request = 0;

const groups = computed(() => {
  const byName = new Map(detail.value?.params.map((p) => [p.name, p]));
  return PARAM_GROUPS.map((g) => ({
    title: g.title,
    params: g.names.flatMap((n) => (byName.has(n) ? [byName.get(n)!] : [])),
  }));
});

watch(
  () => [store.selectedPad, store.generation] as const,
  async ([index]) => {
    const token = ++request;
    detail.value = null;
    waveform.value = null;
    if (index === null) return;
    detailLoading.value = true;
    try {
      const d = await api.padDetail(index);
      if (token !== request) return;
      detail.value = d;
      if (!d.sample) return;
      const key = `${store.generation}:${store.status?.project}:${index}:${pad(index)?.fileSize}`;
      const cached = waveCache.get(key);
      if (cached) {
        waveform.value = cached;
        return;
      }
      waveLoading.value = true;
      const wf = await api.waveform(index, WAVE_POINTS);
      waveCache.set(key, wf);
      if (token === request) waveform.value = wf;
    } catch (e) {
      if (token === request) handleError(e);
    } finally {
      if (token === request) {
        detailLoading.value = false;
        waveLoading.value = false;
      }
    }
  },
  { immediate: true },
);

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
</script>

<template>
  <div class="samples">
    <BankStrip :counts="counts" :loading="store.padsLoading" />

    <div class="body">
      <section class="grid-col">
        <PadGrid :bank="store.bank" :selected="store.selectedPad" :filled="(i) => !!pad(i)?.hasSample" @select="store.selectedPad = $event">
          <template #default="{ index }">
            <template v-if="pad(index)?.hasSample">
              <span class="pad-name">{{ pad(index)!.name }}</span>
              <span class="pad-meta mono">{{ bpm(pad(index)!.bpm) }}</span>
            </template>
          </template>
        </PadGrid>
      </section>

      <section class="detail panel">
        <div v-if="store.selectedPad === null" class="placeholder muted">Select a pad to see its sample and parameters.</div>

        <template v-else>
          <header class="head">
            <span class="pad-label mono">{{ detail?.label ?? "" }}</span>
            <div class="title">
              <div class="name">{{ detail ? detail.name || "Empty pad" : "" }}</div>
              <div v-if="detail?.sample" class="facts muted">
                <span>{{ detail.sample.channels === 1 ? "Mono" : "Stereo" }}</span>
                <span class="mono">{{ duration(detail.sample.frames) }}</span>
                <span class="mono">{{ detail.sample.frames.toLocaleString() }} frames</span>
              </div>
            </div>
            <span v-if="detailLoading" class="spinner" />
            <button v-if="detail?.sample" :disabled="previewing" title="Play the pad on the device" @click="preview">
              <Icon name="play" :size="14" /> {{ previewing ? "Playing…" : "Preview" }}
            </button>
          </header>

          <template v-if="detail?.sample">
            <WaveformView :waveform="waveform" :sample="detail.sample" :loading="waveLoading" />
            <div class="points">
              <div><span class="label">Start</span><span class="mono">{{ duration(detail.sample.start) }}</span></div>
              <div><span class="label">End</span><span class="mono">{{ duration(detail.sample.end) }}</span></div>
              <div><span class="label">Loop top</span><span class="mono">{{ duration(detail.sample.loopTop) }}</span></div>
              <div><span class="label">Chops</span><span class="mono">{{ detail.sample.chopPoints.length }}</span></div>
            </div>

            <div class="groups">
              <div v-for="g in groups" :key="g.title" class="group">
                <div class="label">{{ g.title }}</div>
                <dl>
                  <template v-for="p in g.params" :key="p.name">
                    <dt>{{ PARAM_VIEW[p.name]?.label ?? p.name }}</dt>
                    <dd class="mono">{{ showParam(p) }}</dd>
                  </template>
                </dl>
              </div>
            </div>
          </template>

          <div v-else-if="detail" class="placeholder muted">This pad has no sample.</div>
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
  grid-template-columns: minmax(360px, 440px) 1fr;
  gap: 14px;
}

.grid-col {
  min-height: 0;
}

.pad-name {
  font-size: 12px;
  line-height: 1.25;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
  word-break: break-word;
}

.pad-meta {
  font-size: 10.5px;
  color: var(--muted);
}

.detail {
  min-height: 0;
  overflow: auto;
  padding: 16px;
  display: flex;
  flex-direction: column;
  gap: 14px;
}

/* The panel scrolls; keep sections, including the fixed-height waveform, at full size. */
.detail > * {
  flex-shrink: 0;
}

.placeholder {
  margin: auto;
}

.head {
  display: flex;
  align-items: center;
  gap: 14px;
}

.pad-label {
  font-size: 22px;
  font-weight: 600;
  color: var(--accent);
  min-width: 48px;
}

.title {
  flex: 1;
  min-width: 0;
}

.name {
  font-size: 17px;
  font-weight: 600;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.facts {
  display: flex;
  gap: 14px;
  font-size: 12.5px;
}

.points {
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
  grid-template-columns: repeat(auto-fill, minmax(230px, 1fr));
  gap: 10px;
}

.group {
  padding: 12px;
  border: 1px solid var(--line);
  border-radius: 8px;
  background: var(--panel-2);
}

dl {
  margin: 8px 0 0;
  display: grid;
  grid-template-columns: 1fr auto;
  row-gap: 5px;
  column-gap: 12px;
}

dt {
  color: var(--muted);
}

dd {
  margin: 0;
  text-align: right;
}
</style>
