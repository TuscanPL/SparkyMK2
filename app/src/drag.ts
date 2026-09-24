// Dragging pads onto other pads, and dropping audio files from the file manager.
import { reactive } from "vue";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { editBlock, importFiles, importFromDevice, moveSample, store, uploadToCard } from "./store";
import type { Volume } from "./api";

const THRESHOLD = 5;
const BANK_HOVER_MS = 350;

export const drag = reactive({
  /** Pad being dragged, or -1. */
  from: -1,
  x: 0,
  y: 0,
  /** Pad under the pointer while dragging a pad or files, or -1. */
  over: -1,
  /** Files are being dragged over the window. */
  files: false,
  /** A sound being dragged out of the card browser onto a pad. */
  sound: null as { volume: Volume; path: string; name: string } | null,
});

let justDragged = false;
let bankTimer: number | undefined;
let hoveredBank = -1;

/** Find the pad and bank buttons under a point (CSS pixels). */
function hitTest(x: number, y: number) {
  const el = document.elementFromPoint(x, y);
  const pad = el?.closest<HTMLElement>("[data-pad]");
  const bank = el?.closest<HTMLElement>("[data-bank]");
  return {
    pad: pad ? Number(pad.dataset.pad) : -1,
    bank: bank ? Number(bank.dataset.bank) : -1,
  };
}

/** Switch banks when the pointer rests on a bank button, so pads can move across banks. */
function hoverBank(bank: number) {
  if (bank === hoveredBank) return;
  hoveredBank = bank;
  window.clearTimeout(bankTimer);
  if (bank >= 0 && bank !== store.bank) {
    bankTimer = window.setTimeout(() => (store.bank = bank), BANK_HOVER_MS);
  }
}

function track(x: number, y: number) {
  drag.x = x;
  drag.y = y;
  const hit = hitTest(x, y);
  drag.over = hit.pad;
  hoverBank(hit.bank);
}

function endHover() {
  drag.over = -1;
  hoverBank(-1);
}

/** Start watching a pointer press on a pad; it becomes a drag once it moves. */
export function pressPad(event: PointerEvent, index: number) {
  if (event.button !== 0 || !store.pads[index]?.hasSample || editBlock(index)) return;
  const startX = event.clientX;
  const startY = event.clientY;
  const move = (e: PointerEvent) => {
    if (drag.from < 0 && Math.hypot(e.clientX - startX, e.clientY - startY) < THRESHOLD) return;
    drag.from = index;
    track(e.clientX, e.clientY);
  };
  const up = () => {
    window.removeEventListener("pointermove", move);
    window.removeEventListener("pointerup", up);
    if (drag.from < 0) return;
    const { from, over } = drag;
    drag.from = -1;
    endHover();
    justDragged = true;
    window.setTimeout(() => (justDragged = false), 0);
    if (over >= 0 && over !== from) {
      if (editBlock(over)) return;
      moveSample(from, over);
    }
  };
  window.addEventListener("pointermove", move);
  window.addEventListener("pointerup", up);
}

/** Start dragging a sound off the card browser; it becomes a drag once it moves. */
export function pressSound(event: PointerEvent, sound: { volume: Volume; path: string; name: string }) {
  if (event.button !== 0) return;
  const startX = event.clientX;
  const startY = event.clientY;
  const move = (e: PointerEvent) => {
    if (!drag.sound && Math.hypot(e.clientX - startX, e.clientY - startY) < THRESHOLD) return;
    drag.sound = sound;
    track(e.clientX, e.clientY);
  };
  const up = () => {
    window.removeEventListener("pointermove", move);
    window.removeEventListener("pointerup", up);
    if (!drag.sound) return;
    const { over } = drag;
    drag.sound = null;
    endHover();
    justDragged = true;
    window.setTimeout(() => (justDragged = false), 0);
    if (over >= 0 && !editBlock(over)) importFromDevice(over, sound.volume, sound.path);
  };
  window.addEventListener("pointermove", move);
  window.addEventListener("pointerup", up);
}

/** Whether a click is the end of a drag and should be ignored. */
export function clickAfterDrag(): boolean {
  return justDragged;
}

/**
 * What divides a file drop's position to give CSS pixels. Positions are typed as physical
 * pixels, and are on Linux and Windows, but macOS hands them over already logical;
 * dividing there sent a drop on a Retina screen to the pad at half its coordinates.
 */
const logicalDrops = navigator.userAgent.includes("Mac OS X");
const dropScale = () => (logicalDrops ? 1 : window.devicePixelRatio);

/** Listen for files dragged in from outside the window. */
export async function listenForFileDrops() {
  await getCurrentWebview().onDragDropEvent((event) => {
    const p = event.payload;
    // The Files tab takes a drop anywhere in the window; Samples needs a pad under it.
    if (store.connected && store.tab === "files") {
      if (p.type === "drop" && p.paths.length && !store.pending) {
        uploadToCard(p.paths, store.card?.volume ?? "card", store.card?.path ?? "");
      }
      drag.files = p.type === "enter" || p.type === "over";
      return;
    }
    const usable = store.connected && store.tab === "samples";
    if (p.type === "leave" || !usable) {
      drag.files = false;
      endHover();
      return;
    }
    const x = p.position.x / dropScale();
    const y = p.position.y / dropScale();
    if (p.type === "enter" || p.type === "over") {
      drag.files = true;
      track(x, y);
      return;
    }
    drag.files = false;
    const { pad } = hitTest(x, y);
    endHover();
    if (pad >= 0 && p.paths.length) importFiles(p.paths, pad);
  });
}
