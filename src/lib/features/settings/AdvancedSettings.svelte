<script lang="ts">
  import { onMount } from "svelte";
  import { t, language } from "../../i18n";
  import { saveSettings } from "../../services/local-state";
  import { getNetworkLogStatus, type NetworkLogStatus } from "../../services/network";
  import { settings, defaultSettings } from "../../stores/settings";
  import { toMessage } from "../../utils/errors";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import ResetSetting from "../../components/ui/ResetSetting.svelte";
  import SettingsGroup from "../../components/ui/SettingsGroup.svelte";
  import Toggle from "../../components/ui/Toggle.svelte";

  let status = $state<NetworkLogStatus | null>(null);
  let error = $state<string | null>(null);
  let saving = $state(false);

  onMount(() => { void refresh(); });

  async function refresh(): Promise<void> {
    try {
      status = await getNetworkLogStatus();
      error = null;
    } catch (cause) {
      error = toMessage(cause);
    }
  }

  async function setLogging(enabled: boolean): Promise<void> {
    saving = true;
    error = null;
    try {
      settings.set(await saveSettings({ ...$settings.data, diagnosticsEnabled: enabled }));
      await refresh();
    } catch (cause) {
      error = toMessage(cause);
    } finally {
      saving = false;
    }
  }
</script>

<section class="flex flex-col gap-4">
  {#if error !== null}<ErrorBanner message={error} onRetry={() => void refresh()} />{/if}
  <SettingsGroup icon="info" title={t("Log e diagnostica", $language)}>
    <div class="flex items-center gap-2"><div class="min-w-0 flex-1"><Toggle label={t("Salva log di rete", $language)} checked={$settings.data.diagnosticsEnabled} disabled={saving} onChange={(enabled) => void setLogging(enabled)} /></div><ResetSetting label={t("Ripristina log di rete", $language)} disabled={saving || $settings.data.diagnosticsEnabled === defaultSettings.diagnosticsEnabled} onClick={() => void setLogging(defaultSettings.diagnosticsEnabled)} /></div>
    {#if status?.directory}
      <div class="pt-3">
        <p class="text-xs text-zinc-500 light:text-zinc-600">{t("Cartella dei log", $language)}</p>
        <p class="mt-1 break-all font-mono text-xs text-zinc-300 light:text-zinc-700">{status.directory}</p>
      </div>
    {/if}
    {#if status?.lastError}<p class="text-xs text-amber-300 light:text-amber-800" role="status">{status.lastError}</p>{/if}
  </SettingsGroup>
</section>
