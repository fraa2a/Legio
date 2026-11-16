import { invoke } from "./invoke";

import type { Appearance } from "./appearance";
import type { PlayStatus } from "./library-organization";

export type Theme = "system" | "dark" | "light" | "bronze" | "crimson" | "steel" | "emerald" | "sand" | "midnight" | "violet" | "contrast" | "oled" | "frost";

export type LibraryViewMode = "grid" | "list" | "shelves";
export type LibrarySortKey = "name" | "playtime" | "lastPlayed" | "added" | "origin";
export type LibraryCollectionFilter = { kind: "all" } | { kind: "uncollected" } | { kind: "collection"; id: string };

export interface LibraryViewPreferences {
  mode: LibraryViewMode;
  sort: LibrarySortKey;
  descending: boolean;
  // Grid tile width as a percentage of the default width.
  coverScale: number;
  favoritesOnly: boolean;
  playStatus: PlayStatus | null;
  collection: LibraryCollectionFilter;
}

export interface Settings {
  discordPresence: { enabled: boolean };
  onboardingCompleted: boolean;
  theme: Theme;
  appearance: Appearance;
  language: "system" | "it" | "en";
  steamLibraryPollMinutes: number;
  syncSteamGames: boolean;
  storeSort: "relevance" | "name_asc" | "name_desc";
  storeAvailability: "all" | "available" | "verified";
  downloadPath: string | null;
  closeToTray: boolean;
  hideOnGameStart: boolean;
  launchOnSystemStart: boolean;
  launchMinimized: boolean;
  launchInLibrary: boolean;
  sidebarCollapsed: boolean;
  libraryPortrait: boolean;
  libraryView: LibraryViewPreferences;
  automaticGameUpdates: boolean;
  downloadWhilePlaying: boolean;
  downloadNotifications: boolean;
  verifyVerifiedDownloads: boolean;
  diagnosticsEnabled: boolean;
  applicationLoggingEnabled: boolean;
  deferExtractionWhilePlaying: boolean;
  thunderstoreLinksEnabled: boolean;
  newsEnabled: boolean;
  automaticUpdateCheck: boolean;
  previewUpdates: boolean;
  lastSeenVersion: string | null;
  pendingUpdateNotes: { version: string; notes: string } | null;
}

export interface Game {
  id: string;
  steamAppId: number | null;
  automaticName: string | null;
  nameOverride: string | null;
  name: string;
  steamInstallPath: string | null;
  steamAccountId: string | null;
  executablePath: string | null;
  installationRoot: string | null;
}

export interface CreateGameInput {
  name: string;
  steamAppId: number | null;
}

export interface UpdateGameInput {
  id: string;
  steamAppId: number | null;
  automaticName: string | null;
  nameOverride: string | null;
}

export function getSettings(): Promise<Settings> {
  return invoke<Settings>("get_settings");
}

export function saveSettings(settings: Settings): Promise<Settings> {
  return invoke<Settings>("save_settings", { settings });
}

export function listGames(): Promise<Game[]> {
  return invoke<Game[]>("list_games");
}

export function listHiddenSteamGames(): Promise<Game[]> {
  return invoke<Game[]>("list_hidden_steam_games");
}

export function setGameHidden(id: string, hidden: boolean): Promise<void> {
  return invoke<void>("set_game_hidden", { id, hidden });
}

export function createGame(input: CreateGameInput): Promise<Game> {
  return invoke<Game>("create_game", { input });
}

export function updateGame(input: UpdateGameInput): Promise<Game> {
  return invoke<Game>("update_game", { input });
}

export function removeGame(id: string): Promise<void> {
  return invoke<void>("remove_game", { id });
}

export function getGameUninstallTarget(id: string): Promise<string> {
  return invoke<string>("get_game_uninstall_target", { id });
}

export function uninstallGameFiles(id: string, installationRoot: string): Promise<{ warnings: string[] }> {
  return invoke<{ warnings: string[] }>("uninstall_game", { id, installationRoot });
}

export function createDesktopShortcut(gameId: string): Promise<{ path: string; warning: string | null }> {
  return invoke("create_game_shortcut", { gameId, location: "desktop" });
}

export function transferGame(gameId: string, libraryId: string): Promise<{ game: Game; warning: string | null }> {
  return invoke("transfer_game", { gameId, libraryId });
}
