import { get, writable } from "svelte/store";
import { language } from "../i18n";
import { invoke } from "@tauri-apps/api/core";
import { decodeImageResponse } from "./image-response";
import { artworkDisplayWidth } from "../stores/artwork-display";

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
  return invoke<SteamDetailsResult>("get_steam_details", { steamAppId, refresh, language: get(language) });
}

export type SteamAssetKind = "header" | "capsule" | "screenshot" | "hero" | "logo" | "library_capsule" | "library_header" | "hero_blur" | "client_icon";

export interface SteamAsset {
  bytes: Uint8Array<ArrayBuffer>;
  contentType: string;
  stale: boolean;
  cacheWarning: string | null;
  refreshAfter: number;
}

export function getSteamAsset(
  steamAppId: number,
  asset: SteamAssetKind,
  index?: number,
  full?: boolean,
  refresh = false,
): Promise<SteamAsset> {
  return invoke<ArrayBuffer>("get_steam_asset", { steamAppId, asset, index, full, refresh }).then(decodeImageResponse<SteamAsset>);
}

export function prefetchSteamHero(steamAppId: number): Promise<void> {
  return invoke("prefetch_steam_hero", { steamAppId });
}

export interface SteamImageRequest {
  steamAppId: number;
  asset: SteamAssetKind;
  fallbackAsset: SteamAssetKind | null;
  index: number | null;
  version: number | null;
  full: boolean;
  displayWidth?: number;
}

export interface SteamImage {
  refreshAt: number;
  url: string;
  stale: boolean;
  cacheWarning: string | null;
}

interface CachedSteamImage extends SteamImage {
  users: number;
  byteLength: number;
}

const imageCache = new Map<string, CachedSteamImage>();
const pendingImages = new Map<string, Promise<SteamImage>>();
const maxIdleImages = 128;
const maxIdleBytes = 32 * 1024 * 1024;
const freshFor = 72 * 60 * 60 * 1000;
const refreshing = new Map<string, Promise<void>>();
const appRefreshes = new Map<number, Promise<void>>();
export const steamImageRevision = writable(0);

function imageKey(request: SteamImageRequest): string {
  return JSON.stringify([
    request.steamAppId,
    request.asset,
    request.fallbackAsset,
    request.index,
    ["header", "capsule", "screenshot"].includes(request.asset) ? request.version : null,
    ["hero", "hero_blur"].includes(request.asset) ? false : request.full,
    ["hero", "hero_blur"].includes(request.asset) ? request.displayWidth ?? get(artworkDisplayWidth) : null,
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
  let bytes = [...imageCache.values()].filter((image) => image.users === 0).reduce((total, image) => total + image.byteLength, 0);
  for (const [key, image] of imageCache) {
    if (idle <= maxIdleImages && bytes <= maxIdleBytes) break;
    if (image.users > 0 || key === protectedKey) continue;
    URL.revokeObjectURL(image.url);
    imageCache.delete(key);
    idle--;
    bytes -= image.byteLength;
  }
}

export function peekSteamImage(request: SteamImageRequest): SteamImage | undefined {
  return cachedImage(imageKey(request));
}

export function retainSteamImage(request: SteamImageRequest): SteamImage | undefined {
  const image = cachedImage(imageKey(request));
  if (image !== undefined) {
    image.users++;
    if (Date.now() >= image.refreshAt) refreshImage(request, imageKey(request), image);
  }
  return image;
}

export function releaseSteamImage(request: SteamImageRequest): void {
  const image = imageCache.get(imageKey(request));
  if (image !== undefined) image.users--;
  pruneImages();
}

export function loadSteamImage(request: SteamImageRequest): Promise<SteamImage> {
  const batch = appRefreshes.get(request.steamAppId);
  if (batch !== undefined) return batch.then(() => loadSteamImage(request));
  const key = imageKey(request);
  const cached = cachedImage(key);
  if (cached !== undefined) {
    if (Date.now() >= cached.refreshAt) refreshImage(request, key, cached);
    return Promise.resolve(cached);
  }
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
      url: URL.createObjectURL(new Blob([result.bytes], { type: result.contentType })),
      stale: result.stale,
      cacheWarning: result.cacheWarning,
      users: 0,
      byteLength: result.bytes.byteLength,
      refreshAt: result.stale ? 0 : result.refreshAfter ?? Date.now() + freshFor,
    };
    imageCache.set(key, image);
    pruneImages(key);
    if (result.stale) refreshImage(request, key, image);
    return image;
  }).finally(() => pendingImages.delete(key));
  pendingImages.set(key, load);
  return load;
}

