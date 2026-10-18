<script lang="ts">
  import { t, language } from "../../i18n";
  import LibrarySettings from "./LibrarySettings.svelte";
  import SourceSettings from "./SourceSettings.svelte";
  import Button from "../../components/ui/Button.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import SettingsGroup from "../../components/ui/SettingsGroup.svelte";
  import SettingsRow from "../../components/ui/SettingsRow.svelte";
  import { pickGameDirectory } from "../../services/dialog";
  import { settings, updateSettings as saveChanges } from "../../stores/settings";
  import { browseInstalledFolder, installedFolder, installedFolderError } from "../../stores/downloads";
  import { toMessage } from "../../utils/errors";
  let savingSettings = $state(false);
  let openingFolder = $state(false);
  let settingsError = $state<string | null>(null);
  async function updateSettings(changes: Partial<typeof $settings.data>): Promise<void> {
    savingSettings = true; settingsError = null;
    try { await saveChanges(changes); }
    catch(error) { settingsError = toMessage(error); }
    finally { savingSettings = false; }
  }
  async function setDownloadPath(): Promise<void> {
    try {
      const path = await pickGameDirectory($settings.data.downloadPath, t("Imposta la cartella per download e installazioni", $language));
      if(path !== null) await updateSettings({ downloadPath: path });
    } catch(error) { settingsError = toMessage(error); }
  }
  async function browseFolder(): Promise<void> {
    openingFolder = true;
    try { await browseInstalledFolder(); } finally { openingFolder = false; }
  }
</script>
<section class="flex flex-col gap-4">
  {#if settingsError}<ErrorBanner message={settingsError} />{/if}
  {#if $installedFolderError}<ErrorBanner message={$installedFolderError} />{/if}
  <SettingsGroup icon="folder" title={t("Cartelle", $language)}>
    <SettingsRow label={t("Cartella per download e installazioni", $language)} description={$settings.data.downloadPath ?? t("Cartella predefinita di Legio", $language)}>
      <div class="flex flex-wrap gap-2">
        <Button label={t("Imposta cartella...", $language)} variant="secondary" disabled={savingSettings} onClick={() => void setDownloadPath()} />
        {#if $settings.data.downloadPath !== null}<Button label={t("Usa cartella predefinita", $language)} variant="secondary" disabled={savingSettings} onClick={() => void updateSettings({ downloadPath: null })} />{/if}
      </div>
    </SettingsRow>
    <SettingsRow label={t("Cartella dei giochi installati", $language)} description={$installedFolder.data?.directory ?? undefined}>
      <Button label={t("Apri cartella", $language)} variant="secondary" disabled={openingFolder} onClick={() => void browseFolder()} />
    </SettingsRow>
  </SettingsGroup>
  <LibrarySettings />
  <SourceSettings />
</section>
