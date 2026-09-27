<script lang="ts">
  import { toMessage } from "../../utils/errors";
  import { addGame, saveGame } from "../../stores/games";
  import {
    importScannedGame,
    manualImport,
    resetManualImport,
    type ManualImportOutcome,
  } from "../../stores/manual-import";
  import { loadSteamDetails } from "../../stores/steam-details";
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
  let importOutcome = $state<ManualImportOutcome | null>(null);

  const parsedAppId = $derived(parseAppId(steamAppId));
  const canCreateFromSteam = $derived(parsedAppId !== null && steamName.trim().length > 0);
  const canImportExecutable = $derived(
    $manualImport.selectedPath !== null && importOutcome === null,
  );

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
      const result = await loadSteamDetails(parsedAppId, false);
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
      importOutcome = await importScannedGame($manualImport.gameName);
    } catch (error) {
      actionError = toMessage(error);
    } finally {
      pending = false;
    }
  }

  async function linkCandidate(candidate: { steamAppId: number; name: string }): Promise<void> {
    if (importOutcome === null) return;
    actionError = null;
    pending = true;
    try {
      const game = await saveGame({
        id: importOutcome.game.id,
        steamAppId: candidate.steamAppId,
        automaticName: candidate.name,
        nameOverride: importOutcome.game.nameOverride,
      });
      importOutcome = {
        game,
        identification: {
          game,
          status: "matched",
          candidates: [candidate],
          message: null,
        },
        identificationError: null,
      };
    } catch (error) {
      actionError = toMessage(error);
    } finally {
      pending = false;
    }
  }

  const identificationMessage = $derived.by(() => {
    if (importOutcome === null) return null;
    if (importOutcome.identificationError !== null) {
      return `Gioco importato. Rilevazione Steam non disponibile: ${importOutcome.identificationError}`;
    }
    switch (importOutcome.identification?.status) {
      case "matched":
        return `App ID Steam rilevato: ${importOutcome.game.automaticName} (${importOutcome.game.steamAppId}).`;
      case "ambiguous":
        return "Trovate più corrispondenze con lo stesso nome. Scegli quella corretta per collegarla.";
      case "unavailable":
        return `Gioco importato. Rilevazione Steam non disponibile: ${importOutcome.identification.message ?? "riprova più tardi"}.`;
      case "no_match":
        return "Gioco importato. Nessuna corrispondenza esatta nel catalogo Steam.";
      default:
        return "Gioco importato nella libreria.";
    }
  });

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
      disabled={pending || importOutcome !== null}
    />
    {#if identificationMessage !== null}
      <div class="rounded-lg bg-sky-500/10 p-3 text-sm text-sky-100 light:text-sky-900" role="status">
        {identificationMessage}
      </div>
      {#if importOutcome?.identification?.status === "ambiguous"}
        <fieldset class="flex flex-col gap-2" disabled={pending}>
          <legend class="text-sm text-zinc-300 light:text-zinc-700">Corrispondenze Steam</legend>
          {#each importOutcome.identification.candidates as candidate (candidate.steamAppId)}
            <div class="flex flex-wrap items-center justify-between gap-2 rounded-lg bg-white/5 p-3 light:bg-zinc-100">
              <span class="text-sm text-zinc-100 light:text-zinc-900">
                {candidate.name} <span class="text-zinc-500">({candidate.steamAppId})</span>
              </span>
              <Button
                label="Collega"
                variant="secondary"
                disabled={pending}
                onClick={() => void linkCandidate(candidate)}
              />
            </div>
          {/each}
        </fieldset>
      {/if}
    {/if}
  {/if}

  <div class="flex flex-wrap justify-end gap-2">
    {#if importOutcome !== null}
      <Button label="Fine" onClick={close} />
    {:else}
      <Button label="Annulla" variant="secondary" onClick={close} />
    {/if}
    {#if importOutcome === null && mode === "steam"}
      <Button
        label="Aggiungi"
        disabled={pending || !canCreateFromSteam}
        onClick={() => void createFromSteam()}
      />
    {:else if importOutcome === null}
      <Button
        label={pending ? "Importazione e riconoscimento..." : "Aggiungi alla libreria"}
        disabled={pending || !canImportExecutable}
        onClick={() => void importFromExecutable()}
      />
    {/if}
  </div>
</Dialog>
