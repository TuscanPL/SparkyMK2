// Listening to a sound that lives on the device.
//
// The protocol can only preview a pad, so anything else is decoded by the backend into a
// WAV and played here, through the computer's speakers rather than the SP-404MKII.
import { reactive } from "vue";
import { api, errorText, type Volume } from "./api";
import { notify } from "./store";

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

let context: AudioContext | undefined;
let source: AudioBufferSourceNode | undefined;
/** Bumped per request, so a slow fetch cannot start over a newer one. */
let token = 0;
/** A keyboard audition waiting for the keys to settle. */
let pending: number | undefined;

/** Pause after the last arrow key before fetching, so holding one down skips ahead
 * instead of queueing a read over USB for every sound it passes. */
const AUDITION_DELAY_MS = 180;

// Decoded sounds, most recently heard last. Browsing a folder goes back and forth, and a
// cached sound plays at once instead of being read over USB again. Capped by memory: a
// decoded buffer is 32-bit float, and previews are cut at 30 s, so ~11 MB at most each.
const CACHE_BYTES = 64 * 1024 * 1024;
const cache = new Map<string, AudioBuffer>();
let cacheBytes = 0;

/** By size as well as path, like the waveform cache: a file overwritten in the Files tab
 * almost always changes size, so it is fetched again rather than replayed stale. */
function keyOf(volume: Volume, path: string, size: number): string {
  return `${volume}:${path}:${size}`;
}

function sizeOf(buffer: AudioBuffer): number {
  return buffer.length * buffer.numberOfChannels * 4;
}

function remember(key: string, buffer: AudioBuffer) {
  const old = cache.get(key);
  if (old) {
    cache.delete(key);
    cacheBytes -= sizeOf(old);
  }
  cache.set(key, buffer);
  cacheBytes += sizeOf(buffer);
  // A Map iterates oldest first, so this drops the sounds heard longest ago.
  for (const [k, b] of cache) {
    if (cacheBytes <= CACHE_BYTES) break;
    cache.delete(k);
    cacheBytes -= sizeOf(b);
  }
}

function recall(key: string): AudioBuffer | undefined {
  const buffer = cache.get(key);
  if (buffer) {
    cache.delete(key);
    cache.set(key, buffer);
  }
  return buffer;
}

export function stopPreview() {
  token++;
  window.clearTimeout(pending);
  pending = undefined;
  source?.stop();
  source?.disconnect();
  source = undefined;
  preview.playing = null;
  preview.loading = null;
}

function start(buffer: AudioBuffer, path: string, mine: number) {
  context ??= new AudioContext();
  source = context.createBufferSource();
  source.buffer = buffer;
  source.connect(context.destination);
  source.onended = () => {
    if (mine === token) {
      preview.playing = null;
      source = undefined;
    }
  };
  preview.loading = null;
  preview.playing = path;
  source.start();
}

/** Fetch a sound from the device and play it; clicking the one already playing stops it. */
export async function playPreview(volume: Volume, path: string, size = 0) {
  if (preview.playing === path || preview.loading === path) {
    stopPreview();
    return;
  }
  stopPreview();
  const mine = ++token;
  const key = keyOf(volume, path, size);
  const cached = recall(key);
  if (cached) {
    start(cached, path, mine);
    return;
  }
  preview.loading = path;
  try {
    const wav = await api.previewAudio(volume, path);
    context ??= new AudioContext();
    // decodeAudioData detaches the buffer it is given, so hand it a copy.
    const buffer = await context.decodeAudioData(wav.slice(0));
    // Keep it even if the listener has moved on: browsing tends to come back.
    remember(key, buffer);
    if (mine !== token) return;
    start(buffer, path, mine);
  } catch (e) {
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
export function auditionPreview(volume: Volume, path: string, size = 0) {
  stopPreview();
  if (cache.has(keyOf(volume, path, size))) {
    playPreview(volume, path, size);
    return;
  }
  pending = window.setTimeout(() => {
    pending = undefined;
    playPreview(volume, path, size);
  }, AUDITION_DELAY_MS);
}
