import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open, save } from "@tauri-apps/plugin-dialog";
import { t } from "../i18n";
import { invoke } from "./invoke";
import type { ModProfile } from "./modding";

export interface ProfileImportOutcome {
  profile: ModProfile;
  installed: string[];
  skippedFiles: string[];
  missingPackages: string[];
  warnings: string[];
}

export interface ModInstallProgress {
  profileId: string;
  package: string | null;
  stage: "resolving" | "downloading" | "extracting" | "installing" | "done";
  current: number;
  total: number;
  downloadedBytes: number | null;
  totalBytes: number | null;
}

const profileFilters = () => [{ name: t("Profilo mod (r2modman)", undefined), extensions: ["r2z"] }];

export function pickProfileFile(): Promise<string | null> {
  return open({ title: t("Seleziona un profilo da importare", undefined), directory: false, multiple: false, filters: profileFilters() });
}

export function pickProfileExportPath(defaultName: string): Promise<string | null> {
  return save({ title: t("Esporta profilo", undefined), defaultPath: defaultName, filters: profileFilters() });
}

export const exportModProfileFile = (gameId: string, profileId: string, destinationPath: string) =>
  invoke<void>("export_mod_profile_file", { gameId, profileId, destinationPath });

export const shareModProfileCode = (gameId: string, profileId: string) =>
  invoke<string>("share_mod_profile_code", { gameId, profileId });

export const importModProfileFile = (gameId: string, path: string, profileName?: string) =>
  invoke<ProfileImportOutcome>("import_mod_profile_file", { gameId, path, profileName });

export const importModProfileCode = (gameId: string, code: string, profileName?: string) =>
  invoke<ProfileImportOutcome>("import_mod_profile_code", { gameId, code, profileName });

export const cancelModInstall = (profileId: string) => invoke<void>("cancel_mod_install", { profileId });

export function observeModInstallProgress(handler: (progress: ModInstallProgress) => void): Promise<UnlistenFn> {
  return listen<ModInstallProgress>("modding-install-progress", (event) => handler(event.payload));
}
