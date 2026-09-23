<script lang="ts">
  import { refreshCatalog, searchCatalog, type CatalogSearch } from "../services/catalog";
  import { getLegioSource, refreshLegioSource, type SourceEntry, type SourceSnapshot } from "../services/legio-source";

  let query = $state("");
  let result = $state<CatalogSearch | null>(null);
  let searching = $state(false);
  let refreshing = $state(false);
  let searchError = $state<string | null>(null);
  let refreshError = $state<string | null>(null);
  let source = $state<SourceSnapshot | null>(null);
  let sourceError = $state<string | null>(null);
  let sourceRefreshing = $state(false);
  let searchGeneration = 0;

  function messageFor(reason: unknown): string {
    if (typeof reason === "object" && reason !== null && "message" in reason && typeof reason.message === "string") {
      return reason.message;
    }
    return String(reason);
  }

  async function search(value: string) {
    const generation = ++searchGeneration;
    searching = true;
    searchError = null;
    result = null;
    try {
      const response = await searchCatalog(value);
      if (generation === searchGeneration) result = response;
    } catch (reason) {
      if (generation === searchGeneration) searchError = messageFor(reason);
    } finally {
      if (generation === searchGeneration) searching = false;
    }
  }

  async function refresh() {
    refreshing = true;
    refreshError = null;
    try {
      await refreshCatalog(query);
      // The query may have changed while the remote refresh was in flight.
      await search(query);
    } catch (reason) {
      refreshError = messageFor(reason);
    } finally {
      refreshing = false;
    }
  }

  async function loadSource() {
    try {
      source = await getLegioSource();
      sourceError = null;
    } catch (reason) {
      sourceError = messageFor(reason);
    }
  }

  async function refreshSource() {
    sourceRefreshing = true;
    sourceError = null;
    try {
      source = await refreshLegioSource();
      await search(query);
    } catch (reason) {
      sourceError = messageFor(reason);
    } finally {
      sourceRefreshing = false;
    }
  }

  function sourceEntry(steamAppId: number): SourceEntry | undefined {
    return source?.manifest?.verified.find((entry) => entry.steamAppId === steamAppId)
      ?? source?.manifest?.unverified.find((entry) => entry.steamAppId === steamAppId);
  }

  $effect(() => {
    void search(query);
  });

  $effect(() => {
    void loadSource();
  });
</script>

