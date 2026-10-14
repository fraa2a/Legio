import { listen } from "@tauri-apps/api/event";
import { invoke } from "./invoke";

export interface GameUpdate {
  gameId: string;
  sourceId: string;
  entryId: string;
  steamAppId: number;
  name: string;
  installedVersion: string;
  version: string;
  sizeBytes: number;
  verified: boolean;
  acceptedUnverified: boolean;
  fresh: boolean;
}
export interface GameSourceBinding {
  sourceId: string | null;
  entryId: string | null;
  installedVersion: string;
}
export const listGameUpdates = () => invoke<GameUpdate[]>("list_game_updates");
export const queueGameUpdate = (gameId: string, acceptUnverified: boolean) => invoke<string>("queue_game_update", { gameId, acceptUnverified });
export const getGameSourceBinding = (gameId: string) => invoke<GameSourceBinding>("get_game_source_binding", { gameId });
export const bindGameSource = (gameId: string, sourceId: string, entryId: string, installedVersion: string, acceptUnverified: boolean) =>
  invoke<void>("bind_game_source", { gameId, sourceId, entryId, installedVersion, acceptUnverified });

export const listenGameUpdatesChanged = (callback: () => void) => listen("game-updates-changed", callback);
export const listenDownloadsChanged = (callback: () => void) => listen("downloads-changed", callback);
