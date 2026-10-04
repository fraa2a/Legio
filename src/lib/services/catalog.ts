import { invoke } from "@tauri-apps/api/core";

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

export function searchCatalog(query: string, limit?: number): Promise<CatalogSearch> {
  return invoke<CatalogSearch>("search_catalog", { query, limit });
}

export function refreshCatalog(query: string, skip = 0, limit?: number): Promise<CatalogSearch> {
  return invoke<CatalogSearch>("refresh_catalog", { query, skip, limit });
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
  const request = refreshCatalogPage(query, 0).finally(() => pending.delete(key));
  pending.set(key, request);
  return request;
}

export async function refreshCatalogPage(query: string, skip: number, limit?: number): Promise<CatalogSearch> {
  const result = await refreshCatalog(query, skip, limit);
  rememberCatalogSearch(query, result);
  return result;
}

export function rememberCatalogSearch(query: string, result: CatalogSearch): void {
  const key = query.trim().toLocaleLowerCase();
  recent.delete(key);
  recent.set(key, { result, at: Date.now() });
  if (recent.size > 100) {
    const oldest = recent.keys().next().value;
    if (oldest !== undefined) recent.delete(oldest);
  }
}
