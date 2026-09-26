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

export function selectSection(section: Section): void {
  activeSection.set(section);
}
