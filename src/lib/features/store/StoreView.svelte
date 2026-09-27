<script lang="ts">
  import Badge from "../../components/ui/Badge.svelte";
  import Button from "../../components/ui/Button.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import StateBlock from "../../components/ui/StateBlock.svelte";
  import { catalog, runCatalogSearch } from "../../stores/catalog";
  import type { LoadStatus } from "../../stores/resource";
  import { refreshSource, sourceRefreshError, source } from "../../stores/source";
  import StoreGameDetail from "./StoreGameDetail.svelte";
  import StoreGameCard from "./StoreGameCard.svelte";
  import { sourceStatusFor, type SourceStatus } from "./source-status";

  interface StoreEntry {
    steamAppId: number;
    name: string;
    status: SourceStatus;
  }

  let query = $state("");
  let selected = $state<StoreEntry | null>(null);

  const manifest = $derived($source.data.manifest);
  const trimmed = $derived(query.trim());
  const verifiedCount = $derived(manifest?.verified.length ?? 0);
  const unverifiedCount = $derived(manifest?.unverified.length ?? 0);

  // Without a query the store lists every release the Legio source publishes,
  // otherwise it lists the catalog hits that the source can actually deliver.
  const entries = $derived.by((): StoreEntry[] => {
    if (trimmed.length === 0) {
      if (manifest === null) return [];
      return [...manifest.verified, ...manifest.unverified].map((entry) => ({
        steamAppId: entry.steamAppId,
        name: entry.name,
        status: sourceStatusFor(manifest, entry.steamAppId),
      }));
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
    const value = query;
    const timer = setTimeout(() => void runCatalogSearch(value), 300);
    return () => clearTimeout(timer);
  });
</script>

{#if selected !== null}
  <StoreGameDetail
    steamAppId={selected.steamAppId}
    name={selected.name}
    onBack={() => (selected = null)}
  />
{:else}
  <header class="mb-5 flex flex-wrap items-end justify-between gap-3">
    <div>
      <p class="text-xs font-medium uppercase tracking-wider text-zinc-500">Catalogo Legio</p>
      <h2 class="mt-1 text-2xl font-semibold text-zinc-50 light:text-zinc-900">Store</h2>
      <p class="mt-1 text-sm text-zinc-400 light:text-zinc-600">Sfoglia i titoli disponibili e controlla i dettagli prima di scaricare.</p>
    </div>
  </header>

  <section class="mb-5 flex flex-wrap items-center gap-3 rounded-xl border border-white/10 bg-white/5 px-4 py-3 text-sm text-zinc-400 light:border-zinc-900/10 light:bg-white light:text-zinc-600">
    {#if manifest === null}
      <span>Nessun manifest della sorgente Legio disponibile.</span>
      <Button label="Scarica sorgente" onClick={() => void refreshSource()} />
    {:else}
      <span>Sorgente Legio: {verifiedCount} verificate, {unverifiedCount} non verificate</span>
      {#if $source.data.stale}
        <Badge tone="warning" title="Cache scaduta" />
      {/if}
      <Button label="Aggiorna sorgente" variant="secondary" onClick={() => void refreshSource()} />
    {/if}
    {#if $source.data.warning}
      <span class="text-amber-300 light:text-amber-800">{$source.data.warning}</span>
    {/if}
  </section>

  {#if $sourceRefreshError}
    <div class="mb-4">
      <ErrorBanner
        message={$sourceRefreshError}
        onRetry={() => void refreshSource()}
        retryLabel="Riprova"
      />
    </div>
  {/if}

  <div class="mb-6 flex flex-col gap-2 rounded-xl bg-white/5 p-4 light:bg-white">
    <label for="store-query" class="text-sm text-zinc-400 light:text-zinc-600">Cerca un titolo</label>
    <input
      id="store-query"
      type="search"
      bind:value={query}
      placeholder="Titolo del gioco"
      class="h-11 w-full rounded-lg border border-white/10 bg-zinc-900/60 px-3 text-sm text-zinc-100 outline-none placeholder:text-zinc-500 focus:border-white/40 light:border-zinc-900/10 light:bg-zinc-50 light:text-zinc-900 light:focus:border-zinc-900/40"
    />
  </div>

  {#if trimmed.length === 0 && entries.length > 0}
    <p class="mb-4 text-xs text-zinc-500">
      {entries.length} titoli disponibili nella sorgente Legio
    </p>
  {:else if $catalog.results.length > 0}
    <div class="mb-4 flex flex-wrap items-center gap-3 text-xs text-zinc-500">
      <span>{$catalog.total} risultati in cache</span>
      {#if $catalog.refreshing}
        <span role="status">Aggiornamento in corso...</span>
      {/if}
      {#if $catalog.stale}
        <Badge tone="warning" title="Dati in cache" />
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
    onRetry={() => void runCatalogSearch($catalog.query)}
  />

  {#if entries.length > 0}
    <ul class="grid grid-cols-1 gap-4 sm:grid-cols-2 xl:grid-cols-3">
      {#each entries as entry (entry.steamAppId)}
        <StoreGameCard
          steamAppId={entry.steamAppId}
          name={entry.name}
          status={entry.status}
          onOpen={() => (selected = entry)}
        />
      {/each}
    </ul>
  {/if}
{/if}
