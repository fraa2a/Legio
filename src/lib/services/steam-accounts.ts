import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { invoke } from "./invoke";
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

export interface SteamLaunchResult {
  gameId: string;
  steamAppId: number;
}

export interface SteamGameLaunchInspection {
  status: "no_override" | "already_matches" | "mismatch" | "unknown";
  steamRunning: boolean;
  currentAccountName: string | null;
  targetAccountName: string | null;
}

export function inspectSteamGameLaunch(gameId: string): Promise<SteamGameLaunchInspection> {
  return invoke<SteamGameLaunchInspection>("inspect_steam_game_launch", { gameId });
}

export function launchSteamGame(gameId: string, confirmAccountSwitch: boolean): Promise<SteamLaunchResult> {
  return invoke<SteamLaunchResult>("launch_steam_game", { gameId, confirmAccountSwitch });
}

export interface GameLaunchState {
  gameId: string;
  status: "idle" | "launching" | "running";
  error?: string;
  compatibilityLogPath?: string;
  compatibilityLogError?: string;
  runnerExitCode?: number;
}

export function onGameLaunchStates(callback: (states: GameLaunchState[]) => void): Promise<UnlistenFn> {
  return listen<GameLaunchState[]>("legio:game-launch-states", (event) => callback(event.payload));
}

export function onShortcutLaunchFailure(callback: (message: string) => void): Promise<UnlistenFn> {
  return listen<string>("legio:shortcut-launch-failed", (event) => callback(event.payload));
}

export function listGameLaunchStates(): Promise<GameLaunchState[]> {
  return invoke<GameLaunchState[]>("list_game_launch_states");
}

export function cancelGameLaunch(gameId: string): Promise<void> {
  return invoke<void>("cancel_game_launch", { gameId });
}

export function stopGame(gameId: string): Promise<void> {
  return invoke<void>("stop_game", { gameId });
}
