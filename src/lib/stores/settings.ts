import { getSettings, saveSettings, type Settings } from "../services/local-state";
import { defaultAppearance } from "../services/appearance";
import { get, writable } from "svelte/store";
import { createResource } from "./resource";
import { installedFolder } from "./downloads";
import { setApplicationLoggingEnabled } from "../services/application-log";

export const defaultSettings: Settings = {
  discordPresence: { enabled: true },
  onboardingCompleted: false,
  appearance: defaultAppearance(), theme: "system", language: "system", steamLibraryPollMinutes: 30, syncSteamGames: true, storeSort: "relevance", storeAvailability: "all", downloadPath: null,
  closeToTray: true, hideOnGameStart: true, launchOnSystemStart: false,
  launchMinimized: false, launchInLibrary: false, downloadNotifications: true, verifyVerifiedDownloads: true,
  diagnosticsEnabled: false, applicationLoggingEnabled: false, sidebarCollapsed: true, libraryPortrait: false, automaticGameUpdates: false, downloadWhilePlaying: true, deferExtractionWhilePlaying: false,
};

export const settings = createResource<Settings>(defaultSettings, getSettings);
settings.subscribe(state => {
  if (state.status === "ready") setApplicationLoggingEnabled(state.data.applicationLoggingEnabled);
});

export const settingsError = writable<string | null>(null);

let saveQueue: Promise<void> = Promise.resolve();

export function updateSettings(changes: Partial<Settings>): Promise<Settings> {
  const patch = structuredClone(changes);
  const task = saveQueue.then(async () => {
    if (get(settings).status !== "ready") await settings.load();
    const current = get(settings);
    if (current.status !== "ready") {
      throw new Error(current.error ?? "Could not load settings before saving");
    }
    const previous = current.data;
    const saved = await saveSettings({ ...previous, ...patch });
    settings.set(saved);
    if (saved.downloadPath !== previous.downloadPath) await installedFolder.reload();
    return saved;
  });
  saveQueue = task.then(() => {}, () => {});
  return task;
}
