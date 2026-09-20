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

export function stopPreview() {
  token++;
  source?.stop();
  source?.disconnect();
  source = undefined;
  preview.playing = null;
  preview.loading = null;
}

/** Fetch a sound from the device and play it; clicking the one already playing stops it. */
export async function playPreview(volume: Volume, path: string) {
  if (preview.playing === path || preview.loading === path) {
    stopPreview();
    return;
  }
  stopPreview();
  const mine = ++token;
  preview.loading = path;
  try {
    const wav = await api.previewAudio(volume, path);
    if (mine !== token) return;
    context ??= new AudioContext();
    // decodeAudioData detaches the buffer it is given, so hand it a copy.
    const buffer = await context.decodeAudioData(wav.slice(0));
    if (mine !== token) return;
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
  } catch (e) {
    if (mine === token) {
      preview.loading = null;
      preview.playing = null;
      notify(errorText(e));
    }
  }
}
