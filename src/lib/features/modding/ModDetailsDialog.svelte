<script lang="ts">
  import { t, language } from "../../i18n";
  import Badge from "../../components/ui/Badge.svelte";
  import Button from "../../components/ui/Button.svelte";
  import Dialog from "../../components/ui/Dialog.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import Icon from "../../components/ui/Icon.svelte";
  import SelectField from "../../components/ui/SelectField.svelte";
  import Skeleton from "../../components/ui/Skeleton.svelte";
  import { openModPackagePage, type PackageDetails, type PackageSummary, type ProfileMod } from "../../services/modding";
  import { packageDetails } from "../../stores/modding";
  import { toMessage } from "../../utils/errors";
  import { formatBytes, formatDate, regionalLocale } from "../../utils/format";
  import { displayPackageId, formatCount, modKey, packageTitle, splitPackageId, type InstalledState } from "./modding-model";
  import ModIcon from "./ModIcon.svelte";

  let {
    ecosystemGameId,
    summary,
    installed,
    installedMods,
    disabled,
    onClose,
    onInstall,
  }: {
    ecosystemGameId: string;
    summary: PackageSummary | null;
    installed: InstalledState;
    installedMods: ReadonlyMap<string, ProfileMod>;
    disabled: boolean;
    onClose: () => void;
    onInstall: (fullName: string, version: string | null) => void;
  } = $props();

  let details = $state<PackageDetails | null>(null);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let chosenVersion = $state("");
  // Keeps the content in place while the dialog animates closed.
  let shown = $state<PackageSummary | null>(null);

  $effect(() => {
    const current = summary;
    if (current === null) return;
    shown = current;
    details = null;
    error = null;
    chosenVersion = "";
    let cancelled = false;
    loading = true;
    packageDetails(ecosystemGameId, current.fullName).then(
      (value) => { if (!cancelled) details = value; },
      (cause: unknown) => { if (!cancelled) error = toMessage(cause); },
    ).finally(() => { if (!cancelled) loading = false; });
    return () => { cancelled = true; };
  });

  const latest = $derived(details?.versions[0]?.versionNumber ?? shown?.latestVersion ?? "");
  const versionNumber = $derived(chosenVersion === "" ? latest : chosenVersion);
  const version = $derived(details?.versions.find((entry) => entry.versionNumber === versionNumber) ?? null);
  const versionOptions = $derived((details?.versions ?? []).map((entry, index) => ({
    value: entry.versionNumber,
    label: index === 0 ? t("{0} (ultima)", $language, [entry.versionNumber]) : entry.versionNumber,
  })));
  const isLatest = $derived(versionNumber === latest);
  const installedVersion = $derived(installed.kind === "missing" ? null : installed.version);
  const installLabel = $derived.by(() => {
    if (installedVersion === versionNumber) return t("Installata", $language);
    if (installedVersion !== null) return isLatest ? t("Aggiorna a {0}", $language, [versionNumber]) : t("Installa la versione {0}", $language, [versionNumber]);
    return isLatest ? t("Installa", $language) : t("Installa la versione {0}", $language, [versionNumber]);
  });

  async function openPage(): Promise<void> {
    if (shown === null) return;
    try {
      await openModPackagePage(shown.packageUrl);
    } catch (cause) {
      error = toMessage(cause);
    }
  }
</script>

