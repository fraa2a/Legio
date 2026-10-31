<script lang="ts">
  import { onDestroy, untrack } from "svelte";
  import { get } from "svelte/store";
  import { t, language } from "../../i18n";
  import { pickModProfileFile } from "../../services/dialog";
  import { importModProfileCode, importModProfileFile, type ProfileImportOutcome } from "../../services/mod-profile-exchange";
  import { cancelModInstall, observeInstallProgress, type InstallProgress, type ModProfile } from "../../services/modding";
  import { modProfiles } from "../../stores/modding";
  import { toMessage } from "../../utils/errors";
  import Button from "../../components/ui/Button.svelte";
  import Callout from "../../components/ui/Callout.svelte";
  import Dialog from "../../components/ui/Dialog.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import ProgressBar from "../../components/ui/ProgressBar.svelte";
  import SegmentedControl from "../../components/ui/SegmentedControl.svelte";
  import Spinner from "../../components/ui/Spinner.svelte";
  import TextField from "../../components/ui/TextField.svelte";
  import {
    canStartImport, describeProgress, fileNameOf, maxProfileNameLength, normalizeProfileCode, profileCodeError,
    profileNameError, summarizeImport, type ImportSource,
  } from "./mod-profile-exchange-model";

  let { open, gameId, onClose, onImported }: {
    open: boolean;
    gameId: string;
    onClose: () => void;
    onImported: (profile: ModProfile) => void;
  } = $props();

  let source = $state<ImportSource>("file");
  let path = $state<string | null>(null);
  let code = $state("");
  let name = $state("");
  let pending = $state(false);
  let cancelling = $state(false);
  let error = $state<string | null>(null);
  let outcome = $state<ProfileImportOutcome | null>(null);
  let progress = $state<InstallProgress | null>(null);
  let importProfileId = $state<string | null>(null);
  let unlisten: (() => void) | null = null;
  let existingProfiles = new Set<string>();

  const sourceOptions = $derived([
    { value: "file", label: t("File .r2z", $language) },
    { value: "code", label: t("Codice", $language) },
  ]);
  const codeError = $derived(profileCodeError(code, $language));
  const nameError = $derived(profileNameError(name, $language));
  const ready = $derived(canStartImport(source, path, code, name));
  const view = $derived(describeProgress(progress, $language));
  const summary = $derived(outcome === null ? null : summarizeImport(outcome));

  $effect(() => {
    if (open) untrack(reset);
  });

  onDestroy(() => stopListening());

  function reset(): void {
    source = "file";
    path = null;
    code = "";
    name = "";
    pending = false;
    cancelling = false;
    error = null;
    outcome = null;
    progress = null;
    importProfileId = null;
  }

  function stopListening(): void {
    unlisten?.();
    unlisten = null;
  }

  // The new profile id is unknown until its first event; installs into existing profiles are ignored.
  function track(event: InstallProgress): void {
    if (importProfileId === null && !existingProfiles.has(event.profileId)) importProfileId = event.profileId;
    if (event.profileId === importProfileId) progress = event;
  }

  async function chooseFile(): Promise<void> {
    try {
      const picked = await pickModProfileFile();
      if (picked !== null) path = picked;
    } catch (reason) {
      error = toMessage(reason);
    }
  }

  async function startImport(): Promise<void> {
    if (!ready || pending) return;
    const profileName = name.trim().length > 0 ? name.trim() : undefined;
    pending = true;
    cancelling = false;
    error = null;
    progress = null;
    importProfileId = null;
    existingProfiles = new Set(get(modProfiles(gameId)).data.map((profile) => profile.id));
    try {
      unlisten = await observeInstallProgress(track);
      const result = source === "file" && path !== null
        ? await importModProfileFile(gameId, path, profileName)
        : await importModProfileCode(gameId, normalizeProfileCode(code) ?? code, profileName);
      outcome = result;
      onImported(result.profile);
    } catch (reason) {
      if (!cancelling) error = toMessage(reason);
    } finally {
      stopListening();
      pending = false;
      cancelling = false;
    }
  }

  async function cancelImport(): Promise<void> {
    if (importProfileId === null || cancelling) return;
    cancelling = true;
    try {
      await cancelModInstall(importProfileId);
    } catch (reason) {
      cancelling = false;
      error = toMessage(reason);
    }
  }
