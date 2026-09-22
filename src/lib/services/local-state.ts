import { invoke } from "@tauri-apps/api/core";

export type Theme = "system" | "dark" | "light";

export interface Settings {
  theme: Theme;
}

export interface Game {
  id: string;
  steamAppId: number | null;
  automaticName: string | null;
  nameOverride: string | null;
  name: string;
  steamInstallPath: string | null;
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

export function createGame(input: CreateGameInput): Promise<Game> {
  return invoke<Game>("create_game", { input });
}

export function updateGame(input: UpdateGameInput): Promise<Game> {
  return invoke<Game>("update_game", { input });
}

export function removeGame(id: string): Promise<void> {
  return invoke<void>("remove_game", { id });
}
