<script lang="ts">
  import { onMount } from "svelte";
  import Button from "../../components/ui/Button.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
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
</script>

<section class="flex flex-col gap-4">
  <div>
    <h3 class="text-xl font-semibold text-zinc-100 light:text-zinc-900">Rete</h3>
    <p class="mt-1 text-sm text-zinc-400 light:text-zinc-600">
      Stato della connessione e raggiungibilità dei server usati dallo store.
    </p>
  </div>

  <div class="flex flex-wrap items-center gap-3 text-sm text-zinc-300 light:text-zinc-700">
    <span>Stato: {$networkSummary.status}</span>
    <span class="text-zinc-400 light:text-zinc-600">Steam: {$networkSummary.steam}</span>
  </div>

  {#if $networkSummary.detail}
    <p class="text-xs text-zinc-500">Dettaglio: {$networkSummary.detail}</p>
  {/if}

  {#if $connectivityError}
    <ErrorBanner message={$connectivityError} onRetry={() => void checkConnectivity()} />
  {/if}

  <div>
    <Button label="Verifica connettività" variant="secondary" onClick={() => void checkConnectivity()} />
  </div>

  <div class="mt-3 flex flex-col gap-3 border-t border-white/10 pt-4 light:border-zinc-900/10">
    <div>
      <h4 class="font-medium text-zinc-100 light:text-zinc-900">Diagnostica di rete</h4>
      <p class="mt-1 text-sm text-zinc-400 light:text-zinc-600">
        Ultimo errore registrato e stato della coda diagnostica locale.
      </p>
    </div>
    {#if diagnosticsError !== null}
      <ErrorBanner message={diagnosticsError} onRetry={() => void refreshDiagnostics()} />
    {:else if diagnostics !== null}
      {#if diagnostics.lastError}
        <ErrorBanner message={diagnostics.lastError} />
      {:else}
        <p class="text-sm text-emerald-300 light:text-emerald-700">Nessun errore di rete registrato.</p>
      {/if}
      <dl class="grid gap-2 text-sm text-zinc-300 light:text-zinc-700">
        <div class="flex flex-wrap gap-2">
          <dt class="text-zinc-500">Record in attesa</dt>
          <dd>{diagnostics.pendingRecords}</dd>
        </div>
        <div class="flex flex-wrap gap-2">
          <dt class="text-zinc-500">Record scartati</dt>
          <dd>{diagnostics.droppedRecords}</dd>
        </div>
        {#if diagnostics.directory}
          <div class="flex flex-col gap-1">
            <dt class="text-zinc-500">Cartella diagnostica</dt>
            <dd class="break-all">{diagnostics.directory}</dd>
          </div>
        {/if}
      </dl>
    {/if}
    <div>
      <Button
        label={loadingDiagnostics ? "Aggiornamento..." : "Aggiorna diagnostica"}
        variant="secondary"
        disabled={loadingDiagnostics}
        onClick={() => void refreshDiagnostics()}
      />
    </div>
  </div>
</section>