function refreshImage(request: SteamImageRequest, key: string, previous: CachedSteamImage): void {
  if (["hero", "hero_blur"].includes(request.asset) && request.displayWidth !== undefined
    && request.displayWidth !== get(artworkDisplayWidth)) return;
  if (refreshing.has(key) || appRefreshes.has(request.steamAppId)) return;
  previous.refreshAt = Date.now() + freshFor;
  const task = getSteamAsset(request.steamAppId, request.asset, request.index ?? undefined, request.full, true)
    .catch((error: unknown) => {
      if (request.fallbackAsset === null) throw error;
      return getSteamAsset(request.steamAppId, request.fallbackAsset, request.index ?? undefined, request.full, true);
    })
    .then((result) => {
      if (imageCache.get(key) !== previous) return;
      if (result.stale) {
        previous.cacheWarning = result.cacheWarning;
        previous.refreshAt = Date.now() + 5 * 60 * 1000;
      } else {
        const oldUrl = previous.url;
        previous.url = URL.createObjectURL(new Blob([result.bytes], { type: result.contentType }));
        previous.byteLength = result.bytes.byteLength;
        previous.stale = false;
        previous.refreshAt = result.refreshAfter ?? Date.now() + freshFor;
        previous.cacheWarning = result.cacheWarning;
        URL.revokeObjectURL(oldUrl);
      }
      steamImageRevision.update(value => value + 1);
    })
    .catch((error: unknown) => {
      if (imageCache.get(key) !== previous) return;
      previous.cacheWarning = error instanceof Error ? error.message : String(error);
      previous.refreshAt = Date.now() + 5 * 60 * 1000;
      steamImageRevision.update(value => value + 1);
    })
    .finally(() => refreshing.delete(key));
  refreshing.set(key, task);
}

function requestFromKey(key: string): SteamImageRequest {
  const [steamAppId, asset, fallbackAsset, index, version, full, displayWidth] = JSON.parse(key);
  return { steamAppId, asset, fallbackAsset, index, version, full, displayWidth: displayWidth ?? undefined };
}

export function refreshSteamImages(steamAppId: number): Promise<void> {
  const current = appRefreshes.get(steamAppId);
  if (current !== undefined) return current;
  const task = (async () => {
    const inflight = [...pendingImages, ...refreshing].filter(([key]) => requestFromKey(key).steamAppId === steamAppId);
    await Promise.allSettled(inflight.map(([, promise]) => promise));
    await invoke("reset_steam_artwork_cache", { steamAppId });
    const requests = new Map<string, SteamImageRequest>();
    for (const key of imageCache.keys()) {
      const request = requestFromKey(key);
      if (request.steamAppId === steamAppId && (!["hero", "hero_blur"].includes(request.asset)
        || request.displayWidth === get(artworkDisplayWidth))) requests.set(key, request);
    }
    const results = await Promise.allSettled([...requests].map(async ([key, request]) => {
      const result = await getSteamAsset(steamAppId, request.asset, request.index ?? undefined, request.full, true).catch((error: unknown) => {
        if (request.fallbackAsset === null) throw error;
        return getSteamAsset(steamAppId, request.fallbackAsset, request.index ?? undefined, request.full, true);
      });
      if (result.stale) throw new Error(result.cacheWarning ?? "Could not refresh Steam images");
      const previous = imageCache.get(key);
      const url = URL.createObjectURL(new Blob([result.bytes], { type: result.contentType }));
      imageCache.set(key, {
        url, users: previous?.users ?? 0, byteLength: result.bytes.byteLength,
        stale: false, cacheWarning: result.cacheWarning, refreshAt: result.refreshAfter ?? Date.now() + freshFor,
      });
      if (previous !== undefined) URL.revokeObjectURL(previous.url);
    }));
    steamImageRevision.update((value) => value + 1);
    pruneImages();
    const failures = results.filter((result) => result.status === "rejected");
    if (failures.length > 0) throw failures[0].reason;
  })().finally(() => appRefreshes.delete(steamAppId));
  appRefreshes.set(steamAppId, task);
  return task;
}
