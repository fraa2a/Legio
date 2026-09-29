<script lang="ts">
  import type { Game } from "../../services/local-state";
  import { toMessage } from "../../utils/errors";
  import { deleteGame, saveGame } from "../../stores/games";
  import { detectManualGameSteamAppId } from "../../stores/manual-import";
  import Button from "../../components/ui/Button.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import Panel from "../../components/ui/Panel.svelte";
  import TextField from "../../components/ui/TextField.svelte";
  import type { SteamIdentityCandidate, SteamIdentificationResult } from "../../services/manual-import";

  let { game, onRemoved, section = "general" }: {
    game: Game;
    onRemoved: () => void;
    section?: "general" | "danger";
  } = $props();

  let nameDraft = $state<string | null>(null);
  let pending = $state(false);
  let confirmingRemove = $state(false);
  let actionError = $state<string | null>(null);
  let identityPending = $state(false);
  let identityError = $state<string | null>(null);
  let identityResult = $state<SteamIdentificationResult | null>(null);

  const nameValue = $derived(nameDraft ?? game.nameOverride ?? game.name);
  const trimmedName = $derived(nameValue.trim());
  const nameChanged = $derived(trimmedName !== (game.nameOverride ?? "").trim());
  const canResetName = $derived(game.automaticName !== null && game.nameOverride !== null);

  function handleNameInput(value: string): void {
    nameDraft = value;
  }

  async function saveName(override: string | null): Promise<void> {
    actionError = null;
    pending = true;
    try {
      await saveGame({
        id: game.id,
        steamAppId: game.steamAppId,
        automaticName: game.automaticName,
        nameOverride: override,
      });
      nameDraft = null;
    } catch (error) {
      actionError = toMessage(error);
    } finally {
      pending = false;
    }
  }

  async function remove(): Promise<void> {
    actionError = null;
    pending = true;
    try {
      await deleteGame(game.id);
      confirmingRemove = false;
      onRemoved();
    } catch (error) {
      actionError = toMessage(error);
    } finally {
      pending = false;
    }
  }

  async function detectIdentity(): Promise<void> {
    identityError = null;
    identityPending = true;
    try {
      identityResult = await detectManualGameSteamAppId(game.id);
    } catch (error) {
      identityError = toMessage(error);
    } finally {
      identityPending = false;
    }
  }

  async function linkCandidate(candidate: SteamIdentityCandidate): Promise<void> {
    identityError = null;
    identityPending = true;
    try {
      const updated = await saveGame({
        id: game.id,
        steamAppId: candidate.steamAppId,
        automaticName: candidate.name,
        nameOverride: game.nameOverride,
      });
      identityResult = {
        game: updated,
        status: "matched",
        candidates: [candidate],
        message: null,
      };
    } catch (error) {
      identityError = toMessage(error);
    } finally {
      identityPending = false;
    }
  }

  const identityMessage = $derived.by(() => {
    if (identityResult === null) return null;
    switch (identityResult.status) {
      case "matched":
        return `App ID Steam rilevato: ${identityResult.game.automaticName} (${identityResult.game.steamAppId}).`;
      case "ambiguous":
        return "Trovate più corrispondenze con lo stesso nome. Scegli quella corretta.";
      case "unavailable":
        return identityResult.message ?? "Rilevazione Steam non disponibile.";
      case "no_match":
        return "Nessuna corrispondenza esatta nel catalogo Steam.";
      default:
        return null;
    }
  });
</script>

{#if section === "general"}
<Panel title="Impostazioni">
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
      onClick={() => void saveName(trimmedName)}
    />
    {#if canResetName}
      <Button
        label="Ripristina nome automatico"
        variant="secondary"
        disabled={pending}
        onClick={() => void saveName(null)}
      />
    {/if}
  </div>
</Panel>

{#if game.steamInstallPath === null && game.executablePath !== null}
  <Panel title="Identità Steam">
    {#if game.steamAppId !== null}
      <p class="text-sm text-zinc-300 light:text-zinc-700">
        {game.automaticName ?? game.name} <span class="text-zinc-500">({game.steamAppId})</span>
      </p>
    {:else}
      <p class="text-sm text-zinc-400 light:text-zinc-600">
        Legio confronta il nome dell'eseguibile e della cartella con il catalogo Steam.
      </p>
      <div>
        <Button
          label={identityPending ? "Ricerca in corso..." : "Rileva Steam App ID"}
          variant="secondary"
          disabled={identityPending}
          onClick={() => void detectIdentity()}
        />
      </div>
    {/if}
    {#if identityError !== null}
      <ErrorBanner message={identityError} onRetry={() => void detectIdentity()} />
    {/if}
    {#if identityMessage !== null}
      <p class="text-sm text-zinc-300 light:text-zinc-700" role="status">{identityMessage}</p>
    {/if}
    {#if identityResult?.status === "ambiguous"}
      <div class="flex flex-col gap-2">
        {#each identityResult.candidates as candidate (candidate.steamAppId)}
          <div class="flex flex-wrap items-center justify-between gap-2 rounded-lg bg-white/5 p-3 light:bg-zinc-100">
            <span class="text-sm text-zinc-100 light:text-zinc-900">
              {candidate.name} <span class="text-zinc-500">({candidate.steamAppId})</span>
            </span>
            <Button
              label="Collega"
              variant="secondary"
              disabled={identityPending}
              onClick={() => void linkCandidate(candidate)}
            />
          </div>
        {/each}
      </div>
    {/if}
  </Panel>
{/if}

{:else}
<Panel title="Rimuovi dalla libreria">
  {#if actionError !== null}
    <ErrorBanner message={actionError} />
  {/if}
  {#if confirmingRemove}
    <p class="text-sm text-zinc-300 light:text-zinc-700">
      {game.name} verrà rimosso dalla libreria. I file installati non vengono eliminati.
    </p>
    <div class="flex flex-wrap gap-2">
      <Button label="Annulla" variant="secondary" disabled={pending} onClick={() => (confirmingRemove = false)} />
      <Button label="Rimuovi" variant="danger" disabled={pending} onClick={() => void remove()} />
    </div>
  {:else}
    <p class="text-sm text-zinc-400 light:text-zinc-600">
      Rimuovi questa voce senza toccare i file del gioco.
    </p>
    <div>
      <Button label="Rimuovi gioco" variant="danger" onClick={() => (confirmingRemove = true)} />
    </div>
  {/if}
</Panel>
{/if}
