<script lang="ts">
  import { t, language } from "../../i18n";
  import Badge from "../../components/ui/Badge.svelte";
  import Button from "../../components/ui/Button.svelte";
  import Callout from "../../components/ui/Callout.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import Pagination from "../../components/ui/Pagination.svelte";
  import Icon from "../../components/ui/Icon.svelte";
  import IconButton from "../../components/ui/IconButton.svelte";
  import RefreshIcon from "../../components/ui/RefreshIcon.svelte";
  import SelectField from "../../components/ui/SelectField.svelte";
  import StateBlock from "../../components/ui/StateBlock.svelte";
  import TextField from "../../components/ui/TextField.svelte";
  import { listModCategories, searchMods, type ModProfile, type PackageSort, type PackageSummary } from "../../services/modding";
  import { catalogFor, catalogs, installPackages, loadOnce, modUpdates, profileMods, refreshCatalog } from "../../stores/modding";
  import type { LoadStatus } from "../../stores/resource";
  import { showToast } from "../../stores/toast";
  import { toMessage } from "../../utils/errors";
  import { formatDate, regionalLocale } from "../../utils/format";
  import { pageCountFor, pageSize } from "../../utils/pagination";
  import {
    activeFilterCount,
    catalogPercent,
    classifyModdingError,
    defaultBrowserFilters,
    formatCount,
    indexByFullName,
    installedState,
    installNotice,
    modKey,
    packageQuery,
    packageTitle,
    sortOptions,
    type BrowserFilters,
    type ResultNotice,
  } from "./modding-model";
  import { openModTab, type ModTabs } from "./mod-panel-model";
  import ModFilterDialog from "./ModFilterDialog.svelte";
  import ModIcon from "./ModIcon.svelte";
  import ModPanelLayout from "./ModPanelLayout.svelte";

  let {
    gameId,
    ecosystemGameId,
    profile,
    disabled,
    tabs,
    onTabsChange,
  }: {
    gameId: string;
    ecosystemGameId: string;
    profile: ModProfile;
    disabled: boolean;
    tabs: ModTabs;
    onTabsChange: (tabs: ModTabs) => void;
  } = $props();

  let filters = $state<BrowserFilters>({ ...defaultBrowserFilters });
  let items = $state<PackageSummary[]>([]);
  let total = $state(0);
  let status = $state<LoadStatus>("idle");
  let searchError = $state<string | null>(null);
  let page = $state(1);
  let loadingPage = $state(false);
  let categories = $state<string[]>([]);
  let filtersOpen = $state(false);
  let actionError = $state<string | null>(null);
  let notice = $state<ResultNotice | null>(null);
  let request = 0;

  const catalog = $derived(catalogFor($catalogs, ecosystemGameId));
  const syncedAt = $derived(catalog.status?.syncedAt ?? null);
  const mods = $derived(profileMods(profile.id));
  const updates = $derived(modUpdates(profile.id));
  const installed = $derived(indexByFullName($mods.data));
  const updateIndex = $derived(indexByFullName($updates.data));
  const filterCount = $derived(activeFilterCount(filters));
  const locale = $derived(regionalLocale($language));
  const pageCount = $derived(pageCountFor(total));

  // A new search replaces the whole list; moving between pages keeps the current one visible until the next arrives.
  async function search(target: number, fresh = false): Promise<void> {
    const current = ++request;
    const query = packageQuery(filters, (target - 1) * pageSize);
    if (fresh) status = "loading";
    else loadingPage = true;
    searchError = null;
    try {
      const result = await searchMods(ecosystemGameId, query);
      if (current !== request) return;
      items = result.items;
      total = result.total;
      page = target;
      status = items.length === 0 ? "empty" : "ready";
    } catch (error) {
      if (current !== request) return;
      searchError = toMessage(error);
      status = fresh ? "error" : status;
    } finally {
      if (current === request) loadingPage = false;
    }
  }

  // The active profile can change while this tab is open, and its mods may not be loaded yet.
  $effect(() => {
    loadOnce(mods);
    loadOnce(updates);
  });

  $effect(() => {
    void syncedAt;
    void JSON.stringify(filters);
    const timer = setTimeout(() => void search(1, true), 300);
    return () => clearTimeout(timer);
  });

  $effect(() => {
    void syncedAt;
    const community = ecosystemGameId;
    let cancelled = false;
    listModCategories(community).then(
      (value) => { if (!cancelled) categories = value; },
      (error: unknown) => { if (!cancelled) searchError = toMessage(error); },
    );
    return () => { cancelled = true; };
  });

  async function install(fullName: string, version: string | null): Promise<void> {
    actionError = null;
    notice = null;
    try {
      const result = await installPackages(gameId, profile.id, [version === null ? { fullName } : { fullName, version }]);
      const next = installNotice(result, $language);
      if (next.tone === "success" && next.items.length === 0) showToast(next.message, "success");
      else notice = next;
    } catch (error) {
      const failure = classifyModdingError(error, $language);
      if (failure.kind === "cancelled") showToast(failure.message, "info");
      else actionError = failure.message;
    }
  }

  const refreshTitle = $derived(!catalog.refreshing
    ? t("Aggiorna il catalogo", $language)
    : catalog.progress === null || catalog.progress.chunksTotal === 0
      ? t("Aggiornamento del catalogo...", $language)
      : t("Aggiornamento del catalogo: {0}%", $language, [catalogPercent(catalog.progress.chunksDone, catalog.progress.chunksTotal)]));
  const emptyMessage = $derived(syncedAt === null
    ? t("Il catalogo di questo gioco non è ancora stato scaricato.", $language)
    : t("Nessuna mod corrisponde ai filtri.", $language));
