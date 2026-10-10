import { invoke } from "./invoke";

export type CatalogSort = "relevance" | "name_asc" | "name_desc";
export type CatalogAvailability = "all" | "available" | "verified";
export interface CatalogOptions {
  sort: CatalogSort;
  availability: CatalogAvailability;
}

const defaults: CatalogOptions = { sort: "relevance", availability: "all" };

export interface CatalogGame {
  steamAppId: number;
  name: string;
  availability: "unknown" | "unavailable" | "verified" | "unverified";
}

export interface CatalogSearch {
  games: CatalogGame[];
  total: number;
  nextOffset: number | null;
  cachedAt: number | null;
  stale: boolean;
  sourceCachedAt: number | null;
  sourceStale: boolean;
}

export function searchCatalog(query: string, limit?: number, options: CatalogOptions = defaults): Promise<CatalogSearch> {
  return invoke<CatalogSearch>("search_catalog", { query, limit, options });
}

export function refreshCatalog(query: string, skip = 0, limit?: number, options: CatalogOptions = defaults): Promise<CatalogSearch> {
  return invoke<CatalogSearch>("refresh_catalog", { query, skip, limit, options });
}

const recent = new Map<string, { result: CatalogSearch; at: number }>();
const pending = new Map<string, Promise<CatalogSearch>>();
const maxAge = 24 * 60 * 60 * 1000;
let generation = 0;

function cacheKey(query: string, options: CatalogOptions): string {
  return JSON.stringify([query.trim().toLocaleLowerCase(), options.sort, options.availability]);
}

export function invalidateCatalogAvailability(): void {
  generation++;
  recent.clear();
  pending.clear();
}

export function cachedCatalogSearch(query: string, options: CatalogOptions = defaults): CatalogSearch | null {
  const entry = recent.get(cacheKey(query, options));
  return entry !== undefined && Date.now() - entry.at < maxAge ? entry.result : null;
}

export function refreshCatalogCached(query: string, options: CatalogOptions = defaults, forceRefresh = false): Promise<CatalogSearch> {
  const key = cacheKey(query, options);
  const cached = forceRefresh ? null : cachedCatalogSearch(query, options);
  if (cached !== null) return Promise.resolve(cached);
  const active = pending.get(key);
  if (active !== undefined) return active;
  const request = refreshCatalogPage(query, 0, undefined, options).finally(() => {
    if (pending.get(key) === request) pending.delete(key);
  });
  pending.set(key, request);
  return request;
}

export async function refreshCatalogPage(query: string, skip: number, limit?: number, options: CatalogOptions = defaults): Promise<CatalogSearch> {
  const started = generation;
  const result = await refreshCatalog(query, skip, limit, options);
  if (started === generation) rememberCatalogSearch(query, result, options);
  return result;
}

export function rememberCatalogSearch(query: string, result: CatalogSearch, options: CatalogOptions = defaults): void {
  const key = cacheKey(query, options);
  recent.delete(key);
  recent.set(key, { result, at: Date.now() });
  if (recent.size > 100) {
    const oldest = recent.keys().next().value;
    if (oldest !== undefined) recent.delete(oldest);
  }
}
