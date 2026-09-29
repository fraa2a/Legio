<script lang="ts">
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
      : $networkSummary.status === "Non verificato"
        ? "bg-zinc-400"
        : "bg-red-400",
  );
</script>

<section class="flex flex-col gap-4">
  {#if $connectivityError}
    <ErrorBanner message={$connectivityError} onRetry={() => void checkConnectivity()} />
  {/if}

  <SettingsGroup
    title="Connessione"
    description="Stato della connessione e raggiungibilità dei server usati dallo store."
  >
    <SettingsRow label="Connessione">
      <span class="flex items-center gap-2 text-sm text-zinc-300 light:text-zinc-700">
        <span class="size-2 rounded-full {statusDotClass}" aria-hidden="true"></span>
        {$networkSummary.status}
      </span>
    </SettingsRow>
    <SettingsRow label="Server Steam">
      <span class="text-sm text-zinc-300 light:text-zinc-700">{$networkSummary.steam}</span>
    </SettingsRow>
    {#if $networkSummary.detail}
      <SettingsRow label="Dettaglio">
        <span class="max-w-72 text-right text-sm text-zinc-400 light:text-zinc-600">{$networkSummary.detail}</span>
      </SettingsRow>
    {/if}
    <div>
      <Button label="Verifica connettività" variant="secondary" onClick={() => void checkConnectivity()} />
    </div>
  </SettingsGroup>

  <SettingsGroup
    title="Diagnostica di rete"
    description="Ultimo errore registrato e stato della coda diagnostica locale."
  >
    {#if diagnosticsError !== null}
      <ErrorBanner message={diagnosticsError} onRetry={() => void refreshDiagnostics()} />
    {:else if diagnostics !== null}
      {#if diagnostics.lastError}
        <ErrorBanner message={diagnostics.lastError} />
      {:else}
        <p class="text-sm text-emerald-300 light:text-emerald-700">Nessun errore di rete registrato.</p>
      {/if}
      <SettingsRow label="Record in attesa">
        <span class="text-sm text-zinc-300 light:text-zinc-700">{diagnostics.pendingRecords}</span>
      </SettingsRow>
      <SettingsRow label="Record scartati">
        <span class="text-sm text-zinc-300 light:text-zinc-700">{diagnostics.droppedRecords}</span>
      </SettingsRow>
      {#if diagnostics.directory}
        <SettingsRow label="Cartella diagnostica">
          <span class="max-w-72 break-all text-right text-xs text-zinc-400 light:text-zinc-600">{diagnostics.directory}</span>
        </SettingsRow>
      {/if}
      <div>
        <Button
          label={loadingDiagnostics ? "Aggiornamento..." : "Aggiorna diagnostica"}
          variant="secondary"
          disabled={loadingDiagnostics}
          onClick={() => void refreshDiagnostics()}
        />
      </div>
    {/if}
  </SettingsGroup>
</section>