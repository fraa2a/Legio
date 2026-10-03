<script lang="ts">
  import { t, language } from "../../i18n";
  import { onMount } from "svelte";
  import Button from "../../components/ui/Button.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import SettingsGroup from "../../components/ui/SettingsGroup.svelte";
  import SettingsRow from "../../components/ui/SettingsRow.svelte";
  import { getNetworkLogStatus, type NetworkLogStatus } from "../../services/network";
  import { checkConnectivity, connectivityError, networkSummary } from "../../stores/network";

  let diagnostics = $state<NetworkLogStatus | null>(null);
  let diagnosticsError = $state<string | null>(null);
  let loadingDiagnostics = $state(false);

  onMount(() => {
    void refreshDiagnostics();
  });

  async function refreshDiagnostics(): Promise<void> {
    loadingDiagnostics = true;
    diagnosticsError = null;
    try {
      diagnostics = await getNetworkLogStatus();
    } catch (error) {
      diagnosticsError = error instanceof Error ? error.message : String(error);
    } finally {
      loadingDiagnostics = false;
    }
  }

  const statusDotClass = $derived(
    $networkSummary.status === "Online"
      ? "bg-emerald-400"
      : $networkSummary.status === t("Non verificato", $language)
        ? "bg-zinc-400"
        : "bg-red-400",
  );
</script>

<section class="flex flex-col gap-4">
  {#if $connectivityError}
    <ErrorBanner message={$connectivityError} onRetry={() => void checkConnectivity()} />
  {/if}

  <SettingsGroup
    title={t("Connessione", $language)}
    description={t("Stato della connessione e raggiungibilità dei server usati dallo store.", $language)}
  >
    <SettingsRow label={t("Connessione", $language)}>
      <span class="flex items-center gap-2 text-sm text-zinc-300 light:text-zinc-700">
        <span class="size-2 rounded-full {statusDotClass}" aria-hidden="true"></span>
        {$networkSummary.status}
      </span>
    </SettingsRow>
    <SettingsRow label={t("Server Steam", $language)}>
      <span class="text-sm text-zinc-300 light:text-zinc-700">{$networkSummary.steam}</span>
    </SettingsRow>
    {#if $networkSummary.detail}
      <SettingsRow label={t("Dettaglio", $language)}>
        <span class="max-w-72 text-right text-sm text-zinc-400 light:text-zinc-600">{$networkSummary.detail}</span>
      </SettingsRow>
    {/if}
    <div>
      <Button label={t("Verifica connettività", $language)} variant="secondary" onClick={() => void checkConnectivity()} />
    </div>
  </SettingsGroup>

  <SettingsGroup
    title={t("Diagnostica di rete", $language)}
    description={t("Ultimo errore registrato e stato della coda diagnostica locale.", $language)}
  >
    {#if diagnosticsError !== null}
      <ErrorBanner message={diagnosticsError} onRetry={() => void refreshDiagnostics()} />
    {:else if diagnostics !== null}
      {#if diagnostics.lastError}
        <ErrorBanner message={diagnostics.lastError} />
      {:else}
        <p class="text-sm text-emerald-300 light:text-emerald-700">{t("Nessun errore di rete registrato.", $language)}</p>
      {/if}
      <SettingsRow label={t("Record in attesa", $language)}>
        <span class="text-sm text-zinc-300 light:text-zinc-700">{diagnostics.pendingRecords}</span>
      </SettingsRow>
      <SettingsRow label={t("Record scartati", $language)}>
        <span class="text-sm text-zinc-300 light:text-zinc-700">{diagnostics.droppedRecords}</span>
      </SettingsRow>
      {#if diagnostics.directory}
        <SettingsRow label={t("Cartella diagnostica", $language)}>
          <span class="max-w-72 break-all text-right text-xs text-zinc-400 light:text-zinc-600">{diagnostics.directory}</span>
        </SettingsRow>
      {/if}
      <div>
        <Button
          label={loadingDiagnostics ? t("Aggiornamento...", $language) : t("Aggiorna diagnostica", $language)}
          variant="secondary"
          disabled={loadingDiagnostics}
          onClick={() => void refreshDiagnostics()}
        />
      </div>
    {/if}
  </SettingsGroup>
</section>