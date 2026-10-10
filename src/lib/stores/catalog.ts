import { get, writable } from "svelte/store";
import { cachedCatalogSearch, refreshCatalogCached, refreshCatalogPage, rememberCatalogSearch, searchCatalog, type CatalogGame, type CatalogSearch, type CatalogOptions } from "../services/catalog";
import { toMessage } from "../utils/errors";
import type { LoadStatus } from "./resource";
import { source } from "./source";
import { sourceStatusFor } from "../features/store/source-status";

interface CatalogState {
  query: string;
  options: CatalogOptions;
  status: LoadStatus;
  results: CatalogGame[];
  total: number;
  nextOffset: number | null;
  sourceStale: boolean;
  stale: boolean;
  refreshing: boolean;
  error: string | null;
}

const initial: CatalogState = {
  query: "",
  options: { sort: "relevance", availability: "all" },
  status: "idle",
  results: [],
  total: 0,
  nextOffset: null,
  sourceStale: false,
  stale: false,
  refreshing: false,
  error: null,
};

export const catalog = writable<CatalogState>(initial);

let requestId = 0;
let previousManifest = get(source).data.manifest;

source.subscribe(({ data }) => {
  if (data.manifest !== previousManifest) {
    previousManifest = data.manifest;
    requestId++;
  }
  catalog.update((state) => ({
    ...state,
    results: state.results.map((game) => ({ ...game, availability: sourceStatusFor(data.manifest, game.steamAppId).availability })),
    sourceStale: data.stale,
  }));
});

function applySearch(state: CatalogState, search: CatalogSearch, refreshing: boolean): CatalogState {
  const snapshot = get(source).data;
  return {
    ...state,
    status: search.games.length === 0 ? "empty" : "ready",
    results: search.games.map((game) => ({ ...game, availability: sourceStatusFor(snapshot.manifest, game.steamAppId).availability })),
    total: search.total,
    nextOffset: search.nextOffset,
    stale: search.stale,
    sourceStale: snapshot.stale,
    refreshing,
    error: null,
  };
}

export async function runCatalogSearch(query: string, options: CatalogOptions = get(catalog).options, forceRefresh = false): Promise<void> {
  const request = ++requestId;
  const trimmed = query.trim();
  options = { ...options };
  const cached = forceRefresh ? null : cachedCatalogSearch(trimmed, options);
  if (cached !== null) {
    catalog.update((state) => applySearch({ ...state, query: trimmed, options }, cached, false));
    return;
  }
  catalog.update(() => ({ ...initial, query: trimmed, options, status: "loading" }));

  let local: CatalogSearch;
  try {
    local = await searchCatalog(trimmed, undefined, options);
  } catch (error) {
    if (request !== requestId) return;
    catalog.update((state) => ({ ...state, status: "error", refreshing: false, error: toMessage(error) }));
    return;
  }
  if (request !== requestId) return;
  catalog.update((state) => applySearch(state, local, true));

  try {
    const fresh = await refreshCatalogCached(trimmed, options, forceRefresh);
    if (request !== requestId) return;
    catalog.update((state) => applySearch(state, fresh, false));
  } catch (error) {
    if (request !== requestId) return;
    catalog.update((state) => ({ ...state, refreshing: false, error: toMessage(error) }));
  }
}

export async function loadMoreCatalog(query: string, limit: number): Promise<void> {
  const current = get(catalog);
  if (current.query !== query.trim() || current.refreshing) return;
  const request = requestId;
  catalog.update((state) => ({ ...state, refreshing: true, error: null }));
  try {
    let result: CatalogSearch | null = null;
    if (current.total > current.results.length) {
      result = { ...await searchCatalog(current.query, limit, current.options), nextOffset: current.nextOffset };
      if (request === requestId) rememberCatalogSearch(current.query, result, current.options);
    } else if (current.nextOffset !== null) {
      result = await refreshCatalogPage(current.query, current.nextOffset, limit, current.options);
    }
    if (request !== requestId) return;
    if (result !== null) catalog.update((state) => applySearch(state, result, false));
    else catalog.update((state) => ({ ...state, refreshing: false }));
  } catch (error) {
    if (request === requestId) catalog.update((state) => ({ ...state, refreshing: false, error: toMessage(error) }));
  }
}
