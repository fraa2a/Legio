<script lang="ts">
  import { onMount } from "svelte";
  import { t, language } from "../../i18n";
  import { getNetworkLogStatus, type NetworkLogStatus } from "../../services/network";
  import { getApplicationLogStatus, type ApplicationLogStatus } from "../../services/application-log";
  import { settings, defaultSettings, updateSettings } from "../../stores/settings";
  import { toMessage } from "../../utils/errors";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import ResetSetting from "../../components/ui/ResetSetting.svelte";
  import SettingsGroup from "../../components/ui/SettingsGroup.svelte";
  import Toggle from "../../components/ui/Toggle.svelte";
  import { appInfo } from "../../stores/app-info";
  import { getCompatibilityLogsDirectory } from "../../services/game-settings";

  let status = $state<NetworkLogStatus | null>(null);
  let applicationStatus = $state<ApplicationLogStatus | null>(null);
  let error = $state<string | null>(null);
  let saving = $state(false);
  let launchDirectory = $state<string | null>(null);

  onMount(() => { void refresh(); });

  async function refresh(): Promise<void> {
    try {
      [status, applicationStatus] = await Promise.all([getNetworkLogStatus(), getApplicationLogStatus()]);
      if ($appInfo.data.platform === "linux") launchDirectory = await getCompatibilityLogsDirectory();
      error = null;
    } catch (cause) {
      error = toMessage(cause);
    }
  }

  async function setLogging(kind: "network" | "application", enabled: boolean): Promise<void> {
    saving = true;
    error = null;
    try {
      await updateSettings(kind === "network" ? { diagnosticsEnabled: enabled } : { applicationLoggingEnabled: enabled });
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
    <div class="flex items-center gap-2"><div class="min-w-0 flex-1"><Toggle label={t("Salva log applicativi", $language)} checked={$settings.data.applicationLoggingEnabled} disabled={saving} onChange={(enabled) => void setLogging("application", enabled)} /></div><ResetSetting label={t("Ripristina log applicativi", $language)} disabled={saving || $settings.data.applicationLoggingEnabled === defaultSettings.applicationLoggingEnabled} onClick={() => void setLogging("application", defaultSettings.applicationLoggingEnabled)} /></div>
    <p class="text-xs text-zinc-400 light:text-zinc-600">{t("Registra avvio, navigazione, errori e panic in app.txt. Per diagnosticare un crash, attiva i log e riavvia Legio prima di riprodurlo.", $language)}</p>
    <div class="flex items-center gap-2"><div class="min-w-0 flex-1"><Toggle label={t("Salva log di rete", $language)} checked={$settings.data.diagnosticsEnabled} disabled={saving} onChange={(enabled) => void setLogging("network", enabled)} /></div><ResetSetting label={t("Ripristina log di rete", $language)} disabled={saving || $settings.data.diagnosticsEnabled === defaultSettings.diagnosticsEnabled} onClick={() => void setLogging("network", defaultSettings.diagnosticsEnabled)} /></div>
    <p class="text-xs text-zinc-400 light:text-zinc-600">{t("Entrambi i log sono disattivati di default. I file vengono ruotati per limitare lo spazio occupato; disattivarli interrompe la registrazione di nuovi eventi.", $language)}</p>
    {#if applicationStatus?.directory ?? status?.directory}
      <div class="pt-3">
        <p class="text-xs text-zinc-500 light:text-zinc-600">{t("Cartella dei log", $language)}</p>
        <p class="mt-1 break-all font-mono text-xs text-zinc-300 light:text-zinc-700">{applicationStatus?.directory ?? status?.directory}</p>
        <p class="mt-2 text-xs text-zinc-400 light:text-zinc-600">{t("Registro degli avvii: game-launches.jsonl. Include fasi, errori e codici di uscita senza argomenti o valori delle variabili d'ambiente.", $language)}</p>
      </div>
    {/if}
    {#if launchDirectory}
      <p class="text-xs text-zinc-400 light:text-zinc-600">{t("Ogni gioco manuale salva launch.json, launch.log, stdout.log, stderr.log e runner-exit-code.txt nella propria cartella. Il debug aggiunge i dettagli di Proton o Wine.", $language)}</p>
      <p class="break-all font-mono text-xs text-zinc-300 light:text-zinc-700">{launchDirectory}</p>
    {/if}
    {#if status?.lastError}<p class="text-xs text-amber-300 light:text-amber-800" role="status">{status.lastError}</p>{/if}
    {#if applicationStatus?.lastError}<p class="text-xs text-amber-300 light:text-amber-800" role="status">{applicationStatus.lastError}</p>{/if}
  </SettingsGroup>
</section>
