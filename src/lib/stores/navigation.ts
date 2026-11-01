import { get, writable } from "svelte/store";

export const sections = ["home", "library", "store", "mods", "downloads"] as const;

export type Section = (typeof sections)[number];

export const sectionLabels: Record<Section, string> = {
  home: "Home",
  library: "Libreria",
  store: "Store",
  mods: "Mod",
  downloads: "Download",
};

export const activeSection = writable<Section>("home");

export const selectedGameId = writable<string | null>(null);

export interface StoreGameSelection {
  steamAppId: number;
  name: string;
}

export const selectedStoreGame = writable<StoreGameSelection | null>(null);

export interface ModManagerSelection {
  gameId: string;
  // Back navigation returns to the game page when the manager was opened from it.
  returnTo: "mods" | "game";
}

export const selectedModGame = writable<ModManagerSelection | null>(null);

export const settingsOpen = writable(false);
export const settingsCategory = writable("general");

// A page with unsaved work can hold navigation until the user confirms. The guard
// returns true when it took over and will call `proceed` itself.
type LeaveGuard = (proceed: () => void) => boolean;

let leaveGuard: LeaveGuard | null = null;

export function setLeaveGuard(guard: LeaveGuard): () => void {
  leaveGuard = guard;
  return () => {
    if (leaveGuard === guard) leaveGuard = null;
  };
}

function leave(action: () => void): void {
  const guard = leaveGuard;
  if (guard === null || !guard(() => {
    if (leaveGuard === guard) leaveGuard = null;
    action();
  })) action();
}

export function selectSection(section: Section): void {
  leave(() => {
    selectedGameId.set(null);
    selectedStoreGame.set(null);
    selectedModGame.set(null);
    activeSection.set(section);
  });
}

function showGame(gameId: string): void {
  selectedModGame.set(null);
  selectedGameId.set(gameId);
  activeSection.set("library");
}

export function openGame(gameId: string): void {
  leave(() => showGame(gameId));
}

export function openModManager(gameId: string, returnTo: ModManagerSelection["returnTo"] = "mods"): void {
  leave(() => {
    selectedGameId.set(null);
    selectedModGame.set({ gameId, returnTo });
    activeSection.set("mods");
  });
}

export function closeModManager(): void {
  leave(() => {
    const selection = get(selectedModGame);
    if (selection?.returnTo === "game") showGame(selection.gameId);
    else selectedModGame.set(null);
  });
}

export function closeGame(): void {
  selectedGameId.set(null);
}

export function openStoreGame(game: StoreGameSelection): void {
  selectedStoreGame.set(game);
  activeSection.set("store");
}

export function closeStoreGame(): void {
  selectedStoreGame.set(null);
}

export function openSettings(category = "general"): void {
  settingsCategory.set(category);
  settingsOpen.set(true);
}

export function closeSettings(): void {
  settingsOpen.set(false);
}
