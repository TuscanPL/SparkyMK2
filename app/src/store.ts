// App state shared by all views, and the actions that talk to the device.
import { reactive } from "vue";
import {
  api,
  errorText,
  type CardListing,
  type Volume,
  type Pad,
  type PadDetail,
  type PadOperation,
  type PadState,
  type PortInfo,
  type ScreenImage,
  type Status,
  type Waveform,
} from "./api";
import { bankOf, bpm, padLabel } from "./format";

export type Tab = "samples" | "patterns" | "screens" | "files" | "settings";

interface Toast {
  id: number;
  text: string;
  kind: "error" | "info";
}

export interface DialogButton {
  label: string;
  value: string;
  kind?: "primary" | "danger";
}

interface Dialog {
  title: string;
  message: string;
  buttons: DialogButton[];
  resolve: (value: string | null) => void;
}

const POLL_MS = 2000;
const WAVE_POINTS = 2048;
const PREFS_KEY = "sparkymk2.prefs";

const savedPrefs = JSON.parse(localStorage.getItem(PREFS_KEY) ?? "{}");

export const store = reactive({
  ports: [] as PortInfo[],
  scanning: false,
  connecting: false,
  connected: false,
  status: null as Status | null,
  projects: [] as string[],
  pads: [] as Pad[],
  patterns: [] as boolean[],
  screens: [] as ScreenImage[],
  card: null as CardListing | null,
  cardLoading: false,
  /** Bytes moved and expected while a transfer runs. */
  transfer: null as { name: string; done: number; total: number } | null,
  padsLoading: false,
  patternsLoading: false,
  screensLoading: false,
  switchingProject: false,
  tab: "samples" as Tab,
  bank: 0,
  selectedPad: null as number | null,
  selectedPattern: null as number | null,
  detail: null as PadDetail | null,
  /** Bumped on every read-back, so controls rebuild from the device's values. */
  detailRevision: 0,
  detailLoading: false,
  waveform: null as Waveform | null,
  waveLoading: false,
  /** Pads with an import or other long operation running. */
  working: {} as Record<number, string>,
  /** Edits in flight. */
  pending: 0,
  dialog: null as Dialog | null,
  prefs: {
    detectBpm: savedPrefs.detectBpm ?? true,
    importOpen: savedPrefs.importOpen ?? true,
    /** Fetch every sound in a folder as it opens, trading a slower open for instant browsing. */
    preloadFolders: savedPrefs.preloadFolders ?? false,
    bpmRange: savedPrefs.bpmRange ?? 0,
  },
  toasts: [] as Toast[],
});

let toastId = 0;
let pollTimer: number | undefined;
let lastPollError = "";
let detailRequest = 0;
/** Waveforms by project, pad and file size. Edits that rewrite audio drop their entry. */
const waveCache = new Map<string, Waveform>();

export function notify(text: string, kind: Toast["kind"] = "error") {
  const id = ++toastId;
  store.toasts.push({ id, text, kind });
  window.setTimeout(() => dismiss(id), kind === "error" ? 7000 : 4000);
}

export function dismiss(id: number) {
  const i = store.toasts.findIndex((t) => t.id === id);
  if (i >= 0) store.toasts.splice(i, 1);
}

export function savePrefs() {
  localStorage.setItem(PREFS_KEY, JSON.stringify(store.prefs));
}

/** Show a dialog; resolves with the chosen button's value, or null when dismissed. */
export function ask(title: string, message: string, buttons: DialogButton[]): Promise<string | null> {
  return new Promise((resolve) => {
    store.dialog = { title, message, buttons, resolve };
  });
}

export function closeDialog(value: string | null) {
  const dialog = store.dialog;
  store.dialog = null;
  dialog?.resolve(value);
}

async function confirmAction(title: string, message: string, action: string): Promise<boolean> {
  const answer = await ask(title, message, [
    { label: "Cancel", value: "cancel" },
    { label: action, value: "ok", kind: "danger" },
  ]);
  return answer === "ok";
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
  store.screens = [];
  store.card = null;
  store.selectedPad = null;
  store.selectedPattern = null;
  store.detail = null;
  store.waveform = null;
  waveCache.clear();
}

