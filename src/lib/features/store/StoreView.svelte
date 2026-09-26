<script lang="ts">
  import { formatBytes } from "../../utils/format";
  import Badge from "../../components/ui/Badge.svelte";
  import Button from "../../components/ui/Button.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import StateBlock from "../../components/ui/StateBlock.svelte";
  import { catalog, runCatalogSearch } from "../../stores/catalog";
  import type { LoadStatus } from "../../stores/resource";
  import { refreshSource, sourceRefreshError, source } from "../../stores/source";
  import StoreGameDetail from "./StoreGameDetail.svelte";
  import { availabilityMeta, sourceStatusFor, type SourceStatus } from "./source-status";

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
  <section class="mb-6 flex flex-wrap items-center gap-3 text-sm text-zinc-400 light:text-zinc-600">
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

  <div class="mb-6 flex flex-col gap-2">
    <label for="store-query" class="text-sm text-zinc-400 light:text-zinc-600">Cerca un titolo</label>
    <input
      id="store-query"
      type="search"
      bind:value={query}
      placeholder="Titolo del gioco"
      class="w-full max-w-sm rounded-lg bg-white/5 px-3 py-2 text-sm text-zinc-100 placeholder:text-zinc-500 light:bg-white light:text-zinc-900"
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
    <ul class="flex flex-col gap-3">
      {#each entries as entry (entry.steamAppId)}
        {@const meta = availabilityMeta[entry.status.availability]}
        <li class="rounded-xl bg-white/5 light:bg-zinc-100">
          <button
            type="button"
            class="flex w-full flex-wrap items-center gap-3 rounded-xl p-4 text-left"
            aria-label="Dettagli di {entry.name} nello store"
            onclick={() => (selected = entry)}
          >
            <span class="min-w-0 flex-1 truncate font-medium text-zinc-50 light:text-zinc-900">
              {entry.name}
            </span>
            {#if entry.status.entry !== null}
              <span class="text-xs text-zinc-400 light:text-zinc-600">
                v{entry.status.entry.release.version} · {formatBytes(entry.status.entry.download.sizeBytes)}
              </span>
            {/if}
            <Badge tone={meta.tone} title={meta.label} />
          </button>
        </li>
      {/each}
    </ul>
  {/if}
{/if}
