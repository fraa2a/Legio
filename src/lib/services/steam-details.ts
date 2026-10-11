import { get, writable } from "svelte/store";
import { invoke } from "./invoke";
import { decodeImageResponse } from "./image-response";
import { artworkDisplayWidth } from "../stores/artwork-display";
import { createArtworkWork, isArtworkCancelled, type ArtworkWork } from "./artwork-work";
import { toMessage } from "../utils/errors";

export { getSteamDetails, type SteamDetails, type SteamDetailsResult, type SteamDetailsError } from "./steam-metadata";
export type SteamAssetKind = "header" | "capsule" | "screenshot" | "hero" | "logo" | "library_capsule" | "library_header" | "hero_blur" | "client_icon";
export type AssetPolicy = "cache_or_fetch" | "refresh_if_expired" | "force_refresh";
export type PublicationOutcome = "displayed" | "abandoned" | "rejected";
export interface SteamAsset {
  bytes: Uint8Array<ArrayBuffer>; contentType: string; stale: boolean; cacheWarning: string | null;
  refreshAfter: number; replacementToken?: string | null;
}
export interface SteamImageRequest {
  steamAppId: number; asset: SteamAssetKind; fallbackAsset: SteamAssetKind | null; index: number | null;
  version: number | null; full: boolean; displayWidth?: number; screenshotIdentity?: string;
}
export interface SteamImage { refreshAt: number; url: string; stale: boolean; cacheWarning: string | null }
export interface RetainedSteamImage extends SteamImage { release(): void }
interface Generation extends SteamImage { users: number; byteLength: number; retired: boolean }
interface ImageTask { work: ArtworkWork; users: number; promise: Promise<Generation> }
const imageCache = new Map<string, Generation>();
const requestsByKey = new Map<string, SteamImageRequest>();
const openRefreshes = new Map<number, { work: ArtworkWork; checked: Set<string> }>();
const pendingImages = new Map<string, ImageTask>();
const pendingAssets = new Map<string, Promise<SteamAsset>>();
const fallbackImages = new Map<string, { key: string; request: SteamImageRequest }>();
const appRefreshes = new Map<number, Promise<void>>();
const freshFor = 5 * 24 * 60 * 60 * 1000;
const maxIdleImages = 128;
const maxIdleBytes = 32 * 1024 * 1024;
export const steamImageRevision = writable(0);
function imageKey(request: SteamImageRequest): string {
  const adaptive = request.asset === "hero" || request.asset === "hero_blur";
  return JSON.stringify([request.steamAppId, request.asset, null,
    request.asset === "screenshot" && request.screenshotIdentity ? null : request.index,
    request.asset === "screenshot" ? request.screenshotIdentity ?? null : null,
    adaptive ? false : request.full, adaptive ? request.displayWidth ?? get(artworkDisplayWidth) : null]);
}
function requestKey(request: SteamImageRequest): string { return JSON.stringify([imageKey(request), request.fallbackAsset]); }
function resolvedKey(request: SteamImageRequest): string {
  const primary = imageKey(request); return imageCache.has(primary) ? primary : fallbackImages.get(requestKey(request))?.key ?? primary;
}
function touch(key: string): Generation | undefined {
  const image = imageCache.get(key); if (image) { imageCache.delete(key); imageCache.set(key, image); } return image;
}
function releaseGeneration(image: Generation): void {
  image.users--; if (image.retired && image.users === 0) URL.revokeObjectURL(image.url);
}
function retire(image: Generation): void { image.retired = true; if (image.users === 0) URL.revokeObjectURL(image.url); }
function pruneImages(protectedKey?: string): void {
  let idle = [...imageCache.values()].filter(image => image.users === 0).length;
  let bytes = [...imageCache.values()].filter(image => image.users === 0).reduce((sum, image) => sum + image.byteLength, 0);
  for (const [key, image] of imageCache) {
    if (idle <= maxIdleImages && bytes <= maxIdleBytes) break;
    if (image.users > 0 || key === protectedKey) continue;
    retire(image); imageCache.delete(key); if (!pendingImages.has(key)) requestsByKey.delete(key); idle--; bytes -= image.byteLength;
  }
  for (const [key, fallback] of fallbackImages) if (!imageCache.has(fallback.key)) fallbackImages.delete(key);
}
export function requestSteamAsset(request: SteamImageRequest, policy: AssetPolicy, work: ArtworkWork, publication: "display" | "native_only"): Promise<SteamAsset> {
  return invoke<ArrayBuffer>("get_steam_asset", { steamAppId: request.steamAppId, asset: request.asset, index: request.index ?? undefined, full: request.full,
    policy, screenshotIdentity: request.screenshotIdentity, requestId: work.requestId, origin: work.origin, publication }).then(decodeImageResponse<SteamAsset>);
}
export function getSteamAsset(steamAppId: number, asset: SteamAssetKind, index?: number, full = false, refresh = false, screenshotIdentity?: string): Promise<SteamAsset> {
  const request: SteamImageRequest = { steamAppId, asset, index: index ?? null, full, fallbackAsset: null, version: null, screenshotIdentity };
  const key = JSON.stringify([imageKey(request), refresh]); const pending = pendingAssets.get(key); if (pending) return pending;
  const work = createArtworkWork(steamAppId, "visible");
  const task = requestSteamAsset(request, refresh ? "force_refresh" : "cache_or_fetch", work, "native_only").finally(() => { work.release(); pendingAssets.delete(key); });
  pendingAssets.set(key, task); return task;
}
export function prefetchSteamHero(steamAppId: number): Promise<void> { return invoke("prefetch_steam_hero", { steamAppId }); }
async function finish(result: SteamAsset, outcome: PublicationOutcome): Promise<string | null> {
  if (!result.replacementToken) return result.cacheWarning;
  try { const publication = await invoke<{ cacheWarning: string | null }>("finish_steam_asset_update", { token: result.replacementToken, outcome }); return publication.cacheWarning ?? result.cacheWarning; }
  catch (error) { return toMessage(error); }
}
function beginImageTask(request: SteamImageRequest, origin: ArtworkWork["origin"], policy: AssetPolicy): ImageTask {
  const key = imageKey(request); requestsByKey.set(key, request); const existing = pendingImages.get(key); if (existing) return existing;
  const work = createArtworkWork(request.steamAppId, origin);
  const task: ImageTask = { work, users: 0, promise: Promise.resolve(undefined as never) };
  task.promise = requestSteamAsset(request, policy, work, "display").then(async result => {
    const previous = imageCache.get(key);
    if (previous && policy === "refresh_if_expired" && !result.replacementToken && !result.stale && result.refreshAfter === previous.refreshAt) return previous;
    if (task.users === 0) { await finish(result, "abandoned"); throw { kind: "cancelled", message: "Artwork request cancelled" }; }
    if (previous && result.stale) { previous.stale = true; previous.cacheWarning = result.cacheWarning; steamImageRevision.update(value => value + 1); if (policy === "force_refresh") throw new Error(result.cacheWarning ?? "Could not refresh Steam images"); return previous; }
    const url = URL.createObjectURL(new Blob([result.bytes], { type: result.contentType }));
    try {
      if ((previous || result.replacementToken) && typeof Image !== "undefined") { const candidate = new Image(); candidate.src = url; await candidate.decode(); }
    } catch (error) { URL.revokeObjectURL(url); await finish(result, "rejected"); throw error; }
    if (task.users === 0) { URL.revokeObjectURL(url); await finish(result, "abandoned"); throw { kind: "cancelled", message: "Artwork request cancelled" }; }
    const cacheWarning = await finish(result, "displayed");
    if (task.users === 0) { URL.revokeObjectURL(url); throw { kind: "cancelled", message: "Artwork request cancelled" }; }
    const image: Generation = { url, stale: result.stale, cacheWarning, refreshAt: result.stale ? 0 : result.refreshAfter ?? Date.now() + freshFor, byteLength: result.bytes.byteLength, users: 0, retired: false };
    imageCache.set(key, image); if (previous) retire(previous);
    pruneImages(key); steamImageRevision.update(value => value + 1); return image;
  }).catch(error => {
    const previous = imageCache.get(key);
    if (previous && !isArtworkCancelled(error)) { previous.cacheWarning = toMessage(error); steamImageRevision.update(value => value + 1); }
    throw error;
  }).finally(() => { if (pendingImages.get(key) === task) pendingImages.delete(key); work.release(); });
  pendingImages.set(key, task); return task;
}
function acquirePrimary(request: SteamImageRequest, origin: ArtworkWork["origin"], policy: AssetPolicy): { image?: Generation; promise: Promise<Generation>; release(): void } {
  const key = imageKey(request); const cached = policy === "cache_or_fetch" ? touch(key) : undefined;
  let generation = cached; let released = false; let task: ImageTask | undefined;
  if (cached) cached.users++; else { task = beginImageTask(request, origin, policy); task.users++; }
  const promise = cached ? Promise.resolve(cached) : task!.promise.then(image => { if (!released) { generation = image; image.users++; } return image; });
  return { image: cached, promise, release() {
    if (released) return; released = true;
    if (generation) releaseGeneration(generation);
    if (task && --task.users === 0 && pendingImages.get(key) === task) task.work.release();
    pruneImages(key);
  } };
}
export function acquireSteamImage(request: SteamImageRequest, origin: ArtworkWork["origin"] = "visible"): { image: SteamImage | undefined; promise: Promise<SteamImage>; release(): void } {
  const resolved = resolvedKey(request);
  if (imageCache.has(resolved)) { const retained = retainSteamImage(request)!; return { image: retained, promise: Promise.resolve(retained), release: retained.release }; }
  const primary = acquirePrimary(request, origin, "cache_or_fetch"); let fallback: ReturnType<typeof acquirePrimary> | undefined; let released = false;
  const promise = primary.promise.catch(async error => {
    if (request.fallbackAsset === null || isArtworkCancelled(error) || released) throw error;
    const fallbackRequest = { ...request, asset: request.fallbackAsset, fallbackAsset: null };
    fallback = acquirePrimary(fallbackRequest, origin, "cache_or_fetch");
    const image = await fallback.promise; fallbackImages.set(requestKey(request), { key: imageKey(fallbackRequest), request }); return image;
  }).then(image => {
    const opening = openRefreshes.get(request.steamAppId);
    if (opening && !opening.checked.has(imageKey(request)) && !opening.work.signal?.aborted) {
      opening.checked.add(imageKey(request));
      queueMicrotask(() => { void refreshRequest(request, "refresh_if_expired", opening.work).catch(() => undefined); });
    }
    return image;
  });
  return { image: primary.image, promise, release() { if (released) return; released = true; primary.release(); fallback?.release(); } };
}
export function peekSteamImage(request: SteamImageRequest): SteamImage | undefined { return touch(resolvedKey(request)); }
export function retainSteamImage(request: SteamImageRequest): RetainedSteamImage | undefined {
  const key = resolvedKey(request); const image = touch(key); if (!image) return undefined;
  image.users++; let released = false;
  return { ...image, release() { if (released) return; released = true; releaseGeneration(image); pruneImages(key); } };
}
export async function loadSteamImage(request: SteamImageRequest): Promise<SteamImage> {
  const consumer = acquireSteamImage(request); try { return await consumer.promise; } finally { consumer.release(); }
}
function requestFromKey(key: string): SteamImageRequest {
  const known = requestsByKey.get(key); if (known) return known;
  const [steamAppId, asset, fallbackAsset, index, screenshotIdentity, full, displayWidth] = JSON.parse(key);
  return { steamAppId, asset, fallbackAsset, index, screenshotIdentity: screenshotIdentity ?? undefined, version: null, full, displayWidth: displayWidth ?? undefined };
}
async function refreshRequest(request: SteamImageRequest, policy: AssetPolicy, work?: ArtworkWork): Promise<void> {
  const origin = request.asset === "hero_blur" ? "decorative" : request.asset === "screenshot" ? "prefetch" : "visible";
  const primary = acquirePrimary(request, origin, policy); let fallback: ReturnType<typeof acquirePrimary> | undefined;
  const cancel = () => { primary.release(); fallback?.release(); };
  work?.signal?.addEventListener("abort", cancel, { once: true });
  if (work?.signal?.aborted) cancel();
  try { await primary.promise; fallbackImages.delete(requestKey(request)); }
  catch (error) {
    if (request.fallbackAsset === null || isArtworkCancelled(error)) throw error;
    const fallbackRequest = { ...request, asset: request.fallbackAsset, fallbackAsset: null };
    fallback = acquirePrimary(fallbackRequest, "visible", policy); await fallback.promise;
    fallbackImages.set(requestKey(request), { key: imageKey(fallbackRequest), request });
  } finally { work?.signal?.removeEventListener("abort", cancel); cancel(); }
}
function refreshImages(steamAppId: number, policy: AssetPolicy, work?: ArtworkWork): Promise<void> {
  const existing = appRefreshes.get(steamAppId); if (existing) return existing;
  const requests = new Map<string, SteamImageRequest>();
  for (const key of imageCache.keys()) { const request = requestFromKey(key);
    if (request.steamAppId === steamAppId && (!["hero", "hero_blur"].includes(request.asset) || request.displayWidth === undefined || request.displayWidth === get(artworkDisplayWidth))) requests.set(key, request);
  }
  for (const fallback of fallbackImages.values()) if (fallback.request.steamAppId === steamAppId && (!["hero", "hero_blur"].includes(fallback.request.asset) || fallback.request.displayWidth === undefined || fallback.request.displayWidth === get(artworkDisplayWidth))) requests.set(imageKey(fallback.request), fallback.request);
  const opening = openRefreshes.get(steamAppId);
  if (policy === "refresh_if_expired" && opening) for (const key of requests.keys()) opening.checked.add(key);
  const ordered = [...requests.values()].sort((a, b) => rank(a.asset) - rank(b.asset));
  const task = Promise.allSettled(ordered.map(request => refreshRequest(request, policy, work))).then(results => {
    const failure = results.find(result => result.status === "rejected" && !isArtworkCancelled(result.reason));
    if (failure?.status === "rejected") throw failure.reason;
  }).finally(() => { if (appRefreshes.get(steamAppId) === task) appRefreshes.delete(steamAppId); });
  appRefreshes.set(steamAppId, task); return task;
}
function rank(asset: SteamAssetKind): number { return asset === "hero" || asset === "header" ? 0 : asset === "logo" ? 1 : asset === "hero_blur" ? 3 : 2; }
export function refreshSteamImages(steamAppId: number): Promise<void> { return refreshImages(steamAppId, "force_refresh"); }
export function refreshSteamImagesOnOpen(steamAppId: number, work: ArtworkWork): Promise<void> {
  const opening = { work, checked: new Set<string>() }; openRefreshes.set(steamAppId, opening);
  work.signal?.addEventListener("abort", () => { if (openRefreshes.get(steamAppId) === opening) openRefreshes.delete(steamAppId); }, { once: true });
  return refreshImages(steamAppId, "refresh_if_expired", work);
}
