<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { t, language } from "../../i18n";
  import Button from "../../components/ui/Button.svelte";
  import Callout from "../../components/ui/Callout.svelte";
  import ConfirmDialog from "../../components/ui/ConfirmDialog.svelte";
  import EmptyState from "../../components/ui/EmptyState.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import Icon from "../../components/ui/Icon.svelte";
  import IconButton from "../../components/ui/IconButton.svelte";
  import SegmentedControl from "../../components/ui/SegmentedControl.svelte";
  import SelectField from "../../components/ui/SelectField.svelte";
  import StateBlock from "../../components/ui/StateBlock.svelte";
  import { getModProfileSize, openModProfileFolder, type ModProfile } from "../../services/modding";
  import { launchStateByGame } from "../../stores/launch";
  import { activeSection, selectedModGame, setLeaveGuard } from "../../stores/navigation";
  import {
    bindGame,
    busyProfiles,
    cancelInstall,
    catalogFor,
    catalogs,
    createProfile,
    deleteProfile,
    duplicateProfile,
    ensureCatalog,
    installLoader,
    installProgress,
    loadModdableGames,
    loadOnce,
    modLaunchStatus,
    moddableGameById,
    moddableGames,
    modProfiles,
    modUpdates,
    profileMods,
    renameProfile,
    selectProfile,
  } from "../../stores/modding";
  import { showTitleBarContent } from "../../stores/title-bar";
  import { showToast } from "../../stores/toast";
  import { toMessage } from "../../utils/errors";
  import { formatBytes } from "../../utils/format";
  import EcosystemGameDialog from "./EcosystemGameDialog.svelte";
  import InstalledModsList from "./InstalledModsList.svelte";
  import InstallProgressCard from "./InstallProgressCard.svelte";
  import ModBrowser from "./ModBrowser.svelte";
  import {
    canChooseEcosystemGame,
    classifyModdingError,
    copyProfileName,
    describeModdingError,
    installNotice,
    launchNotice,
    moddableGameState,
    profileChoices,
    shownProfile,
  } from "./modding-model";
  import ModConfigEditor from "./ModConfigEditor.svelte";
  import { emptyModTabs, type ModTabs } from "./mod-panel-model";
  import ProfileImportDialog from "./ProfileImportDialog.svelte";
  import ProfileNameDialog from "./ProfileNameDialog.svelte";
  import ProfileShareDialog from "./ProfileShareDialog.svelte";

  let { gameId }: { gameId: string } = $props();

  type Tab = "installed" | "browse" | "config";
  type NameDialog = { kind: "create" | "rename" | "duplicate" };

  onMount(() => {
    void loadModdableGames();
    return setLeaveGuard((proceed) => {
      if (!configDirty) return false;
      discardAction = proceed;
      return true;
    });
  });

  const game = $derived($moddableGameById.get(gameId) ?? null);
  const gameState = $derived(game === null ? null : moddableGameState(game));
  const ecosystemGameId = $derived(game?.binding?.ecosystemGameId ?? null);
  const profiles = $derived(modProfiles(gameId));
  const launchStatus = $derived(modLaunchStatus(gameId));
  const shown = $derived(shownProfile(game, $profiles.data));
  const profile = $derived(shown?.profile ?? null);
  const profileActive = $derived(shown?.active ?? false);
  const mods = $derived(profile === null ? null : profileMods(profile.id));
  const running = $derived(($launchStateByGame.get(gameId)?.status ?? "idle") !== "idle");
  const busy = $derived(profile !== null && $busyProfiles.has(profile.id));
  const progress = $derived(profile === null ? null : ($installProgress.get(profile.id) ?? null));
  const notice = $derived(launchNotice($launchStatus.data));
  const syncedAt = $derived(ecosystemGameId === null ? null : (catalogFor($catalogs, ecosystemGameId).status?.syncedAt ?? null));
  const options = $derived(profileChoices($profiles.data));
  const managed = $derived(gameState === "ready" && game?.binding != null);
  const onPage = $derived($activeSection === "mods" && $selectedModGame?.gameId === gameId);

  let tab = $state<Tab>("installed");
  let actionError = $state<string | null>(null);
  let bindingOpen = $state(false);
  let nameDialog = $state<NameDialog | null>(null);
  let deleteOpen = $state(false);
  let deleting = $state(false);
  let cancelling = $state(false);
  let selectRevision = $state(0);
  let tabRevision = $state(0);
  let importOpen = $state(false);
  let shareOpen = $state(false);
  let configDirty = $state(false);
  let profileSize = $state<number | null>(null);
  let discardAction = $state<(() => void) | null>(null);
  // Opened mods stay while switching sections of this game and refer to its Thunderstore community.
  let modTabs = $state<ModTabs>(emptyModTabs);
  let installedTabs = $state<ModTabs>(emptyModTabs);
  let autoBindAttempted = false;

  // A game opened through its Steam match is bound on first visit, so profiles can be created right away.
  $effect(() => {
    const target = game;
    if (target === null || target.binding !== null || gameState !== "ready" || target.ecosystemGameId === null) return;
    const ecosystem = target.ecosystemGameId;
    untrack(() => {
      if (autoBindAttempted) return;
      autoBindAttempted = true;
      bindGame(gameId, ecosystem).catch((error: unknown) => (actionError = toMessage(error)));
    });
  });

  $effect(() => {
    if (game === null || game.binding === null) return;
    loadOnce(profiles);
    void launchStatus.load();
  });

  $effect(() => {
    if (ecosystemGameId !== null) void ensureCatalog(ecosystemGameId);
  });

  $effect(() => {
    void ecosystemGameId;
    untrack(() => {
      modTabs = emptyModTabs;
      installedTabs = emptyModTabs;
    });
  });

  $effect(() => {
    void syncedAt;
    const current = profile;
    if (current !== null) untrack(() => void modUpdates(current.id).reload());
  });

  $effect(() => {
    if (!onPage) return;
    return showTitleBarContent({
      title: profile?.name ?? null,
      start: managed && profile !== null ? profilePicker : undefined,
      end: managed ? profileActions : undefined,
    });
  });

  // Installs, imports and uninstalls reload the mod list or end a busy state, so the size follows them.
  $effect(() => {
    const current = profile;
    void $mods?.data;
    void busy;
    if (current === null) {
      profileSize = null;
      return;
    }
    let cancelled = false;
    getModProfileSize(current.id).then(
      (bytes) => { if (!cancelled) profileSize = bytes; },
      (error: unknown) => {
        console.warn("Could not measure the mod profile", error);
        if (!cancelled) profileSize = null;
      },
    );
    return () => { cancelled = true; };
  });

  async function run(action: () => Promise<void>): Promise<void> {
    actionError = null;
    try {
      await action();
    } catch (error) {
      const failure = classifyModdingError(error, $language);
      if (failure.kind === "cancelled") showToast(failure.message, "info");
      else actionError = failure.message;
    }
  }

  // Unsaved config edits are lost when the editor goes away, so ask first.
  function afterDiscard(action: () => void): boolean {
    if (!configDirty) {
      action();
      return true;
    }
    discardAction = action;
    return false;
  }

  function discardChanges(): void {
    const action = discardAction;
    discardAction = null;
    configDirty = false;
    action?.();
  }

  function selectTab(value: string): void {
    if (!afterDiscard(() => (tab = value as Tab))) tabRevision++;
  }

  function requestProfile(value: string): void {
    if (!afterDiscard(() => void changeProfile(value))) selectRevision++;
  }

  function onImported(imported: ModProfile): void {
    void modProfiles(gameId).reload();
    // An update keeps the profile id, so its cached mod list is stale.
    void profileMods(imported.id).reload();
    if (!running) afterDiscard(() => void run(() => selectProfile(gameId, imported.id)));
  }

  const changeProfile = (value: string) => run(async () => {
    try {
      await selectProfile(gameId, value);
    } catch (error) {
      selectRevision++;
      throw error;
    }
  });

  async function submitName(name: string): Promise<void> {
    const kind = nameDialog?.kind;
    if (kind === "rename" && profile !== null) {
      await renameProfile(gameId, profile.id, name);
      return;
    }
    const duplicate = kind === "duplicate" && profile !== null;
    const created = duplicate ? await duplicateProfile(gameId, profile.id, name) : await createProfile(gameId, name);
    if (running) return;
    await selectProfile(gameId, created.id);
    // Every modded launch needs BepInEx, so a new empty profile gets it right away; copies keep what they had.
    if (!duplicate) void installBepInEx(created.id);
  }

  async function confirmDelete(): Promise<void> {
    if (profile === null) return;
    deleting = true;
    actionError = null;
    try {
      const outcome = await deleteProfile(gameId, profile.id);
      deleteOpen = false;
      if (outcome.directoryError !== null) {
        showToast(t("Profilo eliminato, ma la sua cartella non è stata rimossa: {0}", $language, [outcome.directoryError]), "warning");
      }
    } catch (error) {
      deleteOpen = false;
      actionError = describeModdingError(toMessage(error), $language);
    } finally {
      deleting = false;
    }
  }

  const openFolder = () => run(async () => {
    if (profile !== null) await openModProfileFolder(profile.id);
  });

  const installBepInEx = (profileId: string) => run(async () => {
    if (running || $busyProfiles.has(profileId)) return;
    const result = installNotice(await installLoader(gameId, profileId), $language);
    showToast(result.message, result.tone === "warning" ? "warning" : "success");
  });

  async function cancel(): Promise<void> {
    if (profile === null) return;
    cancelling = true;
    try {
      await cancelInstall(profile.id);
    } catch (error) {
      actionError = toMessage(error);
    } finally {
      cancelling = false;
    }
  }

  const dialogTitle = $derived(nameDialog?.kind === "rename"
    ? t("Rinomina profilo", $language)
    : nameDialog?.kind === "duplicate" ? t("Duplica profilo", $language) : t("Nuovo profilo", $language));
  const dialogInitialName = $derived(nameDialog?.kind === "rename" && profile !== null
    ? profile.name
    : nameDialog?.kind === "duplicate" && profile !== null ? copyProfileName(profile.name, $profiles.data, $language) : "");
