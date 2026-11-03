<script lang="ts">
  import { t, language } from "../../i18n";
  import Badge from "../../components/ui/Badge.svelte";
  import Button from "../../components/ui/Button.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import Icon from "../../components/ui/Icon.svelte";
  import SegmentedControl from "../../components/ui/SegmentedControl.svelte";
  import SelectField from "../../components/ui/SelectField.svelte";
  import Skeleton from "../../components/ui/Skeleton.svelte";
  import StateBlock from "../../components/ui/StateBlock.svelte";
  import { openModExternalLink, openModPackagePage, type ModUpdate, type PackageDetails, type PackageDocument, type PackageSummary, type ProfileMod } from "../../services/modding";
  import { packageDetails } from "../../stores/modding";
  import type { LoadStatus } from "../../stores/resource";
  import { toMessage } from "../../utils/errors";
  import { formatBytes, formatDate, regionalLocale } from "../../utils/format";
  import { describeModdingError, displayPackageId, formatCount, installedState, modKey, packageTitle, splitFullName, splitPackageId } from "./modding-model";
  import { installActionLabel, versionChoices, type ModTab } from "./mod-panel-model";
  import ModDocument from "./ModDocument.svelte";
  import ModIcon from "./ModIcon.svelte";
  import { loadPackageDocument } from "./package-documents";

  let {
    ecosystemGameId,
    tab,
    installed,
    updates,
    disabled,
    openOnInstalled,
    onInstall,
    onOpen,
    onSummary,
  }: {
    ecosystemGameId: string;
    tab: ModTab;
    installed: ReadonlyMap<string, ProfileMod>;
    updates: ReadonlyMap<string, ModUpdate>;
    disabled: boolean;
    // Mods opened from the installed list start on the version in the profile.
    openOnInstalled: boolean;
    onInstall: (fullName: string, version: string | null) => void;
    onOpen: (fullName: string) => void;
    onSummary: (summary: PackageSummary) => void;
  } = $props();

  let details = $state<PackageDetails | null>(null);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let chosenVersion = $state("");
  type ModTabView = PackageDocument | "dependencies";
  let view = $state<ModTabView>("readme");
  let documentStatus = $state<LoadStatus>("idle");
  let documentHtml = $state("");
  let documentError = $state<string | null>(null);
  let documentAttempt = $state(0);
  // The tab object is replaced when its summary arrives; the name it shows never changes.
  const fullName = $derived(tab.fullName);

  $effect(() => {
    const name = fullName;
    if (isLocal) {
      loading = false;
      return;
    }
    let cancelled = false;
    packageDetails(ecosystemGameId, name).then(
      (value) => {
        if (cancelled) return;
        details = value;
        if (value !== null) onSummary(value);
      },
      (cause: unknown) => { if (!cancelled) error = describeModdingError(toMessage(cause), $language); },
    ).finally(() => { if (!cancelled) loading = false; });
    return () => { cancelled = true; };
  });

  // The readme and changelog belong to the selected version and load once the version is known.
  $effect(() => {
    const name = fullName;
    const number = versionNumber;
    if (isLocal) return;
    const kind = view;
    void documentAttempt;
    if (number === "" || kind === "dependencies") return;
    let cancelled = false;
    documentStatus = "loading";
    documentHtml = "";
    documentError = null;
    loadPackageDocument(name, number, kind).then(
      (html) => {
        if (cancelled) return;
        documentHtml = html ?? "";
        documentStatus = html === null ? "empty" : "ready";
      },
      (cause: unknown) => {
        if (cancelled) return;
        documentError = describeModdingError(toMessage(cause), $language);
        documentStatus = "error";
      },
    );
    return () => { cancelled = true; };
  });

  const info = $derived(details ?? tab.summary);
  const latest = $derived(details?.versions[0]?.versionNumber ?? "");
  const installedMod = $derived(installed.get(tab.key) ?? null);
  const isLocal = $derived(installedMod !== null && !installedMod.onlineSource);
  // Local mods, and installed ones missing from the catalog, only have what the profile stores.
  const minimal = $derived(installedMod !== null && (isLocal || (!loading && details === null)));
  const versionNumber = $derived(chosenVersion !== "" ? chosenVersion : openOnInstalled && installedMod !== null && details?.versions.some((entry) => entry.versionNumber === installedMod.version) ? installedMod.version : latest);
  const version = $derived(details?.versions.find((entry) => entry.versionNumber === versionNumber) ?? null);
  const isLatest = $derived(versionNumber === latest);
  const installState = $derived(installedState(fullName, installed, updates));
  const installedVersion = $derived(installState.kind === "missing" ? null : installState.version);
  const locale = $derived(regionalLocale($language));
  const fieldId = $derived(`mod-version-${tab.key}`);
  const websiteUrl = $derived(version?.websiteUrl ?? null);
  const donationLink = $derived(details?.donationLink ?? null);

  async function openLink(open: () => Promise<void>): Promise<void> {
    try {
      await open();
    } catch (cause) {
      error = describeModdingError(toMessage(cause), $language);
    }
  }
