import { writable } from "svelte/store";

export const sections = ["home", "library", "store", "downloads"] as const;

export type Section = (typeof sections)[number];

export const sectionLabels: Record<Section, string> = {
  home: "Home",
  library: "Libreria",
  store: "Store",
  downloads: "Download",
};

export const activeSection = writable<Section>("home");

export const selectedGameId = writable<string | null>(null);

export const settingsOpen = writable(false);

export function selectSection(section: Section): void {
  activeSection.set(section);
}

export function openGame(gameId: string): void {
  selectedGameId.set(gameId);
  activeSection.set("library");
}

export function closeGame(): void {
  selectedGameId.set(null);
}

export function openSettings(): void {
  settingsOpen.set(true);
}

export function closeSettings(): void {
  settingsOpen.set(false);
}
