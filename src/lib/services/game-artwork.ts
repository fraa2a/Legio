import { invoke } from "@tauri-apps/api/core";
import { writable } from "svelte/store";

export const gameArtworkRevision = writable<Record<string, number>>({});

export type ArtworkKind = "icon" | "banner";

function artworkChanged(gameId: string, kind: ArtworkKind): void {
  const key = `${gameId}:${kind}`;
  gameArtworkRevision.update((revisions) => ({ ...revisions, [key]: (revisions[key] ?? 0) + 1 }));
  const entry = cache.get(key);
  if (entry !== undefined) {
    cache.delete(key);
    entry.obsolete = true;
    dispose(entry);
  }
}

interface CachedArtwork {
  promise: Promise<string | null>;
  url: string | null;
  users: number;
  obsolete: boolean;
  settled: boolean;
}

const cache = new Map<string, CachedArtwork>();

function dispose(entry: CachedArtwork): void {
  if (entry.users === 0 && entry.settled && entry.obsolete && entry.url !== null) {
    URL.revokeObjectURL(entry.url);
    entry.url = null;
  }
}

function prune(): void {
  const idle = [...cache].filter(([, entry]) => entry.users === 0 && entry.settled);
  for (const [key, entry] of idle.slice(0, Math.max(0, idle.length - 32))) {
    cache.delete(key);
    entry.obsolete = true;
    dispose(entry);
  }
}

export function acquireGameArtwork(gameId: string, kind: ArtworkKind): { url: string | null; ready: Promise<string | null>; release: () => void } {
  const key = `${gameId}:${kind}`;
  let entry = cache.get(key);
  if (entry === undefined) {
    const created: CachedArtwork = { promise: Promise.resolve(null), url: null, users: 0, obsolete: false, settled: false };
    created.promise = (kind === "icon" ? getGameIcon(gameId) : getGameBanner(gameId)).then((artwork) => {
      if (artwork !== null && !(created.obsolete && created.users === 0)) {
        created.url = URL.createObjectURL(new Blob([Uint8Array.from(artwork.bytes)], { type: artwork.contentType }));
      }
      return created.url;
    }).catch((error: unknown) => {
      if (cache.get(key) === created) cache.delete(key);
      created.obsolete = true;
      throw error;
    }).finally(() => { created.settled = true; dispose(created); prune(); });
    entry = created;
    cache.set(key, created);
  }
  cache.delete(key);
  cache.set(key, entry);
  entry.users++;
  const acquired = entry;
  let released = false;
  return {
    url: acquired.url,
    ready: acquired.promise,
    release: () => {
      if (released) return;
      released = true;
      acquired.users--;
      dispose(acquired);
      prune();
    },
  };
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
    artworkChanged(gameId, "icon");
    return result;
  });
}

export function extractGameIcon(gameId: string): Promise<GameArtwork> {
  return invoke<GameArtwork>("extract_game_icon", { gameId }).then((result) => {
    artworkChanged(gameId, "icon");
    return result;
  });
}

export function resetGameIcon(gameId: string): Promise<void> {
  return invoke<void>("reset_game_icon", { gameId }).then(() => artworkChanged(gameId, "icon"));
}

export function getGameBanner(gameId: string): Promise<GameArtwork | null> {
  return invoke<GameArtwork | null>("get_game_banner", { gameId });
}

export function setGameBanner(gameId: string, filePath: string): Promise<GameArtwork> {
  return invoke<GameArtwork>("set_game_banner", { gameId, filePath }).then((result) => {
    artworkChanged(gameId, "banner");
    return result;
  });
}

export function resetGameBanner(gameId: string): Promise<void> {
  return invoke<void>("reset_game_banner", { gameId }).then(() => artworkChanged(gameId, "banner"));
}