<Dialog open={summary !== null} title={shown === null ? "" : packageTitle(shown.name)} size="wide" {onClose}>
  {#snippet children(close)}
    {#if shown !== null}
      {@const summary = shown}
      <div class="flex flex-col gap-5">
        <div class="flex items-start gap-4">
          <ModIcon source={summary.icon} class="size-20 rounded-xl" />
          <div class="min-w-0 flex-1">
            <p class="text-sm text-zinc-400 light:text-zinc-600">{t("di {0}", $language, [summary.owner])}</p>
            <p class="mt-1 text-sm text-zinc-200 light:text-zinc-800">{version?.description ?? summary.description}</p>
            <div class="mt-2 flex flex-wrap gap-1.5">
              {#if summary.isDeprecated}<Badge tone="warning" title={t("Deprecata", $language)} />{/if}
              {#if summary.hasNsfwContent}<Badge tone="danger" title="NSFW" />{/if}
              {#each summary.categories as category (category)}<Badge title={category} />{/each}
            </div>
          </div>
        </div>

        <dl class="grid grid-cols-2 gap-3 sm:grid-cols-4">
          {#each [
            { label: t("Download totali", $language), value: formatCount(summary.totalDownloads, regionalLocale($language)) },
            { label: t("Valutazione", $language), value: formatCount(summary.ratingScore, regionalLocale($language)) },
            { label: t("Aggiornata", $language), value: formatDate(summary.dateUpdated, $language) },
            { label: t("Dimensione", $language), value: version === null ? "-" : formatBytes(version.fileSize, $language) },
          ] as stat (stat.label)}
            <div class="rounded-xl bg-white/5 px-3 py-2 light:bg-zinc-900/5">
              <dt class="text-xs text-zinc-400 light:text-zinc-600">{stat.label}</dt>
              <dd class="mt-0.5 truncate text-sm font-medium tabular-nums text-zinc-100 light:text-zinc-900">{stat.value}</dd>
            </div>
          {/each}
        </dl>

        {#if loading}
          <div class="flex flex-col gap-2" role="status">
            <span class="sr-only">{t("Caricamento...", $language)}</span>
            <Skeleton class="h-10" />
            <Skeleton class="h-16" />
          </div>
        {:else if details !== null}
          <div class="grid gap-4 sm:grid-cols-[14rem_minmax(0,1fr)]">
            <SelectField id="mod-version" label={t("Versione", $language)} value={versionNumber} options={versionOptions} onChange={(value) => (chosenVersion = value)} />
            <div class="min-w-0">
              <p class="text-sm text-zinc-400 light:text-zinc-600">{t("Dipendenze", $language)}</p>
              {#if version === null || version.dependencies.length === 0}
                <p class="mt-2 text-sm text-zinc-300 light:text-zinc-700">{t("Nessuna dipendenza.", $language)}</p>
              {:else}
                <ul class="mt-1.5 flex max-h-40 flex-col gap-1 overflow-y-auto">
                  {#each version.dependencies as dependency (dependency)}
                    {@const present = installedMods.has(modKey(splitPackageId(dependency).fullName))}
                    <li class="flex items-center gap-2 text-sm text-zinc-200 light:text-zinc-800">
                      <span class="min-w-0 flex-1 truncate">{displayPackageId(dependency)}</span>
                      {#if present}<Icon name="check" size="size-4 shrink-0 text-legio-success" />{/if}
                    </li>
                  {/each}
                </ul>
              {/if}
            </div>
          </div>
        {:else if error === null}
          <p class="text-sm text-zinc-400 light:text-zinc-600">{t("Il pacchetto non è più nel catalogo. Aggiorna il catalogo e riprova.", $language)}</p>
        {/if}

        {#if error !== null}<ErrorBanner message={error} />{/if}

        <div class="flex flex-wrap justify-end gap-2">
          <Button label={t("Apri su Thunderstore", $language)} variant="secondary" onClick={() => void openPage()}>
            <Icon name="external-link" size="size-4" />
          </Button>
          <Button label={t("Chiudi", $language)} variant="secondary" onClick={close} />
          <Button
            label={installLabel}
            disabled={disabled || details === null || installedVersion === versionNumber}
            onClick={() => { onInstall(summary.fullName, isLatest ? null : versionNumber); close(); }}
          >
            <Icon name="download" size="size-4" />
          </Button>
        </div>
      </div>
    {/if}
  {/snippet}
</Dialog>