</script>

{#snippet profilePicker()}
  {#key selectRevision}
    <SelectField
      id="mod-active-profile"
      label={t("Profilo", $language)}
      labelHidden
      variant="toolbar"
      value={profile?.id ?? ""}
      {options}
      disabled={running || busy}
      onChange={requestProfile}
      class="w-56 min-w-0"
    />
  {/key}
{/snippet}

{#snippet profileActions()}
  <div class="legio-glass flex h-11 items-center gap-0.5 rounded-xl p-1" role="toolbar" aria-label={t("Profilo delle mod", $language)}>
    <IconButton label={t("Nuovo profilo", $language)} title={t("Nuovo profilo", $language)} onClick={() => afterDiscard(() => (nameDialog = { kind: "create" }))}>
      <Icon name="plus" size="size-4" />
    </IconButton>
    <IconButton label={t("Rinomina profilo", $language)} title={t("Rinomina profilo", $language)} disabled={profile === null} onClick={() => (nameDialog = { kind: "rename" })}>
      <Icon name="pencil" size="size-4" />
    </IconButton>
    <IconButton label={t("Duplica profilo", $language)} title={t("Duplica profilo", $language)} disabled={profile === null || busy} onClick={() => afterDiscard(() => (nameDialog = { kind: "duplicate" }))}>
      <Icon name="copy" size="size-4" />
    </IconButton>
    <IconButton label={t("Importa profilo", $language)} title={t("Importa profilo", $language)} onClick={() => (importOpen = true)}>
      <Icon name="upload" size="size-4" />
    </IconButton>
    <IconButton label={t("Condividi profilo", $language)} title={t("Condividi profilo", $language)} disabled={profile === null} onClick={() => (shareOpen = true)}>
      <Icon name="share" size="size-4" />
    </IconButton>
    <IconButton label={t("Apri la cartella del profilo", $language)} title={t("Apri la cartella del profilo", $language)} disabled={profile === null} onClick={() => void openFolder()}>
      <Icon name="folder-open" size="size-4" />
    </IconButton>
    <IconButton variant="danger" label={t("Elimina profilo", $language)} title={t("Elimina profilo", $language)} disabled={profile === null || running || busy} onClick={() => (deleteOpen = true)}>
      <Icon name="trash" size="size-4" />
    </IconButton>
  </div>
{/snippet}

{#if game === null}
  <div class="flex min-h-full flex-col gap-4">
    {#if $moddableGames.status === "error"}
      <ErrorBanner message={$moddableGames.error ?? ""} onRetry={() => void moddableGames.reload()} retryLabel={t("Riprova", $language)} />
    {:else if $moddableGames.status === "loading" || $moddableGames.status === "idle"}
      <StateBlock status="loading" loadingMessage={t("Caricamento...", $language)} error={null} />
    {:else}
      <EmptyState icon="mods" title={t("Il gioco non è più disponibile.", $language)} />
    {/if}
  </div>
{:else}
  <!-- The lists and the config editor fill the window and scroll inside. -->
  <div class="flex flex-col gap-4 {profile !== null ? 'h-full' : 'min-h-full'}">
    {#if actionError !== null}<ErrorBanner message={actionError} />{/if}
    {#if $profiles.status === "error"}
      <ErrorBanner message={$profiles.error ?? ""} onRetry={() => void profiles.reload()} retryLabel={t("Riprova", $language)} />
    {/if}

    {#if gameState !== "ready"}
      <EmptyState
        icon="mods"
        title={gameState === "unmatched" ? t("Gioco non riconosciuto da Thunderstore", $language) : t("Loader non ancora supportato", $language)}
        description={gameState === "unmatched"
          ? t("Collega il gioco Thunderstore corrispondente per installare le sue mod.", $language)
          : t("{0} usa un loader che Legio non supporta ancora. Per ora sono supportati i giochi con BepInEx.", $language, [game.displayName ?? game.name])}
      >
        {#if canChooseEcosystemGame(game)}
          <Button label={t("Collega a Thunderstore", $language)} variant={gameState === "unmatched" ? "primary" : "secondary"} onClick={() => (bindingOpen = true)} />
        {/if}
      </EmptyState>
    {:else if game.binding === null}
      {#if actionError === null}
        <StateBlock status="loading" loadingMessage={t("Collegamento a Thunderstore...", $language)} error={null} />
      {/if}
    {:else}
      {#if running}
        <Callout tone="info" message={t("Il gioco è in esecuzione: chiudilo per installare, attivare o rimuovere mod e per cambiare profilo.", $language)} />
      {/if}
      {#if profile !== null && !profileActive}
        {@const target = profile}
        <Callout
          tone="info"
          title={t("Profilo non attivo", $language)}
          message={t("Il gioco parte senza mod. Attiva questo profilo per avviarlo con le sue mod.", $language)}
          onRetry={running || busy ? undefined : () => requestProfile(target.id)}
          retryLabel={t("Attiva profilo", $language)}
        />
      {:else if profile !== null && notice === "launch-unsupported"}
        <Callout tone="warning" title={t("Avvio con mod non disponibile", $language)} message={t("Legio non può caricare le mod di questo gioco su questo sistema, ad esempio per i giochi Linux nativi: il gioco partirà senza mod.", $language)} />
      {:else if profile !== null && notice === "loader-missing" && !busy}
        {@const target = profile}
        <Callout
          tone="warning"
          title={t("BepInEx non è installato in questo profilo", $language)}
          message={t("Senza BepInEx il gioco non si avvia con questo profilo. Installalo, oppure scegli Vanilla per giocare senza mod.", $language)}
          onRetry={running ? undefined : () => void installBepInEx(target.id)}
          retryLabel={t("Installa BepInEx", $language)}
        />
      {/if}
      {#if $launchStatus.error !== null}
        <ErrorBanner tone="warning" message={$launchStatus.error} onRetry={() => void launchStatus.reload()} retryLabel={t("Riprova", $language)} />
      {/if}

      {#if busy}
        <InstallProgressCard {progress} {cancelling} onCancel={() => void cancel()} />
      {/if}

      <div class="flex shrink-0 flex-wrap items-center gap-x-4 gap-y-2 rounded-2xl legio-glass p-2">
        {#if profile !== null && ecosystemGameId !== null}
          {#key tabRevision}
            <SegmentedControl
              name="mod-tabs"
              label={t("Sezione delle mod", $language)}
              labelHidden
              class="w-full sm:w-96"
              value={tab}
              options={[
                { value: "installed", label: t("Installate", $language) },
                { value: "browse", label: t("Sfoglia", $language) },
                { value: "config", label: t("Config", $language) },
              ]}
              onChange={selectTab}
            />
          {/key}
        {/if}
        <div class="ml-auto flex min-w-0 items-center gap-3 pl-2 text-sm text-zinc-400 light:text-zinc-600">
          {#if profile !== null && profileSize !== null}
            <span class="inline-flex shrink-0 items-center gap-1.5 tabular-nums" title={t("Spazio occupato dal profilo", $language)}>
              <Icon name="folder" size="size-4" />
              {formatBytes(profileSize, $language)}
            </span>
          {/if}
          <span class="min-w-0 truncate">{t("Thunderstore: {0}", $language, [game.displayName ?? game.name])}</span>
          {#if canChooseEcosystemGame(game)}
            <IconButton size="sm" label={t("Cambia gioco Thunderstore", $language)} title={t("Cambia gioco Thunderstore", $language)} disabled={running} onClick={() => (bindingOpen = true)}>
              <Icon name="pencil" size="size-4" />
            </IconButton>
          {/if}
        </div>
      </div>

      {#if profile === null}
        <StateBlock status={$profiles.status} loadingMessage={t("Caricamento dei profili...", $language)} error={null} />
        {#if $profiles.status !== "loading" && $profiles.status !== "idle"}
          <EmptyState
            icon="mods"
            title={t("Nessun profilo", $language)}
            description={t("Crea un profilo per installare mod: ogni profilo ha le sue mod e le sue configurazioni. Per giocare senza mod scegli Vanilla nella pagina del gioco.", $language)}
          >
            <div class="flex flex-wrap justify-center gap-2">
              <Button label={t("Crea profilo", $language)} onClick={() => (nameDialog = { kind: "create" })}>
                <Icon name="plus" size="size-4" />
              </Button>
              <Button label={t("Importa profilo", $language)} variant="secondary" onClick={() => (importOpen = true)}>
                <Icon name="upload" size="size-4" />
              </Button>
            </div>
          </EmptyState>
        {/if}
      {:else if ecosystemGameId !== null}
        {#if tab === "installed"}
          <InstalledModsList {gameId} {ecosystemGameId} {profile} locked={running} {busy} tabs={installedTabs} onTabsChange={(next) => (installedTabs = next)} onBrowse={() => (tab = "browse")} />
        {:else if tab === "browse"}
          <ModBrowser {gameId} {ecosystemGameId} {profile} disabled={running || busy} tabs={modTabs} onTabsChange={(next) => (modTabs = next)} />
        {:else}
          <div class="flex min-h-0 flex-1 flex-col">
            <ModConfigEditor {gameId} profileId={profile.id} onDirtyChange={(dirty) => (configDirty = dirty)} />
          </div>
        {/if}
      {/if}
    {/if}
  </div>

  <EcosystemGameDialog
    open={bindingOpen}
    gameName={game.name}
    currentId={game.binding?.ecosystemGameId ?? null}
    suggestedId={game.ecosystemGameId}
    profileCount={$profiles.data.length}
    onClose={() => (bindingOpen = false)}
    onSelect={(id) => bindGame(gameId, id)}
  />

  <ProfileNameDialog
    open={nameDialog !== null}
    title={dialogTitle}
    confirmLabel={nameDialog?.kind === "rename" ? t("Rinomina", $language) : nameDialog?.kind === "duplicate" ? t("Duplica", $language) : t("Crea", $language)}
    initialName={dialogInitialName}
    profiles={$profiles.data}
    exceptId={nameDialog?.kind === "rename" ? (profile?.id ?? null) : null}
    onClose={() => (nameDialog = null)}
    onSubmit={submitName}
  />

  <ConfirmDialog
    open={deleteOpen}
    title={t("Elimina profilo", $language)}
    message={t("Eliminare il profilo {0} con tutte le sue mod e configurazioni? Il gioco tornerà ad avviarsi senza mod.", $language, [profile?.name ?? ""])}
    confirmLabel={t("Elimina", $language)}
    tone="danger"
    pending={deleting}
    onConfirm={() => void confirmDelete()}
    onClose={() => (deleteOpen = false)}
  />
  <ProfileImportDialog open={importOpen} {gameId} activeProfileId={profile?.id ?? null} onClose={() => (importOpen = false)} {onImported} />

  {#if profile !== null}
    <ProfileShareDialog open={shareOpen} {gameId} {profile} onClose={() => (shareOpen = false)} />
  {/if}

  <ConfirmDialog
    open={discardAction !== null}
    title={t("Modifiche non salvate", $language)}
    message={t("Le modifiche alla configurazione non salvate andranno perse. Continuare?", $language)}
    confirmLabel={t("Scarta modifiche", $language)}
    cancelLabel={t("Continua a modificare", $language)}
    tone="danger"
    onConfirm={discardChanges}
    onClose={() => (discardAction = null)}
  />
{/if}
