import { getSettings, saveSettings, type Settings } from "../services/local-state";
import contract from "../services/settings-contract.json";
import { get, writable } from "svelte/store";
import { createResource } from "./resource";
import { installedFolder } from "./downloads";
import { setApplicationLoggingEnabled } from "../services/application-log";

export const defaultSettings = contract.defaults as Settings;

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
