<script lang="ts">
  import { onMount } from "svelte";
  import { t, language } from "../../i18n";
  import { getNetworkLogStatus, type NetworkLogStatus } from "../../services/network";
  import { getApplicationLogStatus, type ApplicationLogStatus } from "../../services/application-log";
  import { settings, defaultSettings, updateSettings } from "../../stores/settings";
  import { toMessage } from "../../utils/errors";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import SettingsGroup from "../../components/ui/SettingsGroup.svelte";
  import ToggleSetting from "../../components/ui/ToggleSetting.svelte";
  import Callout from "../../components/ui/Callout.svelte";
  import { appInfo } from "../../stores/app-info";
  import { getCompatibilityLogsDirectory } from "../../services/game-settings";
  import { createSupportBundle } from "../../services/support-bundle";
  import { showToast } from "../../stores/toast";
  import SettingsRow from "../../components/ui/SettingsRow.svelte";
  import Button from "../../components/ui/Button.svelte";

  let status = $state<NetworkLogStatus | null>(null);
  let applicationStatus = $state<ApplicationLogStatus | null>(null);
  let error = $state<string | null>(null);
  let saving = $state(false);
  let launchDirectory = $state<string | null>(null);
  let creatingBundle = $state(false);

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

  async function createBundle(): Promise<void> {
    creatingBundle = true;
    try {
      if (await createSupportBundle()) showToast(t("Pacchetto di supporto salvato.", $language), "success");
    } catch (cause) {
      showToast(toMessage(cause), "error");
    } finally {
      creatingBundle = false;
    }
  }
</script>

<section class="flex flex-col gap-4">
  {#if error !== null}<ErrorBanner message={error} onRetry={() => void refresh()} />{/if}
  <SettingsGroup icon="info" title={t("Log e diagnostica", $language)}>
    <ToggleSetting label={t("Salva log applicativi", $language)} defaultValue={defaultSettings.applicationLoggingEnabled} checked={$settings.data.applicationLoggingEnabled} disabled={saving} onChange={(enabled) => void setLogging("application", enabled)} />
    <p class="text-xs text-zinc-400 light:text-zinc-600">{t("Registra avvio, navigazione, errori e panic in app.txt. Per diagnosticare un crash, attiva i log e riavvia Legio prima di riprodurlo.", $language)}</p>
    <ToggleSetting label={t("Salva log di rete", $language)} defaultValue={defaultSettings.diagnosticsEnabled} checked={$settings.data.diagnosticsEnabled} disabled={saving} onChange={(enabled) => void setLogging("network", enabled)} />
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
    <SettingsRow label={t("Crea pacchetto di supporto", $language)} description={t("Salva uno zip con versione, sistema, impostazioni non sensibili e log redatti da allegare a una segnalazione. Il contenuto è elencato in README.txt.", $language)}>
      <Button label={t("Crea pacchetto", $language)} variant="secondary" disabled={creatingBundle} onClick={() => void createBundle()} />
    </SettingsRow>
    {#if status?.lastError}<Callout tone="warning" message={status.lastError} />{/if}
    {#if applicationStatus?.lastError}<Callout tone="warning" message={applicationStatus.lastError} />{/if}
  </SettingsGroup>
</section>
