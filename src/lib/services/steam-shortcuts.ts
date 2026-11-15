import { invoke } from "./invoke";

export interface SteamShortcutAccount {
  steamId: string;
  displayName: string;
  legioShortcuts: number;
  totalShortcuts: number;
  error: string | null;
}

export interface SteamShortcutAccounts {
  accounts: SteamShortcutAccount[];
  steamRunning: boolean;
  diagnostics: string[];
}

export type SteamShortcutAction = "add" | "update" | "unchanged";

export interface SteamShortcutPreview {
  items: { gameId: string; name: string; action: SteamShortcutAction }[];
  steamRunning: boolean;
}

export interface SteamShortcutExport {
  added: number;
  updated: number;
  unchanged: number;
  artworkWritten: number;
  warnings: string[];
}

export function listSteamShortcutAccounts(): Promise<SteamShortcutAccounts> {
  return invoke<SteamShortcutAccounts>("list_steam_shortcut_accounts");
}

export function previewSteamShortcuts(steamId: string, gameIds: string[]): Promise<SteamShortcutPreview> {
  return invoke<SteamShortcutPreview>("preview_steam_shortcuts", { steamId, gameIds });
}

export function exportSteamShortcuts(steamId: string, gameIds: string[]): Promise<SteamShortcutExport> {
  return invoke<SteamShortcutExport>("export_steam_shortcuts", { steamId, gameIds });
}

export function removeExportedSteamShortcuts(steamId: string, gameIds: string[] | null): Promise<{ removed: number }> {
  return invoke<{ removed: number }>("remove_exported_steam_shortcuts", { steamId, gameIds });
}
