// Exporting pads as WAV files, and putting such an export back on the same pads.
//
// Files are named after their pads (`C05 Kick.wav`) and a bank or project export carries
// `sparkymk2.json` with every pad's settings, so a restore brings back the arrangement,
// gaps included, and the settings with it. Only SparkyMK2 reads that file: the device's
// own IMPORT still sees plain WAVs, and fills pads in order.
import { open } from "@tauri-apps/plugin-dialog";
import { api, errorText, type Place } from "./api";
import { BANK_LETTERS, bankOf, exportStamp, fsSafe, padFromName, padLabel } from "./format";
import { ask, dropWaveform, editBlock, loadPads, notify, selectPad, store } from "./store";

/** On the card, exports land in IMPORT, where the device's own IMPORT browser looks. */
const CARD_FOLDER = "IMPORT";

export type ExportScope = "pad" | "bank" | "project";

/** Ask where an export goes: the SD card, or a folder picked on the computer. */
async function choosePlace(title: string): Promise<{ target: Place; base: string } | null> {
  const where = await ask(
    title,
    "Save the WAVs to the SD card, in IMPORT where the device's own IMPORT finds them, or to a folder on this computer?",
    [
      { label: "Cancel", value: "cancel" },
      { label: "Computer…", value: "local" },
      { label: "SD card", value: "card", kind: "primary" },
    ],
  );
  if (where === "card") return { target: "card", base: CARD_FOLDER };
  if (where !== "local") return null;
  const picked = await open({ directory: true, title: "Export into" });
  return typeof picked === "string" ? { target: "local", base: picked } : null;
}

/** A child of `base`; "/" works on every system the app runs on, and on the card. */
function within(base: string, name: string): string {
  return `${base.replace(/[/\\]+$/, "")}/${name}`;
}

function sounds(n: number): string {
  return n === 1 ? "1 sound" : `${n} sounds`;
}

/**
 * Export the pads in `indices` that hold a sample. A single pad's WAV goes straight into
 * the folder picked; a bank or project gets a folder of its own, named after it and the
 * time, so an export never mixes with an older one or leaves its files behind.
 */
export async function exportSounds(scope: ExportScope, indices: number[]) {
  const used = indices.filter((i) => store.pads[i]?.hasSample);
  if (!used.length) {
    notify("Nothing to export: those pads are empty.", "info");
    return;
  }
  const status = store.status;
  if (!status) return;
  const project = fsSafe(store.projects[status.project - 1] || `PROJECT_${String(status.project).padStart(2, "0")}`);
  const bank = BANK_LETTERS[bankOf(used[0])];
  const what = scope === "pad" ? `pad ${padLabel(used[0])}` : scope === "bank" ? `bank ${bank}` : "the whole project";
  const place = await choosePlace(`Export ${what}`);
  if (!place) return;

  const stamp = exportStamp();
  const folder =
    scope === "pad"
      ? place.base
      : within(place.base, scope === "bank" ? `${project} ${bank} ${stamp}` : `${project} ${stamp}`);
  store.pending++;
  try {
    const summary = await api.exportPads(used, place.target, folder, scope !== "pad");
    const where = place.target === "card" ? `the SD card, ${folder}` : folder;
    notify(`${sounds(summary.written)} exported to ${where}`, "info");
  } catch (e) {
    notify(errorText(e));
  } finally {
    store.pending--;
    store.transfer = null;
  }
}

/** Put an export back: every sound on the pad it came from, with its settings. */
export async function restoreExport(target: Place, folder: string) {
  let sheet;
  try {
    sheet = await api.readExport(target, folder);
  } catch (e) {
    notify(errorText(e));
    return;
  }
  const pads = sheet.pads.map((p) => padFromName(p.pad)).filter((i): i is number => i !== null);
  if (!pads.length) {
    notify("That export holds no pads.", "info");
    return;
  }
  const blocked = pads.map(editBlock).find(Boolean);
  if (blocked) {
    notify(blocked);
    return;
  }
  const occupied = pads.filter((i) => store.pads[i]?.hasSample);
  const replaces = occupied.length
    ? ` It replaces ${occupied.length === 1 ? "the sample" : `${occupied.length} samples`} already on ${occupied.map(padLabel).join(", ")}. This can't be undone.`
    : "";
  const answer = await ask(
    `Restore ${sounds(pads.length)}?`,
    `From ${sheet.project || "an export"}: each sound goes back on its own pad, with its settings.${replaces}`,
    [
      { label: "Cancel", value: "cancel" },
      { label: "Restore", value: "ok", kind: occupied.length ? "danger" : "primary" },
    ],
  );
  if (answer !== "ok") return;

  for (const i of pads) store.working[i] = "Restoring";
  store.pending++;
  try {
    const restored = await api.restorePads(target, folder);
    notify(`${sounds(restored)} restored`, "info");
  } catch (e) {
    notify(errorText(e));
  } finally {
    // Whatever got through, show it: a restore can fail partway, after some pads changed.
    for (const i of pads) {
      delete store.working[i];
      dropWaveform(i);
    }
    await loadPads();
    if (store.selectedPad !== null) await selectPad(store.selectedPad);
    store.pending--;
    store.transfer = null;
  }
}

/** Restore an export kept on the computer, picking its folder. */
export async function restoreFromComputer() {
  const picked = await open({ directory: true, title: "Restore an export" });
  if (typeof picked === "string") await restoreExport("local", picked);
}
