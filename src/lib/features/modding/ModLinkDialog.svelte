<script lang="ts">
  import { untrack } from "svelte";
  import { t, language } from "../../i18n";
  import Button from "../../components/ui/Button.svelte";
  import Callout from "../../components/ui/Callout.svelte";
  import Dialog from "../../components/ui/Dialog.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import SelectField from "../../components/ui/SelectField.svelte";
  import Spinner from "../../components/ui/Spinner.svelte";
  import type { ModLinkResolution } from "../../services/mod-links";
  import { launchStateByGame } from "../../stores/launch";
  import { busyProfiles, cancelInstall, installPackages, installProgress } from "../../stores/modding";
  import { currentModLink, dismissModLink, resolveLink } from "../../stores/mod-links";
  import { openModManager, selectSection } from "../../stores/navigation";
  import { showToast } from "../../stores/toast";
  import { toMessage } from "../../utils/errors";
  import InstallProgressCard from "./InstallProgressCard.svelte";
  import ModIcon from "./ModIcon.svelte";
  import {
    defaultLinkProfileId,
    linkGameOptions,
    linkInstallNote,
    linkInstallState,
    linkProfile,
    linkProfileOptions,
    missingVersionNames,
  } from "./mod-link-model";
  import { classifyModdingError, installNotice, packageTitle, type ResultNotice } from "./modding-model";

  const request = $derived($currentModLink);
  const target = $derived(request?.target ?? null);

  let resolution = $state<ModLinkResolution | null>(null);
  let resolving = $state(false);
  let loadError = $state<string | null>(null);
  let gameId = $state<string | null>(null);
  let profileId = $state<string | null>(null);
  let installing = $state(false);
  let cancelling = $state(false);
  let actionError = $state<string | null>(null);
  let notice = $state<ResultNotice | null>(null);
  let generation = 0;

  const candidate = $derived(resolution?.candidates.find((entry) => entry.gameId === gameId) ?? null);
  const profile = $derived(linkProfile(candidate, profileId));
  const running = $derived(candidate !== null && ($launchStateByGame.get(candidate.gameId)?.status ?? "idle") !== "idle");
  const busy = $derived(profile !== null && !installing && $busyProfiles.has(profile.id));
  const progress = $derived(profile === null ? null : ($installProgress.get(profile.id) ?? null));
  const installState = $derived(target === null ? "ready" : linkInstallState(profile, target.version, running, busy));
  const installNote = $derived(linkInstallNote(installState, profile, $language));
  const unsyncedNames = $derived(resolution?.unsynced.map((game) => game.name) ?? []);
  const missingNames = $derived(resolution === null ? [] : missingVersionNames(resolution));

  $effect(() => {
    const current = target;
    untrack(() => void begin(current));
  });

  async function begin(current: typeof target): Promise<void> {
    const mine = ++generation;
    resolution = null;
    loadError = null;
    gameId = null;
    profileId = null;
    installing = false;
    cancelling = false;
    actionError = null;
    notice = null;
    if (current === null) {
      resolving = false;
      return;
    }
    resolving = true;
    try {
      const resolved = await resolveLink(current.fullName, current.version);
      if (mine !== generation) return;
      resolution = resolved;
      const first = resolved.candidates[0] ?? null;
      gameId = first?.gameId ?? null;
      profileId = first === null ? null : defaultLinkProfileId(first);
    } catch (error) {
      if (mine === generation) loadError = toMessage(error);
    } finally {
      if (mine === generation) resolving = false;
    }
  }

  function selectGame(value: string): void {
    gameId = value;
    const next = resolution?.candidates.find((entry) => entry.gameId === value);
    profileId = next === undefined ? null : defaultLinkProfileId(next);
  }

  async function install(): Promise<void> {
    if (target === null || candidate === null || profile === null || installState !== "ready") return;
    installing = true;
    cancelling = false;
    actionError = null;
    try {
      const result = await installPackages(candidate.gameId, profile.id, [{ fullName: target.fullName, version: target.version }]);
      notice = installNotice(result, $language);
    } catch (error) {
      const failure = classifyModdingError(error, $language);
      if (failure.kind === "cancelled") showToast(failure.message, "info");
      else actionError = failure.message;
    } finally {
      installing = false;
      cancelling = false;
    }
  }

  async function cancel(): Promise<void> {
    if (profile === null) return;
    cancelling = true;
    try {
      await cancelInstall(profile.id);
    } catch (error) {
      cancelling = false;
      actionError = toMessage(error);
    }
  }

  function openManager(gameToOpen: string): void {
    dismissModLink();
    openModManager(gameToOpen);
  }

  function openModsPage(): void {
    dismissModLink();
    selectSection("mods");
  }
</script>

