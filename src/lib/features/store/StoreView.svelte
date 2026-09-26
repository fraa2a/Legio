<script lang="ts">
  import type { CatalogGame } from "../../services/catalog";
  import Badge from "../../components/ui/Badge.svelte";
  import Button from "../../components/ui/Button.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import StateBlock from "../../components/ui/StateBlock.svelte";
  import { catalog, runCatalogSearch } from "../../stores/catalog";
  import { refreshSource, sourceRefreshError, source } from "../../stores/source";

  const availabilityMeta: Record<
    CatalogGame["availability"],
    { label: string; tone: "neutral" | "success" | "warning" | "danger" }
  > = {
    verified: { label: "Verificato", tone: "success" },
    unverified: { label: "Non verificato", tone: "warning" },
    unavailable: { label: "Non disponibile", tone: "danger" },
    unknown: { label: "Sconosciuto", tone: "neutral" },
  };

  let query = $state("");

  const manifest = $derived($source.data.manifest);
  const verifiedCount = $derived(manifest?.verified.length ?? 0);
  const unverifiedCount = $derived(manifest?.unverified.length ?? 0);

  $effect(() => {
    const value = query;
    const timer = setTimeout(() => void runCatalogSearch(value), 300);
    return () => clearTimeout(timer);
  });
</script>

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
    <ErrorBanner message={$sourceRefreshError} onRetry={() => void refreshSource()} retryLabel="Riprova" />
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

<StateBlock
  status={$catalog.status}
  hasData={$catalog.results.length > 0}
  loadingMessage="Ricerca in corso..."
  emptyMessage="Nessun risultato per questa ricerca."
  error={$catalog.error}
  onRetry={() => void runCatalogSearch($catalog.query)}
/>

{#if $catalog.results.length > 0}
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
  <ul class="flex flex-col gap-3">
    {#each $catalog.results as result (result.steamAppId)}
      <li class="flex flex-wrap items-center gap-3 rounded-xl bg-white/5 p-4 light:bg-zinc-100">
        <p class="min-w-0 flex-1 truncate font-medium text-zinc-50 light:text-zinc-900">{result.name}</p>
        <Badge tone={availabilityMeta[result.availability].tone} title={availabilityMeta[result.availability].label} />
      </li>
    {/each}
  </ul>
{/if}
