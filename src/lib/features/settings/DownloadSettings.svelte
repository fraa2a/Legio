<script lang="ts">
  import { t, language } from "../../i18n";
  import { onDestroy } from "svelte";
  import Button from "../../components/ui/Button.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import SettingsGroup from "../../components/ui/SettingsGroup.svelte";
  import SettingsRow from "../../components/ui/SettingsRow.svelte";
  import TextField from "../../components/ui/TextField.svelte";
  import ResetSetting from "../../components/ui/ResetSetting.svelte";
  import Toggle from "../../components/ui/Toggle.svelte";
  import { pickGameDirectory } from "../../services/dialog";
  import { saveSettings } from "../../services/local-state";
  import { defaultSettings, settings } from "../../stores/settings";
  import { toMessage } from "../../utils/errors";
  import {
    bandwidthLimit,
    bandwidthLimitError,
    browseInstalledFolder,
    installedFolder,
    installedFolderError,
    saveBandwidthLimit,
  } from "../../stores/downloads";

  const bytesPerMegabyte = 1024 * 1024;

  let limitDraft = $state<string | null>(null);
  let savingLimit = $state(false);
  let failedLimit: string | null = null;

  const savedMegabytes = $derived(
    ($bandwidthLimit.data / bytesPerMegabyte).toFixed(2).replace(/\.?0+$/, ""),
  );

  const draftMegabytes = $derived(limitDraft ?? savedMegabytes);
  const draftBytes = $derived(Math.round(Number(draftMegabytes) * bytesPerMegabyte));
  const draftChanged = $derived(
    draftMegabytes.trim().length > 0 &&
      Number.isFinite(Number(draftMegabytes)) &&
      draftBytes !== $bandwidthLimit.data,
  );

  const limitDescription = $derived(
    $bandwidthLimit.data === 0
      ? t("Nessun limite di banda: i download usano tutta la connessione disponibile.", $language)
      : t("Limite attuale: {0} MB/s", $language, [savedMegabytes]),
  );

  $effect(() => {
    if (!draftChanged || savingLimit || draftMegabytes === failedLimit) return;
    const timer = setTimeout(() => void applyLimit(), 500);
    return () => clearTimeout(timer);
  });

  onDestroy(() => { if (draftChanged && !savingLimit && draftMegabytes !== failedLimit) void applyLimit(); });

  async function applyLimit(): Promise<void> {
    const value = draftMegabytes;
    savingLimit = true;
    await saveBandwidthLimit(draftBytes);
    if ($bandwidthLimitError === null) { limitDraft = null; failedLimit = null; }
    else failedLimit = value;
    savingLimit = false;
  }

  let savingSettings = $state(false);
  let settingsError = $state<string | null>(null);

  async function updateSettings(changes: Partial<typeof $settings.data>): Promise<void> {
    savingSettings = true;
    settingsError = null;
    try {
      settings.set(await saveSettings({ ...$settings.data, ...changes }));
    } catch (error) {
      settingsError = toMessage(error);
    } finally {
      savingSettings = false;
    }
  }

  async function setDownloadPath(): Promise<void> {
    try {
      const path = await pickGameDirectory($settings.data.downloadPath, t("Imposta la cartella per download e installazioni", $language));
      if (path !== null) await updateSettings({ downloadPath: path });
    } catch (error) {
      settingsError = toMessage(error);
    }
  }

  let openingFolder = $state(false);

  async function browseFolder(): Promise<void> {
    openingFolder = true;
    await browseInstalledFolder();
    openingFolder = false;
  }
</script>

<section class="flex flex-col gap-4">
  {#if settingsError !== null}<ErrorBanner message={settingsError} />{/if}
  {#if $installedFolderError}<ErrorBanner message={$installedFolderError} />{/if}
  {#if $bandwidthLimit.status === "error"}<ErrorBanner message={t("Impossibile leggere il limite di banda: {0}", $language, [$bandwidthLimit.error])} onRetry={() => void bandwidthLimit.load()} />{/if}
  {#if $bandwidthLimitError}<ErrorBanner message={$bandwidthLimitError} />{/if}

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

  <SettingsGroup icon="download" title={t("Velocità di download", $language)} description={limitDescription}>
    <div class="flex flex-wrap items-end gap-3">
      <div class="w-56"><TextField id="download-bandwidth-limit" label={t("Limite di banda (MB/s)", $language)} type="number" inputmode="numeric" value={draftMegabytes} placeholder="0" hint={t("0 significa senza limite. Il limite viene applicato subito e riportato al prossimo avvio.", $language)} disabled={savingLimit || $bandwidthLimit.status === "error"} oninput={(value) => (limitDraft = value)} /></div>
      <ResetSetting label={t("Ripristina limite di banda", $language)} disabled={savingLimit || ($bandwidthLimit.data === 0 && limitDraft === null)} onClick={() => { limitDraft = "0"; }} />
    </div>
  </SettingsGroup>

  <SettingsGroup icon="check" title={t("Integrità dei download", $language)} description={t("Senza verifica SHA-256, Legio non può rilevare archivi corrotti o alterati. La modifica si applica ai prossimi download.", $language)}>
    <div class="flex items-center gap-2"><div class="min-w-0 flex-1"><Toggle label={t("Verifica SHA-256 dei giochi verified (consigliato)", $language)} checked={$settings.data.verifyVerifiedDownloads} disabled={savingSettings || $settings.status !== "ready"} onChange={(checked) => void updateSettings({ verifyVerifiedDownloads: checked })} /></div><ResetSetting label={t("Ripristina verifica dei download", $language)} disabled={savingSettings || $settings.data.verifyVerifiedDownloads === defaultSettings.verifyVerifiedDownloads} onClick={() => void updateSettings({ verifyVerifiedDownloads: defaultSettings.verifyVerifiedDownloads })} /></div>
  </SettingsGroup>

  <SettingsGroup icon="info" title={t("Notifiche dei download", $language)}>
    <div class="flex items-center gap-2"><div class="min-w-0 flex-1"><Toggle label={t("Notifica di sistema al termine del download", $language)} checked={$settings.data.downloadNotifications} disabled={savingSettings} onChange={(checked) => void updateSettings({ downloadNotifications: checked })} /></div><ResetSetting label={t("Ripristina notifiche dei download", $language)} disabled={savingSettings || $settings.data.downloadNotifications === defaultSettings.downloadNotifications} onClick={() => void updateSettings({ downloadNotifications: defaultSettings.downloadNotifications })} /></div>
  </SettingsGroup>
</section>
