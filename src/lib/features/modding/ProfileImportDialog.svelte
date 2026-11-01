<script lang="ts">
  import { onDestroy, untrack } from "svelte";
  import { t, language } from "../../i18n";
  import { pickModProfileFile } from "../../services/dialog";
  import { importModProfileCode, importModProfileFile, type ProfileImportOutcome } from "../../services/mod-profile-exchange";
  import { cancelModInstall, observeCatalogProgress, observeInstallProgress, type CatalogProgress, type InstallProgress, type ModProfile } from "../../services/modding";
  import { moddableGameById, modProfiles, refreshCatalog } from "../../stores/modding";
  import { toMessage } from "../../utils/errors";
  import Button from "../../components/ui/Button.svelte";
  import Callout from "../../components/ui/Callout.svelte";
  import Dialog from "../../components/ui/Dialog.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import ProgressBar from "../../components/ui/ProgressBar.svelte";
  import SegmentedControl from "../../components/ui/SegmentedControl.svelte";
  import SelectField from "../../components/ui/SelectField.svelte";
  import Spinner from "../../components/ui/Spinner.svelte";
  import TextField from "../../components/ui/TextField.svelte";
  import InstallProgressCard from "./InstallProgressCard.svelte";
  import { canStartImport, defaultTargetProfileId, fileNameOf, importTargetProfileId, normalizeProfileCode, profileCodeError, summarizeImport, type ImportSource, type ImportTarget } from "./mod-profile-exchange-model";
  import { catalogPercent, describeModdingError, maxProfileNameLength, profileNameError } from "./modding-model";

  let { open, gameId, activeProfileId, onClose, onImported }: {
    open: boolean;
    gameId: string;
    activeProfileId: string | null;
    onClose: () => void;
    onImported: (profile: ModProfile) => void;
  } = $props();

  let source = $state<ImportSource>("file");
  let path = $state<string | null>(null);
  let code = $state("");
  let name = $state("");
  let target = $state<ImportTarget>("new");
  let targetProfileId = $state<string | null>(null);
  let pending = $state(false);
  let cancelling = $state(false);
  let error = $state<string | null>(null);
  let outcome = $state<ProfileImportOutcome | null>(null);
  let progress = $state<InstallProgress | null>(null);
  let catalogProgress = $state<CatalogProgress | null>(null);
  let updated = $state(false);
  let operationId: string | null = null;
  let unlisteners: (() => void)[] = [];

  const sourceOptions = $derived([
    { value: "file", label: t("File .r2z", $language) },
    { value: "code", label: t("Codice", $language) },
  ]);
  const profiles = $derived(modProfiles(gameId));
  const community = $derived($moddableGameById.get(gameId)?.binding?.ecosystemGameId ?? null);
  const targetOptions = $derived([
    { value: "new", label: t("Nuovo profilo", $language) },
    ...($profiles.data.length > 0 ? [{ value: "existing", label: t("Aggiorna un profilo esistente", $language) }] : []),
  ]);
  const profileChoices = $derived($profiles.data.map((profile) => ({ value: profile.id, label: profile.name })));
  const codeError = $derived(profileCodeError(code, $language));
  // The name is optional: left empty, the backend keeps the imported profile's own name.
  const nameError = $derived(name.trim().length === 0 ? null : profileNameError(name, $profiles.data, $language));
  const ready = $derived(canStartImport(source, path, code, nameError, target, targetProfileId));
  const summary = $derived(outcome === null ? null : summarizeImport(outcome));
  // The catalog is refreshed before any install starts, so its progress shows only until the first install event.
  const refreshingCatalog = $derived(catalogProgress !== null && progress === null);

  $effect(() => {
    if (open) untrack(reset);
  });

  onDestroy(() => stopListening());

  function reset(): void {
    source = "file";
    path = null;
    code = "";
    name = "";
    target = "new";
    targetProfileId = defaultTargetProfileId($profiles.data, activeProfileId);
    pending = false;
    cancelling = false;
    error = null;
    outcome = null;
    progress = null;
    catalogProgress = null;
    updated = false;
    operationId = null;
  }

  function stopListening(): void {
    for (const unlisten of unlisteners) unlisten();
    unlisteners = [];
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
    const profileName = target === "new" && name.trim().length > 0 ? name.trim() : undefined;
    const targetId = importTargetProfileId(target, targetProfileId);
    pending = true;
    cancelling = false;
    error = null;
    progress = null;
    catalogProgress = null;
    updated = targetId !== undefined;
    const operation = crypto.randomUUID();
    operationId = operation;
    try {
      unlisteners.push(await observeInstallProgress(track));
      unlisteners.push(await observeCatalogProgress((event) => { if (event.community === community) catalogProgress = event; }));
      const result = source === "file" && path !== null
        ? await importModProfileFile(gameId, path, operation, profileName, targetId)
        : await importModProfileCode(gameId, normalizeProfileCode(code) ?? code, operation, profileName, targetId);
      outcome = result;
      onImported(result.profile);
      // The import may have rebuilt the catalog; this only reads its new status.
      if (community !== null) void refreshCatalog(community, false);
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
        message={updated
          ? t("Profilo \"{0}\" aggiornato: {1} mod installate.", $language, [outcome.profile.name, summary.installed])
          : t("Profilo \"{0}\" importato: {1} mod installate.", $language, [outcome.profile.name, summary.installed])}
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
    {:else if pending && refreshingCatalog && catalogProgress !== null}
      <section class="flex flex-wrap items-center gap-4 rounded-2xl legio-glass p-4" aria-label={t("Aggiornamento del catalogo mod", $language)}>
        <Spinner />
        <div class="min-w-0 flex-1 basis-56" role="status" aria-live="polite">
          <p class="truncate text-sm font-medium text-zinc-100 light:text-zinc-900">{t("Aggiornamento del catalogo mod...", $language)}</p>
          <ProgressBar value={catalogPercent(catalogProgress.chunksDone, catalogProgress.chunksTotal)} tone="accent" label={t("Avanzamento del catalogo", $language)} class="mt-2 h-1.5" />
        </div>
      </section>
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
      <SelectField id="mod-profile-import-target" label={t("Destinazione", $language)} options={targetOptions} value={target} onChange={(value) => (target = value as ImportTarget)} />
      {#if target === "existing"}
        <SelectField
          id="mod-profile-import-target-profile"
          label={t("Profilo da aggiornare", $language)}
          options={profileChoices}
          value={targetProfileId ?? ""}
          onChange={(value) => (targetProfileId = value)}
        />
        <Callout tone="warning" variant="inline" message={t("Le mod attuali del profilo verranno sostituite da quelle importate, comprese le mod locali.", $language)} />
      {:else}
        <TextField
          id="mod-profile-import-name"
          label={t("Nome del profilo (facoltativo)", $language)}
          placeholder={t("Usa il nome del profilo importato", $language)}
          hint={t("Al massimo {0} caratteri.", $language, [maxProfileNameLength])}
          error={nameError}
          bind:value={name}
        />
      {/if}
      <Callout variant="inline" message={t("Le mod vengono installate nella versione indicata dal profilo. Se il catalogo mod del gioco manca o è incompleto, viene scaricato prima dell'import.", $language)} />
      {#if error !== null}<ErrorBanner message={error} />{/if}
      <div class="flex justify-end gap-2">
        <Button label={t("Annulla", $language)} variant="secondary" onClick={close} />
        <Button label={t("Importa", $language)} disabled={!ready} onClick={() => void startImport()} />
      </div>
    {/if}
  {/snippet}
</Dialog>
