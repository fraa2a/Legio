import { invoke } from "./invoke";
import contract from "./settings-contract.json";

export const playStatuses = ["none", "backlog", "playing", "completed", "onHold", "dropped"] as const;
export type PlayStatus = (typeof playStatuses)[number];

export interface GameOrganization {
  gameId: string;
  favorite: boolean;
  playStatus: PlayStatus;
  // Milliseconds since the Unix epoch; null for games added before tracking began.
  addedAt: number | null;
}

export interface Collection {
  id: string;
  name: string;
  gameIds: string[];
}

export interface LibraryOrganization {
  games: GameOrganization[];
  collections: Collection[];
}

export const maxCollectionNameLength = contract.limits.collectionNameChars;

export function getLibraryOrganization(): Promise<LibraryOrganization> {
  return invoke<LibraryOrganization>("get_library_organization");
}

export function setGamesFavorite(gameIds: string[], favorite: boolean): Promise<LibraryOrganization> {
  return invoke<LibraryOrganization>("set_games_favorite", { gameIds, favorite });
}

export function setGamesPlayStatus(gameIds: string[], status: PlayStatus): Promise<LibraryOrganization> {
  return invoke<LibraryOrganization>("set_games_play_status", { gameIds, status });
}

export function setGamesCollections(gameIds: string[], collectionIds: string[]): Promise<LibraryOrganization> {
  return invoke<LibraryOrganization>("set_games_collections", { gameIds, collectionIds });
}

export function createCollection(name: string): Promise<LibraryOrganization> {
  return invoke<LibraryOrganization>("create_collection", { name });
}

export function renameCollection(id: string, name: string): Promise<LibraryOrganization> {
  return invoke<LibraryOrganization>("rename_collection", { id, name });
}

export function deleteCollection(id: string): Promise<LibraryOrganization> {
  return invoke<LibraryOrganization>("delete_collection", { id });
}

export function reorderCollections(ids: string[]): Promise<LibraryOrganization> {
  return invoke<LibraryOrganization>("reorder_collections", { ids });
}
