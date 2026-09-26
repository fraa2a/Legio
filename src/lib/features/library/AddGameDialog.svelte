<script lang="ts">
  import { getSteamDetails } from "../../services/steam-details";
  import { toMessage } from "../../utils/errors";
  import { addGame } from "../../stores/games";
  import { importScannedGame, manualImport, resetManualImport } from "../../stores/manual-import";
  import Button from "../../components/ui/Button.svelte";
  import Dialog from "../../components/ui/Dialog.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import TextField from "../../components/ui/TextField.svelte";
  import ExecutableScanPanel from "./ExecutableScanPanel.svelte";

  let { onClose }: { onClose: () => void } = $props();

  type Mode = "steam" | "executable";

  let mode = $state<Mode>("steam");
  let steamAppId = $state("");
  let steamName = $state("");
  let steamHint = $state("Il nome mostrato in libreria. Obbligatorio.");
  let pending = $state(false);
  let actionError = $state<string | null>(null);

  const parsedAppId = $derived(parseAppId(steamAppId));
  const canCreateFromSteam = $derived(parsedAppId !== null && steamName.trim().length > 0);
  const canImportExecutable = $derived($manualImport.selectedPath !== null);

  function parseAppId(value: string): number | null {
    const trimmed = value.trim();
    if (!/^\d+$/.test(trimmed)) return null;
    const parsed = Number(trimmed);
    return parsed > 0 ? parsed : null;
  }

  async function fetchSteamName(): Promise<void> {
    if (parsedAppId === null) return;
    actionError = null;
    pending = true;
    try {
      const result = await getSteamDetails(parsedAppId, false);
      if (result.details === null) {
        steamHint = "Nessun dato Steam disponibile per questo App ID: inserisci il nome a mano.";
        return;
      }
      steamName = result.details.name;
      steamHint = result.stale ? "Dati Steam dalla cache locale." : steamHint;
    } catch (error) {
      actionError = toMessage(error);
    } finally {
      pending = false;
    }
  }

  async function createFromSteam(): Promise<void> {
    if (parsedAppId === null) return;
    actionError = null;
    pending = true;
    try {
      await addGame({ name: steamName.trim(), steamAppId: parsedAppId });
      close();
    } catch (error) {
      actionError = toMessage(error);
    } finally {
      pending = false;
    }
  }

  async function importFromExecutable(): Promise<void> {
    actionError = null;
    pending = true;
    try {
      await importScannedGame($manualImport.gameName);
      close();
    } catch (error) {
      actionError = toMessage(error);
    } finally {
      pending = false;
    }
  }

  function close(): void {
    resetManualImport();
    onClose();
  }
</script>

<Dialog open title="Aggiungi un gioco" size="wide" onClose={close}>
  <div class="flex gap-2" role="group" aria-label="Origine del gioco">
    <Button
      label="App ID Steam"
      variant={mode === "steam" ? "primary" : "secondary"}
      pressed={mode === "steam"}
      onClick={() => (mode = "steam")}
    />
    <Button
      label="Eseguibile"
      variant={mode === "executable" ? "primary" : "secondary"}
      pressed={mode === "executable"}
      onClick={() => (mode = "executable")}
    />
  </div>

  {#if actionError !== null}
    <ErrorBanner message={actionError} />
  {/if}

  {#if mode === "steam"}
    <section class="flex flex-col gap-3">
      <TextField
        id="add-steam-app-id"
        label="App ID Steam"
        bind:value={steamAppId}
        inputmode="numeric"
        placeholder="es. 620"
      />
      <div class="flex flex-wrap gap-2">
        <Button
          label="Recupera dati Steam"
          variant="secondary"
          disabled={pending || parsedAppId === null}
          onClick={() => void fetchSteamName()}
        />
      </div>
      <TextField id="add-steam-name" label="Nome visualizzato" bind:value={steamName} hint={steamHint} />
      <p class="text-xs text-zinc-500">
        La voce viene creata come collegamento manuale a Steam, senza percorso di installazione.
      </p>
    </section>
  {:else}
    <ExecutableScanPanel
      hint="Scegli la cartella del gioco: la scansione elenca ogni eseguibile trovato con punteggio e segnali. In alternativa indica direttamente il file eseguibile."
      nameLabel="Nome visualizzato (opzionale)"
      disabled={pending}
    />
  {/if}

  <div class="flex flex-wrap justify-end gap-2">
    <Button label="Annulla" variant="secondary" onClick={close} />
    {#if mode === "steam"}
      <Button
        label="Aggiungi"
        disabled={pending || !canCreateFromSteam}
        onClick={() => void createFromSteam()}
      />
    {:else}
      <Button
        label="Aggiungi alla libreria"
        disabled={pending || !canImportExecutable}
        onClick={() => void importFromExecutable()}
      />
    {/if}
  </div>
</Dialog>
