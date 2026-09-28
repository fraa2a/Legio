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
  <header class="mb-5 flex flex-wrap items-end justify-between gap-3">
    <div>
      <p class="text-xs font-medium uppercase tracking-wider text-zinc-500">Catalogo Legio</p>
      <h2 class="mt-1 text-2xl font-semibold text-zinc-50 light:text-zinc-900">Store</h2>
      <p class="mt-1 text-sm text-zinc-400 light:text-zinc-600">Sfoglia i titoli disponibili e controlla i dettagli prima di scaricare.</p>
    </div>
  </header>

  {#if $sourceRefreshError}
    <div class="mb-4">
      <ErrorBanner
        message={$sourceRefreshError}
        onRetry={() => void refreshSource()}
        retryLabel="Riprova"
      />
    </div>
  {/if}

  {#if $source.data.warning}<p class="mb-4 text-xs text-amber-300 light:text-amber-800">{$source.data.warning}</p>{/if}

  {#if trimmed.length > 0 && $catalog.results.length > 0}
    <div class="mb-4 flex flex-wrap items-center gap-3 text-xs text-zinc-500">
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
{/if}
