import { invoke } from "./invoke";
import type { Game } from "./local-state";

export interface ExecutableCandidate {
  path: string;
  score: number;
  signals: string[];
}

export interface ExecutableScan {
  candidates: ExecutableCandidate[];
  selectedPath: string | null;
}

export interface ManualImportInput {
  executablePath: string;
  name: string | null;
}

export interface GameActionResult {
  game: Game;
  shortcutWarning: string | null;
}

export interface SteamIdentityCandidate {
  steamAppId: number;
  name: string;
}

export interface SteamIdentificationResult {
  game: Game;
  status: "matched" | "no_match" | "ambiguous" | "unavailable" | "already_linked" | "not_manual";
  candidates: SteamIdentityCandidate[];
  message: string | null;
}

export interface SteamIdentificationPreview {
  status: SteamIdentificationResult["status"];
  candidates: SteamIdentityCandidate[];
  message: string | null;
}

export function scanGameExecutables(
  directory: string,
  gameName: string | null,
): Promise<ExecutableScan> {
  return invoke<ExecutableScan>("scan_game_executables", { directory, gameName });
}

export function importManualGame(input: ManualImportInput): Promise<GameActionResult> {
  return invoke<GameActionResult>("import_manual_game", { input });
}

export function identifyManualGameSteamAppId(gameId: string): Promise<SteamIdentificationResult> {
  return invoke<SteamIdentificationResult>("identify_manual_game_steam_app_id", { gameId });
}

export function previewManualGameSteamAppId(executablePath: string): Promise<SteamIdentificationPreview> {
  return invoke<SteamIdentificationPreview>("preview_manual_game_steam_app_id", { executablePath });
}

export function setGameExecutable(gameId: string, executablePath: string): Promise<GameActionResult> {
  return invoke<GameActionResult>("set_game_executable", { gameId, executablePath });
}
