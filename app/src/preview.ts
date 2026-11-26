// Listening to a sound that lives on the device.
//
// The protocol can only preview a pad, so anything else is read and decoded by the
// backend and played on this computer's speakers, through the output chosen in Settings.
import { reactive } from "vue";
import { listen } from "@tauri-apps/api/event";
import { api, errorText, logLine, type CardEntry, type Volume } from "./api";
import { notify, store } from "./store";

/** Extensions the backend can turn into something playable; mirrors `files::playable`. */
const PLAYABLE = ["wav", "bwf", "aif", "aiff", "flac", "mp3", "smp"];

export function playable(name: string): boolean {
  const dot = name.lastIndexOf(".");
  return dot > 0 && PLAYABLE.includes(name.slice(dot + 1).toLowerCase());
}

export const preview = reactive({
  /** Path of the sound being fetched, or null. */
  loading: null as string | null,
  /** Path of the sound playing, or null. */
  playing: null as string | null,
});

/** Progress of a folder being preloaded into the cache. */
export const preload = reactive({ running: false, done: 0, total: 0 });

/** Id of the sound the backend is playing for us, from `sound_play`. */
let playingId = 0;
/** Bumped per request, so a slow fetch cannot start over a newer one. */
let token = 0;
/** A keyboard audition waiting for the keys to settle. */
let pending: number | undefined;

/** Pause after the last arrow key before fetching, so holding one down skips ahead
 * instead of queueing a read over USB for every sound it passes. */
const AUDITION_DELAY_MS = 180;

/** The backend keeps decoded sounds (up to 256 MB) so going back to one plays it at
 * once; mirrors `audio::CACHE_BYTES`, for the folder preload to stop before it. */
const CACHE_BYTES = 256 * 1024 * 1024;

// Sounds play from the backend, on the output chosen in Settings, not in the webview:
// webviews cannot pick an output, and on Windows the default is often the SP-404MKII.
type Ended = { id: number; error: string | null };
/** The last sound to end, kept for the case where it ends (fails to open, or is very
 * short) before `sound_play` has even answered with its id. */
let lastEnded: Ended | null = null;

function ended(e: Ended) {
  playingId = 0;
  preview.playing = null;
  if (e.error) {
    logLine("warn", `audio: ${e.error}`);
    notify(e.error);
  }
}

void listen<Ended>("sound-ended", (e) => {
  lastEnded = e.payload;
  if (e.payload.id === playingId) ended(e.payload);
});

export function stopPreview() {
  token++;
  window.clearTimeout(pending);
  pending = undefined;
  if (preview.playing !== null || preview.loading !== null) void api.soundStop();
  playingId = 0;
  preview.playing = null;
  preview.loading = null;
}

/** Fetch a sound from the device and play it; clicking the one already playing stops it. */
export async function playPreview(volume: Volume, path: string, size: number | null = null) {
  if (preview.playing === path || preview.loading === path) {
    stopPreview();
    return;
  }
  stopPreview();
  const mine = ++token;
  preview.loading = path;
  try {
    // Answers at once when the sound is cached.
    await api.soundFetch(volume, path, size);
    if (mine !== token) return;
    const output = store.prefs.audioOutput || null;
    logLine("debug", `audio: play ${path} on ${output ?? "automatic output"}`);
    const id = await api.soundPlay(volume, path, size, output);
    // Stopped or replaced while it was starting: the stop may have gone out first.
    if (mine !== token) {
      if (preview.playing === null && preview.loading === null) void api.soundStop();
      return;
    }
    preview.loading = null;
    preview.playing = path;
    playingId = id;
    if (lastEnded?.id === id) ended(lastEnded);
  } catch (e) {
    logLine("warn", `audio: ${path} did not play: ${errorText(e)}`);
    if (mine === token) {
      preview.loading = null;
      preview.playing = null;
      notify(errorText(e));
    }
  }
}

/**
 * Play a sound reached by keyboard. The previous one stops at once; a sound already heard
 * plays straight away, anything else waits for the keys to settle before it is fetched.
 */
export async function auditionPreview(volume: Volume, path: string, size: number | null = null) {
  stopPreview();
  const mine = token;
  if (await api.soundCached(volume, path, size)) {
    if (mine === token) void playPreview(volume, path, size);
    return;
  }
  if (mine !== token) return;
  pending = window.setTimeout(() => {
    pending = undefined;
    void playPreview(volume, path, size);
  }, AUDITION_DELAY_MS);
}

/** Bumped when a preload should stop, such as when another folder opens. */
let preloadRun = 0;

export function cancelPreload() {
  preloadRun++;
  preload.running = false;
}

/**
 * Fetch every sound in a folder into the cache, so browsing it plays each one at once.
 * One file at a time: the device serves requests in turn, so a sound the listener clicks
 * meanwhile waits behind a single file rather than the whole folder. Stops short of a
 * folder bigger than the cache instead of evicting its own first sounds to fit the last.
 */
export async function preloadFolder(volume: Volume, entries: CardEntry[]) {
  const mine = ++preloadRun;
  const sounds = entries.filter((e) => !e.isDir && playable(e.name));
  preload.total = sounds.length;
  preload.done = 0;
  preload.running = sounds.length > 0;
  let folderBytes = 0;
  for (const entry of sounds) {
    if (mine !== preloadRun) return;
    try {
      folderBytes += await api.soundFetch(volume, entry.path, entry.size);
      if (folderBytes > CACHE_BYTES) break;
    } catch {
      // Too big or not really audio: it simply stays unloaded, and says why if clicked.
    }
    preload.done++;
  }
  if (mine === preloadRun) preload.running = false;
}
