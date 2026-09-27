<script lang="ts">
  import type { Theme } from "../../services/local-state";
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
  import { checkConnectivity, connectivityError, networkSummary } from "../../stores/network";
  import { changeTheme, settings, settingsError } from "../../stores/settings";

  const themes: { value: Theme; label: string }[] = [
    { value: "system", label: "Sistema" },
    { value: "dark", label: "Scuro" },
    { value: "light", label: "Chiaro" },
  ];

  let selectedTheme: Theme | null = $state(null);

  const currentTheme = $derived(selectedTheme ?? $settings.data.theme);

  async function selectTheme(theme: Theme): Promise<void> {
    selectedTheme = theme;
    await changeTheme(theme);
    selectedTheme = null;
  }

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

<section class="mb-8">
  <h2 class="mb-3 text-lg font-medium text-zinc-100 light:text-zinc-800">Tema</h2>

  {#if $settingsError}
    <div class="mb-3">
      <ErrorBanner message={$settingsError} />
    </div>
  {/if}

  <fieldset class="flex flex-wrap gap-2">
    <legend class="sr-only">Tema dell'interfaccia</legend>
    {#each themes as theme (theme.value)}
      <label
        class="flex cursor-pointer items-center gap-2 rounded-lg bg-white/5 px-4 py-2 text-sm text-zinc-200 transition-colors duration-200 hover:bg-white/10 has-checked:bg-white/20 light:bg-zinc-100 light:text-zinc-800"
      >
        <input
          type="radio"
          name="theme"
          value={theme.value}
          checked={currentTheme === theme.value}
          onchange={() => void selectTheme(theme.value)}
          class="size-4 accent-white"
        />
        {theme.label}
      </label>
    {/each}
  </fieldset>
</section>

<section class="mb-8">
  <h2 class="mb-3 text-lg font-medium text-zinc-100 light:text-zinc-800">Rete</h2>

  <div class="flex flex-wrap items-center gap-3 text-sm text-zinc-300 light:text-zinc-700">
    <span>Stato: {$networkSummary.status}</span>
    <span class="text-zinc-400 light:text-zinc-600">Steam: {$networkSummary.steam}</span>
  </div>

  {#if $networkSummary.detail}
    <p class="mt-1 text-xs text-zinc-500">Dettaglio: {$networkSummary.detail}</p>
  {/if}

  {#if $connectivityError}
    <div class="mt-3">
      <ErrorBanner message={$connectivityError} onRetry={() => void checkConnectivity()} />
    </div>
  {/if}

  <div class="mt-3">
    <Button label="Verifica connettività" variant="secondary" onClick={() => void checkConnectivity()} />
  </div>
</section>

<section class="mb-8">
  <h2 class="mb-3 text-lg font-medium text-zinc-100 light:text-zinc-800">Download</h2>

  {#if $bandwidthLimit.status === "error"}
    <ErrorBanner
      message={`Impossibile leggere il limite di banda: ${$bandwidthLimit.error}`}
      onRetry={() => void bandwidthLimit.load()}
    />
  {:else}
    <p class="mb-3 text-sm text-zinc-400 light:text-zinc-600">
      {#if $bandwidthLimit.data === 0}
        Nessun limite di banda: i download usano tutta la connessione disponibile.
      {:else}
        Limite attuale: {savedMegabytes} MB/s
      {/if}
    </p>

    {#if $bandwidthLimitError}
      <div class="mb-3">
        <ErrorBanner message={$bandwidthLimitError} />
      </div>
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
      <div class="mt-3">
        <ErrorBanner message={$installedFolderError} />
      </div>
    {/if}

    <div class="mt-4 flex flex-wrap items-center gap-3">
      <Button label="Sfoglia" variant="secondary" disabled={openingFolder} onClick={() => void browseFolder()} />
      <span class="text-xs text-zinc-500">Cartella dei giochi installati dallo store</span>
    </div>
  {/if}
</section>