<section class="mt-8" aria-labelledby="catalog-heading">
  <h2 id="catalog-heading" class="text-xl font-semibold">Store availability</h2>
  <p class="mt-2 text-sm text-slate-400">Typing searches previously fetched games in the local cache. Fetch sends the current search to Hydra. Legio download availability comes only from the separate Legio source.</p>
  <label class="mt-4 block text-sm" for="catalog-query">Search cached game names</label>
  <input id="catalog-query" class="mt-1 block w-full rounded border border-slate-600 bg-slate-950 px-3 py-2 text-slate-100" type="search" autocomplete="off" bind:value={query} />
  <div class="mt-4 flex flex-wrap gap-3">
    <button class="rounded bg-amber-400 px-3 py-2 text-sm font-semibold text-slate-950 disabled:opacity-50" type="button" disabled={refreshing} onclick={refresh}>
      {refreshing ? "Fetching..." : "Fetch matching games from Hydra"}
    </button>
    <button class="text-sm underline disabled:opacity-50" type="button" disabled={searching} onclick={() => search(query)}>Retry local search</button>
  </div>
  <div class="mt-5 rounded border border-slate-700 p-4 text-sm">
    <h3 class="font-semibold">Legio download source</h3>
    <p class="mt-1 text-slate-400">{source?.cachedAt ? `Last valid source: ${new Date(source.cachedAt * 1000).toLocaleString()}.` : "No validated source is cached yet."} {source?.stale ? "Source data is stale." : source?.cachedAt ? "Source data is fresh." : "Download availability is unknown."}</p>
    <button class="mt-3 rounded bg-amber-400 px-3 py-2 font-semibold text-slate-950 disabled:opacity-50" type="button" disabled={sourceRefreshing} onclick={refreshSource}>
      {sourceRefreshing ? "Refreshing source..." : "Refresh Legio source"}
    </button>
    {#if source?.warning}
      <p class="mt-3 break-words rounded border border-amber-700 bg-amber-950/30 p-3" role="alert">Source refresh failed: {source.warning}. Showing the last valid source as stale.</p>
    {/if}
    {#if sourceError}
      <p class="mt-3 break-words rounded border border-red-900 bg-red-950/30 p-3" role="alert">Legio source is unavailable: {sourceError}. Check connectivity and retry.</p>
    {/if}
  </div>
  {#if refreshing}
    <p class="mt-3 text-sm" role="status">Fetching matches from Hydra. Local search remains available.</p>
  {/if}
  {#if refreshError}
    <p class="mt-3 break-words rounded border border-red-900 bg-red-950/30 p-4 text-sm" role="alert">Catalog fetch failed: {refreshError}. Existing cached data remains available for local search. Run the native app, check connectivity, and retry the fetch.</p>
  {/if}
  {#if searching}
    <p class="mt-3 text-sm" role="status">Searching the local catalog...</p>
  {:else if searchError}
    <p class="mt-3 break-words rounded border border-red-900 bg-red-950/30 p-4 text-sm" role="alert">Local catalog search failed: {searchError}. Run the native app and retry local search.</p>
  {:else if result}
    {#if result.cachedAt === null}
      <p class="mt-3 text-sm" role="status">No catalog cache yet. Enter a game name and fetch matches from Hydra to populate it.</p>
    {:else}
      <p class="mt-3 text-sm" role="status">Showing {result.games.length} of {result.total} matching games.</p>
      <p class="mt-2 text-xs text-slate-400">Cache timestamp: {new Date(result.cachedAt * 1000).toLocaleString()}. {result.stale ? "Cache is stale. Fetch matches to update it." : "Using the local cache."}</p>
      {#if result.games.length === 0}
        <p class="mt-3 rounded border border-dashed border-slate-600 p-4 text-sm">No cached games match this search.</p>
      {:else}
        <ul class="mt-3 space-y-2 text-sm">
          {#each result.games as game (game.steamAppId)}
            {@const entry = sourceEntry(game.steamAppId)}
            <li class="rounded border border-slate-800 p-3">
              <p class="break-words">{game.name}</p>
              <p class="mt-1 text-xs text-slate-400">Steam App ID {game.steamAppId}</p>
              {#if game.availability === "unknown"}
                <p class="mt-2 text-amber-300">Download availability unknown. Refresh the Legio source to check.</p>
              {:else if game.availability === "unavailable"}
                <p class="mt-2 text-slate-300">Download unavailable</p>
              {:else}
                <p class="mt-2 text-emerald-300">Download available</p>
                {#if game.availability === "verified"}
                  <p class="mt-1 text-emerald-300">Verified by Legio Staff</p>
                {:else}
                  <p class="mt-1 text-amber-300">Unverified download</p>
                  <details class="mt-2 rounded border border-amber-700 bg-amber-950/30 p-3">
                    <summary class="cursor-pointer font-semibold">Review unverified release</summary>
                    <p class="mt-2">This download has not been verified by Legio staff. It may contain unwanted or harmful files. Continue only if you trust the source.</p>
                    <p class="mt-2 text-slate-300">Review the release details before choosing to install. Installation is not available yet.</p>
                    {#if entry}
                      <p class="mt-2 break-all text-xs">Archive URL: {entry.download.url}</p>
                      <p class="mt-1 break-all text-xs">Expected SHA-256: {entry.download.sha256}</p>
                    {/if}
                  </details>
                {/if}
                {#if entry}
                  <p class="mt-2 break-words text-xs text-slate-400">Release: {entry.name}, version {entry.release.version}. Published {new Date(entry.release.publishedAt).toLocaleString()}.</p>
                {/if}
              {/if}
              {#if game.availability !== "unknown" && (result.sourceStale || source?.stale)}
                <p class="mt-2 text-amber-300">Availability is based on a stale Legio source. Refresh before relying on this release.</p>
              {/if}
            </li>
          {/each}
        </ul>
      {/if}
    {/if}
  {/if}
</section>