/** Reload everything that depends on the current project. */
export async function loadProject() {
  waveCache.clear();
  store.status = await api.status();
  store.projects = await api.projectNames();
  await loadPads();
  store.patterns = [];
  store.screens = [];
  if (store.selectedPad !== null) await selectPad(store.selectedPad);
  if (store.tab === "patterns") await loadPatterns();
  if (store.tab === "screens") await loadScreens();
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

export async function loadCard(volume: Volume, path: string) {
  store.cardLoading = true;
  try {
    store.card = await api.listVolume(volume, path);
  } catch (e) {
    handleError(e);
  } finally {
    store.cardLoading = false;
  }
}

/** Re-read the directory now showing, after something on the device changed. */
export async function reloadCard() {
  if (store.card) await loadCard(store.card.volume, store.card.path);
}

/** Run a card operation, showing the saving indicator and clearing progress after. */
async function onCard<T>(run: () => Promise<T>): Promise<T | null> {
  store.pending++;
  try {
    return await run();
  } catch (e) {
    handleError(e);
    return null;
  } finally {
    store.pending--;
    store.transfer = null;
  }
}

function baseName(path: string): string {
  return path.split(/[/\\]/).pop() ?? path;
}

/** Copy local files onto a volume in `dir`; stops at the first failure. */
export async function uploadToCard(locals: string[], volume: Volume, dir: string) {
  const where = volume === "card" ? "the card" : "the device";
  for (const local of locals) {
    const name = baseName(local);
    const remote = dir ? `${dir}/${name}` : name;
    if ((await onCard(() => api.uploadFile(local, volume, remote))) === null) break;
  }
  await reloadCard();
  notify(
    locals.length === 1
      ? `${baseName(locals[0])} copied to ${where}`
      : `${locals.length} files copied to ${where}`,
    "info",
  );
}

export async function downloadFromCard(volume: Volume, path: string, isDir: boolean, into: string) {
  const bytes = await onCard(() =>
    isDir ? api.downloadFolder(volume, path, into) : api.downloadFile(volume, path, into),
  );
  if (bytes !== null) notify(`${baseName(path)} saved`, "info");
}

export async function deleteFromCard(volume: Volume, path: string, isDir: boolean) {
  if ((await onCard(() => api.deletePath(volume, path, isDir))) === null) return;
  await reloadCard();
  notify(`${baseName(path)} deleted`, "info");
}

export async function renameOnCard(volume: Volume, path: string, name: string) {
  if ((await onCard(() => api.renamePath(volume, path, name))) === null) return;
  await reloadCard();
}

export async function createCardFolder(volume: Volume, parent: string, name: string) {
  if ((await onCard(() => api.createDir(volume, parent, name))) === null) return;
  await reloadCard();
}

export async function loadScreens() {
  store.screensLoading = true;
  try {
    store.screens = (await api.screens()).slots;
  } catch (e) {
    handleError(e);
  } finally {
    store.screensLoading = false;
  }
}

/** Store one display image in the current project's PICTURE folder. */
export async function applyScreen(slot: string, rows: number[]): Promise<boolean> {
  store.pending++;
  try {
    replaceScreen(await api.setScreen(slot, rows));
    return true;
  } catch (e) {
    handleError(e);
    return false;
  } finally {
    store.pending--;
  }
}

/** Put back the image a slot held before this app first changed it. */
export async function restoreScreen(slot: string): Promise<boolean> {
  store.pending++;
  try {
    replaceScreen(await api.restoreScreen(slot));
    return true;
  } catch (e) {
    handleError(e);
    return false;
  } finally {
    store.pending--;
  }
}

function replaceScreen(screen: ScreenImage) {
  const i = store.screens.findIndex((s) => s.slot === screen.slot);
  if (i >= 0) store.screens[i] = screen;
  else store.screens.push(screen);
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
    store.detail = null;
    store.waveform = null;
    await loadProject();
  } catch (e) {
    handleError(e);
  } finally {
    store.switchingProject = false;
  }
}

/** Select a pad and load its details and waveform. */
export async function selectPad(index: number | null) {
  store.selectedPad = index;
  const token = ++detailRequest;
  store.detail = null;
  store.waveform = null;
  if (index === null) return;
  store.detailLoading = true;
  try {
    const detail = await api.padDetail(index);
    if (token !== detailRequest) return;
    store.detail = detail;
    store.detailLoading = false;
    if (detail.sample) await loadWaveform(index, token);
  } catch (e) {
    if (token === detailRequest) handleError(e);
  } finally {
    if (token === detailRequest) store.detailLoading = false;
  }
}

async function loadWaveform(index: number, token: number) {
  const key = `${store.status?.project}:${index}:${store.pads[index]?.fileSize}`;
  const cached = waveCache.get(key);
  if (cached) {
    store.waveform = cached;
    return;
  }
  store.waveLoading = true;
  try {
    const wf = await api.waveform(index, WAVE_POINTS);
    waveCache.set(key, wf);
    if (token === detailRequest) store.waveform = wf;
  } finally {
    if (token === detailRequest) store.waveLoading = false;
  }
}

