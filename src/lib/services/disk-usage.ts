import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { invoke } from "./invoke";

export interface GameSize {
  gameId: string;
  bytes: number;
  /** Some files could not be read, so the size is a lower bound. */
  partial: boolean;
  measuredAt: number;
}

export interface DiskUsageSnapshot {
  games: GameSize[];
  computing: boolean;
}

export interface LibraryUsage {
  libraryId: string;
  totalBytes: number | null;
  freeBytes: number | null;
  legioBytes: number;
  otherBytes: number | null;
  /** Some game is still unmeasured or only partly readable. */
  estimated: boolean;
}

export const getDiskUsage = () => invoke<DiskUsageSnapshot>("get_disk_usage");
export const refreshDiskUsage = (gameId: string | null = null) => invoke<void>("refresh_disk_usage", { gameId });
export const cancelDiskUsage = () => invoke<void>("cancel_disk_usage");
export const getLibraryUsage = () => invoke<LibraryUsage[]>("get_library_usage");
export const openGameFolder = (id: string) => invoke<void>("open_game_folder", { id });
export const onDiskUsageChanged = (callback: () => void): Promise<UnlistenFn> => listen("legio:disk-usage-changed", () => callback());
