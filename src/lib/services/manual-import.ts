import { invoke } from "@tauri-apps/api/core";
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

export function scanGameExecutables(
  directory: string,
  gameName: string | null,
): Promise<ExecutableScan> {
  return invoke<ExecutableScan>("scan_game_executables", { directory, gameName });
}

export function importManualGame(input: ManualImportInput): Promise<Game> {
  return invoke<Game>("import_manual_game", { input });
}

export function setGameExecutable(gameId: string, executablePath: string): Promise<Game> {
  return invoke<Game>("set_game_executable", { gameId, executablePath });
}
