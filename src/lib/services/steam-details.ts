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

const pendingAssets = new Map<string, Promise<SteamAsset>>();

export function getSteamAsset(
  steamAppId: number,
  asset: SteamAssetKind,
  index?: number,
  full?: boolean,
  refresh = false,
): Promise<SteamAsset> {
  const adaptive = asset === "hero" || asset === "hero_blur";
  const key = JSON.stringify([steamAppId, asset, index ?? null, adaptive ? false : full ?? false, refresh, adaptive ? get(artworkDisplayWidth) : null]);
  const pending = pendingAssets.get(key);
  if (pending !== undefined) return pending;
  const request = invoke<ArrayBuffer>("get_steam_asset", { steamAppId, asset, index, full, refresh })
    .then(decodeImageResponse<SteamAsset>)
    .finally(() => pendingAssets.delete(key));
  pendingAssets.set(key, request);
  return request;
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

export interface RetainedSteamImage extends SteamImage {
  release: () => void;
}

interface CachedSteamImage extends SteamImage {
  users: number;
  byteLength: number;
}

const imageCache = new Map<string, CachedSteamImage>();
const pendingImages = new Map<string, Promise<SteamImage>>();
const fallbackImages = new Map<string, { key: string; request: SteamImageRequest; retryAt: number }>();
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
    null,
    request.index,
    ["header", "capsule", "screenshot"].includes(request.asset) ? request.version : null,
    ["hero", "hero_blur"].includes(request.asset) ? false : request.full,
    ["hero", "hero_blur"].includes(request.asset) ? request.displayWidth ?? get(artworkDisplayWidth) : null,
  ]);
}

function requestKey(request: SteamImageRequest): string {
  return JSON.stringify([imageKey(request), request.fallbackAsset]);
}

function resolvedImageKey(request: SteamImageRequest): string {
  const primary = imageKey(request);
  if (imageCache.has(primary)) return primary;
  return fallbackImages.get(requestKey(request))?.key ?? primary;
}

function imageRefreshAt(request: SteamImageRequest, image: CachedSteamImage): number {
  const fallback = imageCache.has(imageKey(request)) ? undefined : fallbackImages.get(requestKey(request));
  return fallback?.retryAt ?? image.refreshAt;
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
  for (const [key, fallback] of fallbackImages) {
    if (!imageCache.has(fallback.key)) fallbackImages.delete(key);
  }
}

export function peekSteamImage(request: SteamImageRequest): SteamImage | undefined {
  return cachedImage(resolvedImageKey(request));
}

export function retainSteamImage(request: SteamImageRequest): RetainedSteamImage | undefined {
  const key = resolvedImageKey(request);
  const image = cachedImage(key);
  if (image === undefined) return undefined;
  image.users++;
  if (Date.now() >= imageRefreshAt(request, image)) refreshImage(request, imageKey(request), image);
  let released = false;
  return {
    ...image,
    refreshAt: imageRefreshAt(request, image),
    release: () => {
      if (released) return;
      released = true;
      const current = imageCache.get(key);
      if (current !== undefined) current.users--;
      pruneImages();
    },
  };
}

export function loadSteamImage(request: SteamImageRequest): Promise<SteamImage> {
  const batch = appRefreshes.get(request.steamAppId);
  if (batch !== undefined) return batch.then(() => loadSteamImage(request));
  const key = resolvedImageKey(request);
  const cached = cachedImage(key);
  if (cached !== undefined) {
    if (Date.now() >= imageRefreshAt(request, cached)) refreshImage(request, imageKey(request), cached);
    return Promise.resolve(cached);
  }
  return loadPrimaryImage(request).catch(async (error: unknown) => {
    if (request.fallbackAsset === null) throw error;
    const fallback = { ...request, asset: request.fallbackAsset, fallbackAsset: null };
    const image = await loadPrimaryImage(fallback);
    fallbackImages.set(requestKey(request), { key: imageKey(fallback), request, retryAt: image.refreshAt });
    return image;
  });
}

function loadPrimaryImage(request: SteamImageRequest): Promise<SteamImage> {
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
  ).then((result) => {
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
    if (result.stale) refreshImage(requestFromKey(key), key, image);
    return image;
  }).finally(() => pendingImages.delete(key));
  pendingImages.set(key, load);
  return load;
}

