<script lang="ts">
  import { onMount } from "svelte";
  import { t, language } from "../../i18n";
  import type { Game } from "../../services/local-state";
  import {
    compatibilityRunnerLabel,
    getCompatibilityDefaults,
    getGameCompatibilityOverrides,
    listCompatibilityRunners,
    type CompatibilityRunner,
  } from "../../services/game-settings";
  import type { LaunchProfile } from "../../services/launch-profiles";
  import { appInfo } from "../../stores/app-info";
  import { usesCompatibilityRunner } from "./library-model";
  import { addLaunchProfile, copyLaunchProfile, launchProfiles, removeLaunchProfile, saveLaunchProfile } from "../../stores/launch-profiles";
  import { toMessage } from "../../utils/errors";
  import Button from "../../components/ui/Button.svelte";
  import Callout from "../../components/ui/Callout.svelte";
  import ConfirmDialog from "../../components/ui/ConfirmDialog.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import Icon from "../../components/ui/Icon.svelte";
  import IconButton from "../../components/ui/IconButton.svelte";
  import Panel from "../../components/ui/Panel.svelte";
  import SelectField from "../../components/ui/SelectField.svelte";
  import TextField from "../../components/ui/TextField.svelte";
  import Toggle from "../../components/ui/Toggle.svelte";
  import LinuxPerformanceFields from "./LinuxPerformanceFields.svelte";
  import {
    copyName,
    draftFromProfile,
    maxProfileNameLength,
    maxProfilesPerGame,
    overrideCount,
    overridesFromDraft,
    profileNameError,
    profilesOf,
    type ProfileDraft,
  } from "./launch-profiles";

  let { game }: { game: Game } = $props();

  const steamManaged = $derived(game.steamInstallPath !== null);
  const compatibilityGame = $derived(!steamManaged && usesCompatibilityRunner(game, $appInfo.data.platform));
  const profiles = $derived(profilesOf($launchProfiles.data, game.id));

  // The profile being edited; "new" while creating one.
  let editing = $state<string | "new" | null>(null);
  let draft = $state<ProfileDraft>(draftFromProfile(null));
  let saving = $state(false);
  let formError = $state<string | null>(null);
  let listError = $state<string | null>(null);
  let removeTarget = $state<LaunchProfile | null>(null);
  let removing = $state(false);
  let runners = $state<CompatibilityRunner[]>([]);
  let inheritedRunner = $state("");
  let gameModeAvailable = $state(false);
  let mangohudAvailable = $state(false);
  let gamescopeAvailable = $state(false);
  let environmentError = $state<string | null>(null);

  onMount(() => {
    if ($launchProfiles.status === "idle") void launchProfiles.load();
    if (!compatibilityGame) return;
    void Promise.all([listCompatibilityRunners(), getGameCompatibilityOverrides(game.id), getCompatibilityDefaults()])
      .then(([discovery, overrides, defaults]) => {
        runners = discovery.runners;
        gameModeAvailable = discovery.gameModeAvailable;
        mangohudAvailable = discovery.mangohudAvailable;
        gamescopeAvailable = discovery.gamescopeAvailable;
        inheritedRunner = overrides.runnerPath ?? defaults.runnerPath ?? discovery.runners[0]?.path ?? "";
      })
      .catch((cause) => (environmentError = toMessage(cause)));
  });

  const runnerOptions = $derived.by(() => {
    const known = runners.map((runner) => ({ value: runner.path, label: compatibilityRunnerLabel(runner) }));
    return draft.runnerPath.length > 0 && !known.some((option) => option.value === draft.runnerPath)
      ? [{ value: draft.runnerPath, label: draft.runnerPath }, ...known]
      : known;
  });
  const atLimit = $derived(profiles.length >= maxProfilesPerGame);

  function startEditing(profile: LaunchProfile | null): void {
    draft = draftFromProfile(profile);
    formError = null;
    editing = profile?.id ?? "new";
  }

  function toggleRunner(enabled: boolean): void {
    draft.runnerOverridden = enabled;
    draft.runnerPath = enabled ? inheritedRunner : "";
  }

  async function save(): Promise<void> {
    if (editing === null) return;
    saving = true;
    formError = null;
    try {
      const nameError = profileNameError(draft.name, profiles, editing === "new" ? null : editing);
      if (nameError !== null) throw new Error(nameError);
      const overrides = overridesFromDraft($state.snapshot(draft));
      if (editing === "new") await addLaunchProfile(game.id, draft.name, overrides);
      else await saveLaunchProfile(editing, draft.name, overrides);
      editing = null;
    } catch (cause) {
      formError = toMessage(cause);
    } finally {
      saving = false;
    }
  }

  async function duplicate(profile: LaunchProfile): Promise<void> {
    listError = null;
    try {
      await copyLaunchProfile(profile.id, copyName(profile.name, profiles));
    } catch (cause) {
      listError = toMessage(cause);
    }
  }

  async function confirmRemove(): Promise<void> {
    if (removeTarget === null) return;
    removing = true;
    listError = null;
    try {
      await removeLaunchProfile(removeTarget.id);
      if (editing === removeTarget.id) editing = null;
      removeTarget = null;
    } catch (cause) {
      listError = toMessage(cause);
      removeTarget = null;
    } finally {
      removing = false;
    }
  }

  function summary(profile: LaunchProfile): string {
    const count = overrideCount(profile.overrides);
    return count === 0 ? t("Eredita tutte le impostazioni del gioco", $language) : t("{0} impostazioni personalizzate", $language, [String(count)]);
  }