</script>

<Dialog {open} title={t("Importa profilo", $language)} dismissible={!pending} {onClose}>
  {#snippet children(close)}
    {#if summary !== null && outcome !== null}
      <Callout
        tone="success"
        variant="inline"
        message={t("Profilo \"{0}\" importato: {1} mod installate.", $language, [outcome.profile.name, summary.installed])}
      />
      {#if summary.missingPackages.length > 0}
        <Callout
          tone="warning"
          title={t("Mod non trovate su Thunderstore", $language)}
          message={t("Queste mod non sono state installate perché non sono più disponibili.", $language)}
          items={summary.missingPackages}
        />
      {/if}
      {#if summary.skippedFiles.length > 0}
        <Callout
          tone="warning"
          title={t("File ignorati", $language)}
          message={t("Questi file del profilo non sono di un tipo consentito.", $language)}
          items={summary.skippedFiles}
        />
      {/if}
      {#if summary.warnings.length > 0}
        <Callout tone="warning" title={t("Avvisi", $language)} items={summary.warnings} />
      {/if}
      <div class="flex justify-end"><Button label={t("Chiudi", $language)} onClick={close} /></div>
    {:else if pending}
      <div class="flex flex-col gap-3" role="status" aria-live="polite">
        <div class="flex items-center gap-3 text-sm text-zinc-200 light:text-zinc-800">
          <Spinner />
          <span class="min-w-0 break-words">{cancelling ? t("Annullamento in corso...", $language) : view.label}</span>
        </div>
        {#if view.percent !== null}<ProgressBar value={view.percent} label={t("Avanzamento dell'importazione", $language)} tone="accent" />{/if}
      </div>
      <div class="flex justify-end">
        <Button label={t("Annulla importazione", $language)} variant="secondary" disabled={importProfileId === null || cancelling} onClick={() => void cancelImport()} />
      </div>
    {:else}
      <SegmentedControl name="mod-profile-import-source" label={t("Origine", $language)} options={sourceOptions} value={source} onChange={(value) => (source = value as ImportSource)} />
      {#if source === "file"}
        <div class="flex items-center gap-3">
          <p class="min-w-0 flex-1 truncate text-sm {path === null ? 'text-zinc-500' : 'text-zinc-100 light:text-zinc-900'}" title={path ?? undefined}>
            {path === null ? t("Nessun file selezionato", $language) : fileNameOf(path)}
          </p>
          <Button label={t("Scegli file...", $language)} variant="secondary" onClick={() => void chooseFile()} />
        </div>
      {:else}
        <div class="flex flex-col gap-1.5">
          <TextField id="mod-profile-import-code" label={t("Codice del profilo", $language)} placeholder="123e4567-e89b-12d3-a456-426614174000" bind:value={code} />
          {#if codeError}<p class="text-xs text-legio-error" role="alert">{codeError}</p>{/if}
        </div>
      {/if}
      <div class="flex flex-col gap-1.5">
        <TextField
          id="mod-profile-import-name"
          label={t("Nome del profilo (facoltativo)", $language)}
          placeholder={t("Usa il nome del profilo importato", $language)}
          hint={t("Massimo {0} caratteri.", $language, [maxProfileNameLength])}
          bind:value={name}
        />
        {#if nameError}<p class="text-xs text-legio-error" role="alert">{nameError}</p>{/if}
      </div>
      <Callout variant="inline" message={t("Serve il catalogo mod del gioco già scaricato. Le mod vengono installate nella versione indicata dal profilo.", $language)} />
      {#if error !== null}<ErrorBanner message={error} />{/if}
      <div class="flex justify-end gap-2">
        <Button label={t("Annulla", $language)} variant="secondary" onClick={close} />
        <Button label={t("Importa", $language)} disabled={!ready} onClick={() => void startImport()} />
      </div>
    {/if}
  {/snippet}
</Dialog>
