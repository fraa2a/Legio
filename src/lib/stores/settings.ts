import { getSettings, saveSettings, type Settings, type Theme } from "../services/local-state";
import { toMessage } from "../utils/errors";
import { writable } from "svelte/store";
import { createResource } from "./resource";

const fallback: Settings = { theme: "system" };

export const settings = createResource<Settings>(fallback, getSettings);

export const settingsError = writable<string | null>(null);

export async function changeTheme(theme: Theme): Promise<void> {
  settingsError.set(null);
  try {
    settings.set(await saveSettings({ theme }));
  } catch (error) {
    settingsError.set(toMessage(error));
  }
}

export function resolvedTheme(theme: Theme, prefersDark: boolean): "dark" | "light" {
  if (theme === "system") return prefersDark ? "dark" : "light";
  return theme;
}