</script>

<Panel title={t("Profili di avvio", $language)} description={t("Configurazioni con nome per questo gioco, scelte dal menu del pulsante Gioca. Ogni campo non personalizzato eredita le impostazioni del gioco.", $language)}>
  {#snippet actions()}
    {#if editing === null}
      <Button label={t("Nuovo profilo", $language)} variant="secondary" disabled={atLimit} onClick={() => startEditing(null)}>
        <Icon name="plus" />
      </Button>
    {/if}
  {/snippet}

  {#if $launchProfiles.status === "error"}
    <ErrorBanner message={$launchProfiles.error ?? ""} onRetry={() => void launchProfiles.reload()} retryLabel={t("Riprova", $language)} />
  {/if}
  {#if listError !== null}<ErrorBanner message={listError} />{/if}
  {#if environmentError !== null}<ErrorBanner message={environmentError} tone="warning" />{/if}

  {#if editing === null}
    {#if profiles.length === 0}
      <p class="text-sm text-legio-fg-muted">{t("Nessun profilo. Il gioco si avvia con le sue impostazioni.", $language)}</p>
    {:else}
      <ul class="flex flex-col gap-2">
        {#each profiles as profile (profile.id)}
          <li class="flex items-center gap-3 rounded-card border border-legio-line-subtle p-4">
            <div class="min-w-0 flex-1">
              <p class="truncate text-sm font-medium text-legio-fg">{profile.name}</p>
              <p class="truncate text-xs text-legio-fg-muted">{summary(profile)}</p>
            </div>
            <IconButton label={t("Modifica {0}", $language, [profile.name])} title={t("Modifica", $language)} onClick={() => startEditing(profile)}><Icon name="pencil" /></IconButton>
            <IconButton label={t("Duplica {0}", $language, [profile.name])} title={t("Duplica", $language)} disabled={atLimit} onClick={() => void duplicate(profile)}><Icon name="copy" /></IconButton>
            <IconButton label={t("Elimina {0}", $language, [profile.name])} title={t("Elimina", $language)} variant="danger" onClick={() => (removeTarget = profile)}><Icon name="trash" /></IconButton>
          </li>
        {/each}
      </ul>
    {/if}
    {#if atLimit}<Callout tone="info" variant="inline" message={t("Hai raggiunto il limite di {0} profili per gioco.", $language, [String(maxProfilesPerGame)])} />{/if}
  {:else}
    <div class="flex flex-col gap-4">
      <TextField id="launch-profile-name" label={t("Nome del profilo", $language)} bind:value={draft.name} maxlength={maxProfileNameLength} placeholder={t("Ad esempio Finestra", $language)} disabled={saving} />

      <div class="flex flex-col gap-2">
        <Toggle label={steamManaged ? t("Personalizza gli argomenti di avvio", $language) : t("Personalizza gli argomenti dopo l'eseguibile", $language)} checked={draft.argumentsOverridden} disabled={saving} onChange={(enabled) => (draft.argumentsOverridden = enabled)} />
        <TextField id="launch-profile-arguments" label={t("Opzioni di avvio", $language)} bind:value={draft.argumentsText} disabled={saving || !draft.argumentsOverridden} placeholder={draft.argumentsOverridden ? "-windowed -novid" : t("Eredita dal gioco", $language)}
          hint={t("Racchiudi tra virgolette i valori che contengono spazi. Un campo vuoto rimuove gli argomenti del gioco.", $language)} />
      </div>

      {#if !steamManaged}
        <div class="flex flex-col gap-2">
          <Toggle label={t("Personalizza le variabili", $language)} checked={draft.environmentOverridden} disabled={saving} onChange={(enabled) => (draft.environmentOverridden = enabled)} />
          <TextField id="launch-profile-environment" type="textarea" rows={4} label={t("Variabili ambiente", $language)} bind:value={draft.environmentText} disabled={saving || !draft.environmentOverridden} placeholder={draft.environmentOverridden ? "" : t("Eredita dal gioco", $language)}
            hint={t("Una voce KEY=VALUE per riga. Sostituisce le variabili del gioco.", $language)} />
        </div>
      {/if}

      {#if compatibilityGame}
        <div class="flex flex-col gap-2">
          <Toggle label={t("Personalizza la versione Proton", $language)} checked={draft.runnerOverridden} disabled={saving} onChange={toggleRunner} />
          <SelectField id="launch-profile-runner" label={t("Runner di compatibilità", $language)} value={draft.runnerPath} options={runnerOptions} disabled={saving || !draft.runnerOverridden}
            onChange={(value) => (draft.runnerPath = value)} />
        </div>
        <div class="flex flex-col gap-2">
          <Toggle label={t("Personalizza le prestazioni", $language)} checked={draft.performanceOverridden} disabled={saving} onChange={(enabled) => (draft.performanceOverridden = enabled)} />
          <LinuxPerformanceFields value={draft.performance} {gameModeAvailable} {mangohudAvailable} {gamescopeAvailable} idPrefix="launch-profile" disabled={saving || !draft.performanceOverridden} onChange={(value) => (draft.performance = value)} />
        </div>
      {/if}

      {#if formError !== null}<ErrorBanner message={formError} />{/if}
      <div class="flex flex-wrap gap-2">
        <Button label={saving ? t("Salvataggio...", $language) : t("Salva profilo", $language)} disabled={saving} onClick={() => void save()} />
        <Button label={t("Annulla", $language)} variant="secondary" disabled={saving} onClick={() => (editing = null)} />
      </div>
    </div>
  {/if}
</Panel>

<ConfirmDialog
  open={removeTarget !== null}
  title={t("Elimina profilo", $language)}
  message={t("Eliminare il profilo {0}? Il gioco tornerà a usare le sue impostazioni.", $language, [removeTarget?.name ?? ""])}
  cancelLabel={t("Indietro", $language)}
  confirmLabel={t("Elimina", $language)}
  tone="danger"
  pending={removing}
  onConfirm={() => void confirmRemove()}
  onClose={() => (removeTarget = null)}
/>
