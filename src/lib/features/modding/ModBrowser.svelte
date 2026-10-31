<script lang="ts">
  import { t, language } from "../../i18n";
  import Badge from "../../components/ui/Badge.svelte";
  import Button from "../../components/ui/Button.svelte";
  import Callout from "../../components/ui/Callout.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import Icon from "../../components/ui/Icon.svelte";
  import IconButton from "../../components/ui/IconButton.svelte";
  import ProgressBar from "../../components/ui/ProgressBar.svelte";
  import RefreshIcon from "../../components/ui/RefreshIcon.svelte";
  import SelectField from "../../components/ui/SelectField.svelte";
  import StateBlock from "../../components/ui/StateBlock.svelte";
  import TextField from "../../components/ui/TextField.svelte";
  import Toggle from "../../components/ui/Toggle.svelte";
  import { listModCategories, searchMods, type ModProfile, type PackageSort, type PackageSummary } from "../../services/modding";
  import { catalogFor, catalogs, installPackages, loadOnce, modUpdates, profileMods, refreshCatalog } from "../../stores/modding";
  import type { LoadStatus } from "../../stores/resource";
  import { showToast } from "../../stores/toast";
  import { toMessage } from "../../utils/errors";
  import { formatDate, formatDateTime, regionalLocale } from "../../utils/format";
  import {
    appendPage,
    browserPageSize,
    catalogPercent,
    classifyModdingError,
    defaultBrowserFilters,
    formatCount,
    indexByFullName,
    installedState,
    installNotice,
    packageQuery,
    packageTitle,
    sortOptions,
    type BrowserFilters,
    type ResultNotice,
  } from "./modding-model";
  import ModDetailsDialog from "./ModDetailsDialog.svelte";
  import ModIcon from "./ModIcon.svelte";

  let {
    gameId,
    ecosystemGameId,
    profile,
    disabled,
  }: {
    gameId: string;
    ecosystemGameId: string;
    profile: ModProfile;
    disabled: boolean;
  } = $props();

  let filters = $state<BrowserFilters>({ ...defaultBrowserFilters });
  let items = $state<PackageSummary[]>([]);
  let total = $state(0);
  let status = $state<LoadStatus>("idle");
  let searchError = $state<string | null>(null);
  let loadingMore = $state(false);
  let categories = $state<string[]>([]);
  let selected = $state<PackageSummary | null>(null);
  let actionError = $state<string | null>(null);
  let notice = $state<ResultNotice | null>(null);
  let request = 0;

  const catalog = $derived(catalogFor($catalogs, ecosystemGameId));
  const syncedAt = $derived(catalog.status?.syncedAt ?? null);
  const mods = $derived(profileMods(profile.id));
  const updates = $derived(modUpdates(profile.id));
  const installed = $derived(indexByFullName($mods.data));
  const updateIndex = $derived(indexByFullName($updates.data));
  const categoryOptions = $derived([
    { value: "", label: t("Tutte le categorie", $language) },
    ...categories.map((category) => ({ value: category, label: category })),
  ]);
  const locale = $derived(regionalLocale($language));

  async function search(offset: number): Promise<void> {
    const current = ++request;
    const query = packageQuery(filters, offset);
    if (offset === 0) status = "loading";
    else loadingMore = true;
    searchError = null;
    try {
      const page = await searchMods(ecosystemGameId, query);
      if (current !== request) return;
      items = offset === 0 ? page.items : appendPage(items, page.items);
      total = page.total;
      status = items.length === 0 ? "empty" : "ready";
    } catch (error) {
      if (current !== request) return;
      searchError = toMessage(error);
      status = offset === 0 ? "error" : status;
    } finally {
      if (current === request) loadingMore = false;
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
    const timer = setTimeout(() => void search(0), 300);
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

  const emptyMessage = $derived(syncedAt === null
    ? t("Il catalogo di questo gioco non è ancora stato scaricato.", $language)
    : t("Nessuna mod corrisponde ai filtri.", $language));
</script>

<div class="flex flex-col gap-4">
  <div class="flex flex-wrap items-center gap-3 rounded-xl legio-glass p-3">
    <div class="min-w-0 flex-1 basis-56 text-sm">
      {#if catalog.refreshing}
        <p class="text-zinc-200 light:text-zinc-800" role="status">
          {catalog.progress === null || catalog.progress.chunksTotal === 0
            ? t("Aggiornamento del catalogo...", $language)
            : t("Aggiornamento del catalogo: {0}%", $language, [catalogPercent(catalog.progress.chunksDone, catalog.progress.chunksTotal)])}
        </p>
        <ProgressBar
          value={catalog.progress === null ? 0 : catalogPercent(catalog.progress.chunksDone, catalog.progress.chunksTotal)}
          tone="accent"
          label={t("Avanzamento del catalogo", $language)}
          class="mt-2"
        />
      {:else if catalog.status !== null}
        <p class="text-zinc-300 light:text-zinc-700">
          {t("{0} pacchetti nel catalogo", $language, [formatCount(catalog.status.packageCount, locale)])}
          <span class="text-zinc-500"> · {t("aggiornato il {0}", $language, [formatDateTime(catalog.status.syncedAt, $language)])}</span>
        </p>
      {:else}
        <p class="text-zinc-400 light:text-zinc-600">{t("Catalogo Thunderstore non ancora scaricato.", $language)}</p>
      {/if}
    </div>
    <IconButton
      label={t("Aggiorna il catalogo", $language)}
      title={t("Aggiorna il catalogo", $language)}
      disabled={catalog.refreshing}
      onClick={() => void refreshCatalog(ecosystemGameId, true)}
    >
      <RefreshIcon spinning={catalog.refreshing} />
    </IconButton>
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

  <div class="flex flex-wrap gap-3">
    <div class="min-w-64 flex-[2]">
      <TextField id="mod-search" type="search" label={t("Cerca", $language)} placeholder={t("Nome, autore o descrizione", $language)} bind:value={filters.text} />
    </div>
    <SelectField id="mod-category" label={t("Categoria", $language)} class="min-w-48 flex-1 sm:max-w-64" value={filters.category} options={categoryOptions} onChange={(value) => (filters.category = value)} />
    <SelectField id="mod-sort" label={t("Ordina per", $language)} class="min-w-48 flex-1 sm:max-w-64" value={filters.sort} options={sortOptions($language)} onChange={(value) => (filters.sort = value as PackageSort)} />
  </div>
  <div class="flex flex-wrap gap-x-8 gap-y-2">
    <Toggle label={t("Mostra mod deprecate", $language)} checked={filters.includeDeprecated} onChange={(value) => (filters.includeDeprecated = value)} />
    <Toggle label={t("Mostra contenuti NSFW", $language)} checked={filters.includeNsfw} onChange={(value) => (filters.includeNsfw = value)} />
  </div>

  <StateBlock
    {status}
    hasData={items.length > 0}
    loadingMessage={t("Ricerca in corso...", $language)}
    skeletonRows={4}
    skeletonClass="h-20"
    emptyIcon="search"
    {emptyMessage}
    error={searchError}
    onRetry={() => void search(0)}
  />

  {#if items.length > 0}
    <p class="text-xs text-zinc-500">{t("{0} risultati", $language, [total])}</p>
    <ul class="flex flex-col gap-2">
      {#each items as item (item.fullName)}
        {@const installState = installedState(item.fullName, installed, updateIndex)}
        <li class="flex flex-wrap items-center gap-3 rounded-xl legio-glass p-3 transition-colors hover:bg-white/5 light:hover:bg-zinc-900/5">
          <button
            type="button"
            class="flex min-w-0 flex-1 basis-64 items-center gap-3 rounded-lg text-left focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-(--legio-focus)"
            aria-label={t("Dettagli di {0}", $language, [packageTitle(item.name)])}
            onclick={() => (selected = item)}
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
    {#if items.length < total}
      <div class="flex justify-center pt-2">
        <Button
          label={loadingMore ? t("Caricamento...", $language) : t("Carica altre {0}", $language, [browserPageSize])}
          variant="secondary"
          disabled={loadingMore}
          onClick={() => void search(items.length)}
        />
      </div>
    {/if}
    {#if searchError !== null && status !== "error"}<ErrorBanner message={searchError} onRetry={() => void search(items.length)} retryLabel={t("Riprova", $language)} />{/if}
  {/if}
</div>

<ModDetailsDialog
  {ecosystemGameId}
  summary={selected}
  installed={selected === null ? { kind: "missing" } : installedState(selected.fullName, installed, updateIndex)}
  installedMods={installed}
  {disabled}
  onClose={() => (selected = null)}
  onInstall={(fullName, version) => void install(fullName, version)}
/>
