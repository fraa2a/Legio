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
  import StateBlock from "../../components/ui/StateBlock.svelte";
  import Toggle from "../../components/ui/Toggle.svelte";
  import { pickModArchive } from "../../services/dialog";
  import type { ModProfile, PackageDetails, ProfileMod } from "../../services/modding";
  import {
    importLocalArchive,
    installLoader,
    modUpdates,
    packageDetails,
    profileMods,
    setModEnabled,
    uninstallMod,
    updateMods,
  } from "../../stores/modding";
  import { showToast } from "../../stores/toast";
  import { toMessage } from "../../utils/errors";
  import {
    dependentsOf,
    describeModdingError,
    displayPackageId,
    indexByFullName,
    installNotice,
    isCancellation,
    isLoaderPackage,
    modKey,
    packageTitle,
    splitFullName,
    type ResultNotice,
  } from "./modding-model";
  import ModIcon from "./ModIcon.svelte";

  let {
    gameId,
    ecosystemGameId,
    profile,
    locked,
    busy,
    onBrowse,
  }: {
    gameId: string;
    ecosystemGameId: string;
    profile: ModProfile;
    // The game is running with this profile, so its files cannot change.
    locked: boolean;
    busy: boolean;
    onBrowse: () => void;
  } = $props();

  const mods = $derived(profileMods(profile.id));
  const updates = $derived(modUpdates(profile.id));
  const updateIndex = $derived(indexByFullName($updates.data));
  const enabledCount = $derived($mods.data.filter((mod) => mod.enabled).length);
  const disabled = $derived(locked || busy);

  let details = $state<Record<string, PackageDetails | null>>({});
  let actionError = $state<string | null>(null);
  let notice = $state<ResultNotice | null>(null);
  let removeTarget = $state<ProfileMod | null>(null);
  let removing = $state(false);
  let toggleRevision = $state(0);

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

  const removeDependents = $derived(removeTarget === null ? [] : dependentsOf(removeTarget.fullName, $mods.data, details));

  function fail(error: unknown): void {
    const message = toMessage(error);
    if (isCancellation(message)) showToast(t("Installazione annullata.", $language), "info");
    else actionError = describeModdingError(message, $language);
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

  const toggle = (mod: ProfileMod, enabled: boolean) => run(async () => {
    try {
      await setModEnabled(gameId, profile.id, mod.fullName, enabled);
    } finally {
      toggleRevision++;
    }
  });

  const updateAll = () => run(async () => showInstall(await updateMods(gameId, profile.id)));
  const updateOne = (mod: ProfileMod) => run(async () => showInstall(await updateMods(gameId, profile.id, [mod.fullName])));
  const installBepInEx = () => run(async () => showInstall(await installLoader(gameId, ecosystemGameId, profile.id)));

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
        <Button label={t("Installa solo BepInEx", $language)} variant="secondary" {disabled} onClick={() => void installBepInEx()} />
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
          {#if $updates.data.length > 0}
            <Button label={t("Aggiorna tutto ({0})", $language, [$updates.data.length])} variant="primary" size="sm" {disabled} onClick={() => void updateAll()}>
              <Icon name="download" size="size-4" />
            </Button>
          {/if}
        </div>
      {/snippet}
      <ul class="flex flex-col gap-2">
        {#each $mods.data as mod (mod.fullName)}
          {@const name = splitFullName(mod.fullName)}
          {@const title = packageTitle(name.name)}
          {@const detail = details[modKey(mod.fullName)] ?? null}
          {@const update = updateIndex.get(modKey(mod.fullName))}
          {@const loader = isLoaderPackage(mod.fullName)}
          <li class="flex flex-wrap items-center gap-3 rounded-xl legio-glass p-3">
            <ModIcon source={detail?.icon ?? null} class="size-11 rounded-lg {mod.enabled ? '' : 'opacity-50 grayscale'}" />
            <div class="min-w-0 flex-1 basis-48">
              <div class="flex flex-wrap items-center gap-2">
                <p class="min-w-0 truncate font-medium {mod.enabled ? 'text-zinc-50 light:text-zinc-900' : 'text-zinc-400 light:text-zinc-500'}">{title}</p>
                <span class="text-xs tabular-nums text-zinc-500">{mod.version}</span>
                {#if loader}<Badge tone="info" title={t("Loader", $language)} />{/if}
                {#if !mod.onlineSource}<Badge title={t("Locale", $language)} />{/if}
                {#if !mod.enabled}<Badge title={t("Disattivata", $language)} />{/if}
                {#if update !== undefined}<Badge tone="success" title={t("Aggiornamento {0}", $language, [update.latestVersion])} />{/if}
              </div>
              <p class="mt-0.5 truncate text-xs text-zinc-400 light:text-zinc-600">
                {name.owner}{#if detail !== null && detail.description !== ""} · {detail.description}{/if}
              </p>
            </div>
            <div class="flex shrink-0 items-center gap-3">
              {#if update !== undefined}
                <Button label={t("Aggiorna", $language)} variant="secondary" size="sm" {disabled} onClick={() => void updateOne(mod)} />
              {/if}
              {#key toggleRevision}
                <span title={loader ? t("Il loader resta sempre attivo", $language) : undefined}>
                  <Toggle
                    labelHidden
                    label={t("Attiva {0}", $language, [title])}
                    checked={mod.enabled}
                    disabled={disabled || loader}
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
    </Panel>
  {/if}
</div>

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
