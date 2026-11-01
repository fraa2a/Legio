import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { decodeImageResponse } from "./image-response";
import { invoke } from "./invoke";

export type PackageLoader =
  | "bepinex" | "melonloader" | "recursive-melonloader" | "lovely" | "godotml" | "return-of-modding" | "northstar"
  | "shimloader" | "gdweave" | "umm" | "bepisloader" | "rivet" | "none" | "unknown";

export interface GameModding { gameId: string; ecosystemGameId: string; activeProfileId: string | null }

export interface ModdableGame {
  gameId: string;
  name: string;
  steamAppId: number | null;
  ecosystemGameId: string | null;
  displayName: string | null;
  packageLoader: PackageLoader | null;
  supported: boolean;
  binding: GameModding | null;
}

export interface EcosystemGameSummary { id: string; displayName: string; packageLoader: PackageLoader; supported: boolean }

export interface ModProfile { id: string; gameId: string; name: string; createdAt: number }

export interface ProfileMod {
  profileId: string;
  fullName: string;
  version: string;
  enabled: boolean;
  onlineSource: boolean;
  installedAt: number;
  installOrder: number;
  isLoader: boolean;
  // Packages (`Owner-Name`) the installed version declares, read from its stored manifest.
  dependencies: string[];
}

export interface CatalogStatus { ecosystemGameId: string; packageCount: number; syncedAt: number }

export type PackageSort = "lastUpdated" | "mostDownloads" | "topRated" | "newest" | "alphabetical";

export interface PackageQuery {
  text?: string;
  categories?: string[];
  includeDeprecated?: boolean;
  includeNsfw?: boolean;
  sort?: PackageSort;
  offset?: number;
  limit?: number;
}

export interface PackageSummary {
  fullName: string;
  owner: string;
  name: string;
  packageUrl: string;
  categories: string[];
  isDeprecated: boolean;
  isPinned: boolean;
  hasNsfwContent: boolean;
  ratingScore: number;
  dateCreated: string;
  dateUpdated: string;
  totalDownloads: number;
  latestVersion: string;
  description: string;
  icon: string;
}

export interface PackagePage { total: number; items: PackageSummary[] }

export interface PackageVersionDetails {
  fullName: string;
  versionNumber: string;
  description: string;
  icon: string;
  downloadUrl: string;
  fileSize: number;
  dependencies: string[];
  downloads: number;
  dateCreated: string;
}

export type PackageDetails = PackageSummary & { versions: PackageVersionDetails[] };

export interface ModInstallRequest { fullName: string; version?: string }
export interface ModInstallResult { installed: string[]; skipped: string[]; warnings: string[] }
export interface ModUpdate { fullName: string; installedVersion: string; latestVersion: string }
export interface UninstallOutcome { dependents: string[] }
export interface ToggleFailure { fullName: string; message: string }
export interface ToggleAllOutcome { changed: number; failed: ToggleFailure[] }
export interface DeleteProfileOutcome { directoryError: string | null }
export interface LocalImportOutcome { installed: ProfileMod; missingDependencies: string[] }

export interface ModLaunchStatus {
  bound: boolean;
  ecosystemGameId: string | null;
  activeProfileId: string | null;
  launchSupported: boolean;
  loaderDeployed: boolean;
  preloaderFound: boolean;
  doorstopVersion: number | null;
}

export interface CatalogProgress { community: string; chunksDone: number; chunksTotal: number }

export type InstallStage = "resolving" | "downloading" | "extracting" | "installing" | "done";

export interface InstallProgress {
  profileId: string;
  // Echoes the `operationId` passed to the command that started the install.
  operationId: string | null;
  package: string | null;
  stage: InstallStage;
  current: number;
  total: number;
  downloadedBytes: number | null;
  totalBytes: number | null;
  // True only while the running stage stops on `cancel_mod_install`.
  cancellable: boolean;
}

export interface ModIcon { bytes: Uint8Array<ArrayBuffer>; contentType: string }

