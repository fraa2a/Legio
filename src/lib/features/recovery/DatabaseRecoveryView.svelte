<script lang="ts">
  import { onMount } from "svelte";
  import { t, language } from "../../i18n";
  import Button from "../../components/ui/Button.svelte";
  import ConfirmDialog from "../../components/ui/ConfirmDialog.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import Logo from "../../components/ui/Logo.svelte";
  import Panel from "../../components/ui/Panel.svelte";
  import SelectField from "../../components/ui/SelectField.svelte";
  import { databaseRecovery, loadDatabaseBackups, openDatabaseFolder, restoreSelectedBackup, selectDatabaseBackup, startWithNewDatabase } from "../../stores/database-recovery";
  import { showToast } from "../../stores/toast";
  import { formatDateTime } from "../../utils/format";

  let { failure }: { failure: string } = $props();
  let confirmingReset = $state(false);

  const busy = $derived($databaseRecovery.pending !== null);
  const backupOptions = $derived($databaseRecovery.backups.map((backup) => ({
    value: backup.fileName,
    label: t("{0} - schema {1}", $language, [formatDateTime(backup.createdAt / 1000, $language), backup.schemaVersion]),
  })));

  onMount(() => { void loadDatabaseBackups(); });

  async function copyFailure(): Promise<void> {
    try {
      await navigator.clipboard.writeText(failure);
      showToast(t("Copiato", $language), "success");
    } catch {
      showToast(t("Impossibile copiare negli appunti.", $language), "error");
    }
  }
</script>

<main class="legio-content min-h-0 flex-1 overflow-auto rounded-xl bg-black p-6 light:bg-zinc-50" aria-labelledby="database-recovery-title">
  <div class="mx-auto flex max-w-2xl flex-col gap-6">
    <div class="flex items-center gap-3 px-6 text-white light:text-zinc-900">
      <Logo size="size-9" />
      <span class="font-mono text-2xl font-bold tracking-[0.12em]">LEGIO</span>
    </div>
    <section class="flex flex-col gap-4 rounded-2xl legio-glass p-6">
      <header>
        <h1 id="database-recovery-title" class="text-xl font-semibold text-white light:text-zinc-900">{t("Impossibile aprire il database locale", $language)}</h1>
        <p class="mt-2 text-sm text-zinc-400 light:text-zinc-600">{t("Legio non riesce ad aprire o aggiornare il database della libreria. Nessun file è stato eliminato.", $language)}</p>
      </header>
      <pre class="max-h-40 overflow-auto whitespace-pre-wrap break-all rounded-lg bg-white/5 p-3 font-mono text-xs text-zinc-200 select-text light:bg-zinc-900/5 light:text-zinc-800">{failure}</pre>
      <div class="flex flex-wrap gap-2">
        <Button label={t("Copia", $language)} variant="secondary" size="sm" onClick={() => void copyFailure()} />
        <Button label={t("Apri cartella dati", $language)} variant="secondary" size="sm" onClick={() => void openDatabaseFolder()} />
      </div>
      {#if $databaseRecovery.error}<ErrorBanner message={$databaseRecovery.error} />{/if}
    </section>
    <Panel title={t("Ripristina un backup", $language)} description={t("Legio salva una copia del database prima di ogni aggiornamento. Il database attuale viene conservato con un nuovo nome e Legio si riavvia.", $language)}>
      {#if !$databaseRecovery.backupsLoaded}
        <p class="text-sm text-zinc-400 light:text-zinc-600" role="status">{t("Caricamento...", $language)}</p>
      {:else if backupOptions.length === 0}
        <p class="text-sm text-zinc-400 light:text-zinc-600">{t("Nessun backup disponibile.", $language)}</p>
      {:else}
        <SelectField id="database-backup" label={t("Backup", $language)} value={$databaseRecovery.selected} options={backupOptions} disabled={busy} onChange={selectDatabaseBackup} />
        <div>
          <Button label={$databaseRecovery.pending === "restore" ? t("Ripristino...", $language) : t("Ripristina backup", $language)} disabled={busy} onClick={() => void restoreSelectedBackup()} />
        </div>
      {/if}
    </Panel>
    <Panel title={t("Ricomincia con un database nuovo", $language)} description={t("La libreria riparte vuota. Il database attuale viene conservato con un nuovo nome nella cartella dati.", $language)}>
      <div>
        <Button label={$databaseRecovery.pending === "reset" ? t("Riavvio...", $language) : t("Usa un database nuovo", $language)} variant="danger-soft" disabled={busy} onClick={() => (confirmingReset = true)} />
      </div>
    </Panel>
  </div>
</main>

<ConfirmDialog
  open={confirmingReset}
  title={t("Ricominciare con un database nuovo?", $language)}
  message={t("Legio si riavvierà con una libreria vuota. Il database attuale non viene eliminato e potrai recuperarlo dalla cartella dati.", $language)}
  cancelLabel={t("Indietro", $language)}
  confirmLabel={t("Usa un database nuovo", $language)}
  tone="danger"
  onConfirm={() => { confirmingReset = false; void startWithNewDatabase(); }}
  onClose={() => (confirmingReset = false)}
/>