<Dialog open={request !== null} title={t("Installa mod da Thunderstore", $language)} dismissible={!installing} onClose={dismissModLink}>
  {#snippet children(close)}
    {#if request?.error != null}
      <Callout tone="danger" variant="inline" title={t("Link di Thunderstore non valido", $language)} message={request.error} />
      <div class="flex justify-end"><Button label={t("Chiudi", $language)} onClick={close} /></div>
    {:else if target !== null}
      <div class="flex items-start gap-3">
        <ModIcon source={resolution?.package?.icon ?? null} class="size-14 rounded-xl" />
        <div class="min-w-0 flex-1">
          <p class="truncate text-base font-semibold text-zinc-100 light:text-zinc-900">{packageTitle(target.name)}</p>
          <p class="text-sm text-zinc-400 light:text-zinc-600">{t("di {0} · versione {1}", $language, [target.owner, target.version])}</p>
          {#if resolution?.package?.description}
            <p class="mt-1 line-clamp-2 text-xs text-zinc-500">{resolution.package.description}</p>
          {/if}
        </div>
      </div>

      {#if resolving}
        <p class="flex items-center gap-2 text-sm text-zinc-400 light:text-zinc-600" role="status"><Spinner />{t("Ricerca nei giochi della libreria...", $language)}</p>
      {:else if loadError !== null}
        <ErrorBanner message={loadError} onRetry={() => void begin(target)} retryLabel={t("Riprova", $language)} />
        <div class="flex justify-end"><Button label={t("Chiudi", $language)} variant="secondary" onClick={close} /></div>
      {:else if candidate === null}
        {#if missingNames.length > 0}
          <Callout
            tone="warning"
            variant="inline"
            message={t("Questa versione della mod non è nel catalogo di questi giochi, nemmeno dopo l'aggiornamento: potrebbe essere stata rimossa da Thunderstore.", $language)}
            items={missingNames}
          />
        {:else}
          <Callout
            tone="warning"
            variant="inline"
            message={t("Nessun gioco della libreria usa questa mod: collega il gioco a Thunderstore dalla pagina Mod.", $language)}
          />
        {/if}
        {#if unsyncedNames.length > 0}
          <Callout
            variant="inline"
            message={t("Il catalogo mod di questi giochi non è disponibile, quindi non è stato possibile controllarli.", $language)}
            items={unsyncedNames}
          />
        {/if}
        <div class="flex justify-end gap-2">
          <Button label={t("Chiudi", $language)} variant="secondary" onClick={close} />
          <Button label={t("Apri la pagina Mod", $language)} onClick={openModsPage} />
        </div>
      {:else if notice !== null}
        <Callout tone={notice.tone} variant="inline" message={notice.message} items={notice.items} />
        <div class="flex justify-end gap-2">
          <Button label={t("Chiudi", $language)} variant="secondary" onClick={close} />
          <Button label={t("Apri le mod del gioco", $language)} onClick={() => openManager(candidate.gameId)} />
        </div>
      {:else}
        {#if resolution !== null && resolution.candidates.length > 1}
          <SelectField id="mod-link-game" label={t("Gioco", $language)} value={candidate.gameId} options={linkGameOptions(resolution.candidates)} disabled={installing} onChange={selectGame} />
        {:else}
          <p class="text-sm text-zinc-300 light:text-zinc-700">{t("Gioco: {0}", $language, [candidate.name])}</p>
        {/if}
        {#if candidate.profiles.length === 0}
          <Callout variant="inline" message={t("Questo gioco non ha ancora profili mod. Aprilo dalla pagina Mod per crearne uno.", $language)} />
          <div class="flex justify-end gap-2">
            <Button label={t("Chiudi", $language)} variant="secondary" onClick={close} />
            <Button label={t("Apri le mod del gioco", $language)} onClick={() => openManager(candidate.gameId)} />
          </div>
        {:else}
          <SelectField id="mod-link-profile" label={t("Profilo", $language)} value={profileId ?? ""} options={linkProfileOptions(candidate, $language)} disabled={installing} onChange={(value) => (profileId = value)} />
          {#if installing}
            <InstallProgressCard {progress} {cancelling} onCancel={() => void cancel()} />
          {:else}
            {#if installNote !== null}
              <Callout tone={installState === "ready" ? "info" : "warning"} variant="inline" message={installNote} />
            {/if}
            {#if actionError !== null}<ErrorBanner message={actionError} />{/if}
            <div class="flex justify-end gap-2">
              <Button label={t("Annulla", $language)} variant="secondary" onClick={close} />
              <Button label={t("Installa", $language)} disabled={installState !== "ready"} onClick={() => void install()} />
            </div>
          {/if}
        {/if}
      {/if}
    {/if}
  {/snippet}
</Dialog>
