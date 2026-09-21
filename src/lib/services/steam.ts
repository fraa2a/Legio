import { invoke } from "@tauri-apps/api/core";

export interface DetectedSteamGame {
  appId: number;
  name: string;
  installDir: string;
}

export interface SteamScan {
  games: DetectedSteamGame[];
  diagnostics: string[];
}

export function scanSteamInstallations(): Promise<SteamScan> {
  return invoke<SteamScan>("scan_steam_installations");
}
