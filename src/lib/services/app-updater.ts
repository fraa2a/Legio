import { invoke } from "./invoke";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { writable } from "svelte/store";
import { toMessage } from "../utils/errors";

type UpdateState = {
  version: string | null;
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
  updateState.update((state) => ({ ...state, version: null, unavailable: false, checking: true, checked: false, error: null }));
  try {
    const previous = availableUpdate;
    availableUpdate = null;
    if (previous !== null) await previous.close();
    const aur = await invoke<boolean>("is_aur_package");
    const update = await check();
    availableUpdate = update;
    let nativeNotified = update !== null && notifiedVersion === update.version;
    let notificationError: string | null = null;
    if (update !== null && !nativeNotified) {
      try {
        await invoke("notify_update_available", { version: update.version, aur });
        notifiedVersion = update.version;
        nativeNotified = true;
      } catch (error) {
        notificationError = toMessage(error);
      }
    }
    updateState.update((state) => ({ ...state, version: update?.version ?? null, aur, checked: true, nativeNotified, error: notificationError }));
  } catch (error) {
    const message = toMessage(error);
    const unavailable = /^(?:None of the fallback platforms `[^`]+` were|the platform `[^`]+` was not) found in the response `platforms` object$/.test(message);
    updateState.update((state) => ({ ...state, checked: unavailable, unavailable, nativeNotified: false, error: unavailable ? null : message }));
  } finally {
    updateState.update((state) => ({ ...state, checking: false }));
  }
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
    await update.downloadAndInstall();
    availableUpdate = null;
    await relaunch();
  } catch (error) {
    updateState.update((state) => ({ ...state, error: toMessage(error) }));
  } finally {
    updateState.update((state) => ({ ...state, installing: false }));
  }
}
