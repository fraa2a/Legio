import { writable } from "svelte/store";

export const sections = ["home", "library", "store", "downloads", "settings"] as const;

export type Section = (typeof sections)[number];

export const sectionLabels: Record<Section, string> = {
  home: "Home",
  library: "Libreria",
  store: "Store",
  downloads: "Download",
  settings: "Impostazioni",
};

export const activeSection = writable<Section>("home");

export const selectedGameId = writable<string | null>(null);

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
