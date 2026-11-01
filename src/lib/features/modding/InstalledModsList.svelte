<script lang="ts">
  import { untrack } from "svelte";
  import { t, language } from "../../i18n";
  import Badge from "../../components/ui/Badge.svelte";
  import Button from "../../components/ui/Button.svelte";
  import Callout from "../../components/ui/Callout.svelte";
  import ConfirmDialog from "../../components/ui/ConfirmDialog.svelte";
  import EmptyState from "../../components/ui/EmptyState.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import Icon from "../../components/ui/Icon.svelte";
  import IconButton from "../../components/ui/IconButton.svelte";
  import Panel from "../../components/ui/Panel.svelte";
  import SelectField from "../../components/ui/SelectField.svelte";
  import StateBlock from "../../components/ui/StateBlock.svelte";
  import TextField from "../../components/ui/TextField.svelte";
  import Toggle from "../../components/ui/Toggle.svelte";
  import { pickModArchive } from "../../services/dialog";
  import type { ModProfile, PackageDetails, ProfileMod } from "../../services/modding";
  import {
    importLocalArchive,
    installPackages,
    modUpdates,
    packageDetails,
    profileMods,
    setAllModsEnabled,
    setModsEnabled,
    uninstallMod,
    updateMods,
  } from "../../stores/modding";
  import { showToast } from "../../stores/toast";
  import DisableDependentsDialog from "./DisableDependentsDialog.svelte";
  import {
    arrangeInstalledMods,
    classifyModdingError,
    defaultInstalledFilters,
    dependencyFix,
    dependencyIssueLabels,
    dependencyIssues,
    dependentsOf,
    describeModdingError,
    disabledPositionOptions,
    displayPackageId,
    enabledDependents,
    enablePlan,
    hasInstalledFilters,
    indexByFullName,
    installedSortOptions,
    installNotice,
    modKey,
    packageTitle,
    splitFullName,
    type DependencyIssues,
    type DisabledPosition,
    type InstalledSort,
    type ResultNotice,
  } from "./modding-model";
  import { openModTab, type ModTabs } from "./mod-panel-model";
  import ModIcon from "./ModIcon.svelte";
  import ModPanelLayout from "./ModPanelLayout.svelte";

  let {
    gameId,
    ecosystemGameId,
    profile,
    locked,
    busy,
    tabs,
    onTabsChange,
    onBrowse,
  }: {
    gameId: string;
    ecosystemGameId: string;
    profile: ModProfile;
    // The game is running with this profile, so its files cannot change.
    locked: boolean;
    busy: boolean;
    tabs: ModTabs;
    onTabsChange: (tabs: ModTabs) => void;
    onBrowse: () => void;
  } = $props();

  const mods = $derived(profileMods(profile.id));
  const updates = $derived(modUpdates(profile.id));
  const updateIndex = $derived(indexByFullName($updates.data));
  const installed = $derived(indexByFullName($mods.data));
  const updateKeys = $derived(new Set($updates.data.map((update) => modKey(update.fullName))));
  const switchable = $derived($mods.data.filter((mod) => !mod.isLoader));
  const enabledCount = $derived($mods.data.filter((mod) => mod.enabled).length);
  const disabled = $derived(locked || busy);

  let filters = $state({ ...defaultInstalledFilters });
  const visible = $derived(arrangeInstalledMods($mods.data, filters, updateKeys));
  const issues = $derived(dependencyIssues($mods.data));
  const sortOptions = $derived(installedSortOptions($language));
  const positionOptions = $derived(disabledPositionOptions($language));

  let details = $state<Record<string, PackageDetails | null>>({});
  let actionError = $state<string | null>(null);
  let notice = $state<ResultNotice | null>(null);
  let removeTarget = $state<ProfileMod | null>(null);
  let removing = $state(false);
  let toggleRevision = $state(0);
  let disableTarget = $state<ProfileMod | null>(null);

  $effect(() => {
    void mods.load();
    void updates.load();
  });

  $effect(() => {
    const list = $mods.data;
    const community = ecosystemGameId;
    let cancelled = false;
    const known = untrack(() => details);
    for (const mod of list) {
      const key = modKey(mod.fullName);
      if (!mod.onlineSource || key in known) continue;
      packageDetails(community, mod.fullName).then(
        (value) => { if (!cancelled) details = { ...details, [key]: value }; },
        (error: unknown) => {
          console.warn("Could not read mod details", error);
          if (!cancelled) details = { ...details, [key]: null };
        },
      );
    }
    return () => { cancelled = true; };
  });

  const removeDependents = $derived(removeTarget === null ? [] : dependentsOf(removeTarget.fullName, $mods.data));
  const disableDependents = $derived(disableTarget === null ? [] : enabledDependents(disableTarget.fullName, $mods.data));

  function fail(error: unknown): void {
    const failure = classifyModdingError(error, $language);
    if (failure.kind === "cancelled") showToast(failure.message, "info");
    else actionError = failure.message;
  }

  async function run(action: () => Promise<void>): Promise<void> {
    actionError = null;
    notice = null;
    try {
      await action();
    } catch (error) {
      fail(error);
    }
  }

  function showInstall(result: Awaited<ReturnType<typeof updateMods>>): void {
    const next = installNotice(result, $language);
    if (next.tone === "success" && next.items.length === 0) showToast(next.message, "success");
    else notice = next;
  }

  const switchMods = (fullNames: string[], enabled: boolean) => run(async () => {
    try {
      await setModsEnabled(gameId, profile.id, fullNames, enabled);
    } finally {
      toggleRevision++;
    }
  });

  // Switching a mod on also switches on the mods it needs; switching it off asks about the mods that need it.
  function toggle(mod: ProfileMod, enabled: boolean): void {
    if (enabled) {
      void switchMods(enablePlan([mod.fullName], $mods.data), true);
    } else if (enabledDependents(mod.fullName, $mods.data).length === 0) {
      void switchMods([mod.fullName], false);
    } else {
      disableTarget = mod;
      toggleRevision++;
    }
  }

  function disableChosen(withDependents: boolean): void {
    const mod = disableTarget;
    if (mod === null) return;
    const names = withDependents ? [...disableDependents, mod.fullName] : [mod.fullName];
    disableTarget = null;
    void switchMods(names, false);
  }

  const fixDependencies = (found: DependencyIssues) => run(async () => {
    const fix = dependencyFix(found, $mods.data);
    if (fix.enable.length > 0) await setModsEnabled(gameId, profile.id, fix.enable, true);
    if (fix.install.length > 0) showInstall(await installPackages(gameId, profile.id, fix.install.map((fullName) => ({ fullName }))));
  });

  const switchAll = (enabled: boolean) => run(async () => {
    const outcome = await setAllModsEnabled(gameId, profile.id, enabled);
    if (outcome.failed.length === 0) {
      showToast(enabled ? t("Mod attivate: {0}.", $language, [outcome.changed]) : t("Mod disattivate: {0}.", $language, [outcome.changed]), "success");
      return;
    }
    notice = {
      tone: "warning",
      message: enabled
        ? t("Mod attivate: {0}. Alcune non sono state attivate:", $language, [outcome.changed])
        : t("Mod disattivate: {0}. Alcune non sono state disattivate:", $language, [outcome.changed]),
      items: outcome.failed.map((failure) => `${packageTitle(splitFullName(failure.fullName).name)}: ${describeModdingError(failure.message, $language)}`),
    };
  });

  const installVersion = (fullName: string, version: string | null) => run(async () =>
    showInstall(await installPackages(gameId, profile.id, [version === null ? { fullName } : { fullName, version }])));

  const updateAll = () => run(async () => showInstall(await updateMods(gameId, profile.id)));
  const updateOne = (mod: ProfileMod) => run(async () => showInstall(await updateMods(gameId, profile.id, [mod.fullName])));

  const importArchive = () => run(async () => {
    const path = await pickModArchive();
    if (path === null) return;
    const outcome = await importLocalArchive(gameId, profile.id, path);
    const name = packageTitle(splitFullName(outcome.installed.fullName).name);
    if (outcome.missingDependencies.length === 0) showToast(t("{0} importata.", $language, [name]), "success");
    else notice = {
      tone: "warning",
      message: t("{0} importata. Mancano alcune dipendenze: installale dal catalogo.", $language, [name]),
      items: outcome.missingDependencies.map(displayPackageId),
    };
  });

  async function confirmRemove(): Promise<void> {
    const mod = removeTarget;
    if (mod === null) return;
    removing = true;
    actionError = null;
    notice = null;
    try {
      const outcome = await uninstallMod(gameId, profile.id, mod.fullName);
      const name = packageTitle(splitFullName(mod.fullName).name);
      if (outcome.dependents.length === 0) showToast(t("{0} disinstallata.", $language, [name]), "success");
      else notice = {
        tone: "warning",
        message: t("{0} disinstallata. Queste mod dipendono da essa e potrebbero non funzionare:", $language, [name]),
        items: outcome.dependents.map((fullName) => packageTitle(splitFullName(fullName).name)),
      };
      removeTarget = null;
    } catch (error) {
      removeTarget = null;
      fail(error);
    } finally {
      removing = false;
    }
  }
