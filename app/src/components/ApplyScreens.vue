<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, reactive, ref } from "vue";
import type { LibraryItem } from "../api";
import { SCREEN_SAVER_SLOTS, STARTUP_SLOTS } from "../screens";
import { applyToProjects, notify, projectName, slotName, store } from "../store";

const props = defineProps<{ item: LibraryItem }>();
const emit = defineEmits<{ close: [] }>();

const isSet = computed(() => props.item.kind === "set");
/** For a frame: which slots it goes into. */
const slots = reactive<Record<string, boolean>>({});
/** For a set: which of its two groups go out. */
const groups = reactive({ startup: true, saver: true });
const projects = reactive<Record<number, boolean>>({});
const done = ref(0);
const running = ref(false);
const failed = ref<[number, string][]>([]);

const PROJECTS = Array.from({ length: 16 }, (_, i) => i + 1);

const chosenProjects = computed(() => PROJECTS.filter((p) => projects[p]));

/** What goes where: `[slots, frames]`, matched by position. */
const plan = computed((): [string[], number[][]] => {
  if (isSet.value) {
    const all = [...STARTUP_SLOTS, ...SCREEN_SAVER_SLOTS];
    const pairs = all
      .map((slot, i) => [slot, props.item.frames[i]] as const)
      .filter(([slot]) => (STARTUP_SLOTS.includes(slot) ? groups.startup : groups.saver));
    return [pairs.map(([s]) => s), pairs.map(([, f]) => f)];
  }
  const chosen = [...STARTUP_SLOTS, ...SCREEN_SAVER_SLOTS].filter((s) => slots[s]);
  return [chosen, chosen.map(() => props.item.frames[0])];
});

const ready = computed(() => !running.value && chosenProjects.value.length > 0 && plan.value[0].length > 0);

function pickSlots(list: string[]) {
  const on = !list.every((s) => slots[s]);
  for (const s of list) slots[s] = on;
}

function pickProjects(on: boolean) {
  for (const p of PROJECTS) projects[p] = on;
}

async function run() {
  running.value = true;
  failed.value = [];
  done.value = 0;
  const list = chosenProjects.value;
  const [targetSlots, frames] = plan.value;
  failed.value = await applyToProjects(list, targetSlots, frames, (n) => (done.value = n));
  running.value = false;
  if (!store.connected) {
    emit("close");
    return;
  }
  const ok = list.length - failed.value.length;
  if (ok) {
    notify(
      `"${props.item.name}" written to ${ok === 1 ? "1 project" : `${ok} projects`}; each shows it the next time it loads`,
      "info",
    );
  }
  if (!failed.value.length) emit("close");
}

function onKey(e: KeyboardEvent) {
  if (e.key === "Escape" && !running.value) emit("close");
}
onMounted(() => window.addEventListener("keydown", onKey));
onBeforeUnmount(() => window.removeEventListener("keydown", onKey));
</script>

<template>
  <div class="backdrop" @click.self="!running && emit('close')">
    <div class="dialog panel" role="dialog" aria-modal="true">
      <h2>Apply "{{ item.name }}" to projects</h2>

      <div class="section">
        <div class="label">{{ isSet ? "Parts of the set" : "Put the frame in" }}</div>
        <div v-if="isSet" class="choices">
          <label><input v-model="groups.startup" type="checkbox" /> Startup (2 frames)</label>
          <label><input v-model="groups.saver" type="checkbox" /> Screen saver (4 frames)</label>
        </div>
        <template v-else>
          <div class="choices">
            <button class="small" @click="pickSlots(STARTUP_SLOTS)">All startup</button>
            <button class="small" @click="pickSlots(SCREEN_SAVER_SLOTS)">All screen saver</button>
          </div>
          <div class="choices">
            <label v-for="slot in [...STARTUP_SLOTS, ...SCREEN_SAVER_SLOTS]" :key="slot">
              <input v-model="slots[slot]" type="checkbox" /> {{ slotName(slot) }}
            </label>
          </div>
        </template>
      </div>

      <div class="section">
        <div class="head">
          <span class="label">Projects</span>
          <div class="spacer" />
          <button class="small" @click="pickProjects(true)">All</button>
          <button class="small" @click="pickProjects(false)">None</button>
        </div>
        <div class="projects">
          <label v-for="p in PROJECTS" :key="p" :class="{ current: store.status?.project === p }">
            <input v-model="projects[p]" type="checkbox" />
            <span class="mono">{{ String(p).padStart(2, "0") }}</span>
            <span class="name">{{ projectName(p) }}</span>
            <span v-if="store.status?.project === p" class="tag">current</span>
          </label>
        </div>
      </div>

      <p class="muted note">
        The images go straight into each project's folder; the unit stays on the project it is on. The images they
        replace are kept on this computer.
      </p>

      <ul v-if="failed.length" class="failed">
        <li v-for="[p, why] in failed" :key="p">{{ projectName(p) }}: {{ why }}</li>
      </ul>

      <div class="buttons">
        <span v-if="running" class="progress muted">
          <span class="spinner" /> Writing {{ done + 1 > chosenProjects.length ? chosenProjects.length : done + 1 }} of
          {{ chosenProjects.length }}…
        </span>
        <div class="spacer" />
        <button :disabled="running" @click="emit('close')">{{ failed.length ? "Close" : "Cancel" }}</button>
        <button class="primary" :disabled="!ready || store.pending > 0" @click="run">
          Apply to {{ chosenProjects.length === 1 ? "1 project" : `${chosenProjects.length} projects` }}
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
  width: 560px;
  max-height: calc(100vh - 40px);
  overflow: auto;
  padding: 20px;
  box-shadow: 0 20px 60px rgba(0, 0, 0, 0.5);
}

h2 {
  margin: 0 0 14px;
  font-size: 16px;
}

.section {
  margin-bottom: 14px;
}

.head,
.choices,
.buttons {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
}

.choices {
  margin-top: 8px;
}

.choices label {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-right: 8px;
  font-size: 13px;
}

.projects {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 2px 14px;
  margin-top: 8px;
}

.projects label {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 3px 0;
  font-size: 13px;
  min-width: 0;
}

.projects .name {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.tag {
  color: var(--accent);
  font-size: 11px;
}

.note {
  margin: 0;
  font-size: 12px;
}

.failed {
  margin: 10px 0 0;
  padding-left: 18px;
  color: var(--danger);
  font-size: 12px;
}

.buttons {
  margin-top: 18px;
}

.progress {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 12px;
}

.spacer {
  flex: 1;
}

button.small {
  padding: 4px 10px;
  font-size: 13px;
}
</style>
