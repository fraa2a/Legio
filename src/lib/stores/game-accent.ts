import { writable } from "svelte/store";
import { sampleImageColor } from "../services/game-accent";

// The dominant artwork color of the open game page, keyed by the artwork it was sampled from.
export const gameArtColor = writable<{ key: string; color: string } | null>(null);

// A load event that arrives after the page moved to other artwork is dropped.
export function publishArtColor(key: string, currentKey: string | null, image: HTMLImageElement): void {
  if (key !== currentKey || !image.isConnected) return;
  const color = sampleImageColor(image);
  gameArtColor.set(color === null ? null : { key, color });
}

// Keeps only a color sampled from the artwork that is current.
export function keepArtColorFor(currentKey: string | null): void {
  gameArtColor.update((entry) => (entry !== null && entry.key === currentKey ? entry : null));
}
