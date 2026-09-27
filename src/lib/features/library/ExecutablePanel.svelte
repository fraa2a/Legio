<script lang="ts">
  import { untrack } from "svelte";
  import type { Game } from "../../services/local-state";
  import { toMessage } from "../../utils/errors";
  import {
    browseGameDirectory,
    manualImport,
    resetManualImport,
    saveExecutableForGame,
    setScanGameName,
  } from "../../stores/manual-import";
  import Button from "../../components/ui/Button.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import Panel from "../../components/ui/Panel.svelte";
  import ExecutableScanPanel from "./ExecutableScanPanel.svelte";

  let { game }: { game: Game } = $props();

  // A game without an executable needs the scan panel open right away.
  let scanning = $state(untrack(() => game.executablePath === null));
  let pending = $state(false);
  let actionError = $state<string | null>(null);

  $effect(() => resetManualImport);

  function startScan(): void {
    actionError = null;
    scanning = true;
    setScanGameName(game.name);
    void browseGameDirectory(game.executablePath);
  }

  function stopScan(): void {
    resetManualImport();
    scanning = false;
  }

  async function save(): Promise<void> {
    actionError = null;
    pending = true;
    try {
      await saveExecutableForGame(game.id);
      resetManualImport();
    } catch (error) {
      actionError = toMessage(error);
    } finally {
      pending = false;
    }
  }
</script>

<Panel title="Eseguibile">
  {#if actionError !== null}
    <ErrorBanner message={actionError} />
  {/if}
  <p class="text-sm text-zinc-400 light:text-zinc-600">
    Eseguibile salvato: <span class="break-all">{game.executablePath ?? "nessuno"}</span>. La
    scansione non modifica nulla finché non confermi la scelta.
  </p>
  {#if scanning}
    <ExecutableScanPanel
      hint="Scegli la cartella da cui cercare l'eseguibile. La scansione è indipendente da quella già salvata."
      nameLabel="Nome per la scansione"
      startPath={game.executablePath}
      disabled={pending}
    />
    <div class="flex flex-wrap justify-end gap-2">
      <Button label="Annulla scansione" variant="secondary" onClick={stopScan} />
      <Button
        label="Salva eseguibile scelto"
        disabled={pending || $manualImport.selectedPath === null}
        onClick={() => void save()}
      />
    </div>
  {:else}
    <div>
      <Button label="Nuova scansione" variant="secondary" onClick={startScan} />
    </div>
  {/if}
</Panel>
