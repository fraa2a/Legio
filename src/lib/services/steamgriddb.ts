import { invoke } from "./invoke";
import { notifyArtworkChanged } from "./game-artwork";

export type SteamGridDbListing = "grid" | "hero" | "icon";
export type SteamGridDbSlot = "icon" | "banner";

export interface SteamGridDbGame {
  id: number;
  name: string;
}

export interface SteamGridDbImage {
  id: number;
  url: string;
  thumbUrl: string;
  width: number;
  height: number;
}

// The key is write-only: the backend never returns it.
export const hasSteamGridDbKey = () => invoke<boolean>("has_steamgriddb_key");
export const setSteamGridDbKey = (key: string) => invoke<void>("set_steamgriddb_key", { key });
export const clearSteamGridDbKey = () => invoke<void>("clear_steamgriddb_key");

export const searchSteamGridDbGames = (query: string | null, steamAppId: number | null) =>
  invoke<SteamGridDbGame[]>("search_steamgriddb_games", { query, steamAppId });

export const listSteamGridDbArtwork = (gameId: number, kind: SteamGridDbListing) =>
  invoke<SteamGridDbImage[]>("list_steamgriddb_artwork", { gameId, kind });

export async function applySteamGridDbArtwork(gameId: string, slot: SteamGridDbSlot, url: string): Promise<void> {
  await invoke<void>("apply_steamgriddb_artwork", { gameId, slot, url });
  notifyArtworkChanged(gameId, slot);
}
