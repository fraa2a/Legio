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
export const listGameUpdates = () => invoke<GameUpdate[]>("list_game_updates");
export const queueGameUpdate = (gameId: string, acceptUnverified: boolean) => invoke<string>("queue_game_update", { gameId, acceptUnverified });
export const listenGameUpdatesChanged = (callback: () => void) => listen("game-updates-changed", callback);
export const listenDownloadsChanged = (callback: () => void) => listen("downloads-changed", callback);
