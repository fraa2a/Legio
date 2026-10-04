import { writable } from "svelte/store";
import { cachedCatalogSearch, refreshCatalogCached, searchCatalog, type CatalogGame, type CatalogSearch } from "../services/catalog";
import { toMessage } from "../utils/errors";
import type { LoadStatus } from "./resource";

interface CatalogState {
  query: string;
  status: LoadStatus;
  results: CatalogGame[];
  total: number;
  sourceStale: boolean;
  stale: boolean;
  refreshing: boolean;
  error: string | null;
}

const initial: CatalogState = {
  query: "",
  status: "idle",
  results: [],
  total: 0,
  sourceStale: false,
  stale: false,
  refreshing: false,
  error: null,
};

export const catalog = writable<CatalogState>(initial);

let requestId = 0;

function applySearch(state: CatalogState, search: CatalogSearch, refreshing: boolean): CatalogState {
  return {
    ...state,
    status: search.games.length === 0 ? "empty" : "ready",
    results: search.games,
    total: search.total,
    stale: search.stale,
    sourceStale: search.sourceStale,
    refreshing,
    error: null,
  };
}

export async function runCatalogSearch(query: string): Promise<void> {
  const request = ++requestId;
  const trimmed = query.trim();
  if (trimmed.length === 0) {
    catalog.set(initial);
    try {
      const local = await searchCatalog("");
      if (request === requestId) catalog.update((state) => applySearch(state, local, false));
    } catch (error) {
      if (request === requestId) catalog.update((state) => ({ ...state, error: toMessage(error) }));
    }
    return;
  }
  const cached = cachedCatalogSearch(trimmed);
  if (cached !== null) {
    catalog.update((state) => applySearch({ ...state, query: trimmed }, cached, false));
    return;
  }
  catalog.update((state) => ({ ...state, query: trimmed, status: "loading", error: null }));

  let local: CatalogSearch;
  try {
    local = await searchCatalog(trimmed);
  } catch (error) {
    if (request !== requestId) return;
    catalog.update((state) => ({ ...state, status: "error", refreshing: false, error: toMessage(error) }));
    return;
  }
  if (request !== requestId) return;
  catalog.update((state) => applySearch(state, local, true));

  try {
    const fresh = await refreshCatalogCached(trimmed);
    if (request !== requestId) return;
    catalog.update((state) => applySearch(state, fresh, false));
  } catch (error) {
    if (request !== requestId) return;
    catalog.update((state) => ({ ...state, refreshing: false, error: toMessage(error) }));
  }
}
