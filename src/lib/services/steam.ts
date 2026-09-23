import { invoke } from "@tauri-apps/api/core";

export interface DetectedSteamGame {
  appId: number;
  name: string;
  installDir: string;
  installPath: string;
}

export interface SteamScan {
  games: DetectedSteamGame[];
  diagnostics: string[];
}

export function scanSteamInstallations(): Promise<SteamScan> {
  return invoke<SteamScan>("scan_steam_installations");
}

export interface SteamImportResult {
  detected: number;
  inserted: number;
  updated: number;
  unchanged: number;
  removed: number;
  diagnostics: string[];
}

export function importSteamInstallations(): Promise<SteamImportResult> {
  return invoke<SteamImportResult>("import_steam_installations");
}
