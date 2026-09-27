<script lang="ts">
  import Button from "../../components/ui/Button.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import TextField from "../../components/ui/TextField.svelte";
  import {
    bandwidthLimit,
    bandwidthLimitError,
    browseInstalledFolder,
    installedFolderError,
    saveBandwidthLimit,
  } from "../../stores/downloads";

  const bytesPerMegabyte = 1024 * 1024;

  let limitDraft = $state<string | null>(null);
  let savingLimit = $state(false);

  const savedMegabytes = $derived(
    ($bandwidthLimit.data / bytesPerMegabyte).toFixed(2).replace(/\.?0+$/, ""),
  );

  const draftMegabytes = $derived(limitDraft ?? savedMegabytes);
  const draftBytes = $derived(Math.round(Number(draftMegabytes) * bytesPerMegabyte));
  const draftChanged = $derived(
    draftMegabytes.trim().length > 0 &&
      Number.isFinite(Number(draftMegabytes)) &&
      draftBytes !== $bandwidthLimit.data,
  );

  async function applyLimit(): Promise<void> {
    savingLimit = true;
    await saveBandwidthLimit(draftBytes);
    limitDraft = null;
    savingLimit = false;
  }

  let openingFolder = $state(false);

  async function browseFolder(): Promise<void> {
    openingFolder = true;
    await browseInstalledFolder();
    openingFolder = false;
  }
</script>

<section class="flex flex-col gap-4">
  <div>
    <h3 class="text-xl font-semibold text-zinc-100 light:text-zinc-900">Download</h3>
    <p class="mt-1 text-sm text-zinc-400 light:text-zinc-600">
      Limita la velocità dei download e apri la cartella delle installazioni.
    </p>
  </div>

  {#if $bandwidthLimit.status === "error"}
    <ErrorBanner
      message={`Impossibile leggere il limite di banda: ${$bandwidthLimit.error}`}
      onRetry={() => void bandwidthLimit.load()}
    />
  {:else}
    <p class="text-sm text-zinc-400 light:text-zinc-600">
      {#if $bandwidthLimit.data === 0}
        Nessun limite di banda: i download usano tutta la connessione disponibile.
      {:else}
        Limite attuale: {savedMegabytes} MB/s
      {/if}
    </p>

    {#if $bandwidthLimitError}
      <ErrorBanner message={$bandwidthLimitError} />
    {/if}

    <div class="flex flex-wrap items-end gap-3">
      <TextField
        id="download-bandwidth-limit"
        label="Limite di banda (MB/s)"
        type="number"
        inputmode="numeric"
        value={draftMegabytes}
        placeholder="0"
        hint="0 significa senza limite. Il limite viene applicato subito e riportato al prossimo avvio."
        disabled={savingLimit}
        oninput={(value) => (limitDraft = value)}
      />
      <Button
        label="Applica"
        disabled={!draftChanged || savingLimit}
        onClick={() => void applyLimit()}
      />
    </div>

    {#if $installedFolderError}
      <ErrorBanner message={$installedFolderError} />
    {/if}

    <div class="flex flex-wrap items-center gap-3">
      <Button
        label="Sfoglia"
        variant="secondary"
        disabled={openingFolder}
        onClick={() => void browseFolder()}
      />
      <span class="text-xs text-zinc-500">Cartella dei giochi installati dallo store</span>
    </div>
  {/if}
</section>
