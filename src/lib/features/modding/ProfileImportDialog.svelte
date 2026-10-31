<script lang="ts">
  import { onDestroy, untrack } from "svelte";
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
  import SegmentedControl from "../../components/ui/SegmentedControl.svelte";
  import TextField from "../../components/ui/TextField.svelte";
  import InstallProgressCard from "./InstallProgressCard.svelte";
  import { canStartImport, fileNameOf, normalizeProfileCode, profileCodeError, summarizeImport, type ImportSource } from "./mod-profile-exchange-model";
  import { describeModdingError, maxProfileNameLength, profileNameError } from "./modding-model";

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
  let operationId: string | null = null;
  let unlisten: (() => void) | null = null;

  const sourceOptions = $derived([
    { value: "file", label: t("File .r2z", $language) },
    { value: "code", label: t("Codice", $language) },
  ]);
  const profiles = $derived(modProfiles(gameId));
  const codeError = $derived(profileCodeError(code, $language));
  // The name is optional: left empty, the backend keeps the imported profile's own name.
  const nameError = $derived(name.trim().length === 0 ? null : profileNameError(name, $profiles.data, $language));
  const ready = $derived(canStartImport(source, path, code, nameError));
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
    operationId = null;
  }

  function stopListening(): void {
    unlisten?.();
    unlisten = null;
  }

  function track(event: InstallProgress): void {
    if (operationId !== null && event.operationId === operationId) progress = event;
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
    const operation = crypto.randomUUID();
    operationId = operation;
    try {
      unlisten = await observeInstallProgress(track);
      const result = source === "file" && path !== null
        ? await importModProfileFile(gameId, path, operation, profileName)
        : await importModProfileCode(gameId, normalizeProfileCode(code) ?? code, operation, profileName);
      outcome = result;
      onImported(result.profile);
    } catch (reason) {
      if (!cancelling) error = describeModdingError(toMessage(reason), $language);
    } finally {
      stopListening();
      pending = false;
      cancelling = false;
    }
  }

  async function cancelImport(): Promise<void> {
    if (progress === null || cancelling) return;
    cancelling = true;
    try {
      await cancelModInstall(progress.profileId);
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
      <InstallProgressCard {progress} {cancelling} onCancel={() => void cancelImport()} />
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
        <TextField id="mod-profile-import-code" label={t("Codice del profilo", $language)} placeholder="123e4567-e89b-12d3-a456-426614174000" error={codeError} bind:value={code} />
      {/if}
      <TextField
        id="mod-profile-import-name"
        label={t("Nome del profilo (facoltativo)", $language)}
        placeholder={t("Usa il nome del profilo importato", $language)}
        hint={t("Al massimo {0} caratteri.", $language, [maxProfileNameLength])}
        error={nameError}
        bind:value={name}
      />
      <Callout variant="inline" message={t("Serve il catalogo mod del gioco già scaricato. Le mod vengono installate nella versione indicata dal profilo.", $language)} />
      {#if error !== null}<ErrorBanner message={error} />{/if}
      <div class="flex justify-end gap-2">
        <Button label={t("Annulla", $language)} variant="secondary" onClick={close} />
        <Button label={t("Importa", $language)} disabled={!ready} onClick={() => void startImport()} />
      </div>
    {/if}
  {/snippet}
</Dialog>
