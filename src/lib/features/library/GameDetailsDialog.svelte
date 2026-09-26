<script lang="ts">
  import type { Game } from "../../services/local-state";
  import { toMessage } from "../../utils/errors";
  import { saveGame } from "../../stores/games";
  import {
    browseGameDirectory,
    manualImport,
    resetManualImport,
    saveExecutableForGame,
    setScanGameName,
  } from "../../stores/manual-import";
  import { clearSteamDetails, loadSteamDetails } from "../../stores/steam-details";
  import Badge from "../../components/ui/Badge.svelte";
  import Button from "../../components/ui/Button.svelte";
  import Dialog from "../../components/ui/Dialog.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import TextField from "../../components/ui/TextField.svelte";
  import ExecutableScanPanel from "./ExecutableScanPanel.svelte";
  import SteamMetadata from "./SteamMetadata.svelte";

  let {
    game,
    onClose,
    onDelete,
  }: {
    game: Game;
    onClose: () => void;
    onDelete: (game: Game) => void;
  } = $props();

  const steamAppId = $derived(game.steamAppId);
  const isSteamGame = $derived(steamAppId !== null);

  let nameDraft = $state<string | null>(null);
  let scanning = $state(false);
  let pending = $state(false);
  let actionError = $state<string | null>(null);

  const nameValue = $derived(nameDraft ?? game.nameOverride ?? game.name);
  const trimmedName = $derived(nameValue.trim());
  const nameChanged = $derived(trimmedName !== (game.nameOverride ?? "").trim());
  const canResetName = $derived(game.automaticName !== null && game.nameOverride !== null);

  $effect(() => {
    if (steamAppId === null) return;
    void loadSteamDetails(steamAppId, false);
    return () => clearSteamDetails();
  });

  function handleNameInput(value: string): void {
    nameDraft = value;
  }

  function startScan(): void {
    actionError = null;
    scanning = true;
    setScanGameName(game.name);
    void browseGameDirectory(game.executablePath);
  }

  async function saveName(): Promise<void> {
    actionError = null;
    pending = true;
    try {
      await saveGame({
        id: game.id,
        steamAppId: game.steamAppId,
        automaticName: game.automaticName,
        nameOverride: trimmedName.length > 0 ? trimmedName : null,
      });
      nameDraft = null;
    } catch (error) {
      actionError = toMessage(error);
    } finally {
      pending = false;
    }
  }

  async function resetName(): Promise<void> {
    actionError = null;
    pending = true;
    try {
      await saveGame({
        id: game.id,
        steamAppId: game.steamAppId,
        automaticName: game.automaticName,
        nameOverride: null,
      });
      nameDraft = null;
    } catch (error) {
      actionError = toMessage(error);
    } finally {
      pending = false;
    }
  }

  async function saveExecutable(): Promise<void> {
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

  function close(): void {
    resetManualImport();
    onClose();
  }
</script>

<Dialog open title="Impostazioni gioco" size="wide" onClose={close}>
  <div class="flex flex-wrap items-center gap-2">
    <p class="min-w-0 flex-1 truncate text-lg font-medium text-zinc-50 light:text-zinc-900">
      {game.name}
    </p>
    <Badge tone={isSteamGame ? "info" : "neutral"} title={isSteamGame ? "Steam" : "Manuale"} />
    {#if game.nameOverride !== null}
      <Badge tone="warning" title="Nome personalizzato" />
    {/if}
  </div>

  {#if actionError !== null}
    <ErrorBanner message={actionError} />
  {/if}

  <section class="flex flex-col gap-3">
    <h3 class="text-sm font-semibold text-zinc-200 light:text-zinc-800">Nome</h3>
    <TextField
      id="game-name"
      label="Nome visualizzato"
      value={nameValue}
      oninput={handleNameInput}
      disabled={pending}
      hint={game.automaticName !== null
        ? `Nome automatico: ${game.automaticName}`
        : "Nessun nome automatico disponibile per questa voce."}
    />
    <div class="flex flex-wrap gap-2">
      <Button
        label="Salva nome"
        disabled={pending || !nameChanged || trimmedName.length === 0}
        onClick={() => void saveName()}
      />
      {#if canResetName}
        <Button
          label="Ripristina nome automatico"
          variant="secondary"
          disabled={pending}
          onClick={() => void resetName()}
        />
      {/if}
    </div>
  </section>

  <section class="flex flex-col gap-2">
    <h3 class="text-sm font-semibold text-zinc-200 light:text-zinc-800">Dettagli</h3>
    <dl class="grid gap-1 text-sm text-zinc-400 light:text-zinc-600">
      <div class="flex gap-2">
        <dt class="w-32 shrink-0">Steam App ID</dt>
        <dd class="min-w-0 break-words">{game.steamAppId ?? "nessuno"}</dd>
      </div>
      <div class="flex gap-2">
        <dt class="w-32 shrink-0">{isSteamGame ? "Installazione" : "Eseguibile"}</dt>
        <dd class="min-w-0 break-all">{game.steamInstallPath ?? game.executablePath ?? "non impostato"}</dd>
      </div>
      {#if game.steamAccountId !== null}
        <div class="flex gap-2">
          <dt class="w-32 shrink-0">Account Steam</dt>
          <dd class="min-w-0 break-words">{game.steamAccountId}</dd>
        </div>
      {/if}
    </dl>
  </section>

  {#if !isSteamGame}
    <section class="flex flex-col gap-3">
      <h3 class="text-sm font-semibold text-zinc-200 light:text-zinc-800">Eseguibile</h3>
      <p class="text-xs text-zinc-500">
        Eseguibile salvato: {game.executablePath ?? "nessuno"}. Una nuova scansione non modifica
        nulla finché non confermi la scelta.
      </p>
      {#if scanning}
        <ExecutableScanPanel
          hint="Scegli la cartella da cui cercare l'eseguibile. La scansione è indipendente da quella già salvata."
          nameLabel="Nome per la scansione"
          startPath={game.executablePath}
          disabled={pending}
        />
        <div class="flex flex-wrap justify-end gap-2">
          <Button
            label="Annulla scansione"
            variant="secondary"
            onClick={() => {
              resetManualImport();
              scanning = false;
            }}
          />
          <Button
            label="Salva eseguibile scelto"
            disabled={pending || $manualImport.selectedPath === null}
            onClick={() => void saveExecutable()}
          />
        </div>
      {:else}
        <div class="flex flex-wrap gap-2">
          <Button label="Nuova scansione" variant="secondary" onClick={startScan} />
        </div>
      {/if}
    </section>
  {/if}

  {#if steamAppId !== null}
    <SteamMetadata {steamAppId} />
  {/if}

  <div class="flex flex-wrap justify-between gap-2">
    <Button label="Chiudi" variant="secondary" onClick={close} />
    <Button label="Rimuovi dalla libreria" variant="danger" disabled={pending} onClick={() => onDelete(game)} />
  </div>
</Dialog>