</script>

<ModPanelLayout
  {ecosystemGameId}
  {tabs}
  {installed}
  updates={updateIndex}
  {disabled}
  onChange={onTabsChange}
  onInstall={(fullName, version) => void install(fullName, version)}
>
  <div class="flex flex-col gap-3 rounded-2xl legio-glass p-4">
    <div class="flex flex-wrap items-end gap-3">
      <div class="min-w-64 flex-[2]">
        <TextField id="mod-search" type="search" label={t("Cerca", $language)} placeholder={t("Nome, autore o descrizione", $language)} bind:value={filters.text} />
      </div>
      <SelectField id="mod-sort" label={t("Ordina per", $language)} class="min-w-48 flex-1 sm:max-w-64" value={filters.sort} options={sortOptions($language)} onChange={(value) => (filters.sort = value as PackageSort)} />
      <div class="flex h-10 items-center gap-2">
        <Button
          label={filterCount === 0 ? t("Filtri", $language) : t("Filtri ({0})", $language, [filterCount])}
          variant="secondary"
          pressed={filterCount > 0}
          onClick={() => (filtersOpen = true)}
        >
          <Icon name="funnel" size="size-4" />
        </Button>
        <IconButton
          size="lg"
          variant="soft"
          label={t("Aggiorna il catalogo", $language)}
          title={refreshTitle}
          onClick={() => void refreshCatalog(ecosystemGameId, true)}
        >
          <RefreshIcon spinning={catalog.refreshing} />
        </IconButton>
      </div>
    </div>
    {#if items.length > 0}
      <p class="text-xs text-zinc-400 light:text-zinc-600">{t("{0} risultati", $language, [total])}</p>
    {/if}
  </div>

  {#if catalog.error !== null}
    <ErrorBanner
      tone={catalog.status === null ? "error" : "warning"}
      message={catalog.status === null ? catalog.error : t("Catalogo non aggiornato: {0}", $language, [catalog.error])}
      onRetry={() => void refreshCatalog(ecosystemGameId, true)}
      retryLabel={t("Riprova", $language)}
    />
  {/if}
  {#if actionError !== null}<ErrorBanner message={actionError} />{/if}
  {#if notice !== null}
    <Callout tone={notice.tone} message={notice.message} items={notice.items} onDismiss={() => (notice = null)} />
  {/if}

  <StateBlock
    {status}
    hasData={items.length > 0}
    loadingMessage={t("Ricerca in corso...", $language)}
    skeletonRows={4}
    skeletonClass="h-20"
    emptyIcon="search"
    {emptyMessage}
    error={searchError}
    onRetry={() => void search(1, true)}
  />

  {#if items.length > 0}
    <ul class="flex flex-col gap-2">
      {#each items as item (item.fullName)}
        {@const installState = installedState(item.fullName, installed, updateIndex)}
        {@const opened = tabs.active === modKey(item.fullName)}
        <li class="flex flex-wrap items-center gap-3 rounded-xl legio-glass p-3 transition-colors hover:bg-white/5 light:hover:bg-zinc-900/5 {opened ? 'ring-1 ring-legio-accent ring-inset' : ''}">
          <button
            type="button"
            class="flex min-w-0 flex-1 basis-64 items-center gap-3 rounded-lg text-left focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-(--legio-focus)"
            aria-label={t("Dettagli di {0}", $language, [packageTitle(item.name)])}
            aria-current={opened ? "true" : undefined}
            onclick={() => onTabsChange(openModTab(tabs, item.fullName, item))}
          >
            <ModIcon source={item.icon} class="size-12 rounded-lg" />
            <span class="min-w-0 flex-1">
              <span class="flex flex-wrap items-center gap-2">
                <span class="min-w-0 truncate font-medium text-zinc-50 light:text-zinc-900">{packageTitle(item.name)}</span>
                <span class="text-xs text-zinc-500">{t("di {0}", $language, [item.owner])}</span>
                {#if item.isPinned}<Badge tone="info" title={t("In evidenza", $language)} />{/if}
                {#if item.isDeprecated}<Badge tone="warning" title={t("Deprecata", $language)} />{/if}
                {#if item.hasNsfwContent}<Badge tone="danger" title="NSFW" />{/if}
              </span>
              <span class="mt-0.5 line-clamp-2 text-xs text-zinc-400 light:text-zinc-600">{item.description}</span>
              <span class="mt-1 flex flex-wrap gap-x-4 text-xs tabular-nums text-zinc-500">
                <span class="inline-flex items-center gap-1"><Icon name="download" size="size-3.5" />{formatCount(item.totalDownloads, locale)}</span>
                <span class="inline-flex items-center gap-1"><Icon name="thumbs-up" size="size-3.5" />{formatCount(item.ratingScore, locale)}</span>
                <span>{t("Aggiornata il {0}", $language, [formatDate(item.dateUpdated, $language)])}</span>
              </span>
            </span>
          </button>
          <div class="flex shrink-0 items-center gap-2">
            {#if installState.kind === "installed"}
              <Badge tone="success" title={t("Installata {0}", $language, [installState.version])} />
            {:else if installState.kind === "outdated"}
              <Button label={t("Aggiorna a {0}", $language, [installState.latestVersion])} variant="secondary" size="sm" {disabled} onClick={() => void install(item.fullName, null)} />
            {:else}
              <Button label={t("Installa", $language)} size="sm" {disabled} onClick={() => void install(item.fullName, null)}>
                <Icon name="download" size="size-4" />
              </Button>
            {/if}
          </div>
        </li>
      {/each}
    </ul>
    {#if pageCount > 1}
      <Pagination {page} {pageCount} disabled={loadingPage} onChange={(next) => void search(next)} />
    {/if}
    {#if searchError !== null && status !== "error"}<ErrorBanner message={searchError} onRetry={() => void search(page)} retryLabel={t("Riprova", $language)} />{/if}
  {/if}
</ModPanelLayout>

<ModFilterDialog
  open={filtersOpen}
  {filters}
  {categories}
  onChange={(next) => (filters = next)}
  onClose={() => (filtersOpen = false)}
/>

