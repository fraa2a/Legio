<script lang="ts">
  import { createSaveQueue } from "../../services/save-queue";
  import { t, language } from "../../i18n";
  import { onDestroy } from "svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import SettingsGroup from "../../components/ui/SettingsGroup.svelte";
  import Slider from "../../components/ui/Slider.svelte";
  import ResetSetting from "../../components/ui/ResetSetting.svelte";
  import ToggleSetting from "../../components/ui/ToggleSetting.svelte";
  import { defaultSettings, settings, updateSettings as saveChanges } from "../../stores/settings";
  import { toMessage } from "../../utils/errors";
  import {
    bandwidthLimit,
    bandwidthLimitError,
    saveBandwidthLimit,
  } from "../../stores/downloads";
  import { formatMegabytes, nearestStop, stopBytes, unlimitedStop } from "./bandwidth-stops";

  let limitDraft = $state<number | null>(null);
  let savingLimit = $state(false);
  let failedLimit: number | null = null;
  const saveQueue = createSaveQueue();

  const draftBytes = $derived(limitDraft === null ? $bandwidthLimit.data : stopBytes(limitDraft));
  const draftChanged = $derived(limitDraft !== null && draftBytes !== $bandwidthLimit.data);
  const thumbBytes = $derived(stopBytes(limitDraft ?? nearestStop($bandwidthLimit.data)));
  const limitLabel = $derived(thumbBytes === 0 ? t("Senza limite", $language) : t("{0} MB/s", $language, [formatMegabytes(thumbBytes)]));

  $effect(() => {
    if (!draftChanged || savingLimit || draftBytes === failedLimit) return;
    const timer = setTimeout(() => void applyLimit(), 500);
    return () => clearTimeout(timer);
  });

  onDestroy(() => {
    const bytes = draftBytes;
    const changed = draftChanged;
    void saveQueue.flush(async () => {
      if (changed && bytes !== failedLimit && bytes !== $bandwidthLimit.data) await applyLimit(bytes);
    });
  });

  function applyLimit(bytes = draftBytes): Promise<void> {
    return saveQueue.run(async () => {
      savingLimit = true;
      try {
        await saveBandwidthLimit(bytes);
        if ($bandwidthLimitError === null) {
          if (draftBytes === bytes) limitDraft = null;
          failedLimit = null;
        } else failedLimit = bytes;
      } finally {
        savingLimit = false;
      }
    });
  }

  let settingsError = $state<string | null>(null);

  async function updateSettings(changes: Partial<typeof $settings.data>): Promise<void> {
    settingsError = null;
    try {
      await saveChanges(changes);
    } catch (error) {
      settingsError = toMessage(error);
    }
  }
</script>

<section class="flex flex-col gap-6">
  {#if settingsError !== null}<ErrorBanner message={t("Impossibile salvare le impostazioni.", $language)} detail={settingsError} />{/if}
  {#if $bandwidthLimit.status === "error"}<ErrorBanner message={t("Impossibile leggere il limite di banda: {0}", $language, [$bandwidthLimit.error])} onRetry={() => void bandwidthLimit.load()} />{/if}
  {#if $bandwidthLimitError}<ErrorBanner message={$bandwidthLimitError} />{/if}



  <SettingsGroup icon="speed" title={t("Velocità di download", $language)}>
    <div class="flex max-w-md flex-col gap-2">
      <div class="flex items-center justify-between gap-2">
        <label for="download-bandwidth-limit" class="text-sm font-medium text-legio-fg">{t("Limite di banda", $language)}</label>
        <div class="flex shrink-0 items-center gap-1">
          <output for="download-bandwidth-limit" class="text-xs tabular-nums text-legio-fg-muted">{limitLabel}</output>
          <ResetSetting label={t("Ripristina limite di banda", $language)} disabled={draftBytes === 0} onClick={() => { limitDraft = unlimitedStop; }} />
        </div>
      </div>
      <Slider id="download-bandwidth-limit" label={t("Limite di banda", $language)} value={limitDraft ?? nearestStop($bandwidthLimit.data)} min={0} max={unlimitedStop} valueText={limitLabel}
        disabled={$bandwidthLimit.status === "error"} class="w-full" onInput={(value) => { limitDraft = value; }} />
      <p class="text-xs text-legio-fg-muted">{t("Il limite viene applicato subito e riportato al prossimo avvio.", $language)}</p>
    </div>
  </SettingsGroup>

  <SettingsGroup icon="integrity" title={t("Integrità dei download", $language)}>
    <ToggleSetting label={t("Verifica SHA-256 dei giochi verified (consigliato)", $language)} checked={$settings.data.verifyVerifiedDownloads} defaultValue={defaultSettings.verifyVerifiedDownloads} disabled={$settings.status !== "ready"} onChange={(checked) => void updateSettings({ verifyVerifiedDownloads: checked })} />
  </SettingsGroup>

  <SettingsGroup icon="bell" title={t("Notifiche dei download", $language)}>
    <ToggleSetting label={t("Notifica di sistema al termine del download", $language)} checked={$settings.data.downloadNotifications} defaultValue={defaultSettings.downloadNotifications} onChange={(checked) => void updateSettings({ downloadNotifications: checked })} />
  </SettingsGroup>

  <SettingsGroup icon="gamepad" title={t("Download e aggiornamenti durante il gioco", $language)}>
    <ToggleSetting label={t("Consenti download mentre un gioco è aperto", $language)} checked={$settings.data.downloadWhilePlaying} defaultValue={defaultSettings.downloadWhilePlaying} onChange={checked => void updateSettings({ downloadWhilePlaying: checked })} />
    <ToggleSetting label={t("Posticipa l’estrazione automatica dei download mentre giochi", $language)} checked={$settings.data.deferExtractionWhilePlaying} defaultValue={defaultSettings.deferExtractionWhilePlaying} onChange={checked => void updateSettings({ deferExtractionWhilePlaying: checked })} />
    <ToggleSetting label={t("Aggiornamenti automatici dei giochi", $language)} checked={$settings.data.automaticGameUpdates} defaultValue={defaultSettings.automaticGameUpdates} onChange={checked => void updateSettings({ automaticGameUpdates: checked })} />
  </SettingsGroup>
</section>
