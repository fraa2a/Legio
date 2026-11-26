<script lang="ts">
  import { tick } from "svelte";
  import { t, language } from "../../i18n";
  import Badge from "../../components/ui/Badge.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import Pagination from "../../components/ui/Pagination.svelte";
  import StateBlock from "../../components/ui/StateBlock.svelte";
  import Callout from "../../components/ui/Callout.svelte";
  import type { Settings } from "../../services/local-state";
  import { catalog, loadMoreCatalog, runCatalogSearch } from "../../stores/catalog";
  import { settings, updateSettings } from "../../stores/settings";
  import { toMessage } from "../../utils/errors";
  import { pageCountFor, pageItems, pageSize } from "../../utils/pagination";
  import { storeQuery } from "../../stores/library-ui";
  import { activeSection, openStoreGame, selectedStoreGame } from "../../stores/navigation";
  import { showTitleBarContent } from "../../stores/title-bar";
  import type { LoadStatus } from "../../stores/resource";
  import { refreshSource, sourceRefreshError, source } from "../../stores/source";
  import GameDetailView from "../library/GameDetailView.svelte";
  import StoreFilterMenu from "./StoreFilterMenu.svelte";
  import StoreGameCard from "./StoreGameCard.svelte";
  import { sourceReleasesFor, type SourceStatus } from "./source-status";

  interface StoreEntry {
    steamAppId: number;
    name: string;
    status: SourceStatus;
  }

  const manifest = $derived($source.data.manifest);
  const trimmed = $derived($storeQuery.trim());
  const options = $derived({ sort: $settings.data.storeSort, availability: $settings.data.storeAvailability });
  const onPage = $derived($activeSection === "store" && $selectedStoreGame === null);
  let page = $state(1);
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

  $effect(() => {
    if (!onPage) return;
    return showTitleBarContent({ start: filterControls });
  });

  // Each title appears once; its releases are selected on the detail page.
  const entries: StoreEntry[] = $derived($catalog.results
      .map((result) => ({
        steamAppId: result.steamAppId,
        name: result.name,
        status: { availability: result.availability, entry: sourceReleasesFor(manifest, result.steamAppId)[0]?.entry ?? null },
      })));

  const pageCount = $derived(pageCountFor(entries.length));
  const currentPage = $derived(Math.min(page, pageCount));
  // The catalog holds more games than are loaded; they are fetched when the last loaded page is reached.
  const hasMore = $derived($catalog.total > $catalog.results.length || ($catalog.nextOffset !== null && entries.length >= currentPage * pageSize));

  function goToPage(next: number): void {
    const forward = next > currentPage;
    page = next;
    if (forward && hasMore && entries.length <= next * pageSize) {
      void loadMoreCatalog($storeQuery, Math.max(next * pageSize, $catalog.results.length + pageSize));
    }
  }

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

  $effect(() => { void $storeQuery; void options; void manifest; page = 1; });

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
        message={t("Impossibile aggiornare le sorgenti.", $language)}
        detail={$sourceRefreshError}
        onRetry={() => void refreshSource()}
        retryLabel={t("Riprova", $language)}
      />
    {/if}
    {#if filterError !== null}<ErrorBanner message={filterError} />{/if}

    {#if $source.data.warning}<Callout tone="warning" message={$source.data.warning} />{/if}

    {#if $catalog.error && $catalog.status !== "error"}
      <ErrorBanner message={$catalog.error}
        onRetry={() => void runCatalogSearch($storeQuery, options, true)}
        retryLabel={t("Riprova", $language)} />
    {/if}

    {#if trimmed.length > 0 && $catalog.results.length > 0}
      <div class="flex flex-wrap items-center gap-3 text-xs text-legio-fg-subtle">
        {#if $catalog.refreshing}
          <span role="status">{t("Aggiornamento in corso...", $language)}</span>
        {/if}
        {#if $source.data.stale}
          <Badge tone="warning" title={t("Sorgente non aggiornata", $language)} />
        {/if}
      </div>
    {/if}

    <StateBlock
      status={listStatus}
      hasData={entries.length > 0}
      loadingMessage={t("Ricerca in corso...", $language)}
      skeletonRows={4}
      skeletonClass="h-28 sm:h-32"
      emptyIcon="search"
      {emptyMessage}
      error={$catalog.error}
      onRetry={() => void runCatalogSearch($storeQuery, options, true)}
    />

    {#if entries.length > 0}
      <ul class="flex flex-col gap-2">
        {#each pageItems(entries, currentPage) as entry (entry.steamAppId)}
          <StoreGameCard
            steamAppId={entry.steamAppId}
            name={entry.name}
            status={entry.status}
            onOpen={() => openEntry(entry)}
          />
        {/each}
      </ul>
    {/if}
    {#if pageCount > 1 || hasMore}
      <Pagination page={currentPage} {pageCount} {hasMore} disabled={$catalog.refreshing} onChange={goToPage} />
    {/if}
  </div>
{/if}

{#snippet filterControls()}
  <StoreFilterMenu sort={options.sort} availability={options.availability} disabled={savingFilter} onChange={(changes) => void saveFilter(changes)} />
{/snippet}
