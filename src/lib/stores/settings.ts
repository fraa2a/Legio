import { getSettings, saveSettings, type Settings } from "../services/local-state";
import { defaultAppearance } from "../services/appearance";
import { get, writable } from "svelte/store";
import { createResource } from "./resource";
import { installedFolder } from "./downloads";
import { setApplicationLoggingEnabled } from "../services/application-log";

export const defaultSettings: Settings = {
  discordPresence: { enabled: true },
  onboardingCompleted: false,
  appearance: defaultAppearance(), theme: "system", language: "system", steamLibraryPollMinutes: 30, downloadPath: null,
  closeToTray: true, hideOnGameStart: true, launchOnSystemStart: false,
  launchMinimized: false, launchInLibrary: false, downloadNotifications: true, verifyVerifiedDownloads: true,
  diagnosticsEnabled: false, applicationLoggingEnabled: false, sidebarCollapsed: true, deferExtractionWhilePlaying: false,
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
    const previous = get(settings).data;
    const saved = await saveSettings({ ...previous, ...patch });
    settings.set(saved);
    if (saved.downloadPath !== previous.downloadPath) await installedFolder.load();
    return saved;
  });
  saveQueue = task.then(() => {}, () => {});
  return task;
}
