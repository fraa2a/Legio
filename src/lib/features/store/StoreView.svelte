<script lang="ts">
  import { t, language } from "../../i18n";
  import { SvelteMap } from "svelte/reactivity";
  import Badge from "../../components/ui/Badge.svelte";
  import Button from "../../components/ui/Button.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import StateBlock from "../../components/ui/StateBlock.svelte";
  import { catalog, loadMoreCatalog, runCatalogSearch } from "../../stores/catalog";
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

  const collator = new Intl.Collator(undefined, { sensitivity: "base" });

  const manifest = $derived($source.data.manifest);
  const trimmed = $derived($storeQuery.trim());
  let visibleCount = $state(20);

  // Each title appears once; its releases are selected on the detail page.
  const entries = $derived.by((): StoreEntry[] => {
    if (trimmed.length === 0) {
      const sourceEntries = manifest === null ? [] : [...manifest.verified, ...manifest.unverified];
      const names = new SvelteMap($catalog.results.map((entry) => [entry.steamAppId, entry.name]));
      for (const entry of sourceEntries) names.set(entry.steamAppId, entry.name);
      return [...names].map(([steamAppId, name]) => ({ steamAppId, name, status: sourceStatusFor(manifest, steamAppId) }))
        .sort((left, right) => collator.compare(left.name, right.name));
    }
    return $catalog.results
      .map((result) => ({
        steamAppId: result.steamAppId,
        name: result.name,
        status: sourceStatusFor(manifest, result.steamAppId),
      }));
  });

  const emptyMessage = $derived.by(() => {
    if (trimmed.length === 0) {
      return t("Nessun gioco nella cache locale. Cerca un titolo o aggiungi una sorgente nelle impostazioni.", $language);
    }
    return t("Nessun risultato per questa ricerca.", $language);
  });

  const listStatus = $derived.by((): LoadStatus => {
    if ($catalog.status !== "ready" && trimmed.length > 0) return $catalog.status;
    return entries.length > 0 ? "ready" : "empty";
  });

  $effect(() => { void $storeQuery; visibleCount = 20; });

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
        retryLabel={t("Riprova", $language)}
      />
    {/if}

    {#if $source.data.warning}<p class="text-xs text-amber-300 light:text-amber-800">{$source.data.warning}</p>{/if}

    {#if trimmed.length > 0 && $catalog.results.length > 0}
      <div class="flex flex-wrap items-center gap-3 text-xs text-zinc-500">
        {#if $catalog.refreshing}
          <span role="status">{t("Aggiornamento in corso...", $language)}</span>
        {/if}
        {#if $catalog.sourceStale}
          <Badge tone="warning" title={t("Sorgente non aggiornata", $language)} />
        {/if}
      </div>
    {/if}

    <StateBlock
      status={listStatus}
      hasData={entries.length > 0}
      loadingMessage={t("Ricerca in corso...", $language)}
      {emptyMessage}
      error={$catalog.error}
      onRetry={() => void runCatalogSearch($storeQuery)}
    />

    {#if entries.length > 0}
      <ul class="flex flex-col gap-2">
        {#each entries.slice(0, visibleCount) as entry (entry.steamAppId)}
          <StoreGameCard
            steamAppId={entry.steamAppId}
            name={entry.name}
            status={entry.status}
            onOpen={() => openStoreGame(entry)}
          />
        {/each}
      </ul>
    {/if}
    {#if entries.length > visibleCount || $catalog.total > $catalog.results.length || $catalog.nextOffset !== null}
      <div class="flex justify-center pt-2">
        <Button label={t("Carica altri 20", $language)} variant="secondary" disabled={$catalog.refreshing}
          onClick={() => {
            visibleCount += 20;
            if (entries.length <= visibleCount) void loadMoreCatalog($storeQuery, Math.max(visibleCount, $catalog.results.length + 20));
          }} />
      </div>
    {/if}
  </div>
{/if}
