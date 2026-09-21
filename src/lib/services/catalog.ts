import { invoke } from "@tauri-apps/api/core";

export interface CatalogGame {
  steamAppId: number;
  name: string;
}

export interface CatalogSearch {
  games: CatalogGame[];
  total: number;
  cachedAt: number | null;
  stale: boolean;
}

export function searchCatalog(query: string): Promise<CatalogSearch> {
  return invoke<CatalogSearch>("search_catalog", { query });
}

export function refreshCatalog(query: string): Promise<CatalogSearch> {
  return invoke<CatalogSearch>("refresh_catalog", { query });
}
