<script lang="ts">
  import { t, language } from "../../i18n";
  import { onDestroy } from "svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import SettingsGroup from "../../components/ui/SettingsGroup.svelte";
  import TextField from "../../components/ui/TextField.svelte";
  import ResetSetting from "../../components/ui/ResetSetting.svelte";
  import ToggleSetting from "../../components/ui/ToggleSetting.svelte";
  import { defaultSettings, settings, updateSettings as saveChanges } from "../../stores/settings";
  import { toMessage } from "../../utils/errors";
  import {
    bandwidthLimit,
    bandwidthLimitError,
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
    limitDraft !== null &&
    draftMegabytes.trim().length > 0 &&
      Number.isFinite(Number(draftMegabytes)) &&
      draftBytes !== $bandwidthLimit.data,
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
      await saveChanges(changes);
    } catch (error) {
      settingsError = toMessage(error);
    } finally {
      savingSettings = false;
    }
  }

</script>

<section class="flex flex-col gap-4">
  {#if settingsError !== null}<ErrorBanner message={settingsError} />{/if}
  {#if $bandwidthLimit.status === "error"}<ErrorBanner message={t("Impossibile leggere il limite di banda: {0}", $language, [$bandwidthLimit.error])} onRetry={() => void bandwidthLimit.load()} />{/if}
  {#if $bandwidthLimitError}<ErrorBanner message={$bandwidthLimitError} />{/if}



  <SettingsGroup icon="download" title={t("Velocità di download", $language)}>
    <div class="flex flex-wrap items-end gap-3">
      <div class="w-56"><TextField id="download-bandwidth-limit" label={t("Limite di banda (MB/s)", $language)} type="number" inputmode="numeric" value={draftMegabytes} placeholder="0" hint={t("0 significa senza limite. Il limite viene applicato subito e riportato al prossimo avvio.", $language)} disabled={savingLimit || $bandwidthLimit.status === "error"} oninput={(value) => (limitDraft = value)} /></div>
      <ResetSetting label={t("Ripristina limite di banda", $language)} disabled={savingLimit || ($bandwidthLimit.data === 0 && limitDraft === null)} onClick={() => { limitDraft = "0"; }} />
    </div>
  </SettingsGroup>

  <SettingsGroup icon="check" title={t("Integrità dei download", $language)}>
    <ToggleSetting label={t("Verifica SHA-256 dei giochi verified (consigliato)", $language)} checked={$settings.data.verifyVerifiedDownloads} defaultValue={defaultSettings.verifyVerifiedDownloads} disabled={savingSettings || $settings.status !== "ready"} onChange={(checked) => void updateSettings({ verifyVerifiedDownloads: checked })} />
  </SettingsGroup>

  <SettingsGroup icon="info" title={t("Notifiche dei download", $language)}>
    <ToggleSetting label={t("Notifica di sistema al termine del download", $language)} checked={$settings.data.downloadNotifications} defaultValue={defaultSettings.downloadNotifications} disabled={savingSettings} onChange={(checked) => void updateSettings({ downloadNotifications: checked })} />
  </SettingsGroup>

  <SettingsGroup icon="downloads" title={t("Download e aggiornamenti durante il gioco", $language)}>
    <ToggleSetting label={t("Consenti download mentre un gioco è aperto", $language)} checked={$settings.data.downloadWhilePlaying} defaultValue={defaultSettings.downloadWhilePlaying} disabled={savingSettings} onChange={checked => void updateSettings({ downloadWhilePlaying: checked })} />
    <ToggleSetting label={t("Posticipa l’estrazione automatica dei download mentre giochi", $language)} checked={$settings.data.deferExtractionWhilePlaying} defaultValue={defaultSettings.deferExtractionWhilePlaying} disabled={savingSettings} onChange={checked => void updateSettings({ deferExtractionWhilePlaying: checked })} />
    <ToggleSetting label={t("Aggiornamenti automatici dei giochi", $language)} checked={$settings.data.automaticGameUpdates} defaultValue={defaultSettings.automaticGameUpdates} disabled={savingSettings} onChange={checked => void updateSettings({ automaticGameUpdates: checked })} />
  </SettingsGroup>
</section>
