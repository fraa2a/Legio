import { invoke } from "./invoke";

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
