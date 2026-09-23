import { invoke } from "@tauri-apps/api/core";
import type { Game } from "./local-state";

export interface SavedSteamAccount {
  steamId: string;
  displayName: string;
}

export interface SavedSteamAccounts {
  accounts: SavedSteamAccount[];
  diagnostics: string[];
}

export interface SteamAccountCheck {
  status: "not_required" | "missing_saved_account" | "match" | "mismatch" | "unknown";
  accountRequirementMet: boolean;
  selectedSteamId: string | null;
  message: string | null;
}

export function listSavedSteamAccounts(): Promise<SavedSteamAccounts> {
  return invoke<SavedSteamAccounts>("list_saved_steam_accounts");
}

export function setGameSteamAccountPreference(gameId: string, steamId: string | null): Promise<Game> {
  return invoke<Game>("set_game_steam_account_preference", { gameId, steamId });
}

export function checkGameSteamAccount(gameId: string): Promise<SteamAccountCheck> {
  return invoke<SteamAccountCheck>("check_game_steam_account", { gameId });
}

export interface SteamLaunchResult {
  gameId: string;
  steamAppId: number;
}

export function launchSteamGame(gameId: string): Promise<SteamLaunchResult> {
  return invoke<SteamLaunchResult>("launch_steam_game", { gameId });
}