function refreshImage(request: SteamImageRequest, key: string, previous: CachedSteamImage): void {
  if (["hero", "hero_blur"].includes(request.asset) && request.displayWidth !== undefined
    && request.displayWidth !== get(artworkDisplayWidth)) return;
  if (refreshing.has(key) || appRefreshes.has(request.steamAppId)) return;
  const sourceKey = resolvedImageKey(request);
  const fallback = sourceKey === key ? undefined : fallbackImages.get(requestKey(request));
  if (fallback !== undefined) fallback.retryAt = Date.now() + freshFor;
  else previous.refreshAt = Date.now() + freshFor;
  let resolved = request;
  const task = getSteamAsset(request.steamAppId, request.asset, request.index ?? undefined, request.full, true)
    .catch((error: unknown) => {
      if (request.fallbackAsset === null) throw error;
      resolved = { ...request, asset: request.fallbackAsset, fallbackAsset: null };
      return getSteamAsset(request.steamAppId, resolved.asset, request.index ?? undefined, request.full, true);
    })
    .then((result) => {
      if (imageCache.get(sourceKey) !== previous) return;
      if (result.stale) {
        previous.cacheWarning = result.cacheWarning;
        if (fallback !== undefined) fallback.retryAt = Date.now() + 5 * 60 * 1000;
        else previous.refreshAt = Date.now() + 5 * 60 * 1000;
      } else {
        const resolvedKey = imageKey(resolved);
        if (resolvedKey === sourceKey) {
          const oldUrl = previous.url;
          previous.url = URL.createObjectURL(new Blob([result.bytes], { type: result.contentType }));
          previous.byteLength = result.bytes.byteLength;
          previous.stale = false;
          previous.refreshAt = result.refreshAfter ?? Date.now() + freshFor;
          previous.cacheWarning = result.cacheWarning;
          URL.revokeObjectURL(oldUrl);
        } else if (!imageCache.has(resolvedKey)) {
          imageCache.set(resolvedKey, {
            url: URL.createObjectURL(new Blob([result.bytes], { type: result.contentType })),
            byteLength: result.bytes.byteLength, users: 0, stale: false,
            refreshAt: result.refreshAfter ?? Date.now() + freshFor, cacheWarning: result.cacheWarning,
          });
        }
        if (resolved !== request) fallbackImages.set(requestKey(request), { key: resolvedKey, request, retryAt: result.refreshAfter ?? Date.now() + freshFor });
        else fallbackImages.delete(requestKey(request));
      }
      steamImageRevision.update(value => value + 1);
      pruneImages(imageKey(resolved));
    })
    .catch((error: unknown) => {
      if (imageCache.get(sourceKey) !== previous) return;
      previous.cacheWarning = error instanceof Error ? error.message : String(error);
      if (fallback !== undefined) fallback.retryAt = Date.now() + 5 * 60 * 1000;
      else previous.refreshAt = Date.now() + 5 * 60 * 1000;
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
    for (const fallback of fallbackImages.values()) {
      if (fallback.request.steamAppId === steamAppId && (!["hero", "hero_blur"].includes(fallback.request.asset)
        || (fallback.request.displayWidth ?? get(artworkDisplayWidth)) === get(artworkDisplayWidth))) {
        requests.set(imageKey(fallback.request), fallback.request);
      }
    }
    const updated = new Set<string>();
    const results = await Promise.allSettled([...requests].map(async ([, request]) => {
      let resolved = request;
      const result = await getSteamAsset(steamAppId, request.asset, request.index ?? undefined, request.full, true).catch((error: unknown) => {
        if (request.fallbackAsset === null) throw error;
        resolved = { ...request, asset: request.fallbackAsset, fallbackAsset: null };
        return getSteamAsset(steamAppId, resolved.asset, request.index ?? undefined, request.full, true);
      });
      const key = imageKey(resolved);
      if (resolved !== request) fallbackImages.set(requestKey(request), { key, request, retryAt: result.refreshAfter ?? Date.now() + freshFor });
      else fallbackImages.delete(requestKey(request));
      if (result.stale) throw new Error(result.cacheWarning ?? "Could not refresh Steam images");
      if (updated.has(key)) return;
      updated.add(key);
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
