<script lang="ts">
  import { refreshCatalog, searchCatalog, type CatalogSearch } from "../services/catalog";

  let query = $state("");
  let result = $state<CatalogSearch | null>(null);
  let searching = $state(false);
  let refreshing = $state(false);
  let searchError = $state<string | null>(null);
  let refreshError = $state<string | null>(null);
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

  $effect(() => {
    void search(query);
  });
</script>

<section class="mt-8" aria-labelledby="catalog-heading">
  <h2 id="catalog-heading" class="text-xl font-semibold">Catalog search</h2>
  <p class="mt-2 text-sm text-slate-400">Typing searches only previously fetched games in the local cache, not the full Steam catalog. Fetch explicitly sends the current search to Hydra and adds up to 50 results to the cache. Results are catalog metadata, not installed games or available downloads.</p>
  <label class="mt-4 block text-sm" for="catalog-query">Search cached game names</label>
  <input id="catalog-query" class="mt-1 block w-full rounded border border-slate-600 bg-slate-950 px-3 py-2 text-slate-100" type="search" autocomplete="off" bind:value={query} />
  <div class="mt-4 flex flex-wrap gap-3">
    <button class="rounded bg-amber-400 px-3 py-2 text-sm font-semibold text-slate-950 disabled:opacity-50" type="button" disabled={refreshing} onclick={refresh}>
      {refreshing ? "Fetching..." : "Fetch matching games from Hydra"}
    </button>
    <button class="text-sm underline disabled:opacity-50" type="button" disabled={searching} onclick={() => search(query)}>Retry local search</button>
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
            <li class="rounded border border-slate-800 p-3">
              <p class="break-words">{game.name}</p>
              <p class="mt-1 text-xs text-slate-400">Steam App ID {game.steamAppId}</p>
            </li>
          {/each}
        </ul>
      {/if}
    {/if}
  {/if}
</section>
