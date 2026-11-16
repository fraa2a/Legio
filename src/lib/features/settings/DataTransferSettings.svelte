<script lang="ts">
  import { t, language } from "../../i18n";
  import Button from "../../components/ui/Button.svelte";
  import ConfirmDialog from "../../components/ui/ConfirmDialog.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import SettingsGroup from "../../components/ui/SettingsGroup.svelte";
  import SettingsRow from "../../components/ui/SettingsRow.svelte";
  import { cancelImport, chooseImportFile, confirmImport, dataTransfer, exportData } from "../../stores/data-transfer";
  import { showToast } from "../../stores/toast";

  const busy = $derived($dataTransfer.pending !== null);

  async function runExport(): Promise<void> {
    if (await exportData()) showToast(t("Dati esportati", $language), "success");
  }
</script>

<SettingsGroup icon="package" title={t("Dati", $language)}>
  <SettingsRow label={t("Esporta i dati", $language)} description={t("Salva libreria, tempi di gioco, impostazioni e immagini personalizzate in un unico file, come backup o per spostarli su un altro PC.", $language)}>
    <Button label={t("Esporta", $language)} variant="secondary" disabled={busy} onClick={() => void runExport()} />
  </SettingsRow>
  <SettingsRow label={t("Importa i dati", $language)} description={t("Sostituisce i dati attuali con quelli di un file esportato da Legio.", $language)}>
    <Button label={t("Importa", $language)} variant="secondary" disabled={busy} onClick={() => void chooseImportFile()} />
  </SettingsRow>
  {#if $dataTransfer.error !== null && $dataTransfer.importPath === null}<ErrorBanner message={$dataTransfer.error} />{/if}
</SettingsGroup>

<ConfirmDialog
  open={$dataTransfer.importPath !== null}
  title={t("Importa i dati", $language)}
  message={t("Libreria, tempi di gioco e impostazioni attuali verranno sostituiti da quelli del file. I dati attuali restano in una copia rinominata nella cartella dei dati. Legio si riavvierà per completare l'importazione.", $language)}
  confirmLabel={t("Importa e riavvia", $language)}
  tone="danger"
  pending={$dataTransfer.pending === "import"}
  error={$dataTransfer.error}
  onConfirm={() => void confirmImport()}
  onClose={cancelImport}
>
  <p class="text-xs text-zinc-400 light:text-zinc-600">{t("Le cartelle di installazione di un altro PC potrebbero non esistere qui: i giochi interessati andranno reinstallati o ricollegati.", $language)}</p>
</ConfirmDialog>