</script>

<ModPanelLayout
  {ecosystemGameId}
  {tabs}
  {installed}
  updates={updateIndex}
  {disabled}
  openOnInstalled
  onChange={onTabsChange}
  onInstall={(fullName, version) => void installVersion(fullName, version)}
>
  <div class="flex flex-col gap-4">
    {#if actionError !== null}<ErrorBanner message={actionError} />{/if}
    {#if notice !== null}
      <Callout tone={notice.tone} message={notice.message} items={notice.items} onDismiss={() => (notice = null)} />
    {/if}

    <StateBlock
      status={$mods.status === "empty" ? "ready" : $mods.status}
      hasData={$mods.data.length > 0}
      loadingMessage={t("Caricamento delle mod...", $language)}
      skeletonRows={3}
      skeletonClass="h-18"
      error={$mods.error}
      onRetry={() => void mods.reload()}
    />

    {#if $mods.status === "empty"}
      <EmptyState
        icon="package"
        title={t("Nessuna mod in questo profilo", $language)}
        description={t("Sfoglia il catalogo Thunderstore: BepInEx viene installato insieme alla prima mod.", $language)}
      >
        <div class="flex flex-wrap justify-center gap-2">
          <Button label={t("Sfoglia le mod", $language)} onClick={onBrowse} />
          <Button label={t("Importa mod locale", $language)} variant="secondary" {disabled} onClick={() => void importArchive()} />
        </div>
      </EmptyState>
    {:else if $mods.data.length > 0}
      <Panel
        title={t("Mod installate", $language)}
        description={t("{0} installate · {1} attive", $language, [$mods.data.length, enabledCount])}
      >
        {#snippet actions()}
          <div class="flex flex-wrap gap-2">
            <Button label={t("Importa mod locale", $language)} variant="secondary" size="sm" {disabled} onClick={() => void importArchive()}>
              <Icon name="upload" size="size-4" />
            </Button>
            <Button label={t("Attiva tutte", $language)} variant="secondary" size="sm" disabled={disabled || !switchable.some((mod) => !mod.enabled)} onClick={() => void switchAll(true)} />
            <Button label={t("Disattiva tutte", $language)} variant="secondary" size="sm" disabled={disabled || !switchable.some((mod) => mod.enabled)} onClick={() => void switchAll(false)} />
            {#if $updates.data.length > 0}
              <Button label={t("Aggiorna tutto ({0})", $language, [$updates.data.length])} variant="primary" size="sm" {disabled} onClick={() => void updateAll()}>
                <Icon name="download" size="size-4" />
              </Button>
            {/if}
          </div>
        {/snippet}
        <div class="mb-3 flex flex-wrap items-end gap-2">
          <div class="min-w-48 flex-1">
            <TextField id="mod-installed-search" type="search" labelHidden label={t("Cerca tra le mod installate", $language)} placeholder={t("Cerca per nome o autore", $language)} bind:value={filters.text} />
          </div>
          <SelectField id="mod-installed-sort" labelHidden label={t("Ordina per", $language)} value={filters.sort} options={sortOptions} onChange={(value) => (filters.sort = value as InstalledSort)} class="w-52" />
          <IconButton
            label={filters.descending ? t("Ordine decrescente", $language) : t("Ordine crescente", $language)}
            title={filters.descending ? t("Ordine decrescente", $language) : t("Ordine crescente", $language)}
            variant="soft"
            onClick={() => (filters.descending = !filters.descending)}
          >
            <Icon name="chevron-down" size="size-4 transition-transform motion-reduce:transition-none {filters.descending ? '' : 'rotate-180'}" />
          </IconButton>
          <SelectField id="mod-installed-disabled" labelHidden label={t("Posizione delle mod disattivate", $language)} value={filters.disabled} options={positionOptions} onChange={(value) => (filters.disabled = value as DisabledPosition)} class="w-52" />
          {#if $updates.data.length > 0}
            <Toggle label={t("Solo con aggiornamenti", $language)} bind:checked={filters.onlyUpdates} />
          {/if}
        </div>
        {#if visible.length === 0}
          <EmptyState icon="search" title={t("Nessuna mod corrisponde ai filtri", $language)}>
            {#if hasInstalledFilters(filters)}
              <Button label={t("Azzera i filtri", $language)} variant="secondary" onClick={() => (filters = { ...filters, text: "", onlyUpdates: false })} />
            {/if}
          </EmptyState>
        {:else}
          <ul class="flex flex-col gap-2">
            {#each visible as mod (mod.fullName)}
              {@const name = splitFullName(mod.fullName)}
              {@const title = packageTitle(name.name)}
              {@const detail = details[modKey(mod.fullName)] ?? null}
              {@const update = updateIndex.get(modKey(mod.fullName))}
              {@const issue = issues.get(modKey(mod.fullName))}
              {@const opened = tabs.active === modKey(mod.fullName)}
            <li class="flex flex-wrap items-center gap-3 rounded-xl legio-glass p-3 transition-colors hover:bg-white/5 light:hover:bg-zinc-900/5 {opened ? 'ring-1 ring-legio-accent ring-inset' : ''}">
              <div class="min-w-0 flex-1 basis-48">
                <button
                  type="button"
                  class="flex w-full min-w-0 items-center gap-3 rounded-lg text-left focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-(--legio-focus)"
                  aria-label={t("Dettagli di {0}", $language, [title])}
                  aria-current={opened ? "true" : undefined}
                  onclick={() => onTabsChange(openModTab(tabs, mod.fullName))}
                >
                  <ModIcon source={detail?.icon ?? null} class="size-12 shrink-0 rounded-lg {mod.enabled ? '' : 'opacity-50 grayscale'}" />
                  <span class="min-w-0 flex-1">
                    <span class="flex flex-wrap items-center gap-2">
                      <span class="min-w-0 truncate font-medium {mod.enabled ? 'text-zinc-50 light:text-zinc-900' : 'text-zinc-400 light:text-zinc-500'}">{title}</span>
                      <span class="text-xs tabular-nums text-zinc-500">{mod.version}</span>
                      {#if mod.isLoader}<Badge tone="info" title={t("Loader", $language)} />{/if}
                      {#if !mod.onlineSource}<Badge title={t("Locale", $language)} />{/if}
                      {#if !mod.enabled}<Badge title={t("Disattivata", $language)} />{/if}
                      {#if update !== undefined}<Badge tone="success" title={t("Aggiornamento {0}", $language, [update.latestVersion])} />{/if}
                    </span>
                    <span class="mt-0.5 block truncate text-xs text-zinc-400 light:text-zinc-600">
                      {name.owner}{#if detail !== null && detail.description !== ""} · {detail.description}{/if}
                    </span>
                  </span>
                </button>
                {#if issue !== undefined}
                  <div class="mt-1.5 flex flex-wrap items-center gap-1.5">
                    {#each dependencyIssueLabels(issue, $language) as label, index (index)}
                      <Badge tone="warning" title={label}><Icon name="warning" size="size-3" /></Badge>
                    {/each}
                    <Button label={t("Risolvi", $language)} variant="secondary" size="sm" {disabled} onClick={() => void fixDependencies(issue)} />
                  </div>
                {/if}
              </div>
              <div class="flex shrink-0 items-center gap-3">
                  {#if update !== undefined}
                    <Button label={t("Aggiorna", $language)} variant="secondary" size="sm" {disabled} onClick={() => void updateOne(mod)} />
                  {/if}
                  {#key toggleRevision}
                    <span title={mod.isLoader ? t("Il loader resta sempre attivo", $language) : undefined}>
                      <Toggle
                        labelHidden
                        label={t("Attiva {0}", $language, [title])}
                        checked={mod.enabled}
                        disabled={disabled || mod.isLoader}
                        onChange={(enabled) => void toggle(mod, enabled)}
                      />
                    </span>
                  {/key}
                  <IconButton
                    variant="danger"
                    label={t("Disinstalla {0}", $language, [title])}
                    title={t("Disinstalla", $language)}
                    {disabled}
                    onClick={() => (removeTarget = mod)}
                  >
                    <Icon name="trash" size="size-4" />
                  </IconButton>
                </div>
              </li>
            {/each}
          </ul>
        {/if}
      </Panel>
    {/if}
  </div>
</ModPanelLayout>

<DisableDependentsDialog
  open={disableTarget !== null}
  title={disableTarget === null ? "" : packageTitle(splitFullName(disableTarget.fullName).name)}
  dependents={disableDependents.map((fullName) => packageTitle(splitFullName(fullName).name))}
  onDisableOne={() => disableChosen(false)}
  onDisableAll={() => disableChosen(true)}
  onClose={() => (disableTarget = null)}
/>

<ConfirmDialog
  open={removeTarget !== null}
  title={t("Disinstalla mod", $language)}
  message={t("Disinstallare {0} da questo profilo?", $language, [removeTarget === null ? "" : packageTitle(splitFullName(removeTarget.fullName).name)])}
  confirmLabel={t("Disinstalla", $language)}
  tone="danger"
  pending={removing}
  onConfirm={() => void confirmRemove()}
  onClose={() => (removeTarget = null)}
>
  {#if removeDependents.length > 0}
    <Callout
      tone="warning"
      variant="inline"
      message={t("Queste mod dipendono da essa e resteranno installate, ma potrebbero non funzionare:", $language)}
      items={removeDependents.map((fullName) => packageTitle(splitFullName(fullName).name))}
    />
  {/if}
</ConfirmDialog>