</script>

<div class="flex flex-col gap-4 p-4">
  {#if minimal && installedMod !== null}
    {@const name = splitFullName(installedMod.fullName)}
    <div class="flex items-start gap-3">
      <ModIcon source={null} class="size-16 rounded-xl" />
      <div class="min-w-0 flex-1">
        <h2 class="truncate text-base font-semibold text-zinc-50 light:text-zinc-900">{packageTitle(name.name)}</h2>
        {#if name.owner !== ""}<p class="text-sm text-zinc-400 light:text-zinc-600">{t("di {0}", $language, [name.owner])}</p>{/if}
        <div class="mt-1.5 flex flex-wrap items-center gap-1.5">
          <span class="text-xs tabular-nums text-zinc-400 light:text-zinc-600">{installedMod.version}</span>
          {#if installedMod.isLoader}<Badge tone="info" title={t("Loader", $language)} />{/if}
          {#if isLocal}<Badge title={t("Locale", $language)} />{/if}
        </div>
      </div>
    </div>
    <p class="text-sm text-zinc-200 light:text-zinc-800">{isLocal ? t("Mod importata da un archivio locale: non è su Thunderstore.", $language) : t("Il pacchetto non è più nel catalogo. Aggiorna il catalogo e riprova.", $language)}</p>
    <section aria-label={t("Dipendenze", $language)}>
      <h3 class="text-sm text-zinc-400 light:text-zinc-600">{t("Dipendenze", $language)}</h3>
      {#if installedMod.dependencies.length === 0}
        <p class="mt-1.5 text-sm text-zinc-300 light:text-zinc-700">{t("Nessuna dipendenza.", $language)}</p>
      {:else}
        <ul class="mt-1.5 flex flex-col gap-0.5">
          {#each installedMod.dependencies as dependency (dependency)}
            {@const title = packageTitle(splitFullName(dependency).name)}
            <li>
              <button
                type="button"
                class="flex w-full items-center gap-2 rounded-md px-2 py-1 text-left text-sm text-zinc-200 transition-colors hover:bg-white/10 focus-visible:outline-2 focus-visible:outline-(--legio-focus) light:text-zinc-800 light:hover:bg-zinc-900/10"
                aria-label={t("Apri {0}", $language, [title])}
                onclick={() => onOpen(dependency)}
              >
                <span class="min-w-0 flex-1 truncate">{title}</span>
                {#if installed.has(modKey(dependency))}
                  <span class="sr-only">{t("Installata", $language)}</span>
                  <Icon name="check" size="size-4 shrink-0 text-legio-success" />
                {/if}
              </button>
            </li>
          {/each}
        </ul>
      {/if}
    </section>
  {:else if info === null}
    <div class="flex flex-col gap-2" role="status">
      <span class="sr-only">{t("Caricamento...", $language)}</span>
      <Skeleton class="h-16" />
      <Skeleton class="h-24" />
    </div>
  {:else}
    {@const pageUrl = info.packageUrl}
    <div class="flex items-start gap-3">
      <ModIcon source={info.icon} class="size-16 rounded-xl" />
      <div class="min-w-0 flex-1">
        <h2 class="truncate text-base font-semibold text-zinc-50 light:text-zinc-900">{packageTitle(info.name)}</h2>
        <p class="text-sm text-zinc-400 light:text-zinc-600">{t("di {0}", $language, [info.owner])}</p>
        <div class="mt-1.5 flex flex-wrap gap-1.5">
          {#if info.isDeprecated}<Badge tone="warning" title={t("Deprecata", $language)} />{/if}
          {#if info.hasNsfwContent}<Badge tone="danger" title="NSFW" />{/if}
          {#each info.categories as category (category)}<Badge title={category} />{/each}
        </div>
      </div>
    </div>
    <p class="text-sm text-zinc-200 light:text-zinc-800">{version?.description ?? info.description}</p>

    <div class="flex flex-wrap gap-2">
      {#if websiteUrl !== null}
        <Button label={t("Sito web", $language)} variant="secondary" size="sm" onClick={() => void openLink(() => openModExternalLink(websiteUrl))}>
          <Icon name="globe" size="size-4" />
        </Button>
      {/if}
      {#if donationLink !== null}
        <Button label={t("Dona", $language)} variant="secondary" size="sm" onClick={() => void openLink(() => openModExternalLink(donationLink))}>
          <Icon name="heart" size="size-4" />
        </Button>
      {/if}
      <Button label={t("Apri su Thunderstore", $language)} variant="secondary" size="sm" onClick={() => void openLink(() => openModPackagePage(pageUrl))}>
        <Icon name="external-link" size="size-4" />
      </Button>
    </div>

    <dl class="grid grid-cols-2 gap-2">
      {#each [
        { label: t("Download totali", $language), value: formatCount(info.totalDownloads, locale) },
        { label: t("Valutazione", $language), value: formatCount(info.ratingScore, locale) },
        { label: t("Aggiornata", $language), value: formatDate(info.dateUpdated, $language) },
        { label: t("Dimensione", $language), value: version === null ? "-" : formatBytes(version.fileSize, $language) },
      ] as stat (stat.label)}
        <div class="rounded-lg bg-white/5 p-2.5 light:bg-zinc-900/5">
          <dt class="text-xs text-zinc-400 light:text-zinc-600">{stat.label}</dt>
          <dd class="mt-0.5 truncate text-sm font-medium tabular-nums text-zinc-100 light:text-zinc-900">{stat.value}</dd>
        </div>
      {/each}
    </dl>
  {/if}

  {#if loading && info !== null}
    <Skeleton class="h-10" />
  {:else if details !== null}
    <div class="flex flex-wrap items-end gap-2">
      <SelectField
        id={fieldId}
        label={t("Versione", $language)}
        class="min-w-40 flex-1"
        value={versionNumber}
        options={versionChoices(details.versions, $language)}
        onChange={(value) => (chosenVersion = value)}
      />
      <Button
        label={installActionLabel(installedVersion, versionNumber, isLatest, $language)}
        disabled={disabled || installedVersion === versionNumber}
        onClick={() => onInstall(fullName, isLatest ? null : versionNumber)}
      >
        <Icon name={installedVersion === versionNumber ? "check" : "download"} size="size-4" />
      </Button>
    </div>

    <div class="flex flex-col gap-3">
      <SegmentedControl
        name="mod-document-{tab.key}"
        label={t("Documento", $language)}
        labelHidden
        options={[
          { value: "readme", label: "Readme" },
          { value: "changelog", label: "Changelog" },
          { value: "dependencies", label: t("Dipendenze", $language) },
        ]}
        value={view}
        onChange={(value) => (view = value as ModTabView)}
      />
      {#if view === "dependencies"}
        {#if version === null || version.dependencies.length === 0}
          <p class="text-sm text-zinc-300 light:text-zinc-700">{t("Nessuna dipendenza.", $language)}</p>
        {:else}
          <ul class="flex flex-col gap-0.5">
            {#each version.dependencies as dependency (dependency)}
              {@const dependencyName = splitPackageId(dependency).fullName}
              <li>
                <button
                  type="button"
                  class="flex w-full items-center gap-2 rounded-md px-2 py-1 text-left text-sm text-zinc-200 transition-colors hover:bg-white/10 focus-visible:outline-2 focus-visible:outline-(--legio-focus) light:text-zinc-800 light:hover:bg-zinc-900/10"
                  aria-label={t("Apri {0}", $language, [displayPackageId(dependency)])}
                  onclick={() => onOpen(dependencyName)}
                >
                  <span class="min-w-0 flex-1 truncate">{displayPackageId(dependency)}</span>
                  {#if installed.has(modKey(dependencyName))}
                    <span class="sr-only">{t("Installata", $language)}</span>
                    <Icon name="check" size="size-4 shrink-0 text-legio-success" />
                  {/if}
                </button>
              </li>
            {/each}
          </ul>
        {/if}
      {:else}
        <StateBlock
          status={documentStatus}
          hasData={documentHtml !== ""}
          skeletonRows={3}
          skeletonClass="h-12"
          emptyIcon="file-code"
          emptyMessage={view === "readme" ? t("Nessun readme", $language) : t("Nessun changelog", $language)}
          error={documentError}
          onRetry={() => documentAttempt++}
        />
        {#if documentStatus === "ready"}
          <ModDocument html={documentHtml} onError={(message) => (error = describeModdingError(message, $language))} />
        {/if}
      {/if}
    </div>
  {:else if !loading && error === null && !minimal}
    <p class="text-sm text-zinc-400 light:text-zinc-600">{t("Il pacchetto non è più nel catalogo. Aggiorna il catalogo e riprova.", $language)}</p>
  {/if}

  {#if error !== null}<ErrorBanner message={error} />{/if}
</div>
