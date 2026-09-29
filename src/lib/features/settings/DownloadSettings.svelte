<script lang="ts">
  import Button from "../../components/ui/Button.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import SettingsGroup from "../../components/ui/SettingsGroup.svelte";
  import SettingsRow from "../../components/ui/SettingsRow.svelte";
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

  const limitDescription = $derived(
    $bandwidthLimit.data === 0
      ? "Nessun limite di banda: i download usano tutta la connessione disponibile."
      : `Limite attuale: ${savedMegabytes} MB/s`,
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
  {#if $bandwidthLimit.status === "error"}
    <ErrorBanner
      message={`Impossibile leggere il limite di banda: ${$bandwidthLimit.error}`}
      onRetry={() => void bandwidthLimit.load()}
    />
  {:else}
    {#if $bandwidthLimitError}
      <ErrorBanner message={$bandwidthLimitError} />
    {/if}

    <SettingsGroup title="Velocità di download" description={limitDescription}>
      <div class="flex flex-wrap items-end gap-3">
        <div class="w-56">
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
        </div>
        <Button label="Applica" disabled={!draftChanged || savingLimit} onClick={() => void applyLimit()} />
      </div>
    </SettingsGroup>

    {#if $installedFolderError}
      <ErrorBanner message={$installedFolderError} />
    {/if}

    <SettingsGroup title="Cartella dei giochi installati">
      <SettingsRow label="Posizione dei giochi installati dallo store">
        <Button label="Sfoglia" variant="secondary" disabled={openingFolder} onClick={() => void browseFolder()} />
      </SettingsRow>
    </SettingsGroup>
  {/if}
</section>