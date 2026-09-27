<script lang="ts">
  import Button from "../../components/ui/Button.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import { checkConnectivity, connectivityError, networkSummary } from "../../stores/network";
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
</section>
