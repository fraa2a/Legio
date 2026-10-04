import { invoke } from "@tauri-apps/api/core";

export interface CatalogGame {
  steamAppId: number;
  name: string;
  availability: "unknown" | "unavailable" | "verified" | "unverified";
}

export interface CatalogSearch {
  games: CatalogGame[];
  total: number;
  cachedAt: number | null;
  stale: boolean;
  sourceCachedAt: number | null;
  sourceStale: boolean;
}

export function searchCatalog(query: string): Promise<CatalogSearch> {
  return invoke<CatalogSearch>("search_catalog", { query });
}

export function refreshCatalog(query: string): Promise<CatalogSearch> {
  return invoke<CatalogSearch>("refresh_catalog", { query });
}

const recent = new Map<string, { result: CatalogSearch; at: number }>();
const pending = new Map<string, Promise<CatalogSearch>>();
const maxAge = 24 * 60 * 60 * 1000;

export function cachedCatalogSearch(query: string): CatalogSearch | null {
  const entry = recent.get(query.trim().toLocaleLowerCase());
  return entry !== undefined && Date.now() - entry.at < maxAge ? entry.result : null;
}

export function refreshCatalogCached(query: string): Promise<CatalogSearch> {
  const key = query.trim().toLocaleLowerCase();
  const cached = cachedCatalogSearch(key);
  if (cached !== null) return Promise.resolve(cached);
  const active = pending.get(key);
  if (active !== undefined) return active;
  const request = refreshCatalog(query).then((result) => {
    recent.set(key, { result, at: Date.now() });
    if (recent.size > 100) {
      const oldest = recent.keys().next().value;
      if (oldest !== undefined) recent.delete(oldest);
    }
    return result;
  }).finally(() => pending.delete(key));
  pending.set(key, request);
  return request;
}
