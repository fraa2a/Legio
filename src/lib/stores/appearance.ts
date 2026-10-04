import { writable } from "svelte/store";
import type { Appearance } from "../services/appearance";
import type { Theme } from "../services/local-state";

export const appearancePreview = writable<{ theme: Theme; appearance: Appearance } | null>(null);
