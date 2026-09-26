<script lang="ts">
  import type { Game } from "../../services/local-state";
  import { toMessage } from "../../utils/errors";
  import { deleteGame, saveGame } from "../../stores/games";
  import Button from "../../components/ui/Button.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import Panel from "../../components/ui/Panel.svelte";
  import TextField from "../../components/ui/TextField.svelte";

  let { game, onRemoved }: { game: Game; onRemoved: () => void } = $props();

  let nameDraft = $state<string | null>(null);
  let pending = $state(false);
  let confirmingRemove = $state(false);
  let actionError = $state<string | null>(null);

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
</script>

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