export const listModdableGames = () => invoke<ModdableGame[]>("list_moddable_games");
export const listEcosystemGames = () => invoke<EcosystemGameSummary[]>("list_ecosystem_games");
export const bindGameToEcosystem = (gameId: string, ecosystemGameId: string) =>
  invoke<GameModding>("bind_game_to_ecosystem", { gameId, ecosystemGameId });

export const refreshModCatalog = (ecosystemGameId: string, force: boolean) =>
  invoke<CatalogStatus>("refresh_mod_catalog", { ecosystemGameId, force });
export const searchMods = (ecosystemGameId: string, query: PackageQuery) =>
  invoke<PackagePage>("search_mods", { ecosystemGameId, query });
export const getModPackage = (ecosystemGameId: string, fullName: string) =>
  invoke<PackageDetails | null>("get_mod_package", { ecosystemGameId, fullName });
export const listModCategories = (ecosystemGameId: string) => invoke<string[]>("list_mod_categories", { ecosystemGameId });

export function getModIcon(url: string): Promise<ModIcon> {
  return invoke<ArrayBuffer>("get_mod_icon", { url }).then(decodeImageResponse<{ contentType: string }>);
}

export const openModPackagePage = (url: string) => invoke<void>("open_mod_package_page", { url });

export const listModProfiles = (gameId: string) => invoke<ModProfile[]>("list_mod_profiles", { gameId });
export const createModProfile = (gameId: string, name: string) => invoke<ModProfile>("create_mod_profile", { gameId, name });
export const renameModProfile = (profileId: string, name: string) => invoke<ModProfile>("rename_mod_profile", { profileId, name });
export const duplicateModProfile = (profileId: string, name: string) =>
  invoke<ModProfile>("duplicate_mod_profile", { profileId, name });
export const deleteModProfile = (profileId: string) => invoke<DeleteProfileOutcome>("delete_mod_profile", { profileId });
export const setActiveModProfile = (gameId: string, profileId: string | null) =>
  invoke<GameModding>("set_active_mod_profile", { gameId, profileId });
export const openModProfileFolder = (profileId: string) => invoke<void>("open_mod_profile_folder", { profileId });

export const listProfileMods = (profileId: string) => invoke<ProfileMod[]>("list_profile_mods", { profileId });
export const installMods = (profileId: string, packages: ModInstallRequest[]) =>
  invoke<ModInstallResult>("install_mods", { profileId, packages });
export const installModLoader = (profileId: string) => invoke<ModInstallResult>("install_mod_loader", { profileId });
export const listModUpdates = (profileId: string) => invoke<ModUpdate[]>("list_mod_updates", { profileId });
export const updateProfileMods = (profileId: string, fullNames: string[] | null = null) =>
  invoke<ModInstallResult>("update_profile_mods", { profileId, fullNames });
export const cancelModInstall = (profileId: string) => invoke<void>("cancel_mod_install", { profileId });
export const uninstallProfileMod = (profileId: string, fullName: string) =>
  invoke<UninstallOutcome>("uninstall_profile_mod", { profileId, fullName });
export const setProfileModEnabled = (profileId: string, fullName: string, enabled: boolean) =>
  invoke<void>("set_profile_mod_enabled", { profileId, fullName, enabled });
export const importLocalMod = (profileId: string, archivePath: string) =>
  invoke<LocalImportOutcome>("import_local_mod", { profileId, archivePath });

export const getModLaunchStatus = (gameId: string) => invoke<ModLaunchStatus>("get_mod_launch_status", { gameId });
export const restoreModdedGameFolder = (gameId: string) => invoke<void>("restore_modded_game_folder", { gameId });

export function observeCatalogProgress(callback: (progress: CatalogProgress) => void): Promise<UnlistenFn> {
  return listen<CatalogProgress>("modding-catalog-progress", (event) => callback(event.payload));
}

export function observeInstallProgress(callback: (progress: InstallProgress) => void): Promise<UnlistenFn> {
  return listen<InstallProgress>("modding-install-progress", (event) => callback(event.payload));
}

export const setAllProfileModsEnabled = (profileId: string, enabled: boolean) =>
  invoke<ToggleAllOutcome>("set_all_profile_mods_enabled", { profileId, enabled });
