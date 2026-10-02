import { invoke } from "@tauri-apps/api/core";

export interface SteamDetails {
  steamAppId: number;
  name: string;
  appType: string;
  shortDescription: string | null;
  detailedDescription: string | null;
  systemRequirements?: { minimum: string | null; recommended: string | null } | null;
  developers: string[];
  publishers: string[];
  genres: string[];
  platforms: { windows: boolean; mac: boolean; linux: boolean } | null;
  releaseDate: { comingSoon: boolean; date: string } | null;
  assets: {
    header: string | null;
    capsule: string | null;
    background: string | null;
    screenshots: { thumbnail: string | null; full: string | null }[];
  };
}

export interface SteamDetailsResult {
  details: SteamDetails | null;
  cachedAt: number | null;
  stale: boolean;
}

export interface SteamDetailsError {
  kind: "invalid_app_id" | "unavailable" | "invalid_response" | "timeout" | "network" | "http" | "too_large" | "database" | "internal";
  message: string;
  status: number | null;
}

export function getSteamDetails(steamAppId: number, refresh: boolean): Promise<SteamDetailsResult> {
  return invoke<SteamDetailsResult>("get_steam_details", { steamAppId, refresh });
}

export type SteamAssetKind = "header" | "capsule" | "screenshot" | "hero" | "logo" | "library_capsule" | "library_header" | "hero_blur" | "client_icon";

export interface SteamAsset {
  bytes: number[];
  contentType: string;
  stale: boolean;
  cacheWarning: string | null;
}

export function getSteamAsset(
  steamAppId: number,
  asset: SteamAssetKind,
  index?: number,
  full?: boolean,
): Promise<SteamAsset> {
  return invoke<SteamAsset>("get_steam_asset", { steamAppId, asset, index, full });
}

export interface SteamImageRequest {
  steamAppId: number;
  asset: SteamAssetKind;
  fallbackAsset: SteamAssetKind | null;
  index: number | null;
  version: number | null;
  full: boolean;
}

export interface SteamImage {
  url: string;
  stale: boolean;
  cacheWarning: string | null;
}

interface CachedSteamImage extends SteamImage {
  users: number;
}

const imageCache = new Map<string, CachedSteamImage>();
const pendingImages = new Map<string, Promise<SteamImage>>();
const maxIdleImages = 64;

function imageKey(request: SteamImageRequest): string {
  return JSON.stringify([
    request.steamAppId,
    request.asset,
    request.fallbackAsset,
    request.index,
    request.version,
    request.full,
  ]);
}

function cachedImage(key: string): CachedSteamImage | undefined {
  const image = imageCache.get(key);
  if (image !== undefined) {
    imageCache.delete(key);
    imageCache.set(key, image);
  }
  return image;
}

function pruneImages(protectedKey?: string): void {
  let idle = [...imageCache.values()].filter((image) => image.users === 0).length;
  for (const [key, image] of imageCache) {
    if (idle <= maxIdleImages) break;
    if (image.users > 0 || key === protectedKey) continue;
    URL.revokeObjectURL(image.url);
    imageCache.delete(key);
    idle--;
  }
}

export function peekSteamImage(request: SteamImageRequest): SteamImage | undefined {
  return cachedImage(imageKey(request));
}

export function retainSteamImage(request: SteamImageRequest): SteamImage | undefined {
  const image = cachedImage(imageKey(request));
  if (image !== undefined) image.users++;
  return image;
}

export function releaseSteamImage(request: SteamImageRequest): void {
  const image = imageCache.get(imageKey(request));
  if (image !== undefined) image.users--;
  pruneImages();
}

export function loadSteamImage(request: SteamImageRequest): Promise<SteamImage> {
  const key = imageKey(request);
  const cached = cachedImage(key);
  if (cached !== undefined) return Promise.resolve(cached);
  const pending = pendingImages.get(key);
  if (pending !== undefined) return pending;

  const load = getSteamAsset(
    request.steamAppId,
    request.asset,
    request.index ?? undefined,
    request.full,
  ).catch((error: unknown) => {
    if (request.fallbackAsset === null) throw error;
    return getSteamAsset(request.steamAppId, request.fallbackAsset, request.index ?? undefined, request.full);
  }).then((result) => {
    const image: CachedSteamImage = {
      url: URL.createObjectURL(new Blob([Uint8Array.from(result.bytes)], { type: result.contentType })),
      stale: result.stale,
      cacheWarning: result.cacheWarning,
      users: 0,
    };
    imageCache.set(key, image);
    pruneImages(key);
    return image;
  }).finally(() => pendingImages.delete(key));
  pendingImages.set(key, load);
  return load;
}
