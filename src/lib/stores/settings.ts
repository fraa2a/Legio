import { getSettings, type Settings } from "../services/local-state";
import { defaultAppearance } from "../services/appearance";
import { writable } from "svelte/store";
import { createResource } from "./resource";

const fallback: Settings = {
  appearance: defaultAppearance(), theme: "system", language: "system", steamLibraryPollMinutes: 30, downloadPath: null,
  closeToTray: true, hideOnGameStart: true, launchOnSystemStart: false,
  launchMinimized: false, launchInLibrary: false, downloadNotifications: true, verifyVerifiedDownloads: true,
};

export const settings = createResource<Settings>(fallback, getSettings);

export const settingsError = writable<string | null>(null);
