import { invoke } from "./invoke";
import { Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { writable } from "svelte/store";
import { toMessage } from "../utils/errors";
import { MAX_NOTES_CHARS, rememberPendingNotes } from "../stores/update-notes";

type UpdateState = {
  version: string | null;
  notes: string | null;
  aur: boolean;
  unavailable: boolean;
  checking: boolean;
  checked: boolean;
  installing: boolean;
  nativeNotified: boolean;
  error: string | null;
};

export const updateState = writable<UpdateState>({
  version: null,
  notes: null,
  aur: false,
  unavailable: false,
  checking: false,
  checked: false,
  installing: false,
  nativeNotified: false,
  error: null,
});

let availableUpdate: Update | null = null;
let notifiedVersion: string | null = null;
let checkingTask: Promise<void> | null = null;
let installingTask: Promise<void> | null = null;

export function checkForAppUpdate(): Promise<void> {
  if (installingTask !== null) return Promise.resolve();
  checkingTask ??= performCheck().finally(() => { checkingTask = null; });
  return checkingTask;
}

async function performCheck(): Promise<void> {
  updateState.update((state) => ({ ...state, version: null, notes: null, unavailable: false, checking: true, checked: false, error: null }));
  try {
    const previous = availableUpdate;
    availableUpdate = null;
    if (previous !== null) await previous.close();
    const aur = await invoke<boolean>("is_aur_package");
    const result = await invoke<{ update: ConstructorParameters<typeof Update>[0] | null; unavailable: boolean }>("check_app_update");
    if (result.unavailable) {
      updateState.update((state) => ({ ...state, checked: true, unavailable: true, nativeNotified: false, error: null }));
      return;
    }
    const update = result.update === null ? null : new Update(result.update);
    availableUpdate = update;
    let nativeNotified = update !== null && notifiedVersion === update.version;
    let notificationError: string | null = null;
    if (update !== null && !nativeNotified) {
      try {
        await invoke("notify_update_available", { version: update.version });
        notifiedVersion = update.version;
        nativeNotified = true;
      } catch (error) {
        notificationError = toMessage(error);
      }
    }
    updateState.update((state) => ({ ...state, version: update?.version ?? null, notes: releaseNotes(update), aur, checked: true, nativeNotified, error: notificationError }));
  } catch (error) {
    updateState.update((state) => ({ ...state, checked: false, unavailable: false, nativeNotified: false, error: toMessage(error) }));
  } finally {
    updateState.update((state) => ({ ...state, checking: false }));
  }
}

// The manifest is untrusted: notes are shown as plain text and capped.
function releaseNotes(update: Update | null): string | null {
  const body = update?.body?.trim();
  return body ? body.slice(0, MAX_NOTES_CHARS) : null;
}

export function installAppUpdate(): Promise<void> {
  installingTask ??= performInstall().finally(() => { installingTask = null; });
  return installingTask;
}

async function performInstall(): Promise<void> {
  await checkingTask;
  const update = availableUpdate;
  if (update === null) return;
  updateState.update((state) => ({ ...state, installing: true, error: null }));
  try {
    await rememberPendingNotes(update.version, releaseNotes(update));
    await update.downloadAndInstall();
    availableUpdate = null;
    try {
      await update.close();
    } catch (error) {
      updateState.update((state) => ({ ...state, error: toMessage(error) }));
    }
    await relaunch();
  } catch (error) {
    updateState.update((state) => ({ ...state, error: toMessage(error) }));
  } finally {
    updateState.update((state) => ({ ...state, installing: false }));
  }
}
