<script lang="ts">
  import { onMount } from "svelte";
  import { t, language } from "../../i18n";
  import { getNetworkLogStatus, type NetworkLogStatus } from "../../services/network";
  import { settings, defaultSettings, updateSettings } from "../../stores/settings";
  import { toMessage } from "../../utils/errors";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import ResetSetting from "../../components/ui/ResetSetting.svelte";
  import SettingsGroup from "../../components/ui/SettingsGroup.svelte";
  import Toggle from "../../components/ui/Toggle.svelte";
  import { appInfo } from "../../stores/app-info";
  import { getCompatibilityLogsDirectory } from "../../services/game-settings";

  let status = $state<NetworkLogStatus | null>(null);
  let error = $state<string | null>(null);
  let saving = $state(false);
  let launchDirectory = $state<string | null>(null);

  onMount(() => { void refresh(); });

  async function refresh(): Promise<void> {
    try {
      status = await getNetworkLogStatus();
      if ($appInfo.data.platform === "linux") launchDirectory = await getCompatibilityLogsDirectory();
      error = null;
    } catch (cause) {
      error = toMessage(cause);
    }
  }

  async function setLogging(enabled: boolean): Promise<void> {
    saving = true;
    error = null;
    try {
      await updateSettings({ diagnosticsEnabled: enabled });
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
        <p class="mt-2 text-xs text-zinc-400 light:text-zinc-600">{t("Registro degli avvii: game-launches.jsonl. Include fasi, errori e codici di uscita senza argomenti o valori delle variabili d'ambiente.", $language)}</p>
      </div>
    {/if}
    {#if launchDirectory}
      <p class="text-xs text-zinc-400 light:text-zinc-600">{t("Ogni gioco manuale salva launch.json, launch.log, stdout.log, stderr.log e runner-exit-code.txt nella propria cartella. Il debug aggiunge i dettagli di Proton o Wine.", $language)}</p>
      <p class="break-all font-mono text-xs text-zinc-300 light:text-zinc-700">{launchDirectory}</p>
    {/if}
    {#if status?.lastError}<p class="text-xs text-amber-300 light:text-amber-800" role="status">{status.lastError}</p>{/if}
  </SettingsGroup>
</section>
