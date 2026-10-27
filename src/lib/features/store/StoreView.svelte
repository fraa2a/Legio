<script lang="ts">
  import { tick } from "svelte";
  import { t, language } from "../../i18n";
  import Badge from "../../components/ui/Badge.svelte";
  import Button from "../../components/ui/Button.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import StateBlock from "../../components/ui/StateBlock.svelte";
  import SelectField from "../../components/ui/SelectField.svelte";
  import type { CatalogSort, CatalogAvailability } from "../../services/catalog";
  import type { Settings } from "../../services/local-state";
  import { catalog, loadMoreCatalog, runCatalogSearch } from "../../stores/catalog";
  import { settings, updateSettings } from "../../stores/settings";
  import { toMessage } from "../../utils/errors";
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
  const options = $derived({ sort: $settings.data.storeSort, availability: $settings.data.storeAvailability });
  let visibleCount = $state(20);
  let savingFilter = $state(false);
  let filterError = $state<string | null>(null);
  let listElement: HTMLDivElement | undefined = $state();
  let scrollPage: HTMLElement | null = null;
  let listScrollTop = 0;
  let restoreScroll = false;

  function openEntry(entry: StoreEntry): void {
    scrollPage = listElement?.closest<HTMLElement>("[data-page]") ?? null;
    listScrollTop = scrollPage?.scrollTop ?? 0;
    restoreScroll = true;
    openStoreGame(entry);
    void tick().then(() => {
      if ($selectedStoreGame?.steamAppId === entry.steamAppId && scrollPage !== null) scrollPage.scrollTop = 0;
    });
  }

  $effect(() => {
    if ($selectedStoreGame !== null || !restoreScroll) return;
    restoreScroll = false;
    void tick().then(() => {
      if ($selectedStoreGame === null && scrollPage?.isConnected) scrollPage.scrollTop = listScrollTop;
    });
  });

  async function saveFilter(changes: Partial<Settings>): Promise<void> {
    savingFilter = true;
    filterError = null;
    try {
      await updateSettings(changes);
    } catch (error) {
      filterError = toMessage(error);
    } finally {
      savingFilter = false;
    }
  }

  // Each title appears once; its releases are selected on the detail page.
  const entries: StoreEntry[] = $derived($catalog.results
      .map((result) => ({
        steamAppId: result.steamAppId,
        name: result.name,
        status: sourceStatusFor(manifest, result.steamAppId),
      })));

  const emptyMessage = $derived.by(() => {
    if (options.availability !== "all") return t("Nessun gioco corrisponde ai filtri selezionati.", $language);
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
  <div bind:this={listElement} class="flex min-h-full flex-col gap-4">
    {#if $sourceRefreshError}
      <ErrorBanner
        message={$sourceRefreshError}
        onRetry={() => void refreshSource()}
        retryLabel={t("Riprova", $language)}
      />
    {/if}
    {#if filterError !== null}<ErrorBanner message={filterError} />{/if}

    {#if $source.data.warning}<p class="text-xs text-legio-warning">{$source.data.warning}</p>{/if}

    {#if $catalog.error && $catalog.status !== "error"}
      <ErrorBanner message={$catalog.error}
        onRetry={() => void runCatalogSearch($storeQuery, options, true)}
        retryLabel={t("Riprova", $language)} />
    {/if}

    <div class="flex flex-wrap gap-3">
      <SelectField id="store-sort" label={t("Ordina per", $language)} value={options.sort}
        class="min-w-48 flex-1 sm:max-w-64"
        options={[
          { value: "relevance", label: t("Più affine", $language) },
          { value: "name_asc", label: t("Alfabetico crescente (A-Z)", $language) },
          { value: "name_desc", label: t("Alfabetico decrescente (Z-A)", $language) },
        ]}
        disabled={savingFilter}
        onChange={(value) => void saveFilter({ storeSort: value as CatalogSort })} />
      <SelectField id="store-availability" label={t("Disponibilità", $language)} value={options.availability}
        class="min-w-48 flex-1 sm:max-w-64"
        options={[
          { value: "all", label: t("Tutti", $language) },
          { value: "available", label: t("Solo disponibili", $language) },
          { value: "verified", label: t("Solo verified", $language) },
        ]}
        disabled={savingFilter}
        onChange={(value) => void saveFilter({ storeAvailability: value as CatalogAvailability })} />
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
            onOpen={() => openEntry(entry)}
          />
        {/each}
      </ul>
    {/if}
    {#if entries.length > visibleCount || $catalog.total > $catalog.results.length || ($catalog.nextOffset !== null && entries.length >= visibleCount)}
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
