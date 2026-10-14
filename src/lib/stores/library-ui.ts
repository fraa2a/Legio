import { derived, writable } from "svelte/store";

import { settings } from "./settings";

export const libraryPortrait = derived(settings, ($settings) => $settings.data.libraryPortrait);
export const libraryQuery = writable("");
export const storeQuery = writable("");
export const addGameDialogOpen = writable(false);
