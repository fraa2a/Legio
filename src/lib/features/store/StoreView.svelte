<script lang="ts">
  import { t, language } from "../../i18n";
  import Badge from "../../components/ui/Badge.svelte";
  import Button from "../../components/ui/Button.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import StateBlock from "../../components/ui/StateBlock.svelte";
  import SelectField from "../../components/ui/SelectField.svelte";
  import type { CatalogSort, CatalogAvailability } from "../../services/catalog";
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

  const manifest = $derived($source.data.manifest);
  const trimmed = $derived($storeQuery.trim());
  let sort = $state<CatalogSort>("relevance");
  let availability = $state<CatalogAvailability>("all");
  const options = $derived({ sort, availability });
  let visibleCount = $state(20);

  // Each title appears once; its releases are selected on the detail page.
  const entries: StoreEntry[] = $derived($catalog.results
      .map((result) => ({
        steamAppId: result.steamAppId,
        name: result.name,
        status: sourceStatusFor(manifest, result.steamAppId),
      })));

  const emptyMessage = $derived.by(() => {
    if (availability !== "all") return t("Nessun gioco corrisponde ai filtri selezionati.", $language);
    if (trimmed.length === 0) {
      return t("Nessun gioco nella cache locale. Cerca un titolo o aggiungi una sorgente nelle impostazioni.", $language);
    }
    return t("Nessun risultato per questa ricerca.", $language);
  });

  const listStatus = $derived.by((): LoadStatus => {
    if ($catalog.status !== "ready" && entries.length === 0) return $catalog.status;
    return entries.length > 0 ? "ready" : "empty";
  });

  $effect(() => { void $storeQuery; void options; void manifest; visibleCount = 20; });

  $effect(() => {
    const value = $storeQuery;
    const selection = options;
    void manifest;
    const timer = setTimeout(() => void runCatalogSearch(value, selection), 300);
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

    {#if $catalog.error && $catalog.status !== "error"}
      <ErrorBanner message={$catalog.error}
        onRetry={() => void runCatalogSearch($storeQuery, options, true)}
        retryLabel={t("Riprova", $language)} />
    {/if}

    <div class="flex flex-wrap gap-3">
      <SelectField id="store-sort" label={t("Ordina per", $language)} value={sort}
        class="min-w-48 flex-1 sm:max-w-64"
        options={[
          { value: "relevance", label: t("Più affine", $language) },
          { value: "name_asc", label: t("Alfabetico crescente (A-Z)", $language) },
          { value: "name_desc", label: t("Alfabetico decrescente (Z-A)", $language) },
        ]}
        onChange={(value) => { sort = value as CatalogSort; }} />
      <SelectField id="store-availability" label={t("Disponibilità", $language)} value={availability}
        class="min-w-48 flex-1 sm:max-w-64"
        options={[
          { value: "all", label: t("Tutti", $language) },
          { value: "available", label: t("Solo disponibili", $language) },
          { value: "verified", label: t("Solo verified", $language) },
        ]}
        onChange={(value) => { availability = value as CatalogAvailability; }} />
    </div>

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
      onRetry={() => void runCatalogSearch($storeQuery, options, true)}
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
