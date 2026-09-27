import { invoke } from "@tauri-apps/api/core";
import { writable } from "svelte/store";

export const gameArtworkRevision = writable(0);

function artworkChanged(): void {
  gameArtworkRevision.update((revision) => revision + 1);
}

export interface GameArtwork {
  bytes: number[];
  contentType: string;
}

export function getGameIcon(gameId: string): Promise<GameArtwork | null> {
  return invoke<GameArtwork | null>("get_game_icon", { gameId });
}

export function setGameIcon(gameId: string, filePath: string): Promise<GameArtwork> {
  return invoke<GameArtwork>("set_game_icon", { gameId, filePath }).then((result) => {
    artworkChanged();
    return result;
  });
}

export function extractGameIcon(gameId: string): Promise<GameArtwork> {
  return invoke<GameArtwork>("extract_game_icon", { gameId }).then((result) => {
    artworkChanged();
    return result;
  });
}

export function resetGameIcon(gameId: string): Promise<void> {
  return invoke<void>("reset_game_icon", { gameId }).then(artworkChanged);
}

export function getGameBanner(gameId: string): Promise<GameArtwork | null> {
  return invoke<GameArtwork | null>("get_game_banner", { gameId });
}

export function setGameBanner(gameId: string, filePath: string): Promise<GameArtwork> {
  return invoke<GameArtwork>("set_game_banner", { gameId, filePath }).then((result) => {
    artworkChanged();
    return result;
  });
}

export function resetGameBanner(gameId: string): Promise<void> {
  return invoke<void>("reset_game_banner", { gameId }).then(artworkChanged);
}