function dropWaveform(index: number) {
  for (const key of [...waveCache.keys()]) if (key.split(":")[1] === String(index)) waveCache.delete(key);
}

export function handleError(e: unknown) {
  const text = errorText(e);
  console.error(text);
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
    const previous = store.status;
    const projectChanged = previous !== null && status.project !== previous.project;
    // Edits made on the device happen behind its menu, and it tells us nothing about
    // them; once the menu closes, everything we hold may be out of date.
    const leftMenu =
      previous?.workingMode === 4 && status.workingMode !== null && status.workingMode !== 4;
    store.status = status;
    if (!store.switchingProject) {
      if (projectChanged) {
        store.selectedPad = null;
        store.selectedPattern = null;
        await loadProject();
      } else if (leftMenu) {
        // The same project, so the selection stays; only its contents are re-read.
        await loadProject();
        if (store.card) await reloadCard();
      }
    }
  } catch (e) {
    const text = errorText(e);
    if (text !== lastPollError) handleError(e);
    lastPollError = text;
  }
  if (store.connected) schedulePoll();
}

// ---- Editing ----

/** Why pads in a bank can't be edited right now, if they can't. */
export function editBlock(index: number | null): string | null {
  if (!store.status || index === null) return "Not connected";
  if (store.status.workingMode === 4) return "The SP-404MKII is showing a menu";
  const bank = store.status.banks[bankOf(index)];
  if (bank?.protected) return `Bank ${bank.letter} is protected`;
  return null;
}

async function edit<T>(work: () => Promise<T>): Promise<T | undefined> {
  store.pending++;
  try {
    return await work();
  } catch (e) {
    handleError(e);
    return undefined;
  } finally {
    store.pending--;
  }
}

/** Take a pad's state read back from the device. */
export function applyPad(state: PadState) {
  store.pads[state.pad.index] = state.pad;
  if (store.selectedPad === state.detail.index) store.detail = state.detail;
  store.detailRevision++;
}

export async function setPadParam(index: number, name: string, value: number) {
  const state = await edit(() => api.setPadParam(index, name, value));
  if (state) applyPad(state);
  else resyncDetail();
}

/** Give controls a fresh copy of the details, so drafts snap back after a failed edit. */
function resyncDetail() {
  store.detailRevision++;
}

export async function setChopPoints(index: number, points: number[]) {
  const state = await edit(() => api.setChopPoints(index, points));
  if (state) applyPad(state);
  else resyncDetail();
}

export async function renameSample(index: number, name: string) {
  const state = await edit(() => api.renameSample(index, name));
  if (state) applyPad(state);
  else resyncDetail();
}

const OPERATIONS: Record<PadOperation, { title: string; message: string; action: string; busy: string }> = {
  truncate: {
    title: "Truncate sample?",
    message: "Audio before Start and after End is removed from the sample on the device. This can't be undone.",
    action: "Truncate",
    busy: "Truncating",
  },
  normalize: {
    title: "Normalize sample?",
    message: "The sample's level is raised to full scale on the device. This can't be undone.",
    action: "Normalize",
    busy: "Normalizing",
  },
  delete: {
    title: "Delete sample?",
    message:
      "The sample is erased from the pad and from the project on the device. Nothing on the SD card is touched. This can't be undone.",
    action: "Delete",
    busy: "Deleting",
  },
};

export async function padOperation(index: number, operation: PadOperation) {
  const op = OPERATIONS[operation];
  const name = store.pads[index]?.name;
  if (!(await confirmAction(op.title, `${padLabel(index)} “${name}”: ${op.message}`, op.action))) return;
  store.working[index] = op.busy;
  try {
    const state = await edit(() => api.padOperation(index, operation));
    if (!state) return;
    dropWaveform(index);
    applyPad(state);
    if (store.selectedPad === index) await selectPad(index);
  } finally {
    delete store.working[index];
  }
}

