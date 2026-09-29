import { invoke } from "@tauri-apps/api/core";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { writable } from "svelte/store";
import { toMessage } from "../utils/errors";

type UpdateState = {
  version: string | null;
  aur: boolean;
  checking: boolean;
  checked: boolean;
  installing: boolean;
  error: string | null;
};

export const updateState = writable<UpdateState>({
  version: null,
  aur: false,
  checking: false,
  checked: false,
  installing: false,
  error: null,
});

let availableUpdate: Update | null = null;

export async function checkForAppUpdate(): Promise<void> {
  updateState.update((state) => ({ ...state, checking: true, checked: false, error: null }));
  try {
    const [update, aur] = await Promise.all([check(), invoke<boolean>("is_aur_package")]);
    availableUpdate = update;
    updateState.update((state) => ({ ...state, version: update?.version ?? null, aur, checked: true }));
  } catch (error) {
    updateState.update((state) => ({ ...state, error: toMessage(error) }));
  } finally {
    updateState.update((state) => ({ ...state, checking: false }));
  }
}

export async function installAppUpdate(): Promise<void> {
  if (!availableUpdate) return;
  updateState.update((state) => ({ ...state, installing: true, error: null }));
  try {
    await availableUpdate.downloadAndInstall();
    await relaunch();
  } catch (error) {
    updateState.update((state) => ({ ...state, error: toMessage(error) }));
  } finally {
    updateState.update((state) => ({ ...state, installing: false }));
  }
}
