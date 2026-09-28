<script lang="ts">
  import Badge from "../../components/ui/Badge.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import StateBlock from "../../components/ui/StateBlock.svelte";
  import { catalog, runCatalogSearch } from "../../stores/catalog";
  import { storeQuery } from "../../stores/library-ui";
  import { openStoreGame, selectedStoreGame } from "../../stores/navigation";
  import type { LoadStatus } from "../../stores/resource";
  import { refreshSource, sourceRefreshError, source } from "../../stores/source";
  import GameDetailView from "../library/GameDetailView.svelte";
  import StoreGameCard from "./StoreGameCard.svelte";
  import { sourceStatusFor, type SourceStatus } from "./source-status";

  interface StoreEntry {
    steamAppId: number;
    name: string;
    status: SourceStatus;
  }

  const collator = new Intl.Collator("it", { sensitivity: "base" });

  const manifest = $derived($source.data.manifest);
  const trimmed = $derived($storeQuery.trim());

  // Without a query the store lists every release the Legio source publishes,
  // otherwise it lists the catalog hits that the source can actually deliver.
  const entries = $derived.by((): StoreEntry[] => {
    if (trimmed.length === 0) {
      if (manifest === null) return [];
      return [...manifest.verified, ...manifest.unverified]
        .map((entry) => ({
          steamAppId: entry.steamAppId,
          name: entry.name,
          status: sourceStatusFor(manifest, entry.steamAppId),
        }))
        .sort((left, right) => collator.compare(left.name, right.name));
    }
    return $catalog.results
      .map((result) => ({
        steamAppId: result.steamAppId,
        name: result.name,
        status: sourceStatusFor(manifest, result.steamAppId),
      }))
      .filter(
        (entry) =>
          entry.status.availability === "verified" || entry.status.availability === "unverified",
      );
  });

  const emptyMessage = $derived.by(() => {
    if (trimmed.length === 0) {
      return "La sorgente Legio non pubblica ancora alcun titolo. Aggiorna la sorgente e riprova.";
    }
    if ($catalog.results.length > 0) {
      return "Nessuno dei risultati trovati è disponibile nella sorgente Legio.";
    }
    return "Nessun risultato per questa ricerca.";
  });

  // The catalog answers "ready" as soon as a search returns, but the store only
  // lists what the source can deliver, so emptiness is decided on the listing.
  const listStatus = $derived.by((): LoadStatus => {
    if ($catalog.status !== "ready" && trimmed.length > 0) return $catalog.status;
    return entries.length > 0 ? "ready" : "empty";
  });

  $effect(() => {
    const value = $storeQuery;
    const timer = setTimeout(() => void runCatalogSearch(value), 300);
    return () => clearTimeout(timer);
  });
</script>

{#if $selectedStoreGame !== null}
  <GameDetailView storeGame={$selectedStoreGame} />
{:else}
  <div class="flex min-h-full flex-col gap-4">
    {#if $sourceRefreshError}
      <ErrorBanner
        message={$sourceRefreshError}
        onRetry={() => void refreshSource()}
        retryLabel="Riprova"
      />
    {/if}

    {#if $source.data.warning}<p class="text-xs text-amber-300 light:text-amber-800">{$source.data.warning}</p>{/if}

    {#if trimmed.length > 0 && $catalog.results.length > 0}
      <div class="flex flex-wrap items-center gap-3 text-xs text-zinc-500">
        {#if $catalog.refreshing}
          <span role="status">Aggiornamento in corso...</span>
        {/if}
        {#if $catalog.sourceStale}
          <Badge tone="warning" title="Sorgente non aggiornata" />
        {/if}
      </div>
    {/if}

    <StateBlock
      status={listStatus}
      hasData={entries.length > 0}
      loadingMessage="Ricerca in corso..."
      {emptyMessage}
      error={$catalog.error}
      onRetry={() => void runCatalogSearch($storeQuery)}
    />

    {#if entries.length > 0}
      <ul class="flex flex-col gap-2">
        {#each entries as entry (entry.steamAppId)}
          <StoreGameCard
            steamAppId={entry.steamAppId}
            name={entry.name}
            status={entry.status}
            onOpen={() => openStoreGame(entry)}
          />
        {/each}
      </ul>
    {/if}
  </div>
{/if}