export async function moveSample(from: number, to: number) {
  if (from === to || !store.pads[from]?.hasSample) return;
  let exchange = false;
  if (store.pads[to]?.hasSample) {
    const answer = await ask(
      `${padLabel(to)} already holds a sample`,
      `Replace “${store.pads[to].name}” with “${store.pads[from].name}”, or swap the two pads? Replacing erases ${padLabel(to)}'s sample.`,
      [
        { label: "Cancel", value: "cancel" },
        { label: "Swap", value: "swap", kind: "primary" },
        { label: "Replace", value: "replace", kind: "danger" },
      ],
    );
    if (answer !== "swap" && answer !== "replace") return;
    exchange = answer === "swap";
  }
  store.working[from] = "Moving";
  store.working[to] = "Moving";
  try {
    const done = await edit(async () => {
      await api.moveSample(from, to, exchange);
      return true;
    });
    if (!done) return;
    waveCache.clear();
    await loadPads();
    if (store.selectedPad === from || store.selectedPad === to) await selectPad(to);
  } finally {
    delete store.working[from];
    delete store.working[to];
  }
}

/** Import files onto consecutive pads, starting at `start`, in file name order. */
export async function importFiles(paths: string[], start: number) {
  const fileName = (path: string) => path.split(/[\\/]/).pop() ?? path;
  const sorted = [...paths].sort((a, b) =>
    fileName(a).localeCompare(fileName(b), undefined, { numeric: true, sensitivity: "base" }),
  );
  const targets = sorted.slice(0, 160 - start).map((path, i) => ({ path, index: start + i }));
  if (!targets.length) return;
  const blocked = targets.map((t) => editBlock(t.index)).find(Boolean);
  if (blocked) {
    notify(blocked);
    return;
  }
  const occupied = targets.filter((t) => store.pads[t.index]?.hasSample);
  if (occupied.length) {
    const which = occupied.map((t) => padLabel(t.index)).join(", ");
    const ok = await confirmAction(
      occupied.length === 1 ? `Replace ${which}?` : `Replace ${occupied.length} samples?`,
      `${which} already ${occupied.length === 1 ? "holds a sample" : "hold samples"}. Importing replaces ${occupied.length === 1 ? "it" : "them"}. This can't be undone.`,
      "Replace",
    );
    if (!ok) return;
  }
  if (paths.length > targets.length) notify(`Only ${targets.length} of ${paths.length} files fit before J16.`, "info");
  for (const t of targets) store.working[t.index] = "Waiting";
  for (const t of targets) {
    store.working[t.index] = "Importing";
    try {
      const result = await edit(() => api.importAudio(t.index, t.path, store.prefs.detectBpm, store.prefs.bpmRange));
      if (!result) continue;
      dropWaveform(t.index);
      applyPad(result.state);
      if (store.selectedPad === t.index) await selectPad(t.index);
      if (store.prefs.detectBpm && result.detectedBpm === null) {
        notify(`${padLabel(t.index)}: no tempo detected`, "info");
      }
    } finally {
      delete store.working[t.index];
    }
  }
  if (store.selectedPad === null) await selectPad(targets[0].index);
}

/** Import one sound that is already on the device onto a pad. */
export async function importFromDevice(index: number, volume: Volume, remote: string) {
  const blocked = editBlock(index);
  if (blocked) {
    notify(blocked);
    return;
  }
  const name = remote.split("/").pop() ?? remote;
  if (store.pads[index]?.hasSample) {
    const ok = await confirmAction(
      `Replace ${padLabel(index)}?`,
      `${padLabel(index)} already holds a sample. Importing ${name} replaces it. This can't be undone.`,
      "Replace",
    );
    if (!ok) return;
  }
  store.working[index] = "Importing";
  try {
    const result = await edit(() =>
      api.importFromDevice(index, volume, remote, store.prefs.detectBpm, store.prefs.bpmRange),
    );
    if (!result) return;
    dropWaveform(index);
    applyPad(result.state);
    if (store.selectedPad === index || store.selectedPad === null) await selectPad(index);
    if (store.prefs.detectBpm && result.detectedBpm === null) {
      notify(`${padLabel(index)}: no tempo detected`, "info");
    }
  } finally {
    delete store.working[index];
  }
}

export async function analyzeBpm(index: number, mode: "detect" | "length") {
  store.working[index] = mode === "detect" ? "Detecting BPM" : "Setting BPM";
  try {
    const result = await edit(() => api.analyzeBpm(index, mode, store.prefs.bpmRange));
    if (!result) return;
    applyPad(result.state);
    notify(result.bpm === null ? "No tempo detected" : `BPM set to ${bpm(result.bpm)}`, "info");
  } finally {
    delete store.working[index];
  }
}

export async function setGlobalParam(name: string, value: number) {
  const status = await edit(() => api.setGlobalParam(name, value));
  if (status) store.status = status;
}

export async function renameProject(project: number, name: string) {
  const status = await edit(async () => {
    store.projects = await api.renameProject(project, name);
    return api.status();
  });
  if (status) store.status = status;
}
