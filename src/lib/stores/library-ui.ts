import { derived, get, writable } from "svelte/store";

import type { LibraryViewPreferences } from "../services/local-state";
import { settings, updateSettings } from "./settings";

export const libraryPortrait = derived(settings, ($settings) => $settings.data.libraryPortrait);
export const libraryQuery = writable("");
export const storeQuery = writable("");
export const addGameDialogOpen = writable(false);

// Unsaved view changes show at once; the saved settings take over once no save is pending.
const viewDraft = writable<LibraryViewPreferences | null>(null);
let viewSaves = 0;

export const libraryView = derived([settings, viewDraft], ([$settings, $draft]) => $draft ?? $settings.data.libraryView);

/** Shows a change without saving it, such as the cover size while its slider is dragged. */
export function previewLibraryView(changes: Partial<LibraryViewPreferences>): void {
  viewDraft.set({ ...get(libraryView), ...changes });
}

/** Saves the shown view with `changes`. A failed save restores the saved view and rejects. */
export async function updateLibraryView(changes: Partial<LibraryViewPreferences> = {}): Promise<void> {
  const next = { ...get(libraryView), ...changes };
  viewDraft.set(next);
  const save = ++viewSaves;
  try {
    await updateSettings({ libraryView: next });
  } finally {
    if (save === viewSaves) viewDraft.set(null);
  }
}
