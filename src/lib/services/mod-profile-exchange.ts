import { invoke } from "./invoke";
import type { ModProfile } from "./modding";

export interface ProfileImportOutcome {
  profile: ModProfile;
  installed: string[];
  skippedFiles: string[];
  missingPackages: string[];
  warnings: string[];
}

export const exportModProfileFile = (gameId: string, profileId: string, destinationPath: string) =>
  invoke<void>("export_mod_profile_file", { gameId, profileId, destinationPath });

export const shareModProfileCode = (gameId: string, profileId: string) =>
  invoke<string>("share_mod_profile_code", { gameId, profileId });

export const importModProfileFile = (gameId: string, path: string, profileName?: string) =>
  invoke<ProfileImportOutcome>("import_mod_profile_file", { gameId, path, profileName });

export const importModProfileCode = (gameId: string, code: string, profileName?: string) =>
  invoke<ProfileImportOutcome>("import_mod_profile_code", { gameId, code, profileName });
