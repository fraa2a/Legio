import { getSettings, type Settings } from "../services/local-state";
import { defaultAppearance } from "../services/appearance";
import { writable } from "svelte/store";
import { createResource } from "./resource";

export const defaultSettings: Settings = {
  discordPresence: { enabled: false, applicationId: "1557120430475575366" },
  onboardingCompleted: false,
  appearance: defaultAppearance(), theme: "system", language: "system", steamLibraryPollMinutes: 30, downloadPath: null,
  closeToTray: true, hideOnGameStart: true, launchOnSystemStart: false,
  launchMinimized: false, launchInLibrary: false, downloadNotifications: true, verifyVerifiedDownloads: true,
  diagnosticsEnabled: true, sidebarCollapsed: true, deferExtractionWhilePlaying: false,
};

export const settings = createResource<Settings>(defaultSettings, getSettings);

export const settingsError = writable<string | null>(null);
