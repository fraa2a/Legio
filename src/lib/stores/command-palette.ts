import { writable } from "svelte/store";

export const paletteOpen = writable(false);

// Text typed into the title bar search before the palette opened; the palette takes it once.
export const paletteSeed = writable("");

export function openPalette(seed = ""): void {
  paletteSeed.set(seed);
  paletteOpen.set(true);
}
