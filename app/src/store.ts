// App state shared by all views, and the actions that talk to the device.
import { reactive } from "vue";
import { api, errorText, type Pad, type PortInfo, type Status } from "./api";

export type Tab = "samples" | "patterns" | "settings";

interface Toast {
  id: number;
  text: string;
}

const POLL_MS = 2000;

export const store = reactive({
  ports: [] as PortInfo[],
  scanning: false,
  connecting: false,
  connected: false,
  status: null as Status | null,
  projects: [] as string[],
  pads: [] as Pad[],
  patterns: [] as boolean[],
  padsLoading: false,
  patternsLoading: false,
  switchingProject: false,
  tab: "samples" as Tab,
  bank: 0,
  selectedPad: null as number | null,
  selectedPattern: null as number | null,
  /** Bumped by Refresh so views drop cached device data. */
  generation: 0,
  toasts: [] as Toast[],
});

let toastId = 0;
let pollTimer: number | undefined;
let lastPollError = "";

export function notify(text: string) {
  const id = ++toastId;
  store.toasts.push({ id, text });
  window.setTimeout(() => dismiss(id), 6000);
}

export function dismiss(id: number) {
  const i = store.toasts.findIndex((t) => t.id === id);
  if (i >= 0) store.toasts.splice(i, 1);
}

export async function scanPorts() {
  store.scanning = true;
  try {
    store.ports = await api.listPorts();
  } catch (e) {
    notify(errorText(e));
  } finally {
    store.scanning = false;
  }
}

export async function connect(port: string | null) {
  store.connecting = true;
  try {
    await api.connect(port);
    store.connected = true;
    await loadProject();
    schedulePoll();
  } catch (e) {
    notify(errorText(e));
    await api.disconnect().catch(() => {});
    store.connected = false;
  } finally {
    store.connecting = false;
  }
}

/** Pick up a connection the backend still holds, for example after a page reload. */
export async function resume() {
  try {
    await api.status();
  } catch {
    return;
  }
  store.connected = true;
  try {
    await loadProject();
    schedulePoll();
  } catch (e) {
    handleError(e);
  }
}

export async function disconnect() {
  window.clearTimeout(pollTimer);
  await api.disconnect().catch(() => {});
  resetDevice();
}

function resetDevice() {
  store.connected = false;
  store.status = null;
  store.projects = [];
  store.pads = [];
  store.patterns = [];
  store.selectedPad = null;
  store.selectedPattern = null;
}

/** Reload everything that depends on the current project. */
export async function loadProject() {
  store.generation++;
  store.status = await api.status();
  store.projects = await api.projectNames();
  await loadPads();
  store.patterns = [];
  if (store.tab === "patterns") await loadPatterns();
}

export async function loadPads() {
  store.padsLoading = true;
  try {
    store.pads = await api.pads();
  } catch (e) {
    handleError(e);
  } finally {
    store.padsLoading = false;
  }
}

export async function loadPatterns() {
  store.patternsLoading = true;
  try {
    store.patterns = await api.patterns();
  } catch (e) {
    handleError(e);
  } finally {
    store.patternsLoading = false;
  }
}

export async function refresh() {
  try {
    await loadProject();
  } catch (e) {
    handleError(e);
  }
}

export async function selectProject(project: number) {
  if (store.status?.project === project) return;
  store.switchingProject = true;
  try {
    await api.selectProject(project);
    store.selectedPad = null;
    store.selectedPattern = null;
    await loadProject();
  } catch (e) {
    handleError(e);
  } finally {
    store.switchingProject = false;
  }
}

export function handleError(e: unknown) {
  const text = errorText(e);
  if (text.includes("connection to the SP-404MKII was lost") || text === "not connected") {
    window.clearTimeout(pollTimer);
    resetDevice();
  }
  notify(text);
}

function schedulePoll() {
  window.clearTimeout(pollTimer);
  pollTimer = window.setTimeout(poll, POLL_MS);
}

/** Follow changes made on the device itself, such as selecting another project. */
async function poll() {
  if (!store.connected) return;
  try {
    const status = await api.status();
    lastPollError = "";
    const projectChanged = store.status !== null && status.project !== store.status.project;
    store.status = status;
    if (projectChanged && !store.switchingProject) {
      store.selectedPad = null;
      store.selectedPattern = null;
      await loadProject();
    }
  } catch (e) {
    const text = errorText(e);
    if (text !== lastPollError) handleError(e);
    lastPollError = text;
  }
  if (store.connected) schedulePoll();
}
